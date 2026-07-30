//! Executing actions and other related functions.

use std::{sync::Arc, time::Duration};

use glam::Vec3;
use mlua::Function;
use parking_lot::Mutex;

use crate::{
    ClientId, FromServer, GameData, PlayerData, StatusEffects, TickDamageSnapshot, TickEffectKind,
    ToServer,
    lua::{
        EffectsBuilder, EnmityAction, KawariLua, KawariLuaState, LuaContent, LuaPlayer, LuaZone,
        StatusGrant, TickKind,
    },
    server::{
        WorldServer,
        actor::{NetworkedActor, NpcState},
        combat_state::PlayerCombatState,
        effect::{gain_effect, send_effects_list},
        instance::{Instance, QueuedTaskData},
        jobs::{
            bard,
            dispatch::{JobActionUpdate, job_for},
            summoner,
        },
        network::{DestinationNetwork, NetworkState},
        set_character_mode, set_shared_group_timeline_state,
    },
    zone_connection::{BaseParameters, DamageRollModifiers},
};
use kawari::{
    common::{
        ANIMATION_LOCK_TIME, COMBO_TIMEOUT, CharacterMode, DEAD_FADE_OUT_TIME, ObjectId,
        ObjectTypeId, TimepointData,
    },
    config::FilesystemConfig,
    ipc::zone::{
        ActionEffect1, ActionEffect8, ActionEffect16, ActionEffect24, ActionEffect32,
        ActionEffectHeader, ActionRequest, ActionType, ActorControlCategory, DamageType,
        EffectEntry, EffectResult, STATUS_NOTIFICATION_GAINED_FROM_OTHER, ServerZoneIpcData,
        ServerZoneIpcSegment, TargetEffect, TargetEffectKind,
    },
};

/// Fraction of healing done that is converted into enmity, then split across every enemy
/// engaged with the heal target. Roughly matches retail, where healing generates about half
/// its value in enmity.
const HEAL_ENMITY_MODIFIER: f32 = 0.5;

const STATUS_FEINT: u16 = 1195;
const STATUS_ADDLE: u16 = 1203;
const STATUS_SWIFTCAST: u16 = 167;
const STATUS_RAGING_STRIKES: u16 = 125;
const STATUS_BATTLE_VOICE: u16 = 141;
const STATUS_RADIANT_FINALE: u16 = 2964;
const STATUS_MAGES_BALLAD: u16 = 2217;
const STATUS_ARMYS_PAEON: u16 = 2218;
const STATUS_WANDERERS_MINUET: u16 = 2216;

/// The cooldown group used by GCD weaponskills/spells (Action.CooldownGroup). Only this group's
/// recast is shortened by skill/spell speed; oGCD ability cooldowns are fixed.
const GCD_COOLDOWN_GROUP: u8 = 58;
const ADDITIONAL_ACTION_LOCK_100MS: u32 = 10;

/// Retail's action handler accepts a GCD/recast a hair before it technically expires, to absorb the
/// small offset between the client's locally predicted GCD and the server clock. Even with the GCD
/// started at cast time and centisecond-exact recast math, the client still *sends* the next action
/// a few milliseconds before its GCD wheel visually completes (input buffering / sub-frame timing).
/// A strict `elapsed >= duration` check rejects that request as a double-cast, which shows up as the
/// periodic "有伤害/没伤害" dropped-cast loop. A few tens of milliseconds of slack covers the
/// prediction offset without letting a genuine early double-cast (which is hundreds of ms early)
/// through. See [[gcd-cast-timing]].
const COOLDOWN_TOLERANCE: Duration = Duration::from_millis(50);

/// Extra grace applied only to the *rejection* check for an incoming action request. The client
/// predicts cooldowns/charges locally and fires the instant its own timer says ready; the request
/// reaches the server ~one uplink latency later (~120ms observed, up to ~200ms on jitter), so the
/// server's authoritative timer is slightly behind. Accept an action whose cooldown is within this
/// window of expiring instead of rejecting it — matching retail's early-use grace. This is only
/// for the accept/reject decision; the cooldown itself still starts from the real (unshifted) time.
const COOLDOWN_REJECTION_TOLERANCE: Duration = Duration::from_millis(500);

/// Mounting always uses a fixed 1-second summon cast ("Summoning..."), regardless of which mount or
/// the caster's stats. The client sends the *Mount* sheet row as the action_id, so reading a cast
/// time from the Action sheet would be meaningless, and mount casts aren't shortened by spell/skill
/// speed. Expressed in centiseconds (10ms units) to match the cast-timing pipeline.
const MOUNT_CAST_CENTISEC: u32 = 100;

/// Localhost responses can arrive before the client-side action hook finishes recording
/// `LastUsedActionSequence`. Retail always has at least network/server latency here; keep a small
/// delay so TargetEffect.SourceSequence can be matched without falling back to the 300ms task tick.
/// This is not intended to emulate RTT; it just needs to be long enough for the client to finish
/// request bookkeeping before the action-effect packet is processed.
const INSTANT_ACTION_RESPONSE_DELAY: Duration = Duration::from_millis(10);

fn is_spell_action(game_data: &mut GameData, action_id: u32) -> bool {
    game_data.get_action_category(action_id) == 2
}

fn actor_has_status(actor: &NetworkedActor, status_id: u16) -> bool {
    actor
        .status_effects()
        .is_some_and(|status_effects| status_effects.get(status_id).is_some())
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActionDamageModifiers {
    pub(crate) roll: DamageRollModifiers,
    raging_strikes: bool,
    mages_ballad: bool,
    radiant_finale_bonus_percent: u8,
}

impl ActionDamageModifiers {
    pub(crate) fn apply_base_damage(self, amount: u32) -> u32 {
        let mut amount = amount;
        if self.raging_strikes {
            amount = apply_damage_percent(amount, 115);
        }
        if self.mages_ballad {
            amount = apply_damage_percent(amount, 101);
        }
        if self.radiant_finale_bonus_percent > 0 {
            amount =
                apply_damage_percent(amount, 100 + u32::from(self.radiant_finale_bonus_percent));
        }
        amount
    }
}

fn apply_damage_percent(amount: u32, percent: u32) -> u32 {
    (u64::from(amount) * u64::from(percent) / 100).min(u64::from(u32::MAX)) as u32
}

pub(crate) fn action_damage_modifiers(actor: Option<&NetworkedActor>) -> ActionDamageModifiers {
    let Some(actor) = actor else {
        return ActionDamageModifiers::default();
    };

    let mut modifiers = ActionDamageModifiers {
        raging_strikes: actor_has_status(actor, STATUS_RAGING_STRIKES),
        mages_ballad: actor_has_status(actor, STATUS_MAGES_BALLAD),
        ..Default::default()
    };

    if actor_has_status(actor, STATUS_BATTLE_VOICE) {
        modifiers.roll.direct_hit_rate_bonus += 0.20;
    }
    if actor_has_status(actor, STATUS_ARMYS_PAEON) {
        modifiers.roll.direct_hit_rate_bonus += 0.03;
    }
    if actor_has_status(actor, STATUS_WANDERERS_MINUET) {
        modifiers.roll.crit_rate_bonus += 0.02;
    }
    // The bonus magnitude rides in the status param (set at grant time on both the caster's own
    // GainEffectSelf and every propagated party copy), so any holder scales — not just the Bard
    // whose combat_state carries the coda math.
    if let Some(status) = actor
        .status_effects()
        .and_then(|s| s.get(STATUS_RADIANT_FINALE))
    {
        modifiers.radiant_finale_bonus_percent = status.param as u8;
    }

    modifiers
}

/// Party members that should receive a caster's propagated party buff: every member whose actor is
/// present as a Player in the caster's instance, excluding the caster.
///
/// `instance_player_ids` is the set of ObjectIds in the caster's instance that are
/// `NetworkedActor::Player`; `party_member_ids` is `party.members[].actor_id` (order preserved).
/// Returns recipients in party order, deduplicated, caster excluded.
pub(crate) fn party_player_recipients(
    caster: ObjectId,
    party_member_ids: &[ObjectId],
    instance_player_ids: &std::collections::HashSet<ObjectId>,
) -> Vec<ObjectId> {
    let mut recipients = Vec::new();
    for &member in party_member_ids {
        if member == caster
            || !instance_player_ids.contains(&member)
            || recipients.contains(&member)
        {
            continue;
        }
        recipients.push(member);
    }
    recipients
}

pub(crate) fn apply_target_player_mitigation(
    amount: u32,
    target: Option<&NetworkedActor>,
    damage_type: DamageType,
) -> u32 {
    if let Some(NetworkedActor::Player { parameters, .. }) = target {
        let mitigation = parameters.mitigation_against(damage_type == DamageType::Magic);
        return ((amount as f64) * (1.0 - mitigation)).floor() as u32;
    }

    amount
}

/// The `LoseEffect` status ids in `effects` that `actor_id` currently holds.
///
/// Must be called before the job modules run, because they consume most procs themselves and the
/// live state is already clear by the time the `LoseEffect` entries are walked.
///
/// Filtering on what is actually held is also what keeps the client from announcing a removal
/// twice: an action may list several ids for one concept (Blast Arrow declares both 2692 and
/// 3142 for "Blast Arrow Ready") and only one of them is ever present.
fn collect_held_lost_statuses(
    instance: &Instance,
    actor_id: ObjectId,
    effects: &[TargetEffect],
) -> Vec<u16> {
    let Some(actor) = instance.find_actor(actor_id) else {
        return Vec::new();
    };
    let Some(status_effects) = actor.status_effects() else {
        return Vec::new();
    };

    effects
        .iter()
        .filter_map(|effect| match effect.0 {
            TargetEffectKind::LoseEffect { effect_id, .. } => Some(effect_id),
            _ => None,
        })
        .filter(|effect_id| status_effects.get(*effect_id).is_some())
        .collect()
}

fn remove_status_from_actor_instance(
    instance: &mut Instance,
    actor_id: ObjectId,
    status_id: u16,
) -> bool {
    let Some(actor) = instance.find_actor_mut(actor_id) else {
        return false;
    };
    let Some(status_effects) = actor.status_effects_mut() else {
        return false;
    };
    if status_effects.get(status_id).is_none() {
        return false;
    }

    status_effects.remove(status_id);
    instance.retain_tasks(|task| {
        !(task.from_actor_id == actor_id
            && matches!(
                task.data,
                QueuedTaskData::LoseStatusEffect { effect_id, .. } if effect_id == status_id
            ))
    });
    true
}

fn outgoing_damage_multiplier(has_feint: bool, has_addle: bool, damage_type: DamageType) -> f64 {
    let is_magic = damage_type == DamageType::Magic;
    let mut multiplier = 1.0;

    // Feint primarily weakens physical attacks, with a smaller magical reduction.
    if has_feint {
        multiplier *= if is_magic { 0.95 } else { 0.90 };
    }

    // Addle primarily weakens magical attacks, with a smaller physical reduction.
    if has_addle {
        multiplier *= if is_magic { 0.90 } else { 0.95 };
    }

    multiplier
}

fn send_job_gauge_update(
    network: &mut NetworkState,
    from_actor_id: ObjectId,
    classjob_id: u8,
    data: u64,
) {
    let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ActorGauge { classjob_id, data });
    network.send_to_by_actor_id(
        from_actor_id,
        FromServer::PacketSegment(ipc, from_actor_id),
        DestinationNetwork::ZoneClients,
    );
}

/// Maximum number of targets a single `ActionEffect32` packet can carry. Targets beyond this are
/// dropped (their damage is swallowed), matching how retail caps one effect packet.
const MAX_AOE_TARGETS: usize = 32;

/// Build the smallest `ActionEffectN` IPC data that holds `targets`, packing each target's effect into
/// its own 8-slot row. `targets` is `(target, effect)` pairs and must already be capped to
/// [`MAX_AOE_TARGETS`]. Returns `None` if there are no targets.
fn build_aoe_effect_packet(
    header: ActionEffectHeader,
    targets: &[(ObjectTypeId, TargetEffect)],
    center: kawari::common::Position,
) -> Option<ServerZoneIpcData> {
    if targets.is_empty() {
        return None;
    }

    /// Fill a fixed `[[TargetEffect; 8]; N]` / `[ObjectTypeId; N]` pair from `targets`.
    macro_rules! build_variant {
        ($variant:ident, $struct:ident, $n:expr) => {{
            let mut effects = [[TargetEffect::default(); 8]; $n];
            let mut target_ids = [ObjectTypeId::default(); $n];
            for (i, (target, effect)) in targets.iter().enumerate() {
                effects[i][0] = *effect;
                target_ids[i] = *target;
            }
            ServerZoneIpcData::$variant(Box::new($struct {
                header,
                effects,
                target_ids,
                position: center,
            }))
        }};
    }

    Some(match targets.len() {
        0..=8 => build_variant!(ActionEffect8, ActionEffect8, 8),
        9..=16 => build_variant!(ActionEffect16, ActionEffect16, 16),
        17..=24 => build_variant!(ActionEffect24, ActionEffect24, 24),
        _ => build_variant!(ActionEffect32, ActionEffect32, 32),
    })
}

