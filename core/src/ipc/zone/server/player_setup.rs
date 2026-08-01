use binrw::binrw;

use crate::{
    common::{
        FestivalId, ObjectId, PlayerStateFlags1, PlayerStateFlags2, PlayerStateFlags3,
        read_bool_from, read_string, write_bool_as, write_string,
    },
    constants::{
        ACTIVE_HELP_BITMASK_SIZE, ADVENTURE_BITMASK_SIZE, AETHER_CURRENT_BITMASK_SIZE,
        AETHER_CURRENT_COMP_FLG_SET_BITMASK_SIZE, AETHERYTE_UNLOCK_BITMASK_SIZE,
        BEAST_TRIBE_ARRAY_SIZE, BEGINNER_TRAINING_ARRAY_SIZE, BUDDY_EQUIP_BITMASK_SIZE,
        CAUGHT_FISH_BITMASK_SIZE, CAUGHT_SPEARFISH_BITMASK_SIZE, CHOCOBO_TAXI_STANDS_BITMASK_SIZE,
        CLASSJOB_ARRAY_SIZE, CONTENT_ROULETTE_ARRAY_SIZE, CONTENTS_NOTE_BITMASK_SIZE,
        CRYSTALLINE_CONFLICT_ARRAY_SIZE,
        CUTSCENE_SEEN_BITMASK_SIZE, DUNGEON_ARRAY_SIZE, FISHING_RECORD_TYPE_ARRAY_SIZE,
        FRAMERS_KIT_BITMASK_SIZE, FRONTLINE_ARRAY_SIZE, GLASSES_STYLES_BITMASK_SIZE,
        GUILDHEST_ARRAY_SIZE, MAPS_WITH_UP_TO_16_REGIONS_ARRAY_SIZE,
        MAPS_WITH_UP_TO_32_REGIONS_ARRAY_SIZE, MASKED_CARNIVALE_ARRAY_SIZE, MINION_BITMASK_SIZE,
        MISC_CONTENT_ARRAY_SIZE, MOUNT_BITMASK_SIZE, ORCHESTRION_ROLL_BITMASK_SIZE,
        ORNAMENT_BITMASK_SIZE, RAID_ARRAY_SIZE, SATISFACTION_NPC_ARRAY_SIZE,
        SECRET_RECIPE_BOOK_BITMASK_SIZE, SPECIAL_CONTENT_ARRAY_SIZE, TRIAL_ARRAY_SIZE,
        TRIPLE_TRIAD_CARDS_BITMASK_SIZE, TRIPLE_TRIAD_NPC_BITMASK_SIZE, UNLOCK_BITMASK_SIZE,
        UNLOCKED_FISHING_SPOTS_BITMASK_SIZE,
        VVD_NOTEBOOK_CONTENTS_BITMASK_SIZE,
    },
};

