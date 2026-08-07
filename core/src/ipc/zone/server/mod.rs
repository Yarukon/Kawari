use binrw::binrw;
use kawari_core_macro::opcode_data;
use physis::savedata::chardat::CustomizeData;

use super::OnlineStatusMask;
pub use super::social_list::{SocialList, SocialListUIFlags, SocialListUILanguages};

mod chara_info;
use chara_info::CharaInfoFromContentIdsData;

mod spawn_player;
pub use spawn_player::SpawnPlayer;

mod status_effect;
pub use status_effect::StatusEffect;

mod update_class_info;
pub use update_class_info::UpdateClassInfo;

mod player_setup;
pub use player_setup::PlayerSetup;

mod player_stats;
pub use player_stats::PlayerStats;

mod actor_control;
pub use actor_control::{
    ActorControl, ActorControlCategory, ActorControlSelf, ActorControlTarget, LiveEventType,
    STATUS_NOTIFICATION_GAINED_FROM_OTHER,
};

mod zone_init;
pub use zone_init::{ZoneInit, ZoneInitFlags};

mod spawn_npc;
pub use spawn_npc::{CharacterDataFlag, SpawnNpc};

mod common_spawn;
pub use common_spawn::{
    BattleNpcSubKind, CommonSpawn, DisplayFlag, GameMasterRank, ObjectKind, PlayerSubKind,
};

mod status_effect_list;
pub use status_effect_list::StatusEffectList;

mod weather_change;
pub use weather_change::WeatherChange;

mod container_info;
pub use container_info::ContainerInfo;

mod item_info;
pub use item_info::ItemInfo;

mod event_scene;
pub use event_scene::{EventScene, SceneFlags};

mod event_start;
pub use event_start::{EventStart, EventType};

mod action_effect;
pub use action_effect::{
    ActionEffect1, ActionEffectFlag, DamageElement, DamageKind, DamageType, TargetEffect,
    TargetEffectKind,
};

mod aoe_effect;
pub use aoe_effect::{
    ActionEffect8, ActionEffect16, ActionEffect24, ActionEffect32, ActionEffectHeader,
};

mod actor_set_pos;
pub use actor_set_pos::{ActorSetPos, WarpType};

mod equip;
pub use equip::Equip;

mod currency_info;
pub use currency_info::CurrencyInfo;

pub use super::config::Config;

mod spawn_object;
pub use spawn_object::SpawnObject;

mod quest_active_list;
pub use quest_active_list::{ActiveQuest, QuestActiveList};

mod glamour;
pub use glamour::{GlamourDresserContents, GlamourPlate, GlamourPlateSaveAck, GlamourPlates};

mod effect_result;
pub use effect_result::{EffectEntry, EffectResult};

mod condition;
pub use condition::{Condition, Conditions};

mod chat_message;
pub use chat_message::ChatMessage;

mod free_company;
pub use free_company::FcHierarchy;

mod actor_move;
use crate::common::{
    DeepDungeonRoomFlag, HandlerId, LandData, LegacyEquipmentModelId, ObjectTypeId, Position,
    WeaponModelId, read_packed_position, read_quantized_rotation, write_packed_position,
    write_quantized_rotation,
};
use crate::constants::{
    AVAILABLE_CLASSJOBS, COMPLETED_LEGACY_QUEST_BITMASK_SIZE, COMPLETED_LEVEQUEST_BITMASK_SIZE,
    COMPLETED_QUEST_BITMASK_SIZE, COMPLETED_RECIPES_BITMASK_SIZE,
    GATHERED_GATHERING_ITEMS_BITMASK_SIZE, TITLE_UNLOCK_BITMASK_SIZE,
    UNLOCKED_MAP_MARKERS_BITMASK_SIZE,
};
pub use crate::ipc::zone::server::actor_move::ActorMove;

mod server_notice;
pub use server_notice::{ServerNoticeFlags, ServerNoticeMessage};

mod quest_tracker;
pub use quest_tracker::{QuestTracker, TrackedQuest};

mod apartment_list;
pub use apartment_list::{ApartmentList, ApartmentListEntry};

mod house_list;
pub use house_list::{House, HouseExterior, HouseList, HouseStatus};

mod housing_ward;
pub use housing_ward::{HousingWardInfo, HousingWardSummaryItem};

mod housing_interior_furniture;
pub use housing_interior_furniture::{
    Furniture, FurnitureList, FurnitureTranslatedForObserver, HousingInteriorDetails,
};

mod housing_occupied_land_info;
pub use housing_occupied_land_info::HousingOccupiedLandInfo;

mod housing_vacant_land_info;
pub use housing_vacant_land_info::HousingVacantLandInfo;

mod housing_estate_greeting;
pub use housing_estate_greeting::HousingEstateGreeting;

mod trust_information;
pub use trust_information::{TrustContent, TrustInformation};

mod event_resume;
pub use event_resume::EventResume;

mod map_markers;
pub use map_markers::MapMarkers;

mod enmity_list;
pub use enmity_list::{EnmityList, PlayerEnmity};

mod hater_list;
pub use hater_list::{Hater, HaterList};

mod map_effects;
pub use map_effects::MapEffects;

mod marketboard;
pub use marketboard::MarketBoardItem;

mod linkshell;
pub use linkshell::*;

mod spawn_treasure;
pub use spawn_treasure::{SpawnTreasure, TreasureKind};

mod mogpendium;
pub use mogpendium::{Mogpendium, MogpendiumCompletionFlags};

mod cross_realm_listing;
pub use cross_realm_listing::{CrossRealmListing, CrossRealmListings};

mod mail;
pub use mail::{
    AttachedItemInfo, LETTER_MSG_MAX_LENGTH, Letter, LetterPreview, LetterType, MAX_ATTACHMENTS,
    MAX_FRIEND_LETTERS, MAX_MAIL, MAX_MAIL_ATTACHMENTS_STORAGE, MAX_REWARD_LETTERS,
    MAX_SYSTEM_LETTERS, PREVIEW_MSG_MAX_LENGTH,
};

use crate::common::{
    CHAR_NAME_MAX_LENGTH, ContainerType, ItemOperationKind, ObjectId, read_bool_from, read_string,
    write_bool_as, write_string,
};
pub use crate::ipc::zone::black_list::{Blacklist, BlacklistedCharacter};
use crate::opcodes::ServerZoneIpcType;
use crate::packet::IpcSegment;
use crate::packet::ServerIpcSegmentHeader;

use crate::ipc::{
    chat::ChatChannel,
    zone::{
        PartyMemberEntry, PartyMemberPositions, PartyUpdateStatus, StrategyBoard,
        StrategyBoardUpdate, WaymarkPlacementMode, WaymarkPosition, WaymarkPreset,
    },
};

use crate::ipc::zone::social_list::{FriendGroupIconInfo, GrandCompany};
use crate::ipc::zone::{ActionType, InviteReply, InviteType, InviteUpdateType, SearchInfo};

pub type ServerZoneIpcSegment =
    IpcSegment<ServerIpcSegmentHeader<ServerZoneIpcType>, ServerZoneIpcType, ServerZoneIpcData>;

/// The editable "design block" of an adventurer plate (CharaCard).
///
/// This is the 192-byte span from `version` to `timestamp` (inclusive) that the client
/// submits verbatim when saving a plate (`SubmitAdventurerPlate`) and that the server echoes
/// back inside the `AdventurerPlate` response. The client treats
/// this as a frozen snapshot: it includes not only style/portrait fields but also a snapshot
/// of the character's customize (face) data, gear dye stains, and equipped item ids taken at
/// save time. That is why the game has a "reset due to Fantasia" flag (`flags & 1`) and a
/// "gear info mismatch" warning — the plate does not track the live character.
#[binrw]
#[derive(Debug, Clone)]
pub struct PlateDesign {
    pub version: u8,
    pub expression: u8,
    pub camera_zoom: u8,
    pub directional_lighting_color_red: u8,
    pub directional_lighting_color_green: u8,
    pub directional_lighting_color_blue: u8,
    pub directional_lighting_color_brightness: u8,
    pub ambient_lighting_color_red: u8,
    pub ambient_lighting_color_green: u8,
    pub ambient_lighting_color_blue: u8,
    pub ambient_lighting_color_brightness: u8,
    pub class_job_id: u8,
    pub customize: CustomizeData,
    pub stain_ids1: [u8; 12],
    pub gear_visibility_flag: u8,
    pub top_border: u8,
    pub bottom_border: u8,
    pub preferred_class_job_id: u8,
    pub active_hours_weekdays: [u8; 3],
    pub active_hours_weekends: [u8; 3],
    pub play_styles: [u8; 6],
    /// `& 1` == the plate was reset because the character used a Fantasia (the snapshot's
    /// customize no longer matches the live character); `& 2` == visibility is "no one"
    /// (only yourself).
    pub flags: u8,
    pub unk13: u8,
    /// `& 1` == visibility is "friends only".
    pub privacy_flags: u8,
    pub stain_ids2: [u8; 12],
    pub unk14: u8,
    pub banner_timeline: u16,
    pub animation_progress: u16,
    pub head_direction_y: u16,
    pub head_direction_x: u16,
    pub eye_direction_y: u16,
    pub eye_direction_x: u16,
    pub camera_position_x: u16,
    pub camera_position_y: u16,
    pub camera_position_z: u16,
    pub camera_target_x: u16,
    pub camera_target_y: u16,
    pub camera_target_z: u16,
    pub image_rotation: u16,
    pub directional_lighting_vertical: u16,
    pub directional_lighting_horizontal: u16,
    pub banner_decoration: u16,
    pub banner_bg: u16,
    pub banner_frame: u16,
    pub title: u16,
    pub decorations: [u16; 5],
    pub glasses_ids: [u16; 2],
    pub unk16: u16,
    pub unk18: u32,
    pub item_ids: [u32; 12],
    pub timestamp: u32,
}

impl Default for PlateDesign {
    /// A sensible default plate design, used when a character opens their own plate before ever
    /// saving one. The camera/lighting/banner values are the "fairly standard" defaults captured
    /// from a freshly-opened plate on retail, so the portrait renders reasonably. Character-
    /// specific snapshot fields (customize, gear stains, item ids, name) and visibility flags are
    /// left empty/zero here and get filled in by the character's live data on first save.
    fn default() -> Self {
        Self {
            version: 4,
            expression: 0,
            camera_zoom: 160,
            directional_lighting_color_red: 255,
            directional_lighting_color_green: 255,
            directional_lighting_color_blue: 255,
            directional_lighting_color_brightness: 127,
            ambient_lighting_color_red: 51,
            ambient_lighting_color_green: 51,
            ambient_lighting_color_blue: 51,
            ambient_lighting_color_brightness: 127,
            class_job_id: 0,
            customize: CustomizeData::default(),
            stain_ids1: [0; 12],
            gear_visibility_flag: 0,
            top_border: 3,
            bottom_border: 3,
            preferred_class_job_id: 0,
            active_hours_weekdays: [0; 3],
            active_hours_weekends: [0; 3],
            play_styles: [0; 6],
            flags: 0,
            unk13: 0,
            privacy_flags: 0,
            stain_ids2: [0; 12],
            unk14: 0,
            banner_timeline: 1,
            animation_progress: 0,
            head_direction_y: 0,
            head_direction_x: 0,
            eye_direction_y: 0,
            eye_direction_x: 0,
            camera_position_x: 0,
            camera_position_y: 0,
            camera_position_z: 15564,
            camera_target_x: 0,
            camera_target_y: 0,
            camera_target_z: 0,
            image_rotation: 0,
            directional_lighting_vertical: 133,
            directional_lighting_horizontal: 65459,
            banner_decoration: 1,
            banner_bg: 8,
            banner_frame: 1,
            title: 0,
            decorations: [3, 11, 2, 0, 0],
            glasses_ids: [0, 0],
            unk16: 0,
            unk18: 0,
            item_ids: [0; 12],
            timestamp: 0,
        }
    }
}