/// Bit 7 of an effect's flags byte: the status lands on the action's source, not its target.
const EFFECT_FLAG_AT_SOURCE: u8 = 0x80;

/// Marks a caster-bound status gain as such for the client.
///
/// The two gain kinds say *who a single entry is for*, independent of whom the action was
/// aimed at: 14 (`ApplyStatusEffectTarget`) goes to the action's target, 15
/// (`ApplyStatusEffectSource`) bypasses it and goes to the caster. Standard Finish shows both
/// in one packet while aimed at the dancer -- the party buffs ride 14, the Last Dance Ready
/// proc rides 15 -- so the choice cannot come from the action's target. It comes from which
/// Lua call the script used: `gain_effect` means "to the target", `gain_effect_self` means
/// "to the caster".
///
/// Only the flag needs adding. `unk3` is the flags byte (BossMod calls it Param4) and bit 7 is
/// "at source"; without it the client credits the buff to the target and announces it there --
/// Apex Arrow on a striking dummy read as the dummy gaining Blast Arrow Ready.
fn wire_effect_kind(kind: TargetEffectKind) -> TargetEffectKind {
    match kind {
        TargetEffectKind::GainEffectSelf {
            unk1,
            unk2,
            unk3,
            effect_id,
            param,
            duration,
        } => TargetEffectKind::GainEffectSelf {
            unk1,
            unk2,
            unk3: unk3 | EFFECT_FLAG_AT_SOURCE,
            effect_id,
            param,
            duration,
        },
        other => other,
    }
}

/// Builds the `ActionEffect1` effect array shown to the client.
///
/// Status gains must reach the client through here: this array drives the buff-applied animation
/// and the "X gains Y" battle log line. Filtering them out left songs and self-buffs silent.
fn target_action_result_effects(effects: &[TargetEffect]) -> [TargetEffect; 8] {
    let mut target_effects = [TargetEffect::default(); 8];
    let mut count = 0usize;

    for effect in effects {
        if matches!(effect.0, TargetEffectKind::LoseEffect { .. }) {
            continue;
        }
        if count == target_effects.len() {
            break;
        }

        target_effects[count] = TargetEffect(wire_effect_kind(effect.0));
        count += 1;
    }

    target_effects
}

/// The `(recipient, status_id)` pairs the outgoing effect array already notifies (path 1). Only the
/// first `wire_effect_count` slots are considered, so entries dropped by the 8-slot cap are
/// correctly absent. A `GainEffect` credits the action's `target`; a `GainEffectSelf` credits the
/// `caster`. Pass `0` for the AoE branch, which writes no status entries.
fn wire_notified_status_pairs(
    effects: &[TargetEffect],
    wire_effect_count: u8,
    target: ObjectId,
    caster: ObjectId,
) -> Vec<(ObjectId, u16)> {
    let mut pairs = Vec::new();
    for effect in effects.iter().take(wire_effect_count as usize) {
        match effect.0 {
            TargetEffectKind::GainEffect { effect_id, .. } => pairs.push((target, effect_id)),
            TargetEffectKind::GainEffectSelf { effect_id, .. } => pairs.push((caster, effect_id)),
            _ => {}
        }
    }
    pairs
}

/// Grants that still need path 2 (a cat 23 notification): deduplicated, and with any grant whose
/// `(recipient, effect_id)` pair is already covered by the outgoing effect array (`notified`)
/// removed, so no recipient is notified twice for the same status.
fn grants_needing_actor_control(
    grants: &[StatusGrant],
    notified: &[(ObjectId, u16)],
) -> Vec<StatusGrant> {
    let mut kept: Vec<StatusGrant> = Vec::new();
    for grant in grants {
        let pair = (grant.recipient, grant.effect_id);
        if notified.contains(&pair) {
            continue;
        }
        if kept
            .iter()
            .any(|g| g.recipient == grant.recipient && g.effect_id == grant.effect_id)
        {
            continue;
        }
        kept.push(*grant);
    }
    kept
}

/// AoE damage falloff applied to *secondary* targets (the primary at slot 0 always takes full
/// damage). FFXIV does not encode falloff in any Excel sheet — two actions with byte-identical
/// Action-sheet geometry can have different falloff — so this is a hardcoded table sourced from the
/// per-action ActionTransient tooltip ("对目标之外的敌人威力降低50%"). Actions absent from the table
/// take no falloff (full damage to every target), which is correct for spread-type AoEs
/// (Shadowbite, Rain of Death, Ladonsbite, Quick Nock, Wide Volley, Apex Arrow, ...).
fn aoe_secondary_falloff_base(action_id: u32, base_damage: u32) -> u32 {
    let fraction = match action_id {
        // Bard 50%-falloff finishers/procs.
        7404 | 25784 | 36976 | 36977 => 0.5_f32,
        _ => 1.0_f32,
    };
    if fraction >= 1.0 {
        base_damage
    } else {
        (base_damage as f32 * fraction).round() as u32
    }
}

fn cooldown_groups_for_action(game_data: &mut GameData, action_id: u32) -> Vec<usize> {
    let mut groups = Vec::new();

    let cooldown_group = game_data.get_action_cooldown_group(action_id);
    if cooldown_group > 0 {
        groups.push((cooldown_group - 1) as usize);
    }

    let additional_cooldown_group = game_data.get_action_additional_cooldown_group(action_id);
    if additional_cooldown_group > 0 && additional_cooldown_group != cooldown_group {
        groups.push((additional_cooldown_group - 1) as usize);
    }

    groups
}

