//! Bard (BRD) job-specific logic: action remaps, gauge state, and status syncing.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::{
    StatusEffects,
    gamedata::GameData,
    lua::GaugeAction,
    server::{
        actor::NetworkedActor,
        combat_state::PlayerCombatState,
        jobs::dispatch::{Job, JobActionUpdate, JobCooldownUpdate, JobRefreshResult},
    },
};
use kawari::{common::ObjectId, ipc::zone::ActionRequest};

/// ClassJob row id for Bard (BRD). NOTE: 5 is Archer (ARC); Bard is 23.
const CLASSJOB_BARD: u8 = 23;
/// Some data paths carry the Bard ClassJobCategory row instead of the ClassJob row.
const CLASSJOB_CATEGORY_BARD: u8 = 24;

// ==================== Action IDs ====================

// GCD Actions
const ACTION_HEAVY_SHOT: u32 = 97;
const ACTION_STRAIGHT_SHOT: u32 = 98;
const ACTION_VENOMOUS_BITE: u32 = 100;
const ACTION_QUICK_NOCK: u32 = 106;
const ACTION_WINDBITE: u32 = 113;
const ACTION_BURST_SHOT: u32 = 16495;
const ACTION_REFULGENT_ARROW: u32 = 7409;
const ACTION_SHADOWBITE: u32 = 16494;
const ACTION_CAUSTIC_BITE: u32 = 7406;
const ACTION_STORMBITE: u32 = 7407;
const ACTION_IRON_JAWS: u32 = 3560;
const ACTION_APEX_ARROW: u32 = 16496;
const ACTION_BLAST_ARROW: u32 = 25784;
const ACTION_LADONSBITE: u32 = 25783;
const ACTION_WIDE_VOLLEY: u32 = 36974;

// Song Actions
const ACTION_MAGES_BALLAD: u32 = 114;
const ACTION_ARMYS_PAEON: u32 = 116;
const ACTION_WANDERERS_MINUET: u32 = 3559;

// Ability Actions (oGCD)
const ACTION_EMPYREAL_ARROW: u32 = 3558;
const ACTION_PITCH_PERFECT: u32 = 7404;
const ACTION_RADIANT_FINALE: u32 = 25785;
const ACTION_BATTLE_VOICE: u32 = 118;
const ACTION_RAGING_STRIKES: u32 = 101;
const ACTION_BARRAGE: u32 = 107;
const ACTION_TROUBADOUR: u32 = 7405;
const ACTION_NATURES_MINNE: u32 = 7408;
const ACTION_HEARTBREAK_SHOT: u32 = 36975;
const ACTION_RESONANT_ARROW: u32 = 36976;
const ACTION_RADIANT_ENCORE: u32 = 36977;

// ==================== Status IDs ====================

// Song Statuses. Applied by each song action's Lua script; listed here so that starting a song
// can end the one it replaces (only one song is ever active).
const STATUS_WANDERERS_MINUET: u16 = 2216;
const STATUS_MAGES_BALLAD: u16 = 2217;
const STATUS_ARMYS_PAEON: u16 = 2218;
const SONG_STATUSES: [u16; 3] = [
    STATUS_WANDERERS_MINUET,
    STATUS_MAGES_BALLAD,
    STATUS_ARMYS_PAEON,
];

// Party raid-buff statuses (propagated to party members; see `party_buff_for_action`).
const STATUS_BATTLE_VOICE: u16 = 141;
const STATUS_RADIANT_FINALE: u16 = 2964;

// Song Proc Statuses
const STATUS_REPERTOIRE: u16 = 3137; // Wanderer's Minuet proc - Pitch Perfect ready

// Ready Statuses
const STATUS_HAWK_EYE: u16 = 3861; // Refulgent Arrow / Shadowbite ready
const STATUS_SHADOWBITE_READY: u16 = 3002;
const STATUS_BLAST_ARROW_READY: u16 = 2692;
const STATUS_BLAST_ARROW_READY_2: u16 = 3142; // Alternative status ID
const STATUS_RESONANT_ARROW_READY: u16 = 3862;
const STATUS_RADIANT_ENCORE_READY: u16 = 3863;

// ==================== Gauge Constants ====================

const BARD_GAUGE_SOUL_VOICE: u8 = 0;
const MAX_SOUL_VOICE: u8 = 100;
const REPERTOIRE_PROC_CHANCE_PERCENT: u8 = 80;
const REPERTOIRE_SOUL_VOICE_GAIN: u8 = 5;
const WANDERERS_REPERTOIRE_MAX: u8 = 3;
const ARMYS_REPERTOIRE_MAX: u8 = 4;
const BLOODLETTER_COOLDOWN_GROUP_INDEX: usize = 9;

const SONG_FLAG_MAGES_BALLAD: u8 = 1 << 0;
const SONG_FLAG_ARMYS_PAEON: u8 = 1 << 1;
const SONG_FLAG_WANDERERS_MINUET: u8 = SONG_FLAG_MAGES_BALLAD | SONG_FLAG_ARMYS_PAEON;
const SONG_FLAG_MAGES_BALLAD_LAST_PLAYED: u8 = 1 << 2;
const SONG_FLAG_ARMYS_PAEON_LAST_PLAYED: u8 = 1 << 3;
const SONG_FLAG_WANDERERS_MINUET_LAST_PLAYED: u8 =
    SONG_FLAG_MAGES_BALLAD_LAST_PLAYED | SONG_FLAG_ARMYS_PAEON_LAST_PLAYED;
const SONG_FLAG_MAGES_BALLAD_CODA: u8 = 1 << 4;
const SONG_FLAG_ARMYS_PAEON_CODA: u8 = 1 << 5;
const SONG_FLAG_WANDERERS_MINUET_CODA: u8 = 1 << 6;
const SONG_FLAG_ACTIVE_MASK: u8 = SONG_FLAG_MAGES_BALLAD | SONG_FLAG_ARMYS_PAEON;
const SONG_FLAG_LAST_PLAYED_MASK: u8 =
    SONG_FLAG_MAGES_BALLAD_LAST_PLAYED | SONG_FLAG_ARMYS_PAEON_LAST_PLAYED;
const SONG_FLAG_CODA_MASK: u8 =
    SONG_FLAG_MAGES_BALLAD_CODA | SONG_FLAG_ARMYS_PAEON_CODA | SONG_FLAG_WANDERERS_MINUET_CODA;

const LEVEL_RADIANT_FINALE_CODA: u8 = 90;
/// Soul Voice gauge unlock level (Trait 287 / Apex Arrow action 16496, ClassJobLevel=80).
/// Gates every path that accumulates or emits the gauge.
const LEVEL_SOUL_VOICE_UNLOCK: u8 = 80;
const LEVEL_EMPYREAL_ARROW_REPERTOIRE: u8 = 68;

// ==================== Duration Constants ====================