/// The display "banner" block attached to a gearset's job portrait.
///
/// This is ClientStructs `BannerData` (0x34 / 52 bytes). It is reused verbatim across several
/// packets: the client submits it via the `SubmitPortraitData` upstream packet when toggling the
/// custom-portrait button or saving the banner editor, it is carried inline by `EquipGearset2`
/// when switching to a gearset that has a valid linked portrait, and it forms the per-member
/// `banner` field of `PartyMemberPortrait`.
///
/// Every field is a raw scalar (`u8`/`u16`/`u32`), written little-endian in declaration order.
/// The trailing `checksum` is a CRC32 fingerprint of the gearset the banner was captured for;
/// the server passes it through untouched and never recomputes it.
#[binrw]
#[derive(Debug, Clone)]
pub struct PortraitBanner {
    /// Valid flag.
    pub has_data: u8,
    pub expression: u8,
    pub camera_zoom: u8,
    pub dir_light_r: u8,
    pub dir_light_g: u8,
    pub dir_light_b: u8,
    pub dir_light_brightness: u8,
    pub amb_light_r: u8,
    pub amb_light_g: u8,
    pub amb_light_b: u8,
    pub amb_light_brightness: u8,
    pub flags: u8,
    pub banner_timeline: u16,
    pub animation_progress: u16,
    pub head_direction_y: u16,
    pub head_direction_x: u16,
    pub eye_direction_y: u16,
    pub eye_direction_x: u16,
    pub camera_position_x: u16,
    pub camera_position_y: u16,
    pub camera_position_z: u16,
    pub camera_target_x: u16,
    pub camera_target_y: u16,
    pub camera_target_z: u16,
    pub image_rotation: u16,
    pub directional_lighting_vertical: u16,
    pub directional_lighting_horizontal: u16,
    pub banner_decoration: u16,
    pub banner_bg: u16,
    pub banner_frame: u16,
    /// Gearset CRC32 fingerprint; server passes through, never recomputes.
    pub checksum: u32,
}

impl Default for PortraitBanner {
    /// The "fairly standard" default banner values captured from a freshly-opened portrait on
    /// retail, matching the equivalent fields of [`PlateDesign::default()`], so the portrait
    /// renders reasonably before the player customizes it.
    fn default() -> Self {
        Self {
            has_data: 1,
            expression: 0,
            camera_zoom: 160,
            dir_light_r: 255,
            dir_light_g: 255,
            dir_light_b: 255,
            dir_light_brightness: 127,
            amb_light_r: 51,
            amb_light_g: 51,
            amb_light_b: 51,
            amb_light_brightness: 127,
            flags: 0,
            banner_timeline: 1,
            animation_progress: 0,
            head_direction_y: 0,
            head_direction_x: 0,
            eye_direction_y: 0,
            eye_direction_x: 0,
            camera_position_x: 0,
            camera_position_y: 0,
            camera_position_z: 15564,
            camera_target_x: 0,
            camera_target_y: 0,
            camera_target_z: 0,
            image_rotation: 0,
            directional_lighting_vertical: 133,
            directional_lighting_horizontal: 65459,
            banner_decoration: 1,
            banner_bg: 8,
            banner_frame: 1,
            checksum: 0,
        }
    }
}

/// One party member's job portrait slot. 176 bytes. Wire-identical across the single-slot
/// `PartyMemberPortrait` (prefixed by an 8-byte slot header) and the batch packets
/// `PartyMemberPortraits4` / `PartyMemberPortraits8` (N of these back-to-back, no header).
/// Field offsets verified against client sub_140BA7960 / sub_140BA7C90 / sub_140BA7F30.
#[binrw]
#[derive(Debug, Clone, Default)]
pub struct PartyPortraitEntry {
    /// Encrypted/session-level actor id. Kawari has no such system — send 0.
    pub encrypted_aid: u64, // entry +0
    pub content_id: u64,    // +8
    /// Bitfield: show main/off-hand & head gear when weapon sheathed (same semantics as
    /// `PlateDesign.gear_visibility_flag`).
    pub gear_visibility_flag: u8, // +16
    #[brw(pad_after = 2)] // +18..19 pad
    pub class_job_id: u8, // +17
    pub banner: PortraitBanner, // +20  (52B, checksum at entry+68)
    pub item_ids: [u32; 12],    // +72
    pub glasses: [u16; 2],      // +120
    pub customize: CustomizeData, // +124 (26B)
    pub stain0: [u8; 12],       // +150
    #[brw(pad_after = 2)] // +174..175 pad
    pub stain1: [u8; 12], // +162
}

/// One melded materia within an examine equipment entry. 4 bytes: u16 id + u16 grade.
/// Verified by capture diffing (id and grade are each 2 bytes, e.g. `0E 00 0B 00`).
#[binrw]
#[derive(Debug, Clone, Default)]
pub struct ExamineMateria {
    /// Materia catalog id (index into the Materia sheet). 0 = empty.
    pub id: u16,
    /// Materia grade (0-based; grade 11 = Materia XII). 2 bytes on the wire.
    pub grade: u16,
}

/// One equipment slot in the examine packet. 40 bytes. Slot order follows the equipment array
/// (main, off, head, body, hands, waist, legs, feet, ears, neck, right ring, left ring, soul
/// crystal). Field offsets verified against the client handler `Inspect.HandleExaminePacket`
/// (0x140c15120): catalog +0x00, glamour +0x04, crafter +0x08, materia +0x12, stains +0x26/+0x27.
#[binrw]
#[derive(Debug, Clone, Default)]
pub struct ExamineEquipEntry {
    /// Real (unglamoured) item catalog id. 0 = empty slot.
    pub catalog_id: u32, // +0x00
    /// Glamoured-to item id, or 0 for none.
    pub glamour_id: u32, // +0x04
    /// Crafter content id / signature (0 for non-signed items).
    pub crafter_content_id: u64, // +0x08
    /// High-quality flag: `1` when the item is HQ, `0` otherwise (client calls SetIsHighQuality
    /// with this). Verified: an item with materia and dye but no HQ state still has this at `00 00`.
    pub is_hq: u16, // +0x10
    /// Up to 5 melded materia.
    pub materia: [ExamineMateria; 5], // +0x12 (20 bytes)
    /// Primary dye.
    pub stain0: u8, // +0x26
    /// Secondary dye.
    pub stain1: u8, // +0x27
}

/// Entry in the `RecruitingPartyCount` packet. Parallel with `RecruitingPartyDetail`.
#[binrw]
#[derive(Debug, Clone, Copy, Default)]
pub struct RecruitingPartyEntry {
    /// Unused by client; always 0. Likely alignment padding.
    pub unk0: u32,
    /// Category bitmap. Client extracts the lowest set bit's index as the category code:
    /// 0=none, 1=roulette, 2=dungeon, 3=guildhest, 4=trial, 5=raid, 6=high-end,
    /// 7=pvp, 8=gold-saucer, 9=critical-engagement, 10=treasure-hunt, 11=hunt,
    /// 12=gathering, 13=deep-dungeon, 14=variant-dungeon, 15=criterion-dungeon.
    pub category_bits: u32,
}

/// Detail in the `RecruitingPartyCount` packet. Parallel with `RecruitingPartyEntry`.
#[binrw]
#[derive(Debug, Clone, Copy, Default)]
pub struct RecruitingPartyDetail {
    /// Row ID in ContentRoulette (if kind=1) or ContentFinderCondition (if kind=2).
    pub content_id: u16,
    /// 1 = ContentRoulette, 2 = ContentFinderCondition. Observed 0 for category code 10.
    pub content_kind: u16,
    /// Unused by client; always 0. Likely alignment padding.
    pub unk4: u32,
}