fn action_cooldown_rejections(
    game_data: &mut GameData,
    combat_state: &mut PlayerCombatState,
    action_id: u32,
    level: u8,
) -> Vec<(usize, Duration)> {
    let primary_group = game_data.get_action_cooldown_group(action_id);
    let max_charges = game_data.get_action_max_charges_at_level(action_id, level);

    cooldown_groups_for_action(game_data, action_id)
        .into_iter()
        // For multi-charge actions the additional non-GCD group carries a 1s client-side
        // visual lock only (retail behaviour confirmed by capture). The server must not
        // reject on it: two consecutive charges both land inside that 1s window. Only the
        // primary charge group gates real availability; the GCD group (58) always gates GCDs.
        .filter(|&group| {
            if max_charges > 1
                && primary_group > 0
                && group != (usize::from(primary_group) - 1)
                && group != (usize::from(GCD_COOLDOWN_GROUP) - 1)
            {
                return false;
            }
            true
        })
        .filter_map(|group| {
            if combat_state.cooldown_ready(group, COOLDOWN_REJECTION_TOLERANCE) {
                None
            } else {
                Some((group, combat_state.cooldown_remaining(group)))
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct StartedCooldown {
    cooldown_group: u32,
    action_id: u32,
    duration_centisec: u32,
}

fn start_action_cooldowns(
    actor: &mut NetworkedActor,
    game_data: &mut GameData,
    action_id: u32,
) -> Vec<StartedCooldown> {
    let level = actor.get_common_spawn().level;
    let NetworkedActor::Player {
        combat_state,
        parameters,
        remove_cooldowns,
        ..
    } = actor
    else {
        return Vec::new();
    };

    // GM cheat: never put anything on cooldown so actions can be spammed.
    if *remove_cooldowns {
        return Vec::new();
    }

    let recast_100ms = u32::from(game_data.get_action_recast(action_id));
    if recast_100ms == 0 {
        return Vec::new();
    }

    let primary_group = game_data.get_action_cooldown_group(action_id);
    let additional_group = game_data.get_action_additional_cooldown_group(action_id);
    let max_charges = game_data.get_action_max_charges_at_level(action_id, level);

    // Standard GCD recast (2.5s base) used for the GCD cooldown group whenever it's the
    // *additional* cooldown of an action — most importantly demi summons, which have
    // `CooldownGroup=10 (60s)` plus `AdditionalCooldownGroup=58 (GCD)`. Non-GCD additional groups
    // (e.g. group 71 on multi-charge abilities) are only a short anti-repeat lock; the real charge
    // recovery lives on the primary group.
    const STANDARD_GCD_100MS: u32 = 25;

    // Avoid double-stamping the same group when an action lists it as both primary and
    // additional (shouldn't happen, but the data is external).
    let mut applied: Vec<u8> = Vec::with_capacity(2);
    let mut started = Vec::with_capacity(2);
    for &group_id in &[primary_group, additional_group] {
        if group_id == 0 || applied.contains(&group_id) {
            continue;
        }

        applied.push(group_id);

        // Primary group → action's own Recast100ms. Additional GCD group → the standard 2.5s GCD
        // lock. Other additional groups are short shared locks to avoid immediate double-taps.
        let base_100ms = if group_id == primary_group {
            recast_100ms
        } else if group_id == GCD_COOLDOWN_GROUP {
            STANDARD_GCD_100MS
        } else {
            ADDITIONAL_ACTION_LOCK_100MS
        };

        // Skill/spell speed shortens magic/weaponskill recasts. Abilities stay fixed.
        let base_centisec = base_100ms * 10;
        let recast_centisec = if group_id == GCD_COOLDOWN_GROUP
            || (group_id == primary_group
                && matches!(game_data.get_action_category(action_id), 2 | 3))
        {
            parameters.apply_speed(base_centisec)
        } else {
            base_centisec
        };

        combat_state.start_cooldown(
            (group_id - 1) as usize,
            action_id,
            Duration::from_millis(u64::from(recast_centisec) * 10),
            if group_id == primary_group {
                max_charges
            } else {
                1
            },
            COOLDOWN_TOLERANCE,
        );
        // For multi-charge actions the wire packet must encode the *entire pool* duration
        // (MaxCharges × R). The client derives perCharge = total / MaxCharges and displays
        // charges as floor(elapsed / perCharge). With total = R the client computes
        // perCharge = R/MaxCharges → wrong; the UI never decrements correctly.
        // CooldownState.start_cooldown above still receives the single-charge duration so
        // internal charge accounting is unaffected.
        let wire_duration_centisec = if group_id == primary_group && max_charges > 1 {
            recast_centisec.saturating_mul(u32::from(max_charges))
        } else {
            recast_centisec
        };
        started.push(StartedCooldown {
            // ActorControl uses zero-based cooldown group ids on the wire.
            cooldown_group: u32::from(group_id - 1),
            action_id,
            duration_centisec: wire_duration_centisec,
        });
    }

    started
}

/// LogMessage row 582 = "该动作暂时无法发动。" — the standard "action not ready" reject text.
const LOG_MESSAGE_ACTION_NOT_READY: u32 = 582;

fn reset_client_action_cooldowns(
    network: &mut NetworkState,
    actor_id: ObjectId,
    action_id: u32,
    recast_elapsed_centisec: u32,
    recast_total_centisec: u32,
    source_seq: u32,
) {
    // The client optimistically locks the recast group (and the shared GCD) at cast-send time,
    // *before* the request reaches us. On a genuine rejection we must roll that lock back or the
    // player is stuck waiting the full predicted recast for an action that never happened. Retail
    // does this with ActorControlSelf category 700, which the client interprets in one of two ways
    // depending on the elapsed/total fields (confirmed by IDA + retail capture):
    //   * elapsed==0 && total==0 -> ResetCooldownForGroup: charge-aware reset (full clear for a
    //     non-charge action, refund exactly one charge for a multi-charge pool, plus clears the
    //     shared GCD group). This is the "the action never happened, give it all back" path — used
    //     for job-state / resource rejections where the group is genuinely idle.
    //   * either nonzero -> SetCooldown: writes Elapsed=elapsed/100, Total=total/100 (seconds)
    //     verbatim to the action's primary recast group, calibrating the client to the server's
    //     authoritative cooldown. This is the path for cooldown rejections (double-tap / charge not
    //     ready): the caller passes the group's real pool-scaled timer values so the client's charge
    //     math (floor(MaxCharges * Elapsed / Total)) lands on the correct remaining charges.
    // The caller resolves which path applies simply by reading the primary group's timer values:
    // an on-cooldown group yields nonzero (Path B); a ready/untracked group yields (0,0) (Path A).
    // source_seq echoes the client's ActionRequest.sequence so the client cancels the correct
    // in-flight optimistic action.
    network.send_to_by_actor_id(
        actor_id,
        FromServer::ActorControlSelf(ActorControlCategory::ActionRejected {
            log_message_id: LOG_MESSAGE_ACTION_NOT_READY,
            action_type: 1,
            action_id,
            recast_elapsed_centisec,
            recast_total_centisec,
            source_seq,
        }),
        DestinationNetwork::ZoneClients,
    );
}

pub(super) fn clear_action_cooldowns(
    actor: &mut NetworkedActor,
    game_data: &mut GameData,
    action_id: u32,
) -> Vec<u32> {
    let NetworkedActor::Player { combat_state, .. } = actor else {
        return Vec::new();
    };

    let mut groups = Vec::new();
    for group in cooldown_groups_for_action(game_data, action_id) {
        combat_state.clear_cooldown(group);
        groups.push(group as u32);
    }

    groups
}

fn send_dirty_status_effects(
    network: Arc<Mutex<NetworkState>>,
    instance: &mut Instance,
    actor_id: ObjectId,
) {
    let is_dirty = instance
        .find_actor(actor_id)
        .and_then(NetworkedActor::status_effects)
        .map(StatusEffects::is_dirty)
        .unwrap_or(false);

    if !is_dirty {
        return;
    }

    send_effects_list(network, instance, actor_id);

    if let Some(actor) = instance.find_actor_mut(actor_id)
        && let Some(status_effects) = actor.status_effects_mut()
    {
        status_effects.reset_dirty();
    }
}

fn resolve_player_action_id(
    actor: &NetworkedActor,
    actor_id: ObjectId,
    request: &ActionRequest,
    game_data: &mut GameData,
    check_cooldown: bool,
) -> Option<u32> {
    let NetworkedActor::Player {
        combat_state,
        remove_cooldowns,
        ..
    } = actor
    else {
        return Some(request.action_id);
    };

    let class_job = actor.get_common_spawn().class_job;
    let level = actor.get_common_spawn().level;
    let resolved_action_id = if request.action_type == ActionType::Action
        && let Some(job) = job_for(class_job)
    {
        let resolved = job.resolve_action(request, combat_state, level, game_data);
        if !job.can_execute(resolved, combat_state, level) {
            tracing::warn!(
                ?actor_id,
                action_id = request.action_id,
                resolved_action_id = resolved,
                level,
                summoner_state = ?combat_state.summoner,
                bard_state = ?combat_state.bard,
                "Rejected job action because the current job state does not allow it",
            );
            return None;
        }
        resolved
    } else {
        request.action_id
    };

    // Only the immediate message handler checks the cooldown (to reject genuine double-casts). The
    // tick-driven execute path passes false, so the 500ms server-tick granularity can't spuriously
    // reject an action whose GCD was already validated and started at cast time.
    if check_cooldown && request.action_type == ActionType::Action && !*remove_cooldowns {
        let mut combat_state = combat_state.clone();
        let rejected_groups =
            action_cooldown_rejections(game_data, &mut combat_state, resolved_action_id, level);
        if !rejected_groups.is_empty() {
            tracing::warn!(
                ?actor_id,
                action_id = request.action_id,
                resolved_action_id,
                rejected_groups = ?rejected_groups,
                "Rejected action because one or more cooldown groups are not ready",
            );
            return None;
        }
    }

    if request.action_type == ActionType::Action {
        let mp_cost = game_data.get_action_mp_cost(resolved_action_id);
        let current_mp = actor.get_common_spawn().resource_points;
        if mp_cost > u32::from(current_mp) {
            tracing::warn!(
                ?actor_id,
                action_id = request.action_id,
                resolved_action_id,
                current_mp,
                mp_cost,
                "Rejected action because the actor does not have enough MP",
            );
            return None;
        }
    }

    Some(resolved_action_id)
}

/// Process action-related messages.
pub fn handle_action_messages(
    data: Arc<Mutex<WorldServer>>,
    game_data: Arc<Mutex<GameData>>,
    network: Arc<Mutex<NetworkState>>,
    lua: Arc<Mutex<KawariLua>>,
    msg: &ToServer,
) -> bool {
    if let ToServer::ActionRequest(from_id, from_actor_id, request) = msg {
        let mut resolved_request = request.clone();

        if request.action_type == ActionType::Action {
            let resolved_action_id = {
                let data = data.lock();
                let Some(instance) = data.find_actor_instance(*from_actor_id) else {
                    return true;
                };
                let Some(actor) = instance.find_actor(*from_actor_id) else {
                    return true;
                };

                let mut game_data = game_data.lock();
                resolve_player_action_id(actor, *from_actor_id, request, &mut game_data, true)
            };

            let Some(resolved_action_id) = resolved_action_id else {
                // Genuine rejection: roll back the client's optimistic recast lock. Read the
                // primary cooldown group's authoritative timer so the client calibrates (Path B)
                // when the group is really on cooldown, and fully resets (Path A) when it's idle.
                let (recast_elapsed_centisec, recast_total_centisec) = {
                    let mut world = data.lock();
                    let mut game_data = game_data.lock();
                    world
                        .find_actor_instance_mut(*from_actor_id)
                        .and_then(|instance| instance.find_actor_mut(*from_actor_id))
                        .and_then(|actor| {
                            if let NetworkedActor::Player { combat_state, .. } = actor {
                                let group = game_data.get_action_cooldown_group(request.action_id);
                                if group > 0 {
                                    Some(combat_state.cooldown_timer_values(usize::from(group - 1)))
                                } else {
                                    Some((0, 0))
                                }
                            } else {
                                None
                            }
                        })
                        .unwrap_or((0, 0))
                };
                let mut network = network.lock();
                reset_client_action_cooldowns(
                    &mut network,
                    *from_actor_id,
                    request.action_id,
                    recast_elapsed_centisec,
                    recast_total_centisec,
                    u32::from(request.sequence),
                );
                return true;
            };
            resolved_request.action_id = resolved_action_id;
        }

        // Mounts always use a fixed 1s summon cast: the client sends the Mount *sheet* row as the
        // action_id (so get_casttime, which reads the Action sheet, would return a bogus duration),
        // and mount casts aren't affected by spell/skill speed. Everything else reads its cast time
        // from the Action sheet and is shortened by the caster's speed with the client's exact
        // (centisecond) rounding, so the cast finishes at the same instant on both sides.
        let cast_centisec = if resolved_request.action_type == ActionType::Mount {
            MOUNT_CAST_CENTISEC
        } else {
            let (cast_time_100ms, is_spell) = {
                let mut game_data = game_data.lock();
                (
                    game_data
                        .get_casttime(resolved_request.action_id)
                        .unwrap_or_default(),
                    resolved_request.action_type == ActionType::Action
                        && is_spell_action(&mut game_data, resolved_request.action_id),
                )
            };
            let base_centisec = u32::from(cast_time_100ms) * 10;
            let data = data.lock();
            data.find_actor_instance(*from_actor_id)
                .and_then(|instance| instance.find_actor(*from_actor_id))
                .and_then(|actor| match actor {
                    NetworkedActor::Player { parameters, .. } => {
                        if base_centisec > 0
                            && is_spell
                            && actor_has_status(actor, STATUS_SWIFTCAST)
                        {
                            Some(0)
                        } else {
                            Some(parameters.apply_speed(base_centisec))
                        }
                    }
                    _ => None,
                })
                .unwrap_or(base_centisec)
        };
        let delay_milliseconds = u64::from(cast_centisec) * 10;

        let mut world = data.lock();
        let Some(instance) = world.find_actor_instance_mut(*from_actor_id) else {
            return true;
        };

        if cast_centisec > 0 {
            let Some(actor) = instance.find_actor(*from_actor_id) else {
                return true;
            };

            let actor_cast = ServerZoneIpcSegment::new(ServerZoneIpcData::ActorCast {
                spell_id: resolved_request.action_id as u16,
                action_id: resolved_request.action_id,
                action_type: resolved_request.action_type,
                omen_delay: 0,
                cast_time: delay_milliseconds as f32 / 1000.0,
                target: resolved_request.target.object_id,
                rotation: resolved_request.rotation1,
                interruptible: false,
                ballista_entity_id: ObjectId::default(),
                position: actor.position(),
            });
            let mut network = network.lock();
            network.send_in_range_inclusive_instance(
                *from_actor_id,
                instance,
                FromServer::PacketSegment(actor_cast, *from_actor_id),
                DestinationNetwork::ZoneClients,
            );
        }

        // Start the server-side GCD now, at cast start. This handler runs immediately on the
        // request (not on the 500ms server tick like execute_action), so the anti-double-cast
        // check lines up with the client's locally predicted GCD instead of lagging a whole tick.
        let started_cooldowns = if let Some(actor) = instance.find_actor_mut(*from_actor_id) {
            let mut game_data = game_data.lock();
            start_action_cooldowns(actor, &mut game_data, resolved_request.action_id)
        } else {
            Vec::new()
        };
        if !started_cooldowns.is_empty() {
            let mut network = network.lock();
            for cooldown in started_cooldowns {
                network.send_to_by_actor_id(
                    *from_actor_id,
                    FromServer::ActorControlSelf(ActorControlCategory::SetCooldownTimerMax {
                        cooldown_group: cooldown.cooldown_group,
                        action_id: cooldown.action_id,
                        duration_centisec: cooldown.duration_centisec,
                    }),
                    DestinationNetwork::ZoneClients,
                );
            }
        }

        // A cast bar (delay > 0) is interruptible by movement, *except* mounting — in current
        // retail you can move freely while summoning a mount without cancelling it.
        let interruptible =
            delay_milliseconds > 0 && resolved_request.action_type != ActionType::Mount;

        if delay_milliseconds == 0 {
            let from_id = *from_id;
            let from_actor_id = *from_actor_id;
            let request = resolved_request;
            drop(world);
            tokio::task::spawn(async move {
                tokio::time::sleep(INSTANT_ACTION_RESPONSE_DELAY).await;
                execute_action(
                    network,
                    data,
                    game_data,
                    lua,
                    from_id,
                    from_actor_id,
                    request,
                );
            });
            return true;
        }

        instance.insert_task(
            *from_id,
            *from_actor_id,
            Duration::from_millis(delay_milliseconds),
            QueuedTaskData::CastAction {
                request: resolved_request,
                interruptible,
            },
        );

        return true;
    }

    false
}

/// Executes an action, and returns a list of Tasks that must be executed by the client.
pub fn execute_action(
    network: Arc<Mutex<NetworkState>>,
    data: Arc<Mutex<WorldServer>>,
    game_data: Arc<Mutex<GameData>>,
    lua: Arc<Mutex<KawariLua>>,
    from_id: ClientId,
    from_actor_id: ObjectId,
    request: ActionRequest,
) {
    if request.action_type == ActionType::Mount {
        {
            let mut data = data.lock();
            let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                return;
            };

            let Some(actor) = instance.find_actor_mut(from_actor_id) else {
                return;
            };

            let current_mount;
            {
                let common = actor.get_common_spawn_mut();
                common.current_mount = request.action_id as u16;
                common.mode = CharacterMode::Mounted;
                current_mount = common.current_mount;
            }

            let mut network = network.lock();
            network.send_to_by_actor_id(
                from_actor_id,
                FromServer::SetCurrentMount(current_mount),
                DestinationNetwork::ZoneClients,
            );
        }

        {
            let data = data.lock();
            let Some(instance) = data.find_actor_instance(from_actor_id) else {
                return;
            };
            let Some(actor) = instance.find_actor(from_actor_id) else {
                return;
            };

            let _ = execute_mount_action(network.clone(), from_actor_id, &request, actor, instance);
        }

        let mut data = data.lock();
        let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
            return;
        };
        let mut network = network.lock();
        summoner::sync_pet_for_mount(&mut network, instance, from_actor_id);
        return;
    }

    let resolved_request = request.clone();
    let mut lua_player = LuaPlayer {
        player_data: PlayerData::default(),
        status_effects: StatusEffects::default(),
        queued_tasks: Vec::new(),
        zone_data: LuaZone::default(),
        content_data: LuaContent::default(),
        base_parameters: BaseParameters::default(),
        combat_state: PlayerCombatState::default(),
        level: 0,
    };

    let (mut common_spawn, _combo_action_id, in_combo, remove_cooldowns, class_job) = {
        let data = data.lock();
        let Some(instance) = data.find_actor_instance(from_actor_id) else {
            return;
        };
        let Some(actor) = instance.find_actor(from_actor_id) else {
            return;
        };

        let NetworkedActor::Player {
            teleport_query,
            parameters,
            status_effects,
            combat_state,
            remove_cooldowns,
            last_combo_action,
            ..
        } = actor
        else {
            return;
        };

        lua_player.player_data.teleport_query = teleport_query.clone();
        lua_player.base_parameters = parameters.clone();
        lua_player.status_effects = status_effects.clone();
        lua_player.combat_state = combat_state.clone();
        lua_player.level = actor.get_common_spawn().level as u16;

        let combo_action_id = {
            let mut game_data = game_data.lock();
            game_data.get_combo_action(resolved_request.action_id)
        };

        (
            actor.get_common_spawn().clone(),
            combo_action_id,
            combo_action_id == *last_combo_action,
            *remove_cooldowns,
            actor.get_common_spawn().class_job,
        )
    };

    let effects_builder = {
        let data = data.lock();
        let Some(instance) = data.find_actor_instance(from_actor_id) else {
            return;
        };
        let Some(actor) = instance.find_actor(from_actor_id) else {
            return;
        };

        match resolved_request.action_type {
            ActionType::None => None,
            ActionType::Action => {
                execute_normal_action(lua.clone(), &resolved_request, &mut lua_player, in_combo)
            }
            ActionType::Item => execute_item_action(
                game_data.clone(),
                lua.clone(),
                &resolved_request,
                &mut lua_player,
            ),
            ActionType::Mount => execute_mount_action(
                network.clone(),
                from_actor_id,
                &resolved_request,
                actor,
                instance,
            ),
            _ => unimplemented!(),
        }
    };

    if let Some(mut effects_builder) = effects_builder {
        // Retail's Teleport (action 5) ActionEffect1 carries a single magic-61 effect holding the
        // destination TerritoryType (one populated effect slot). Kawari's Teleport Lua returns an
        // empty builder, so without this the result is malformed (no effects) and the teleport-out
        // animation never resolves — the caster stays stuck in the teleport pose. Resolve the
        // destination territory from the queued aetheryte and attach the effect to match retail.
        const TELEPORT_ACTION_ID: u32 = 5;
        if resolved_request.action_id == TELEPORT_ACTION_ID {
            let territory = {
                let mut game_data = game_data.lock();
                game_data
                    .get_aetheryte(
                        lua_player.player_data.teleport_query.aetheryte_id as u32,
                        false,
                    )
                    .map(|(_, zone)| zone)
                    .unwrap_or_default()
            };
            effects_builder
                .effects
                .push(TargetEffect(TargetEffectKind::Teleport {
                    unk: [0; 5],
                    territory,
                }));
        }

        let cleared_cooldown_groups;
        // Unified per-job gauge send: `Some((gauge_class_job_id, gauge_data))` or `None`.
        let job_gauge_data;
        let bard_action_update;
        // Sampled inside the data block below, read by the LoseEffect loop further down.
        let lost_statuses: Vec<u16>;
        let action_mp_cost = if resolved_request.action_type == ActionType::Action {
            let mut game_data = game_data.lock();
            game_data.get_action_mp_cost(resolved_request.action_id)
        } else {
            0
        };
        // Captured inside the data block below, used by the AoE fan-out further down.
        let aoe_base_damage: u32;
        let aoe_damage_type: DamageType;
        let aoe_radius: f32;
        let consume_swiftcast: bool;
        let source_damage_modifiers: ActionDamageModifiers;
        // Whether this action summons a generic carbuncle; the spawn is deferred until after the
        // result packet so the client plays the summon animation before the pet appears.
        let mut summon_pet_after = false;
        let has_summoner_pet_transition =
            summoner::has_pet_transition_for_action(resolved_request.action_id);

        {
            let mut data = data.lock();
            let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                return;
            };

            if action_mp_cost > 0 {
                let Some(actor) = instance.find_actor_mut(from_actor_id) else {
                    return;
                };
                let common = actor.get_common_spawn_mut();
                if action_mp_cost > u32::from(common.resource_points) {
                    tracing::warn!(
                        ?from_actor_id,
                        action_id = resolved_request.action_id,
                        current_mp = common.resource_points,
                        action_mp_cost,
                        "Skipped action execution because the actor no longer has enough MP",
                    );
                    return;
                }
                common.resource_points -= action_mp_cost as u16;
                common_spawn.resource_points = common.resource_points;
            }

            if let Some(actor) = instance.find_actor_mut(resolved_request.target.object_id)
                && let NetworkedActor::Npc {
                    currently_invulnerable,
                    ..
                } = actor
                && *currently_invulnerable
            {
                effects_builder.effects = effects_builder
                    .effects
                    .iter()
                    .map(|effect| match effect.0 {
                        TargetEffectKind::Damage { .. } => {
                            TargetEffect(TargetEffectKind::Invincible {})
                        }
                        _ => *effect,
                    })
                    .collect();
            }

            let combo_sequence = if let Some(actor) = instance.find_actor_mut(from_actor_id) {
                if let NetworkedActor::Player {
                    last_combo_action,
                    combo_sequence,
                    ..
                } = actor
                {
                    let sequence = *combo_sequence;
                    if in_combo {
                        *combo_sequence = combo_sequence.saturating_add(1);
                    } else {
                        *combo_sequence = 0;
                    }
                    *last_combo_action = resolved_request.action_id as u16;
                    Some(sequence)
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(sequence) = combo_sequence {
                instance.retain_tasks(|task| {
                    !(task.from_actor_id == from_actor_id
                        && matches!(task.data, QueuedTaskData::ResetCombo))
                });

                instance.insert_task(
                    from_id,
                    from_actor_id,
                    COMBO_TIMEOUT,
                    QueuedTaskData::ResetCombo,
                );

                effects_builder
                    .effects
                    .push(TargetEffect(TargetEffectKind::ExecuteCombo {
                        sequence,
                        unk2: 0,
                        unk3: 0,
                        unk4: 0,
                        unk5: 128,
                        action_id: resolved_request.action_id as u16,
                    }));
            }

            if summoner::is_summoner(class_job) {
                summoner::augment_action_result_effects(
                    resolved_request.action_id,
                    &mut effects_builder.effects,
                );
            }

            // Capture the base (pre-roll) damage and AoE radius before the loop below rolls the
            // primary target's damage in place, so we can fan the hit out to nearby enemies after.
            (aoe_base_damage, aoe_damage_type) = effects_builder
                .effects
                .iter()
                .find_map(|effect| match effect.0 {
                    TargetEffectKind::Damage {
                        amount,
                        damage_type,
                        ..
                    } => Some((amount, damage_type)),
                    _ => None,
                })
                .unwrap_or((0, DamageType::Physical));
            aoe_radius = {
                let mut game_data = game_data.lock();
                let cast_type = game_data.get_action_cast_type(resolved_request.action_id);
                let effect_range = game_data.get_action_effect_range(resolved_request.action_id);
                // cast_type 1 = single target. Everything else with a radius is some AoE shape; we
                // approximate them all as a circle around the primary target for now.
                if cast_type != 1 && effect_range > 0 {
                    f32::from(effect_range)
                } else {
                    0.0
                }
            };
            consume_swiftcast = {
                let mut game_data = game_data.lock();
                resolved_request.action_type == ActionType::Action
                    && is_spell_action(&mut game_data, resolved_request.action_id)
                    && game_data
                        .get_casttime(resolved_request.action_id)
                        .unwrap_or_default()
                        > 0
                    && instance
                        .find_actor(from_actor_id)
                        .is_some_and(|actor| actor_has_status(actor, STATUS_SWIFTCAST))
            };
            source_damage_modifiers = action_damage_modifiers(instance.find_actor(from_actor_id));

            for effect in &mut effects_builder.effects {
                match &mut effect.0 {
                    TargetEffectKind::Damage {
                        amount,
                        damage_kind,
                        damage_type,
                        damage_element,
                        ..
                    } => {
                        // Roll crit/direct-hit/variance from the attacker's stats, and tell the
                        // client the resulting hit severity so it shows the right number style.
                        let base_amount = source_damage_modifiers.apply_base_damage(*amount);
                        let (rolled, kind) = lua_player
                            .base_parameters
                            .roll_damage_with_modifiers(base_amount, source_damage_modifiers.roll);
                        *amount = apply_target_player_mitigation(
                            rolled,
                            instance.find_actor(resolved_request.target.object_id),
                            *damage_type,
                        );
                        *damage_kind = kind;

                        if let Some(actor) =
                            instance.find_actor_mut(resolved_request.target.object_id)
                            && let Some(hate_list) = actor.npc_hate_list_mut()
                        {
                            let entry = hate_list.entry(from_actor_id).or_insert(0);
                            *entry = entry.saturating_add(*amount as u32);
                        }

                        let Some(actor) =
                            instance.find_actor_mut(resolved_request.target.object_id)
                        else {
                            return;
                        };
                        actor.apply_damage(*amount);

                        let mut game_data = game_data.lock();
                        *damage_element =
                            game_data.get_action_damage_element(resolved_request.action_id);
                    }
                    TargetEffectKind::Heal { amount, .. } => {
                        let heal_target = resolved_request.target.object_id;

                        // Actually restore the target's HP, clamped to their maximum.
                        if let Some(actor) = instance.find_actor_mut(heal_target) {
                            let common_spawn = actor.get_common_spawn_mut();
                            common_spawn.health_points = common_spawn
                                .health_points
                                .saturating_add(*amount as u32)
                                .min(common_spawn.max_health_points);
                        }

                        // Healing generates enmity for the *healer*, split across every enemy
                        // currently engaged with the heal target. No engaged enemies means no
                        // enmity, so out-of-combat healing never pulls anything.
                        let engaged: Vec<ObjectId> = instance
                            .actors
                            .iter()
                            .filter_map(|(id, actor)| match actor {
                                NetworkedActor::Npc {
                                    hate_list,
                                    state,
                                    spawn,
                                    ..
                                } if *state != NpcState::Dead
                                    && spawn.common.health_points > 0
                                    && hate_list.contains_key(&heal_target) =>
                                {
                                    Some(*id)
                                }
                                _ => None,
                            })
                            .collect();

                        if !engaged.is_empty() {
                            let total = (*amount as f32 * HEAL_ENMITY_MODIFIER).round() as u32;
                            let each = (total / engaged.len() as u32).max(1);
                            for npc_id in engaged {
                                if let Some(actor) = instance.find_actor_mut(npc_id)
                                    && let Some(hate_list) = actor.npc_hate_list_mut()
                                {
                                    let entry = hate_list.entry(from_actor_id).or_insert(0);
                                    *entry = entry.saturating_add(each);
                                }
                            }
                        }
                    }
                    TargetEffectKind::InterruptAction {} => {
                        instance.cancel_actor_tasks(resolved_request.target.object_id);
                    }
                    TargetEffectKind::SummonPet { .. } => {
                        // Defer the actual pet spawn until *after* the result packet is sent, so the
                        // client receives the SummonPet effect (which plays the summon gesture/VFX)
                        // before the pet actor appears. Spawning here would pop the pet in with no
                        // animation. Egi-II summons use the same wire effect but are handled by
                        // the elemental primal transition path below, not the generic carbuncle
                        // spawn path.
                        summon_pet_after =
                            !summoner::is_elemental_primal_summon(resolved_request.action_id);
                    }
                    _ => {}
                }
            }

            // Resolve server-side enmity instructions (provoke / flat enmity / transfers) now
            // that the action's target is known.
            for enmity_action in &effects_builder.enmity_actions {
                match enmity_action {
                    EnmityAction::Add { amount } => {
                        if let Some(actor) =
                            instance.find_actor_mut(resolved_request.target.object_id)
                            && let Some(hate_list) = actor.npc_hate_list_mut()
                        {
                            let entry = hate_list.entry(from_actor_id).or_insert(0);
                            *entry = entry.saturating_add(*amount);
                        }
                    }
                    EnmityAction::Provoke => {
                        if let Some(actor) =
                            instance.find_actor_mut(resolved_request.target.object_id)
                            && let Some(hate_list) = actor.npc_hate_list_mut()
                        {
                            let highest = hate_list.values().copied().max().unwrap_or(0);
                            hate_list.insert(from_actor_id, highest.saturating_add(1));
                        }
                    }
                    EnmityAction::Transfer { percent } => {
                        // Shirk: copy a fraction of the caster's enmity onto the target on
                        // every enemy engaged with the caster. The caster keeps their enmity.
                        let transfer_target = resolved_request.target.object_id;
                        let percent = (*percent).min(100);
                        let sources: Vec<(ObjectId, u32)> = instance
                            .actors
                            .iter()
                            .filter_map(|(id, actor)| match actor {
                                NetworkedActor::Npc { hate_list, .. } => {
                                    hate_list.get(&from_actor_id).map(|hate| (*id, *hate))
                                }
                                _ => None,
                            })
                            .collect();
                        for (npc_id, source_hate) in sources {
                            let transferred = ((source_hate as u64 * percent as u64) / 100) as u32;
                            if transferred == 0 {
                                continue;
                            }
                            if let Some(actor) = instance.find_actor_mut(npc_id)
                                && let Some(hate_list) = actor.npc_hate_list_mut()
                            {
                                let entry = hate_list.entry(transfer_target).or_insert(0);
                                *entry = entry.saturating_add(transferred);
                            }
                        }
                    }
                }
            }

            // Apply the gauge changes the action requested (e.g. Necrotize spending Aetherflow),
            // before the gauge is rebuilt below so the change is reflected immediately.
            if !effects_builder.gauge_actions.is_empty()
                && let Some(NetworkedActor::Player { combat_state, .. }) =
                    instance.find_actor_mut(from_actor_id)
            {
                if let Some(job) = job_for(class_job) {
                    for gauge_action in &effects_builder.gauge_actions {
                        job.apply_gauge_action(combat_state, gauge_action);
                    }
                }
            }

            // Register any DoT/HoT ticks the action applied. The status itself was already added to
            // the wire effects (as a normal gain_effect); here we attach the per-tick potency so the
            // 3-second regen tick (see server_logic_tick) can resolve damage/healing each tick.
            for tick_action in &effects_builder.tick_actions {
                let tick_target = if tick_action.on_self {
                    from_actor_id
                } else {
                    resolved_request.target.object_id
                };
                let kind = match tick_action.kind {
                    TickKind::DamageMagic => TickEffectKind::DamageMagic,
                    TickKind::DamagePhysical => TickEffectKind::DamagePhysical,
                    TickKind::Heal => TickEffectKind::Heal,
                    TickKind::RestoreMp => TickEffectKind::RestoreMp,
                };
                let damage_snapshot = match tick_action.kind {
                    TickKind::DamageMagic => {
                        let base_amount = lua_player
                            .base_parameters
                            .calc_magical_damage(tick_action.potency as u32);
                        Some(TickDamageSnapshot {
                            base_amount: source_damage_modifiers.apply_base_damage(base_amount),
                            roll_modifiers: source_damage_modifiers.roll,
                        })
                    }
                    TickKind::DamagePhysical => {
                        let base_amount = lua_player
                            .base_parameters
                            .calc_physical_damage(tick_action.potency as u32);
                        Some(TickDamageSnapshot {
                            base_amount: source_damage_modifiers.apply_base_damage(base_amount),
                            roll_modifiers: source_damage_modifiers.roll,
                        })
                    }
                    TickKind::Heal | TickKind::RestoreMp => None,
                };
                if let Some(actor) = instance.find_actor_mut(tick_target)
                    && let Some(status_effects) = actor.status_effects_mut()
                {
                    status_effects.add_tick(
                        tick_action.effect_id,
                        tick_action.param,
                        tick_action.duration,
                        kind,
                        tick_action.potency,
                        damage_snapshot,
                        from_actor_id,
                    );
                }

                // A damaging DoT must generate enmity the moment it's applied, exactly like a direct
                // hit — otherwise opening on an unaware enemy with a DoT (e.g. SCH Biolysis) would
                // never put the caster in its hate list, and the enemy would never aggro. Use one
                // tick's worth of damage (resolved from the caster's stats) as the initial enmity.
                // HoTs (on_self) and any non-NPC target are skipped.
                if !tick_action.on_self {
                    let initial_enmity = match tick_action.kind {
                        TickKind::DamageMagic | TickKind::DamagePhysical => {
                            damage_snapshot.map(|snapshot| snapshot.base_amount)
                        }
                        TickKind::Heal => None,
                        TickKind::RestoreMp => None,
                    };
                    if let Some(amount) = initial_enmity
                        && let Some(actor) = instance.find_actor_mut(tick_target)
                        && let Some(hate_list) = actor.npc_hate_list_mut()
                    {
                        let entry = hate_list.entry(from_actor_id).or_insert(0);
                        *entry = entry.saturating_add(amount as u32);
                    }
                }
            }

            // The action's LoseEffect statuses that the caster really holds, sampled BEFORE the
            // job modules below can consume them. See `lost_statuses` use further down.
            lost_statuses =
                collect_held_lost_statuses(instance, from_actor_id, &effects_builder.effects);

            // Register damage barriers requested by the action. The status itself is also sent as a
            // normal gain effect, but the absorb pool lives server-side and is consumed on damage.
            for barrier_action in &effects_builder.barrier_actions {
                let barrier_target = if barrier_action.on_self {
                    from_actor_id
                } else {
                    resolved_request.target.object_id
                };
                if let Some(actor) = instance.find_actor_mut(barrier_target) {
                    let max_health_points = actor.get_common_spawn().max_health_points;
                    if let Some(status_effects) = actor.status_effects_mut() {
                        status_effects.add_barrier(
                            barrier_action.effect_id,
                            barrier_action.param,
                            barrier_action.duration,
                            barrier_action.amount,
                            from_actor_id,
                            max_health_points,
                        );
                    }
                }
            }

            job_gauge_data = if let Some(job) = job_for(class_job)
                && let Some(actor) = instance.find_actor_mut(from_actor_id)
            {
                bard_action_update =
                    job.update_state_after_action(resolved_request.action_id, actor, from_actor_id);
                let level = actor.get_common_spawn().level;
                if let NetworkedActor::Player { combat_state, .. } = actor {
                    job.build_gauge_data(combat_state, level)
                        .map(|data| (job.gauge_class_job_id(class_job), data))
                } else {
                    None
                }
            } else {
                bard_action_update = JobActionUpdate::default();
                None
            };

            // Party raid-buff propagation (Battle Voice / Radiant Finale). Runs after the bard
            // state update (which computed the coda bonus) and before the grant routing consumes
            // status_grants. Coda bonus is 0 for Battle Voice, harmless.
            let coda_bonus = if let Some(NetworkedActor::Player { combat_state, .. }) =
                instance.find_actor(from_actor_id)
            {
                combat_state.bard.radiant_finale_damage_bonus_percent
            } else {
                0
            };
            if let Some((status_id, param, duration)) = job_for(class_job)
                .and_then(|job| job.party_buff_for_action(resolved_request.action_id, coda_bonus))
            {
                // Radiant Finale only: stamp the coda bonus onto the caster's own GainEffectSelf
                // entry so the param-based read-site scales the caster too. Battle Voice's self
                // entry already has param 0, so guarding on the RF status id leaves it untouched.
                if status_id == STATUS_RADIANT_FINALE {
                    for effect in &mut effects_builder.effects {
                        if let TargetEffectKind::GainEffectSelf {
                            effect_id,
                            param: self_param,
                            ..
                        } = &mut effect.0
                            && *effect_id == status_id
                        {
                            *self_param = param;
                        }
                    }
                }
                // Every Player present in the caster's instance is a candidate recipient.
                let instance_player_ids: std::collections::HashSet<ObjectId> = instance
                    .actors
                    .iter()
                    .filter(|(_, actor)| matches!(actor, NetworkedActor::Player { .. }))
                    .map(|(id, _)| *id)
                    .collect();
                // Snapshot the party members in a SHORT network-lock scope; do not hold the lock
                // across the push loop (matches the party.rs broadcast helpers' discipline).
                let member_ids: Vec<ObjectId> = {
                    let network = network.lock();
                    crate::server::party::get_party_id_from_actor_id(&network, from_actor_id)
                        .and_then(|id| network.parties.get(&id))
                        .map(|party| party.members.iter().map(|m| m.actor_id).collect())
                        .unwrap_or_default()
                };
                for recipient in
                    party_player_recipients(from_actor_id, &member_ids, &instance_player_ids)
                {
                    effects_builder.status_grants.push(StatusGrant {
                        recipient,
                        effect_id: status_id,
                        param,
                        duration,
                    });
                }
            }

            if remove_cooldowns {
                if let Some(actor) = instance.find_actor_mut(from_actor_id) {
                    let mut game_data = game_data.lock();
                    cleared_cooldown_groups =
                        clear_action_cooldowns(actor, &mut game_data, resolved_request.action_id);
                } else {
                    cleared_cooldown_groups = Vec::new();
                }
            } else {
                // Normal cooldowns are started at cast start in handle_action_messages (which runs
                // immediately), not here on the 500ms tick, so they stay aligned with the client.
                cleared_cooldown_groups = Vec::new();
            }

            update_actor_hp_mp(network.clone(), instance, resolved_request.target.object_id);
            if from_actor_id != resolved_request.target.object_id && action_mp_cost > 0 {
                update_actor_hp_mp(network.clone(), instance, from_actor_id);
            }
            summoner::register_slipstream_lingering_aoe_after_action(
                instance,
                from_actor_id,
                resolved_request.action_id,
                resolved_request.target.object_id,
            );
            if consume_swiftcast {
                remove_status_from_actor_instance(instance, from_actor_id, STATUS_SWIFTCAST);
            }
            send_dirty_status_effects(network.clone(), instance, from_actor_id);
        }

        {
            let mut network = network.lock();

            // Only the remove-cooldowns cheat pushes explicit cooldown packets; normal GCDs are
            // predicted client-side (see start_action_cooldowns), so we don't echo them back.
            for cooldown_group in cleared_cooldown_groups {
                network.send_to_by_actor_id(
                    from_actor_id,
                    FromServer::ActorControlSelf(ActorControlCategory::SetCooldownTimer {
                        cooldown_group,
                        elapsed_centisec: 0,
                        total_centisec: 0,
                    }),
                    DestinationNetwork::ZoneClients,
                );
            }

            if let Some(cooldown_update) = bard_action_update.cooldown_update {
                network.send_to_by_actor_id(
                    from_actor_id,
                    FromServer::ActorControlSelf(ActorControlCategory::IncrementRecast {
                        cooldown_group: cooldown_update.cooldown_group,
                        delta_time_centisec: cooldown_update.delta_centisec,
                    }),
                    DestinationNetwork::ZoneClients,
                );
            }
        }

        if has_summoner_pet_transition {
            let mut data = data.lock();
            let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                return;
            };
            let mut network = network.lock();
            summoner::prepare_pet_transition_for_action(
                &mut network,
                instance,
                from_actor_id,
                resolved_request.action_id,
            );
        }

        // The global sequence the action packet below actually consumed. The `EffectResult`s sent
        // further down describe the status changes *from this action*, so they have to quote the
        // same number rather than taking one of their own -- that is what lets the client tie the
        // two together.
        //
        // Stays 0 only if no action packet went out at all (the AoE builder refusing to produce
        // one), in which case there is nothing for the client to correlate against anyway.
        let mut action_global_sequence = 0;

        // The (recipient, status_id) pairs the outgoing effect array already notified via path 1.
        // Populated only on the single-target path; the AoE builder writes no status entries, so it
        // stays empty and every status_grant falls through to path 2 (cat 23).
        let mut notified: Vec<(ObjectId, u16)> = Vec::new();

        {
            let effects = target_action_result_effects(&effects_builder.effects);

            let action_animation_id = {
                let mut game_data = game_data.lock();
                if resolved_request.action_type == ActionType::Item {
                    game_data
                        .lookup_item_action_data(resolved_request.action_id)
                        .map(|(action_type, _, _)| action_type)
                        .unwrap_or(resolved_request.action_id as u16)
                } else {
                    resolved_request.action_id as u16
                }
            };

            let aoe_damage_element = if aoe_radius > 0.0 && aoe_base_damage > 0 {
                let mut game_data = game_data.lock();
                Some(game_data.get_action_damage_element(resolved_request.action_id))
            } else {
                None
            };

            let mut data = data.lock();
            let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                return;
            };

            // Gather every *other* enemy inside the AoE radius (if this action is an AoE at all),
            // rolling and applying each one's damage/enmity/HP now. The primary target occupies
            // slot 0; these are slots 1.. of the same effect packet.
            let mut secondary_targets: Vec<(ObjectTypeId, TargetEffect)> = Vec::new();
            if let Some(damage_element) = aoe_damage_element {
                if let Some(center) = instance
                    .find_actor(resolved_request.target.object_id)
                    .map(|actor| actor.position().0)
                {
                    let mut secondaries: Vec<ObjectId> = instance
                        .actors
                        .iter()
                        .filter_map(|(id, actor)| match actor {
                            NetworkedActor::Npc {
                                spawn,
                                state,
                                targetable,
                                ..
                            } if *id != resolved_request.target.object_id
                                && *state != NpcState::Dead
                                && *targetable
                                && !spawn.common.owner_id.is_valid()
                                && spawn.common.health_points > 0
                                && Vec3::distance(spawn.common.position.0, center)
                                    <= aoe_radius =>
                            {
                                Some(*id)
                            }
                            _ => None,
                        })
                        .collect();

                    // Reserve slot 0 for the primary; secondaries fill the rest, capped at the
                    // largest AoE packet. Anything past that is dropped (its damage swallowed),
                    // matching how retail caps a single effect packet.
                    let secondary_cap = MAX_AOE_TARGETS - 1;
                    if secondaries.len() > secondary_cap {
                        tracing::debug!(
                            "AoE {} hit {} secondaries, capping at {} (dropping {})",
                            resolved_request.action_id,
                            secondaries.len(),
                            secondary_cap,
                            secondaries.len() - secondary_cap,
                        );
                        secondaries.truncate(secondary_cap);
                    }

                    for target_id in secondaries {
                        let falloff_base =
                            aoe_secondary_falloff_base(resolved_request.action_id, aoe_base_damage);
                        let base_amount = source_damage_modifiers.apply_base_damage(falloff_base);
                        let (rolled, kind) = lua_player
                            .base_parameters
                            .roll_damage_with_modifiers(base_amount, source_damage_modifiers.roll);
                        let rolled = apply_target_player_mitigation(
                            rolled,
                            instance.find_actor(target_id),
                            aoe_damage_type,
                        );

                        if let Some(actor) = instance.find_actor_mut(target_id)
                            && let Some(hate_list) = actor.npc_hate_list_mut()
                        {
                            let entry = hate_list.entry(from_actor_id).or_insert(0);
                            *entry = entry.saturating_add(rolled as u32);
                        }

                        if let Some(actor) = instance.find_actor_mut(target_id) {
                            actor.apply_damage(rolled);
                        } else {
                            continue;
                        }

                        secondary_targets.push((
                            ObjectTypeId {
                                object_id: target_id,
                                object_type: resolved_request.target.object_type,
                            },
                            TargetEffect(TargetEffectKind::Damage {
                                amount: rolled,
                                damage_kind: kind,
                                damage_type: aoe_damage_type,
                                damage_element,
                                bonus_percent: 0,
                                unk3: 0,
                                unk4: 0,
                            }),
                        ));
                    }
                }
            }

            if secondary_targets.is_empty() {
                // Single target (or an AoE that hit nothing else): a plain ActionEffect1, carrying
                // the primary's full effect set (damage, combo, gained buffs, ...).
                // Record which (recipient, status) pairs this array notifies so status_grants that
                // are already covered here are not also sent a cat 23 (path 2).
                let wire_effect_count = effects
                    .iter()
                    .filter(|e| !matches!(e.0, TargetEffectKind::None))
                    .count() as u8;
                notified = wire_notified_status_pairs(
                    &effects,
                    wire_effect_count,
                    resolved_request.target.object_id,
                    from_actor_id,
                );
                let mut net = network.lock();
                let ipc =
                    ServerZoneIpcSegment::new(ServerZoneIpcData::ActionEffect1(ActionEffect1 {
                        animation_target_id: resolved_request.target,
                        target_id_again: resolved_request.target,
                        action_id: resolved_request.action_id,
                        animation_lock: ANIMATION_LOCK_TIME,
                        rotation: common_spawn.rotation,
                        spell_id: action_animation_id,
                        source_sequence: resolved_request.sequence,
                        target_count: 1,
                        effects,
                        action_type: resolved_request.action_type,
                        global_sequence: net.global_action_sequence,
                        ..Default::default()
                    }));
                action_global_sequence = net.global_action_sequence;
                net.global_action_sequence += 1;
                net.send_in_range_inclusive_instance(
                    from_actor_id,
                    instance,
                    FromServer::PacketSegment(ipc, from_actor_id),
                    DestinationNetwork::ZoneClients,
                );
            } else {
                // Multiple targets: one ActionEffectN packet, primary at slot 0 then each secondary.
                let center = instance
                    .find_actor(resolved_request.target.object_id)
                    .map(|actor| actor.position().0)
                    .unwrap_or_default();

                let mut all_targets: Vec<(ObjectTypeId, TargetEffect)> =
                    Vec::with_capacity(secondary_targets.len() + 1);
                let primary_effect = effects_builder
                    .effects
                    .iter()
                    .copied()
                    .find(|e| matches!(e.0, TargetEffectKind::Damage { .. }))
                    .unwrap_or_default();
                all_targets.push((resolved_request.target, primary_effect));
                all_targets.extend(secondary_targets.iter().copied());

                let mut net = network.lock();
                let header = ActionEffectHeader {
                    animation_target_id: resolved_request.target,
                    action_id: resolved_request.action_id,
                    animation_lock: ANIMATION_LOCK_TIME,
                    rotation: common_spawn.rotation,
                    spell_id: action_animation_id,
                    source_sequence: resolved_request.sequence,
                    action_type: resolved_request.action_type,
                    target_count: all_targets.len() as u8,
                    global_sequence: net.global_action_sequence,
                    ..Default::default()
                };
                if let Some(ipc_data) =
                    build_aoe_effect_packet(header, &all_targets, kawari::common::Position(center))
                {
                    action_global_sequence = net.global_action_sequence;
                    net.global_action_sequence += 1;
                    let ipc = ServerZoneIpcSegment::new(ipc_data);
                    net.send_in_range_inclusive_instance(
                        from_actor_id,
                        instance,
                        FromServer::PacketSegment(ipc, from_actor_id),
                        DestinationNetwork::ZoneClients,
                    );
                }

                // Drop the network lock before update_actor_hp_mp (which locks it internally),
                // then sync each secondary's HP bar (the primary's is synced elsewhere).
                drop(net);
                for (target, _) in &secondary_targets {
                    update_actor_hp_mp(network.clone(), instance, target.object_id);
                }
            }
        }

        if let Some((gauge_class_job_id, data)) = job_gauge_data {
            let mut network = network.lock();
            send_job_gauge_update(&mut network, from_actor_id, gauge_class_job_id, data);
        }

        if has_summoner_pet_transition {
            let mut data = data.lock();
            let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                return;
            };
            let mut network = network.lock();
            let _ = summoner::spawn_pet_after_action(
                &mut network,
                instance,
                from_actor_id,
                resolved_request.action_id,
                resolved_request.target.object_id,
            );
            if summoner::is_demi_summon(resolved_request.action_id) {
                summoner::schedule_demi_auto_attack(instance, from_actor_id);
            }
        }

        // Now that the result packet (carrying the SummonPet effect, which plays the summon
        // gesture/VFX) has been sent, actually spawn the pet so it appears with animation.
        if summon_pet_after {
            let mut data = data.lock();
            if let Some(instance) = data.find_actor_instance_mut(from_actor_id) {
                summoner::apply_summon_pet_effect(network.clone(), instance, from_actor_id);
            }
        }

        {
            let mut num_self_entries = 0u8;
            let mut self_entries = [EffectEntry::default(); 4];
            let mut num_target_entries = 0u8;
            let mut target_entries = [EffectEntry::default(); 4];

            for effect in &effects_builder.effects {
                if let TargetEffectKind::GainEffect {
                    effect_id,
                    duration,
                    param,
                    ..
                } = effect.0
                {
                    let index = gain_effect(
                        network.clone(),
                        data.clone(),
                        ClientId::default(),
                        resolved_request.target.object_id,
                        effect_id,
                        param,
                        duration,
                        from_actor_id,
                        false,
                    );

                    target_entries[num_target_entries as usize] = EffectEntry {
                        index,
                        id: effect_id,
                        param,
                        duration,
                        source_actor_id: from_actor_id,
                        ..Default::default()
                    };
                    num_target_entries += 1;
                }

                if let TargetEffectKind::GainEffectSelf {
                    effect_id,
                    duration,
                    param,
                    ..
                } = effect.0
                {
                    let index = gain_effect(
                        network.clone(),
                        data.clone(),
                        from_id,
                        from_actor_id,
                        effect_id,
                        param,
                        duration,
                        from_actor_id,
                        false,
                    );

                    self_entries[num_self_entries as usize] = EffectEntry {
                        index,
                        id: effect_id,
                        param,
                        duration,
                        source_actor_id: from_actor_id,
                        ..Default::default()
                    };
                    num_self_entries += 1;
                }

                if let TargetEffectKind::LoseEffect { effect_id, .. } = effect.0 {
                    let mut data = data.lock();
                    if let Some(instance) = data.find_actor_instance_mut(from_actor_id) {
                        remove_status_from_actor_instance(instance, from_actor_id, effect_id);
                        if lost_statuses.contains(&effect_id) {
                            let ipc = ActorControlCategory::LoseEffect {
                                effect_id: effect_id as u32,
                                unk2: 0,
                                source_actor_id: from_actor_id,
                            };
                            network.lock().send_ac_in_range_inclusive_instance(
                                instance,
                                from_actor_id,
                                ipc,
                            );
                        }
                    }
                }
            }

            if num_self_entries > 0 {
                let mut data = data.lock();
                let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                    return;
                };
                let Some(actor) = instance.find_actor(from_actor_id) else {
                    return;
                };
                let current_common_spawn = actor.get_common_spawn().clone();
                let shield = actor.shield_percent();
                let ipc =
                    ServerZoneIpcSegment::new(ServerZoneIpcData::EffectResult(EffectResult {
                        count: 1,
                        global_sequence: action_global_sequence,
                        target_id: from_actor_id,
                        health_points: current_common_spawn.health_points,
                        max_health_points: current_common_spawn.max_health_points,
                        resource_points: current_common_spawn.resource_points,
                        classjob_id: current_common_spawn.class_job,
                        shield,
                        entry_count: num_self_entries,
                        statuses: self_entries,
                        ..Default::default()
                    }));
                let mut network = network.lock();
                network.send_in_range_inclusive_instance(
                    from_actor_id,
                    instance,
                    FromServer::PacketSegment(ipc, from_actor_id),
                    DestinationNetwork::ZoneClients,
                );
            }

            if num_target_entries > 0 {
                let mut data = data.lock();
                let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                    return;
                };

                let Some(actor) = instance.find_actor(resolved_request.target.object_id) else {
                    return;
                };
                let target_common_spawn = actor.get_common_spawn().clone();
                let shield = actor.shield_percent();

                let ipc =
                    ServerZoneIpcSegment::new(ServerZoneIpcData::EffectResult(EffectResult {
                        count: 1,
                        global_sequence: action_global_sequence,
                        target_id: resolved_request.target.object_id,
                        health_points: target_common_spawn.health_points,
                        max_health_points: target_common_spawn.max_health_points,
                        resource_points: target_common_spawn.resource_points,
                        classjob_id: target_common_spawn.class_job,
                        shield,
                        entry_count: num_target_entries,
                        statuses: target_entries,
                        ..Default::default()
                    }));
                let mut network = network.lock();
                let Some(instance) = data.find_actor_instance_mut(from_actor_id) else {
                    return;
                };
                network.send_in_range_inclusive_instance(
                    resolved_request.target.object_id,
                    instance,
                    FromServer::PacketSegment(ipc, resolved_request.target.object_id),
                    DestinationNetwork::ZoneClients,
                );
            }

            // Path 2: statuses granted to a recipient the effect array did not already notify (a
            // party buff to a dance partner, an AoE self-buff dropped by the AoE builder, ...). Each
            // surviving grant is applied server-side (inform_players: false) and announced with its
            // own cat 23, but only when the recipient is a player -- retail never sends cat 23 to an
            // enemy (PLAN F1 = B-narrowed).
            let surviving_grants =
                grants_needing_actor_control(&effects_builder.status_grants, &notified);
            for grant in &surviving_grants {
                gain_effect(
                    network.clone(),
                    data.clone(),
                    ClientId::default(),
                    grant.recipient,
                    grant.effect_id,
                    grant.param,
                    grant.duration,
                    from_actor_id,
                    false,
                );

                let mut data = data.lock();
                let Some(instance) = data.find_actor_instance_mut(grant.recipient) else {
                    continue;
                };
                let is_player = matches!(
                    instance.find_actor(grant.recipient),
                    Some(NetworkedActor::Player { .. })
                );
                if !is_player {
                    continue;
                }
                let ipc = if from_actor_id != grant.recipient {
                    ActorControlCategory::StatusEffectNotification {
                        effect_id: grant.effect_id as u32,
                        effect_kind: STATUS_NOTIFICATION_GAINED_FROM_OTHER,
                        effect_id_again: grant.effect_id as u32,
                        source_actor_id: from_actor_id,
                    }
                } else {
                    ActorControlCategory::GainEffect {
                        effect_id: grant.effect_id as u32,
                        param: grant.param as u32,
                        source_actor_id: from_actor_id,
                    }
                };
                network
                    .lock()
                    .send_ac_in_range_inclusive_instance(instance, grant.recipient, ipc);
            }

            {
                let mut data = data.lock();
                if let Some(instance) = data.find_actor_instance_mut(from_actor_id) {
                    send_dirty_status_effects(network.clone(), instance, from_actor_id);
                    if resolved_request.target.object_id != from_actor_id {
                        send_dirty_status_effects(
                            network.clone(),
                            instance,
                            resolved_request.target.object_id,
                        );
                    }
                }
            }

            // Path 2 changed each grant recipient's status list; flush each against its OWN instance
            // (a third-party recipient need not share the caster's instance). send_dirty_status_effects
            // is idempotent, so a recipient coinciding with the caster/target (already flushed above)
            // no-ops here.
            for grant in &surviving_grants {
                let mut data = data.lock();
                if let Some(instance) = data.find_actor_instance_mut(grant.recipient) {
                    send_dirty_status_effects(network.clone(), instance, grant.recipient);
                }
            }
        }
    }

    let mut network = network.lock();
    network.send_to(
        from_id,
        FromServer::NewTasks(lua_player.queued_tasks),
        DestinationNetwork::ZoneClients,
    );
}

