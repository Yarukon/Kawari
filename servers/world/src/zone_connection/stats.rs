//! Managing statistics, including your classjob and other related information.

use crate::{
    GameData, ToServer, ZoneConnection,
    gamedata::{Attributes, CapSlot, ItemLevelCaps, MateriaStats, Modifiers},
    inventory::{EquippedStorage, Storage},
    zone_connection::effective_level,
};
use icarus::ParamGrow::ParamGrowRow;
use kawari::{
    common::{MAXIMUM_RESTED_EXP, ObjectId},
    ipc::zone::{
        ActorControlCategory, DamageKind, PlayerStats, ServerZoneIpcData, ServerZoneIpcSegment,
        UpdateClassInfo,
    },
};
use mlua::{UserData, UserDataMethods};
use physis::equipment::EquipSlot;

#[derive(Clone, Copy)]
struct LevelModifier {
    main: u16,
    sub: u16,
    div: u16,
}

/// BaseParam ids for the item stats that live in their own Item sheet columns rather than in an
/// item's BaseParam array, and so have no id of their own to hand to the item level clamp.
const BASE_PARAM_PHYSICAL_DAMAGE: u8 = 12;
const BASE_PARAM_MAGICAL_DAMAGE: u8 = 13;
const BASE_PARAM_DEFENSE: u8 = 21;
const BASE_PARAM_MAGIC_DEFENSE: u8 = 24;

/// Base CP every Disciple of the Hand starts with, before gear and materia.
///
/// Flat: unlike HP and MP this does not grow with level, and no Excel sheet carries it -- not
/// `ParamGrow`, not `ClassJob`. Confirmed against retail.
const BASE_CP: u32 = 180;

/// Base GP every Disciple of the Land starts with, before gear and materia.
///
/// Also flat and also absent from the sheets. Only GP *regeneration* scales with level (via the
/// traits learned at 70/80/83), which is a separate mechanic and not this value.
const BASE_GP: u32 = 400;

/// `ClassJob` row ids of the Disciples of the Hand, whose resource bar is CP.
///
/// Carpenter through Culinarian, contiguous. Cross-checks in the sheet: `ClassJobCategory` 33 and
/// `BattleClassIndex` -1. (`DohDolJobIndex` restarts at 0 for Miner, so it cannot tell the two
/// Disciple families apart on its own.)
const CLASSJOBS_DOH: std::ops::RangeInclusive<u8> = 8..=15;

/// `ClassJob` row ids of the Disciples of the Land, whose resource bar is GP.
///
/// Miner, Botanist, Fisher. `ClassJobCategory` 32.
const CLASSJOBS_DOL: std::ops::RangeInclusive<u8> = 16..=18;

/// Adds `value` to `param_id`'s entry in an item's contribution list, creating it if absent.
///
/// Used to fold the stats that live in their own Item columns into the same list as the BaseParam
/// array, so that item level sync clamps each param exactly once. Adding zero is a no-op, which
/// keeps items that carry no defense or weapon damage out of the list entirely.
fn merge_contribution(contributions: &mut Vec<(u8, i32)>, param_id: u8, value: i32) {
    if param_id == 0 || value == 0 {
        return;
    }
    match contributions.iter_mut().find(|(id, _)| *id == param_id) {
        Some((_, total)) => *total += value,
        None => contributions.push((param_id, value)),
    }
}

const LEVEL_MODIFIERS: [LevelModifier; 101] = [
    LevelModifier {
        main: 20,
        sub: 56,
        div: 56,
    },
    LevelModifier {
        main: 20,
        sub: 56,
        div: 56,
    },
    LevelModifier {
        main: 21,
        sub: 57,
        div: 57,
    },
    LevelModifier {
        main: 22,
        sub: 60,
        div: 60,
    },
    LevelModifier {
        main: 24,
        sub: 62,
        div: 62,
    },
    LevelModifier {
        main: 26,
        sub: 65,
        div: 65,
    },
    LevelModifier {
        main: 27,
        sub: 68,
        div: 68,
    },
    LevelModifier {
        main: 29,
        sub: 70,
        div: 70,
    },
    LevelModifier {
        main: 31,
        sub: 73,
        div: 73,
    },
    LevelModifier {
        main: 33,
        sub: 76,
        div: 76,
    },
    LevelModifier {
        main: 35,
        sub: 78,
        div: 78,
    },
    LevelModifier {
        main: 36,
        sub: 82,
        div: 82,
    },
    LevelModifier {
        main: 38,
        sub: 85,
        div: 85,
    },
    LevelModifier {
        main: 41,
        sub: 89,
        div: 89,
    },
    LevelModifier {
        main: 44,
        sub: 93,
        div: 93,
    },
    LevelModifier {
        main: 46,
        sub: 96,
        div: 96,
    },
    LevelModifier {
        main: 49,
        sub: 100,
        div: 100,
    },
    LevelModifier {
        main: 52,
        sub: 104,
        div: 104,
    },
    LevelModifier {
        main: 54,
        sub: 109,
        div: 109,
    },
    LevelModifier {
        main: 57,
        sub: 113,
        div: 113,
    },
    LevelModifier {
        main: 60,
        sub: 116,
        div: 116,
    },
    LevelModifier {
        main: 63,
        sub: 122,
        div: 122,
    },
    LevelModifier {
        main: 67,
        sub: 127,
        div: 127,
    },
    LevelModifier {
        main: 71,
        sub: 133,
        div: 133,
    },
    LevelModifier {
        main: 74,
        sub: 138,
        div: 138,
    },
    LevelModifier {
        main: 78,
        sub: 144,
        div: 144,
    },
    LevelModifier {
        main: 81,
        sub: 150,
        div: 150,
    },
    LevelModifier {
        main: 85,
        sub: 155,
        div: 155,
    },
    LevelModifier {
        main: 89,
        sub: 162,
        div: 162,
    },
    LevelModifier {
        main: 92,
        sub: 168,
        div: 168,
    },
    LevelModifier {
        main: 97,
        sub: 173,
        div: 173,
    },
    LevelModifier {
        main: 101,
        sub: 181,
        div: 181,
    },
    LevelModifier {
        main: 106,
        sub: 188,
        div: 188,
    },
    LevelModifier {
        main: 110,
        sub: 194,
        div: 194,
    },
    LevelModifier {
        main: 115,
        sub: 202,
        div: 202,
    },
    LevelModifier {
        main: 119,
        sub: 209,
        div: 209,
    },
    LevelModifier {
        main: 124,
        sub: 215,
        div: 215,
    },
    LevelModifier {
        main: 128,
        sub: 223,
        div: 223,
    },
    LevelModifier {
        main: 134,
        sub: 229,
        div: 229,
    },
    LevelModifier {
        main: 139,
        sub: 236,
        div: 236,
    },
    LevelModifier {
        main: 144,
        sub: 244,
        div: 244,
    },
    LevelModifier {
        main: 150,
        sub: 253,
        div: 253,
    },
    LevelModifier {
        main: 155,
        sub: 263,
        div: 263,
    },
    LevelModifier {
        main: 161,
        sub: 272,
        div: 272,
    },
    LevelModifier {
        main: 166,
        sub: 283,
        div: 283,
    },
    LevelModifier {
        main: 171,
        sub: 292,
        div: 292,
    },
    LevelModifier {
        main: 177,
        sub: 302,
        div: 302,
    },
    LevelModifier {
        main: 183,
        sub: 311,
        div: 311,
    },
    LevelModifier {
        main: 189,
        sub: 322,
        div: 322,
    },
    LevelModifier {
        main: 196,
        sub: 331,
        div: 331,
    },
    LevelModifier {
        main: 202,
        sub: 341,
        div: 341,
    },
    LevelModifier {
        main: 204,
        sub: 342,
        div: 366,
    },
    LevelModifier {
        main: 205,
        sub: 344,
        div: 392,
    },
    LevelModifier {
        main: 207,
        sub: 345,
        div: 418,
    },
    LevelModifier {
        main: 209,
        sub: 346,
        div: 444,
    },
    LevelModifier {
        main: 210,
        sub: 347,
        div: 470,
    },
    LevelModifier {
        main: 212,
        sub: 349,
        div: 496,
    },
    LevelModifier {
        main: 214,
        sub: 350,
        div: 522,
    },
    LevelModifier {
        main: 215,
        sub: 351,
        div: 548,
    },
    LevelModifier {
        main: 217,
        sub: 352,
        div: 574,
    },
    LevelModifier {
        main: 218,
        sub: 354,
        div: 600,
    },
    LevelModifier {
        main: 224,
        sub: 355,
        div: 630,
    },
    LevelModifier {
        main: 228,
        sub: 356,
        div: 660,
    },
    LevelModifier {
        main: 236,
        sub: 357,
        div: 690,
    },
    LevelModifier {
        main: 244,
        sub: 358,
        div: 720,
    },
    LevelModifier {
        main: 252,
        sub: 359,
        div: 750,
    },
    LevelModifier {
        main: 260,
        sub: 360,
        div: 780,
    },
    LevelModifier {
        main: 268,
        sub: 361,
        div: 810,
    },
    LevelModifier {
        main: 276,
        sub: 362,
        div: 840,
    },
    LevelModifier {
        main: 284,
        sub: 363,
        div: 870,
    },
    LevelModifier {
        main: 292,
        sub: 364,
        div: 900,
    },
    LevelModifier {
        main: 296,
        sub: 365,
        div: 940,
    },
    LevelModifier {
        main: 300,
        sub: 366,
        div: 980,
    },
    LevelModifier {
        main: 305,
        sub: 367,
        div: 1020,
    },
    LevelModifier {
        main: 310,
        sub: 368,
        div: 1060,
    },
    LevelModifier {
        main: 315,
        sub: 370,
        div: 1100,
    },
    LevelModifier {
        main: 320,
        sub: 372,
        div: 1140,
    },
    LevelModifier {
        main: 325,
        sub: 374,
        div: 1180,
    },
    LevelModifier {
        main: 330,
        sub: 376,
        div: 1220,
    },
    LevelModifier {
        main: 335,
        sub: 378,
        div: 1260,
    },
    LevelModifier {
        main: 340,
        sub: 380,
        div: 1300,
    },
    LevelModifier {
        main: 345,
        sub: 382,
        div: 1360,
    },
    LevelModifier {
        main: 350,
        sub: 384,
        div: 1420,
    },
    LevelModifier {
        main: 355,
        sub: 386,
        div: 1480,
    },
    LevelModifier {
        main: 360,
        sub: 388,
        div: 1540,
    },
    LevelModifier {
        main: 365,
        sub: 390,
        div: 1600,
    },
    LevelModifier {
        main: 370,
        sub: 392,
        div: 1660,
    },
    LevelModifier {
        main: 375,
        sub: 394,
        div: 1720,
    },
    LevelModifier {
        main: 380,
        sub: 396,
        div: 1780,
    },
    LevelModifier {
        main: 385,
        sub: 398,
        div: 1840,
    },
    LevelModifier {
        main: 390,
        sub: 400,
        div: 1900,
    },
    LevelModifier {
        main: 395,
        sub: 402,
        div: 1988,
    },
    LevelModifier {
        main: 400,
        sub: 404,
        div: 2076,
    },
    LevelModifier {
        main: 405,
        sub: 406,
        div: 2164,
    },
    LevelModifier {
        main: 410,
        sub: 408,
        div: 2252,
    },
    LevelModifier {
        main: 415,
        sub: 410,
        div: 2340,
    },
    LevelModifier {
        main: 420,
        sub: 412,
        div: 2428,
    },
    LevelModifier {
        main: 425,
        sub: 414,
        div: 2516,
    },
    LevelModifier {
        main: 430,
        sub: 416,
        div: 2604,
    },
    LevelModifier {
        main: 435,
        sub: 418,
        div: 2692,
    },
    LevelModifier {
        main: 440,
        sub: 420,
        div: 2780,
    },
];