#[opcode_data(ServerZoneIpcType)]
#[binrw]
#[br(import(magic: &ServerZoneIpcType, size: &u32))]
#[derive(Debug, Clone)]
pub enum ServerZoneIpcData {
    InitResponse {
        /// The actor id of the player logging in.
        #[brw(pad_before = 8, pad_after = 4)] // empty
        actor_id: ObjectId,
    },
    ZoneInit(ZoneInit),
    ActorControlSelf(ActorControlSelf),
    PlayerStats(PlayerStats),
    PlayerSetup(PlayerSetup),
    UpdateClassInfo(UpdateClassInfo),
    SpawnPlayer(SpawnPlayer),
    LogOutComplete {
        unk: [u8; 8],
    },
    ActorSetPos(ActorSetPos),
    ServerNoticeMessage(ServerNoticeMessage),
    PrepareZoning {
        log_message: u32,
        /// What zone we're about to load into. Index into the TerritoryType Excel sheet.
        target_zone: u16,
        animation: u16,
        /// This, in conjunction with unk1, seem to influence visual effects displayed during the zoning transition. For example, when diving, param4 is 218, and unk1 is 6 (with hide_character set to 1). When surfacing, param4 is 227, unk1 6, and hide_character 1. When going through an underwater portal, param4 is 15, unk1 is 4, and hide_character is 2.
        param4: u8,
        hide_character: u8,
        /// Must match what is used in ActorSetPos (if applicable) otherwise weird stuff like EnterTerritoryEvent is sent by the client again.
        warp_type: WarpType,
        param_7: u8,
        fade_out_time: u8,
        unk1: u8,
        unk2: u16,
    },
    ActorControl(ActorControl),
    ActorMove(ActorMove),
    SocialList(SocialList),
    SpawnNpc(SpawnNpc),
    StatusEffectList(StatusEffectList),
    WeatherId(WeatherChange),
    UpdateItem(ItemInfo),
    ContainerInfo(ContainerInfo),
    EventResume2 {
        /// Data to resume this event.
        #[brw(args { max_params: 2 } )]
        data: EventResume,
    },
    EventResume4 {
        /// Data to resume this event.
        #[brw(args { max_params: 4 } )]
        data: EventResume,
    },
    EventResume8 {
        /// Data to resume this event.
        #[brw(args { max_params: 8 } )]
        data: EventResume,
    },
    EventScene2 {
        /// Data to resume this event.
        #[brw(args { max_params: 2 } )]
        data: EventScene,
    },
    EventScene4 {
        /// Data to resume this event.
        #[brw(args { max_params: 4 } )]
        data: EventScene,
    },
    EventScene8 {
        /// Data to resume this event.
        #[brw(args { max_params: 8 } )]
        data: EventScene,
    },
    EventScene16 {
        /// Data to resume this event.
        #[brw(args { max_params: 16 } )]
        data: EventScene,
    },
    EventScene32 {
        /// Data to resume this event.
        #[brw(args { max_params: 32 } )]
        data: EventScene,
    },
    EventScene64 {
        /// Data to resume this event.
        #[brw(args { max_params: 64 } )]
        data: EventScene,
    },
    EventScene128 {
        /// Data to resume this event.
        #[brw(args { max_params: 128 } )]
        data: EventScene,
    },
    EventScene255 {
        /// Data to resume this event.
        #[brw(args { max_params: 255 } )]
        data: EventScene,
    },
    EventStart(EventStart),
    UpdateHpMpTp {
        /// The new health point value.
        hp: u32,
        /// The new resource point value.
        mp: u16,
        // Unknown. It's filled with... something.
        unk: u16,
    },
    ActionEffect1(ActionEffect1),
    Equip(Equip),
    DeleteActor {
        /// The index into the client-side object pool.
        spawn_index: u8,
        /// The ID of the actor being deleted.
        #[brw(pad_before = 3)] // padding
        actor_id: ObjectId,
    },
    EventFinish {
        /// ID of this event.
        handler_id: HandlerId,
        /// Type of this event.
        event_type: EventType,
        /// Arbitrary value.
        result: u8,
        /// Arbitrary value.
        #[brw(pad_before = 2)] // padding
        #[brw(pad_after = 4)] // padding
        arg: u32,
    },
    Condition(Conditions),
    ActorControlTarget(ActorControlTarget),
    CurrencyCrystalInfo(CurrencyInfo),
    Config(Config),
    InventoryActionAck {
        sequence: u32,
        #[brw(pad_after = 10)]
        action_type: u16,
    },
    PingSyncReply {
        timestamp: u32,
        #[brw(pad_after = 24)]
        transmission_interval: u32,
    },
    QuestCompleteList {
        /// Bitmask of completed quests.
        #[br(count = COMPLETED_QUEST_BITMASK_SIZE)]
        #[bw(pad_size_to = COMPLETED_QUEST_BITMASK_SIZE)]
        completed_quests: Vec<u8>,
        #[brw(pad_after = 1)] // unused I guess
        #[br(count = UNLOCKED_MAP_MARKERS_BITMASK_SIZE)]
        #[bw(pad_size_to = UNLOCKED_MAP_MARKERS_BITMASK_SIZE)]
        unlocked_map_markers: Vec<u8>,
    },
    UnkResponse2 {
        #[brw(pad_after = 7)]
        unk1: u8,
    },
    InventoryTransaction {
        /// This is later reused in InventoryTransactionFinish, so it might be some sort of sequence or context id, but it's not the one sent by the client.
        sequence: u32,
        /// Same as the one sent by the client, not the one that the server responds with in InventoryActionAck!
        operation_type: ItemOperationKind,
        src_actor_id: ObjectId,
        #[brw(pad_size_to = 4)]
        src_storage_id: ContainerType,
        src_container_index: u16,
        #[brw(pad_before = 2)]
        src_stack: u32,
        src_catalog_id: u32,

        /// This section was observed to be static, across two captures and a bunch of discards these never changed.
        /// Always set to 0xE000_0000, also known as no/invalid actor.
        dst_actor_id: ObjectId,
        /// Used in discard operations, both this dummy container and dst_storage_id are set to a container type of 0xFFFF.
        /// While this struct is nearly identical to ItemOperation, it deviates here by not having 2 bytes of padding.
        dummy_container: ContainerType,
        dst_storage_id: ContainerType,
        dst_container_index: u16,
        /// Always set to zero.
        #[brw(pad_before = 2)]
        dst_stack: u32,
        /// Always set to zero.
        dst_catalog_id: u32,
    },
    InventoryTransactionFinish {
        /// Same sequence value as in InventoryTransaction.
        sequence: u32,
        /// Repeated unk1 value. No, it's not a copy-paste error.
        sequence_repeat: u32,
        /// Unknown, seems to always be 0x00000090.
        unk1: u32,
        /// Unknown, seems to always be 0x00000200.
        unk2: u32,
    },
    ContentFinderUpdate {
        /// ContentsFinderQueueState (payload offset 0). Observed values:
        /// 0 = Nothing happens
        /// 1 = Reserving server / pending
        /// 2 = again? ^
        /// 3 = duty ready
        /// 4 = checking member status
        /// nothing appears to happen above 5
        queue_state: u8,
        /// The class you registered with (payload offset 1). Index into the ClassJob Excel sheet.
        classjob_id: u8,
        /// Selected match languages (payload offset 2).
        languages: u8,
        /// Unknown (payload offset 3-7).
        unk_3_7: [u8; 5],
        /// The `DutyFinderSetting` bitfield as a little-endian u64 "mode word" (payload offset
        /// 8-15). The client renders the ready-popup mode icon from it; bit 0x20 additionally gates
        /// ContentsFinderQueueInfo+0x5E (the withdraw-penalty dialog).
        settings: u64,
        /// QueuedContentRouletteId (payload offset 16). 0 = not a roulette.
        roulette_id: u8,
        /// Unknown (payload offset 17).
        unk_17: u8,
        /// Unknown (payload offset 18); retail = 1.
        unk_18: u8,
        /// Ready flag (payload offset 19); bit 0 = began-queue/ready.
        ready_flag: u8,
        /// The content IDs you registered for. Index into the ContentFinderCondition Excel sheet.
        /// The client reads these as 5 x u32 (DWORDs) at payload offset 20-39.
        content_ids: [u32; 5],
    },
    ContentFinderFound {
        /// ContentsFinderQueueState (payload offset 0); 3 = duty ready.
        queue_state: u8,
        /// Unknown (payload offset 1-7).
        unk_1_7: [u8; 7],
        /// The `DutyFinderSetting` bitfield as a little-endian u64 "mode word" (payload offset
        /// 8-15). The client renders the ready-popup mode icon from it.
        settings: u64,
        /// PoppedContentInProgressStartTimestamp (payload offset 16-23).
        in_progress_start_timestamp: u64,
        /// Unknown (payload offset 24); retail/Kawari = 1.
        unk_24: u8,
        /// Unknown (payload offset 25-27).
        unk_25_27: [u8; 3],
        /// The content ID that popped (PoppedQueueEntry / ContentFinderConditionId, payload offset
        /// 28-31). The client reads this as a DWORD.
        content_id: u32,
        /// Unknown (payload offset 32-35).
        unk_32_35: u32,
        /// Unknown (payload offset 36-37).
        unk_36_37: u16,
        /// Unknown (payload offset 38-39).
        unk_38_39: u16,
    },
    SpawnObject(SpawnObject),
    ActorGauge {
        /// The class (ideally the one you actually are) to update the gauge for. Index into the ClassJob Excel sheet.
        classjob_id: u8,
        /// Class-specific gauge data: a little-endian `u64` (the client's `ActorGauge.Payload`). On
        /// the wire it sits one pad byte after `classjob_id` (so at byte 2), followed by 6 trailing
        /// pad bytes — a 16-byte payload total. The leading pad and the length both matter: without
        /// them the gauge is read off-by-one / the packet is dropped, and the gauge stays blank.
        #[brw(pad_before = 1, pad_after = 6)]
        data: u64,
    },
    FreeCompanyInfo {
        unk: [u8; 80],
    },
    TitleList {
        /// Bitmask of unlocked titles.
        #[br(count = TITLE_UNLOCK_BITMASK_SIZE)]
        #[bw(pad_size_to = TITLE_UNLOCK_BITMASK_SIZE)]
        unlock_bitmask: Vec<u8>,
    },
    QuestActiveList(QuestActiveList),
    GlamourDresserContents(GlamourDresserContents),
    GlamourPlates(GlamourPlates),
    GlamourPlateSaveAck(GlamourPlateSaveAck),
    LevequestCompleteList {
        /// Bitmask of completed levequests.
        #[br(count = COMPLETED_LEVEQUEST_BITMASK_SIZE)]
        #[bw(pad_size_to = COMPLETED_LEVEQUEST_BITMASK_SIZE)]
        completed_levequests: Vec<u8>,
        #[br(count = 6)]
        #[bw(pad_size_to = 6)]
        unk2: Vec<u8>,
    },
    ShopLogMessage {
        /// Event ID of this shop.
        handler_id: HandlerId,
        /// When buying: 0x697
        /// When selling: 0x698
        /// When buying back: 0x699
        message_type: u32,
        /// Always 3, regardless of the interactions going on
        params_count: u32,
        item_id: u32,
        item_quantity: u32,
        #[brw(pad_after = 8)]
        total_sale_cost: u32,
    },
    LogMessage {
        handler_id: HandlerId,
        /// Non-stackable item or a single item: 750 / 0x2EE ("You obtained a .")
        /// Stackable item: 751 / 0x2EF ("You obtained .")
        message_type: u32,
        /// Always 2
        params_count: u32,
        item_id: u32,
        #[brw(pad_after = 4)]
        /// Set to zero if only one item was obtained (stackable or not)
        item_quantity: u32,
    },
    UpdateInventorySlot(ItemInfo),
    EffectResult(EffectResult),
    ContentFinderCommencing {
        /// ContentsFinderQueueState for the recipient (payload offset 0). 3 = the recipient has not
        /// accepted yet, 4 = the recipient has accepted. Broadcast to every online party member on
        /// each acceptance event, each carrying that recipient's own state.
        state: u32,
        /// Unknown (payload offset 4); always 1 per capture.
        unk_4: u32,
        /// The content ID being commenced (payload offset 8). Index into the ContentFinderCondition
        /// Excel sheet.
        content_id: u32,
        /// Role queue counts, payload offset 12-19, sharing DOWN_CFDutyInfo's layout. Retail sends
        /// the role slots (12-17) as 0 in the commence/ready packet (unrestricted parties aren't
        /// role-gated); only queued_players/total_needed_players carry the ready-popup fraction.
        queued_tanks: u8,
        needed_tanks: u8,
        queued_healers: u8,
        needed_healers: u8,
        queued_dps: u8,
        needed_dps: u8,
        /// Ready-popup numerator = QueuedPlayers, rises as members accept (payload offset 18,
        /// = DOWN_CFDutyInfo QueuedPlayers).
        accepted_count: u8,
        /// Ready-popup denominator = TotalNeededPlayers, the party size (payload offset 19,
        /// = DOWN_CFDutyInfo TotalNeededPlayers).
        total_count: u8,
        /// Unknown (payload offset 20-23).
        unk_20_23: [u8; 4],
    },
    StatusEffectList3 {
        /// List of status effects.
        status_effects: [StatusEffect; 30],
    },
    CrossworldLinkshells {
        /// List of cross-world linkshells.
        #[brw(pad_before = 8)] // Seems to be empty/zeroes
        #[br(count = CrossworldLinkshell::COUNT)]
        #[brw(pad_size_to = CrossworldLinkshell::COUNT * CrossworldLinkshell::SIZE)]
        linkshells: Vec<CrossworldLinkshell>,
    },
    SetSearchInfo(SearchInfo),
    /// Echoes the player's own search info back after they edit it. Same fields as
    /// [`SearchInfo`], but the wire layout differs: here `unk1` is 13 bytes (vs 9) and the
    /// trailing padding is 134 bytes (vs 138), keeping the total at 216 bytes.
    UpdateSearchInfo {
        online_status: OnlineStatusMask,
        unk1: [u8; 13],
        selected_languages: SocialListUILanguages,
        #[brw(pad_size_to = 60)]
        #[br(count = 60)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        comment: String,
        #[br(count = 134)]
        #[bw(pad_size_to = 134)]
        unk: Vec<u8>,
    },
    Blacklist(Blacklist),
    WalkInEvent {
        /// Object ID of the ClientPath in the zone.
        path_id: u32,
        unk2: u16,
        #[brw(pad_before = 2)]
        unk3: u16,
        /// In some unknown amount of units.
        speed: u16,
        /// Always seems to be 1.
        constant: u16,
        unk4: u16,
        #[brw(pad_after = 4)]
        unk5: u32,
    },
    GrandCompanyInfo {
        /// Which Grand Company this player is affiliated with.
        active_company_id: GrandCompany,
        /// Maelstrom rank.
        maelstrom_rank: u8,
        /// Twin Adder rank.
        twin_adder_rank: u8,
        /// Immortal Flames rank.
        #[brw(pad_after = 4)]
        immortal_flames_rank: u8,
    },
    CraftingLog {
        #[brw(pad_after = 7)] // unaccounted for in the CS size
        #[br(count = COMPLETED_RECIPES_BITMASK_SIZE)]
        #[bw(pad_size_to = COMPLETED_RECIPES_BITMASK_SIZE)]
        bitmask: Vec<u8>,
    },
    GatheringLog {
        #[br(count = GATHERED_GATHERING_ITEMS_BITMASK_SIZE)]
        #[bw(pad_size_to = GATHERED_GATHERING_ITEMS_BITMASK_SIZE)]
        bitmask: Vec<u8>,
    },
    Fellowships {
        unk1: [u8; 808],
    },
    DailyQuests {
        unk1: [u8; 56],
    },
    DailyQuestRepeatFlags {
        unk1: [u8; 8],
    },
    Linkshells {
        /// List of linkshells.
        #[br(count = LinkshellEntry::COUNT)]
        #[bw(pad_size_to = LinkshellEntry::SIZE * LinkshellEntry::COUNT)]
        shells: Vec<LinkshellEntry>,
    },
    ChatMessage(ChatMessage),
    LocationDiscovered {
        map_part_id: u32,
        map_id: u32,
    },
    Mount {
        /// Index into the Mount Excel sheet.
        id: u16,
        unk1: [u8; 14],
    },
    SetOnlineStatus(OnlineStatusMask),
    FreeCompanyGreeting {
        unk: u8, // TODO: What is this? Seems to commonly be 0x01 or 0x02. Could this opcode be used as a general updater? Needs more research.
        #[brw(pad_size_to = 192)]
        #[br(count = 192)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 7)]
        message: String,
    },
    CharaInfoFromContentIds {
        #[brw(pad_before = 8)] // empty
        #[br(count = 10)]
        #[bw(pad_size_to = 10 * CharaInfoFromContentIdsData::SIZE)]
        info: Vec<CharaInfoFromContentIdsData>,
    },
    InviteCharacterResult {
        /// The invited character's content id.
        content_id: u64,
        /// The pre-defined LogMessage to display. 0 seems to indicate no errors, and the client will display a default message such as "You invite <name> to a party."
        message_id: u16,
        #[brw(pad_before = 2)]
        /// The invited character's home world id.
        world_id: u16,
        /// The type of social invite that was sent.
        invite_type: InviteType,
        unk1: u8, // TODO: What is this?
        /// The invited character's name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        character_name: String,
    },
    InviteReplyResult {
        content_id: u64,
        #[brw(pad_before = 4)]
        invite_type: InviteType,
        response: InviteReply,
        unk1: u8,
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 1)]
        character_name: String,
    },
    InviteUpdate {
        sender_account_id: u64,
        #[brw(pad_after = 8)] // empty
        sender_content_id: u64,
        expiration_timestamp: u32, // usually the packet's timestamp + 300
        world_id: u16,
        #[brw(pad_after = 1)] // Pretty sure this is empty
        invite_type: InviteType,
        update_type: InviteUpdateType,
        unk1: u8, // TODO: Usually 1? What is this?
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 6)] // empty
        sender_name: String,
    },
    /// This opcode informs the client about members leaving, joining, going offline, if the party is disbanding, and even handles ready checking directly within it. When a ready check is initiated, the target_content_id field is treated differently and used to keep track of the party's votes. While further information can be found below on the unk2 field, most of this process is described in more detail in party_misc.rs, on the ReadyCheckReply struct.
    PartyUpdate {
        execute_account_id: u64,
        target_account_id: u64,
        execute_content_id: u64,
        target_content_id: u64,
        unk1: u8, // TODO: Usually 1? What is this?
        /// This field seems to control what "mode" the target_content_id field operates in. During ready checks, this field is set to zero, and 2 otherwise. It's unclear at this time what 2 represents. When this field is set to zero, the client seems to treat the target_content_id as a pseudo-array of 8 bytes that indicate the party's yes or no votes for ready checks.
        unk2: u8,
        update_status: PartyUpdateStatus,
        unk3: u8, // TODO: Usually 2? What is this?
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        execute_name: String,
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 3)] // empty
        target_name: String,
    },
    PartyList {
        #[br(count = PartyMemberEntry::NUM_ENTRIES)]
        #[bw(pad_size_to = PartyMemberEntry::NUM_ENTRIES * PartyMemberEntry::SIZE)]
        members: Vec<PartyMemberEntry>,
        party_id: u64,
        party_chatchannel: ChatChannel,
        leader_index: u8,
        #[brw(pad_after = 6)]
        member_count: u8,
    },
    PartyMemberPositions(PartyMemberPositions),
    AcceptQuest {
        /// Row ID - 65535
        #[brw(pad_after = 4)]
        quest_id: u32,
    },
    UpdateQuest {
        // TODO: index into what?
        #[brw(pad_after = 3)]
        index: u8,
        #[brw(pad_after = 4)] // seems empty
        quest: ActiveQuest,
    },
    FinishQuest {
        /// Row ID - 65535
        quest_id: u16,
        flag1: u8,
        #[brw(pad_after = 4)]
        flag2: u8,
    },
    UpdateMapMarkers8 {
        #[brw(args { max_params: 8 } )]
        data: MapMarkers,
    },
    UpdateMapMarkers16 {
        #[brw(args { max_params: 16 } )]
        data: MapMarkers,
    },
    UpdateMapMarkers32 {
        #[brw(args { max_params: 32 } )]
        data: MapMarkers,
    },
    QuestTracker(QuestTracker),
    HouseList(HouseList),
    HousingWardInfo(HousingWardInfo),
    HousingOccupiedLandInfo(HousingOccupiedLandInfo),
    HousingVacantLandInfo(HousingVacantLandInfo),
    HousingEstateGreeting(HousingEstateGreeting),
    ScenarioGuide {
        /// Not sure what this controls.
        quest_id_1: u32,
        /// Quest ID (Row ID - 65535) shown in big text. The next job quest is automatically determined.
        next_quest_id: u32,
        /// The game object ID to center on when opening the map.
        #[brw(pad_before = 4, pad_after = 16)] // seems empty
        layout_id: u32,
    },
    LegacyQuestList {
        #[brw(pad_after = 1)] // unaccounted for in the CS size
        #[br(count = COMPLETED_LEGACY_QUEST_BITMASK_SIZE)]
        #[bw(pad_size_to = COMPLETED_LEGACY_QUEST_BITMASK_SIZE)]
        bitmask: Vec<u8>,
    },
    DirectorVars {
        /// ID of this director.
        handler_id: HandlerId,
        flag: u8,
        branch: u8,
        data: [u8; 10],
        unk1: u16,
        unk2: u16,
        unk3: u16,
        unk4: u16,
    },
    UnkDirector1 {
        unk: [u8; 32],
    },
    /// A single party member's job portrait (8-byte slot header + one entry, 184 bytes), used to
    /// fill an individual slot when a batch packet doesn't cover the whole party (under-sized /
    /// unrestricted parties). The structure is defined but the server does not yet send these.
    PartyMemberPortrait {
        /// Target slot 0..=7. Client discards the packet if >= 8.
        #[brw(pad_after = 7)] // reserved
        slot_index: u8,
        entry: PartyPortraitEntry,
    },
    /// Batch portraits for a 4-member duty: 4 entries (slots 0..=3), no header.
    PartyMemberPortraits4 {
        portraits: [PartyPortraitEntry; 4],
    },
    /// Batch portraits for an 8-member duty: 8 entries (slots 0..=7), no header.
    PartyMemberPortraits8 {
        portraits: [PartyPortraitEntry; 8],
    },
    FieldMarkerPreset(WaymarkPreset),
    DeleteObject {
        /// Index into the client-side object spawn pool.
        #[brw(pad_after = 7)] // padding
        spawn_index: u8,
    },
    GoldSaucerInformation {
        unk: [u8; 40],
    },
    UnkContentFinder {
        /// Attribution content id (payload offset 0). 0 in every observed cancel capture (no name
        /// attribution).
        content_id: u64,
        /// LogMessage sheet id explaining the cancel (payload offset 8): 0x037A = the withdrawer,
        /// 0x0373 = the other members, 0x037C = the timed-out member.
        log_message_id: u32,
        /// Unknown (payload offset 12).
        unk_12: u32,
    },
    TrustInformation(TrustInformation),
    DutySupportInformation {
        /// List indices into the DawnContent Excel sheet.
        #[br(count = 80)]
        #[bw(pad_size_to = 80)]
        available_content: Vec<u8>,
    },
    PortraitsInformation {
        unk: [u8; 56],
    },
    InitializeObfuscation {
        unk_before: [u8; 6],
        /// Zero means "no obsfucation" (not really, but functionally yes.)
        /// To enable obsfucation, you need to set this to a constant that changes every patch. See lib.rs for the constant.
        obsfucation_mode: u8,
        /// First seed used in deobsfucation on the client side.
        seed1: u8,
        /// Second seed used in deobsfucation on the client side.
        seed2: u8,
        #[brw(pad_before = 3)] // seems empty
        /// Third seed used in deobsfucation on the client side.
        seed3: u32,
    },
    StrategyBoardReceivedAck {
        /// The client ID of the player who received the board.
        content_id: u64,
        #[brw(pad_after = 4)] // Seems to be empty/always zeroes
        /// Unknown, possibly a result value. Observed as 1.
        unk: u32,
    },
    BeginStrategyBoardSession {
        /// All of these unknowns are possibly booleans or bitflags. See zone_connection/social.rs::received_strategy_board.
        unk1: u32,
        unk2: u32,
        #[brw(pad_after = 4)] // Seems to be empty/always zeroes
        unk3: u32,
    },
    StrategyBoard {
        /// The content id of the sending player.
        content_id: u64,
        /// The strategy board data.
        board_data: StrategyBoard,
    },
    StrategyBoardUpdate(StrategyBoardUpdate),
    EndStrategyBoardSession {
        unk: [u8; 16], // Always zeroes?
    },
    WaymarkUpdate {
        /// The id number of this waymark. 0 = A, 1 = B, and so on.
        id: u8,
        #[brw(pad_after = 2)] // Empty/always zeroes?
        /// The placement mode of this waymark.
        placement_mode: WaymarkPlacementMode,
        /// The waymark's position in the world.
        pos: WaymarkPosition,
    },
    FreeCompanyHierarchy {
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        leader_name: String,

        #[br(count = 16)]
        #[bw(pad_size_to = 16 * FcHierarchy::SIZE)]
        hierarchy_list: Vec<FcHierarchy>,
    },
    FreeCompanyShortMessage {
        /// The content id of the requested character.
        content_id: u64,
        /// A value the client sends, repeated back to the client.
        sequence: u32,
        /// A 32-bit Unix timestamp indicating when the message was last updated.
        time_last_updated: u32,
        #[brw(pad_size_to = 96)]
        #[br(count = 96)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 8)] // Empty/zeroes
        /// The requested character's FC short message.
        short_message: String,
    },
    FreeCompanyHeader {
        /// The company's ID number. It can also be found in SocialList responses.
        company_id: u64,
        /// The company's crest ID number. Presumably used in places where the company logo is shown.
        crest_id: u64,
        /// Unknown purpose. Possibly for rankings on the Lodestone?
        company_points: u64,
        /// How many company credits the company has to spend on purchases (actions, misc. items, etc.).
        company_credits: u64,
        /// The company's standing with the Grand Company they're allied to.
        reputation: u32,
        /// The amount of points required to rank up.
        next_point: u32,
        /// How many points the company has towards their next rank up.
        current_point: u32,
        /// How many members the company has in total.
        total_members: u16,
        /// How many members in the company are currently online.
        online_members: u16,
        /// The Grand Company this fc is aligned with.
        gc_id: GrandCompany,
        /// The company's current rank (out of 30).
        fc_rank: u8,
        #[brw(pad_size_to = 22)]
        #[br(count = 22)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        /// The company's full name.
        company_name: String,
        #[brw(pad_size_to = 6)]
        #[br(count = 6)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 2)] // Empty/zeroes
        /// The company's short tag.
        company_tag: String,
    },
    FreeCompanyActivityList {
        unk: [u8; 528],
    },
    UnkContentFinder2 {
        /// Index into the ContentFinderCondition Excel sheet.
        content_finder_condition_id: u32,
        unk: [u8; 12],
    },
    Playtime {
        #[brw(pad_after = 4)] // Empty/zeroes
        /// The character's total cumulative playtime, measured in minutes.
        duration: u32,
    },
    Countdown {
        /// The account id of the character that started the countdown.
        account_id: u64,
        /// The content id of the character that started the countdown.
        content_id: u64,
        /// The actor id of the character that started the countdown.
        starter_actor_id: ObjectId,
        unk: u16, // Could be a u8 with padding? Seems to always be 0x5B.
        /// The duration of the countdown in seconds.
        #[brw(pad_after = 3)]
        duration: u16,
        /// The name of the character that started the countdown.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 5)]
        starter_name: String,
    },
    DirectorPopupMessage {
        unk1: u64, // Empty?
        /// Should be the ID of the instance's director.
        handler_id: HandlerId,
        /// Index into the BNPCName Excel sheet.
        npc_name: u32,
        /// Index into the InstanceContentTextData Excel sheet.
        text_data_id: u32,
        unk4: u32,
        unk5: u32,
        unk6: u32,
        unk7: u32,
        unk8: u32,
    },
    DirectorSetupMapEffects64 {
        /// The map effects to setup.
        #[brw(args { max_params: 64 } )]
        data: MapEffects,
    },
    DirectorSetupMapEffects128 {
        /// The map effects to setup.
        #[brw(args { max_params: 128 } )]
        data: MapEffects,
    },
    DirectorMapEffect {
        /// Should be the ID of the instance's director.
        handler_id: HandlerId,
        /// The new state of this map effect. Also used as a fallback if `timeline_id` doesn't work.
        /// Note that this seems to be an arbitrary value, it needs to be different than the current state - otherwise the current animation restarts?
        state: u16,
        /// Which timeline_id to start playing. This is an index into `timeline_indices` of the SGB.
        timeline_id: u16,
        /// The index of the map effect item to change.
        #[brw(pad_after = 7)] // padding, not read by the client
        index: u8,
    },
    FurnitureList(FurnitureList),
    OwnedHousing {
        #[brw(pad_after = 8)] // believe these are always empty?
        unk1: LandData,
        #[brw(pad_after = 8)]
        unk2: LandData,
        #[brw(pad_after = 8)]
        unk3: LandData,
        unk4: LandData,
        #[brw(pad_after = 8)]
        unk5: LandData,
        /// Your apartment unit.
        #[brw(pad_after = 8)]
        apartment: LandData,
    },
    UpdateFittingShop {
        /// Corresponds to the DisplayId column in the FittingShopCategoryItem Excel sheet.
        #[brw(pad_after = 8)] // empty
        display_ids: [u8; 8],
    },
    UnkClassRelated {
        #[brw(pad_after = 3)]
        classjob_id: u8,
        class_level: u16,
        current_level: u16,
    },
    EnmityList(EnmityList),
    HaterList(HaterList),
    DuelInformation {
        account_id: u64,
        /// The opponent's content ID.
        opponent_content_id: u64,
        /// The opponent's object ID.
        opponent_object_id: ObjectId,
        world_id: u16,
        unk1: u16,
        unk2: u8,
        /// The name of the opponent.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 7)] // empty
        opponent_name: String,
    },
    MarketBoardItems {
        /// The items contained in this search result.
        #[br(count = 21)]
        #[brw(pad_size_to = 21 * MarketBoardItem::SIZE)]
        items: Vec<MarketBoardItem>,
        /// Sequence number for this search result.
        #[brw(pad_before = 4, pad_after = 2)] // empty
        sequence: u16,
    },
    EffectResultBasic {
        unk1: u32,
        unk2: u32,
        target_id: ObjectId,
        current_hp: u32,
        unk3: u32,
        unk4: u32,
    },
    ActionEffect8(Box<ActionEffect8>),
    ActionEffect16(Box<ActionEffect16>),
    ActionEffect24(Box<ActionEffect24>),
    ActionEffect32(Box<ActionEffect32>),
    ActorCast {
        /// Usually the same as `action_id`.
        spell_id: u16,
        /// What kind of action is being cast.
        action_type: ActionType,
        /// Omen Delay is for the extra effects that appear when casting, usually telegraphs, like Titan's Landslide line. If you increase that value, the line that usually immediately shows in front of him is delayed.
        omen_delay: u8,
        /// Index into the Action Excel sheet.
        action_id: u32,
        /// Cast time in seconds.
        cast_time: f32,
        /// The target of this cast.
        target: ObjectId,
        /// The cast's rotation.
        #[br(map = read_quantized_rotation)]
        #[bw(map = write_quantized_rotation)]
        rotation: f32,
        /// If true, shows special VFX around the cast bar to show that's interruptible (it currently pulsates.) This is only applicable for other targets.
        #[br(map = read_bool_from::<u16>)]
        #[bw(map = write_bool_as::<u16>)]
        interruptible: bool,
        /// Only used when ActionCategory is 11.
        ballista_entity_id: ObjectId,
        /// Position of the caster.
        #[brw(pad_after = 2)] // empty
        #[br(map = read_packed_position)]
        #[bw(map = write_packed_position)]
        position: Position,
    },
    SearchPlayersResult {
        /// The number of results found after a player search.
        #[brw(pad_after = 4)] // empty
        num_results: u32, // TODO: this might be only an u16 or an u8, since the search results window only shows up to 200 players.
    },
    FriendGroupIcon(FriendGroupIconInfo),
    DeepDungeonParty {
        /// Refers to the player actors in your party, including yourself.
        entity_ids: [ObjectId; 4],
        room_indices: [u8; 4],
    },
    DeepDungeonChests {
        types: [u8; 16],
        room_indices: [u8; 16],
    },
    DeepDungeonSetup {
        bonus_loot_item_id: u32,
        unk1: u8,
        unk2: u8,
        weapon_level: u8,
        armor_level: u8,
        return_progress: u8,
        passage_progress: u8,
        synced_gear_level: u8,
        hoard_count: u8,
        unk3: u8,
        unk4: u8,
        gimmick_effect_id_current: u8,
        gimmick_effect_id_next: u8,
        unk5: [u8; 8],
    },
    DeepDungeonMap {
        layout_initialization_type: u8,
        deep_dungeon_status_id: u8,
        deep_dungeon_ban_id: u8,
        deep_dungeon_danger_id: u8,
        unk1: u8,
        unk2: u8,
        map_data: [DeepDungeonRoomFlag; 25],
    },
    /// Sent in response to examining another character. 944 bytes. Field offsets verified against
    /// the client handler `Inspect.HandleExaminePacket` (0x140c15120). Only the fields needed for
    /// the examine window are modelled; the rest is opaque padding.
    ///
    /// NOTE: retail scrambles this packet's header/name/equipment when packet obfuscation is
    /// active. Kawari sends plaintext (obfuscation is off by default); if obfuscation is ever
    /// enabled, `scramble_packet` needs an arm for this opcode.
    ExamineCharacterInformation {
        /// Examine kind / entity discriminator (4 = a normal player character).
        examine_kind: u8, // 0x00
        /// Sex (0 = male, 1 = female). Maps to Inspect+0x74.
        sex: u8, // 0x01
        /// Current class/job (index into the ClassJob sheet). Not derived from the soul crystal.
        class_job_id: u8, // 0x02
        /// Current level.
        level: u8, // 0x03
        /// Synced (level-capped) level, e.g. inside level-synced content. Maps to Inspect+0x77.
        #[brw(pad_after = 1)] // 0x05 pad
        synced_level: u8, // 0x04
        /// Title id (index into the Title sheet). Verified: 0x06.
        title_id: u16, // 0x06
        /// Grand company affiliation (index into the GrandCompany sheet; 1=Maelstrom, 2=Adders,
        /// 3=Flames, 0=none). Verified: 0x08.
        grand_company: u8, // 0x08
        /// Grand company rank. Verified: 0x09.
        gc_rank: u8, // 0x09
        /// Gear visibility flag (visor/headgear/weapon). Verified: 0x0A.
        gear_visibility_flag: u8, // 0x0A
        #[brw(pad_before = 5)] // 0x0B..0x0F opaque
        /// FC crest data (packet 0x10→Inspect+0x230). Not content_id despite earlier assumption.
        fc_crest_data: u64, // 0x10
        /// FC crest bitfield (packet 0x18→Inspect+0x238).
        fc_crest_bitfield: u8, // 0x18
        #[brw(pad_before = 7)] // 0x19..0x1F pad
        /// Main-hand weapon model (Id/Type/Variant/Stain0/Stain1). Drives the 3D model weapon.
        main_weapon_model: WeaponModelId, // 0x20
        /// Off-hand weapon model. Same encoding.
        sub_weapon_model: WeaponModelId, // 0x28
        #[brw(pad_before = 2)] // 0x30..0x31 pad
        /// Home world id (index into the World sheet).
        world_id: u16, // 0x32
        #[brw(pad_before = 20)] // 0x34..0x47 opaque
        /// Total equipped item level (average). Verified: 0x48. The client displays this value
        /// directly rather than recomputing it from the gear.
        item_level: u16, // 0x48
        #[brw(pad_before = 6)] // 0x4A..0x4F pad
        /// The 14 equipment slots (main, off, head, body, hands, waist, legs, feet, ears, neck,
        /// right ring, left ring, soul crystal), in EquipSlot order. 0x50..0x27F.
        equipment: [ExamineEquipEntry; 14],
        /// The examined player's name. 0x280, 32 bytes null-padded.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        name: String,
        /// Console online id (PSN Online-ID / Xbox Gamertag), maps to Inspect+0x54. Empty (all
        /// zero) on non-console platforms such as the CN client. 0x2A0.
        online_id: [u8; 32],
        /// The examined player's appearance. 0x2C0, 26 bytes.
        customize: CustomizeData,
        /// Equipment model ids (Id/Variant/Stain0) for the 3D model dress-up, in order:
        /// head, body, hands, legs, feet, ears, neck, wrists, left ring, right ring. 0x2DC.
        /// The 0x50 catalog array only feeds the item/tooltip UI; THIS drives the rendered gear.
        #[brw(pad_before = 2)] // 0x2DA..0x2DB pad
        equipment_models: [LegacyEquipmentModelId; 10], // 0x2DC (10x4)
        /// Second dye per equipment slot (same order as `equipment_models`). 0x304.
        equipment_model_stains1: [u8; 10], // 0x304
        /// Facewear (glasses) ids. 0x30E.
        glasses_ids: [u16; 2], // 0x30E
        /// Remaining tail (FC name, content key/value block, etc.), sent as zeroes. 0x312..0x3AF.
        tail: [u8; 158],
    },
    /// Sent in response to `ExamineRequestComments`. 200 bytes: the examined player's actor id
    /// followed by their search comment (empty comment => all zeroes).
    ExamineCharacterComments {
        actor_id: ObjectId, // 0x00
        /// The examined player's search comment. Occupies the remaining 196 bytes.
        #[brw(pad_size_to = 196)]
        #[br(count = 196)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        comment: String,
    },
    /// Sent in response to `RequestFreeCompanyShortInfo`. 336 bytes carrying a player's free
    /// company info (shown in the Examine window and the context-menu FC info view). Kawari has no
    /// free company system, so the FC fields are always zero, which the client renders as "not in
    /// a free company"; only `content_id` and `actor_id` are populated.
    // Field layout cross-verified against three retail captures (黑涡团 / 双蛇党, member counts
    // 155/8/299, creation dates 2019/2025). Kawari has no free company system, so every field is
    // sent as zero (the client renders "not in a free company"); the named fields document the real
    // wire layout for when a free company system exists.
    FreeCompanyShortInfo {
        content_id: u64, // 0x00
        /// Free company id.
        fc_id: u64, // 0x08
        fc_crest_data: u64, // 0x10
        plot_num: u16, // 0x18
        ward_num: u16, // 0x1A
        estate_zone: u16, // 0x1C
        world: u16, // 0x1E
        /// Not filled when the request came in by content id.
        actor_id: ObjectId, // 0x20
        /// FC creation time (time_t, 32-bit). Verified: 0x24.
        fc_create_time: u32, // 0x24
        /// Constant 0x08049C7 across all captures (crest/marker of some kind).
        unk_28: u32, // 0x28
        /// Total member count. Verified: 0x2C.
        fc_total_members: u16, // 0x2C
        /// Online member count. Verified: 0x2E.
        fc_online_members: u16, // 0x2E
        fc_profile_focus: u16, // 0x30
        fc_profile_seeking: u16, // 0x32
        fc_profile_active: u8, // 0x34
        fc_profile_recruitment: u8, // 0x35
        grand_company: u8, // 0x36
        unk_37: u8, // 0x37
        /// FC rank/level. Verified: 0x38 == 30 in all captures.
        fc_level: u8, // 0x38
        unk_39: u8, // 0x39
        fc_name: [u8; 0x16],       // 0x3A
        fc_short_name: [u8; 0x07], // 0x50
        fc_owner_name: [u8; 0x20], // 0x57
        fc_comments: [u8; 0xC1],   // 0x77
        fc_housing_name: [u8; 0x18], // 0x138..0x14F
    },
    OtherSearchInfo {
        /// The requested player's content ID.
        content_id: u64,
        unk1: [u8; 26], // seems empty but not 100%
        /// The requested player's home world. Index into the World Excel sheet.
        world_id: u16,
        /// The requested player's search comment.
        #[brw(pad_size_to = 60)]
        #[br(count = 60)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        comment: String,
        unk2: [u8; 157], // also seems empty
        /// The requested player's Grand Company rank.
        grand_company_rank: u8,
        unk3: [u8; 2], // probably also empty
        /// The requested player's class levels.
        classjob_levels: [(u16, u16); AVAILABLE_CLASSJOBS],
    },
    SetPlayerCustomizeData(CustomizeData),
    CrossworldLinkshellsEx {
        #[brw(pad_before = 8)] // Seems to be empty/zeroes
        #[br(count = CrossworldLinkshellEx::COUNT)]
        #[brw(pad_size_to = CrossworldLinkshellEx::COUNT * CrossworldLinkshellEx::SIZE)]
        linkshells: Vec<CrossworldLinkshellEx>,
    },
    CrossworldLinkshellMemberList {
        linkshell_id: u64,
        #[brw(pad_after = 2)] // Seems to be empty/zeroes
        sequence: u16,
        next_index: u16,
        current_index: u16,
        #[br(count = CWLSMemberListEntry::COUNT)]
        #[brw(pad_size_to = CWLSMemberListEntry::COUNT * CWLSMemberListEntry::SIZE)]
        members: Vec<CWLSMemberListEntry>,
    },
    SpawnTreasure(SpawnTreasure),
    OpenedTreasure {
        unk1: u32,
        unk2: u32,
        unk3: u32,
        unk4: u32,
        entity_id: ObjectId,
        unk6: u32,
    },
    TreasureFadeOut {
        unk1: u32,
        unk2: u32,
    },
    FirstAttack {
        unk1: u32,
        unk2: u32,
        combat_tagger: ObjectId,
        unk3: u32,
    },
    UnkFate {
        /// Index into the FATE Excel sheet.
        fate_id: u32,
        unk1: u32,
        unk2: u32,
        unk3: u32,
        unk4: u32,
        unk5: u32,
    },
    CrossRealmListings(CrossRealmListings),
    CrossRealmListingsOverview {
        unk: [u8; 48],
    },
    CrossRealmListingInformation {
        /// The unique ID for this listing.
        listing_id: u64,
        unk: [u8; 456],
    },
    CWLinkshellNameAvailability {
        unk1: u8, // TODO: What is this? Seems to be always 1.
        /// If the desired name was available or not.
        result: CWLSNameAvailability,
        /// The desired name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 6)]
        name: String,
    },
    NewCrossworldLinkshell {
        /// The CWLS's id number and ChatChannel information.
        ids: CWLSCommonIdentifiers,
        unk_timestamp1: u32, // Unknown 32-bit Unix timestamp, likely the cwls's creation time.
        unk_timestamp2: u32, // Seems to be the same timestamp repeated? Might be the member's join time?
        /// The member's rank in the cross-world linkshell, and the linkshell's name.
        common: CWLSCommon,
    },
    RetainerInfo {
        sequence: u32,
        unk2: u32,
        /// Unique ID for this retainer.
        retainer_id: u64,
        index: u8,
        #[brw(pad_after = 2)] // appears empty
        /// How many of their inventory slots are filled.
        item_count: u8,
        /// The amount of gil in their possession.
        gil: u32,
        unk55: u8,
        unk56: u8,
        classjob_id: u8,
        level: u8,
        unk7: u32,
        unk8: u32,
        unk9: u32,
        /// If set to zero, it shows "contract suspended".
        unk10: u32,
        unk11: u8,
        /// The name of this retainer.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 3)]
        name: String,
    },
    RetainerInfoEnd {
        sequence: u32,
        unk1: u32,
        unk2: u32,
        unk3: u32,
        unk4: u32,
        unk5: u32,
    },
    CrossworldLinkshellDisbanded {
        // The linkshell's id.
        linkshell_id: u64,
        /// The linkshell's name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        name: String,
    },
    CrossworldLinkshellMemberLeft {
        /// The linkshell this player is leaving.
        linkshell_id: u64,
        /// The initiator's content id.
        execute_content_id: u64,
        /// The target's content id.
        target_content_id: u64,
        /// Their home world id.
        target_homeworld_id: u16,
        unk1: u8, // Always 1? Changing it does seemingly nothing.
        /// The target's reason for leaving.
        reason_for_leaving: CWLSLeaveReason,
        /// Their name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 4)] // Seems to be empty/zeroes
        character_name: String,
    },
    CrossworldLinkshellRenamed {
        /// The linkshell this player is renaming.
        linkshell_id: u64,
        /// The content id of the character renaming this LS.
        content_id: u64,
        /// Their home world id.
        home_world_id: u16,
        unk1: u8, // Always 1?
        unk2: u8, // TODO: This might just be padding, or part of a u16?
        /// Their name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        character_name: String,
        /// The linkshell's new name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 4)] // Seems to be empty/zeroes
        new_linkshell_name: String,
    },
    CrossworldLinkshellMemberRank {
        /// The linkshell this action is taking place on.
        linkshell_id: u64,
        /// The initiator's content id.
        execute_content_id: u64,
        /// The target's content id.
        target_content_id: u64,
        /// The target's home world id.
        home_world_id: u16,
        unk1: u8, // Always 1?
        /// The rank assigned to the target.
        #[brw(pad_after = 1)] // Seems to be empty/zeroes
        permission_rank: CWLSPermissionRank,
        /// The target's name.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 3)] // Seems to be empty/zeroes
        target_name: String,
    },
    CrossworldLinkshellInvite(CrossworldLinkshellInvite),
    /// This one is sent to the joining player.
    CrossworldLinkshellJoinedSelf {
        common_ids: CWLSCommonIdentifiers,
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        linkshell_name: String,
    },
    /// This one is sent to everyone else, not to the joining player.
    CrossworldLinkshellJoined2 {
        /// The linkshell being joined.
        linkshell_id: u64,
        /// The joining player's content id.
        content_id: u64,
        /// The joining player's home world id.
        home_world_id: u16,
        unk1: u8, // Yet another always 1?
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 5)]
        target_name: String,
    },
    MailboxStatus {
        /// The amount of letters still pending when the player's mailbox is full. Also affects the Delivery Moogle NPC's dialogue (caps at i32::MAX, making this an i32).
        letters_sent_back: i32,
        /// The amount of items sent by friends that have yet to be taken from letters.
        attachments_counter: u16,
        /// The total amount of new mail, displayed as a small white envelope in the server info bar (caps at 99 in the bar). Also mentioned by the Delivery Moogle when they inform the player about how many letters they have.
        unread_counter: u8,
        /// The amount of mail from friends the player has in their mailbox.
        friend_counter: u8,
        /// The amount of reward mail the player has in their mailbox. Reward mail is mail sent by the system that has cash shop items, etc., attached.
        reward_counter: u8,
        /// The amount of system mail from GMs the player in their mailbox.
        system_counter: u8,
        /// If set, the player's info bar will display that they have new mail from a GM/the system.
        #[br(map = read_bool_from::<u8>)]
        #[bw(map = write_bool_as::<u8>)]
        has_gm_mail: bool,
        /// If set, the player's info bar will display that they have new mail from the support desk. This has higher priority than has_gm_mail.
        #[br(map = read_bool_from::<u8>)]
        #[bw(map = write_bool_as::<u8>)]
        #[brw(pad_after = 4)] // Seemingly empty/zeroes, setting it does nothing noticeable
        has_support_message: bool,
    },
    MailboxPreview {
        /// The letters sent on this iteration. This is part of a series of exchanges like all the other lists in FF14.
        #[brw(pad_size_to = LetterPreview::SIZE * LetterPreview::COUNT)]
        #[br(count = LetterPreview::COUNT)]
        letters: Vec<LetterPreview>,
        /// The next batch of 5 letters' index, if applicable. If there are no more letters, it's set to zero.
        next_index: u8,
        /// The current batch of 5 letters' index.
        current_index: u8,
        #[brw(pad_after = 5)] // Probably just padding, it was observed as all 0s.
        unk: u8,
    },
    Letter(Letter),
    LetterUpdate {
        /// Seems to be a result or mode value. When a letter is sent successfully, this will be 0xDD. When a letter is deleted successfully, it will contain 0x366. When attachments are taken, it will contain 0x24E. Completely unknown purpose, as it doesn't seem to be a timestamp, actor id, or LogMessageType.
        unk_result: u32,
        unk1: u32, // Probably just padding, seems to always be 0.
        /// When deleting a letter, the sender's content id. When sending a letter, zeroes.
        sender_content_id: u64,
        /// When deleting a letter, the letter's timestamp. When sending a letter, zeroes.
        timestamp: u32,
        updated_items: [AttachedItemInfo; MAX_MAIL_ATTACHMENTS_STORAGE],
        unk2: [u8; 4], // Unknown, seems to be nothing but padding.
    },
    ShowLinkshellError {
        /// The LogMessage sheet row index to display to the client.
        #[brw(pad_after = 2)] // Seems to be empty/zeroes
        log_message: u16,
        /// Unknown. Has data and doesn't appear to be a timestamp, actor id, or content id.
        #[brw(pad_after = 16)] // Seems to be empty/zeroes
        unk: u32,
    },
    HousingInteriorDetails(HousingInteriorDetails),
    ApartmentList(ApartmentList),
    FriendRemoved {
        #[brw(pad_after = 4)] // Seems to be empty/zeroes
        content_id: u64,
        unk1: u8, // Always 1?
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        #[brw(pad_after = 3)] // Seems to be empty/zeroes
        name: String,
    },
    NpcYell {
        /// Unknown purpose.
        object_id: ObjectTypeId,
        /// Index into ENpcResident Excel sheet.
        name_id: u32,
        /// Index into the NpcYell Excel sheet.
        npc_yell_id: u32,
        /// First generic parameter.
        param1: u32,
        /// Second generic parameter.
        param2: u32,
        /// Third generic parameter.
        param3: u32,
        /// Fourth generic parameter.
        param4: u32,
    },
    InteriorFurniturePlaced {
        /// Which storage the furniture was placed into.
        storage_id: ContainerType,
        /// Which slot the furniture was placed into.
        slot: u16,
        /// The low 12 bits of the row number on the HousingFurniture sheet for this furniture. The row to that sheet can be obtained from the AdditionalData column on the Item Excel sheet. When the client receives this value, it then ORs it with 0x30000 to recreate the row number.
        catalog_id: u16,
        unk1: u16, // Always 1? Changing it seems to have no visible effect so far.
        /// The furniture's dye/stain.
        stain: u8,
        unk2: [u8; 3],
        /// The furniture's rotation. This is only used when placing furniture from the storeroom to ensure the furniture's front faces the player.
        rotation: f32,
        /// The furniture's position.
        position: Position,
        unk3: [u8; 4],
    },
    ExteriorFurniturePlaced {
        /// Likely the plot upon which this furniture was placed.
        plot_index: u8,
        /// The item slot the furniture was placed into.
        slot: u8,
        unk1: [u8; 2], // Likely just padding
        /// The low 12 bits of the row number on the HousingYardObject sheet for this furniture. The row to that sheet can be obtained from the AdditionalData column on the Item Excel sheet. When the client receives this value, it then ORs it with 0x20000 to recreate the row number.
        catalog_id: u16,
        unk2: u16, // Observed as zeroes
        /// The furniture's dye/stain.
        stain: u8,
        unk3: [u8; 3], // Likely just padding
        /// The furniture's rotation. This is only used when placing furniture from the storeroom to ensure the furniture's front faces the player.
        rotation: f32,
        /// The furniture's position.
        position: Position,
        unk4: u32, // Observed as zeroes
    },
    Mogpendium(Mogpendium),
    PlayerName {
        /// Content ID of the player in question.
        content_id: u64,
        /// Name of the player requested.
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        name: String,
    },
    EorzeanTimeOffset {
        offset: i64, // TODO: Not 100% sure this is an i64, but setting it to negative values does make the time to go back to an extent.
    },
    FurnitureTranslatedForObserver(FurnitureTranslatedForObserver),
    SharedFATEInformation {
        /// The page index, starting from zero.
        page: u8,
        /// Unsure what this means yet.
        unk1: [u8; 15],
    },
    UnkDirector2 {
        unk: [u8; 24],
    },
    StatusEffectListDouble {
        unk: [u8; 720],
    },
    StatusEffectListPlayerDouble {
        unk: [u8; 720],
    },
    UnkJobGauge {
        unk: [u8; 8],
    },
    UpdateRecastTimes {
        unk: [u8; 640],
    },
    AdventurerPlate {
        unk1: u32,
        unk2: u32,
        unk3: u32,
        unk4: u32,
        content_id: u64,
        actor_id: ObjectId,
        unk5: u32,
        world_id: u16,
        favored_class_level: u16,
        favored_class: u8, // TODO: not actually?!
        unk7: u8,
        grand_company: GrandCompany,
        grand_company_rank: u8,
        /// The editable design block (`version`..`timestamp`). This is a frozen snapshot the
        /// client submits when saving (see `SubmitAdventurerPlate`) and the server echoes back.
        design: PlateDesign,
        #[brw(pad_after = 132)] // empty? maybe?
        #[brw(pad_size_to = 60)]
        #[br(count = 60)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        comment: String,
        #[brw(pad_before = 1)] // empty
        #[brw(pad_after = 23)] // empty
        #[brw(pad_size_to = CHAR_NAME_MAX_LENGTH)]
        #[br(count = CHAR_NAME_MAX_LENGTH)]
        #[br(map = read_string)]
        #[bw(map = write_string)]
        name: String,
    },
    /// Sent instead of `AdventurerPlate` when a plate request cannot be fulfilled: the requested
    /// player has never set up a plate, their plate is not visible to the viewer, or the data is
    /// otherwise unavailable. The client shows `log_message_id` from the LogMessage sheet
    /// (5856 = not set, 5858 = not public, 5860 = other reason) and closes the plate window.
    RequestAdventurerPlateError {
        log_message_id: u32,
        /// The target's content/actor id on retail; not read by the client's message handler.
        unk1: u32,
        /// Empty on the wire (16 bytes).
        padding: [u8; 16],
    },
    RecordGatheringLog {
        /// Index into the bitmask.
        index: u8,
        /// New value at this index.
        #[brw(pad_after = 6)] // unused
        value: u8,
    },
    RecruitingPartyCount {
        /// Unused by client; always 0 in observed packets. May be server-side session ID.
        page_id: u32,
        /// Page sequence number (1-based). 0 indicates the final page.
        more_pages: u16,
        /// Number of recruiting entries in this page. Client scans entries [0..count).
        /// An entry with category_bits=0 is category 0 ("none"), not an empty slot.
        count: u16,
        /// Category information for each entry. Parallel with `details`.
        entries: [RecruitingPartyEntry; 60],
        /// Content identification for each entry. Parallel with `entries`.
        details: [RecruitingPartyDetail; 60],
    },
}