const DOT_DURATION: Duration = Duration::from_secs(45);
const SONG_DURATION: Duration = Duration::from_secs(45);
const REPERTOIRE_TICK_INTERVAL: Duration = Duration::from_secs(3);
const MAGES_BALLAD_COOLDOWN_REDUCTION: Duration = Duration::from_millis(7500);
const RAGING_STRIKES_DURATION: Duration = Duration::from_secs(20);
const BATTLE_VOICE_DURATION: Duration = Duration::from_secs(15);
const RADIANT_FINALE_DURATION: Duration = Duration::from_secs(20);
const BARRAGE_DURATION: Duration = Duration::from_secs(10);
const TROUBADOUR_DURATION: Duration = Duration::from_secs(15);
const NATURES_MINNE_DURATION: Duration = Duration::from_secs(15);
const READY_STATUS_DURATION: Duration = Duration::from_secs(30);
const HEAVY_SHOT_HAWK_EYE_PROC_CHANCE_PERCENT: u8 = 20;
const BURST_SHOT_HAWK_EYE_PROC_CHANCE_PERCENT: u8 = 35;
const QUICK_NOCK_HAWK_EYE_PROC_CHANCE_PERCENT: u8 = 20;
const LADONSBITE_HAWK_EYE_PROC_CHANCE_PERCENT: u8 = 35;

/// Bard's current song
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BardSong {
    #[default]
    None,
    MagesBallad,
    ArmysPaeon,
    WanderersMinuet,
}

/// Bard job state tracked server-side
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BardState {
    /// Current Soul Voice gauge (0-100)
    pub soul_voice: u8,
    /// Current Repertoire stacks for Army's Paeon / Wanderer's Minuet.
    #[serde(default)]
    pub repertoire: u8,
    /// Currently active song
    pub current_song: BardSong,
    /// SongFlags byte consumed by the client BardGauge.
    #[serde(default)]
    pub song_flags: u8,
    /// When the current song expires
    #[serde(skip)]
    pub song_expires_at: Option<Instant>,
    /// Next 3-second song tick that can roll Repertoire while in combat.
    #[serde(skip)]
    pub next_repertoire_tick_at: Option<Instant>,
    /// Caustic Bite DoT expiration
    #[serde(skip)]
    pub caustic_bite_expires_at: Option<Instant>,
    /// Stormbite DoT expiration
    #[serde(skip)]
    pub stormbite_expires_at: Option<Instant>,
    /// Hawk Eye stacks (0-3) for Refulgent Arrow ready
    pub hawk_eye_stacks: u8,
    /// When Hawk Eye expires
    #[serde(skip)]
    pub hawk_eye_expires_at: Option<Instant>,
    /// Blast Arrow ready status
    pub blast_arrow_ready: bool,
    /// Resonant Arrow ready status
    pub resonant_arrow_ready: bool,
    /// Radiant Encore ready status
    pub radiant_encore_ready: bool,
    /// Damage bonus granted by the current Radiant Finale status: 2/4/6.
    #[serde(default)]
    pub radiant_finale_damage_bonus_percent: u8,
    /// Number of Coda consumed by the most recent Radiant Finale, captured at Finale press because
    /// the coda flags are cleared immediately. Read by Radiant Encore to scale its potency.
    #[serde(default)]
    pub radiant_encore_coda: u8,
    /// Barrage stacks remaining
    pub barrage_stacks: u8,
    /// When Barrage expires
    #[serde(skip)]
    pub barrage_expires_at: Option<Instant>,
    /// When Raging Strikes expires
    #[serde(skip)]
    pub raging_strikes_expires_at: Option<Instant>,
    /// When Battle Voice expires
    #[serde(skip)]
    pub battle_voice_expires_at: Option<Instant>,
    /// When Radiant Finale expires
    #[serde(skip)]
    pub radiant_finale_expires_at: Option<Instant>,
    /// When Troubadour expires
    #[serde(skip)]
    pub troubadour_expires_at: Option<Instant>,
    /// When Nature's Minne expires
    #[serde(skip)]
    pub natures_minne_expires_at: Option<Instant>,
}

impl BardState {
    /// Check if any DoT is active
    pub fn has_dot_active(&self) -> bool {
        self.caustic_bite_expires_at
            .is_some_and(|t| t > Instant::now())
            || self
                .stormbite_expires_at
                .is_some_and(|t| t > Instant::now())
    }

    /// Check if a song is currently active
    pub fn has_song_active(&self) -> bool {
        self.current_song != BardSong::None
            && self.song_expires_at.is_some_and(|t| t > Instant::now())
    }

    /// Check if Barrage is active
    pub fn has_barrage_active(&self) -> bool {
        self.barrage_stacks > 0 && self.barrage_expires_at.is_some_and(|t| t > Instant::now())
    }

    /// Get remaining song duration in milliseconds
    pub fn song_remaining_ms(&self) -> u16 {
        self.song_expires_at
            .map(|t| {
                t.saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u128::from(u16::MAX)) as u16
            })
            .unwrap_or(0)
    }
}

/// Maps a Bard action to the party-propagatable status it grants, if any.
///
/// Returns `Some((status_id, param, duration_secs))` for exactly the two in-scope raid buffs;
/// `None` for every other action (songs, Raging Strikes, Troubadour, Nature's Minne, Radiant
/// Encore, damage skills, ...).
///
/// `radiant_finale_bonus_percent` is the freshly-computed coda bonus
/// (`radiant_finale_damage_bonus_percent`, values 0/2/4/6) — it is only consumed for Radiant Finale
/// and encoded into that status's `param`. Battle Voice ignores it and returns param 0.
///
/// Durations come from the Rust duration constants so the caster-own and party copies cannot drift.
pub(crate) fn party_buff_for_action(
    action_id: u32,
    radiant_finale_bonus_percent: u8,
) -> Option<(u16, u16, f32)> {
    match action_id {
        ACTION_BATTLE_VOICE => Some((STATUS_BATTLE_VOICE, 0, BATTLE_VOICE_DURATION.as_secs_f32())),
        ACTION_RADIANT_FINALE => Some((
            STATUS_RADIANT_FINALE,
            radiant_finale_bonus_percent as u16,
            RADIANT_FINALE_DURATION.as_secs_f32(),
        )),
        _ => None,
    }
}

pub(crate) fn gauge_class_job_id() -> u8 {
    CLASSJOB_BARD
}

/// Refresh runtime state, clearing expired timers
fn refresh_bard_runtime_state(brd: &mut BardState) {
    let now = Instant::now();

    // Clear expired song
    if brd.song_expires_at.is_some_and(|t| t <= now) {
        brd.current_song = BardSong::None;
        brd.song_expires_at = None;
        brd.next_repertoire_tick_at = None;
        brd.repertoire = 0;
        brd.song_flags &= !SONG_FLAG_ACTIVE_MASK;
    }

    // Clear expired DoTs
    if brd.caustic_bite_expires_at.is_some_and(|t| t <= now) {
        brd.caustic_bite_expires_at = None;
    }
    if brd.stormbite_expires_at.is_some_and(|t| t <= now) {
        brd.stormbite_expires_at = None;
    }

    // Clear expired buffs
    if brd.barrage_expires_at.is_some_and(|t| t <= now) {
        brd.barrage_stacks = 0;
        brd.barrage_expires_at = None;
    }
    if brd.hawk_eye_expires_at.is_some_and(|t| t <= now) {
        brd.hawk_eye_stacks = 0;
        brd.hawk_eye_expires_at = None;
    }
    if brd.raging_strikes_expires_at.is_some_and(|t| t <= now) {
        brd.raging_strikes_expires_at = None;
    }
    if brd.battle_voice_expires_at.is_some_and(|t| t <= now) {
        brd.battle_voice_expires_at = None;
    }
    if brd.radiant_finale_expires_at.is_some_and(|t| t <= now) {
        brd.radiant_finale_expires_at = None;
        brd.radiant_finale_damage_bonus_percent = 0;
    }
    if brd.troubadour_expires_at.is_some_and(|t| t <= now) {
        brd.troubadour_expires_at = None;
    }
    if brd.natures_minne_expires_at.is_some_and(|t| t <= now) {
        brd.natures_minne_expires_at = None;
    }
}