/// Executes an action from an enemy.
pub fn execute_enemy_action(
    network: Arc<Mutex<NetworkState>>,
    instance: &mut Instance,
    lua: Arc<Mutex<KawariLua>>,
    from_actor_id: ObjectId,
    request: ActionRequest,
) {
    let mut lua_player = LuaPlayer {
        player_data: PlayerData::default(),
        status_effects: StatusEffects::default(),
        queued_tasks: Vec::new(),
        zone_data: LuaZone::default(),
        content_data: LuaContent::default(),
        base_parameters: BaseParameters::default(),
        combat_state: PlayerCombatState::default(),
        level: 0,
    };

    let effects_builder;
    let common_spawn;
    let source_has_feint;
    let source_has_addle;
    {
        let Some(actor) = instance.find_actor(from_actor_id) else {
            return;
        };

        common_spawn = actor.get_common_spawn().clone();
        lua_player.level = common_spawn.level as u16;
        let source_status_effects = actor.status_effects();
        source_has_feint = source_status_effects
            .and_then(|status_effects| status_effects.get(STATUS_FEINT))
            .is_some();
        source_has_addle = source_status_effects
            .and_then(|status_effects| status_effects.get(STATUS_ADDLE))
            .is_some();

        effects_builder = match &request.action_type {
            ActionType::Action => {
                execute_normal_action(lua.clone(), &request, &mut lua_player, false)
            }
            _ => unreachable!(),
        };
    }

    if let Some(mut effects_builder) = effects_builder {
        {
            let Some(actor) = instance.find_actor_mut(request.target.object_id) else {
                return;
            };

            // Player targets mitigate enemy damage by their defense; NPCs have none.
            let (mitigation_phys, mitigation_magic) =
                if let NetworkedActor::Player { parameters, .. } = &*actor {
                    (
                        parameters.mitigation_against(false),
                        parameters.mitigation_against(true),
                    )
                } else {
                    (0.0, 0.0)
                };

            // Apply ±5% variance and the target's defense mitigation to each hit.
            for effect in &mut effects_builder.effects {
                if let TargetEffectKind::Damage {
                    amount,
                    damage_type,
                    ..
                } = &mut effect.0
                {
                    let mitigation = if *damage_type == DamageType::Magic {
                        mitigation_magic
                    } else {
                        mitigation_phys
                    };
                    let variance = 0.95 + fastrand::f64() * 0.10;
                    let outgoing_multiplier = outgoing_damage_multiplier(
                        source_has_feint,
                        source_has_addle,
                        *damage_type,
                    );
                    *amount =
                        ((*amount as f64) * variance * outgoing_multiplier * (1.0 - mitigation))
                            .floor() as u32;
                }
            }

            for effect in &effects_builder.effects {
                if let TargetEffectKind::Damage { amount, .. } = effect.0 {
                    actor.apply_damage(amount as u32);
                }
            }
        }

        update_actor_hp_mp(network.clone(), instance, request.target.object_id);

        // Quoted by the `EffectResult` below so the status changes are tied to this action. See the
        // equivalent in `handle_action_messages`.
        let action_global_sequence;

        {
            let mut network = network.lock();

            let mut effects = [TargetEffect::default(); 8];
            effects[..effects_builder.effects.len()].copy_from_slice(&effects_builder.effects);

            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ActionEffect1(ActionEffect1 {
                animation_target_id: request.target,
                target_id_again: request.target,
                action_id: request.action_id,
                animation_lock: ANIMATION_LOCK_TIME,
                rotation: common_spawn.rotation,
                spell_id: request.action_id as u16,
                source_sequence: request.sequence,
                target_count: 1,
                effects,
                action_type: request.action_type,
                global_sequence: network.global_action_sequence,
                ..Default::default()
            }));
            action_global_sequence = network.global_action_sequence;
            network.global_action_sequence += 1;

            network.send_in_range_inclusive_instance(
                from_actor_id,
                instance,
                FromServer::PacketSegment(ipc, from_actor_id),
                DestinationNetwork::ZoneClients,
            );
        }

        {
            let mut num_entries = 0u8;
            let mut entries = [EffectEntry::default(); 4];

            for effect in &effects_builder.effects {
                if let TargetEffectKind::GainEffect {
                    effect_id,
                    duration,
                    param,
                    ..
                } = effect.0
                {
                    entries[num_entries as usize] = EffectEntry {
                        index: num_entries,
                        unk1: 0,
                        id: effect_id,
                        param,
                        unk2: 0,
                        duration,
                        source_actor_id: Default::default(),
                    };
                    num_entries += 1;
                }

                // A LoseEffect deliberately takes no entry. EffectResult is a gain notification
                // and the client writes each entry to the slot named by its `index`, so a zeroed
                // entry reads as "slot 0 is now empty" and wipes whatever really occupied it.
            }

            let Some(actor) = instance.find_actor(request.target.object_id) else {
                return;
            };
            let target_common_spawn = actor.get_common_spawn().clone();
            let shield = actor.shield_percent();

            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::EffectResult(EffectResult {
                count: 1,
                global_sequence: action_global_sequence,
                target_id: request.target.object_id,
                health_points: target_common_spawn.health_points,
                max_health_points: target_common_spawn.max_health_points,
                resource_points: target_common_spawn.resource_points,
                target_index: 0,
                classjob_id: target_common_spawn.class_job,
                shield,
                entry_count: num_entries,
                statuses: entries,
            }));
            let mut network = network.lock();
            network.send_in_range_inclusive_instance(
                from_actor_id,
                instance,
                FromServer::PacketSegment(ipc, from_actor_id),
                DestinationNetwork::ZoneClients,
            );
        }
    }
}