/// Builds the pair of `(ContentFinderUpdate, ContentFinderFound)` packets that pop a duty on the
/// ready popup. The wire layout of these two packets lives here, in one place, so that the solo
/// `register_for_content` self-pop and the server-side party pop propagation stay byte-identical.
///
/// `settings_word` is the `DutyFinderSetting` "mode word" (see [`ServerZoneIpcData::ContentFinderUpdate`]),
/// `classjob_id` is the class the queuer registered with (cosmetic), `content_ids` are the
/// queued ContentFinderCondition ids (the client reads them as 5 x u32), and `total_members` is the
/// number of queued members (party size; the client reads it as the Found count-region total at
/// payload byte [39]).
pub fn build_cf_pop(
    settings_word: u64,
    classjob_id: u8,
    content_ids: [u16; 5],
    total_members: u8,
) -> (ServerZoneIpcData, ServerZoneIpcData) {
    let update = ServerZoneIpcData::ContentFinderUpdate {
        queue_state: 1,
        classjob_id,
        languages: 0,
        unk_3_7: [0; 5],
        settings: settings_word,
        roulette_id: 0,
        unk_17: 0,
        unk_18: 1,
        ready_flag: 1,
        content_ids: content_ids.map(|id| id as u32),
    };

    let found = ServerZoneIpcData::ContentFinderFound {
        queue_state: 3,
        unk_1_7: [0; 7],
        settings: settings_word,
        in_progress_start_timestamp: 0,
        unk_24: 1,
        unk_25_27: [0; 3],
        content_id: content_ids[0] as u32,
        unk_32_35: 0,
        unk_36_37: 0,
        unk_38_39: (total_members as u16) << 8,
    };

    (update, found)
}