#[binrw]
#[derive(Debug, Clone, Default)]
pub struct PlayerSetup {
    /// The content ID of the player.
    pub content_id: u64,
    /// Not exactly unused but unsure of the purpose.
    pub padding: [u64; 2],
    /// The actor ID of the player.
    pub actor_id: ObjectId,
    pub rested_exp: u32,
    pub companion_current_exp: u32,
    /// 0x24 (u32 = 79) -> PlayerState+0x2FC. Quest-journal PRNG seed: sub_140C1B4A0
    /// (journal list builder) reads it as dword_142AAEC34 and runs 11 pseudo-random
    /// iterations (%1000 / %100) to select the randomized journal/supply entries.
    /// NOT "GCSupply stuff" - that FFXIVClientStructs comment is stale (0x2F0 is DoH/DoL
    /// levels, 0x2FC is this seed).
    pub quest_journal_prng_seed: u32,
    pub fish_caught: u32,
    pub use_bait_catalog_id: u32,
    pub num_spearfish_caught: u32,
    pub unknown_pvp2c: u32,
    pub total_frontline_matches: u32,
    pub squadron_mission_completion_timestamp: i32,
    pub squadron_training_completion_timestamp: i32,
    pub unknown_timestamp38: u32,
    pub weekly_bingo_task_status: [u8; 4],
    pub weekly_bingo_flags: u32,
    pub companion_time_left: f32,
    /// 0x54..0x57
    pub unknown44: [u8; 4],
    /// 0x58..0x5B -> PlayerState+0x8D8 (FFXIVClientStructs: UnkTofuTimestamp, private)
    pub tofu_timestamp: u32,
    /// 0x5C..0x5D
    pub unknown_after_tofu: [u8; 2],
    /// 0x5E..0x65 - 4 x u16 -> PvPProfile+0x16..0x1C (unnamed area 0x14-0x23 between GC
    /// ranks and Series). All 0 in retail; no real reader found (searched PvP* code -
    /// 0x16/0x18/0x1A/0x1C hits are other structs' offsets). Likely reserved/dead.
    pub pvp_unknowns: [u16; 4],
    /// 0x66..0x67 -> PvPProfile+0x28 = SeriesExperience. Retail: 0.
    pub pvp_series_exp: u16,
    /// How many player commendations you received.
    pub player_commendations: i16,
    /// 0x6A..0x6D -> PlayerState+0x188/0x18C = FestivalQuestWork / festival flag bitmap
    /// (sub_140BDD9F0: sets a bit in PlayerState+0x18C per ClassJob).
    pub unknown64: [u16; 2],
    pub frontline_weekly_matches: u16,
    /// 0x70 (u16 = 0xC000) = AnimaWeapon object +12 field: low 14 bits = enhance points
    /// (& 0x3FFF, 0 here), bit 0x4000 = have 改良型元灵透镜 (Improved Anima Lens,
    /// EventItem 2002029, quest 67932 - "人造元灵终绽放" stage, user-confirmed).
    /// Written by sub_1408931D0, read by GetAnimaWeapon7EnhancePoint.
    pub unknown2: u16,
    pub active_gc_army_expedition: u16,
    pub active_gc_army_training: u16,
    pub unknown2a: u16,
    pub weekly_bingo_stickers: u16,
    pub pvp_rival_wings_total_matches: u16,
    pub pvp_rival_wings_total_victories: u16,
    pub pvp_rival_wings_weekly_matches: u16,
    pub pvp_rival_wings_weekly_victories: u16,
    /// The maximum attainable level on the account. Unsure of it's in-game effect.
    pub max_level: u8,
    /// Which expansion you have acquired. Unsure of it's in-game effect.
    pub expansion: u8,
    #[br(map = read_bool_from::<u8>)]
    #[bw(map = write_bool_as::<u8>)]
    pub has_premium_saddlebag: bool,
    #[br(map = read_bool_from::<u8>)]
    #[bw(map = write_bool_as::<u8>)]
    pub unknown77: bool,
    #[br(map = read_bool_from::<u8>)]
    #[bw(map = write_bool_as::<u8>)]
    pub unknown78: bool,
    pub race: u8,
    pub tribe: u8,
    pub gender: u8,
    /// Refers to an index in the ClassJob Excel sheet.
    pub current_class: u8,
    /// I guess the first class of your character, but I'm unsure?
    pub first_class: u8,
    /// The character's chosen deity. Indexed into the GuardianDeity Excel sheet.
    pub deity: u8,
    pub nameday_month: u8,
    pub nameday_day: u8,
    /// The character's initial city-state.
    pub city_state: u8,
    /// The Aetheryte used for the Return action. Indexed into the Aetheryte Excel sheet.
    pub homepoint: u16,
    pub quest_special_flags: u8,
    pub pet_data: u8,
    pub companion_rank: u8,
    pub companion_stars: u8,
    pub companion_skill_points: u8,
    pub companion_active_command: u8,
    pub companion_color: u8,
    pub companion_favorite_feed: u8,
    pub favourite_aetheryte_count: u8,
    /// 0x9B -> QuestManager+0x6D8 (not a PlayerState field)
    pub daily_quest_seed: u8,
    /// 0x9C -> global qword_142AB2CB4
    pub unknown97: u8,
    pub weekly_lockout_info: u8,
    pub relic_id: u8,
    pub relic_note_id: u8,
    pub sightseeing_log_unlock_state: u8,
    pub sightseeing_log_unlock_state_ex: u8,
    pub unknown9e: u8,
    pub unknown9e1: u8,
    pub meister_flag: u8,
    /// Controls whether or not you can challenge other players.
    #[br(map = read_bool_from::<u8>)]
    #[bw(map = write_bool_as::<u8>)]
    pub can_do_triple_triad_matches: bool,
    // This is the first byte of the full bitmask. It contains the HW zones, The Fringes and The Ruby Sea. Why this one is here and the rest far down, no idea.
    pub aether_current_comp_flg_set_bitmask1: u8,
    pub unknown_after_aether: u8,
    #[br(map = read_bool_from::<u8>)]
    #[bw(map = write_bool_as::<u8>)]
    pub has_new_gc_army_candidate: bool,
    /// 0xA9 -> PlayerState+0x739 = CompletedLoVMStages. LoVM = Lord of Verminion
    /// (萌宠之王, Gold Saucer minion-battle content) - completed stage count.
    /// Retail: 2 (player completed 2 LoVM stages).
    pub completed_lovm_stages: u8,
    pub unk111: u8,
    /// 0xAB = SatisfactionSupplyManager.SupplySeed (weekly pseudorandom seed for the
    /// selected crafted items of the satisfaction NPCs, FFXIVClientStructs SatisfactionSupplyManager).
    pub supply_seed: u8,
    pub gold_saucer_content_status: u8,
    /// Last expansion mentorship was held. Starts at 1 with Shadowbringers.
    pub mentor_version: u8,
    pub unk_hwd: u8,
    pub weekly_bingo_exp_multiplier: u8,
    pub weekly_bingo_unk63: u8,
    pub series_current_rank: u8,
    pub series_claimed_rank: u8,
    pub previous_series_claimed_rank: u8,
    pub previous_series_rank: u8,
    /// 0xB5..0xBB -> PlayerState+0x188..0x18E (FestivalQuestWork area, see sub_140BDD810/
    /// sub_140BDD9C0 - 3-byte-per-entry festival quest work array + u16 festival flag).
    pub unknowna3: [u8; 7],
    /// Current EXP for all classjobs. This doesn't control the class' "unlocked state" in the Character UI.
    #[br(count = CLASSJOB_ARRAY_SIZE)]
    #[bw(pad_size_to = CLASSJOB_ARRAY_SIZE * 4)]
    pub exp: Vec<i32>,
    pub experience_maelstrom: u32,
    pub experience_twin_adder: u32,
    pub experience_immortal_flames: u32,
    /// 0x154..0x15F (12 bytes = 3 u32) -> PvPProfile+0x38..0x40 = FrontlineTotalFirstPlace /
    /// SecondPlace / ThirdPlace. Retail: [3, 4, 1] (1st 3x, 2nd 4x, 3rd 1x).
    /// (Was misnamed unknown138.)
    #[br(count = 12)]
    #[bw(pad_size_to = 12)]
    pub frontline_total_places: Vec<u8>,
    pub unknown_unix_timestamp: i32,
    /// Current levels for all classjobs. If non-zero, the class is visibly "unlocked" in the Character UI.
    #[br(count = CLASSJOB_ARRAY_SIZE)]
    #[bw(pad_size_to = CLASSJOB_ARRAY_SIZE * 2)]
    pub levels: Vec<u16>,
    pub ui_festival_ids: [FestivalId; 8],
    pub ui_festival_phases: [u16; 8],
    /// 44 x FishingRecordType best-catch fish ids (PlayerState+0x344, packet 0x1CA..0x221).
    /// Index = FishingRecordType row (sheet 249, 0..43). Value < 0x4E20 (20000) = FishParameter
    /// row id (sheet 247, normal fishing); >= 0x4E20 = SpearfishingItem row id (sheet 610,
    /// spearfishing); 0 = no record. Read via word_142AAEC7C by FishRecord.Initialize
    /// (0x140BF1899, <20000) and sub_140BF20E0 (0x140BF2169, >=20000).
    /// Retail capture: 金鳞鱼1056/101.6, 姥鲨898/520.5, 食人鲶鱼355/132.0,
    /// 风筝猫鱼415/239.5, 旋齿鲨290/183.9, 云海帝王294/269.6, 梦幻云海鱼379/76.3,
    /// 火山龟373/62.7, 噬卵者1113/233.3 (ilms) + 6 spearfishing records.
    /// NOTE: values are FishParameter/SpearfishingItem row ids, NOT Item row ids -
    /// earlier identification as "Deprecated gear" was a numeric coincidence.
    #[br(count = FISHING_RECORD_TYPE_ARRAY_SIZE)]
    #[bw(pad_size_to = FISHING_RECORD_TYPE_ARRAY_SIZE * 2)]
    pub fishing_record_best_fish: Vec<u16>,
    /// 44 x matching best-catch max size (ilms x 10, PlayerState+0x39C, packet 0x222..0x279).
    /// Same index as fishing_record_best_fish (e.g. 旋齿鲨 183.9 ilms = 1839).
    #[br(count = FISHING_RECORD_TYPE_ARRAY_SIZE)]
    #[bw(pad_size_to = FISHING_RECORD_TYPE_ARRAY_SIZE * 2)]
    pub fishing_record_best_size: Vec<u16>,
    /// 0x27A..0x2B1. Mixed: 0x28C..0x2AA = QuestManager beast-tribe data; 0x2AC..0x2B1 =
    /// Frontline weekly places (3 u16 -> PvPProfile+0x46/0x48/0x4A FrontlineWeeklyFirst/
    /// Second/ThirdPlace, all 0 in retail). Kept as padding so following offsets stay correct.
    #[br(count = 56)]
    #[bw(pad_size_to = 56)]
    pub unknown_after_194: Vec<u8>,
    /// 0x2B2..0x2C9 (12 u16) = SatisfactionSupplyManager._satisfaction (current-rank satisfaction
    /// points per satisfaction NPC, one u16 per SatisfactionNpc). Not read by PlayerState.ReadPacket.
    /// Retail capture: 11x 0 + 1x 240 (the 12th/7.51 NPC had progress), confirming the [u16;12]
    /// boundary is correct — companion_name (0x2CA) parses right after it, byte-identical round-trip.
    pub supply_satisfcation: [u16; SATISFACTION_NPC_ARRAY_SIZE],
    #[br(count = 21)]
    #[bw(pad_size_to = 21)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub companion_name: String,
    pub companion_def_rank: u8,
    pub companion_att_rank: u8,
    pub companion_heal_rank: u8,
    #[br(count = MOUNT_BITMASK_SIZE)]
    #[bw(pad_size_to = MOUNT_BITMASK_SIZE)]
    pub mounts: Vec<u8>,
    #[br(count = ORNAMENT_BITMASK_SIZE)]
    #[bw(pad_size_to = ORNAMENT_BITMASK_SIZE)]
    pub ornament_mask: Vec<u8>,
    #[br(count = GLASSES_STYLES_BITMASK_SIZE)]
    #[bw(pad_size_to = GLASSES_STYLES_BITMASK_SIZE)]
    pub glasses_styles_mask: Vec<u8>,
    pub padding_probably_after_glasses_styles: u8,
    #[br(count = FRAMERS_KIT_BITMASK_SIZE)]
    #[bw(pad_size_to = FRAMERS_KIT_BITMASK_SIZE)]
    pub framers_kits_mask: Vec<u8>,
    // NOTE: no padding after framers - FRAMERS_KIT_BITMASK_SIZE (44) fills 0x320..0x34B exactly,
    // `name` starts right at 0x34C (verified via retail capture: 0x34A-0x34B = 0x20 0x00 are framers bits).
    // NOTE: It seems this name is bigger than normal, but bytes >=40 may contain the online ID...?
    #[br(count = 64)]
    #[bw(pad_size_to = 64)]
    #[br(map = read_string)]
    #[bw(map = write_string)]
    pub name: String,
    /// Unlock bitmask for everything else, mostly for game features.
    /// This might also be referred to as "rewards".
    #[br(count = UNLOCK_BITMASK_SIZE)]
    #[bw(pad_size_to = UNLOCK_BITMASK_SIZE)]
    pub unlocks: Vec<u8>,
    /// Unlock bitmask for Aetherytes.
    #[br(count = AETHERYTE_UNLOCK_BITMASK_SIZE)]
    #[bw(pad_size_to = AETHERYTE_UNLOCK_BITMASK_SIZE)]
    pub aetherytes: Vec<u8>,
    pub favorite_aetheryte_ids: [u16; 4],
    pub free_aetheryte_id: u16,
    /// Free Aetheryte for Playstation Plus members.
    pub ps_plus_free_aetheryte_id: u16,
    /// Free Aetheryte for Nintendo Switch Online members.
    pub nso_free_aetheryte_id: u16,
    #[br(count = MAPS_WITH_UP_TO_16_REGIONS_ARRAY_SIZE)]
    #[bw(pad_size_to = MAPS_WITH_UP_TO_16_REGIONS_ARRAY_SIZE * 2)]
    pub maps_with_up_to_16_regions: Vec<u16>,
    /// 49 u32 (the client's MapDiscoveryManager reads exactly 49 slots for maps with 17..32 regions).
    /// Previously modelled as 48 + a 4-byte padding field; that "padding" was the 49th slot.
    #[br(count = MAPS_WITH_UP_TO_32_REGIONS_ARRAY_SIZE)]
    #[bw(pad_size_to = MAPS_WITH_UP_TO_32_REGIONS_ARRAY_SIZE * 4)]
    pub maps_with_up_to_32_regions: Vec<u32>,
    /// Which Active Help guides the player has seen.
    #[br(count = ACTIVE_HELP_BITMASK_SIZE)]
    #[bw(pad_size_to = ACTIVE_HELP_BITMASK_SIZE)]
    pub seen_active_help: Vec<u8>,
    /// Unlock bitmask for minions.
    #[br(count = MINION_BITMASK_SIZE)]
    #[bw(pad_size_to = MINION_BITMASK_SIZE)]
    pub minions: Vec<u8>,
    #[br(count = CHOCOBO_TAXI_STANDS_BITMASK_SIZE)]
    #[bw(pad_size_to = CHOCOBO_TAXI_STANDS_BITMASK_SIZE)]
    pub chocobo_taxi_stands_mask: Vec<u8>,
    #[br(count = CUTSCENE_SEEN_BITMASK_SIZE)]
    #[bw(pad_size_to = CUTSCENE_SEEN_BITMASK_SIZE)]
    pub cutscene_seen_mask: Vec<u8>,
    /// 0x750 (1 byte, not u16 - Buddy.ReadPacket reads 0x751 onward).
    pub unknown6ff: u8,
    /// 0x751..0x75E (14 bytes = BUDDY_EQUIP_BITMASK_SIZE).
    #[br(count = BUDDY_EQUIP_BITMASK_SIZE)]
    #[bw(pad_size_to = BUDDY_EQUIP_BITMASK_SIZE)]
    pub buddy_equip_mask: Vec<u8>,
    /// 0x75F..0x761 = equipped BuddyEquip row ids (head/body/legs).
    /// Retail capture: 16 16 16 = 斯莱普尼尔装甲 (Sleipnir Barding, BuddyEquip row 16) equipped on all 3 slots.
    pub companion_equipped_head: u8,
    pub companion_equipped_body: u8,
    pub companion_equipped_legs: u8,
    /// 0x762..0x765 (4 bytes) -> PlayerState+0x2EC..0x2EF (UIState+0xD24). Grand Company
    /// Supply/Provisioning "submitted this week" flag bitmap (55 bits): sub_140C1B4A0 tests
    /// (128>>(v4&7)) & byte_142AAEC24[v4>>3] for v4=0..54. Bit i = entry (i+1), 0-based.
    /// Order matches doh_dol_levels: bit 0-7 = crafters (CRP, BSM, ARM, GSM, LTW, WVR,
    /// ALC, CUL), bit 8 = Miner, bit 9 = Botanist, bit 10 = Fisher. VERIFIED live:
    /// submitting Miner GC-supply set bit 8 (00 00 00 00 -> 00 80 00 00); completing
    /// Custom Delivery (satisfaction) did NOT change it (separate system).
    pub gc_supply_submitted_flags: [u8; 4],
    /// 0x766..0x770 (11 bytes) -> PlayerState+0x2F0..0x2FA (UIState+0xD28) = DoH/DoL
    /// (crafter/gatherer) job levels, one byte per job. Retail capture:
    /// 10x 100 + 90 (Fisher lv90 last) - user-confirmed. Read by quest-journal logic
    /// (sub_140C1AD10, which caps values at 100). NOT "GCSupply stuff" as FFXIVClientStructs
    /// guessed - that comment is stale.
    #[br(count = 11)]
    #[bw(pad_size_to = 11)]
    pub doh_dol_levels: Vec<u8>,
    #[br(count = CAUGHT_FISH_BITMASK_SIZE)]
    #[bw(pad_size_to = CAUGHT_FISH_BITMASK_SIZE)]
    pub caught_fish_mask: Vec<u8>,
    // NOTE: no padding after caught_fish - unlocked_fishing_spots starts right at 0x830
    // (verified: caught_fish 0x771..0x82F, then fishing_spots 0x830..0x859).
    #[br(count = UNLOCKED_FISHING_SPOTS_BITMASK_SIZE)]
    #[bw(pad_size_to = UNLOCKED_FISHING_SPOTS_BITMASK_SIZE)]
    pub unlocked_fishing_spots: Vec<u8>,
    pub fishing_spots_padding: u8,
    #[br(count = CAUGHT_SPEARFISH_BITMASK_SIZE)]
    #[bw(pad_size_to = CAUGHT_SPEARFISH_BITMASK_SIZE)]
    pub caught_spearfish_mask: Vec<u8>,
    pub unlocked_spearfishing_notebooks: [u8; 8],
    pub padding_spearfishing: u8,
    /// 0x88A..0x88C -> PvPProfile+0x10..0x12 = Grand Company ranks: RankMaelstrom /
    /// RankTwinAdder / RankImmortalFlames (read by PvPProfile.ReadPacket, PvPProfile_Instance
    /// @0x142ab21ec). Retail: 11 00 00 = Maelstrom rank 0x11(17), other GCs 0 (player only
    /// joined one GC). (0x88D..0x8A0 is the beast-tribe rank array, all 0x08 = max rank 8.)
    pub gc_ranks: [u8; 3],
    pub beast_reputation_rank: [u8; BEAST_TRIBE_ARRAY_SIZE],
    /// 0x8A1..0x8AC -> PlayerState+0x520..0x52B = _contentRouletteCompletion bitmap
    /// (12 bytes). Read by InstanceContent.IsRouletteIncomplete (0x140C2A5CF): reads
    /// byte_142AAEE58[ContentRoulette-row+71], index 0..11 -> the full bitmap is 12 bytes.
    /// NOTE: CONTENT_ROULETTE_ARRAY_SIZE (T2) is extracted from the sibling ContentRoulette.CanGetAwards
    /// (strict `< 0xC`), not this reader (whose guard is an inclusive `<= 0xB` that would yield 11).
    pub content_roulette_completion: [u8; CONTENT_ROULETTE_ARRAY_SIZE],
    /// Persistent CPose selections, one index per PoseType category:
    /// 0=Idle, 1=WeaponDrawn, 2=Sit, 3=GroundSit, 4=Doze, 5=Umbrella, 6=Accessory, 7=reserved.
    /// The client stores these in `PlayerState.SelectedPoses` and re-applies the matching one
    /// whenever the player returns to idle, so this is what makes a `/cpose` choice persist.
    pub selected_poses: [u8; 8],
    pub player_state_flags1: PlayerStateFlags1,
    pub player_state_flags2: PlayerStateFlags2,
    pub player_state_flags3: PlayerStateFlags3,

