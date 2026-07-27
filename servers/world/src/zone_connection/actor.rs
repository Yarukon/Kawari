//! Everything to do with spawning, managing and moving actors - including the player.

use crate::{
    ToServer, ZoneConnection,
    common::SpawnKind,
    inventory::{Storage, display_flags_to_checksum_flag, gearset_checksum_from_equipped},
    zone_connection::effective_level,
};
use kawari::{
    common::{
        CharacterMode, EquipDisplayFlag, JumpState, MoveAnimationState, MoveAnimationType,
        ObjectId, ObjectTypeId, Position,
    },
    config::get_config,
    ipc::zone::{
        ActorControl, ActorControlCategory, ActorControlSelf, ActorControlTarget, ActorMove,
        CommonSpawn, Config, DisplayFlag, ExamineEquipEntry, ExamineMateria,
        GrandCompany as IpcGrandCompany, ObjectKind, PartyPortraitEntry, PlayerSubKind,
        PortraitBanner, ServerZoneIpcData, ServerZoneIpcSegment, SpawnObject, SpawnPlayer,
        SpawnTreasure,
    },
};

impl ZoneConnection {
    pub async fn set_actor_position(
        &mut self,
        actor_id: ObjectId,
        position: Position,
        rotation: f32,
        anim_type: MoveAnimationType,
        anim_state: MoveAnimationState,
        jump_state: JumpState,
    ) {
        const SPEED_WALKING: u8 = 20;
        const SPEED_RUNNING: u8 = 60;

        let mut anim_type = anim_type;
        let mut anim_speed = SPEED_RUNNING; // TODO: sprint is 78, jog is 72, but falling and normal running are always 60

        // We're purely walking or strafing while walking. No jumping or falling.
        if anim_type & MoveAnimationType::WALKING_OR_LANDING
            == MoveAnimationType::WALKING_OR_LANDING
            && anim_state.is_empty()
            && jump_state.is_empty()
        {
            anim_speed = SPEED_WALKING;
        }

        if anim_state.contains(MoveAnimationState::LEAVING_COLLISION) {
            anim_type |= MoveAnimationType::FALLING;
        }

        if jump_state.contains(JumpState::ASCENDING) {
            anim_type |= MoveAnimationType::FALLING;
            if anim_state.contains(MoveAnimationState::LEAVING_COLLISION)
                || anim_state.contains(MoveAnimationState::START_FALLING)
            {
                anim_type |= MoveAnimationType::JUMPING;
            }
        }

        if anim_state.contains(MoveAnimationState::ENTERING_COLLISION) {
            anim_type = MoveAnimationType::WALKING_OR_LANDING;
        }

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ActorMove(ActorMove {
            rotation,

            anim_type,
            anim_state,
            anim_speed,
            position,
        }));

