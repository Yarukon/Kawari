//! Job dispatch scaffolding: a `Job` lifecycle trait, a separate `JobActors` capability seam for
//! job-owned persistent actors/VFX/tasks, and a `job_for` registry that resolves a `class_job` id to
//! a `&'static dyn Job`.
//!
//! Each trait method is a thin delegating wrapper over an existing per-job free fn (see `bard.rs` /
//! `summoner.rs`), so routing a call site through the trait produces byte-identical behavior. Job
//! impls are zero-sized unit structs (`struct Bard;` / `struct Summoner;`) — all mutable state
//! already lives in `PlayerCombatState`.

use std::sync::Arc;

use parking_lot::Mutex;

use crate::FromServer;
use crate::gamedata::GameData;
use crate::lua::GaugeAction;
use crate::server::actor::NetworkedActor;
use crate::server::combat_state::PlayerCombatState;
use crate::server::instance::Instance;
use crate::server::jobs::bard::Bard;
use crate::server::jobs::summoner::Summoner;
use crate::server::network::{DestinationNetwork, NetworkState};
use kawari::common::ObjectId;
use kawari::ipc::zone::{ActionRequest, ServerZoneIpcData, ServerZoneIpcSegment, TargetEffect};

/// Unified per-action state-update result. Superset of the two existing job structs; jobs that don't
/// use a field leave it at `Default`.
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::server) struct JobActionUpdate {
    /// Mirror `JobRefreshResult`; populated for job-wiring symmetry, not yet read on the action path.
    #[allow(dead_code)]
    pub changed: bool,
    /// Mirror `JobRefreshResult`; populated for job-wiring symmetry, not yet read on the action path.
    #[allow(dead_code)]
    pub status_timer_refreshed: bool,
    pub cooldown_update: Option<JobCooldownUpdate>,
}

/// Unified runtime-refresh result (superset of Bard/Summoner refresh results).
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::server) struct JobRefreshResult {
    pub changed: bool,
    pub status_timer_refreshed: bool,
    /// Summoner-only; `false` for every other job.
    pub demi_just_ended: bool,
    pub cooldown_update: Option<JobCooldownUpdate>,
}

/// Neutral cooldown-update shape (field-identical to `bard::BardCooldownUpdate`).
#[derive(Debug, Default, Clone, Copy)]
pub(in crate::server) struct JobCooldownUpdate {
    pub cooldown_group: u32,
    /// Relative cooldown reduction in centiseconds.
    pub delta_centisec: u32,
}

/// Per-action lifecycle for a combat job. Implemented by ZST unit structs. All heavy logic stays in
/// the job module; each method delegates to the existing free fn.
pub(in crate::server) trait Job: Sync {
    /// Dispatch key(s) this job answers to (class + category ids), for the registry only.
    fn class_jobs(&self) -> &'static [u8];

    /// Class-job id under which this job's `ActorGauge` packets are sent. Bard returns `CLASSJOB_BARD`
    /// regardless of the dispatch class; most jobs return the live `dispatch_class_job`.
    fn gauge_class_job_id(&self, dispatch_class_job: u8) -> u8 {
        dispatch_class_job
    }

    /// S1: action morph (Burst→Refulgent, Ruin→Astral Impulse, …). Pure; reads a clone of state.
    fn resolve_action(
        &self,
        request: &ActionRequest,
        combat_state: &PlayerCombatState,
        level: u8,
        game_data: &mut GameData,
    ) -> u32;

    /// S1: post-morph execution gate.
    fn can_execute(&self, action_id: u32, combat_state: &PlayerCombatState, level: u8) -> bool;

    /// S5: apply one Lua `modify_gauge` action to combat state.
    fn apply_gauge_action(&self, combat_state: &mut PlayerCombatState, action: &GaugeAction);

    /// S6: mutate job state after the action resolves. Returns the unified update (bard fills it;
    /// summoner returns `Default`).
    fn update_state_after_action(
        &self,
        action_id: u32,
        actor: &mut NetworkedActor,
        owner_actor_id: ObjectId,
    ) -> JobActionUpdate;

    /// S6/S8/zone/login: build the 8-byte `ActorGauge` tail. `None` = this job has no gauge.
    fn build_gauge_data(&self, combat_state: &PlayerCombatState, level: u8) -> Option<u64>;

    /// S6b: the party-propagatable status this action grants, if any.
    fn party_buff_for_action(
        &self,
        _action_id: u32,
        _radiant_finale_bonus_percent: u8,
    ) -> Option<(u16, u16, f32)> {
        None
    }