    /// ContentsNote completion bitmap (104 bits = 13 bytes), one bit per ContentsNote entry.
    pub contents_note: [u8; CONTENTS_NOTE_BITMASK_SIZE],
    pub unlocked_secret_recipe_books: [u8; SECRET_RECIPE_BOOK_BITMASK_SIZE],
    // TODO Figure out what client should do to trigger reading from this region bruh
    #[br(count = 28)]
    #[bw(pad_size_to = 28)]
    pub unknown879: Vec<u8>,
    pub relic_monster_progress: [u8; 10],
    pub objective_progress: [u8; 2],
    #[br(count = ADVENTURE_BITMASK_SIZE)]
    #[bw(pad_size_to = ADVENTURE_BITMASK_SIZE)]
    pub adventure_mask: Vec<u8>,
    #[br(count = 124)]
    #[bw(pad_size_to = 124)]
    pub hunting_mark_data: Vec<u8>,
    #[br(count = TRIPLE_TRIAD_CARDS_BITMASK_SIZE)]
    #[bw(pad_size_to = TRIPLE_TRIAD_CARDS_BITMASK_SIZE)]
    pub triple_triad_cards: Vec<u8>,
    /// 0x9DE..0x9EE (17 bytes = 136 bits) -> UIState+0x1A1E8 = 0x142AC80E8, Triple Triad
    /// NPC-beaten bitmap. Checked by UIState.IsTripleTriadNpcBeaten (0x140C49710): for
    /// NPC ids 2293762..2293873 (0x230002..0x230091), tests bit (row+71-derived index)
    /// in this 17-byte bitmap (v4>>3 < 0x11). Adjacent to UnlockedTripleTriadCardsCount
    /// (0x142AC80E0). Retail: 12 DA 3B 59 21 0D E2 00 00 00 00 00 00 08 00 00 00 =
    /// which TT NPCs the player has beaten.
    #[br(count = TRIPLE_TRIAD_NPC_BITMASK_SIZE)]
    #[bw(pad_size_to = TRIPLE_TRIAD_NPC_BITMASK_SIZE)]
    pub triple_triad_npc_beaten: Vec<u8>,
    // We do -1 because of aether_current_comp_flg_set_bitmask1 being present way earlier.
    #[br(count = AETHER_CURRENT_COMP_FLG_SET_BITMASK_SIZE - 1)]
    #[bw(pad_size_to = AETHER_CURRENT_COMP_FLG_SET_BITMASK_SIZE - 1)]
    pub aether_current_comp_flg_set_bitmask2: Vec<u8>, // This is the rest of the full bitmask. The rest of the zones are in here.
    #[br(count = AETHER_CURRENT_BITMASK_SIZE)]
    #[bw(pad_size_to = AETHER_CURRENT_BITMASK_SIZE)]
    pub aether_currents_mask: Vec<u8>,
    pub unlocked_miner_folklore_tomes: [u8; 2],
    pub unlocked_botainst_folklore_tomes: [u8; 2],
    pub unlocked_fisher_folklore_tomes: [u8; 2],
    #[br(count = ORCHESTRION_ROLL_BITMASK_SIZE)]
    #[bw(pad_size_to = ORCHESTRION_ROLL_BITMASK_SIZE)]
    pub orchestrion_roll_mask: Vec<u8>,
    #[br(count = BEGINNER_TRAINING_ARRAY_SIZE)]
    #[bw(pad_size_to = BEGINNER_TRAINING_ARRAY_SIZE)]
    pub completed_beginner_training: Vec<u8>, // 0xAA0->0x688 _completedBeginnerTraining