pub fn cancel_action(
    network: Arc<Mutex<NetworkState>>,
    from_id: ClientId,
    log_message_id: Option<u32>,
    action_type: Option<ActionType>,
    action_id: Option<u32>,
    interrupted: Option<bool>,
) {
    let log_message_id = log_message_id.unwrap_or(0);
    let action_type = action_type.unwrap_or(ActionType::None);
    let action_id = action_id.unwrap_or(0);
    let interrupted = interrupted.unwrap_or(false);

    let msg = FromServer::ActorControlSelf(ActorControlCategory::CancelCast {
        log_message_id,
        action_type: action_type as u32,
        action_id,
        interrupted,
    });

    let mut network = network.lock();
    network.send_to(from_id, msg, DestinationNetwork::ZoneClients);
}

/// Handles normal actions, powered by Lua.
pub fn execute_normal_action(
    lua: Arc<Mutex<KawariLua>>,
    request: &ActionRequest,
    lua_player: &mut LuaPlayer,
    in_combo: bool,
) -> Option<EffectsBuilder> {
    let mut effects_builder = None;
    let lua = lua.lock();
    let state = lua.0.app_data_ref::<KawariLuaState>().unwrap();

    let key = request.action_id;
    if let Some(action_script) = state.action_scripts.get(&key) {
        let script_bytes = match std::fs::read(action_script) {
            Ok(bytes) => bytes,
            Err(err) => {
                tracing::warn!("Failed to read action script {action_script}: {err:?}");
                return None;
            }
        };

        let result = lua.0.scope(|scope| {
            let connection_data = scope.create_userdata_ref_mut(lua_player)?;

            lua.0
                .load(script_bytes)
                .set_name("@".to_string() + action_script)
                .exec()?;

            let func: Function = lua.0.globals().get("doAction")?;

            effects_builder = Some(func.call::<EffectsBuilder>((connection_data, in_combo))?);

            Ok(())
        });
        if let Err(err) = result {
            tracing::warn!("Error executing action script {action_script}: {err:?}");
            return None;
        }
    } else {
        tracing::warn!("Action {key} isn't scripted yet!");
    }

    effects_builder
}

