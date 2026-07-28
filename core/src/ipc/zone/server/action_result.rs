use binrw::binrw;
use bitflags::bitflags;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, FromRepr};

use crate::{
    common::{ObjectId, ObjectTypeId, read_quantized_rotation, write_quantized_rotation},
    ipc::zone::ActionType,
};

const DAMAGE_HEAL_LARGE_VALUE_FLAG: u8 = 0x40;
const DAMAGE_HEAL_MAX_VALUE: u32 = 0x00FF_FFFF;

fn decode_damage_heal_amount(value: u16, param3: u8, param4: u8) -> u32 {
    let amount = u32::from(value);
    if param4 & DAMAGE_HEAL_LARGE_VALUE_FLAG != 0 {
        amount + (u32::from(param3) << 16)
    } else {
        amount
    }
}

fn encode_damage_heal_amount_low(amount: u32) -> u16 {
    (amount.min(DAMAGE_HEAL_MAX_VALUE) & 0xFFFF) as u16
}

fn encode_damage_heal_amount_high(amount: u32) -> u8 {
    ((amount.min(DAMAGE_HEAL_MAX_VALUE) >> 16) & 0xFF) as u8
}

fn encode_damage_heal_param4(amount: u32, param4: u8) -> u8 {
    let mut param4 = param4 & !DAMAGE_HEAL_LARGE_VALUE_FLAG;
    if amount.min(DAMAGE_HEAL_MAX_VALUE) > u32::from(u16::MAX) {
        param4 |= DAMAGE_HEAL_LARGE_VALUE_FLAG;
    }
    param4
}

fn encode_heal_params(amount: u32, params: [u8; 5]) -> [u8; 5] {
    [
        params[0],
        params[1],
        params[2],
        encode_damage_heal_amount_high(amount),
        encode_damage_heal_param4(amount, params[4]),
    ]
}

// TODO: this might be a flag?
// This value is written verbatim into the effect's `param0` byte, where the client reads the
// hit-severity flags: bit5 (0x20) = critical, bit6 (0x40) = direct hit (per the retail client
// and ffxiv_bossmod's ActionEffect: `Param0 & 0x20` = crit, `Param0 & 0x40` = dhit).
#[binrw]
#[derive(Eq, PartialEq, Clone, Copy, Default)]
pub struct DamageKind(u8);

bitflags! {
    impl DamageKind: u8 {
        const CRITICAL = 0x20;
        const DIRECT_HIT = 0x40;
    }
}

impl std::fmt::Debug for DamageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