    // TODO Figure out how the heck client is reading this
    /// 0xAA4..0xAA7 -> PlayerState+0x68C..0x68F = _completedMaskedCarnivale (32-bit bitmap of
    /// 0xAA4..0xAAE (11 bytes) -> bit-compressed into global AnimaWeapon object
    /// (unk_142AA8CB0) by sub_140892AD0: each byte & 0x7F (7-bit value), bit7 = flag.
    /// a2[8] bit7 -> EventItemManager 98 + EventItem 2001993 (乌兰的笔记 - trade token used
    /// to exchange for anima weapon enhancement items); a2[9] bit7 -> EventItemManager 97 +
    /// EventItem 2001994 (元灵透镜/Anima Lens);
    /// a1[11] = a2[7]>>7. Referenced by LuaPc.SaveAnimaWeapon5EventItems /
    /// GetAnimaWeapon7EnhancePoint / IsEquipAnimaWeapon and HandleActorControlPacket.
    /// 11 bytes = the 11 jobs of the original 3.x Anima/Soul Weapon system (3.15),
    /// NOT the 13 jobs of the later 5.x AnimaWeapon5 upgrade (AnimaWeapon5 sheet has 13 rows).
    /// VERIFIED: player has Summoner anima weapon in progress ("人造元灵终绽放") ->
    /// retail byte[8] = 0x80 (bit7=1, 7-bit value 0 = no enhancement points), so SMN is
    /// the 9th slot in that 11-job ordering.
    /// NOT a PlayerState field (earlier "masked carnivale" id was wrong - that is at 0xB72).
    pub unk_completion2: [u8; 11],