fn level_modifier_for(level: u32) -> LevelModifier {
    LEVEL_MODIFIERS[level.clamp(1, 100) as usize]
}

/// Retail-calibrated HP gained per point of vitality above the level's MAIN baseline.
///
/// `k(Lv)` is server-authoritative and not present in any EXD column or the client binary.
/// Public curves (plugin fits, xivgear, Sapphire, AkhMorning absolute k) are L100-calibrated
/// and wrong below cap — do not substitute them. Anchors below are BRD/WAR retail readings
/// taken 2026-07-16 (no food/FC; job HP mod only) and inverted via
/// `k = (HP − ⌊HpMod×jobHpMod/100⌋) / (VIT − MAIN)`.
///
/// Tank and non-tank are independent tables: the tank/non-tank ratio is ~1.34 at L70 and only
/// approaches 10/7 at L100, so a constant multiplier is wrong.
///
/// Between knots: piecewise linear. Below the first knot: extrapolate the first segment,
/// floored at 2.0. Above 100: clamp to the L100 value.
fn hp_per_vitality(level: u32, is_tank: bool) -> f64 {
    // (level, k) knots. Same levels for both roles so interpolation stays aligned.
    const NON_TANK: [(u32, f64); 6] = [
        (54, 12.036),
        (60, 12.346),
        (70, 13.312),
        (80, 18.025),
        (90, 23.936),
        (100, 30.066),
    ];
    const TANK: [(u32, f64); 6] = [
        (54, 16.482),
        (60, 16.767),
        (70, 17.869),
        (80, 25.542),
        (90, 34.102),
        (100, 42.950),
    ];
    let table = if is_tank { &TANK } else { &NON_TANK };
    interpolate_hp_per_vitality(level, table)
}

fn interpolate_hp_per_vitality(level: u32, table: &[(u32, f64)]) -> f64 {
    let first = table[0];
    let last = table[table.len() - 1];
    if level >= last.0 {
        return last.1;
    }
    if level <= first.0 {
        // Extrapolate the first segment (54→60) down; never go below a tiny positive floor.
        let (l0, k0) = first;
        let (l1, k1) = table[1];
        let slope = (k1 - k0) / f64::from(l1 - l0);
        return (k0 + slope * f64::from(level as i32 - l0 as i32)).max(2.0);
    }
    for window in table.windows(2) {
        let (l0, k0) = window[0];
        let (l1, k1) = window[1];
        if level <= l1 {
            let t = f64::from(level - l0) / f64::from(l1 - l0);
            return k0 + (k1 - k0) * t;
        }
    }
    last.1
}

fn attack_modifier_for_level(level: u32) -> f64 {
    match level {
        0..=50 => 75.0,
        51..=70 => ((level - 50) as f64 * 2.5) + 75.0,
        71..=80 => ((level - 70) as f64 * 4.0) + 125.0,
        81..=90 => ((level - 80) as f64 * 3.0) + 165.0,
        _ => ((level - 90) as f64 * 4.2) + 195.0,
    }
}

fn heal_modifier_for_level(level: u32) -> f64 {
    match level {
        0..=59 => (level as f64 * 1.5) + 10.0,
        60..=69 => ((level - 60) as f64 * 2.0) + 100.0,
        70..=79 => 120.0,
        _ => ((level - 80) as f64 * 2.5) + 120.8,
    }
}

fn tank_attack_modifier_for_level(level: u32) -> f64 {
    match level {
        0..=80 => level as f64 + 35.0,
        81..=90 => ((level - 80) as f64 * 4.1) + 115.0,
        _ => ((level - 90) as f64 * 3.4) + 156.0,
    }
}

fn positive_scaled_bonus(value: u32, baseline: u16, coefficient: f64, divisor: u16) -> u32 {
    let baseline = u32::from(baseline);
    if value <= baseline || divisor == 0 {
        return 0;
    }

    (coefficient * (value - baseline) as f64 / f64::from(divisor)).floor() as u32
}

fn apply_factor(value: u64, factor: u32, divisor: u64) -> u64 {
    value.saturating_mul(u64::from(factor)) / divisor
}

fn classjob_uses_dexterity(classjob_id: u8) -> bool {
    matches!(classjob_id, 5 | 23 | 29 | 30 | 31 | 38 | 41)
}

fn classjob_uses_mind(classjob_id: u8) -> bool {
    matches!(classjob_id, 6 | 24 | 28 | 33 | 40)
}

fn classjob_uses_tenacity(classjob_id: u8) -> bool {
    matches!(classjob_id, 1 | 3 | 19 | 21 | 32 | 37)
}

fn classjob_is_caster_like(classjob_id: u8) -> bool {
    matches!(
        classjob_id,
        6 | 7 | 24 | 25 | 26 | 27 | 28 | 33 | 35 | 36 | 40 | 42
    )
}

fn classjob_primary_stat_id(classjob_id: u8) -> u8 {
    if classjob_uses_mind(classjob_id) {
        5
    } else if classjob_is_caster_like(classjob_id) {
        4
    } else if classjob_uses_dexterity(classjob_id) {
        2
    } else {
        1
    }
}

fn classjob_damage_trait_modifier(classjob_id: u8, level: u32) -> f64 {
    if classjob_id == 36 {
        return match level {
            50.. => 1.5,
            40..=49 => 1.4,
            30..=39 => 1.3,
            20..=29 => 1.2,
            10..=19 => 1.1,
            _ => 1.0,
        };
    }

    if classjob_is_caster_like(classjob_id) {
        return match level {
            40.. => 1.3,
            20..=39 => 1.1,
            _ => 1.0,
        };
    }

    match classjob_id {
        5 | 23 | 31 => match level {
            40.. => 1.2,
            20..=39 => 1.1,
            _ => 1.0,
        },
        38 => match level {
            60.. => 1.2,
            50..=59 => 1.1,
            _ => 1.0,
        },
        _ => 1.0,
    }
}

/// Every BaseParam row, some of them may be useless.
#[derive(Default, Debug, Clone)]
pub struct BaseParameters {
    pub strength: u32,
    pub dexterity: u32,
    pub vitality: u32,
    pub intelligence: u32,
    pub mind: u32,
    pub piety: u32,
    pub hp: u32,
    pub mp: u32,
    pub tp: u32,
    pub gp: u32,
    pub cp: u32,
    pub physical_damage: u32,
    pub magic_damage: u32,
    pub delay: u32,
    pub additional_effect: u32,
    pub attack_speed: u32,
    pub block_rate: u32,
    pub block_strength: u32,
    pub tenacity: u32,
    pub attack_power: u32,
    pub defense: u32,
    pub direct_hit_rate: u32,
    pub evasion: u32,
    pub magic_defense: u32,
    pub critical_hit_power: u32,
    pub critical_hit_resilience: u32,
    pub critical_hit: u32,
    pub critical_hit_evasion: u32,
    pub slashing_resistance: u32,
    pub piercing_resistance: u32,
    pub blunt_resistance: u32,
    pub projectile_resistance: u32,
    pub attack_magic_potency: u32,
    pub healing_magic_potency: u32,
    pub enhancement_magic_potency: u32,
    pub elemental_bonus: u32,
    pub fire_resistance: u32,
    pub ice_resistance: u32,
    pub wind_resistance: u32,
    pub earth_resistance: u32,
    pub lightning_resistance: u32,
    pub water_resistance: u32,
    pub magic_resistance: u32,
    pub determination: u32,
    pub skill_speed: u32,
    pub spell_speed: u32,
    pub haste: u32,
    pub morale: u32,
    pub enmity: u32,
    pub enmity_reduction: u32,
    pub desynthesis_skill_gain: u32,
    pub exp_bonus: u32,
    pub regen: u32,
    pub special_attribute: u32,
    pub main_attribute: u32,
    pub secondary_attribute: u32,
    pub slow_resistance: u32,
    pub petrification_resistance: u32,
    pub paralysis_resistance: u32,
    pub silence_resistance: u32,
    pub blind_resistance: u32,
    pub posion_resistance: u32,
    pub stun_resistance: u32,
    pub sleep_resistance: u32,
    pub bind_resistance: u32,
    pub heavy_resistance: u32,
    pub doom_resistance: u32,
    pub reduced_durability_loss: u32,
    pub increased_spiritbond_gain: u32,
    pub craftmanship: u32,
    pub control: u32,
    pub gathering: u32,
    pub perception: u32,
    pub classjob_id: u8,
    pub level: u8,
    pub job_attack_modifier: u16,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DamageRollModifiers {
    pub crit_rate_bonus: f64,
    pub direct_hit_rate_bonus: f64,
    pub force_critical: bool,
    pub force_direct_hit: bool,
}

impl BaseParameters {
    /// The resource pool the client shows in place of the MP bar for this job.
    ///
    /// `CommonSpawn::resource_points` is one polymorphic field: MP for a battle job, CP for a
    /// Disciple of the Hand, GP for a Disciple of the Land. Sending `mp` unconditionally is what
    /// made crafters and gatherers read 10000 -- `ParamGrow.MpModifier` -- on their CP/GP bar.
    pub fn resource_points(&self) -> u32 {
        if CLASSJOBS_DOH.contains(&self.classjob_id) {
            self.cp
        } else if CLASSJOBS_DOL.contains(&self.classjob_id) {
            self.gp
        } else {
            self.mp
        }
    }

    pub fn get_mut(&mut self, index: u8) -> &mut u32 {
        match index {
            1 => &mut self.strength,
            2 => &mut self.dexterity,
            3 => &mut self.vitality,
            4 => &mut self.intelligence,
            5 => &mut self.mind,
            6 => &mut self.piety,
            7 => &mut self.hp,
            8 => &mut self.mp,
            9 => &mut self.tp,
            10 => &mut self.gp,
            11 => &mut self.cp,
            12 => &mut self.physical_damage,
            13 => &mut self.magic_damage,
            14 => &mut self.delay,
            15 => &mut self.additional_effect,
            16 => &mut self.attack_speed,
            17 => &mut self.block_rate,
            18 => &mut self.block_strength,
            19 => &mut self.tenacity,
            20 => &mut self.attack_power,
            21 => &mut self.defense,
            22 => &mut self.direct_hit_rate,
            23 => &mut self.evasion,
            24 => &mut self.magic_defense,
            25 => &mut self.critical_hit_power,
            26 => &mut self.critical_hit_resilience,
            27 => &mut self.critical_hit,
            28 => &mut self.critical_hit_evasion,
            29 => &mut self.slashing_resistance,
            30 => &mut self.piercing_resistance,
            31 => &mut self.blunt_resistance,
            32 => &mut self.projectile_resistance,
            33 => &mut self.attack_magic_potency,
            34 => &mut self.healing_magic_potency,
            35 => &mut self.enhancement_magic_potency,
            36 => &mut self.elemental_bonus,
            37 => &mut self.fire_resistance,
            38 => &mut self.ice_resistance,
            39 => &mut self.wind_resistance,
            40 => &mut self.earth_resistance,
            41 => &mut self.lightning_resistance,
            42 => &mut self.water_resistance,
            43 => &mut self.magic_resistance,
            44 => &mut self.determination,
            45 => &mut self.skill_speed,
            46 => &mut self.spell_speed,
            47 => &mut self.haste,
            48 => &mut self.morale,
            49 => &mut self.enmity,
            50 => &mut self.enmity_reduction,
            51 => &mut self.desynthesis_skill_gain,
            52 => &mut self.exp_bonus,
            53 => &mut self.regen,
            54 => &mut self.special_attribute,
            55 => &mut self.main_attribute,
            56 => &mut self.secondary_attribute,
            57 => &mut self.slow_resistance,
            58 => &mut self.petrification_resistance,
            59 => &mut self.paralysis_resistance,
            60 => &mut self.silence_resistance,
            61 => &mut self.blind_resistance,
            62 => &mut self.posion_resistance,
            63 => &mut self.stun_resistance,
            64 => &mut self.sleep_resistance,
            65 => &mut self.bind_resistance,
            66 => &mut self.heavy_resistance,
            67 => &mut self.doom_resistance,
            68 => &mut self.reduced_durability_loss,
            69 => &mut self.increased_spiritbond_gain,
            70 => &mut self.craftmanship,
            71 => &mut self.control,
            72 => &mut self.gathering,
            73 => &mut self.perception,
            _ => unreachable!(),
        }
    }