/// Handles item actions, powered by Lua.
pub fn execute_item_action(
    game_data: Arc<Mutex<GameData>>,
    lua: Arc<Mutex<KawariLua>>,
    request: &ActionRequest,
    lua_player: &mut LuaPlayer,
) -> Option<EffectsBuilder> {
    let lua = lua.lock();

    let key = request.action_id;
    let (action_type, action_data, additional_data);
    let is_misc;
    {
        let mut gamedata = game_data.lock();
        (action_type, action_data, additional_data) =
            gamedata.lookup_item_action_data(key).unwrap_or_default();
        is_misc = gamedata.item_is_misc(key);
    }

    let mut effects_builder = None;
    let result = lua.0.scope(|scope| {
        let connection_data = scope.create_userdata_ref_mut(lua_player)?;

        let func: Function = lua.0.globals().get("dispatchItem")?;

        match func.call::<(String, u32)>((
            &connection_data,
            key,
            action_type,
            action_data,
            additional_data,
            is_misc,
        )) {
            Ok((action_script, arg)) => {
                let path = FilesystemConfig::locate_script_file(&action_script);
                let script_bytes = match std::fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        tracing::warn!(
                            "Failed to read item action script {action_script}: {err:?}"
                        );
                        return Ok(());
                    }
                };
                lua.0
                    .load(script_bytes)
                    .set_name("@".to_string() + &action_script)
                    .exec()?;

                let func: Function = lua.0.globals().get("doAction")?;

                effects_builder = Some(func.call::<EffectsBuilder>((connection_data, arg))?);
            }
            Err(err) => {
                tracing::error!("Error while calling dispatchItem: {:?}", err);
            }
        }

        Ok(())
    });
    if let Err(err) = result {
        tracing::warn!("Error executing item action {key}: {err:?}");
    }

    effects_builder
}