fn song_active_flags(song: BardSong) -> u8 {
    match song {
        BardSong::None => 0,
        BardSong::MagesBallad => SONG_FLAG_MAGES_BALLAD,
        BardSong::ArmysPaeon => SONG_FLAG_ARMYS_PAEON,
        BardSong::WanderersMinuet => SONG_FLAG_WANDERERS_MINUET,
    }
}

fn song_last_played_flags(song: BardSong) -> u8 {
    match song {
        BardSong::None => 0,
        BardSong::MagesBallad => SONG_FLAG_MAGES_BALLAD_LAST_PLAYED,
        BardSong::ArmysPaeon => SONG_FLAG_ARMYS_PAEON_LAST_PLAYED,
        BardSong::WanderersMinuet => SONG_FLAG_WANDERERS_MINUET_LAST_PLAYED,
    }
}

fn song_coda_flags(song: BardSong) -> u8 {
    match song {
        BardSong::None => 0,
        BardSong::MagesBallad => SONG_FLAG_MAGES_BALLAD_CODA,
        BardSong::ArmysPaeon => SONG_FLAG_ARMYS_PAEON_CODA,
        BardSong::WanderersMinuet => SONG_FLAG_WANDERERS_MINUET_CODA,
    }
}

fn coda_count(song_flags: u8) -> u8 {
    (song_flags & SONG_FLAG_CODA_MASK).count_ones() as u8
}

/// Add to the Soul Voice gauge, gated on the level-80 unlock. Below it the client hides the
/// gauge widget and level-gates Apex Arrow, so the server must not accumulate the value.
fn add_soul_voice(brd: &mut BardState, level: u8, amount: u8) {
    if level < LEVEL_SOUL_VOICE_UNLOCK {
        return;
    }
    brd.soul_voice = (brd.soul_voice + amount).min(MAX_SOUL_VOICE);
}

fn start_song(brd: &mut BardState, song: BardSong, level: u8) {
    let now = Instant::now();
    brd.current_song = song;
    brd.song_expires_at = Some(now + SONG_DURATION);
    brd.next_repertoire_tick_at = Some(now + REPERTOIRE_TICK_INTERVAL);
    brd.repertoire = 0;
    brd.song_flags &= !(SONG_FLAG_ACTIVE_MASK | SONG_FLAG_LAST_PLAYED_MASK);
    brd.song_flags |= song_active_flags(song) | song_last_played_flags(song);
    if level >= LEVEL_RADIANT_FINALE_CODA {
        brd.song_flags |= song_coda_flags(song);
    }
    add_soul_voice(brd, level, 20);
}

fn take_due_repertoire_tick(brd: &mut BardState) -> bool {
    if !brd.has_song_active() {
        brd.next_repertoire_tick_at = None;
        return false;
    }

    let now = Instant::now();
    let Some(next_tick) = brd.next_repertoire_tick_at else {
        brd.next_repertoire_tick_at = Some(now + REPERTOIRE_TICK_INTERVAL);
        return false;
    };

    if next_tick > now {
        return false;
    }

    brd.next_repertoire_tick_at = Some(now + REPERTOIRE_TICK_INTERVAL);
    true
}

fn song_remaining_secs(brd: &BardState) -> f32 {
    brd.song_expires_at
        .map(|t| t.saturating_duration_since(Instant::now()).as_secs_f32())
        .unwrap_or(0.0)
}

#[derive(Debug, Default, Clone, Copy)]
struct BardRepertoireProc {
    changed: bool,
    reduce_bloodletter_cooldown: bool,
    status_param: Option<u16>,
}

fn grant_repertoire(brd: &mut BardState, level: u8) -> BardRepertoireProc {
    if !brd.has_song_active() {
        return BardRepertoireProc::default();
    }

    let before_soul_voice = brd.soul_voice;
    let before_repertoire = brd.repertoire;
    let mut reduce_bloodletter_cooldown = false;
    let status_param = match brd.current_song {
        BardSong::MagesBallad => {
            reduce_bloodletter_cooldown = true;
            Some(0)
        }
        BardSong::ArmysPaeon => {
            brd.repertoire = brd.repertoire.saturating_add(1).min(ARMYS_REPERTOIRE_MAX);
            Some(u16::from(brd.repertoire))
        }
        BardSong::WanderersMinuet => {
            brd.repertoire = brd
                .repertoire
                .saturating_add(1)
                .min(WANDERERS_REPERTOIRE_MAX);
            Some(u16::from(brd.repertoire))
        }
        BardSong::None => None,
    };

    if level >= LEVEL_SOUL_VOICE_UNLOCK {
        brd.soul_voice = brd
            .soul_voice
            .saturating_add(REPERTOIRE_SOUL_VOICE_GAIN)
            .min(MAX_SOUL_VOICE);
    }

    BardRepertoireProc {
        changed: brd.soul_voice != before_soul_voice
            || brd.repertoire != before_repertoire
            || reduce_bloodletter_cooldown,
        reduce_bloodletter_cooldown,
        status_param,
    }
}

fn add_repertoire_status(
    status_effects: &mut StatusEffects,
    brd: &BardState,
    owner_actor_id: ObjectId,
    status_param: u16,
) -> bool {
    let duration = song_remaining_secs(brd).max(0.1);
    status_effects.add_with_source(STATUS_REPERTOIRE, status_param, duration, owner_actor_id);
    true
}

fn remove_repertoire_status(status_effects: &mut StatusEffects) -> bool {
    if status_effects.get(STATUS_REPERTOIRE).is_none() {
        return false;
    }

    status_effects.remove(STATUS_REPERTOIRE);
    true
}

/// Ends whichever song status is currently up, so that starting a new song cannot leave two
/// song buffs on the player at once.
///
/// Only one song can be active at a time: `start_song` already overwrites the gauge state, but
/// the song *status* is applied by each song's Lua script and would otherwise linger until its
/// own 45s duration lapsed. Swapping songs a few seconds early — the normal rotation — showed
/// both buffs side by side until the old one expired.
///
/// The incoming song's own status is applied by the Lua script for that action, which runs
/// separately from this, so removing all three ids here is safe and keeps the caller from
/// having to know which song it is replacing.
fn remove_song_statuses(status_effects: &mut StatusEffects) -> bool {
    let mut removed = false;
    for status in SONG_STATUSES {
        if status_effects.get(status).is_some() {
            status_effects.remove(status);
            removed = true;
        }
    }
    removed
}

fn add_hawk_eye_status(
    status_effects: &mut StatusEffects,
    brd: &mut BardState,
    owner_actor_id: ObjectId,
    stacks: u8,
) -> bool {
    brd.hawk_eye_stacks = stacks;
    brd.hawk_eye_expires_at = Some(Instant::now() + READY_STATUS_DURATION);
    status_effects.add_with_source(
        STATUS_HAWK_EYE,
        0,
        READY_STATUS_DURATION.as_secs_f32(),
        owner_actor_id,
    );
    true
}

fn remove_hawk_eye_status(status_effects: &mut StatusEffects, brd: &mut BardState) -> bool {
    brd.hawk_eye_stacks = 0;
    brd.hawk_eye_expires_at = None;
    if status_effects.get(STATUS_HAWK_EYE).is_none() {
        return false;
    }

    status_effects.remove(STATUS_HAWK_EYE);
    true
}