    /// Calculates a set of attributes based on the level and class modifiers.
    pub fn calculate_based_on_level(
        &mut self,
        attributes: &Attributes,
        level: u32,
        classjob_id: u8,
        param_grow: &ParamGrowRow,
        modifiers: &Modifiers,
    ) {
        self.classjob_id = classjob_id;
        self.level = level.min(u32::from(u8::MAX)) as u8;
        self.job_attack_modifier = match classjob_primary_stat_id(classjob_id) {
            1 => modifiers.strength,
            2 => modifiers.dexterity,
            4 => modifiers.intelligence,
            5 => modifiers.mind,
            _ => 100,
        };

        // `ParamGrow` has no MAIN column: its `BaseSpeed` is the SUB constant and its
        // `LevelModifier` is DIV, which is why `BaseSpeed` is the right base for the speed
        // substats and tenacity but not for anything that scales off MAIN. The MAIN values come
        // from `LEVEL_MODIFIERS` instead.
        let level_modifier = level_modifier_for(level);
        let main = u32::from(level_modifier.main);
        let sub = u32::from(level_modifier.sub);

        self.strength = modifiers
            .apply_to(1, main)
            .saturating_add_signed(attributes.strength as i32);
        self.dexterity = modifiers
            .apply_to(2, main)
            .saturating_add_signed(attributes.dexterity as i32);
        self.vitality = modifiers
            .apply_to(3, main)
            .saturating_add_signed(attributes.vitality as i32);
        self.intelligence = modifiers
            .apply_to(4, main)
            .saturating_add_signed(attributes.intelligence as i32);
        self.mind = modifiers
            .apply_to(5, main)
            .saturating_add_signed(attributes.mind as i32);
        self.piety = modifiers
            .apply_to(6, main)
            .saturating_add_signed(attributes.piety as i32);

        self.spell_speed = param_grow.BaseSpeed as u32;
        self.tenacity = param_grow.BaseSpeed as u32;
        self.skill_speed = self.tenacity;

        // Crit and direct hit baseline on SUB, determination on MAIN — the same split the damage
        // maths below already assumes, and retail's own. Without these the stats are pure gear
        // sums that sit under their baseline, so `positive_scaled_bonus` returns 0 and the gear's
        // contribution is silently discarded.
        self.critical_hit = sub;
        self.direct_hit_rate = sub;
        self.determination = main;

        self.haste = 100; // Controls cast times

        // This is fixed and isn't modified by any items in retail, so it's safe to be set here.
        self.mp = param_grow.MpModifier as u32;

        // Seeded for every job, not just the Disciples, which is what retail does: the values are
        // sent for battle jobs too, the client simply never displays them. Gating on the job here
        // would instead leave a stale zero behind whenever a character switched away from a
        // Disciple. Gear and materia add to these through `get_mut` (BaseParam 10 and 11) exactly
        // like any other stat.
        self.cp = BASE_CP;
        self.gp = BASE_GP;
    }

    // This should be called after item stat calculations.
    pub fn calculate_potencies(
        &mut self,
        level: u32,
        param_grow: &ParamGrowRow,
        modifiers: Option<&Modifiers>,
    ) {
        let primary_damage_stat = self.primary_damage_stat_value();
        self.attack_power = primary_damage_stat;
        self.attack_magic_potency = if classjob_is_caster_like(self.classjob_id) {
            primary_damage_stat
        } else {
            self.intelligence
        };
        self.healing_magic_potency = if classjob_uses_mind(self.classjob_id) {
            self.mind
        } else {
            primary_damage_stat
        };

        // Retail HP (calibrated 2026-07-16 against live BRD/WAR readings):
        //   HP = ⌊HpMod × jobHpMod / 100⌋ + ⌊(VIT − MAIN) × k(Lv, is_tank)⌋
        // Two independent floors; jobHpMod applies to the HpMod term only.
        // k is NOT ParamGrow.LevelModifier (that column is the damage divisor DIV) — see
        // `hp_per_vitality`. The old formula used DIV/100 as k and multiplied jobHpMod across
        // the whole expression, which halved synced-54 HP (3416 vs retail 6438 at vit 623).
        //
        // Excess is over raw MAIN, not the job-scaled seed. `calculate_based_on_level` still
        // multiplies MAIN by vitMod when seeding VIT, so a tank's 110% vit mod contributes
        // `0.1×MAIN` of excess (and thus HP) — that is intended. Both sites must read MAIN from
        // `level_modifier_for` (not `BaseSpeed`/SUB); they must NOT both apply vitMod, or the
        // job vit contribution cancels and tank HP falls short by hundreds–thousands.
        let classjob_hp_mod;
        let is_tank;

        if let Some(modifiers) = modifiers {
            classjob_hp_mod = modifiers.hp as f64 / 100.0;
            // ClassJob.ModifierHitPoints: tanks are 140 (PLD) / 145 (WAR/DRK/GNB); everyone
            // else is 105. The threshold is the documented tank floor, not a fitted constant.
            is_tank = modifiers.hp >= 140;
        } else {
            classjob_hp_mod = 1.0;
            is_tank = false;
        };

        let hp_mod = param_grow.HpModifier as f64;
        let base_vit = f64::from(level_modifier_for(level).main); // TODO: Tribe adjustments?
        let k = hp_per_vitality(level, is_tank);
        // Guard a pathological VIT < base (should not happen for real characters).
        let excess = (self.vitality as f64 - base_vit).max(0.0);

        self.hp = ((hp_mod * classjob_hp_mod).floor() + (excess * k).floor()) as u32;
    }

    fn primary_damage_stat_value(&self) -> u32 {
        match classjob_primary_stat_id(self.classjob_id) {
            2 => self.dexterity,
            4 => self.intelligence,
            5 => self.mind,
            _ => self.strength,
        }
    }

    fn expected_crit_rate(&self, level_modifier: LevelModifier) -> f64 {
        f64::from(
            50 + positive_scaled_bonus(
                self.critical_hit,
                level_modifier.sub,
                200.0,
                level_modifier.div,
            ),
        ) / 1000.0
    }

    fn crit_damage_factor(&self, level_modifier: LevelModifier) -> u32 {
        1400 + positive_scaled_bonus(
            self.critical_hit,
            level_modifier.sub,
            200.0,
            level_modifier.div,
        )
    }

    fn direct_hit_rate_bonus(&self, level_modifier: LevelModifier) -> f64 {
        f64::from(positive_scaled_bonus(
            self.direct_hit_rate,
            level_modifier.sub,
            550.0,
            level_modifier.div,
        )) / 1000.0
    }

    fn offensive_weapon_damage(&self) -> u32 {
        if classjob_is_caster_like(self.classjob_id) {
            self.magic_damage
        } else {
            self.physical_damage
        }
    }

    fn weapon_damage_factor(&self, level_modifier: LevelModifier) -> u32 {
        self.offensive_weapon_damage()
            + (u32::from(level_modifier.main) * u32::from(self.job_attack_modifier) / 1000)
    }

    fn attack_factor(&self, level_modifier: LevelModifier) -> u32 {
        let coefficient = if classjob_uses_tenacity(self.classjob_id) {
            tank_attack_modifier_for_level(u32::from(self.level))
        } else {
            attack_modifier_for_level(u32::from(self.level))
        };
        100 + positive_scaled_bonus(
            self.primary_damage_stat_value(),
            level_modifier.main,
            coefficient,
            level_modifier.main,
        )
    }

    fn heal_factor(&self, level_modifier: LevelModifier) -> u32 {
        100 + positive_scaled_bonus(
            self.primary_damage_stat_value(),
            level_modifier.main,
            heal_modifier_for_level(u32::from(self.level)),
            level_modifier.main,
        )
    }

    fn determination_factor(&self, level_modifier: LevelModifier) -> u32 {
        1000 + positive_scaled_bonus(
            self.determination,
            level_modifier.main,
            140.0,
            level_modifier.div,
        )
    }

    fn tenacity_factor(&self, level_modifier: LevelModifier) -> u32 {
        if !classjob_uses_tenacity(self.classjob_id) {
            return 1000;
        }

        1000 + positive_scaled_bonus(self.tenacity, level_modifier.sub, 112.0, level_modifier.div)
    }

    fn damage_trait_factor(&self) -> u32 {
        (classjob_damage_trait_modifier(self.classjob_id, u32::from(self.level)) * 100.0).round()
            as u32
    }

    fn calc_expected_damage(&self, potency: u32) -> u32 {
        if potency == 0 {
            return 0;
        }

        let level_modifier = level_modifier_for(u32::from(self.level));
        let weapon_damage_factor = self.weapon_damage_factor(level_modifier);
        if self.primary_damage_stat_value() == 0 || weapon_damage_factor == 0 {
            return 0;
        }

        let mut damage = u64::from(potency);
        damage = apply_factor(damage, self.attack_factor(level_modifier), 100);
        damage = apply_factor(damage, weapon_damage_factor, 100);
        damage = apply_factor(damage, self.determination_factor(level_modifier), 1000);
        damage = apply_factor(damage, self.tenacity_factor(level_modifier), 1000);
        damage = apply_factor(damage, self.damage_trait_factor(), 100);

        // Return the *base* (pre-crit, pre-direct-hit, pre-variance) damage. The crit/direct-hit
        // roll and ±5% variance are applied later by `roll_damage` at the point of impact, so the
        // client can be told the actual hit severity (and so each hit varies).
        damage.min(u64::from(u32::MAX)) as u32
    }

    /// Rolls a single damage instance from a base amount: independently rolls critical hit and
    /// direct hit from this character's rates, applies the ±5% damage variance, and reports which
    /// (if any) occurred so the client can show the right hit severity.
    pub fn roll_damage(&self, base: u32) -> (u32, DamageKind) {
        self.roll_damage_with_modifiers(base, DamageRollModifiers::default())
    }

    /// Rolls a single damage instance, allowing action/status effects to add crit or direct-hit
    /// chance, or force either roll.
    pub fn roll_damage_with_modifiers(
        &self,
        base: u32,
        modifiers: DamageRollModifiers,
    ) -> (u32, DamageKind) {
        if base == 0 {
            return (0, DamageKind::empty());
        }

        let level_modifier = level_modifier_for(u32::from(self.level));
        let crit_rate =
            (self.expected_crit_rate(level_modifier) + modifiers.crit_rate_bonus).clamp(0.0, 1.0);
        let direct_hit_rate = (self.direct_hit_rate_bonus(level_modifier)
            + modifiers.direct_hit_rate_bonus)
            .clamp(0.0, 1.0);
        let is_crit = modifiers.force_critical || fastrand::f64() < crit_rate;
        let is_direct = modifiers.force_direct_hit || fastrand::f64() < direct_hit_rate;

        let mut damage = u64::from(base);
        if is_crit {
            damage = apply_factor(damage, self.crit_damage_factor(level_modifier), 1000);
        }
        if is_direct {
            damage = apply_factor(damage, 125, 100);
        }
        // Retail damage variance is a whole-percent roll from 95% through 105%.
        damage = apply_factor(damage, 95 + fastrand::u32(0..11), 100);

        let mut kind = DamageKind::empty();
        if is_crit {
            kind |= DamageKind::CRITICAL;
        }
        if is_direct {
            kind |= DamageKind::DIRECT_HIT;
        }

        (damage.min(u64::from(u32::MAX)) as u32, kind)
    }