    pub weekly_bingo_order_data: [u8; 16],
    pub weekly_bingo_reward_data: [u8; 4],

    /// 0xAC3..0xACE (12 u8) = SatisfactionSupplyManager._satisfactionRanks (per-NPC satisfaction
    /// rank 1-5, the hearts in the UI). Retail capture: latest = 11x 5 + 1x 2 (the 12th/7.51 NPC
    /// reached rank 2 since the earlier all-0 capture).
    pub supply_satisfaction_ranks: [u8; SATISFACTION_NPC_ARRAY_SIZE],
    pub used_supply_allowances: [u8; SATISFACTION_NPC_ARRAY_SIZE],

    /// 0xADB (1 byte, value 0x1F in retail) -> PlayerState+0x697 = _unlockedSpecialContent
    /// (UIState+0x10CF). Bitmap of unlocked special contents; checked by
    /// UIState.IsInstanceContentUnlocked via unk_142AAEFCF (PlayerState+0x697).
    #[br(count = SPECIAL_CONTENT_ARRAY_SIZE)]
    #[bw(pad_size_to = SPECIAL_CONTENT_ARRAY_SIZE)]
    pub unlocked_special_content: Vec<u8>,

    // unlocked status
    #[br(count = RAID_ARRAY_SIZE)]
    #[bw(pad_size_to = RAID_ARRAY_SIZE)]
    pub unlocked_raids: Vec<u8>,