#[binrw]
#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum EffectKind {
    #[default]
    /// There's no effect entry.
    #[brw(magic = 0u8)]
    None,
    /// The attack missed.
    #[brw(magic = 1u8)]
    Miss,
    /// Do damage!
    #[brw(magic = 3u8)]
    Damage {
        damage_kind: DamageKind,
        #[br(temp)]
        #[bw(ignore)]
        param1: u8,
        #[br(calc = DamageType::from_repr(param1 & 0x0F).unwrap())]
        #[bw(ignore)]
        damage_type: DamageType,
        #[br(calc = DamageElement::from_repr(param1 >> 4).unwrap())]
        #[bw(ignore)]
        damage_element: DamageElement,
        #[br(ignore)]
        #[bw(calc = ((*damage_element as u8) << 4) | *damage_type as u8)]
        actual_param1: u8,
        bonus_percent: u8,
        #[br(temp)]
        #[bw(calc = encode_damage_heal_amount_high(*amount))]
        param3: u8,
        #[br(temp)]
        #[bw(calc = encode_damage_heal_param4(*amount, *unk4))]
        param4: u8,
        #[br(temp)]
        #[bw(calc = encode_damage_heal_amount_low(*amount))]
        value: u16,
        #[br(calc = param3)]
        #[bw(ignore)]
        unk3: u8,
        #[br(calc = param4)]
        #[bw(ignore)]
        unk4: u8,
        #[br(calc = decode_damage_heal_amount(value, param3, param4))]
        #[bw(ignore)]
        amount: u32,
    },
    /// Heals for a specified amount.
    #[brw(magic = 4u8)]
    Heal {
        #[br(temp)]
        #[bw(calc = encode_heal_params(*amount, *unk1))]
        params: [u8; 5],
        #[br(temp)]
        #[bw(calc = encode_damage_heal_amount_low(*amount))]
        value: u16,
        #[br(calc = params)]
        #[bw(ignore)]
        unk1: [u8; 5],
        #[br(calc = decode_damage_heal_amount(value, params[3], params[4]))]
        #[bw(ignore)]
        amount: u32,
    },
    /// Seen while attacking giant clams.
    #[brw(magic = 7u8)]
    Invincible {},
    /// Seen during Head Graze.
    #[brw(magic = 8u8)]
    InterruptAction {},
    /// Executes/combies an action combo.
    #[brw(magic = 27u8)]
    ExecuteCombo {
        /// Unknown, but seen set to 1 during Fountain (which comboes with Cascade.)
        sequence: u8,
        unk2: u8,
        unk3: u8,
        unk4: u8,
        unk5: u8,
        /// Which action begun this combo, I guess.
        action_id: u16,
    },
    /// Seen during Sprint.
    #[brw(magic = 14u8)]
    GainEffect {
        unk1: u8,
        unk2: u8,
        /// Status-specific parameter.
        param: u16,
        unk3: u8,
        /// Index into the Status Excel sheet.
        effect_id: u16,

        // NOTE: the following is for our internal usage, this is not an actual part of the packet
        // TODO: this shouldn't be here, instead we should maybe create a lua-specific struct for all of this information
        #[brw(ignore)]
        duration: f32,
    },
    /// Seen on Summon Bahamut / Summon Solar Bahamut. The payload is `00 00 00 00 00 01 00` on
    /// retail and appears to be the demi-summon transition effect distinct from ordinary pet summon.
    #[brw(magic = 25u8)]
    SummonDemi { unk: [u8; 7] },
    /// Seen during Cascade (and gaining Silken Symmetry.)
    /// Guessed at it's purpose, not 100% certain it's for applying to yourself.
    #[brw(magic = 15u8)]
    GainEffectSelf {
        unk1: u8,
        unk2: u8,
        /// Status-specific parameter.
        param: u16,
        unk3: u8,
        /// Index into the Status Excel sheet.
        effect_id: u16,

        // NOTE: the following is for our internal usage, this is not an actual part of the packet
        // TODO: this shouldn't be here, instead we should maybe create a lua-specific struct for all of this information
        #[brw(ignore)]
        duration: f32,
    },
    /// Seen during the Unveil action.
    #[brw(magic = 16u8)]
    LoseEffect {
        param: u16,
        unk: [u8; 3], // empty?
        effect_id: u16,
    },
    /// Seen during mounting.
    #[brw(magic = 39u8)]
    Mount { unk1: u8, unk2: u32, id: u16 },
    /// Play this VFX.
    #[brw(magic = 59u8)]
    PlayVFX { unk: [u8; 5], effect_id: u16 },
    /// Seen in the Summon Carbuncle action.
    #[brw(magic = 62u8)]
    SummonPet { unk: [u8; 7] },
    /// Knockback (bossmod 0x1F). `value` = row index into the Knockback Excel sheet (client reads
    /// distance/direction/speed from it); `extra_distance` = param0 = additional distance on top of
    /// the sheet distance. Client computes/applies its own displacement (player position is
    /// client-authoritative); server sends the packet only.
    #[brw(magic = 31u8)]
    Knockback {
        extra_distance: u8, // param0
        unk: [u8; 4],       // param1-4, zero
        knockback_id: u16,  // value = Knockback sheet row
    },
    /// Attract / pull-in, sheet-driven variant 1 (bossmod 0x20). `value` = row into the Attract
    /// Excel sheet. Client self-animates the pull.
    #[brw(magic = 32u8)]
    Attract1 { unk: [u8; 5], attract_id: u16 },
    /// Attract variant 2 (bossmod 0x21). Same layout as Attract1.
    #[brw(magic = 33u8)]
    Attract2 { unk: [u8; 5], attract_id: u16 },
    /// Custom attract 1 (bossmod 0x22). No sheet lookup: `distance` = value, `min_distance` = param1,
    /// `speed` = param0. param2-4 zero.
    #[brw(magic = 34u8)]
    AttractCustom1 {
        speed: u8,        // param0
        min_distance: u8, // param1
        unk: [u8; 3],     // param2-4, zero
        distance: u16,    // value
    },
    /// Custom attract 2 (bossmod 0x23). Same layout as AttractCustom1.
    #[brw(magic = 35u8)]
    AttractCustom2 {
        speed: u8,
        min_distance: u8,
        unk: [u8; 3],
        distance: u16,
    },
    /// Custom attract 3 (bossmod 0x24). Same layout as AttractCustom1.
    #[brw(magic = 36u8)]
    AttractCustom3 {
        speed: u8,
        min_distance: u8,
        unk: [u8; 3],
        distance: u16,
    },
    /// Directly set the target's HP (bossmod 0x4A, e.g. Zodiark's Kokytos). `hp` = value = new HP.
    /// NOTE: u16 only — no large-value encoding, so HP > 65535 cannot be represented (fine for
    /// set-to-1 / residual-HP mechanics). Server must ALSO mutate health_points and send an HP update;
    /// the effect alone only tells the client to render the change.
    #[brw(magic = 74u8)]
    SetHP { unk: [u8; 5], hp: u16 },
    /// Interrupt an in-progress cast (bossmod 0x4C). Distinct from magic-8 `InterruptAction` (which is
    /// really NoEffectText). Empty payload. Server must ALSO cancel the target's cast.
    #[brw(magic = 76u8)]
    Interrupt {},
    /// The effect retail attaches to the Teleport action's `ActionResult` (effect type 0x3D / 61).
    /// `territory` holds the destination TerritoryType. The client's per-target effect handler is a
    /// no-op for the base Teleport action, but retail always sends this so the action result is
    /// well-formed (`effect_count = 1`); an empty result (`effect_count = 0`) leaves the teleport-out
    /// visuals unresolved (the caster stays stuck in the teleport animation). `unk` is zero in captures.
    #[brw(magic = 61u8)]
    Teleport { unk: [u8; 5], territory: u16 },
    /// Unknown effect (that should be added!)
    Unknown { magic: u8, unk: [u8; 7] },
}