#[cfg(test)]
mod tests {
    use crate::common::test_opcodes;

    use super::*;

    // Ensure that the IPC data size as reported matches up with what we write
    #[test]
    fn server_zone_ipc_sizes() {
        test_opcodes::<ServerZoneIpcSegment>();
    }

    // The shared BannerData block must serialize to exactly 52 bytes.
    #[test]
    fn portrait_banner_size() {
        crate::common::ensure_size::<PortraitBanner, 52>();
    }

    // Pins `RecruitingPartyCount` against a real retail capture: three recruits, at slots 5, 6 and
    // 10, being ContentRoulette 5 (Expert), ContentRoulette 2 (Leveling) and ContentFinderCondition
    // 4 (Sastasha). The two 60-element arrays are parallel, so a wrong element size or a swapped
    // field would shift the non-zero slots and this round-trip would fail.
    #[test]
    fn recruiting_party_count_matches_retail_capture() {
        use binrw::{BinRead, BinWrite};
        use std::io::Cursor;

        let mut wire = vec![0u8; 968];
        // Header: page_id = 0, more_pages = 0 (final page), count = 16.
        wire[6] = 0x10;
        // entries[i].category_bits at 8 + i * 8 + 4.
        wire[8 + 5 * 8 + 4] = 0x02;
        wire[8 + 6 * 8 + 4] = 0x02;
        wire[8 + 10 * 8 + 4] = 0x04;
        // details[i] at 488 + i * 8: content_id (u16), content_kind (u16).
        wire[488 + 5 * 8] = 5;
        wire[488 + 5 * 8 + 2] = 1;
        wire[488 + 6 * 8] = 2;
        wire[488 + 6 * 8 + 2] = 1;
        wire[488 + 10 * 8] = 4;
        wire[488 + 10 * 8 + 2] = 2;

        let mut cursor = Cursor::new(&wire);
        let parsed = ServerZoneIpcData::read_le_args(
            &mut cursor,
            (&ServerZoneIpcType::RecruitingPartyCount, &968),
        )
        .unwrap();

        let ServerZoneIpcData::RecruitingPartyCount {
            more_pages,
            count,
            entries,
            details,
            ..
        } = &parsed
        else {
            panic!("parsed as the wrong variant: {parsed:#?}");
        };

        assert_eq!(*more_pages, 0);
        assert_eq!(*count, 16);
        // The category code is the index of the lowest set bit: roulettes 1, dungeon 2.
        assert_eq!(entries[5].category_bits.trailing_zeros(), 1);
        assert_eq!(entries[6].category_bits.trailing_zeros(), 1);
        assert_eq!(entries[10].category_bits.trailing_zeros(), 2);
        assert_eq!((details[5].content_id, details[5].content_kind), (5, 1));
        assert_eq!((details[6].content_id, details[6].content_kind), (2, 1));
        assert_eq!((details[10].content_id, details[10].content_kind), (4, 2));
        // Every other slot is empty, so the client's scan skips it.
        assert!(
            (0..60)
                .filter(|i| ![5, 6, 10].contains(i))
                .all(|i| entries[i].category_bits == 0 && details[i].content_id == 0)
        );

        let mut out = Cursor::new(Vec::new());
        parsed.write_le(&mut out).unwrap();
        assert_eq!(out.into_inner(), wire);
    }