    #[br(count = DUNGEON_ARRAY_SIZE)]
    #[bw(pad_size_to = DUNGEON_ARRAY_SIZE)]
    pub unlocked_dungeons: Vec<u8>,

    #[br(count = GUILDHEST_ARRAY_SIZE)]
    #[bw(pad_size_to = GUILDHEST_ARRAY_SIZE)]
    pub unlocked_guildhests: Vec<u8>,

    #[br(count = TRIAL_ARRAY_SIZE)]
    #[bw(pad_size_to = TRIAL_ARRAY_SIZE)]
    pub unlocked_trials: Vec<u8>,

    #[br(count = CRYSTALLINE_CONFLICT_ARRAY_SIZE)]
    #[bw(pad_size_to = CRYSTALLINE_CONFLICT_ARRAY_SIZE)]
    pub unlocked_crystalline_conflict: Vec<u8>,

    #[br(count = FRONTLINE_ARRAY_SIZE)]
    #[bw(pad_size_to = FRONTLINE_ARRAY_SIZE)]
    pub unlocked_frontline: Vec<u8>,

    // cleared status
    #[br(count = RAID_ARRAY_SIZE)]
    #[bw(pad_size_to = RAID_ARRAY_SIZE)]
    pub cleared_raids: Vec<u8>,