#[repr(u8)]
#[derive(
    Debug, Eq, PartialEq, Clone, Copy, Default, Display, Deserialize, Serialize, FromRepr, EnumIter,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DamageType {
    /// Usually reserved for special enemy actions.
    Unique,
    Slashing,
    Piercing,
    Blunt,
    Shot,
    /// Magical damage makes up the breadth of most player spells.
    Magic,
    Breath,
    /// Physical damage makes up the breadth of most player weaponskills.
    #[default]
    Physical,
    LimitBreak,
}

#[cfg(feature = "server")]
impl mlua::IntoLua for DamageType {
    fn into_lua(self, _: &mlua::Lua) -> mlua::Result<mlua::Value> {
        Ok(mlua::Value::Integer(self as i64))
    }
}

#[cfg(feature = "server")]
impl mlua::FromLua for DamageType {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::Integer(integer) => Ok(Self::from_repr(integer as u8).unwrap()),
            _ => unreachable!(),
        }
    }
}

#[repr(u8)]
#[derive(
    Debug, Eq, PartialEq, Clone, Copy, Default, Display, Deserialize, Serialize, FromRepr, EnumIter,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DamageElement {
    Unknown,
    Fire,
    Ice,
    Air,
    Earth,
    Lightning,
    Water,
    #[default]
    Unaspected,
}

#[cfg(feature = "server")]
impl mlua::IntoLua for DamageElement {
    fn into_lua(self, _: &mlua::Lua) -> mlua::Result<mlua::Value> {
        Ok(mlua::Value::Integer(self as i64))
    }
}

#[cfg(feature = "server")]
impl mlua::FromLua for DamageElement {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        match value {
            mlua::Value::Integer(integer) => Ok(Self::from_repr(integer as u8).unwrap()),
            _ => unreachable!(),
        }
    }
}

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionEffect {
    #[brw(pad_size_to = 8)]
    pub kind: EffectKind,
}