    /// Fraction of incoming damage (0.0–0.99) mitigated by this character's defense, per the
    /// retail formula (15% at a defense equal to the level's divisor). `is_magic` selects magic
    /// defense over physical defense.
    pub fn mitigation_against(&self, is_magic: bool) -> f64 {
        let level_modifier = level_modifier_for(u32::from(self.level));
        let defense = if is_magic {
            self.magic_defense
        } else {
            self.defense
        };
        (0.15 * defense as f64 / level_modifier.div as f64).clamp(0.0, 0.99)
    }

    /// Applies this character's skill/spell speed to a base time, matching the client's exact
    /// rounding (CharacterPanelRefined's SpeedCalc). Input and output are both in centiseconds
    /// (10ms units) — the same unit the client's cooldown packets use — so both sides agree to the
    /// centisecond and the GCD ring doesn't rubber-band. Casters use spell speed, others skill speed.
    pub fn apply_speed(&self, base_centisec: u32) -> u32 {
        if base_centisec == 0 {
            return 0;
        }
        let speed = if classjob_is_caster_like(self.classjob_id) {
            self.spell_speed
        } else {
            self.skill_speed
        };
        let level_modifier = level_modifier_for(u32::from(self.level));
        // factor = 1000 + ceil(130*(sub - speed)/div) = 1000 - floor(130*(speed - sub)/div).
        let factor = 1000.0
            - (130.0 * (speed as f64 - level_modifier.sub as f64) / level_modifier.div as f64)
                .floor();
        // Client formula (haste/type modifier assumed 0): floor(floor(factor * base / 100) / 10).
        let inner = (factor * base_centisec as f64 / 100.0).floor();
        ((inner / 10.0).floor() as u32).max(1)
    }

    /// Calculates amount of physical damage to apply based on potency.
    pub fn calc_physical_damage(&self, potency: u32) -> u32 {
        self.calc_expected_damage(potency)
    }

    /// Calculates amount of magic damage to apply based on potency.
    pub fn calc_magical_damage(&self, potency: u32) -> u32 {
        self.calc_expected_damage(potency)
    }

    /// Calculates amount of healing to apply based on potency.
    pub fn calc_heal_amount(&self, potency: u32) -> u32 {
        if potency == 0 {
            return 0;
        }

        let level_modifier = level_modifier_for(u32::from(self.level));
        let weapon_damage_factor = self.weapon_damage_factor(level_modifier);
        if self.primary_damage_stat_value() == 0 || weapon_damage_factor == 0 {
            return 0;
        }

        let mut heal = u64::from(potency);
        heal = apply_factor(heal, self.heal_factor(level_modifier), 100);
        heal = apply_factor(heal, weapon_damage_factor, 100);
        heal = apply_factor(heal, self.determination_factor(level_modifier), 1000);
        heal = apply_factor(heal, self.tenacity_factor(level_modifier), 1000);
        if classjob_is_caster_like(self.classjob_id) {
            heal = apply_factor(heal, self.damage_trait_factor(), 100);
        }
        // Roll a critical heal (heals can crit but never direct-hit) plus ±5% variance.
        if fastrand::f64() < self.expected_crit_rate(level_modifier) {
            heal = apply_factor(heal, self.crit_damage_factor(level_modifier), 1000);
        }
        heal = apply_factor(heal, 95 + fastrand::u32(0..11), 100);

        heal.min(u64::from(u32::MAX)) as u32
    }

    /// Iterates over the given equipped items and calculates defense, along with any stat bonuses.
    ///
    /// When `item_level_caps` is given the gear is synced down: every stat a slot grants — the
    /// BaseParams the item carries, plus defense and weapon damage, which live in their own Item
    /// columns rather than in the BaseParam array — is capped at the most that slot could grant at
    /// the synced item level. Each stat is capped independently against its own BaseParam's
    /// budget, so an item may keep some stats untouched while others are cut.
    ///
    /// `materia` covers what the melded materia add on top, already trimmed to each item's meld cap
    /// (see [`MateriaStats`]). The two rules are per item and mutually exclusive: an item that is
    /// being synced *down* loses its materia entirely — they are not scaled and not capped, they
    /// simply do not count — while an item that isn't synced keeps its materia at their full
    /// trimmed value, which may legitimately push the slot's total past the synced cap.
    pub fn calculate_stat_across_all_items(
        &mut self,
        equipped: &EquippedStorage,
        item_level_caps: Option<&ItemLevelCaps>,
        materia: Option<&MateriaStats>,
    ) {
        for i in 0..equipped.max_slots() {
            let slot = equipped.get_slot(i as u16);
            if slot.quantity == 0 {
                continue;
            }

            let cap_slot = EquipSlot::from_repr(i as u16)
                .and_then(|equip_slot| CapSlot::resolve(equip_slot, slot.equip_slot_category));

            // Caps this item's contribution to a BaseParam, if we're syncing down and that param
            // has a budget at the synced item level.
            let sync = |base_param_id: u8, value: u32| -> u32 {
                let (Some(caps), Some(cap_slot)) = (item_level_caps, cap_slot) else {
                    return value;
                };
                match caps.get(cap_slot, base_param_id) {
                    Some(cap) => value.min(cap as u32),
                    None => value,
                }
            };

            // The item's own base params, with its high-quality bonus already summed in.
            //
            // Defense, magic defense and weapon damage live in their own Item columns rather than
            // in the BaseParam array, but they are the same BaseParams to `get_mut` and to the
            // sync clamp -- and an HQ bonus can target them through BaseParamSpecial. So they are
            // merged in here and every param is clamped exactly once: `sync` caps what the item
            // contributes to a param *as a whole*, and `min(a, cap) + min(b, cap)` is not
            // `min(a + b, cap)`. Accumulating a param in two places would let an HQ item through
            // at up to twice its synced cap.
            //
            // Merging is safe in both directions: no Item row carries 12/13/21/24 in the normal
            // BaseParam array, so these four can never collide with an entry already in the list.
            //
            // Weapon base damage drives calc_physical/magical_damage; only the equipped weapon
            // carries a non-zero value, so summing across slots is fine.
            let mut contributions = slot.base_param_contributions();
            merge_contribution(&mut contributions, BASE_PARAM_DEFENSE, slot.defense as i32);
            merge_contribution(
                &mut contributions,
                BASE_PARAM_MAGIC_DEFENSE,
                slot.magic_defense as i32,
            );
            merge_contribution(
                &mut contributions,
                BASE_PARAM_PHYSICAL_DAMAGE,
                slot.weapon_damage_phys as i32,
            );
            merge_contribution(
                &mut contributions,
                BASE_PARAM_MAGICAL_DAMAGE,
                slot.weapon_damage_mag as i32,
            );

            for (param_id, value) in contributions {
                // No Item row in the sheet carries a negative BaseParamValue, so this clamp only
                // guards against a bad read rather than a real case.
                let value = value.max(0) as u32;
                *self.get_mut(param_id) += sync(param_id, value);
            }

            // Under item level sync a synced-down item's melded materia are dropped entirely --
            // not scaled and not capped. An unsynced item's materia apply at their full
            // already-trimmed value, which may legitimately exceed the sync cap, so they must
            // NOT go through `sync`.
            let synced_down =
                item_level_caps.is_some_and(|caps| slot.item_level > caps.item_level());
            if !synced_down && let Some(materia) = materia {
                for (param_id, value) in materia.contributions_for(i as u16) {
                    *self.get_mut(*param_id) += *value;
                }
            }
        }
    }
}

impl UserData for BaseParameters {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("calc_physical_damage", |_, this, potency: u32| {
            Ok(this.calc_physical_damage(potency))
        });
        methods.add_method("calc_magical_damage", |_, this, potency: u32| {
            Ok(this.calc_magical_damage(potency))
        });
        methods.add_method("calc_heal_amount", |_, this, potency: u32| {
            Ok(this.calc_heal_amount(potency))
        });
        methods.add_method("max_hp", |_, this, _: ()| Ok(this.hp));
    }
}

/// The two level fields Kawari puts in the client's `UpdateClassInfo`, returned as
/// `(current_level, class_level)`.
///
/// `current_level` populates the client's `PlayerState.CurrentLevel`, which its level-sync display
/// (item-level-sync panel, crit%/GCD/DH) reads — so under sync it must be the EFFECTIVE (synced)
/// level, matching retail. `class_level` feeds the client's `ClassJobLevels[]` EXP/progression
/// array and must stay the TRUE level or the level/EXP bar corrupts.
fn class_info_levels(synced_level: Option<u8>, true_level: u16) -> (u16, u16) {
    (
        effective_level(synced_level, true_level as u8) as u16,
        true_level,
    )
}

impl ZoneConnection {
    pub async fn update_class_info(&mut self) {
        let ipc;
        {
            let game_data = self.gamedata.lock();

            let true_level = self.current_level(&game_data);
            let (current_level, class_level) = class_info_levels(self.synced_level, true_level);

            ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateClassInfo(UpdateClassInfo {
                class_id: self.player_data.classjob.current_class as u8,
                class_level,
                current_level,
                synced_level: self.synced_level.unwrap_or_default() as u16,
                current_exp: self.current_exp(&game_data),
                ..Default::default()
            }));
        }
        self.send_ipc_self(ipc).await;

        // Update rested EXP so the bar doesn't reset.
        self.actor_control_self(ActorControlCategory::UpdateRestedExp {
            exp: self.player_data.classjob.rested_exp as u32,
        })
        .await;