    #[br(count = DUNGEON_ARRAY_SIZE)]
    #[bw(pad_size_to = DUNGEON_ARRAY_SIZE)]
    pub cleared_dungeons: Vec<u8>,

    #[br(count = GUILDHEST_ARRAY_SIZE)]
    #[bw(pad_size_to = GUILDHEST_ARRAY_SIZE)]
    pub cleared_guildhests: Vec<u8>,

    #[br(count = TRIAL_ARRAY_SIZE)]
    #[bw(pad_size_to = TRIAL_ARRAY_SIZE)]
    pub cleared_trials: Vec<u8>,

    #[br(count = CRYSTALLINE_CONFLICT_ARRAY_SIZE)]
    #[bw(pad_size_to = CRYSTALLINE_CONFLICT_ARRAY_SIZE)]
    pub cleared_crystalline_conflict: Vec<u8>,

    #[br(count = FRONTLINE_ARRAY_SIZE)]
    #[bw(pad_size_to = FRONTLINE_ARRAY_SIZE)]
    pub cleared_frontline: Vec<u8>,

    /// 0xB72..0xB75 (4 bytes) -> PlayerState+0x68C..0x68F = _completedMaskedCarnivale
    /// (32-bit bitmap of completed Masked Carnivale stages). VERIFIED live: player completed
    /// stages 1-2, value 03 00 00 00 (bit 0,1). Checked by UIState.IsInstanceContentCompleted
    /// case 13 via unk_142AAEFC4. (ReadPacket 0x140BDCD07: [rdi+0xB72]->[rsi+0x68C]).
    #[br(count = MASKED_CARNIVALE_ARRAY_SIZE)]
    #[bw(pad_size_to = MASKED_CARNIVALE_ARRAY_SIZE)]
    pub cleared_masked_carnivale: Vec<u8>,