fn remove_status_if_present(status_effects: &mut StatusEffects, status_id: u16) -> bool {
    if status_effects.get(status_id).is_none() {
        return false;
    }

    status_effects.remove(status_id);
    true
}

fn maybe_proc_hawk_eye(
    status_effects: &mut StatusEffects,
    brd: &mut BardState,
    owner_actor_id: ObjectId,
    chance_percent: u8,
) -> bool {
    if fastrand::u8(0..100) >= chance_percent {
        return false;
    }

    add_hawk_eye_status(status_effects, brd, owner_actor_id, 1)
}

fn apply_repertoire_proc(
    combat_state: &mut PlayerCombatState,
    status_effects: &mut StatusEffects,
    owner_actor_id: ObjectId,
    level: u8,
) -> BardActionUpdate {
    let proc = grant_repertoire(&mut combat_state.bard, level);
    let mut status_timer_refreshed = false;
    if let Some(status_param) = proc.status_param {
        status_timer_refreshed = add_repertoire_status(
            status_effects,
            &combat_state.bard,
            owner_actor_id,
            status_param,
        );
    }

    let cooldown_update = if proc.reduce_bloodletter_cooldown
        && combat_state
            .reduce_cooldown_recovery(
                BLOODLETTER_COOLDOWN_GROUP_INDEX,
                MAGES_BALLAD_COOLDOWN_REDUCTION,
            )
            .is_some()
    {
        Some(BardCooldownUpdate {
            cooldown_group: BLOODLETTER_COOLDOWN_GROUP_INDEX as u32,
            // Retail sends the reduction as a relative skew (7.5s → 750 centiseconds).
            delta_centisec: (MAGES_BALLAD_COOLDOWN_REDUCTION.as_millis() / 10) as u32,
        })
    } else {
        None
    };

    BardActionUpdate {
        changed: proc.changed || status_timer_refreshed || cooldown_update.is_some(),
        status_timer_refreshed,
        cooldown_update,
    }
}

fn visible_song_flags(brd: &BardState) -> u8 {
    let mut song_flags = brd.song_flags;
    song_flags &= !SONG_FLAG_ACTIVE_MASK;
    if brd.has_song_active() {
        song_flags |= song_active_flags(brd.current_song);
    }
    song_flags
}

/// Resolve Bard action remapping (skill morphing)
pub(crate) fn resolve_bard_action(
    request: &ActionRequest,
    combat_state: &PlayerCombatState,
    level: u8,
    _game_data: &mut GameData,
) -> u32 {
    let mut brd = combat_state.bard.clone();
    refresh_bard_runtime_state(&mut brd);

    match request.action_id {
        // Burst Shot → Refulgent Arrow when Hawk Eye is active (level 70+)
        ACTION_BURST_SHOT | ACTION_HEAVY_SHOT if brd.hawk_eye_stacks > 0 && level >= 70 => {
            tracing::debug!("Burst Shot -> Refulgent Arrow (Hawk Eye active)");
            ACTION_REFULGENT_ARROW
        }

        // Shadowbite requires Hawk Eye or Shadowbite Ready status
        ACTION_SHADOWBITE if brd.hawk_eye_stacks > 0 => {
            tracing::debug!("Shadowbite allowed (Hawk Eye active)");
            ACTION_SHADOWBITE
        }

        // Apex Arrow → Blast Arrow when Blast Arrow Ready
        ACTION_APEX_ARROW if brd.blast_arrow_ready => {
            tracing::debug!("Apex Arrow -> Blast Arrow (Blast Arrow Ready)");
            ACTION_BLAST_ARROW
        }

        // Barrage button becomes Resonant Arrow while the Dawntrail follow-up is ready.
        ACTION_BARRAGE if brd.resonant_arrow_ready && level >= 96 => ACTION_RESONANT_ARROW,

        // Radiant Finale button becomes Radiant Encore while the follow-up is ready.
        ACTION_RADIANT_FINALE if brd.radiant_encore_ready && level >= 100 => ACTION_RADIANT_ENCORE,

        // Default: no remapping
        _ => request.action_id,
    }
}

/// Check if the Bard can execute the given action
pub(crate) fn can_execute_bard_action(
    action_id: u32,
    combat_state: &PlayerCombatState,
    level: u8,
) -> bool {
    let mut brd = combat_state.bard.clone();
    refresh_bard_runtime_state(&mut brd);

    match action_id {
        // Songs require level check
        ACTION_MAGES_BALLAD => level >= 30,
        ACTION_ARMYS_PAEON => level >= 40,
        ACTION_WANDERERS_MINUET => level >= 52,

        // Pitch Perfect requires Wanderer's Minuet active
        ACTION_PITCH_PERFECT => {
            brd.current_song == BardSong::WanderersMinuet
                && brd.has_song_active()
                && brd.repertoire > 0
        }

        // Apex Arrow requires Soul Voice >= 20 (80 is only the threshold to grant Blast Arrow Ready,
        // not to use Apex Arrow itself).
        ACTION_APEX_ARROW => brd.soul_voice >= 20 && level >= 80,

        // Blast Arrow requires Blast Arrow Ready
        ACTION_BLAST_ARROW => brd.blast_arrow_ready,

        // Iron Jaws requires at least one DoT active
        ACTION_IRON_JAWS => brd.has_dot_active(),

        // Straight Shot requires a Hawk's Eye proc. Barrage opens this gate too, but only via the
        // Hawk's Eye stacks it grants (see the Barrage arm) — which the consume arm zeroes after one
        // cast, so a single shot is enabled per proc rather than the whole Barrage window.
        ACTION_STRAIGHT_SHOT => brd.hawk_eye_stacks > 0,

        // Wide Volley is gated identically: a Hawk's Eye proc (including the stacks Barrage grants).
        ACTION_WIDE_VOLLEY => brd.hawk_eye_stacks > 0,

        // Heartbreak Shot requires level 92
        ACTION_HEARTBREAK_SHOT => level >= 92,

        // Resonant Arrow requires Resonant Arrow Ready
        ACTION_RESONANT_ARROW => brd.resonant_arrow_ready,

        // Radiant Encore requires Radiant Encore Ready
        ACTION_RADIANT_ENCORE => brd.radiant_encore_ready,

        // Default: allow execution
        _ => true,
    }
}