/// Handles mount-related actions.
pub fn execute_mount_action(
    network: Arc<Mutex<NetworkState>>,
    from_actor_id: ObjectId,
    request: &ActionRequest,
    actor: &NetworkedActor,
    instance: &Instance,
) -> Option<EffectsBuilder> {
    let mut network = network.lock();

    let common_spawn = actor.get_common_spawn();

    let mut effects = [TargetEffect::default(); 8];
    effects[0] = TargetEffect(TargetEffectKind::Mount {
        unk1: 1,
        unk2: 0,
        id: request.action_id as u16,
    });

    let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ActionEffect1(ActionEffect1 {
        animation_target_id: request.target,
        target_id_again: request.target,
        action_id: request.action_id,
        animation_lock: ANIMATION_LOCK_TIME,
        rotation: common_spawn.rotation,
        spell_id: 4,
        source_sequence: request.sequence,
        target_count: 1,
        effects,
        action_type: request.action_type,
        global_sequence: network.global_action_sequence,
        ..Default::default()
    }));
    network.global_action_sequence += 1;

    network.send_in_range_inclusive_instance(
        from_actor_id,
        instance,
        FromServer::PacketSegment(ipc, from_actor_id),
        DestinationNetwork::ZoneClients,
    );

    let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::Mount {
        id: request.action_id as u16,
        unk1: [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    });
    network.send_in_range_inclusive_instance(
        from_actor_id,
        instance,
        FromServer::PacketSegment(ipc, from_actor_id),
        DestinationNetwork::ZoneClients,
    );

    None
}

// Sends the ActorControls to inform the actor that they're dead.
pub fn kill_actor(
    network: Arc<Mutex<NetworkState>>,
    instance: &mut Instance,
    from_actor_id: ObjectId,
) {
    let mut network = network.lock();

    set_character_mode(
        instance,
        &mut network,
        from_actor_id,
        CharacterMode::Dead,
        0,
    );

    network.send_ac_in_range_inclusive_instance(
        instance,
        from_actor_id,
        ActorControlCategory::Kill { animation_id: 0 },
    );

    let mut npc_id = None;
    let mut position = None;
    if let Some(actor) = instance.find_actor(from_actor_id)
        && let Some(npc) = actor.get_npc_spawn()
    {
        npc_id = Some(npc.common.layout_id);
    }

    if let Some(actor) = instance.find_actor_mut(from_actor_id)
        && let NetworkedActor::Npc {
            state,
            spawn,
            hate_list,
            ..
        } = actor
    {
        *state = NpcState::Dead;
        position = Some(spawn.common.position);
        // Clear hate so nothing lingers if this actor is ever revived/reset.
        hate_list.clear();
    }

    if let Some(npc_id) = npc_id
        && let Some(director) = &mut instance.director
    {
        director.on_actor_death(npc_id, position.unwrap());
    }

    instance.cancel_actor_tasks(from_actor_id);

    if let Some(actor) = instance.find_actor_mut(from_actor_id)
        && let NetworkedActor::Npc {
            spawn, timeline, ..
        } = actor
    {
        let mut new_timeline_states = Vec::new();

        for action in &timeline.on_death {
            match action {
                TimepointData::TimelineState { states } => {
                    let gimmick_id = spawn.gimmick_id;
                    new_timeline_states.push((gimmick_id, states.clone()));
                }
                _ => unimplemented!(),
            }
        }

        for (gimmick_id, states) in new_timeline_states {
            let actor_id = instance.find_object_by_bind_layout_id(gimmick_id);
            if let Some(actor_id) = actor_id {
                set_shared_group_timeline_state(instance, &mut network, actor_id, &states);
            }
        }

        instance.insert_task(
            ClientId::default(),
            from_actor_id,
            DEAD_FADE_OUT_TIME,
            QueuedTaskData::DeadFadeOut {
                actor_id: from_actor_id,
            },
        );
    }
}