    /// 0xB76..0xB7C (7 bytes) -> PlayerState+0x690..0x696 = _completedVVDNotebookContents.
    pub completed_vvd_notebook_contents: [u8; VVD_NOTEBOOK_CONTENTS_BITMASK_SIZE],

    #[br(count = MISC_CONTENT_ARRAY_SIZE)]
    #[bw(pad_size_to = MISC_CONTENT_ARRAY_SIZE)]
    pub unlocked_misc_content: Vec<u8>,

    #[br(count = MISC_CONTENT_ARRAY_SIZE)]
    #[bw(pad_size_to = MISC_CONTENT_ARRAY_SIZE)]
    pub cleared_misc_content: Vec<u8>,

    /// 0xB85..0xB86 (2 bytes, read) -> PlayerState+0x736..0x737; 0xB87 is trailing
    /// packet padding (never read, ReadPacket has no movzx for it). Retail packet: 00 00 00,
    /// but runtime values 02 03 observed at 0x736..0x737 (updated after login by other code).
    /// (Note: 0x738 is GoldSaucerContentStatus from pkt 0xAC, NOT part of this field.)
    pub unknown949: [u8; 3],
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::{BinRead, BinWrite};
    use std::io::Cursor;

    /// Serialized PlayerSetup body must equal the authoritative opcode size (2952 bytes).
    #[test]
    fn player_setup_size() {
        let mut cursor = Cursor::new(Vec::new());
        PlayerSetup::default().write_le(&mut cursor).unwrap();
        assert_eq!(cursor.position() as usize, 2952);
    }

    /// Round-trip layout guard using synthetic sentinel values (no real account data). Writes a
    /// PlayerSetup with distinctive markers in fields spread head→tail, serializes to exactly 2952
    /// bytes, reads it back, and asserts each marker survives at its offset — so a future field
    /// resize/misalignment (the class of bug that produced this struct's 16-byte overrun) fails here.
    #[test]
    fn player_setup_roundtrip_layout() {
        let mut ps = PlayerSetup::default();
        // sized Vec fields must be filled to their declared length for a faithful round-trip
        ps.framers_kits_mask = vec![0; FRAMERS_KIT_BITMASK_SIZE];
        ps.caught_fish_mask = vec![0; CAUGHT_FISH_BITMASK_SIZE];
        ps.cutscene_seen_mask = vec![0; CUTSCENE_SEEN_BITMASK_SIZE];
        ps.buddy_equip_mask = vec![0; BUDDY_EQUIP_BITMASK_SIZE];

        // synthetic sentinels spanning the struct
        ps.content_id = 0x1122334455667788;
        ps.actor_id = ObjectId(0xABCDEF01);
        ps.completed_lovm_stages = 7; // head
        ps.companion_name = "Buddy".to_string();
        ps.name = "TestChar".to_string(); // mid (right after framers mask)
        ps.companion_equipped_head = 0x41;
        ps.companion_equipped_body = 0x42;
        ps.companion_equipped_legs = 0x43;
        ps.doh_dol_levels = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        ps.gc_ranks = [0x11, 0x22, 0x33];
        ps.selected_poses = [1, 2, 3, 4, 5, 6, 7, 0];
        ps.supply_satisfaction_ranks = [5; SATISFACTION_NPC_ARRAY_SIZE]; // tail
        ps.unlocked_special_content = vec![0x1F];
        ps.cleared_masked_carnivale = vec![0x03, 0, 0, 0];

        let mut cursor = Cursor::new(Vec::new());
        ps.write_le(&mut cursor).unwrap();
        assert_eq!(cursor.position() as usize, 2952, "serialized size");

        let bytes = cursor.into_inner();
        let got = PlayerSetup::read_le(&mut Cursor::new(&bytes)).unwrap();

        assert_eq!(got.content_id, ps.content_id, "content_id");
        assert_eq!(got.actor_id, ps.actor_id, "actor_id");
        assert_eq!(got.completed_lovm_stages, 7, "completed_lovm_stages");
        assert_eq!(got.companion_name, "Buddy", "companion_name");
        assert_eq!(got.name, "TestChar", "name (offset right after framers mask)");
        assert_eq!(
            [got.companion_equipped_head, got.companion_equipped_body, got.companion_equipped_legs],
            [0x41, 0x42, 0x43],
            "companion_equipped"
        );
        assert_eq!(got.doh_dol_levels, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], "doh_dol_levels");
        assert_eq!(got.gc_ranks, [0x11, 0x22, 0x33], "gc_ranks");
        assert_eq!(got.selected_poses, [1, 2, 3, 4, 5, 6, 7, 0], "selected_poses");
        assert_eq!(got.supply_satisfaction_ranks, [5; 12], "supply_satisfaction_ranks");
        assert_eq!(got.unlocked_special_content, vec![0x1F], "unlocked_special_content");
        assert_eq!(got.cleared_masked_carnivale, vec![0x03, 0, 0, 0], "cleared_masked_carnivale");
    }
}