#[binrw]
#[derive(Clone, Copy, Eq, PartialEq, Default)]
pub struct ActionResultFlag(u8);

bitflags! {
    impl ActionResultFlag : u8 {
        const FORCE_ANIMATION_LOCK = 0x1;
    }
}

impl std::fmt::Debug for ActionResultFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        bitflags::parser::to_writer(self, f)
    }
}

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, Default)]
pub struct ActionResult {
    pub animation_target_id: ObjectTypeId,
    /// Index into the Action Excel sheet.
    pub action_id: u32,
    pub global_sequence: u32,
    /// Controls how long the next action should be delayed, in seconds.
    pub animation_lock: f32,
    /// Only used when ActionCategory is 11.
    pub ballista_entity_id: ObjectId,
    /// The same as `sequence` from this action's `ActionRequest`.
    pub source_sequence: u16,
    #[br(map = read_quantized_rotation)]
    #[bw(map = write_quantized_rotation)]
    pub rotation: f32,
    /// Usually the same as `action_id`.
    pub spell_id: u16,
    pub animation_variation: u8,
    /// The kind of action.
    pub action_type: ActionType,
    pub flags: ActionResultFlag,
    pub effect_count: u8,
    pub unk4: u16,
    pub unk5: [u8; 6], // might be not read by the client?
    pub effects: [ActionEffect; 8],
    #[brw(pad_before = 6, pad_after = 4)]
    pub target_id_again: ObjectTypeId,
}

#[cfg(test)]
mod tests {
    use std::{fs::read, io::Cursor, path::PathBuf};

    use binrw::{BinRead, BinWrite};

    use crate::common::ObjectId;

    use crate::server_zone_tests_dir;

    use super::*;

    #[test]
    fn action_effect_damage_uses_large_value_encoding() {
        let effect = ActionEffect {
            kind: EffectKind::Damage {
                damage_kind: DamageKind::Normal,
                damage_type: DamageType::Magic,
                damage_element: DamageElement::Unaspected,
                bonus_percent: 0,
                unk3: 0,
                unk4: 0,
                amount: 70_000,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 3);
        assert_eq!(raw[4], 1);
        assert_eq!(raw[5] & 0x40, 0x40);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 4_464);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(
            parsed.kind,
            EffectKind::Damage {
                damage_kind: DamageKind::Normal,
                damage_type: DamageType::Magic,
                damage_element: DamageElement::Unaspected,
                bonus_percent: 0,
                unk3: 1,
                unk4: 0x40,
                amount: 70_000,
            }
        );
    }

    #[test]
    fn action_effect_damage_kind_writes_param0_severity_flags() {
        // The client reads hit severity from param0 (byte index 1): bit5 = crit, bit6 = direct hit.
        // Guard against regressing DamageKind's repr back to plain 0/1/2/3 (which the client ignores,
        // making everything render as a Normal hit).
        let cases = [
            (DamageKind::Normal, 0x00u8),
            (DamageKind::Critical, 0x20u8),
            (DamageKind::DirectHit, 0x40u8),
            (DamageKind::CriticalDirectHit, 0x60u8),
        ];
        for (kind, expected_param0) in cases {
            let effect = ActionEffect {
                kind: EffectKind::Damage {
                    damage_kind: kind,
                    damage_type: DamageType::Slashing,
                    damage_element: DamageElement::Unaspected,
                    bonus_percent: 0,
                    unk3: 0,
                    unk4: 0,
                    amount: 100,
                },
            };
            let mut writer = Cursor::new(Vec::new());
            effect.write_le(&mut writer).unwrap();
            let raw = writer.into_inner();
            assert_eq!(raw[0], 3, "effect magic");
            assert_eq!(raw[1], expected_param0, "param0 severity flags for {kind:?}");
        }
    }

