use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use kawari::{common::ObjectId, ipc::zone::SpawnNpc};

use super::jobs::bard::BardState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SummonerAttunement {
    #[default]
    None,
    Ruby,
    Topaz,
    Emerald,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SummonerDemiPhase {
    #[default]
    None,
    Bahamut,
    SolarBahamut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SummonerNextDemi {
    #[default]
    None,
    Bahamut,
    SolarBahamutFirst,
    SolarBahamutSecond,
}

/// Snapshot of a summoner pet captured at the source instance for cross-zone re-instatement.
#[derive(Debug, Clone)]
pub struct CarriedPet {
    pub actor_id: ObjectId,
    pub spawn: SpawnNpc,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SummonerState {
    pub carbuncle_summoned: bool,
    pub attunement: SummonerAttunement,
    pub attunement_stacks: u8,
    pub aetherflow_stacks: u8,
    pub ruby_arcanum: bool,
    pub topaz_arcanum: bool,
    pub emerald_arcanum: bool,
    pub further_ruin: u8,
    #[serde(skip)]
    pub further_ruin_expires_at: Option<Instant>,
    pub mountain_buster_ready: bool,
    pub slipstream_ready: bool,
    pub crimson_cyclone_ready: bool,
    pub crimson_strike_ready: bool,
    pub demi_phase: SummonerDemiPhase,
    pub next_demi: SummonerNextDemi,
    pub demi_enkindle_ready: bool,
    pub demi_finisher_ready: bool,
    #[serde(default)]
    pub demi_auto_attack_count: u8,
    #[serde(skip)]
    pub searing_light_expires_at: Option<Instant>,
    pub searing_flash_ready: bool,
    pub lux_solaris_ready: bool,
    #[serde(skip)]
    pub attunement_expires_at: Option<Instant>,
    #[serde(skip)]
    pub demi_expires_at: Option<Instant>,
    #[serde(skip)]
    pub primal_summon_expires_at: Option<Instant>,
    #[serde(skip)]
    pub searing_flash_expires_at: Option<Instant>,
    #[serde(skip)]
    pub lux_solaris_expires_at: Option<Instant>,
    /// A pet actor carried across a zone transition so it can be re-instated with the SAME object
    /// id (no re-summon / birth animation). None when no pet is out or after re-instatement.
    #[serde(skip)]
    pub carried_pet: Option<CarriedPet>,
}

// Hand-written to skip the transient `carried_pet` (its `SpawnNpc` is not `PartialEq`), mirroring
// `QueuedTask`'s impl that skips its non-comparable `data`. `carried_pet` is `#[serde(skip)]` carry
// state, never part of a summoner's persisted equality.
impl PartialEq for SummonerState {
    fn eq(&self, other: &Self) -> bool {
        self.carbuncle_summoned == other.carbuncle_summoned
            && self.attunement == other.attunement
            && self.attunement_stacks == other.attunement_stacks
            && self.aetherflow_stacks == other.aetherflow_stacks
            && self.ruby_arcanum == other.ruby_arcanum
            && self.topaz_arcanum == other.topaz_arcanum
            && self.emerald_arcanum == other.emerald_arcanum
            && self.further_ruin == other.further_ruin
            && self.further_ruin_expires_at == other.further_ruin_expires_at
            && self.mountain_buster_ready == other.mountain_buster_ready
            && self.slipstream_ready == other.slipstream_ready
            && self.crimson_cyclone_ready == other.crimson_cyclone_ready
            && self.crimson_strike_ready == other.crimson_strike_ready
            && self.demi_phase == other.demi_phase
            && self.next_demi == other.next_demi
            && self.demi_enkindle_ready == other.demi_enkindle_ready
            && self.demi_finisher_ready == other.demi_finisher_ready
            && self.demi_auto_attack_count == other.demi_auto_attack_count
            && self.searing_light_expires_at == other.searing_light_expires_at
            && self.searing_flash_ready == other.searing_flash_ready
            && self.lux_solaris_ready == other.lux_solaris_ready
            && self.attunement_expires_at == other.attunement_expires_at
            && self.demi_expires_at == other.demi_expires_at
            && self.primal_summon_expires_at == other.primal_summon_expires_at
            && self.searing_flash_expires_at == other.searing_flash_expires_at
            && self.lux_solaris_expires_at == other.lux_solaris_expires_at
    }
}

#[derive(Debug, Clone)]
pub struct CooldownState {
    pub action_id: u32,
    /// When the currently recovering charge started. `None` means the action is fully charged.
    pub started_at: Option<Instant>,
    /// Recast time for one charge.
    pub charge_duration: Duration,
    /// Usable charges right now, excluding the charge currently recovering.
    pub charges: u8,
    pub max_charges: u8,
}

impl CooldownState {
    fn max_charges(&self) -> u8 {
        self.max_charges.max(1)
    }

    fn refresh(&mut self) {
        let Some(started_at) = self.started_at else {
            self.charges = self.max_charges();
            return;
        };
        if self.charge_duration.is_zero() {
            self.charges = self.max_charges();
            self.started_at = None;
            return;
        }

        let elapsed = started_at.elapsed();
        let recovered = (elapsed.as_nanos() / self.charge_duration.as_nanos()) as u8;
        if recovered == 0 {
            return;
        }

        let max_charges = self.max_charges();
        self.charges = self.charges.saturating_add(recovered).min(max_charges);
        if self.charges >= max_charges {
            self.started_at = None;
        } else {
            let remainder_nanos = elapsed.as_nanos() % self.charge_duration.as_nanos();
            let remainder = Duration::from_nanos(remainder_nanos.min(u128::from(u64::MAX)) as u64);
            let now = Instant::now();
            self.started_at = Some(now.checked_sub(remainder).unwrap_or(now));
        }
    }

    /// Whether the recast has elapsed. `tolerance` treats the cooldown as ready once it's within
    /// that window of expiring, absorbing the small offset between the client's locally predicted
    /// GCD and the server clock (see `COOLDOWN_TOLERANCE`). Pass `Duration::ZERO` for a strict check.
    pub fn is_ready(&mut self, tolerance: Duration) -> bool {
        self.refresh();
        if self.charges > 0 {
            return true;
        }

        self.started_at
            .map(|started_at| started_at.elapsed() + tolerance >= self.charge_duration)
            .unwrap_or(true)
    }

    pub fn remaining(&mut self) -> Duration {
        self.refresh();
        if self.charges > 0 {
            return Duration::ZERO;
        }

        self.started_at
            .map(|started_at| self.charge_duration.saturating_sub(started_at.elapsed()))
            .unwrap_or_default()
    }

    pub fn reduce_recovery(&mut self, amount: Duration) {
        self.refresh();
        if let Some(started_at) = self.started_at {
            self.started_at = Some(started_at.checked_sub(amount).unwrap_or(started_at));
            self.refresh();
        }
    }

    pub fn timer_values(&mut self) -> (u32, u32) {
        self.refresh();
        let Some(started_at) = self.started_at else {
            return (0, 0);
        };

        // Scale total to the full charge pool so SetCooldownTimer stays consistent with the
        // SetCooldownTimerMax value sent on use for multi-charge actions (which carries
        // MaxCharges × R). The client's charge math requires total = MaxCharges × R.
        let pool_duration = self.charge_duration * u32::from(self.max_charges());
        let total_centisec = duration_to_centisec(pool_duration);
        let elapsed_centisec = duration_to_centisec(started_at.elapsed().min(self.charge_duration));
        (elapsed_centisec, total_centisec)
    }
}

fn duration_to_centisec(duration: Duration) -> u32 {
    (duration.as_millis() / 10).min(u128::from(u32::MAX)) as u32
}

#[derive(Debug, Clone, Default)]
pub struct PlayerCombatState {
    pub cooldowns: Vec<Option<CooldownState>>,
    pub summoner: SummonerState,
    pub bard: BardState,
    /// Whether the player currently has aggro (something hates them). Tracked so the server only
    /// sends a battle-state toggle (weapon drawn + combat music) when it actually changes.
    pub in_combat: bool,
}

impl PlayerCombatState {
    pub const MAX_COOLDOWN_GROUPS: usize = 100;

    pub fn ensure_capacity(&mut self) {
        if self.cooldowns.len() < Self::MAX_COOLDOWN_GROUPS {
            self.cooldowns.resize(Self::MAX_COOLDOWN_GROUPS, None);
        }
    }

    pub fn cooldown_ready(&mut self, cooldown_group_index: usize, tolerance: Duration) -> bool {
        self.ensure_capacity();
        let Some(cooldown) = &mut self.cooldowns[cooldown_group_index] else {
            return true;
        };
        cooldown.is_ready(tolerance)
    }

    pub fn cooldown_remaining(&mut self, cooldown_group_index: usize) -> Duration {
        self.ensure_capacity();
        self.cooldowns[cooldown_group_index]
            .as_mut()
            .map(CooldownState::remaining)
            .unwrap_or_default()
    }

    pub fn start_cooldown(
        &mut self,
        cooldown_group_index: usize,
        action_id: u32,
        duration: Duration,
        max_charges: u8,
        tolerance: Duration,
    ) {
        self.ensure_capacity();
        let max_charges = max_charges.max(1);
        let now = Instant::now();
        let cooldown = self.cooldowns[cooldown_group_index].get_or_insert_with(|| CooldownState {
            action_id,
            started_at: None,
            charge_duration: duration,
            charges: max_charges,
            max_charges,
        });

        cooldown.refresh();
        cooldown.action_id = action_id;
        cooldown.charge_duration = duration;
        cooldown.max_charges = max_charges;
        cooldown.charges = cooldown.charges.min(max_charges);

        if cooldown.charges == 0
            && let Some(started_at) = cooldown.started_at
            && started_at.elapsed() + tolerance >= cooldown.charge_duration
        {
            cooldown.charges = 1;
            cooldown.started_at = None;
        }

        if cooldown.charges > 0 {
            cooldown.charges -= 1;
        }

        if cooldown.charges < max_charges && cooldown.started_at.is_none() {
            cooldown.started_at = Some(now);
        }
    }

    pub fn clear_cooldown(&mut self, cooldown_group_index: usize) {
        self.ensure_capacity();
        self.cooldowns[cooldown_group_index] = None;
    }

    pub fn reduce_cooldown_recovery(
        &mut self,
        cooldown_group_index: usize,
        amount: Duration,
    ) -> Option<(u32, u32)> {
        self.ensure_capacity();
        let cooldown = self.cooldowns[cooldown_group_index].as_mut()?;
        cooldown.reduce_recovery(amount);
        Some(cooldown.timer_values())
    }

    /// Returns `(elapsed_centisec, total_centisec)` for a cooldown group, scaled to the full
    /// charge pool (matching the wire encoding used by SetCooldownTimer). Returns `(0, 0)` if the
    /// group is untracked or fully ready — which the client reads as "reset this group" (Path A),
    /// exactly what we want for a rejection where the action never happened (job-state/MP).
    pub fn cooldown_timer_values(&mut self, cooldown_group_index: usize) -> (u32, u32) {
        self.ensure_capacity();
        self.cooldowns[cooldown_group_index]
            .as_mut()
            .map(CooldownState::timer_values)
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh summoner has no pet carried across a zone transition.
    #[test]
    fn default_summoner_has_no_carried_pet() {
        assert!(SummonerState::default().carried_pet.is_none());
    }
}