/// Update Bard state after an action is executed
pub(crate) fn update_bard_state_after_action(
    action_id: u32,
    actor: &mut NetworkedActor,
    owner_actor_id: ObjectId,
) -> BardActionUpdate {
    let level = actor.get_common_spawn().level;
    let NetworkedActor::Player {
        combat_state,
        status_effects,
        ..
    } = actor
    else {
        return BardActionUpdate::default();
    };

    refresh_bard_runtime_state(&mut combat_state.bard);
    let mut action_update = BardActionUpdate::default();

    match action_id {
        // Song activations
        ACTION_MAGES_BALLAD => {
            start_song(&mut combat_state.bard, BardSong::MagesBallad, level);
            action_update.status_timer_refreshed = remove_repertoire_status(status_effects)
                | remove_song_statuses(status_effects);
        }
        ACTION_ARMYS_PAEON => {
            start_song(&mut combat_state.bard, BardSong::ArmysPaeon, level);
            action_update.status_timer_refreshed = remove_repertoire_status(status_effects)
                | remove_song_statuses(status_effects);
        }
        ACTION_WANDERERS_MINUET => {
            start_song(&mut combat_state.bard, BardSong::WanderersMinuet, level);
            action_update.status_timer_refreshed = remove_repertoire_status(status_effects)
                | remove_song_statuses(status_effects);
        }

        // DoT applications
        ACTION_CAUSTIC_BITE => {
            combat_state.bard.caustic_bite_expires_at = Some(Instant::now() + DOT_DURATION);
        }
        ACTION_STORMBITE => {
            combat_state.bard.stormbite_expires_at = Some(Instant::now() + DOT_DURATION);
        }
        // Venomous Bite is the pre-Lv64 predecessor of Caustic Bite: the field doubles as the
        // pre-Lv64 DoT slot (Venomous/Caustic are mutually exclusive by level).
        ACTION_VENOMOUS_BITE => {
            combat_state.bard.caustic_bite_expires_at = Some(Instant::now() + DOT_DURATION);
        }
        // Windbite is the pre-Lv64 predecessor of Stormbite: the field doubles as the pre-Lv64
        // DoT slot (Windbite/Stormbite are mutually exclusive by level).
        ACTION_WINDBITE => {
            combat_state.bard.stormbite_expires_at = Some(Instant::now() + DOT_DURATION);
        }

        // Iron Jaws refreshes both DoTs
        ACTION_IRON_JAWS => {
            if combat_state.bard.caustic_bite_expires_at.is_some() {
                combat_state.bard.caustic_bite_expires_at = Some(Instant::now() + DOT_DURATION);
            }
            if combat_state.bard.stormbite_expires_at.is_some() {
                combat_state.bard.stormbite_expires_at = Some(Instant::now() + DOT_DURATION);
            }
            add_soul_voice(&mut combat_state.bard, level, 10);
        }

        // Apex Arrow consumes Soul Voice
        ACTION_APEX_ARROW => {
            combat_state.bard.soul_voice = 0;
            if level >= 86 {
                combat_state.bard.blast_arrow_ready = true;
            }
        }

        // Blast Arrow consumes ready status
        ACTION_BLAST_ARROW => {
            combat_state.bard.blast_arrow_ready = false;
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_BLAST_ARROW_READY);
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_BLAST_ARROW_READY_2);
        }

        // GCD weaponskills add Soul Voice and can grant Hawk Eye.
        ACTION_HEAVY_SHOT => {
            add_soul_voice(&mut combat_state.bard, level, 5);
            action_update.status_timer_refreshed |= maybe_proc_hawk_eye(
                status_effects,
                &mut combat_state.bard,
                owner_actor_id,
                HEAVY_SHOT_HAWK_EYE_PROC_CHANCE_PERCENT,
            );
        }
        ACTION_BURST_SHOT => {
            add_soul_voice(&mut combat_state.bard, level, 5);
            action_update.status_timer_refreshed |= maybe_proc_hawk_eye(
                status_effects,
                &mut combat_state.bard,
                owner_actor_id,
                BURST_SHOT_HAWK_EYE_PROC_CHANCE_PERCENT,
            );
        }
        // AoE weaponskills can grant Hawk's Eye but do NOT grant Soul Voice.
        ACTION_QUICK_NOCK => {
            action_update.status_timer_refreshed |= maybe_proc_hawk_eye(
                status_effects,
                &mut combat_state.bard,
                owner_actor_id,
                QUICK_NOCK_HAWK_EYE_PROC_CHANCE_PERCENT,
            );
        }
        ACTION_LADONSBITE => {
            action_update.status_timer_refreshed |= maybe_proc_hawk_eye(
                status_effects,
                &mut combat_state.bard,
                owner_actor_id,
                LADONSBITE_HAWK_EYE_PROC_CHANCE_PERCENT,
            );
        }
        // Wide Volley and Straight Shot consume the Hawk's Eye proc the Lua just used, clearing
        // it from Rust state so the Burst→Refulgent morph / Straight Shot gate don't see a stale proc.
        ACTION_WIDE_VOLLEY | ACTION_STRAIGHT_SHOT => {
            action_update.status_timer_refreshed |=
                remove_hawk_eye_status(status_effects, &mut combat_state.bard);
        }
        ACTION_REFULGENT_ARROW => {
            add_soul_voice(&mut combat_state.bard, level, 10);
            action_update.status_timer_refreshed |=
                remove_hawk_eye_status(status_effects, &mut combat_state.bard);
            combat_state.bard.resonant_arrow_ready = false;
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_RESONANT_ARROW_READY);
        }
        ACTION_SHADOWBITE => {
            add_soul_voice(&mut combat_state.bard, level, 10);
            action_update.status_timer_refreshed |=
                remove_hawk_eye_status(status_effects, &mut combat_state.bard);
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_SHADOWBITE_READY);
        }

        ACTION_EMPYREAL_ARROW => {
            if level >= LEVEL_EMPYREAL_ARROW_REPERTOIRE && combat_state.bard.has_song_active() {
                action_update =
                    apply_repertoire_proc(combat_state, status_effects, owner_actor_id, level);
            }
        }

        // Buff activations
        ACTION_RAGING_STRIKES => {
            combat_state.bard.raging_strikes_expires_at =
                Some(Instant::now() + RAGING_STRIKES_DURATION);
        }
        ACTION_BATTLE_VOICE => {
            combat_state.bard.battle_voice_expires_at =
                Some(Instant::now() + BATTLE_VOICE_DURATION);
        }
        ACTION_RADIANT_FINALE => {
            let coda = coda_count(combat_state.bard.song_flags);
            combat_state.bard.radiant_encore_coda = coda;
            combat_state.bard.radiant_finale_damage_bonus_percent = match coda {
                0 => 0,
                1 => 2,
                2 => 4,
                _ => 6,
            };
            combat_state.bard.radiant_finale_expires_at =
                Some(Instant::now() + RADIANT_FINALE_DURATION);
            combat_state.bard.song_flags &= !SONG_FLAG_CODA_MASK;
            if level >= 100 {
                combat_state.bard.radiant_encore_ready = true;
            }
        }
        ACTION_BARRAGE => {
            combat_state.bard.barrage_stacks = 3;
            combat_state.bard.barrage_expires_at = Some(Instant::now() + BARRAGE_DURATION);
            action_update.status_timer_refreshed |=
                add_hawk_eye_status(status_effects, &mut combat_state.bard, owner_actor_id, 3);
            if level >= 96 {
                combat_state.bard.resonant_arrow_ready = true;
            }
        }
        ACTION_TROUBADOUR => {
            combat_state.bard.troubadour_expires_at = Some(Instant::now() + TROUBADOUR_DURATION);
        }
        ACTION_NATURES_MINNE => {
            combat_state.bard.natures_minne_expires_at =
                Some(Instant::now() + NATURES_MINNE_DURATION);
        }

        // Consume ready statuses
        ACTION_PITCH_PERFECT => {
            combat_state.bard.repertoire = 0;
            action_update.status_timer_refreshed = remove_repertoire_status(status_effects);
        }
        ACTION_RESONANT_ARROW => {
            combat_state.bard.resonant_arrow_ready = false;
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_RESONANT_ARROW_READY);
        }
        ACTION_RADIANT_ENCORE => {
            combat_state.bard.radiant_encore_ready = false;
            action_update.status_timer_refreshed |=
                remove_status_if_present(status_effects, STATUS_RADIANT_ENCORE_READY);
        }

        _ => {}
    }

    action_update
}