    #[test]
    fn action_effect_knockback_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::Knockback {
                extra_distance: 3,
                unk: [0; 4],
                knockback_id: 42,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 31);
        assert_eq!(raw[1], 3);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 42);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_attract1_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::Attract1 {
                unk: [0; 5],
                attract_id: 17,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 32);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 17);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_attract2_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::Attract2 {
                unk: [0; 5],
                attract_id: 99,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 33);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 99);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_attract_custom1_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::AttractCustom1 {
                speed: 5,
                min_distance: 1,
                unk: [0; 3],
                distance: 10,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 34);
        assert_eq!(raw[1], 5);
        assert_eq!(raw[2], 1);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 10);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_attract_custom2_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::AttractCustom2 {
                speed: 6,
                min_distance: 2,
                unk: [0; 3],
                distance: 11,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 35);
        assert_eq!(raw[1], 6);
        assert_eq!(raw[2], 2);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 11);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_attract_custom3_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::AttractCustom3 {
                speed: 7,
                min_distance: 3,
                unk: [0; 3],
                distance: 12,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 36);
        assert_eq!(raw[1], 7);
        assert_eq!(raw[2], 3);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 12);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_set_hp_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::SetHP {
                unk: [0; 5],
                hp: 1,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 74);
        assert_eq!(u16::from_le_bytes([raw[6], raw[7]]), 1);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_interrupt_round_trip() {
        let effect = ActionEffect {
            kind: EffectKind::Interrupt {},
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw[0], 76);
        assert_eq!(&raw[1..8], &[0u8; 7]);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(parsed.kind, effect.kind);
    }

    #[test]
    fn action_effect_teleport_matches_capture() {
        // Golden bytes from a retail Teleport (action 5) ActionResult effect[0]: magic 0x3D (61),
        // all params zero, destination territory 759 (0x02F7) in the trailing u16.
        let effect = ActionEffect {
            kind: EffectKind::Teleport {
                unk: [0; 5],
                territory: 759,
            },
        };

        let mut writer = Cursor::new(Vec::new());
        effect.write_le(&mut writer).unwrap();
        let raw = writer.into_inner();

        assert_eq!(raw.len(), 8);
        assert_eq!(raw, [0x3D, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF7, 0x02]);

        let mut reader = Cursor::new(raw);
        let parsed = ActionEffect::read_le(&mut reader).unwrap();
        assert_eq!(
            parsed.kind,
            EffectKind::Teleport {
                unk: [0; 5],
                territory: 759,
            }
        );
    }

    #[test]
    fn read_actionresult() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(server_zone_tests_dir!("action_result.bin"));

        let buffer = read(d).unwrap();
        let mut buffer = Cursor::new(&buffer);

        let action_result = ActionResult::read_le(&mut buffer).unwrap();
        assert_eq!(
            action_result.animation_target_id.object_id,
            ObjectId(0x40070E42)
        );
        assert_eq!(action_result.action_id, 31);
        assert_eq!(action_result.global_sequence, 2662353);
        assert_eq!(action_result.animation_lock, 0.6);
        assert_eq!(action_result.ballista_entity_id, ObjectId::default());
        assert_eq!(action_result.source_sequence, 1);
        assert_eq!(action_result.rotation, 1.207309);
        assert_eq!(action_result.spell_id, 31);
        assert_eq!(action_result.animation_variation, 0);
        assert_eq!(action_result.flags, ActionResultFlag::empty());
        assert_eq!(action_result.action_type, ActionType::Action);
        assert_eq!(action_result.effect_count, 1);
        assert_eq!(action_result.unk4, 0);
        assert_eq!(action_result.unk5, [0; 6]);

        // effect 0: attack
        assert_eq!(
            action_result.effects[0].kind,
            EffectKind::Damage {
                damage_kind: DamageKind::Normal,
                damage_type: DamageType::Slashing,
                damage_element: DamageElement::Unaspected,
                bonus_percent: 0,
                unk3: 0,
                unk4: 0,
                amount: 22
            }
        );

        // effect 1: start action combo
        assert_eq!(
            action_result.effects[1].kind,
            EffectKind::ExecuteCombo {
                sequence: 0,
                unk2: 0,
                unk3: 0,
                unk4: 0,
                unk5: 128,
                action_id: 31
            }
        );

        assert_eq!(
            action_result.target_id_again.object_id,
            ObjectId(0x40070E42)
        );
    }

    #[test]
    fn read_actionresult_sprint() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(server_zone_tests_dir!("action_result_sprint.bin"));

        let buffer = read(d).unwrap();
        let mut buffer = Cursor::new(&buffer);

        let action_result = ActionResult::read_le(&mut buffer).unwrap();
        assert_eq!(
            action_result.animation_target_id.object_id,
            ObjectId(277554542)
        );
        assert_eq!(action_result.action_id, 3);
        assert_eq!(action_result.global_sequence, 776386);
        assert_eq!(action_result.animation_lock, 0.6);
        assert_eq!(action_result.ballista_entity_id, ObjectId::default());
        assert_eq!(action_result.source_sequence, 1);
        assert_eq!(action_result.rotation, 2.6254003);
        assert_eq!(action_result.spell_id, 3);
        assert_eq!(action_result.animation_variation, 0);
        assert_eq!(action_result.flags, ActionResultFlag::empty());
        assert_eq!(action_result.action_type, ActionType::Action);
        assert_eq!(action_result.effect_count, 1);
        assert_eq!(action_result.unk4, 0);
        assert_eq!(action_result.unk5, [0; 6]);

        assert_eq!(
            action_result.effects[0].kind,
            EffectKind::GainEffect {
                unk1: 0,
                unk2: 48,
                unk3: 0,
                effect_id: 50,
                duration: 0.0,
                param: 30,
            }
        );

        assert_eq!(action_result.target_id_again.object_id, ObjectId(277554542));
    }

    #[test]
    fn read_actionresult_mount() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(server_zone_tests_dir!("action_result_mount.bin"));

        let buffer = read(d).unwrap();
        let mut buffer = Cursor::new(&buffer);

        let action_result = ActionResult::read_le(&mut buffer).unwrap();
        assert_eq!(
            action_result.animation_target_id.object_id,
            ObjectId(277114100)
        );
        assert_eq!(action_result.action_id, 55);
        assert_eq!(action_result.global_sequence, 4232092);
        assert_eq!(action_result.animation_lock, 0.1);
        assert_eq!(action_result.ballista_entity_id, ObjectId::default());
        assert_eq!(action_result.source_sequence, 4);
        assert_eq!(action_result.rotation, -0.8154669);
        assert_eq!(action_result.spell_id, 4);
        assert_eq!(action_result.animation_variation, 0);
        assert_eq!(action_result.flags, ActionResultFlag::empty());
        assert_eq!(action_result.action_type, ActionType::Mount);
        assert_eq!(action_result.effect_count, 1);
        assert_eq!(action_result.unk4, 0);
        assert_eq!(action_result.unk5, [0; 6]);

        assert_eq!(
            action_result.effects[0].kind,
            EffectKind::Mount {
                unk1: 1,
                unk2: 0,
                id: 55,
            }
        );

        assert_eq!(action_result.target_id_again.object_id, ObjectId(277114100));
    }

    #[test]
    fn read_actionresult_unveil() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(server_zone_tests_dir!("action_result_unveil.bin"));

        let buffer = read(d).unwrap();
        let mut buffer = Cursor::new(&buffer);

        let action_result = ActionResult::read_le(&mut buffer).unwrap();
        assert_eq!(
            action_result.animation_target_id.object_id,
            ObjectId(277114100)
        );
        assert_eq!(action_result.action_id, 13266);
        assert_eq!(action_result.global_sequence, 749);
        assert_eq!(action_result.animation_lock, 0.6);
        assert_eq!(action_result.ballista_entity_id, ObjectId::default());
        assert_eq!(action_result.source_sequence, 18);
        assert_eq!(action_result.rotation, -2.0225368);
        assert_eq!(action_result.spell_id, 13266);
        assert_eq!(action_result.animation_variation, 0);
        assert_eq!(action_result.flags, ActionResultFlag::empty());
        assert_eq!(action_result.action_type, ActionType::Action);
        assert_eq!(action_result.effect_count, 1);
        assert_eq!(action_result.unk4, 0);
        assert_eq!(action_result.unk5, [0; 6]);

        assert_eq!(
            action_result.effects[0].kind,
            EffectKind::LoseEffect {
                param: 219,
                unk: [0; 3],
                effect_id: 565
            }
        );

        assert_eq!(action_result.target_id_again.object_id, ObjectId(277114100));
    }
}