    /// mod.rs tick + zone refresh: advance timers, expire statuses. Returns the unified refresh result.
    fn refresh_runtime_state_on_actor(
        &self,
        owner_actor_id: ObjectId,
        actor: &mut NetworkedActor,
    ) -> JobRefreshResult;

    /// Capability hook for the SEPARATE persistent-actor/VFX/task seam. `None` for every job that owns
    /// no pets. Summoner returns `Some(&Summoner)`.
    fn persistent_actors(&self) -> Option<&dyn JobActors> {
        None
    }
}

/// SEPARATE seam: job-owned persistent actors, VFX and scheduled tasks. Summoner-only today.
/// Deliberately NOT part of `Job`. Signatures mirror the existing summoner free fns 1:1; each method
/// delegates. Bard does not implement this trait at all.
pub(in crate::server) trait JobActors: Sync {
    /// S3: does this action drive a pet spawn/transition (deferred until after the result packet)?
    fn has_pet_transition_for_action(&self, action_id: u32) -> bool;

    /// S4: inject SummonPet/ready effects into the outgoing effect list (before AoE base capture).
    fn augment_action_result_effects(&self, action_id: u32, effects: &mut Vec<TargetEffect>);

    /// S2: reconcile the pet actor when the owner mounts.
    fn sync_pet_for_mount(
        &self,
        network: &mut NetworkState,
        instance: &mut Instance,
        owner: ObjectId,
    );

    /// S7: register the Slipstream lingering ground-AoE task after the action resolves. `target` is
    /// the AoE center source (`resolved_request.target.object_id`).
    fn register_lingering_aoe_after_action(
        &self,
        instance: &mut Instance,
        owner: ObjectId,
        action_id: u32,
        target: ObjectId,
    );

    /// S8 (pre-packet): stage the pet transition.
    fn prepare_pet_transition_for_action(
        &self,
        network: &mut NetworkState,
        instance: &mut Instance,
        owner: ObjectId,
        action_id: u32,
    );

    /// S8 (post-packet): spawn the pet so it appears with animation.
    fn spawn_pet_after_action(
        &self,
        network: &mut NetworkState,
        instance: &mut Instance,
        owner: ObjectId,
        action_id: u32,
        target: ObjectId,
    );

    /// S8: is this a demi summon (drives the demi-auto-attack schedule)?
    fn is_demi_summon(&self, action_id: u32) -> bool;

    /// S8: schedule the demi auto-attack task.
    fn schedule_demi_auto_attack(&self, instance: &mut Instance, owner: ObjectId);

    /// S8 (generic carbuncle path): spawn the summon-pet after the result packet.
    fn apply_summon_pet_effect(
        &self,
        network: Arc<Mutex<NetworkState>>,
        instance: &mut Instance,
        owner: ObjectId,
    );

    /// mod.rs tick: the demi window just expired this refresh — tear down the demi actor, then
    /// re-spawn/re-bind carbuncle. `gauge_update` = the freshly-built `(gauge_class_job_id, gauge_data)`
    /// to re-send, or `None`. Delegates to `apply_demi_summon_revert`.
    fn on_demi_expired(
        &self,
        network: &mut NetworkState,
        instance: &mut Instance,
        owner: ObjectId,
        gauge_update: Option<(u8, u64)>,
    );
}

/// The single per-job growth point. One arm per job; everything else is closed to extension.
pub(in crate::server) fn job_for(class_job: u8) -> Option<&'static dyn Job> {
    const BARD: &Bard = &Bard;
    const SUMMONER: &Summoner = &Summoner;
    match class_job {
        cj if BARD.class_jobs().contains(&cj) => Some(BARD),
        cj if SUMMONER.class_jobs().contains(&cj) => Some(SUMMONER),
        _ => None,
    }
}

/// Send an `ActorGauge` packet carrying this job's freshly-built gauge tail. Shared by every job
/// (folded from the former per-module copies in `action.rs` / `summoner.rs`).
pub(in crate::server) fn send_job_gauge_update(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_for_resolves_known_classes_and_rejects_unknown() {
        // Bard answers to both its class id (23) and the ARCHER category id (24).
        for cj in Bard.class_jobs() {
            let job = job_for(*cj).expect("bard class id should resolve");
            assert!(job.class_jobs().contains(cj));
            assert!(job.persistent_actors().is_none());
        }

        // Summoner answers to its class id (27) and owns the persistent-actor seam.
        for cj in Summoner.class_jobs() {
            let job = job_for(*cj).expect("summoner class id should resolve");
            assert!(job.class_jobs().contains(cj));
            assert!(job.persistent_actors().is_some());
        }

        // An unhandled class id resolves to nothing.
        assert!(job_for(0).is_none());
        assert!(job_for(19).is_none()); // Paladin — not yet implemented.
    }
}