    // The shared party portrait entry must serialize to exactly 176 bytes, wire-identical
    // across the single-slot and batch portrait packets.
    #[test]
    fn party_portrait_entry_size() {
        crate::common::ensure_size::<PartyPortraitEntry, 176>();
    }

    // Byte-exact layout guard for the reshaped `ContentFinderUpdate`. Since binrw writes fields
    // sequentially with no padding, field order == byte order; any offset error shows up here.
    // The client reads `content_ids` as 5 x u32 (DWORDs) at payload offset 20-39, so slot 1 must
    // land at offset 24 (not 22, which the old `[u16; 5]` layout gave).
    #[test]
    fn content_finder_update_byte_layout() {
        use binrw::BinWrite;
        use std::io::Cursor;

        let data = ServerZoneIpcData::ContentFinderUpdate {
            queue_state: 1,
            classjob_id: 0x1F,
            languages: 0,
            unk_3_7: [0; 5],
            settings: 0x202020,
            roulette_id: 0,
            unk_17: 0,
            unk_18: 1,
            ready_flag: 1,
            content_ids: [0x0056, 0x1234, 0, 0, 0],
        };

        let mut cursor = Cursor::new(Vec::new());
        data.write_le(&mut cursor).unwrap();
        let body = cursor.into_inner();

        assert_eq!(body.len(), 40);
        #[rustfmt::skip]
        let expected: [u8; 40] = [
            1, 0x1F, 0, // queue_state, classjob_id, languages (0-2)
            0, 0, 0, 0, 0, // unk_3_7 (3-7)
            0x20, 0x20, 0x20, 0, 0, 0, 0, 0, // settings mode word 0x202020 LE (8-15)
            0, 0, 1, 1, // roulette_id, unk_17, unk_18, ready_flag (16-19)
            0x56, 0, 0, 0, // content_ids[0] (20-23)
            0x34, 0x12, 0, 0, // content_ids[1] (24-27)
            0, 0, 0, 0, // content_ids[2] (28-31)
            0, 0, 0, 0, // content_ids[3] (32-35)
            0, 0, 0, 0, // content_ids[4] (36-39)
        ];
        assert_eq!(body, expected);
    }