/// Build gauge data to send to the client.
///
/// The ActorGauge packet carries the 8-byte class-specific tail of the client's BardGauge:
/// bytes 0-1 SongTimer | bytes 2-3 unused | byte 4 Repertoire | byte 5 SoulVoice |
/// byte 6 RadiantFinaleCoda | byte 7 SongFlags.
///
/// Level-sync masking (send-side only; stored state is left intact so un-sync restores it):
/// - Soul Voice (Trait 287) below 80 → byte 5 = 0
/// - Minstrel's Coda (Trait 448) below 90 → clear coda bits in SongFlags and byte 6 = 0
pub(crate) fn build_bard_gauge_data(combat_state: &PlayerCombatState, level: u8) -> u64 {
    let mut brd = combat_state.bard.clone();
    refresh_bard_runtime_state(&mut brd);

    let song_remaining = brd.song_remaining_ms();
    // Mask the gauge below the level-80 unlock so a carried-over (e.g. level-100, synced-down)
    // value is not emitted while the client hides the widget and level-gates Apex Arrow.
    let soul_voice = if level >= LEVEL_SOUL_VOICE_UNLOCK {
        brd.soul_voice
    } else {
        0
    };
    let repertoire = match brd.current_song {
        BardSong::ArmysPaeon if brd.has_song_active() => brd.repertoire.min(ARMYS_REPERTOIRE_MAX),
        BardSong::WanderersMinuet if brd.has_song_active() => {
            brd.repertoire.min(WANDERERS_REPERTOIRE_MAX)
        }
        _ => 0,
    };
    let mut song_flags = visible_song_flags(&brd);
    // Trait 448 Minstrel's Coda (L90): do not emit coda bits under level sync.
    if level < LEVEL_RADIANT_FINALE_CODA {
        song_flags &= !SONG_FLAG_CODA_MASK;
    }
    let radiant_finale_coda = coda_count(song_flags);

    let data = (song_remaining as u64)
        | ((repertoire as u64) << 32)
        | ((soul_voice as u64) << 40)
        | ((radiant_finale_coda as u64) << 48)
        | ((song_flags as u64) << 56);
    tracing::debug!(
        song_remaining,
        repertoire,
        soul_voice,
        radiant_finale_coda,
        song_flags,
        data,
        "Built Bard gauge"
    );
    data
}

