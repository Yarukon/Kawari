use kawari::{common::ITEM_CONDITION_MAX, ipc::zone::ItemInfo};

use serde::{Deserialize, Serialize};

use crate::ItemRow;

/// Strips the HQ flag from a wire item id. The client sends HQ items as `real_id + 1_000_000`
/// (seen in ClientTrigger 407/2800, ActorControl 347/348 and the meld request itself), while the
/// inventory always stores the plain catalog id.
pub fn item_id_base(hq_id: u32) -> u32 {
    if hq_id >= 1_000_000 {
        hq_id - 1_000_000
    } else {
        hq_id
    }
}

/// Represents an item, or if the quantity is zero, an empty slot.
#[derive(Default, Copy, Clone, Serialize, Deserialize, Debug)]
pub struct Item {
    /// How many of this item occupies it's slot.
    pub quantity: u32,
    /// Index into the Item Excel sheet.
    pub item_id: u32,
    /// The player who crafted this item.
    pub crafter_content_id: u64,
    /// Unknown flags.
    pub item_flags: u8,
    /// The condition of this item from 0 to 30000.
    pub condition: u16,
    /// Spiritbond or collectability stat.
    pub spiritbond_or_collectability: u16,
    /// If not zero, what Item this is glamoured to.
    pub glamour_id: u32,
    /// The materia melded into this item.
    pub materia: [u16; 5],
    /// The grade of said materia.
    pub materia_grades: [u8; 5],
    /// Dye information?
    pub stains: [u8; 2],

    // Data only used by us, and not stored.
    #[serde(skip)]
    pub item_level: u16,
    #[serde(skip)]
    pub stack_size: u32,
    #[serde(skip)]
    pub price_low: u32,
    /// This item's EquipSlotCategory row id (0 if it isn't equippable). Needed to tell a
    /// two-handed weapon (13) from a one-handed one (1), which the equipped slot alone can't.
    #[serde(skip)]
    pub equip_slot_category: u8,
    #[serde(skip)]
    pub materia_slot_count: u8,
    #[serde(skip)]
    pub is_advanced_melding_permitted: bool,
    #[serde(skip)]
    pub base_param_ids: [u8; 6],
    #[serde(skip)]
    pub base_param_values: [i16; 6],
    #[serde(skip)]
    pub defense: u16,
    #[serde(skip)]
    pub magic_defense: u16,
    #[serde(skip)]
    pub weapon_damage_phys: u16,
    #[serde(skip)]
    pub weapon_damage_mag: u16,
}

impl Item {
    pub fn new(item_info: &ItemRow, quantity: u32) -> Self {
        // A materia carries its own identity in slot 0. The client reads its grade from there to
        // decide which overmeld success rate to display, so a materia without it reads as grade 0
        // (壹型) and shows that grade's rate instead of its own.
        let mut materia = [0u16; 5];
        let mut materia_grades = [0u8; 5];
        if let Some((row, grade_index)) = item_info.materia_identity {
            materia[0] = row;
            materia_grades[0] = grade_index;
        }

        Self {
            quantity,
            item_id: item_info.id,
            condition: ITEM_CONDITION_MAX,
            materia,
            materia_grades,
            item_level: item_info.item_level,
            stack_size: item_info.stack_size,
            price_low: item_info.price_low,
            equip_slot_category: item_info.equip_category.clone() as u8,
            materia_slot_count: item_info.materia_slot_count,
            is_advanced_melding_permitted: item_info.is_advanced_melding_permitted,
            base_param_ids: item_info.base_param_ids,
            base_param_values: item_info.base_param_values,
            defense: item_info.defense,
            magic_defense: item_info.magic_defense,
            weapon_damage_phys: item_info.weapon_damage_phys,
            weapon_damage_mag: item_info.weapon_damage_mag,
            ..Default::default()
        }
    }

    /// Returns the catalog ID of the glamour, if applicable.
    pub fn apparent_id(&self) -> u32 {
        if self.quantity == 0 {
            return 0;
        }
        if self.glamour_id > 0 {
            return self.glamour_id;
        }
        self.item_id
    }

    pub fn is_empty_slot(&self) -> bool {
        self.quantity == 0 || self.item_id == 0
    }
}

impl From<Item> for ItemInfo {
    fn from(val: Item) -> Self {
        ItemInfo {
            quantity: val.quantity,
            item_id: val.item_id,
            crafter_content_id: val.crafter_content_id,
            item_flags: val.item_flags,
            condition: val.condition,
            spiritbond_or_collectability: val.spiritbond_or_collectability,
            glamour_id: val.glamour_id,
            materia: val.materia,
            materia_grades: val.materia_grades,
            stains: val.stains,
            ..Default::default()
        }
    }
}

impl From<ItemRow> for Item {
    fn from(value: ItemRow) -> Self {
        Self::new(&value, 0)
    }
}