/// Updates other actors about this actor's HP and MP.
pub fn update_actor_hp_mp(
    network: Arc<Mutex<NetworkState>>,
    instance: &mut Instance,
    target_actor_id: ObjectId,
) {
    let mut send_kill_actor = false;

    {
        let Some(actor) = instance.find_actor(target_actor_id) else {
            return;
        };

        let common_spawn = actor.get_common_spawn();

        {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateHpMpTp {
                hp: common_spawn.health_points,
                mp: common_spawn.resource_points,
                unk: 0,
            });
            let mut network = network.lock();
            network.send_in_range_inclusive_instance(
                target_actor_id,
                instance,
                FromServer::PacketSegment(ipc, target_actor_id),
                DestinationNetwork::ZoneClients,
            );
        }

        if common_spawn.health_points == 0 && common_spawn.mode != CharacterMode::Dead {
            send_kill_actor = true;
        }
    }

    send_dirty_status_effects(network.clone(), instance, target_actor_id);

    if send_kill_actor {
        kill_actor(network, instance, target_actor_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLAST_ARROW_READY: u16 = 2692;
    const BLAST_ARROW_READY_ALT: u16 = 3142;

    fn lose(effect_id: u16) -> TargetEffect {
        TargetEffect(TargetEffectKind::LoseEffect {
            param: 0,
            unk: [0; 3],
            effect_id,
        })
    }

    fn gain_target(effect_id: u16) -> TargetEffect {
        TargetEffect(TargetEffectKind::GainEffect {
            unk1: 0,
            unk2: 0,
            unk3: 0,
            effect_id,
            param: 0,
            duration: 30.0,
        })
    }

    fn gain_self(effect_id: u16) -> TargetEffect {
        TargetEffect(TargetEffectKind::GainEffectSelf {
            unk1: 0,
            unk2: 0,
            unk3: 0,
            effect_id,
            param: 0,
            duration: 30.0,
        })
    }

    /// Status gains have to survive into the array, or the client never plays the buff animation
    /// or writes the log line — songs and Raging Strikes were silent because the whole kind was
    /// filtered out. The kind itself is preserved: it encodes who the entry is for.
    #[test]
    fn status_gains_reach_the_client() {
        let effects = target_action_result_effects(&[gain_target(1821), gain_self(3867)]);

        assert!(matches!(
            effects[0].0,
            TargetEffectKind::GainEffect {
                effect_id: 1821,
                ..
            }
        ));
        assert!(matches!(
            effects[1].0,
            TargetEffectKind::GainEffectSelf {
                effect_id: 3867,
                ..
            }
        ));
    }

    /// A removal is announced over its own ActorControl, so it must not take a slot in here: the
    /// client writes each entry to the slot its `index` names, and a blank entry wipes slot 0.
    #[test]
    fn removals_take_no_entry() {
        let effects = target_action_result_effects(&[lose(2692)]);

        // The lose effect is filtered out, so slot 0 stays the default (no effect).
        assert_eq!(effects[0].0, TargetEffectKind::None);
    }

    /// The flags byte has to say "at source", or the client credits the buff to the action's
    /// target: Apex Arrow on a striking dummy announced the dummy gaining Blast Arrow Ready.
    #[test]
    fn a_gain_on_the_caster_is_flagged_at_source() {
        let effects = target_action_result_effects(&[gain_self(2692)]);

        let TargetEffectKind::GainEffectSelf { unk3, .. } = effects[0].0 else {
            panic!(
                "expected the gain to stay GainEffectSelf, got {:?}",
                effects[0].0
            );
        };
        assert_eq!(unk3 & EFFECT_FLAG_AT_SOURCE, EFFECT_FLAG_AT_SOURCE);
    }

    /// The wire layout Kawari writes must line up with the one the client reads. Field-for-field
    /// against a retail Sprint capture: `0E 00 00 14 00 00 AF 04`, which BossMod decodes as
    /// Type=14, Param2=20, Param4=0 (flags), Value=1199.
    #[test]
    fn a_gain_effect_serializes_to_the_retail_byte_layout() {
        use binrw::BinWrite;
        use std::io::Cursor;

        let effect = TargetEffect(TargetEffectKind::GainEffect {
            unk1: 0,
            unk2: 0,
            unk3: 0,
            effect_id: 1199,
            param: 20,
            duration: 30.0,
        });

        let mut cursor = Cursor::new(Vec::new());
        effect.write_le(&mut cursor).unwrap();

        assert_eq!(
            cursor.into_inner(),
            vec![0x0E, 0, 0, 0x14, 0, 0, 0xAF, 0x04]
        );
    }

    /// Only the ids the caster really holds are reported, so the client is not told twice about
    /// one removal. Blast Arrow declares both of its Ready ids, but only one is ever present.
    #[test]
    fn held_lost_statuses_skip_ids_the_actor_does_not_have() {
        let mut instance = Instance::default();
        let actor_id = ObjectId(1);
        instance.insert_empty_actor(actor_id);

        if let Some(actor) = instance.find_actor_mut(actor_id)
            && let Some(status_effects) = actor.status_effects_mut()
        {
            status_effects.add(BLAST_ARROW_READY, 0, 10.0);
        }

        let effects = [lose(BLAST_ARROW_READY), lose(BLAST_ARROW_READY_ALT)];
        let held = collect_held_lost_statuses(&instance, actor_id, &effects);

        assert_eq!(held, vec![BLAST_ARROW_READY]);
    }

    fn grant(recipient: ObjectId, effect_id: u16) -> StatusGrant {
        StatusGrant {
            recipient,
            effect_id,
            param: 0,
            duration: 30.0,
        }
    }

    /// A gain aimed at the action's target is attributed to that target, so the recipient is already
    /// covered by the effect array (path 1) and must not also get a cat 23.
    #[test]
    fn a_targeted_gain_is_reported_as_notified() {
        let (target, caster) = (ObjectId(10), ObjectId(20));
        let effects = [gain_target(1821)];
        let notified = wire_notified_status_pairs(&effects, 1, target, caster);
        assert_eq!(notified, vec![(target, 1821)]);
    }

    /// A self gain is credited to the caster, not the target. A naive implementation that credited
    /// the target would then also cat-23 the caster, double-notifying.
    #[test]
    fn a_self_gain_is_credited_to_the_caster() {
        let (target, caster) = (ObjectId(10), ObjectId(20));
        let effects = [gain_self(3867)];
        let notified = wire_notified_status_pairs(&effects, 1, target, caster);
        assert_eq!(notified, vec![(caster, 3867)]);
    }

    /// When the action is self-targeted (target == caster), a single gain yields exactly one pair,
    /// so it can never double-notify however the entry was authored.
    #[test]
    fn a_self_targeted_action_yields_one_pair_per_status() {
        let caster = ObjectId(20);
        let effects = [gain_target(1821)];
        let notified = wire_notified_status_pairs(&effects, 1, caster, caster);
        assert_eq!(notified, vec![(caster, 1821)]);
    }

    /// Only the first `wire_effect_count` slots reach the wire. A 9th gain dropped by the 8-slot cap
    /// is not notified via path 1, so it must fall through to path 2.
    #[test]
    fn an_entry_dropped_by_the_slot_cap_is_not_notified() {
        let (target, caster) = (ObjectId(10), ObjectId(20));
        let effects: Vec<TargetEffect> = (0..9).map(|i| gain_target(100 + i)).collect();
        let notified = wire_notified_status_pairs(&effects, 8, target, caster);
        assert_eq!(notified.len(), 8);
        assert!(!notified.contains(&(target, 108)));
    }

    /// A grant to a third party (not in the wire's notified set) needs a cat 23 to reach them.
    #[test]
    fn a_grant_to_a_third_party_needs_an_actor_control() {
        let partner = ObjectId(30);
        let grants = [grant(partner, 1821)];
        let notified = vec![(ObjectId(10), 1821)];
        let kept = grants_needing_actor_control(&grants, &notified);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].recipient, partner);
        assert_eq!(kept[0].effect_id, 1821);
    }

    /// A grant whose pair is already in the outgoing effect array must be dropped, or the recipient
    /// is notified twice. This is the regression test for the rolled-back `inform_players` flip.
    #[test]
    fn a_grant_already_covered_by_the_packet_is_dropped() {
        let target = ObjectId(10);
        let grants = [grant(target, 1821)];
        let notified = vec![(target, 1821)];
        let kept = grants_needing_actor_control(&grants, &notified);
        assert!(kept.is_empty());
    }

    /// The same grant twice (a script buffing one actor with one status in one action) notifies once.
    #[test]
    fn duplicate_grants_notify_once() {
        let partner = ObjectId(30);
        let grants = [grant(partner, 1821), grant(partner, 1821)];
        let kept = grants_needing_actor_control(&grants, &[]);
        assert_eq!(kept.len(), 1);
    }

    fn player_id_set(ids: &[ObjectId]) -> std::collections::HashSet<ObjectId> {
        ids.iter().copied().collect()
    }

    /// Recipients are the party members present as Players in the caster's instance, in party order,
    /// with the caster excluded.
    #[test]
    fn party_recipients_are_present_players_excluding_caster() {
        let caster = ObjectId(1);
        let members = [caster, ObjectId(2), ObjectId(3)];
        let present = player_id_set(&[caster, ObjectId(2), ObjectId(3)]);
        let recipients = party_player_recipients(caster, &members, &present);
        assert_eq!(recipients, vec![ObjectId(2), ObjectId(3)]);
    }

    /// A member who is not present in the caster's instance (offline / in another instance / an NPC,
    /// i.e. absent from the Player id set) is excluded.
    #[test]
    fn party_recipients_exclude_members_not_in_instance() {
        let caster = ObjectId(1);
        let members = [caster, ObjectId(2), ObjectId(3)];
        // Member 3 is not a present Player.
        let present = player_id_set(&[caster, ObjectId(2)]);
        let recipients = party_player_recipients(caster, &members, &present);
        assert_eq!(recipients, vec![ObjectId(2)]);
    }

    /// Party order is preserved and duplicate member entries collapse to one recipient.
    #[test]
    fn party_recipients_preserve_order_and_dedup() {
        let caster = ObjectId(1);
        let members = [ObjectId(3), ObjectId(2), ObjectId(3)];
        let present = player_id_set(&[ObjectId(2), ObjectId(3)]);
        let recipients = party_player_recipients(caster, &members, &present);
        assert_eq!(recipients, vec![ObjectId(3), ObjectId(2)]);
    }

    /// A solo caster (empty party) yields no recipients.
    #[test]
    fn party_recipients_empty_when_no_party() {
        let caster = ObjectId(1);
        let present = player_id_set(&[caster]);
        let recipients = party_player_recipients(caster, &[], &present);
        assert!(recipients.is_empty());
    }

    /// A 2964 status with param=6 yields +6% damage regardless of the actor's own Bard combat_state
    /// (which is 0 for a fresh Player). This pins the read-site sourcing the bonus from the status
    /// param, so a propagated Radiant Finale scales for a non-caster.
    #[test]
    fn radiant_finale_bonus_comes_from_status_param() {
        let mut instance = Instance::default();
        let actor_id = ObjectId(1);
        instance.insert_empty_actor(actor_id);
        if let Some(actor) = instance.find_actor_mut(actor_id)
            && let Some(status_effects) = actor.status_effects_mut()
        {
            status_effects.add(STATUS_RADIANT_FINALE, 6, 20.0);
        }

        let actor = instance.find_actor(actor_id);
        let modifiers = action_damage_modifiers(actor);
        // 1000 base damage * (100 + 6)% = 1060.
        assert_eq!(modifiers.apply_base_damage(1000), 1060);
    }
}