/// Apply gauge actions from Lua scripts
pub(crate) fn apply_bard_gauge_action(
    combat_state: &mut PlayerCombatState,
    index: u8,
    amount: i32,
) {
    let brd = &mut combat_state.bard;
    match index {
        BARD_GAUGE_SOUL_VOICE => {
            let new_value = (brd.soul_voice as i32 + amount).clamp(0, MAX_SOUL_VOICE as i32);
            brd.soul_voice = new_value as u8;
        }
        _ => {}
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BardCooldownUpdate {
    pub cooldown_group: u32,
    /// Relative cooldown reduction in centiseconds, sent to the client as ActorControl
    /// category 1537 (IncrementRecast / SkewCooldownForGroup). The client does
    /// `Elapsed += delta/100` on the recast group (clamped to Total, no-op if not running),
    /// which advances the shared charge pool — matching retail. Server-side charge bookkeeping
    /// is handled separately by `reduce_cooldown_recovery`.
    pub delta_centisec: u32,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BardActionUpdate {
    pub changed: bool,
    pub status_timer_refreshed: bool,
    pub cooldown_update: Option<BardCooldownUpdate>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct BardRefreshResult {
    pub changed: bool,
    pub status_timer_refreshed: bool,
    pub cooldown_update: Option<BardCooldownUpdate>,
}

/// Update Bard runtime state on actor refresh.
pub(crate) fn refresh_bard_runtime_state_on_actor(
    owner_actor_id: ObjectId,
    actor: &mut NetworkedActor,
) -> BardRefreshResult {
    let level = actor.get_common_spawn().level;
    let NetworkedActor::Player {
        combat_state,
        status_effects,
        ..
    } = actor
    else {
        return BardRefreshResult::default();
    };

    let before = combat_state.bard.clone();
    refresh_bard_runtime_state(&mut combat_state.bard);
    let mut result = BardRefreshResult {
        changed: combat_state.bard != before,
        ..Default::default()
    };

    if before.hawk_eye_stacks > 0 && combat_state.bard.hawk_eye_stacks == 0 {
        result.status_timer_refreshed |= remove_status_if_present(status_effects, STATUS_HAWK_EYE);
        result.changed |= result.status_timer_refreshed;
    }

    if !combat_state.bard.has_song_active() {
        result.status_timer_refreshed |= remove_repertoire_status(status_effects);
        result.changed |= result.status_timer_refreshed;
        return result;
    }

    let tick_due = take_due_repertoire_tick(&mut combat_state.bard);
    if tick_due && combat_state.in_combat && fastrand::u8(0..100) < REPERTOIRE_PROC_CHANCE_PERCENT {
        let proc_update =
            apply_repertoire_proc(combat_state, status_effects, owner_actor_id, level);
        result.changed |= proc_update.changed;
        result.status_timer_refreshed |= proc_update.status_timer_refreshed;
        result.cooldown_update = proc_update.cooldown_update;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zone_connection::{BaseParameters, TeleportQuery};
    use kawari::common::DistanceRange;
    use kawari::ipc::zone::{Conditions, SpawnPlayer};
    use std::collections::HashMap;

    /// Build a minimal `NetworkedActor::Player` (all-default, level 0) for exercising
    /// `update_bard_state_after_action`. Only `combat_state` / `status_effects` matter here.
    fn make_player_actor() -> NetworkedActor {
        NetworkedActor::Player {
            spawn: SpawnPlayer::default(),
            status_effects: StatusEffects::default(),
            teleport_query: TeleportQuery::default(),
            distance_range: DistanceRange::Normal,
            conditions: Conditions::default(),
            executing_gimmick_jump: false,
            inside_instance_exit: false,
            parameters: BaseParameters::default(),
            dueling_opponent_id: ObjectId::default(),
            remove_cooldowns: false,
            combat_state: PlayerCombatState::default(),
            last_combo_action: 0,
            combo_sequence: 0,
            hated_by: HashMap::new(),
            last_enmity_sent: Vec::new(),
        }
    }

    fn bard_of(actor: &NetworkedActor) -> &BardState {
        match actor {
            NetworkedActor::Player { combat_state, .. } => &combat_state.bard,
            _ => unreachable!("expected a Player actor"),
        }
    }

    fn combat_state_of(actor: &NetworkedActor) -> &PlayerCombatState {
        match actor {
            NetworkedActor::Player { combat_state, .. } => combat_state,
            _ => unreachable!("expected a Player actor"),
        }
    }

    fn status_effects_of(actor: &NetworkedActor) -> &StatusEffects {
        match actor {
            NetworkedActor::Player { status_effects, .. } => status_effects,
            _ => unreachable!("expected a Player actor"),
        }
    }

    #[test]
    fn maybe_proc_hawk_eye_never_procs_at_zero_chance() {
        let mut status_effects = StatusEffects::default();
        let mut brd = BardState::default();
        let procced = maybe_proc_hawk_eye(&mut status_effects, &mut brd, ObjectId::default(), 0);
        assert!(!procced);
        assert_eq!(brd.hawk_eye_stacks, 0);
        assert!(status_effects.get(STATUS_HAWK_EYE).is_none());
    }

    #[test]
    fn maybe_proc_hawk_eye_always_procs_at_full_chance() {
        let mut status_effects = StatusEffects::default();
        let mut brd = BardState::default();
        let procced = maybe_proc_hawk_eye(&mut status_effects, &mut brd, ObjectId::default(), 100);
        assert!(procced);
        assert_eq!(brd.hawk_eye_stacks, 1);
        assert!(status_effects.get(STATUS_HAWK_EYE).is_some());
    }

    #[test]
    fn straight_shot_gated_without_proc_or_barrage() {
        let combat_state = PlayerCombatState::default();
        assert!(!can_execute_bard_action(
            ACTION_STRAIGHT_SHOT,
            &combat_state,
            90
        ));
    }

    #[test]
    fn straight_shot_allowed_with_hawk_eye() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.hawk_eye_stacks = 1;
        combat_state.bard.hawk_eye_expires_at = Some(Instant::now() + READY_STATUS_DURATION);
        assert!(can_execute_bard_action(
            ACTION_STRAIGHT_SHOT,
            &combat_state,
            90
        ));
    }

    #[test]
    fn wide_volley_gated_without_proc_or_barrage() {
        let combat_state = PlayerCombatState::default();
        assert!(!can_execute_bard_action(
            ACTION_WIDE_VOLLEY,
            &combat_state,
            90
        ));
    }

    #[test]
    fn wide_volley_allowed_with_hawk_eye() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.hawk_eye_stacks = 1;
        combat_state.bard.hawk_eye_expires_at = Some(Instant::now() + READY_STATUS_DURATION);
        assert!(can_execute_bard_action(
            ACTION_WIDE_VOLLEY,
            &combat_state,
            90
        ));
    }

    /// Swapping songs must end the outgoing song's status immediately.
    ///
    /// Songs are swapped a few seconds early in a normal rotation. The status is applied by each
    /// song's Lua script and used to expire purely on its own 45s duration, which left both song
    /// buffs on the player until the old one lapsed.
    #[test]
    fn starting_a_song_ends_the_previous_song_status() {
        let mut actor = make_player_actor();

        // Stand in for the Lua-applied status of the song that is already running.
        match &mut actor {
            NetworkedActor::Player { status_effects, .. } => status_effects.add_with_source(
                STATUS_MAGES_BALLAD,
                0,
                45.0,
                ObjectId::default(),
            ),
            _ => unreachable!("expected a Player actor"),
        }

        let update =
            update_bard_state_after_action(ACTION_WANDERERS_MINUET, &mut actor, ObjectId::default());

        assert!(
            status_effects_of(&actor).get(STATUS_MAGES_BALLAD).is_none(),
            "the replaced song's status must be gone"
        );
        assert!(
            update.status_timer_refreshed,
            "the removal has to be flushed to the client"
        );
        assert_eq!(
            bard_of(&actor).current_song,
            BardSong::WanderersMinuet,
            "the gauge must track the new song"
        );
    }

    /// Barrage opens the gate (it grants Hawk's Eye stacks), but the consume arm zeroes those
    /// stacks, so exactly ONE Straight Shot is enabled per Barrage — not the whole ~10s window.
    #[test]
    fn barrage_enables_exactly_one_straight_shot() {
        let mut actor = make_player_actor();
        update_bard_state_after_action(ACTION_BARRAGE, &mut actor, ObjectId::default());
        assert!(can_execute_bard_action(
            ACTION_STRAIGHT_SHOT,
            combat_state_of(&actor),
            90
        ));

        // First Straight Shot consumes the proc granted by Barrage.
        update_bard_state_after_action(ACTION_STRAIGHT_SHOT, &mut actor, ObjectId::default());
        assert!(!can_execute_bard_action(
            ACTION_STRAIGHT_SHOT,
            combat_state_of(&actor),
            90
        ));
    }

    /// Same one-shot-per-Barrage invariant for Wide Volley.
    #[test]
    fn barrage_enables_exactly_one_wide_volley() {
        let mut actor = make_player_actor();
        update_bard_state_after_action(ACTION_BARRAGE, &mut actor, ObjectId::default());
        assert!(can_execute_bard_action(
            ACTION_WIDE_VOLLEY,
            combat_state_of(&actor),
            90
        ));

        update_bard_state_after_action(ACTION_WIDE_VOLLEY, &mut actor, ObjectId::default());
        assert!(!can_execute_bard_action(
            ACTION_WIDE_VOLLEY,
            combat_state_of(&actor),
            90
        ));
    }

    #[test]
    fn venomous_bite_arm_activates_dot_and_iron_jaws_gate() {
        let mut actor = make_player_actor();
        update_bard_state_after_action(ACTION_VENOMOUS_BITE, &mut actor, ObjectId::default());
        assert!(bard_of(&actor).has_dot_active());
        assert!(can_execute_bard_action(
            ACTION_IRON_JAWS,
            combat_state_of(&actor),
            60
        ));
    }

    #[test]
    fn windbite_arm_activates_dot_and_iron_jaws_gate() {
        let mut actor = make_player_actor();
        update_bard_state_after_action(ACTION_WINDBITE, &mut actor, ObjectId::default());
        assert!(bard_of(&actor).has_dot_active());
        assert!(can_execute_bard_action(
            ACTION_IRON_JAWS,
            combat_state_of(&actor),
            60
        ));
    }

    #[test]
    fn wide_volley_arm_clears_hawk_eye_proc() {
        let mut actor = make_player_actor();
        if let NetworkedActor::Player {
            combat_state,
            status_effects,
            ..
        } = &mut actor
        {
            add_hawk_eye_status(
                status_effects,
                &mut combat_state.bard,
                ObjectId::default(),
                1,
            );
        }
        assert_eq!(bard_of(&actor).hawk_eye_stacks, 1);

        update_bard_state_after_action(ACTION_WIDE_VOLLEY, &mut actor, ObjectId::default());
        assert_eq!(bard_of(&actor).hawk_eye_stacks, 0);
        assert!(status_effects_of(&actor).get(STATUS_HAWK_EYE).is_none());
    }

    #[test]
    fn straight_shot_arm_clears_hawk_eye_proc() {
        let mut actor = make_player_actor();
        if let NetworkedActor::Player {
            combat_state,
            status_effects,
            ..
        } = &mut actor
        {
            add_hawk_eye_status(
                status_effects,
                &mut combat_state.bard,
                ObjectId::default(),
                1,
            );
        }
        assert_eq!(bard_of(&actor).hawk_eye_stacks, 1);

        update_bard_state_after_action(ACTION_STRAIGHT_SHOT, &mut actor, ObjectId::default());
        assert_eq!(bard_of(&actor).hawk_eye_stacks, 0);
        assert!(status_effects_of(&actor).get(STATUS_HAWK_EYE).is_none());
    }

    /// Fix A: below the level-80 Soul Voice unlock, weaponskills must not accumulate the gauge.
    #[test]
    fn heavy_shot_grants_no_soul_voice_below_unlock() {
        let mut actor = make_player_actor();
        actor.get_common_spawn_mut().level = 54;
        update_bard_state_after_action(ACTION_HEAVY_SHOT, &mut actor, ObjectId::default());
        assert_eq!(bard_of(&actor).soul_voice, 0);
    }

    /// Fix A: at/above the unlock, Heavy Shot grants the usual +5.
    #[test]
    fn heavy_shot_grants_soul_voice_at_unlock() {
        let mut actor = make_player_actor();
        actor.get_common_spawn_mut().level = 80;
        update_bard_state_after_action(ACTION_HEAVY_SHOT, &mut actor, ObjectId::default());
        assert_eq!(bard_of(&actor).soul_voice, 5);
    }

    /// Fix B: a carried-over full gauge is masked to 0 in the serialized packet below the unlock.
    #[test]
    fn gauge_masks_soul_voice_below_unlock() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.soul_voice = 100;
        let data = build_bard_gauge_data(&combat_state, 54);
        let soul_voice_byte = (data >> 40) as u8;
        assert_eq!(soul_voice_byte, 0);
    }

    /// Fix B: at/above the unlock, the real gauge value is emitted unchanged.
    #[test]
    fn gauge_emits_soul_voice_at_unlock() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.soul_voice = 100;
        let data = build_bard_gauge_data(&combat_state, 80);
        let soul_voice_byte = (data >> 40) as u8;
        assert_eq!(soul_voice_byte, 100);
    }

    /// Trait 448 Minstrel's Coda (L90): carried-over coda bits are masked under sync.
    #[test]
    fn gauge_masks_coda_below_minstrels_coda() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.song_flags = SONG_FLAG_MAGES_BALLAD_CODA
            | SONG_FLAG_ARMYS_PAEON_CODA
            | SONG_FLAG_WANDERERS_MINUET_CODA;
        let data = build_bard_gauge_data(&combat_state, 54);
        let coda_byte = (data >> 48) as u8;
        let song_flags = (data >> 56) as u8;
        assert_eq!(coda_byte, 0);
        assert_eq!(song_flags & SONG_FLAG_CODA_MASK, 0);
        // Stored state is not wiped — only the packet is masked.
        assert_ne!(
            combat_state.bard.song_flags & SONG_FLAG_CODA_MASK,
            0,
            "storage must keep coda across sync"
        );
    }

    /// At/above L90, coda bits are emitted.
    #[test]
    fn gauge_emits_coda_at_minstrels_coda() {
        let mut combat_state = PlayerCombatState::default();
        combat_state.bard.song_flags = SONG_FLAG_MAGES_BALLAD_CODA
            | SONG_FLAG_ARMYS_PAEON_CODA
            | SONG_FLAG_WANDERERS_MINUET_CODA;
        let data = build_bard_gauge_data(&combat_state, 90);
        let coda_byte = (data >> 48) as u8;
        let song_flags = (data >> 56) as u8;
        assert_eq!(coda_byte, 3);
        assert_eq!(song_flags & SONG_FLAG_CODA_MASK, SONG_FLAG_CODA_MASK);
    }

    #[test]
    fn battle_voice_is_a_party_buff_with_no_param() {
        assert_eq!(
            party_buff_for_action(ACTION_BATTLE_VOICE, 0),
            Some((141, 0, 15.0))
        );
        // Battle Voice ignores the coda bonus entirely.
        assert_eq!(
            party_buff_for_action(ACTION_BATTLE_VOICE, 6),
            Some((141, 0, 15.0))
        );
    }

    #[test]
    fn radiant_finale_carries_the_coda_bonus_in_param() {
        assert_eq!(
            party_buff_for_action(ACTION_RADIANT_FINALE, 6),
            Some((2964, 6, 20.0))
        );
        assert_eq!(
            party_buff_for_action(ACTION_RADIANT_FINALE, 4),
            Some((2964, 4, 20.0))
        );
        assert_eq!(
            party_buff_for_action(ACTION_RADIANT_FINALE, 2),
            Some((2964, 2, 20.0))
        );
        assert_eq!(
            party_buff_for_action(ACTION_RADIANT_FINALE, 0),
            Some((2964, 0, 20.0))
        );
    }

    #[test]
    fn out_of_scope_actions_are_not_party_buffs() {
        // Songs, Raging Strikes, Troubadour, Nature's Minne, Radiant Encore, and damage skills
        // must all classify as None — only Battle Voice and Radiant Finale propagate.
        for action_id in [
            ACTION_WANDERERS_MINUET,
            ACTION_MAGES_BALLAD,
            ACTION_ARMYS_PAEON,
            ACTION_RAGING_STRIKES,
            ACTION_TROUBADOUR,
            ACTION_NATURES_MINNE,
            ACTION_RADIANT_ENCORE,
            ACTION_HEAVY_SHOT,
        ] {
            assert_eq!(
                party_buff_for_action(action_id, 6),
                None,
                "action {action_id}"
            );
        }
    }
}