    // Byte-exact layout guard for the reshaped `ContentFinderFound`. The ready/prepare popup
    // renders its mode icon from the `DutyFinderSetting` mode word (little-endian u64 at payload
    // offset 8-15); it must echo the actual settings and never leave the Explorer bit set (payload
    // offset 12). The trailing `unk_38_39` = 0x0100 preserves the old wire byte at offset 39.
    #[test]
    fn content_finder_found_byte_layout() {
        use crate::ipc::zone::DutyFinderSetting;
        use binrw::BinWrite;
        use std::io::Cursor;

        let settings = (DutyFinderSetting::UNRESTRICTED_PARTY | DutyFinderSetting::LEVEL_SYNC)
            .to_ready_mode_word();

        let data = ServerZoneIpcData::ContentFinderFound {
            queue_state: 3,
            unk_1_7: [0; 7],
            settings,
            in_progress_start_timestamp: 0,
            unk_24: 1,
            unk_25_27: [0; 3],
            content_id: 0x56,
            unk_32_35: 0,
            unk_36_37: 0,
            unk_38_39: 0x0100,
        };

        let mut cursor = Cursor::new(Vec::new());
        data.write_le(&mut cursor).unwrap();
        let body = cursor.into_inner();

        assert_eq!(body.len(), 40);
        let mode_word = u64::from_le_bytes(body[8..16].try_into().unwrap());
        assert_eq!(mode_word, 0x202020); // (UNRESTRICTED | LEVEL_SYNC) | 0x20
        assert_eq!(body[12], 0); // Explorer bit (bit 32) clear
        assert_eq!(body[24], 1); // unk_24
        assert_eq!(&body[38..40], &[0x00, 0x01]); // unk_38_39 = 0x0100 LE
    }