        self.send_ipc_from(actor_id, ipc).await;
    }

    pub async fn spawn_actor(&mut self, actor_id: ObjectId, spawn: SpawnKind) {
        // There is no reason for us to spawn our own player again. It's probably a bug!
        assert!(actor_id != self.player_data.character.actor_id);

        let ipc = match spawn {
            SpawnKind::Player(spawn) => {
                ServerZoneIpcSegment::new(ServerZoneIpcData::SpawnPlayer(spawn))
            }
            SpawnKind::Npc(spawn) => ServerZoneIpcSegment::new(ServerZoneIpcData::SpawnNpc(spawn)),
        };
        self.send_ipc_from(actor_id, ipc).await;
    }

    pub async fn delete_actor(&mut self, actor_id: ObjectId, spawn_index: u8) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::DeleteActor {
            spawn_index,
            actor_id,
        });

        self.send_ipc_from(actor_id, ipc).await;
    }

    pub async fn delete_object(&mut self, spawn_index: u8) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::DeleteObject { spawn_index });

        self.send_ipc_self(ipc).await;
    }

    pub async fn toggle_invisibility(&mut self, invisible: bool) {
        self.player_data.gm_invisible = invisible;
        self.actor_control_self(ActorControlCategory::ToggleInvisibility { invisible })
            .await;
    }

    pub async fn actor_control_self(&mut self, category: ActorControlCategory) {
        let ipc =
            ServerZoneIpcSegment::new(ServerZoneIpcData::ActorControlSelf(ActorControlSelf {
                category,
            }));
        self.send_ipc_self(ipc).await;
    }

    /// Broadcasts an actor control to everyone around you, including yourself. Useful for stuff like crafting.
    pub async fn broadcast_actor_control(&mut self, category: ActorControlCategory) {
        let ipc =
            ServerZoneIpcSegment::new(ServerZoneIpcData::ActorControlSelf(ActorControlSelf {
                category: category.clone(),
            }));
        self.send_ipc_self(ipc).await;

        self.handle
            .send(ToServer::BroadcastActorControl(
                self.player_data.character.actor_id,
                category,
            ))
            .await;
    }

    pub async fn actor_control(&mut self, actor_id: ObjectId, category: ActorControlCategory) {
        let ipc =
            ServerZoneIpcSegment::new(ServerZoneIpcData::ActorControl(ActorControl { category }));

        self.send_ipc_from(actor_id, ipc).await;
    }

    pub async fn actor_control_target(
        &mut self,
        actor_id: ObjectId,
        target: ObjectTypeId,
        category: ActorControlCategory,
    ) {
        let ipc =
            ServerZoneIpcSegment::new(ServerZoneIpcData::ActorControlTarget(ActorControlTarget {
                category,
                target,
            }));

        self.send_ipc_from(actor_id, ipc).await;
    }

    /// Spawn the player actor. The client will handle replacing the existing one, if it exists.
    pub async fn respawn_player(&mut self, start_invisible: bool) -> SpawnPlayer {
        let common = self.get_player_common_spawn(start_invisible);
        let config = get_config();

        let spawn = SpawnPlayer {
            account_id: self.player_data.character.service_account_id as u64,
            content_id: self.player_data.character.content_id as u64,
            current_world_id: config.world.world_id,
            home_world_id: config.world.world_id,
            gm_rank: self.player_data.character.gm_rank,
            online_status: self.nametag_online_status(),
            common: common.clone(),
            title_id: self.player_data.volatile.title as u16,
            // The player is always standing (PoseType::Idle) right after (re)spawn, so seed the
            // rendered pose from the persisted idle selection. Non-idle stances are re-applied by
            // the client via ReapplyPose using the full SelectedPoses array in PlayerSetup.
            pose: self.player_data.volatile.poses()[0],
            ..Default::default()
        };

        // send player spawn
        {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::SpawnPlayer(spawn.clone()));
            self.send_ipc_self(ipc).await;
        }

        self.spawned_in = true;

        spawn
    }

    pub async fn update_config(&mut self, actor_id: ObjectId, config: Config) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::Config(config));

        self.send_ipc_from(actor_id, ipc).await;
    }

    fn get_player_common_spawn(&self, start_invisible: bool) -> CommonSpawn {
        let inventory = &self.player_data.inventory;

        let mut database = self.database.lock();
        let chara_make = database.get_chara_make(self.player_data.character.content_id as u64);
        let mut look = chara_make.customize;

        // There seems to be no display flag for this, so clear the bit out
        if self
            .player_data
            .volatile
            .display_flags
            .intersects(EquipDisplayFlag::HIDE_LEGACY_MARK)
        {
            look.facial_features &= !(1 << 7);
        }

        let mut display_flags = self.player_data.volatile.display_flags.into();
        if start_invisible {
            display_flags |= DisplayFlag::INVISIBLE;
        }

        let base_parameters = self.base_parameters(); // TODO: maybe cache this?
        let mut game_data = self.gamedata.lock();

        CommonSpawn {
            class_job: self.player_data.classjob.current_class as u8,
            name: self.player_data.character.name.clone(),
            health_points: base_parameters.hp,
            max_health_points: base_parameters.hp,
            resource_points: base_parameters.resource_points() as u16,
            max_resource_points: base_parameters.resource_points() as u16,
            level: effective_level(self.synced_level, self.current_level(&game_data) as u8),
            object_kind: ObjectKind::Player(PlayerSubKind::Player),
            look,
            display_flags,
            main_weapon_model: inventory.get_main_weapon_id(&mut game_data),
            sec_weapon_model: inventory.get_sub_weapon_id(&mut game_data),
            models: inventory.legacy_model_ids(&mut game_data),
            second_model_stain_ids: inventory.second_model_stain_ids(),
            glasses_ids: inventory.equipped.glasses,
            position: self.player_data.volatile.position,
            rotation: self.player_data.volatile.rotation as f32,
            voice: chara_make.voice_id as u8,
            active_minion: self.active_minion as u16,
            handler_id: self.content_handler_id,
            // TODO: Dismount if entering a duty? Towns are probably fine to leave alone.
            current_mount: self.player_data.volatile.current_mount as u16,
            mode: if self.player_data.volatile.current_mount != 0 {
                CharacterMode::Mounted
            } else {
                CharacterMode::default()
            },
            ..Default::default()
        }
    }

    /// Builds this player's own [`PartyPortraitEntry`] from their live connection data.
    ///
    /// All data comes from `self.player_data` (own connection context): equipped gear (apparent
    /// ids + stains), live glasses, stored plate/banner, customize, class job and content id.
    /// The 12-slot gear ordering mirrors the adventurer plate exactly (waist and soul crystal are
    /// excluded).
    ///
    /// The banner style is gated by the gearset checksum: the stored custom banner is only used if
    /// the player has one saved AND its checksum still matches the current live gear; otherwise the
    /// banner falls back to [`PortraitBanner::default()`]. The gear/customize/stains fields always
    /// carry the current live appearance regardless.
    fn build_own_portrait_entry(&self) -> PartyPortraitEntry {
        let equipped = &self.player_data.inventory.equipped;
        let glasses = equipped.glasses;

        // Customize + class job come from the same sources get_player_common_spawn uses.
        let customize = {
            let mut database = self.database.lock();
            database
                .get_chara_make(self.player_data.character.content_id as u64)
                .customize
        };
        let class_job_id = self.player_data.classjob.current_class as u8;

        // Wire field for the entry: the client-submitted plate visibility byte (its own encoding),
        // i.e. the visibility captured with the portrait. 0 when the player has never saved a plate.
        let gear_visibility_flag = if self.player_data.plate.has_plate {
            self.player_data.plate.design().gear_visibility_flag
        } else {
            0
        };

        // checksum gate: recompute the fingerprint from the live gear and compare against the
        // stored banner's captured checksum. Only reuse the stored banner if it still matches.
        //
        // The visibility component of the checksum must come from the player's LIVE display flags,
        // NOT the stored plate byte: the client bakes the banner checksum from the gearset's
        // visibility at capture (== the live toggles then), and switching a gearset re-sends the
        // banner. If the player later changes a visibility toggle without switching gearset, the
        // client marks the gearset out-of-date and the live flags no longer match the captured
        // banner — which is exactly the mismatch this recompute must detect. (See
        // display_flags_to_checksum_flag for the verified bit mapping.)
        let checksum_vis_flag = display_flags_to_checksum_flag(self.player_data.volatile.display_flags);
        let recompute = gearset_checksum_from_equipped(equipped, glasses, checksum_vis_flag);
        let banner = if self.player_data.plate.has_banner
            && recompute == self.player_data.plate.banner().checksum
        {
            self.player_data.plate.banner()
        } else {
            PortraitBanner::default()
        };

        // 12-slot ordering (waist and soul crystal excluded): MainHand, OffHand, Head, Body,
        // Hands, Legs, Feet, Ears, Neck, Wrists, RightRing, LeftRing. The client fills item_ids
        // and both stain arrays from a single loop over the equipped container (verified against
        // BannerHelper_TryGetItemDataFromEquippedItems), so ids and stains MUST use the same ring
        // order: right ring at index 10, left ring at index 11. (Note: the adventurer-plate code
        // in database/character.rs swaps the rings between item_ids and stains — that is a bug
        // there; do not mirror it.)
        let item_ids = [
            equipped.main_hand.apparent_id(),
            equipped.off_hand.apparent_id(),
            equipped.head.apparent_id(),
            equipped.body.apparent_id(),
            equipped.hands.apparent_id(),
            equipped.legs.apparent_id(),
            equipped.feet.apparent_id(),
            equipped.ears.apparent_id(),
            equipped.neck.apparent_id(),
            equipped.wrists.apparent_id(),
            equipped.right_ring.apparent_id(),
            equipped.left_ring.apparent_id(),
        ];
        let stain0 = [
            equipped.main_hand.stains[0],
            equipped.off_hand.stains[0],
            equipped.head.stains[0],
            equipped.body.stains[0],
            equipped.hands.stains[0],
            equipped.legs.stains[0],
            equipped.feet.stains[0],
            equipped.ears.stains[0],
            equipped.neck.stains[0],
            equipped.wrists.stains[0],
            equipped.right_ring.stains[0],
            equipped.left_ring.stains[0],
        ];
        let stain1 = [
            equipped.main_hand.stains[1],
            equipped.off_hand.stains[1],
            equipped.head.stains[1],
            equipped.body.stains[1],
            equipped.hands.stains[1],
            equipped.legs.stains[1],
            equipped.feet.stains[1],
            equipped.ears.stains[1],
            equipped.neck.stains[1],
            equipped.wrists.stains[1],
            equipped.right_ring.stains[1],
            equipped.left_ring.stains[1],
        ];

        PartyPortraitEntry {
            encrypted_aid: 0, // Kawari has no AID encryption
            content_id: self.player_data.character.content_id as u64,
            gear_visibility_flag,
            class_job_id,
            banner,
            item_ids,
            glasses,
            customize,
            stain0,
            stain1,
        }
    }

    /// Pushes this player's own duty portrait entry.
    ///
    /// In a party, the entry is sent to the server ([`ToServer::UpdatePartyPortrait`]), which stores
    /// it on the party and rebroadcasts the full portrait wall to the pusher's instance — each member
    /// landing at its own `party.members` slot. Solo / partyless, it keeps the direct single-slot 634
    /// in slot 0 on this connection.
    pub async fn send_own_party_portrait(&mut self) {
        let entry = self.build_own_portrait_entry();
        if self.is_in_party() {
            self.handle
                .send(ToServer::UpdatePartyPortrait(
                    self.player_data.character.actor_id,
                    entry,
                ))
                .await;
        } else {
            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::PartyMemberPortrait {
                slot_index: 0,
                entry,
            });
            self.send_ipc_self(ipc).await;
        }
    }

    pub async fn send_conditions(&mut self) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::Condition(self.conditions));
        self.send_ipc_self(ipc).await;

        // Inform the server state as well
        self.handle
            .send(ToServer::UpdateConditions(
                self.player_data.character.actor_id,
                self.conditions,
            ))
            .await;
    }

    pub async fn spawn_object(&mut self, spawn: SpawnObject) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::SpawnObject(spawn));

        self.send_ipc_from(spawn.entity_id, ipc).await;
    }

    /// Sets this actor's CharacterMode and informs other clients.
    pub async fn set_character_mode(&mut self, mode: CharacterMode, arg: u8) {
        self.handle
            .send(ToServer::SetCharacterMode(
                self.player_data.character.actor_id,
                mode,
                arg,
            ))
            .await;
    }

    pub async fn spawn_treasure(&mut self, spawn: SpawnTreasure) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::SpawnTreasure(spawn.clone()));

        self.send_ipc_from(spawn.entity_id, ipc).await;
    }

    /// Builds the [`ServerZoneIpcData::ExamineCharacterInformation`] payload entirely from this
    /// connection's **live** in-memory [`PlayerData`] so that the examine window always reflects
    /// the player's current gear, glamour, display flags and title — even when those have been
    /// mutated since the last DB commit (e.g. after `equip_gearset` or a `Config` packet).
    ///
    /// Character appearance (`CustomizeData`) is the one field that is not mirrored in
    /// `PlayerData`; it is fetched from the DB via `get_chara_make`, which is always current
    /// because the aesthetician writes it immediately on change.
    pub fn build_examine_ipc(&self) -> ServerZoneIpcData {
        let inventory = &self.player_data.inventory;
        let volatile = &self.player_data.volatile;
        let classjob = &self.player_data.classjob;
        let grand_company = &self.player_data.grand_company;
        let character = &self.player_data.character;

        // Appearance data — always up-to-date in the DB (written by aesthetician immediately).
        let chara_make = {
            let mut database = self.database.lock();
            database.get_chara_make(character.content_id as u64)
        };

        // GC rank for the currently active company (1-indexed; 0 = no company).
        let gc_rank = if grand_company.active_company != IpcGrandCompany::None {
            grand_company.company_ranks.0[grand_company.active_company as usize - 1]
        } else {
            0
        };

        // Build the 14-slot equipment array from the live equipped container.
        let equipped = &inventory.equipped;
        let mut equipment: [ExamineEquipEntry; 14] = Default::default();
        for (slot, entry) in equipment.iter_mut().enumerate() {
            let item = equipped.get_slot(slot as u16);
            let mut materia: [ExamineMateria; 5] = Default::default();
            for (i, m) in materia.iter_mut().enumerate() {
                m.id = item.materia[i];
                m.grade = item.materia_grades[i] as u16;
            }
            // bit 0 of item_flags = HQ flag; client calls SetIsHighQuality(slot+0x10 != 0).
            let is_hq = (item.item_flags & 1) as u16;
            *entry = ExamineEquipEntry {
                catalog_id: item.item_id,
                glamour_id: item.glamour_id,
                crafter_content_id: item.crafter_content_id,
                is_hq,
                materia,
                stain0: item.stains[0],
                stain1: item.stains[1],
            };
        }

        let mut game_data = self.gamedata.lock();

        // Current class level via the EXP-array index (same mapping used by `current_level`).
        let level = game_data
            .get_exp_array_index(classjob.current_class as u16)
            .map(|index| classjob.levels.0[index as usize])
            .unwrap_or(0);

        // Average item level of equipped gear — client displays this value directly.
        let item_level = equipped.calculate_item_level(&mut game_data);

        // 3-D model ids from live inventory (identical sources as the spawn path).
        let main_weapon_model = inventory.get_main_weapon_id(&mut game_data);
        let sub_weapon_model = inventory.get_sub_weapon_id(&mut game_data);
        let equipment_models = inventory.legacy_model_ids(&mut game_data);
        let equipment_model_stains1 = inventory.second_model_stain_ids();

        // InspectGearVisibilityFlag: bit0 VisorClosed, bit1 HeadgearHidden, bit2 WeaponHidden.
        let display_flags = volatile.display_flags;
        let mut gear_visibility_flag: u8 = 0;
        if display_flags.contains(EquipDisplayFlag::CLOSE_VISOR) {
            gear_visibility_flag |= 1 << 0;
        }
        if display_flags.contains(EquipDisplayFlag::HIDE_HEAD) {
            gear_visibility_flag |= 1 << 1;
        }
        if display_flags.contains(EquipDisplayFlag::HIDE_WEAPON) {
            gear_visibility_flag |= 1 << 2;
        }

        let config = get_config();
        ServerZoneIpcData::ExamineCharacterInformation {
            examine_kind: 4,
            sex: chara_make.customize.gender as u8,
            class_job_id: classjob.current_class as u8,
            level: level as u8,
            synced_level: 0,
            title_id: volatile.title as u16,
            grand_company: grand_company.active_company as u8,
            gc_rank,
            gear_visibility_flag,
            fc_crest_data: 0,
            fc_crest_bitfield: 0,
            main_weapon_model,
            sub_weapon_model,
            world_id: config.world.world_id,
            item_level,
            equipment,
            name: character.name.clone(),
            online_id: [0; 32],
            customize: chara_make.customize,
            equipment_models,
            equipment_model_stains1,
            glasses_ids: inventory.equipped.glasses,
            tail: [0; 158],
        }
    }
}