/// Bard job dispatch handle. Zero-sized; all state lives in `PlayerCombatState::bard`. Every method
/// delegates to the existing free fn above so dispatch introduces no behavior change.
pub(in crate::server) struct Bard;

impl From<BardCooldownUpdate> for JobCooldownUpdate {
    fn from(value: BardCooldownUpdate) -> Self {
        JobCooldownUpdate {
            cooldown_group: value.cooldown_group,
            delta_centisec: value.delta_centisec,
        }
    }
}

impl Job for Bard {
    fn class_jobs(&self) -> &'static [u8] {
        &[CLASSJOB_BARD, CLASSJOB_CATEGORY_BARD]
    }

    fn gauge_class_job_id(&self, _dispatch_class_job: u8) -> u8 {
        gauge_class_job_id()
    }

    fn resolve_action(
        &self,
        request: &ActionRequest,
        combat_state: &PlayerCombatState,
        level: u8,
        game_data: &mut GameData,
    ) -> u32 {
        resolve_bard_action(request, combat_state, level, game_data)
    }

    fn can_execute(&self, action_id: u32, combat_state: &PlayerCombatState, level: u8) -> bool {
        can_execute_bard_action(action_id, combat_state, level)
    }

    fn apply_gauge_action(&self, combat_state: &mut PlayerCombatState, action: &GaugeAction) {
        apply_bard_gauge_action(combat_state, action.index, action.amount);
    }

    fn update_state_after_action(
        &self,
        action_id: u32,
        actor: &mut NetworkedActor,
        owner_actor_id: ObjectId,
    ) -> JobActionUpdate {
        let update = update_bard_state_after_action(action_id, actor, owner_actor_id);
        JobActionUpdate {
            changed: update.changed,
            status_timer_refreshed: update.status_timer_refreshed,
            cooldown_update: update.cooldown_update.map(JobCooldownUpdate::from),
        }
    }

    fn build_gauge_data(&self, combat_state: &PlayerCombatState, level: u8) -> Option<u64> {
        Some(build_bard_gauge_data(combat_state, level))
    }

    fn party_buff_for_action(
        &self,
        action_id: u32,
        radiant_finale_bonus_percent: u8,
    ) -> Option<(u16, u16, f32)> {
        party_buff_for_action(action_id, radiant_finale_bonus_percent)
    }

    fn refresh_runtime_state_on_actor(
        &self,
        owner_actor_id: ObjectId,
        actor: &mut NetworkedActor,
    ) -> JobRefreshResult {
        let result = refresh_bard_runtime_state_on_actor(owner_actor_id, actor);
        JobRefreshResult {
            changed: result.changed,
            status_timer_refreshed: result.status_timer_refreshed,
            demi_just_ended: false,
            cooldown_update: result.cooldown_update.map(JobCooldownUpdate::from),
        }
    }
}