        // Send this too, otherwise actions dependent on the gauge won't function
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ActorGauge {
            classjob_id: self.player_data.classjob.current_class as u8,
            data: 0,
        });
        self.send_ipc_self(ipc).await;
    }

    pub async fn finish_changing_class(&mut self) {
        // Play the VFX!
        self.broadcast_actor_control(ActorControlCategory::ClassJobChangeVFX {
            classjob_id: self.player_data.classjob.current_class as u32,
        })
        .await;

        let ipc;
        {
            let game_data = self.gamedata.lock();

            ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UnkClassRelated {
                classjob_id: self.player_data.classjob.current_class as u8,
                class_level: self.current_level(&game_data),
                current_level: self.current_level(&game_data),
            });
        }
        self.send_ipc_self(ipc).await;

        // Commit back our classjob data after changing. Done here so it gets picked up by any paths that change classjob.
        {
            let mut db = self.database.lock();
            db.commit_classjob_and_inventory(&self.player_data);
        }
    }

    /// Scaled parameters based on level and item level sync.
    pub fn base_parameters(&self) -> BaseParameters {
        let mut game_data = self.gamedata.lock();

        let modifiers = game_data
            .get_class_job_modifiers(self.player_data.classjob.current_class as u32)
            .expect("Failed to read param grow");

        let attributes = game_data
            .get_racial_base_attributes(self.player_data.subrace)
            .expect("Failed to read racial attributes");

        let level = self
            .synced_level
            .map(|x| x as u16)
            .unwrap_or(self.current_level(&game_data));

        let item_level_sync;
        {
            let param_grow = game_data
                .get_param_grow(level as u32)
                .expect("Failed to read param grow");

            if self.synced_level.is_some() {
                item_level_sync = Some(param_grow.ItemLevelSync);
            } else {
                item_level_sync = None;
            }
        }

        let item_level_caps =
            item_level_sync.map(|item_level| ItemLevelCaps::new(&mut game_data, item_level));

        // Resolved fresh on every recalculation rather than cached on the items, so a meld can
        // never be left describing stats the gear no longer grants.
        let materia = MateriaStats::new(&mut *game_data, &self.player_data.inventory.equipped);

        let param_grow = game_data
            .get_param_grow(level as u32)
            .expect("Failed to read param grow");

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_based_on_level(
            &attributes,
            level as u32,
            self.player_data.classjob.current_class as u8,
            &param_grow,
            &modifiers,
        );
        base_parameters.calculate_stat_across_all_items(
            &self.player_data.inventory.equipped,
            item_level_caps.as_ref(),
            Some(&materia),
        );
        base_parameters.calculate_potencies(level as u32, &param_grow, Some(&modifiers));

        base_parameters
    }

    /// Same as `base_parameters` but doesn't take into account level or item level sync.
    pub fn unscaled_base_parameters(&mut self) -> BaseParameters {
        let mut game_data = self.gamedata.lock();

        let modifiers = game_data
            .get_class_job_modifiers(self.player_data.classjob.current_class as u32)
            .expect("Failed to read param grow");

        let attributes = game_data
            .get_racial_base_attributes(self.player_data.subrace)
            .expect("Failed to read racial attributes");

        let level = self.current_level(&game_data);

        // Nothing is synced here, so every item keeps its materia.
        let materia = MateriaStats::new(&mut *game_data, &self.player_data.inventory.equipped);

        let param_grow = game_data
            .get_param_grow(level as u32)
            .expect("Failed to read param grow");

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_based_on_level(
            &attributes,
            level as u32,
            self.player_data.classjob.current_class as u8,
            &param_grow,
            &modifiers,
        );
        base_parameters.calculate_stat_across_all_items(
            &self.player_data.inventory.equipped,
            None,
            Some(&materia),
        );
        base_parameters.calculate_potencies(level as u32, &param_grow, Some(&modifiers));

        base_parameters
    }

    pub async fn send_stats(&mut self) {
        let base_parameters = self.base_parameters();
        let unscaled_base_parameters = self.unscaled_base_parameters();

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::PlayerStats(PlayerStats {
            strength: base_parameters.strength,
            dexterity: base_parameters.dexterity,
            vitality: base_parameters.vitality,
            intelligence: base_parameters.intelligence,
            mind: base_parameters.mind,
            piety: base_parameters.piety,
            hp: base_parameters.hp,
            mp: base_parameters.mp,
            tp: base_parameters.tp,
            gp: base_parameters.gp,
            cp: base_parameters.cp,
            delay: base_parameters.delay,
            tenacity: base_parameters.tenacity,
            attack_power: base_parameters.attack_power,
            defense: base_parameters.defense,
            direct_hit_rate: base_parameters.direct_hit_rate,
            evasion: base_parameters.evasion,
            magic_defense: base_parameters.magic_defense,
            critical_hit: base_parameters.critical_hit,
            attack_magic_potency: base_parameters.attack_magic_potency,
            healing_magic_potency: base_parameters.healing_magic_potency,
            elemental_bonus: base_parameters.elemental_bonus,
            determination: base_parameters.determination,
            skill_speed: base_parameters.skill_speed,
            spell_speed: base_parameters.spell_speed,
            haste: base_parameters.haste,
            craftmanship: base_parameters.craftmanship,
            control: base_parameters.control,
            gathering: base_parameters.gathering,
            perception: base_parameters.perception,
            base_strength: unscaled_base_parameters.strength,
            base_dexterity: unscaled_base_parameters.dexterity,
            base_vitality: unscaled_base_parameters.vitality,
            base_intelligence: unscaled_base_parameters.intelligence,
            base_mind: unscaled_base_parameters.mind,
            base_piety: unscaled_base_parameters.piety,
        }));
        self.send_ipc_self(ipc).await;

        self.update_server_stats().await;
    }

    /// Inform the server of new updated level/HP/MP stats.
    async fn update_server_stats(&mut self) {
        let current_level;
        {
            let gamedata = self.gamedata.lock();
            current_level = self.current_level(&gamedata);
        }

        let base_parameters = self.base_parameters();

        self.handle
            .send(ToServer::SetNewStatValues(
                self.player_data.character.actor_id,
                effective_level(self.synced_level, current_level as u8),
                self.player_data.classjob.current_class as u8,
                base_parameters,
            ))
            .await;
    }

    pub fn current_level(&self, game_data: &GameData) -> u16 {
        let index = game_data
            .get_exp_array_index(self.player_data.classjob.current_class as u16)
            .expect("Failed to find EXP array index?!");
        self.player_data.classjob.levels.0[index as usize]
    }

    pub fn set_current_level(&mut self, level: u16) {
        self.set_level_for(self.player_data.classjob.current_class as u8, level);
    }

    pub fn set_level_for(&mut self, classjob_id: u8, level: u16) {
        let game_data = self.gamedata.lock();

        let index = game_data
            .get_exp_array_index(classjob_id as u16)
            .expect("Failed to find EXP array index?!");
        self.player_data.classjob.levels.0[index as usize] = level;
    }

    /// Returns the current level stored in the EXP array slot for the given classjob.
    /// Note: some classes/jobs share an EXP array slot (e.g. ACN/SMN/SCH all use index 18),
    /// so this reflects the shared level for those.
    pub fn level_for(&self, classjob_id: u8) -> u16 {
        let game_data = self.gamedata.lock();

        let index = game_data
            .get_exp_array_index(classjob_id as u16)
            .expect("Failed to find EXP array index?!");
        self.player_data.classjob.levels.0[index as usize]
    }

    pub fn current_exp(&self, game_data: &GameData) -> i32 {
        let index = game_data
            .get_exp_array_index(self.player_data.classjob.current_class as u16)
            .expect("Failed to find EXP array index?!");
        self.player_data.classjob.exp.0[index as usize]
    }

    pub fn set_current_exp(&mut self, exp: i32) {
        let game_data = self.gamedata.lock();

        let index = game_data
            .get_exp_array_index(self.player_data.classjob.current_class as u16)
            .expect("Failed to find EXP array index?!");
        self.player_data.classjob.exp.0[index as usize] = exp;
    }

    pub async fn update_hp_mp(&mut self, actor_id: ObjectId, hp: u32, mp: u16) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::UpdateHpMpTp { hp, mp, unk: 0 });

        self.send_ipc_from(actor_id, ipc).await;
    }

    /// Adds EXP to the current classjob, handles level-up and so on.
    pub async fn add_exp(&mut self, exp: i32) {
        let (bonus_percent, exp) = self.use_exp_bonus(exp);

        self.actor_control_self(ActorControlCategory::EXPFloatingMessage {
            classjob_id: self.player_data.classjob.current_class as u32,
            amount: exp as u32,
            bonus_percent: bonus_percent as u32,
        })
        .await;

        self.send_rested_exp().await; // If the EXP bonus was used, we need to update in case.

        let index;
        let mut level_up = 0;
        {
            let mut game_data = self.gamedata.lock();

            index = game_data
                .get_exp_array_index(self.player_data.classjob.current_class as u16)
                .expect("Failed to find EXP array index?!");

            self.player_data.classjob.exp.0[index as usize] += exp;

            // Keep going until we have leftover EXP
            loop {
                let curr_exp = self.player_data.classjob.exp.0[index as usize];
                let max_exp = game_data
                    .get_max_exp(self.player_data.classjob.levels.0[index as usize] as u32);
                let difference = curr_exp - max_exp;
                if difference >= 0 {
                    level_up += 1;
                    self.player_data.classjob.exp.0[index as usize] = difference;
                } else {
                    break;
                }
            }
        }

        if level_up > 0 {
            let curr_level = self.player_data.classjob.levels.0[index as usize];
            let new_level = curr_level + level_up;
            self.set_current_level(new_level);

            self.actor_control_self(ActorControlCategory::LevelUpMessage {
                classjob_id: self.player_data.classjob.current_class as u32,
                level: new_level as u32,
                unk2: 0,
                unk3: 0,
            })
            .await;
        }

        self.send_stats().await;
        self.update_class_info().await;
    }

    /// The number of seconds to add to the rested EXP bonus.
    pub async fn add_rested_exp_seconds(&mut self, seconds: i32) {
        self.player_data.classjob.rested_exp += seconds;
        self.player_data.classjob.rested_exp = self
            .player_data
            .classjob
            .rested_exp
            .clamp(0, MAXIMUM_RESTED_EXP);

        self.send_rested_exp().await;
    }

    /// Sends the rested EXP bonus to the client.
    pub async fn send_rested_exp(&mut self) {
        self.actor_control_self(ActorControlCategory::UpdateRestedExp {
            exp: self.player_data.classjob.rested_exp as u32,
        })
        .await;
    }

    /// "Use" an EXP bonus for the specified amount. Returns the bonus percentage and new amount of EXP earned.
    /// Remember to update rested EXP when calling this function!
    pub fn use_exp_bonus(&mut self, exp: i32) -> (i32, i32) {
        let mut bonus_percent = 0;

        // TODO: Please write a unit test for this
        if self.player_data.classjob.rested_exp > 0 {
            // Here is where the fun calculations come in for rested EXP.
            // We need to basically convert EXP to "seconds" - which is what rested EXP is counted in.

            let mut gamedata = self.gamedata.lock();
            let current_level = self.current_level(&gamedata);

            // This is the size of the bar in EXP.
            let max_exp = gamedata.get_max_exp(current_level as u32);
            assert!(max_exp > 0);

            // This is the size of the bar in seconds.
            let max_seconds = 201600;

            // Get a relative amount of the bar.
            let new_exp_relative = exp as f32 / max_exp as f32;

            // Get the amount of seconds to remove from the rested EXP bonus.
            let seconds_to_remove = new_exp_relative * max_seconds as f32;
            self.player_data.classjob.rested_exp -= seconds_to_remove.round() as i32;

            // Add that sweet EXP bonus.
            bonus_percent += 50;
        }

        // Add EXP bonus on top of already earned EXP.
        let exp = exp + (exp * (bonus_percent as f32 / 100.0).round() as i32);

        (bonus_percent, exp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamedata::cap_for;
    use crate::inventory::Item;

    /// A gear piece carrying the four BaseParams a physical DPS cares about. `equip_slot_category`
    /// is the item's EquipSlotCategory row id.
    ///
    /// Note that `item_level` is left at 0, which is *below* every sync level — so a piece built by
    /// this helper alone never counts as synced down and always keeps its materia. Any test about
    /// sync-vs-materia has to set `item_level` explicitly, the way `il700_body_with_crit_materia`
    /// does. The per-param clamping in `ItemLevelCaps` is unaffected either way.
    fn gear(
        equip_slot_category: u8,
        dexterity: i16,
        vitality: i16,
        critical_hit: i16,
        determination: i16,
    ) -> Item {
        Item {
            quantity: 1,
            item_id: 1,
            equip_slot_category,
            base_param_ids: [2, 3, 27, 44, 0, 0],
            base_param_values: [dexterity, vitality, critical_hit, determination, 0, 0],
            ..Default::default()
        }
    }

    /// The il-133 cap table — what a level 54 sync clamps to, per `ParamGrow[54].ItemLevelSync`.
    ///
    /// Built the way `ItemLevelCaps::new` builds the real thing: each param's `ItemLevel[133]`
    /// budget crossed with every slot's `BaseParam[param].<Slot>Percent` share, run through the
    /// same `cap_for`. Both inputs are read from the sheets, and the caps are derived here rather
    /// than written out, so this fixture exercises the formula instead of restating its results as
    /// constants that could drift from it.
    ///
    /// Every slot gets an entry for every param modelled, including the 0‰ ones — those cap to 0,
    /// which is what the builder stores and is distinct from the `None` ("no budget, leave alone")
    /// that an unmodelled param returns. Only the params the test gear carries are modelled; the
    /// rest of `SYNCED_BASE_PARAMS` is inert here because no fixture item has those stats.
    fn il133_caps() -> ItemLevelCaps {
        // `BaseParam[param].<Slot>Percent`, per-mille. The main stats and substats share one
        // profile; weapon damage is weapon-only and defense is armour-only.
        let stat: [(CapSlot, u16); 13] = [
            (CapSlot::OneHandWeapon, 100),
            (CapSlot::TwoHandWeapon, 140),
            (CapSlot::OffHand, 40),
            (CapSlot::Head, 85),
            (CapSlot::Chest, 135),
            (CapSlot::Hands, 85),
            (CapSlot::Waist, 0),
            (CapSlot::Legs, 135),
            (CapSlot::Feet, 85),
            (CapSlot::Earring, 67),
            (CapSlot::Necklace, 67),
            (CapSlot::Bracelet, 67),
            (CapSlot::Ring, 67),
        ];
        let damage: [(CapSlot, u16); 13] = [
            (CapSlot::OneHandWeapon, 1000),
            (CapSlot::TwoHandWeapon, 1000),
            (CapSlot::OffHand, 0),
            (CapSlot::Head, 0),
            (CapSlot::Chest, 0),
            (CapSlot::Hands, 0),
            (CapSlot::Waist, 0),
            (CapSlot::Legs, 0),
            (CapSlot::Feet, 0),
            (CapSlot::Earring, 0),
            (CapSlot::Necklace, 0),
            (CapSlot::Bracelet, 0),
            (CapSlot::Ring, 0),
        ];
        let defense: [(CapSlot, u16); 13] = [
            (CapSlot::OneHandWeapon, 0),
            (CapSlot::TwoHandWeapon, 0),
            (CapSlot::OffHand, 0),
            (CapSlot::Head, 176),
            (CapSlot::Chest, 236),
            (CapSlot::Hands, 176),
            (CapSlot::Waist, 0),
            (CapSlot::Legs, 236),
            (CapSlot::Feet, 176),
            (CapSlot::Earring, 0),
            (CapSlot::Necklace, 0),
            (CapSlot::Bracelet, 0),
            (CapSlot::Ring, 0),
        ];

        // `(BaseParam id, ItemLevel[133] budget, percent profile)`.
        let params = [
            (2u8, 391u16, &stat), // Dexterity
            (3, 417, &stat),      // Vitality
            (27, 370, &stat),     // Critical Hit
            (44, 370, &stat),     // Determination
            (12, 65, &damage),    // Physical Damage
            (21, 877, &defense),  // Defense
            (24, 877, &defense),  // Magic Defense
        ];

        let mut caps = ItemLevelCaps::default();
        for (base_param_id, budget, percents) in params {
            for (slot, percent) in percents {
                caps.set(*slot, base_param_id, cap_for(budget, *percent));
            }
        }

        caps
    }

    /// A full il-700 Bard set. Every value is the most that slot may carry at il-700, i.e.
    /// `round(ItemLevel[700][param] * BaseParam[param].<Slot>Percent / 1000)`, with
    /// `ItemLevel[700]` = Dexterity 3714, Vitality 3798, CriticalHit 2576, Determination 2576,
    /// PhysicalDamage 139, Defense 6116, MagicDefense 6116.
    fn il700_bard_set() -> EquippedStorage {
        EquippedStorage {
            main_hand: il700_bow(),
            head: Item {
                defense: 1076,
                magic_defense: 1076,
                ..gear(3, 316, 323, 219, 219)
            },
            body: Item {
                defense: 1443,
                magic_defense: 1443,
                ..gear(4, 501, 513, 348, 348)
            },
            hands: Item {
                defense: 1076,
                magic_defense: 1076,
                ..gear(5, 316, 323, 219, 219)
            },
            legs: Item {
                defense: 1443,
                magic_defense: 1443,
                ..gear(7, 501, 513, 348, 348)
            },
            feet: Item {
                defense: 1076,
                magic_defense: 1076,
                ..gear(8, 316, 323, 219, 219)
            },
            ears: gear(9, 249, 254, 173, 173),
            neck: gear(10, 249, 254, 173, 173),
            wrists: gear(11, 249, 254, 173, 173),
            right_ring: gear(12, 249, 254, 173, 173),
            left_ring: gear(12, 249, 254, 173, 173),
            ..Default::default()
        }
    }

    /// An il-700 bow: a two-handed weapon, EquipSlotCategory 13.
    fn il700_bow() -> Item {
        Item {
            weapon_damage_phys: 139,
            ..gear(13, 520, 532, 361, 361)
        }
    }

    #[test]
    fn test_unsynced_gear_is_not_capped() {
        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(&il700_bard_set(), None, None);

        assert_eq!(base_parameters.dexterity, 3715);
        assert_eq!(base_parameters.vitality, 3797);
        assert_eq!(base_parameters.critical_hit, 2579);
        assert_eq!(base_parameters.determination, 2579);
        assert_eq!(base_parameters.physical_damage, 139);
        assert_eq!(base_parameters.defense, 6114);
        assert_eq!(base_parameters.magic_defense, 6114);
    }

    /// Syncing to level 54 clamps gear to il-133 (`ParamGrow[54].ItemLevelSync`). Each stat is
    /// capped independently at what its slot could carry at il-133 — see `ItemLevelCaps`:
    ///
    /// | param | ItemLevel[133] | 2H (140‰) | head/hands/feet (85‰) | chest/legs (135‰) | accessory (67‰) |
    /// |---|---|---|---|---|---|
    /// | Dexterity (2)      | 391 | 55 | 33 | 53 | 26 |
    /// | Vitality (3)       | 417 | 58 | 35 | 56 | 28 |
    /// | Critical Hit (27)  | 370 | 52 | 31 | 50 | 25 |
    /// | Determination (44) | 370 | 52 | 31 | 50 | 25 |
    ///
    /// Summed over the 11 filled slots that gives 390 / 415 / 370 / 370. Defense (21) and magic
    /// defense (24) budget 877 at 176‰ (head/hands/feet) and 236‰ (chest/legs) — 154 and 207,
    /// summing to 876 — and nothing on a weapon. Physical damage (12) budgets `ItemLevel[133]`'s
    /// 65 outright, since BaseParam[12] is 1000‰ on the weapon slots.
    #[test]
    fn test_synced_gear_is_capped_per_stat_and_slot() {
        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(
            &il700_bard_set(),
            Some(&il133_caps()),
            None,
        );

        assert_eq!(base_parameters.dexterity, 390);
        assert_eq!(base_parameters.vitality, 415);
        assert_eq!(base_parameters.critical_hit, 370);
        assert_eq!(base_parameters.determination, 370);
        assert_eq!(base_parameters.physical_damage, 65);
        assert_eq!(base_parameters.defense, 876);
        assert_eq!(base_parameters.magic_defense, 876);
    }

    /// Weapon damage lives in its own Item column, not in the BaseParam array, so it has to be
    /// clamped explicitly — it drives every damage calculation and is the single biggest lever on
    /// a synced player's DPS.
    #[test]
    fn test_synced_weapon_damage_is_capped() {
        let equipped = EquippedStorage {
            main_hand: il700_bow(),
            ..Default::default()
        };

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(&equipped, Some(&il133_caps()), None);

        assert_eq!(base_parameters.physical_damage, 65);
    }

    /// Bard, the job the gear fixtures above are built for.
    const BARD: u8 = 23;

    /// Bard's `ClassJob` modifiers. Only the ones the maths below reads are meaningful.
    fn bard_modifiers() -> Modifiers {
        Modifiers {
            hp: 105,
            mp: 100,
            strength: 90,
            vitality: 100,
            dexterity: 115,
            intelligence: 85,
            mind: 80,
            piety: 100,
        }
    }

    /// Warrior's `ClassJob` modifiers (tank: modifierHitPoints 145 ≥ 140).
    fn warrior_modifiers() -> Modifiers {
        Modifiers {
            hp: 145,
            mp: 100,
            strength: 105,
            vitality: 110,
            dexterity: 95,
            intelligence: 40,
            mind: 55,
            piety: 100,
        }
    }

    /// A character with no tribe/race adjustments, so the level bases stand alone.
    fn no_attributes() -> Attributes {
        Attributes {
            strength: 0,
            dexterity: 0,
            vitality: 0,
            intelligence: 0,
            mind: 0,
            piety: 0,
        }
    }

    /// A synthetic `ParamGrow` row. Only `BaseSpeed`, `LevelModifier`, `HpModifier` and
    /// `MpModifier` are read by the stat maths; the rest of the sheet's 15 columns are inert here.
    ///
    /// Note the sheet has **no MAIN column** — `BaseSpeed` is the SUB constant and `LevelModifier`
    /// is DIV, which is why the MAIN values these tests pin come from `LEVEL_MODIFIERS` instead.
    fn param_grow(base_speed: i32, level_modifier: i32, hp_modifier: u16) -> ParamGrowRow {
        ParamGrowRow {
            BaseSpeed: base_speed,
            LevelModifier: level_modifier,
            HpModifier: hp_modifier,
            MpModifier: 10000,
            ExpToNext: 0,
            HuntingLogExpReward: 0,
            MonsterNoteSeals: 0,
            ScaledQuestXP: 0,
            ItemLevelSync: 0,
            ProperDungeon: 0,
            ProperGuildOrder: 0,
            CraftingLevel: 0,
            AdditionalActions: 0,
            ApplyAction: 0,
            QuestExpModifier: 0,
        }
    }

    /// `ParamGrow[54]` — the level a `ClassJobLevelSync = 54` duty syncs to. MAIN 209 < SUB 346.
    fn param_grow_54() -> ParamGrowRow {
        param_grow(346, 444, 1386)
    }

    /// `ParamGrow[100]` — MAIN 440 > SUB 420, i.e. the crossover against level 54.
    fn param_grow_100() -> ParamGrowRow {
        param_grow(420, 2780, 4205)
    }

    /// Carpenter — the first Disciple of the Hand.
    const CARPENTER: u8 = 8;
    /// Culinarian — the last Disciple of the Hand.
    const CULINARIAN: u8 = 15;
    /// Miner — the first Disciple of the Land.
    const MINER: u8 = 16;
    /// Fisher — the last Disciple of the Land.
    const FISHER: u8 = 18;

    /// A Disciple's `ClassJob` modifiers. Crafters and gatherers all carry 100 for mana.
    fn disciple_modifiers() -> Modifiers {
        Modifiers {
            hp: 100,
            mp: 100,
            strength: 90,
            vitality: 105,
            dexterity: 100,
            intelligence: 90,
            mind: 90,
            piety: 100,
        }
    }

    fn leveled(classjob_id: u8) -> BaseParameters {
        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_based_on_level(
            &no_attributes(),
            100,
            classjob_id,
            &param_grow_100(),
            &disciple_modifiers(),
        );
        base_parameters
    }

    /// Base CP and GP are seeded for every job, including battle jobs, which is what retail sends.
    /// Gating them on the job would leave a stale zero behind on a job change.
    ///
    /// Neither value scales with level — only GP *regeneration* does — so both levels agree.
    #[test]
    fn test_base_cp_and_gp_are_seeded_for_every_job() {
        for level in [54u32, 100] {
            let param_grow = if level == 54 {
                param_grow_54()
            } else {
                param_grow_100()
            };
            for classjob_id in [BARD, CARPENTER, MINER] {
                let mut base_parameters = BaseParameters::default();
                base_parameters.calculate_based_on_level(
                    &no_attributes(),
                    level,
                    classjob_id,
                    &param_grow,
                    &disciple_modifiers(),
                );

                assert_eq!(base_parameters.cp, 180, "level {level} job {classjob_id} cp");
                assert_eq!(base_parameters.gp, 400, "level {level} job {classjob_id} gp");
            }
        }
    }

    /// `resource_points` picks the pool the client actually draws in place of the MP bar. Returning
    /// `mp` for a Disciple is the defect this fixes: it put `ParamGrow.MpModifier`, 10000, on every
    /// crafter's and gatherer's CP/GP bar.
    ///
    /// Both Disciple ranges are checked at both ends, since they are contiguous id ranges and an
    /// off-by-one would silently hand Culinarian or Fisher the wrong pool.
    #[test]
    fn test_resource_points_follows_the_jobs_own_pool() {
        for classjob_id in [CARPENTER, CULINARIAN] {
            let parameters = leveled(classjob_id);
            assert_eq!(
                parameters.resource_points(),
                180,
                "Disciple of the Hand {classjob_id} draws CP"
            );
        }

        for classjob_id in [MINER, FISHER] {
            let parameters = leveled(classjob_id);
            assert_eq!(
                parameters.resource_points(),
                400,
                "Disciple of the Land {classjob_id} draws GP"
            );
        }

        // A battle job keeps MP, and the neighbours just outside both ranges must not be captured.
        for classjob_id in [BARD, 7, 19] {
            let parameters = leveled(classjob_id);
            assert_eq!(
                parameters.resource_points(),
                10000,
                "job {classjob_id} draws MP"
            );
        }
    }

    /// Gear and materia reach CP through the ordinary BaseParam path (id 11), on top of the base.
    /// 661 Item rows carry BaseParam 11, so this is the normal case for crafting gear rather than
    /// an edge case.
    #[test]
    fn test_gear_cp_adds_on_top_of_the_base() {
        let equipped = EquippedStorage {
            body: Item {
                quantity: 1,
                item_id: 1,
                equip_slot_category: 4,
                base_param_ids: [11, 0, 0, 0, 0, 0],
                base_param_values: [24, 0, 0, 0, 0, 0],
                ..Default::default()
            },
            ..Default::default()
        };

        let mut parameters = leveled(CARPENTER);
        parameters.calculate_stat_across_all_items(&equipped, None, None);

        assert_eq!(parameters.cp, 180 + 24);
        assert_eq!(parameters.resource_points(), 204);
    }

    /// Crit, direct hit and determination each need a level base, exactly as the damage maths
    /// already assumes: `expected_crit_rate` and `direct_hit_rate_bonus` baseline them on SUB,
    /// while `determination_factor` baselines on **MAIN**.
    ///
    /// That asymmetry is retail's, not a typo — `CalcCritRate`/`CalcDh` break points start at each
    /// stat's SUB floor (420 at level 100) while `CalcDet` starts at MAIN (440). Without these
    /// bases the stats are pure gear sums, and `positive_scaled_bonus` returns 0 below the
    /// baseline, so crit silently pins to its 5.0% floor and direct hit to 0% — which is exactly
    /// what makes the defect look like correct behaviour.
    #[test]
    fn test_level_bases_seed_crit_direct_hit_and_determination() {
        for (level, param_grow, sub, main) in [
            (54u32, param_grow_54(), 346u32, 209u32),
            (100, param_grow_100(), 420, 440),
        ] {
            let mut base_parameters = BaseParameters::default();
            base_parameters.calculate_based_on_level(
                &no_attributes(),
                level,
                BARD,
                &param_grow,
                &bard_modifiers(),
            );

            let level_modifier = level_modifier_for(level);
            assert_eq!(u32::from(level_modifier.sub), sub, "level {level} sub");
            assert_eq!(u32::from(level_modifier.main), main, "level {level} main");

            assert_eq!(
                base_parameters.critical_hit, sub,
                "level {level} critical hit baselines on sub"
            );
            assert_eq!(
                base_parameters.direct_hit_rate, sub,
                "level {level} direct hit baselines on sub"
            );
            // Determination is the odd one out. Do not "tidy" this into sub.
            assert_eq!(
                base_parameters.determination, main,
                "level {level} determination baselines on MAIN, not sub"
            );
        }
    }

    /// The main stats seed from MAIN, which `ParamGrow` does not carry at all — `BaseSpeed` is SUB.
    ///
    /// Levels 54 and 100 are pinned together because MAIN and SUB **cross over** between them
    /// (209 < 346 at 54, 440 > 420 at 100), so no single swap of one constant for the other can
    /// satisfy both. The speed substats and tenacity genuinely do want `BaseSpeed`; that is what
    /// the column is named after, and it is asserted here so the two never get conflated again.
    #[test]
    fn test_main_stats_seed_from_main_not_base_speed() {
        for (level, param_grow) in [(54u32, param_grow_54()), (100, param_grow_100())] {
            let modifiers = bard_modifiers();
            let mut base_parameters = BaseParameters::default();
            base_parameters.calculate_based_on_level(
                &no_attributes(),
                level,
                BARD,
                &param_grow,
                &modifiers,
            );

            let level_modifier = level_modifier_for(level);
            assert_ne!(
                level_modifier.main, level_modifier.sub,
                "level {level} must tell main and sub apart for this test to bite"
            );
            assert_eq!(
                u32::from(level_modifier.sub),
                param_grow.BaseSpeed as u32,
                "level {level}: BaseSpeed is the SUB constant"
            );

            let main = u32::from(level_modifier.main);
            let sub = u32::from(level_modifier.sub);

            // Vitality's modifier is 100, so its base is MAIN outright.
            assert_eq!(base_parameters.vitality, main, "level {level} vitality");

            assert_eq!(
                base_parameters.dexterity,
                modifiers.apply_to(2, main),
                "level {level} dexterity seeds from main"
            );
            assert_ne!(
                base_parameters.dexterity,
                modifiers.apply_to(2, sub),
                "level {level} dexterity must not seed from BaseSpeed/sub"
            );

            // The speed substats and tenacity keep BaseSpeed — that is what it is the base of.
            assert_eq!(
                base_parameters.skill_speed, sub,
                "level {level} skill speed"
            );
            assert_eq!(
                base_parameters.spell_speed, sub,
                "level {level} spell speed"
            );
            assert_eq!(base_parameters.tenacity, sub, "level {level} tenacity");
        }
    }

    /// HP excess is measured against raw MAIN. For a BRD (`vitMod = 100`) the seed equals MAIN, so
    /// gear-only excess is exact: `(MAIN + 414) − MAIN = 414`. Both the seed and the HP subtrahend
    /// must read MAIN from `level_modifier_for` (not SUB/`BaseSpeed`); if either drifts, excess —
    /// and HP — move with it.
    #[test]
    fn test_hp_is_unaffected_by_the_main_stat_base() {
        let param_grow = param_grow_54();
        let modifiers = bard_modifiers();

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_based_on_level(
            &no_attributes(),
            54,
            BARD,
            &param_grow,
            &modifiers,
        );

        // Stand in for il-133-clamped gear rather than re-deriving the gear sum here.
        // Excess vitality over MAIN is 414 — the same excess the retail L54 BRD anchor used.
        base_parameters.vitality += 414;
        base_parameters.calculate_potencies(54, &param_grow, Some(&modifiers));

        // ⌊1386 × 1.05⌋ + ⌊414 × 12.036⌋ = 1455 + 4982 = 6437
        // (retail reading 6438 differs by 1 HP only because the live k was 12.036232 before
        // rounding the table to 3 d.p.; the MAIN-constant identity is what this test pins.)
        assert_eq!(base_parameters.hp, 6437);
    }

    /// A tank's 110% vit mod must grant HP: seed is `MAIN×1.1`, subtrahend is raw MAIN, so a
    /// naked WAR has excess `0.1×MAIN` and non-zero vit-term HP. Multiplying vitMod into the
    /// subtrahend would cancel that contribution.
    #[test]
    fn test_tank_job_vit_mod_contributes_hp() {
        let param_grow = param_grow_54();
        let modifiers = warrior_modifiers();

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_based_on_level(
            &no_attributes(),
            54,
            21, // WAR
            &param_grow,
            &modifiers,
        );
        // No gear. Seeded vit = floor(MAIN × 1.10) via apply_to; excess over raw MAIN is ~0.1×MAIN.
        base_parameters.calculate_potencies(54, &param_grow, Some(&modifiers));

        let main = u32::from(super::level_modifier_for(54).main);
        let seeded = base_parameters.vitality;
        assert!(seeded > main, "WAR vit mod must raise seeded vitality above MAIN");
        // ⌊1386 × 1.45⌋ + ⌊(seeded − MAIN) × 16.482⌋
        let expected = ((1386.0 * 1.45) as f64).floor() as u32
            + (((seeded - main) as f64) * 16.482).floor() as u32;
        assert_eq!(base_parameters.hp, expected);
        assert!(
            base_parameters.hp > ((1386.0 * 1.45) as f64).floor() as u32,
            "naked tank HP must exceed the HpMod term alone"
        );
    }

    /// The calibrated k table: knots exact, mid-segment linear, above-100 clamped, below-54
    /// extrapolated from the first segment and floored at 2.0.
    #[test]
    fn test_hp_per_vitality_interpolates_and_clamps() {
        assert!((super::hp_per_vitality(54, false) - 12.036).abs() < 1e-9);
        assert!((super::hp_per_vitality(100, false) - 30.066).abs() < 1e-9);
        assert!((super::hp_per_vitality(54, true) - 16.482).abs() < 1e-9);
        assert!((super::hp_per_vitality(100, true) - 42.950).abs() < 1e-9);

        // Midpoint of the 70→80 non-tank segment: (13.312 + 18.025) / 2.
        let k75 = super::hp_per_vitality(75, false);
        assert!((k75 - (13.312 + 18.025) / 2.0).abs() < 1e-9);

        // Above the last knot clamps; tank and non-tank stay distinct.
        assert!((super::hp_per_vitality(101, false) - 30.066).abs() < 1e-9);
        assert!((super::hp_per_vitality(101, true) - 42.950).abs() < 1e-9);

        // Far below the first knot: first-segment slope is small and positive, still ≥ 2.0.
        assert!(super::hp_per_vitality(1, false) >= 2.0);
        assert!(super::hp_per_vitality(1, false) < super::hp_per_vitality(54, false));
    }

    /// Pin the BRD (non-tank) curve at the excess values taken from the 2026-07-16 retail anchors.
    /// Excess is aligned so the test is independent of which MAIN table seeded the vit total.
    #[test]
    fn test_hp_matches_retail_brd_excess_anchors() {
        // (level, HpModifier, excess_vit, expected_hp)
        // expected = ⌊HpMod×1.05⌋ + ⌊excess × k_nontank⌋ with the 3 d.p. table.
        let anchors = [
            (54u32, 1386u16, 414u32, 6437u32),
            (60, 1767, 506, 8102),
            (70, 2275, 876, 14049),
            (80, 3121, 1519, 30656), // 3 d.p. table: retail was 30657 (−1)
            (90, 3661, 1907, 49489), // retail 49490 (−1)
            (100, 4205, 6315, 194281),
        ];
        let modifiers = bard_modifiers();
        for (level, hp_mod, excess, expected_hp) in anchors {
            let param_grow = param_grow(0, 0, hp_mod);
            let main = u32::from(super::level_modifier_for(level).main);
            let mut base_parameters = BaseParameters::default();
            base_parameters.vitality = main + excess;
            base_parameters.calculate_potencies(level, &param_grow, Some(&modifiers));
            assert_eq!(
                base_parameters.hp, expected_hp,
                "BRD L{level}: vit={} excess={excess}",
                main + excess
            );
        }
    }

    /// Pin the WAR (tank) curve to absolute retail HP at the calibrated excess values.
    /// `modifierHitPoints = 145` selects the tank table; subtrahend is raw MAIN (not MAIN×1.1).
    #[test]
    fn test_hp_matches_retail_war_excess_anchors() {
        // (level, HpModifier, excess_over_MAIN, expected_hp)
        // expected = ⌊HpMod×1.45⌋ + ⌊excess × k_tank⌋ with the 3 d.p. table.
        // Excess values are retail VIT − community MAIN (same inversion that produced k).
        // 3 d.p. rounding: retail readings sit 0…−3 HP above these expectations.
        let anchors = [
            (54u32, 1386u16, 434u32, 9162u32),  // retail 9162
            (60, 1767, 528, 11414),             // retail 11415 (−1)
            (70, 2275, 895, 19290),             // retail 19291 (−1)
            (80, 3121, 1536, 43757),            // retail 43757
            (90, 3661, 1925, 70954),            // retail 70955 (−1)
            (100, 4205, 5997, 263668),          // retail 263671 (−3)
        ];
        let modifiers = warrior_modifiers();
        for (level, hp_mod, excess, expected_hp) in anchors {
            let param_grow = param_grow(0, 0, hp_mod);
            let main = u32::from(super::level_modifier_for(level).main);
            let mut base_parameters = BaseParameters::default();
            base_parameters.vitality = main + excess;
            base_parameters.calculate_potencies(level, &param_grow, Some(&modifiers));
            assert_eq!(
                base_parameters.hp, expected_hp,
                "WAR L{level}: vit={} excess={excess}",
                main + excess
            );
        }
    }

    /// `modifierHitPoints >= 140` is the tank gate; 139 must still use the non-tank k table.
    /// (Changing `hp` also changes the first-term job multiplier, so each side has its own
    /// expected value — the point of the test is which *k* is selected, not that job stays 105.)
    #[test]
    fn test_hp_tank_threshold_is_modifier_hit_points_140() {
        let param_grow = param_grow(0, 0, 1386);
        let main = u32::from(super::level_modifier_for(54).main);
        let excess = 414u32;

        let mut just_below = bard_modifiers();
        just_below.hp = 139;
        let mut bp = BaseParameters::default();
        bp.vitality = main + excess;
        bp.calculate_potencies(54, &param_grow, Some(&just_below));
        // ⌊1386 × 1.39⌋ + ⌊414 × 12.036⌋ = 1926 + 4982 = 6908
        assert_eq!(bp.hp, 6908);

        let mut at_floor = bard_modifiers();
        at_floor.hp = 140;
        let mut bp = BaseParameters::default();
        bp.vitality = main + excess;
        bp.calculate_potencies(54, &param_grow, Some(&at_floor));
        // ⌊1386 × 1.40⌋ + ⌊414 × 16.482⌋ = 1940 + 6823 = 8763
        assert_eq!(bp.hp, 8763);
        assert!(bp.hp > 6908, "crossing the tank floor must select the higher k");
    }

    /// A one- and a two-handed weapon both sit in the main hand but draw on different budgets
    /// (100‰ vs 140‰), so the cap has to key off the item's EquipSlotCategory rather than the slot
    /// it occupies or whether an off-hand happens to be equipped.
    #[test]
    fn test_synced_one_handed_weapon_uses_its_own_budget() {
        let one_handed = EquippedStorage {
            // EquipSlotCategory 1: a one-handed weapon, with no off-hand equipped.
            main_hand: gear(1, 520, 532, 361, 361),
            ..Default::default()
        };

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(&one_handed, Some(&il133_caps()), None);

        // round(391 * 100 / 1000) = 39, not the two-handed round(391 * 140 / 1000) = 55.
        assert_eq!(base_parameters.dexterity, 39);
        assert_eq!(base_parameters.vitality, 42);
        assert_eq!(base_parameters.critical_hit, 37);
        assert_eq!(base_parameters.determination, 37);
    }

    /// The il-700 chest piece from `il700_bard_set` (Critical Hit 244) with two Critical Hit XII
    /// melded in. `MateriaStats` has already trimmed them against the item's own meld cap of 348,
    /// so what reaches the stat calculation is a flat +104 on the chest slot.
    fn il700_body_with_crit_materia() -> (EquippedStorage, MateriaStats) {
        let equipped = EquippedStorage {
            body: Item {
                item_level: 700,
                ..gear(4, 501, 513, 244, 348)
            },
            ..Default::default()
        };

        let mut materia = MateriaStats::default();
        materia.add(EquipSlot::Body as u16, 27, 104);

        (equipped, materia)
    }

    /// A cap table that only models Critical Hit on the chest slot, so a test can pick the sync item
    /// level and the cap independently of any real item level's budget.
    fn chest_crit_caps(item_level: u16, cap: u16) -> ItemLevelCaps {
        let mut caps = ItemLevelCaps::default();
        caps.set_item_level(item_level);
        caps.set(CapSlot::Chest, 27, cap);
        caps
    }

    /// Unsynced, materia simply add to the item's own value.
    #[test]
    fn test_materia_add_to_an_unsynced_item() {
        let (equipped, materia) = il700_body_with_crit_materia();

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(&equipped, None, Some(&materia));

        assert_eq!(base_parameters.critical_hit, 244 + 104);
    }

    /// Syncing an item down drops its materia entirely — they are not scaled and not capped, they
    /// stop counting. Only the item's own value comes through, clamped as usual.
    #[test]
    fn test_synced_down_item_loses_its_materia() {
        let (equipped, materia) = il700_body_with_crit_materia();

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(
            &equipped,
            // The item is il-700, so a sync to il-600 syncs it down.
            Some(&chest_crit_caps(600, 200)),
            Some(&materia),
        );

        // Not 304 (materia added on top), and not 200 + something scaled: just the clamped item.
        assert_eq!(base_parameters.critical_hit, 200);
    }

    /// An item at or below the synced item level isn't synced down, so it keeps its materia at their
    /// full trimmed value — even though that takes the slot's total past what the sync would cap the
    /// item itself to. The cap applies to the item, not to the melds.
    #[test]
    fn test_unsynced_item_keeps_its_materia_past_the_sync_cap() {
        let (equipped, materia) = il700_body_with_crit_materia();

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(
            &equipped,
            Some(&chest_crit_caps(700, 200)),
            Some(&materia),
        );

        assert_eq!(base_parameters.critical_hit, 200 + 104);
    }

    /// The high-quality bonus is part of the item's own value for a param, so it has to be summed
    /// with the item's normal value *before* the sync clamp. Clamping the two separately would let
    /// `min(244, 260) + min(30, 260)` = 274 through a cap of 260.
    #[test]
    fn test_hq_bonus_is_summed_before_the_sync_clamp() {
        let hq_body = EquippedStorage {
            body: Item {
                item_level: 700,
                // Bit 0 of item_flags is the HQ flag; ItemSpecialBonus 1 is the HQ stat bonus.
                item_flags: 1,
                item_special_bonus: 1,
                base_param_special_ids: [27, 0, 0, 0, 0, 0],
                base_param_special_values: [30, 0, 0, 0, 0, 0],
                ..gear(4, 0, 0, 244, 0)
            },
            ..Default::default()
        };

        let mut unsynced = BaseParameters::default();
        unsynced.calculate_stat_across_all_items(&hq_body, None, None);
        assert_eq!(unsynced.critical_hit, 274);

        let mut synced = BaseParameters::default();
        synced.calculate_stat_across_all_items(&hq_body, Some(&chest_crit_caps(600, 260)), None);
        assert_eq!(synced.critical_hit, 260);
    }

    /// The same item without the HQ flag doesn't get the bonus, even though it carries the columns.
    #[test]
    fn test_hq_bonus_only_applies_to_hq_items() {
        let nq_body = EquippedStorage {
            body: Item {
                item_level: 700,
                item_special_bonus: 1,
                base_param_special_ids: [27, 0, 0, 0, 0, 0],
                base_param_special_values: [30, 0, 0, 0, 0, 0],
                ..gear(4, 0, 0, 244, 0)
            },
            ..Default::default()
        };

        let mut base_parameters = BaseParameters::default();
        base_parameters.calculate_stat_across_all_items(&nq_body, None, None);

        assert_eq!(base_parameters.critical_hit, 244);
    }

    /// Defense, magic defense and weapon damage live in their own Item columns rather than in the
    /// BaseParam array — but an HQ bonus can still target those same BaseParams through
    /// `BaseParamSpecial`. Both halves have to reach the sync clamp as one number.
    ///
    /// This is the case the crit-based HQ tests above cannot catch: BaseParam 27 has no dedicated
    /// column, so it is only ever accumulated once and passes either way. 21/24/12/13 are the ids
    /// that can be accumulated twice, and 7157 Item rows carry 21 or 24 in `BaseParamSpecial`.
    #[test]
    fn test_hq_bonus_on_a_dedicated_column_param_is_summed_before_the_sync_clamp() {
        let hq_body = EquippedStorage {
            body: Item {
                item_level: 700,
                // Bit 0 of item_flags is the HQ flag; ItemSpecialBonus 1 is the HQ stat bonus.
                item_flags: 1,
                item_special_bonus: 1,
                defense: 127,
                base_param_special_ids: [BASE_PARAM_DEFENSE, 0, 0, 0, 0, 0],
                base_param_special_values: [14, 0, 0, 0, 0, 0],
                ..gear(4, 0, 0, 0, 0)
            },
            ..Default::default()
        };

        let mut unsynced = BaseParameters::default();
        unsynced.calculate_stat_across_all_items(&hq_body, None, None);
        assert_eq!(unsynced.defense, 141);

        let mut caps = ItemLevelCaps::default();
        caps.set_item_level(700);
        caps.set(CapSlot::Chest, BASE_PARAM_DEFENSE, 100);

        let mut synced = BaseParameters::default();
        synced.calculate_stat_across_all_items(&hq_body, Some(&caps), None);
        // min(127 + 14, 100) == 100. Clamping the halves separately yields
        // min(127, 100) + min(14, 100) == 114, inflating defense past its synced cap.
        assert_eq!(synced.defense, 100);
    }

    #[test]
    fn class_info_current_level_is_synced_but_class_level_stays_true() {
        // Under sync the client's `current_level` (→ PlayerState.CurrentLevel, drives the
        // level-sync display) must be the synced level, while `class_level` (→ ClassJobLevels[]
        // EXP/progression) must remain the true level.
        let (current_level, class_level) = class_info_levels(Some(54), 100);
        assert_eq!(current_level, 54);
        assert_eq!(class_level, 100);
    }

    #[test]
    fn class_info_levels_are_true_level_when_not_synced() {
        let (current_level, class_level) = class_info_levels(None, 100);
        assert_eq!(current_level, 100);
        assert_eq!(class_level, 100);
    }
}