    // Byte-exact layout guard for the reshaped `ContentFinderCommencing`. Broadcast to each online
    // party member on every acceptance event; the reshape from a 24-byte blob into named fields must
    // preserve the exact wire values (per-recipient state + X/Y counts). The vector below is the
    // 2-client retail capture (Acc1_进本 line 19): recipient accepted (state 4), 1 of 2 ready, for
    // ContentFinderCondition 0x030F.
    #[test]
    fn content_finder_commencing_byte_layout() {
        use binrw::BinWrite;
        use std::io::Cursor;

        let data = ServerZoneIpcData::ContentFinderCommencing {
            state: 4,
            unk_4: 1,
            content_id: 0x030F,
            queued_tanks: 0,
            needed_tanks: 0,
            queued_healers: 0,
            needed_healers: 0,
            queued_dps: 0,
            needed_dps: 0,
            accepted_count: 1,
            total_count: 2,
            unk_20_23: [0; 4],
        };

        let mut cursor = Cursor::new(Vec::new());
        data.write_le(&mut cursor).unwrap();
        let body = cursor.into_inner();

        assert_eq!(body.len(), 24);
        #[rustfmt::skip]
        let expected: [u8; 24] = [
            4, 0, 0, 0, // state = 4 (0-3)
            1, 0, 0, 0, // unk_4 = 1 (4-7)
            0x0F, 0x03, 0, 0, // content_id = 0x030F (8-11)
            0, 0, 0, 0, // unk_12 (12-15)
            0, 0, // unk_16 (16-17)
            1, // accepted_count (18)
            2, // total_count (19)
            0, 0, 0, 0, // unk_20_23 (20-23)
        ];
        assert_eq!(body, expected);
    }

    // Byte-exact layout guard for the reshaped `UnkContentFinder`, sent to each participant on a
    // Withdraw/Timeout cancel. The vector below is the retail capture (Acc1_出发但辞退 line 25): no
    // attribution (content_id 0) and the generic "registration cancelled" LogMessage 0x0373.
    #[test]
    fn unk_content_finder_byte_layout() {
        use binrw::BinWrite;
        use std::io::Cursor;

        let data = ServerZoneIpcData::UnkContentFinder {
            content_id: 0,
            log_message_id: 0x0373,
            unk_12: 0,
        };

        let mut cursor = Cursor::new(Vec::new());
        data.write_le(&mut cursor).unwrap();
        let body = cursor.into_inner();

        assert_eq!(body.len(), 16);
        #[rustfmt::skip]
        let expected: [u8; 16] = [
            0, 0, 0, 0, 0, 0, 0, 0, // content_id = 0 (0-7)
            0x73, 0x03, 0, 0, // log_message_id = 0x0373 (8-11)
            0, 0, 0, 0, // unk_12 (12-15)
        ];
        assert_eq!(body, expected);
    }

    // `build_cf_pop` must produce the exact same field values the solo `register_for_content` path
    // has always sent, so the two pop paths stay wire-identical.
    #[test]
    fn build_cf_pop_matches_register_for_content() {
        use crate::ipc::zone::DutyFinderSetting;

        let settings = DutyFinderSetting::UNRESTRICTED_PARTY.to_ready_mode_word();
        let (update, found) = build_cf_pop(settings, 0x1F, [0x030F, 0, 0, 0, 0], 1);

        match update {
            ServerZoneIpcData::ContentFinderUpdate {
                queue_state,
                classjob_id,
                languages,
                unk_3_7,
                settings: s,
                roulette_id,
                unk_17,
                unk_18,
                ready_flag,
                content_ids,
            } => {
                assert_eq!(queue_state, 1);
                assert_eq!(classjob_id, 0x1F);
                assert_eq!(languages, 0);
                assert_eq!(unk_3_7, [0; 5]);
                assert_eq!(s, settings);
                assert_eq!(roulette_id, 0);
                assert_eq!(unk_17, 0);
                assert_eq!(unk_18, 1);
                assert_eq!(ready_flag, 1);
                assert_eq!(content_ids, [0x030F, 0, 0, 0, 0]);
            }
            _ => panic!("build_cf_pop did not return a ContentFinderUpdate"),
        }

        match found {
            ServerZoneIpcData::ContentFinderFound {
                queue_state,
                unk_1_7,
                settings: s,
                in_progress_start_timestamp,
                unk_24,
                unk_25_27,
                content_id,
                unk_32_35,
                unk_36_37,
                unk_38_39,
            } => {
                assert_eq!(queue_state, 3);
                assert_eq!(unk_1_7, [0; 7]);
                assert_eq!(s, settings);
                assert_eq!(in_progress_start_timestamp, 0);
                assert_eq!(unk_24, 1);
                assert_eq!(unk_25_27, [0; 3]);
                assert_eq!(content_id, 0x030F);
                assert_eq!(unk_32_35, 0);
                assert_eq!(unk_36_37, 0);
                assert_eq!(unk_38_39, 0x0100);
            }
            _ => panic!("build_cf_pop did not return a ContentFinderFound"),
        }
    }

    // Retail encodes the queued-member total at the Found packet's payload byte [39] (the u16
    // `unk_38_39`, LE, = total << 8): 0x0200 for a 2-person party, 0x0100 for a solo pop.
    #[test]
    fn build_cf_pop_encodes_total_members() {
        use crate::ipc::zone::DutyFinderSetting;

        let settings = DutyFinderSetting::UNRESTRICTED_PARTY.to_ready_mode_word();

        for (total_members, expected) in [(2u8, 0x0200u16), (1u8, 0x0100u16)] {
            let (_update, found) =
                build_cf_pop(settings, 0x1F, [0x030F, 0, 0, 0, 0], total_members);
            match found {
                ServerZoneIpcData::ContentFinderFound { unk_38_39, .. } => {
                    assert_eq!(unk_38_39, expected);
                }
                _ => panic!("build_cf_pop did not return a ContentFinderFound"),
            }
        }
    }

    // The mode word forces the server-authored 0x20 "organized party / no-withdraw-penalty" bit
    // while preserving the player's selected icon flags and never adding the Explorer bit.
    #[test]
    fn ready_mode_word_sets_organized_bit_and_preserves_icons() {
        use crate::ipc::zone::DutyFinderSetting;

        let word = (DutyFinderSetting::UNRESTRICTED_PARTY | DutyFinderSetting::LEVEL_SYNC)
            .to_ready_mode_word();
        assert_eq!(word & 0x20, 0x20); // organized-party / withdraw-penalty gate bit
        assert_eq!(word & 0x100000000, 0); // Explorer bit never added
        assert_eq!(word & 0x2000, 0x2000); // UNRESTRICTED_PARTY preserved
        assert_eq!(word & 0x200000, 0x200000); // LEVEL_SYNC preserved

        assert_eq!(DutyFinderSetting::empty().to_ready_mode_word(), 0x20);
    }
}
