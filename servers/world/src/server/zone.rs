use std::{collections::HashMap, f32::consts::FRAC_PI_2, sync::Arc};

use glam::{Affine3A, EulerRot, Vec3};
use parking_lot::Mutex;
use physis::{
    TerritoryIntendedUse,
    layer::{
        ExitRangeInstanceObject, InstanceObject, LayerEntryData, PopRangeInstanceObject, PopType,
        TriggerBoxShape,
    },
    lgb::Lgb,
    lvb::Lvb,
};

use crate::{
    ClientId, FromServer, GameData, TerritoryNameKind, ToServer,
    lua::LuaZone,
    server::{
        NetworkedActor, WorldServer,
        actor::NpcState,
        combat_state::{CarriedPet, PlayerCombatState},
        instance::{Instance, QueuedTaskData},
        jobs::dispatch::{Job, job_for},
        network::{DestinationNetwork, NetworkState},
    },
    zone_connection::BaseParameters,
};
use kawari::{
    common::{
        DropIn, DropInLayer, DropInObjectData, ENTRANCE_CIRCLE_IDS, EOBJ_EXIT,
        EOBJ_HOUSING_ENTRANCE, EOBJ_SHORTCUT, EOBJ_SHORTCUT_EXPLORER_MODE, EventState, HandlerType,
        ObjectId, Position, WARP_DELAY, euler_to_direction, internal_housing_row,
    },
    config::get_config,
    ipc::zone::{
        ActorControlCategory, ActorSetPos, BattleNpcSubKind, CharacterDataFlag, CommonSpawn,
        DisplayFlag, ObjectKind, ServerZoneIpcData, ServerZoneIpcSegment, SpawnNpc, SpawnObject,
        SpawnTreasure, WarpType,
    },
};

#[derive(Debug)]
pub enum MapGimmick {
    /// Seen for final boss triggers in Sastasha
    Generic {},
    /// Jump pads like the ones in Gold Saucer.
    Jump {
        /// The position to land on.
        to_position: Vec3,
        /// The GimmickJump type.
        gimmick_jump_type: u32,
        /// The animation ID to play for the EObj.
        sgb_animation_id: u32,
        /// The EObj's instance ID to play the animation for.
        eobj_instance_id: u32,
    },
    /// Unsure of what to call these, but these are "exit lines" like as seen in the overworld but go to another poprange in the same zone.
    /// Used heavily in instanced content.
    FakeExit { exit_pop_range_id: u32 },
}

/// Simpler form of a MapRange object designed for collision detection.
#[derive(Debug)]
pub struct MapRange {
    /// Trigger box shape.
    pub trigger_box_shape: TriggerBoxShape,
    /// Position of this range in the world.
    pub position: Vec3,
    /// Facing (world direction yaw) baked from this range's transform rotation. For the entrance
    /// EventRange (EntranceRect) this is the direction the player faces on zone-in.
    pub rotation: f32,
    /// Relative scale of this range.
    pub scale: Vec3,
    /// Whether this map range represents a sanctuary.
    pub sanctuary: bool,
    /// Whether this map range represents a PvP duel area.
    pub duel: bool,
    /// Whether this map range represents a gimmick, like a jumping pad.
    pub gimmick: Option<MapGimmick>,
    /// Game Object ID. Also known as the layout ID. The client sends this when discovering new areas.
    pub instance_id: u32,
    /// The MapRange's discovery index. Unclear if this is the same as DiscoveryIndex on the Map sheet.
    pub discovery_id: Option<u8>,
    /// Whether this map range represents an instance exit.
    pub entrance: bool,
}

#[derive(Debug)]
struct HousingPlot {
    entrance_position: Vec3,
}

/// Represents a loaded zone
#[derive(Default, Debug)]
pub struct Zone {
    pub id: u16,
    pub internal_name: String,
    pub region_name: String,
    pub place_name: String,
    pub intended_use: u8,
    pub layer_groups: Vec<Lgb>,
    pub navimesh_path: String,
    pub map_id: u16,
    cached_npc_base_ids: HashMap<ObjectId, u32>,
    pub map_ranges: Vec<MapRange>,
    dropin_layers: Vec<DropInLayer>,
    cached_objects: HashMap<u32, SpawnObject>,
    cached_npcs: HashMap<u32, SpawnNpc>,
    // Key is Treasure sheet base_id (u32 since physis nested it under GameObjectInstanceObject).
    cached_treasure: HashMap<u32, SpawnTreasure>,
    layer_set: i32,
    bg_path: String,
    cached_housing_plots: Vec<HousingPlot>,
    cached_eobj_base_ids: HashMap<ObjectId, u32>,
}

impl Zone {
    pub fn load(game_data: &mut GameData, id: u16) -> Self {
        let mut zone = Self {
            id,
            ..Default::default()
        };

        let Some(row) = game_data.territory_type_sheet.row(id as u32) else {
            tracing::warn!("Invalid zone id {id}, allowing anyway...");
            return zone;
        };

        zone.intended_use = row.TerritoryIntendedUse;
        zone.map_id = row.Map;

        // e.g. ffxiv/fst_f1/fld/f1f3/level/f1f3
        let bg_path = row.Bg;
        if bg_path.is_empty() {
            tracing::warn!("Invalid zone id {id}, allowing anyway...");
            return zone;
        }

        let path = format!("bg/{}.lvb", &bg_path);
        if let Ok(lvb) = game_data.resource.parsed::<Lvb>(&path) {
            let mut load_lgb = |path: &str| -> Option<Lgb> {
                // Skip LGBs that aren't relevant for the server
                if path.ends_with("bg.lgb")
                    || path.ends_with("vfx.lgb")
                    || path.ends_with("sound.lgb")
                {
                    return None;
                }

                let lgb = game_data.resource.parsed::<Lgb>(path);

                if let Err(e) = &lgb {
                    tracing::warn!(
                        "Failed to parse {path}: {e}, this is most likely a bug in Physis and should be reported somewhere!"
                    )
                }

                lgb.ok()
            };

            for path in &lvb.sections[0].lgb_paths {
                if let Some(lgb) = load_lgb(path) {
                    zone.layer_groups.push(lgb);
                }
            }

            for layer_set in &lvb.sections[0].layer_sets.layer_sets {
                if layer_set.territory_type_id == id {
                    zone.layer_set = layer_set.id;
                    zone.navimesh_path = layer_set
                        .nvm_path
                        .value
                        .replace("/server/data/", "")
                        .to_string();

                    break;
                }
            }

            let mut search_dirs: Vec<String> = get_config()
                .filesystem
                .additional_resource_paths
                .iter()
                .cloned()
                .map(|mut x| {
                    x.push_str("/dropins/");
                    x
                })
                .collect();
            search_dirs.push("resources/dropins/".to_string());

            'outer: for search_dir in search_dirs {
                // Load drop-ins
                for entry in std::fs::read_dir(search_dir)
                    .expect("Didn't find dropins directory?")
                    .flatten()
                {
                    if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                        match serde_json::from_str::<DropIn>(&contents) {
                            Ok(mut dropin) => {
                                if lvb.sections[0].lgb_paths.contains(&dropin.appends) {
                                    tracing::info!("Loaded dropin from {:?}", entry.path());
                                    zone.dropin_layers.append(&mut dropin.layers);
                                    break 'outer;
                                }
                            }
                            Err(err) => {
                                tracing::warn!("Failed to load drop-in {:?}: {err:?}", entry.path())
                            }
                        }
                    }
                }
            }

            zone.bg_path = lvb.sections[0].general.bg_path.value.clone();
        }

        // create NPC ID cache
        for layer_group in &zone.layer_groups {
            for chunk in &layer_group.chunks {
                for layer in &chunk.layers {
                    if !layer.header.has_layer_set(zone.layer_set as u32) {
                        continue;
                    }

                    for object in &layer.objects {
                        let (scale, rotation, translation) =
                            Affine3A::from(object.transform).to_scale_rotation_translation();
                        let facing = euler_to_direction(rotation.to_euler(EulerRot::XYZ));

                        if let LayerEntryData::EventNPC(npc) = &object.data {
                            zone.cached_npc_base_ids.insert(
                                ObjectId(object.instance_id),
                                npc.parent_data.parent_data.base_id,
                            );
                        }
                        if let LayerEntryData::MapRange(map_range) = &object.data {
                            zone.map_ranges.push(MapRange {
                                trigger_box_shape: map_range.parent_data.trigger_box_shape,
                                position: translation,
                                rotation: facing,
                                scale,
                                sanctuary: map_range.rest_bonus_enabled,
                                duel: false,
                                gimmick: None,
                                instance_id: object.instance_id,
                                discovery_id: if map_range.discovery_enabled {
                                    Some(map_range.discovery_id)
                                } else {
                                    None
                                },
                                entrance: false,
                            });
                        }
                        if let LayerEntryData::EventRange(event_range) = &object.data {
                            zone.map_ranges.push(MapRange {
                                trigger_box_shape: event_range.parent_data.trigger_box_shape,
                                position: translation,
                                rotation: facing,
                                scale,
                                sanctuary: false,
                                // This is guesswork since there's only one dueling location in-game
                                // TODO: restore duel support by hardcoding its ID
                                // duel: event_range.unk_flags[0] == 1
                                //     && event_range.unk_flags[3] == 1
                                //     && event_range.unk_flags[4] == 1
                                //     && event_range.unk_flags[5] == 1,
                                duel: false,
                                gimmick: None,
                                instance_id: object.instance_id,
                                discovery_id: None,
                                // Set later!
                                entrance: false,
                            });
                        }
                    }

                    // Second pass for eobjs
                    for object in &layer.objects {
                        // TODO: restore
                        // if !layer.header.has_layer_set(zone.layer_set as u32) {
                        //     continue;
                        // }

                        if let LayerEntryData::EventObject(eobj) = &object.data {
                            let eobj_data = game_data.get_eobj_data(eobj.parent_data.base_id);
                            let event_type = HandlerType::from_repr(eobj_data >> 16);

                            if let Some(HandlerType::GimmickRect) = event_type {
                                // GimmickRects are used for stuff like the Golden Saucer jumping pads, and is handled server-side.
                                // Thus, we need to go through and mark these MapRanges to play said event.
                                if let Some(gimmick_rect_info) =
                                    game_data.get_gimmick_rect_info(eobj_data & 0xFFFF)
                                    && let Some(target_pop_range) =
                                        zone.find_pop_range(gimmick_rect_info.Params[1])
                                {
                                    let gimmick_jump_type = gimmick_rect_info.Params[0];
                                    let target_event_range = gimmick_rect_info.LayoutID;
                                    let sgb_animation_id = gimmick_rect_info.Params[2];

                                    // 8 seems to indicate a jumping pad
                                    if gimmick_rect_info.TriggerIn == 8 {
                                        let (_, _, translation) =
                                            Affine3A::from(target_pop_range.0.transform)
                                                .to_scale_rotation_translation();

                                        let map_gimmick = MapGimmick::Jump {
                                            to_position: translation,
                                            gimmick_jump_type,
                                            sgb_animation_id,
                                            eobj_instance_id: object.instance_id,
                                        };

                                        for map_range in &mut zone.map_ranges {
                                            if map_range.instance_id == target_event_range {
                                                map_range.gimmick = Some(map_gimmick);
                                                break;
                                            }
                                        }
                                    } else {
                                        tracing::warn!(
                                            "Unsupported Gimmick trigger {}",
                                            gimmick_rect_info.TriggerIn
                                        );
                                    }
                                } else {
                                    tracing::warn!(
                                        "Failed to lookup Gimmick {}?!",
                                        eobj_data & 0xFFFF
                                    );
                                }
                            }
                        } else if let LayerEntryData::EventRange(_) = &object.data
                            && let Some(gimmick_rect_info) =
                                game_data.lookup_gimmick_rect(object.instance_id)
                        {
                            let mut map_gimmick = None;
                            match gimmick_rect_info.TriggerIn {
                                1 | 18 => {
                                    // FIXME: 1 is seen for cutscene triggers in Sastasha, while 18 is seen for Variant Dungeon routes in A Merchant's Tale. We should make this less "generic".
                                    map_gimmick = Some(MapGimmick::Generic {});
                                }
                                6 => {
                                    // Seen for same-zone "exit ranges" like the one in the beginning of Sycrus Tower
                                    map_gimmick = Some(MapGimmick::FakeExit {
                                        exit_pop_range_id: gimmick_rect_info.Params[0],
                                    });
                                }
                                _ => tracing::warn!(
                                    "Unknown GimmickRect type: {} for event range instance {}",
                                    gimmick_rect_info.TriggerIn,
                                    object.instance_id
                                ),
                            }

                            for map_range in &mut zone.map_ranges {
                                if map_range.instance_id == object.instance_id {
                                    map_range.gimmick = map_gimmick;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // load names
        let fallback = "<Unable to load name!>";
        zone.internal_name = game_data
            .get_territory_name(id as u32, TerritoryNameKind::Internal)
            .unwrap_or(fallback.to_string());
        zone.region_name = game_data
            .get_territory_name(id as u32, TerritoryNameKind::Region)
            .unwrap_or(fallback.to_string());
        zone.place_name = game_data
            .get_territory_name(id as u32, TerritoryNameKind::Place)
            .unwrap_or(fallback.to_string());

        // create housing plot cache
        if zone.intended_use == TerritoryIntendedUse::HousingOutdoor as u8 {
            let land_sets = game_data
                .get_land_sets(internal_housing_row(id).unwrap())
                .unwrap();
            for land_set in land_sets {
                let map_ranges: Vec<&MapRange> = zone
                    .map_ranges
                    .iter()
                    .filter(|x| x.instance_id == land_set.MapRange)
                    .collect();
                if map_ranges.is_empty() {
                    tracing::warn!(
                        "Failed to find map range for a plot! The entrance won't spawn!"
                    );
                } else {
                    let map_range = map_ranges.first().unwrap();

                    zone.cached_housing_plots.push(HousingPlot {
                        entrance_position: map_range.position,
                    });
                }
            }
        }

        zone
    }

    /// Search for an exit box matching an id.
    pub fn find_exit_box(
        &self,
        instance_id: u32,
    ) -> Option<(&InstanceObject, &ExitRangeInstanceObject)> {
        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    if let LayerEntryData::ExitRange(exit_range) = &object.data
                        && object.instance_id == instance_id
                    {
                        return Some((object, exit_range));
                    }
                }
            }
        }

        None
    }

    pub fn find_pop_range(
        &self,
        instance_id: u32,
    ) -> Option<(&InstanceObject, &PopRangeInstanceObject)> {
        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    if let LayerEntryData::PopRange(pop_range) = &object.data
                        && object.instance_id == instance_id
                    {
                        return Some((object, pop_range));
                    }
                }
            }
        }

        None
    }

    /// Locates the first `PopType::PC` pop range in the zone. Used as a last-resort entrance
    /// fallback for instanced content that has neither an entrance-circle EObj nor an
    /// `entrance`-flagged map range (e.g. `InstanceContent.LGBEventRange == 0`); its PC pop ranges
    /// are the real spawn points. Deterministic layer/object order (same as [`find_pop_range`]).
    pub fn find_first_pc_pop_range(&self) -> Option<&InstanceObject> {
        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    if let LayerEntryData::PopRange(pop_range) = &object.data
                        && pop_range.pop_type == PopType::PC
                    {
                        return Some(object);
                    }
                }
            }
        }

        None
    }

    pub fn to_lua_zone(&self, weather_id: u16) -> LuaZone {
        LuaZone {
            zone_id: self.id,
            weather_id,
            internal_name: self.internal_name.clone(),
            region_name: self.region_name.clone(),
            place_name: self.place_name.clone(),
            intended_use: self.intended_use,
            map_id: self.map_id,
            cached_npc_base_ids: self.cached_npc_base_ids.clone(),
            cached_eobj_base_ids: self.cached_eobj_base_ids.clone(),
            ..Default::default()
        }
    }

    fn find_entrance_from_base_id(&self, base_id: u32) -> Option<&InstanceObject> {
        // First, we need to find the EventObject for the entrance:
        let mut bound_id = None;
        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    if let LayerEntryData::EventObject(eobj) = &object.data
                        && eobj.parent_data.base_id == base_id
                    {
                        bound_id = Some(eobj.bound_instance_id);
                        break;
                    }
                }
            }
        }

        bound_id?;

        // Then find the linked instance object, which is usually a SGB.
        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    if object.instance_id == bound_id.unwrap() {
                        return Some(object);
                    }
                }
            }
        }

        None
    }

    /// Tries to locate the entrance circle used in instanced content.
    pub fn find_entrance(&self) -> Option<&InstanceObject> {
        for base_id in ENTRANCE_CIRCLE_IDS {
            if let Some(object) = self.find_entrance_from_base_id(base_id) {
                return Some(object);
            }
        }

        None
    }

    /// Returns a list of event objects to spawn by default. If `explorer_mode`, replaces the shortcut object.
    ///
    /// For example, the Gold Saucer arcade machines or shortcuts in dungeons.
    pub fn get_event_objects(
        &mut self,
        game_data: &mut GameData,
        explorer_mode: bool,
    ) -> Vec<(SpawnObject, String)> {
        let mut object_spawns = Vec::new();

        for layer_group in &self.layer_groups {
            for layer in &layer_group.chunks[0].layers {
                if !layer.header.has_layer_set(self.layer_set as u32) {
                    continue;
                }

                for object in &layer.objects {
                    let (_, rotation, translation) =
                        Affine3A::from(object.transform).to_scale_rotation_translation();

                    if let LayerEntryData::EventObject(eobj) = &object.data {
                        let not_targetable = if let Some(event_type) = HandlerType::from_repr(
                            game_data.get_eobj_data(eobj.parent_data.base_id) >> 16,
                        ) && matches!(
                            event_type,
                            HandlerType::Invalid | HandlerType::GimmickRect
                        ) {
                            true
                        } else {
                            false // make it selectable to be on the safe side.
                        };

                        let base_id = if eobj.parent_data.base_id == EOBJ_SHORTCUT && explorer_mode
                        {
                            EOBJ_SHORTCUT_EXPLORER_MODE
                        } else {
                            eobj.parent_data.base_id
                        };

                        // Hide shortcuts and exits, these will be spawned by the director.
                        let event_state = if eobj.parent_data.base_id == EOBJ_SHORTCUT
                            || eobj.parent_data.base_id == EOBJ_EXIT
                        {
                            EventState::OFF | EventState::UNK2 | EventState::UNK3
                        } else {
                            EventState::empty()
                        };

                        let spawn = SpawnObject {
                            kind: ObjectKind::EventObj,
                            base_id,
                            not_targetable,
                            event_state,
                            entity_id: ObjectId(fastrand::u32(..)),
                            layout_id: object.instance_id,
                            bind_layout_id: eobj.bound_instance_id,
                            radius: 1.0,
                            rotation: euler_to_direction(rotation.to_euler(EulerRot::XYZ)),
                            position: Position(translation),
                            ..Default::default()
                        };
                        self.cached_objects.insert(eobj.parent_data.base_id, spawn);
                        self.cached_eobj_base_ids
                            .insert(spawn.entity_id, spawn.base_id);

                        if game_data.get_eobj_pop_type(eobj.parent_data.base_id) == 1 {
                            object_spawns.push((spawn, layer.header.name.value.clone()));
                        }
                    }

                    if let LayerEntryData::Treasure(treasure) = &object.data {
                        // physis nested Treasure under GameObjectInstanceObject (parent_data).
                        self.cached_treasure.insert(
                            treasure.parent_data.base_id,
                            SpawnTreasure {
                                base_id: treasure.parent_data.base_id,
                                entity_id: ObjectId(fastrand::u32(..)),
                                layout_id: object.instance_id,
                                rotation: euler_to_direction(rotation.to_euler(EulerRot::XYZ)),
                                position: Position(translation),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }

        // Only dropins are checked for gathering points, because they strip that from retail LGBs.
        for layer in &self.dropin_layers {
            for object in &layer.objects {
                if let DropInObjectData::GatheringPoint { base_id } = object.data {
                    let spawn = SpawnObject {
                        kind: ObjectKind::GatheringPoint,
                        base_id,
                        entity_id: ObjectId(fastrand::u32(..)),
                        layout_id: object.layout_id,
                        radius: 1.0,
                        // Only the last value is needed to spawn the node.
                        // First value is remaining count I believe, but it's immediately overwriten by an ActorControl so I don't see the point in setting it here?
                        // Third value might be index?
                        // If it's >3 then the node doesn't seme to spawn.
                        args1: u32::from_le_bytes([0, 0, 0, 1]),
                        position: object.position,
                        ..Default::default()
                    };
                    self.cached_objects.insert(base_id, spawn);
                    object_spawns.push((spawn, String::default()));
                }
            }
        }

        // housing plot entrances
        for (i, plot) in self.cached_housing_plots.iter().enumerate() {
            let spawn = SpawnObject {
                kind: ObjectKind::EventObj,
                base_id: EOBJ_HOUSING_ENTRANCE,
                entity_id: ObjectId(fastrand::u32(..)),
                radius: 1.0,
                position: Position(plot.entrance_position),
                args2: u32::from_le_bytes([0, i as u8, 0, 0]),
                ..Default::default()
            };
            object_spawns.push((spawn, String::default()));
        }

        object_spawns
    }

    /// Returns an SpawnObject for the given base ID.
    pub fn get_event_object(&self, base_id: u32) -> Option<SpawnObject> {
        self.cached_objects.get(&base_id).cloned()
    }

    /// Returns an SpawnNpc for the given instance ID.
    pub fn get_battle_npc(&self, instance_id: u32) -> Option<SpawnNpc> {
        self.cached_npcs.get(&instance_id).cloned()
    }

    /// Returns a cached BNpc template matching the given ids, preferring an exact base/name pair.
    pub fn find_battle_npc_template(&self, base_id: u32, name_id: u32) -> Option<SpawnNpc> {
        self.cached_npcs
            .values()
            .find(|spawn| spawn.common.base_id == base_id && spawn.common.name_id == name_id)
            .cloned()
            .or_else(|| {
                self.cached_npcs
                    .values()
                    .find(|spawn| spawn.common.base_id == base_id)
                    .cloned()
            })
    }

    /// Returns a SpawnTreasure for the given base ID.
    pub fn get_treasure(&self, base_id: u32) -> Option<SpawnTreasure> {
        self.cached_treasure.get(&base_id).cloned()
    }

    /// Returns a list of battle NPCs to spawn.
    pub fn get_npcs(&mut self, game_data: &mut GameData) -> Vec<SpawnNpc> {
        let mut npc_spawns = Vec::new();

        // Only dropins are checked for battle npcs, because they strip that from retail LGBs.
        for layer in &self.dropin_layers {
            for object in &layer.objects {
                if let DropInObjectData::BattleNpc {
                    base_id,
                    name_id,
                    hp,
                    level,
                    nonpop,
                    hostile,
                    gimmick_id,
                    max_links,
                    link_family,
                    link_range,
                } = object.data
                {
                    let (model_chara, battalion, customize, rank, equip) =
                        game_data.find_bnpc(base_id).unwrap();

                    let usable_hp;
                    if let Some(hp) = hp {
                        usable_hp = hp;
                    } else {
                        let modifiers = game_data
                            .get_class_job_modifiers(0)
                            .expect("Failed to read param grow");

                        let attributes = game_data
                            .get_racial_base_attributes(0)
                            .expect("Failed to read racial attributes");

                        let param_grow = game_data
                            .get_param_grow(level)
                            .expect("Failed to read param grow");

                        let mut base_parameters = BaseParameters::default();
                        base_parameters.calculate_based_on_level(
                            &attributes,
                            level,
                            0,
                            &param_grow,
                            &modifiers,
                        );
                        base_parameters.calculate_potencies(level, &param_grow, None); // TODO: If NPCs have classjob modifiers and such, change that None!

                        usable_hp = base_parameters.hp;
                    }

                    let spawn = SpawnNpc {
                        gimmick_id,
                        character_data_flags: if hostile {
                            CharacterDataFlag::HOSTILE
                        } else {
                            CharacterDataFlag::empty()
                        },
                        character_data_icon: rank,
                        max_links,
                        link_family,
                        link_range,
                        common: CommonSpawn {
                            base_id,
                            name_id,
                            max_health_points: usable_hp,
                            health_points: usable_hp,
                            model_chara,
                            object_kind: ObjectKind::BattleNpc(BattleNpcSubKind::Enemy),
                            battalion,
                            level: level as u8,
                            position: object.position,
                            rotation: object.rotation,
                            look: customize,
                            layout_id: object.layout_id,
                            ..game_data.get_npc_equip(equip as u32).unwrap_or_default()
                        },
                        ..Default::default()
                    };

                    self.cached_npcs.insert(object.layout_id, spawn.clone());
                    if !nonpop {
                        npc_spawns.push(spawn);
                    }
                }
                if let DropInObjectData::EventNpc { base_id } = object.data {
                    let (model_chara, customize, equip) = game_data.find_enpc(base_id).unwrap();

                    let spawn = SpawnNpc {
                        common: CommonSpawn {
                            base_id,
                            name_id: base_id,
                            model_chara,
                            object_kind: ObjectKind::EventNpc,
                            position: object.position,
                            rotation: object.rotation,
                            look: customize,
                            layout_id: object.layout_id,
                            ..game_data.get_npc_equip(equip as u32).unwrap_or_default()
                        },
                        ..Default::default()
                    };

                    self.cached_npcs.insert(object.layout_id, spawn.clone());
                    npc_spawns.push(spawn);
                }
            }
        }

        npc_spawns
    }

    /// Returns a list of MapRanges that overlap this position.
    pub fn get_overlapping_map_ranges(&self, position: Vec3) -> Vec<&MapRange> {
        let mut overlapping = Vec::new();

        for map_range in &self.map_ranges {
            match map_range.trigger_box_shape {
                TriggerBoxShape::Box => {
                    // TODO: support oriented boxes (this is used by sanctuary boundaries, for some reason)
                    let min_x = map_range.position.x - (map_range.scale[0]);
                    let max_x = map_range.position.x + (map_range.scale[0]);

                    let min_y = map_range.position.y - (map_range.scale[1]);
                    let max_y = map_range.position.y + (map_range.scale[1]);

                    let min_z = map_range.position.z - (map_range.scale[2]);
                    let max_z = map_range.position.z + (map_range.scale[2]);

                    if position.x >= min_x
                        && position.x <= max_x
                        && position.y >= min_y
                        && position.y <= max_y
                        && position.z >= min_z
                        && position.z <= max_z
                    {
                        overlapping.push(map_range);
                    }
                }
                TriggerBoxShape::Cylinder => {
                    // TODO: support arbitrarily-rotated cylinders
                    let length = map_range.scale[1] * 2.0;
                    let length_sq = f32::powi(length, 2);

                    let pt1 = Vec3 {
                        x: map_range.position.x,
                        y: map_range.position.y - map_range.scale[1],
                        z: map_range.position.z,
                    };
                    let pt2 = Vec3 {
                        x: map_range.position.x,
                        y: map_range.position.y + map_range.scale[1],
                        z: map_range.position.z,
                    };

                    let radius = map_range.scale[0]; // TODO: support individual radii (if that's even a thing, assert please)
                    let radius_sq = f32::powi(radius, 2);

                    if Self::cylinder_test(pt1, pt2, length_sq, radius_sq, position) != -1.0 {
                        overlapping.push(map_range);
                    }
                }
                _ => {} // TODO: support other box shapes
            }
        }

        overlapping
    }

    // From https://www.flipcode.com/archives/Fast_Point-In-Cylinder_Test.shtml
    fn cylinder_test(pt1: Vec3, pt2: Vec3, length_sq: f32, radius_sq: f32, test_pt: Vec3) -> f32 {
        let dx = pt2.x - pt1.x;
        let dy = pt2.y - pt1.y;
        let dz = pt2.z - pt1.z;

        let pdx = test_pt.x - pt1.x;
        let pdy = test_pt.y - pt1.y;
        let pdz = test_pt.z - pt1.z;

        let dot = pdx * dx + pdy * dy + pdz * dz;
        if dot < 0.0 || dot > length_sq {
            -1.0
        } else {
            let dsq = (pdx * pdx + pdy * pdy + pdz * pdz) - dot * dot / length_sq;

            if dsq > radius_sq { -1.0 } else { dsq }
        }
    }
}

fn begin_change_zone<'a>(
    data: &'a mut WorldServer,
    network: &mut NetworkState,
    game_data: &mut GameData,
    destination_zone_id: Option<u16>,
    actor_id: ObjectId,
    warp_type: WarpType,
    param4: u8,
    hide_character: u8,
    unk1: u8,
) -> (&'a mut Instance, bool) {
    if let Some(destination_zone_id) = destination_zone_id {
        let mut needs_init_zone = false;

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::PrepareZoning {
            target_zone: destination_zone_id,
            warp_type,
            fade_out_time: 1,
            log_message: 0,
            animation: 0,
            param4,
            hide_character,
            param_7: 0,
            unk1,
            unk2: 0,
        });

        network.send_to_by_actor_id(
            actor_id,
            FromServer::PacketSegment(ipc, actor_id),
            DestinationNetwork::ZoneClients,
        );

        // Carry the player's combat state (job gauge, cooldowns, summoned-pet flag) across the zone
        // change. Retail keeps the gauge and pet when you change maps; without this they'd reset,
        // because the destination instance gets a brand-new actor with default state.
        let mut carried_combat_state = None;

        // inform the players in this zone that this actor left
        if let Some(current_instance) = data.find_actor_instance_mut(actor_id) {
            if current_instance.zone.id != destination_zone_id {
                // Cross-zone warp: fully despawn the actor for everyone in the old zone.
                carried_combat_state =
                    take_combat_state_and_despawn_pets(current_instance, network, actor_id);

                network.remove_actor(current_instance, actor_id);
                needs_init_zone = true;
            } else {
                // Same-zone warp: the actor must stay spawned (retail keeps it — the party list
                // HP/MP bars never clear). Instead of removing it, play the teleport-out vanish
                // effect on it for *observers* so their copy fades away here rather than sliding to
                // the destination. The warping player themselves fades via PrepareZoning (above) and
                // must not receive this. Observers are re-shown at the destination later by ZoneIn
                // (triggered when the warping client sends FinishZoning). Retail sends this at
                // teleport start, alongside the teleporter's PrepareZoning.
                network.send_in_range_instance(
                    actor_id,
                    current_instance,
                    FromServer::ActorControl(
                        actor_id,
                        ActorControlCategory::ActorDespawnEffect {
                            warp_mode: 1,
                            animation: 0,
                        },
                    ),
                    DestinationNetwork::ZoneClients,
                );
            }
        }

        // then find or create a new instance with the zone id
        let instance = data.ensure_exists(destination_zone_id, game_data);
        // Insert an empty actor that will be filled later
        instance.insert_empty_actor(actor_id);

        // Restore the carried combat state onto the freshly-inserted actor. ZoneLoaded later clones
        // this when it builds the real player spawn, so the gauge/cooldowns survive the move.
        restore_carried_combat_state(instance, actor_id, carried_combat_state);

        (instance, needs_init_zone)
    } else {
        let instance = data.find_actor_instance_mut(actor_id).unwrap();

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::PrepareZoning {
            target_zone: instance.zone.id,
            warp_type,
            fade_out_time: 1,
            log_message: 0,
            animation: 0,
            param4,
            hide_character,
            param_7: 0,
            unk1,
            unk2: 0,
        });

        network.send_to_by_actor_id(
            actor_id,
            FromServer::PacketSegment(ipc, actor_id),
            DestinationNetwork::ZoneClients,
        );

        (instance, false)
    }
}

/// Sends the needed information to ZoneConnection for a zone change.
/// Take the player's combat state (job gauge, cooldowns, summoned-pet flag) out of their current
/// instance and despawn any pet actors they own there. Returns the cloned combat state so it can be
/// restored onto the freshly-inserted actor in the destination instance via
/// [`restore_carried_combat_state`]. Mirrors what [`begin_change_zone`] does inline, but is reusable
/// by the content (duty) transition paths that build the destination instance themselves.
pub fn take_combat_state_and_despawn_pets(
    current_instance: &mut Instance,
    network: &mut NetworkState,
    actor_id: ObjectId,
) -> Option<PlayerCombatState> {
    let mut carried_combat_state =
        if let Some(NetworkedActor::Player { combat_state, .. }) =
            current_instance.find_actor(actor_id)
        {
            Some(combat_state.clone())
        } else {
            None
        };

    // Snapshot the first pet this player owns so it can be re-instated with the SAME object id in
    // the destination (no re-summon / birth animation — see `reinstate_carried_pet`). Position is
    // not carried; it is recomputed beside the owner's new position at the destination.
    if let Some(state) = carried_combat_state.as_mut() {
        // Don't carry a demi/primal actor across a zone. Retail-confirmed (demi换区.log): a demi
        // (e.g. Solar Bahamut) is dismissed on zone-out and the destination spawns a FRESH Carbuncle
        // with the summoner gauge reset — the demi never persists. Leaving `carried_pet` None here
        // makes ZoneLoaded fall back to the fresh-summon Carbuncle path, matching that revert.
        // (Follow-up: retail fades the new Carbuncle in via cat267 rather than a cat36 birth reveal,
        // and clears the gauge demi/Aetherflow/arcanum/attunement state — tracked separately.)
        let carrying_demi_or_primal = state.summoner.demi_expires_at.is_some()
            || state.summoner.primal_summon_expires_at.is_some();
        if !carrying_demi_or_primal {
            for (id, actor) in &current_instance.actors {
                if let NetworkedActor::Npc {
                    state: npc_state,
                    spawn,
                    ..
                } = actor
                    && spawn.common.owner_id == actor_id
                    // Skip a Dead owned pet: after a summon/demi cast a fading Dead carbuncle can
                    // coexist with the live pet, and HashMap order could otherwise pick it.
                    && *npc_state != NpcState::Dead
                {
                    state.summoner.carried_pet = Some(CarriedPet {
                        actor_id: *id,
                        spawn: spawn.clone(),
                    });
                    break;
                }
            }
        }
    }

    // Fade the pet out + park it at the source (SetPetEntityId + mount_state + cat266 + Targetable(0)),
    // mirroring the mount park sequence, before it is removed from the old instance. Observers in the
    // old zone see it fade and vanish; they don't follow to the destination.
    crate::server::jobs::summoner::sync_pet_for_mount(network, current_instance, actor_id);

    // Despawn this player's pet(s) in the old instance so they don't linger orphaned; the pet is
    // re-instated (carried id) or re-summoned in the destination once the player has loaded (see
    // ZoneLoaded).
    let pet_ids: Vec<ObjectId> = current_instance
        .actors
        .iter()
        .filter_map(|(id, actor)| match actor {
            NetworkedActor::Npc { spawn, .. } if spawn.common.owner_id == actor_id => Some(*id),
            _ => None,
        })
        .collect();
    for pet_id in pet_ids {
        network.remove_actor(current_instance, pet_id);
    }

    carried_combat_state
}

/// Restore combat state previously taken by [`take_combat_state_and_despawn_pets`] onto the
/// (freshly-inserted, default-state) player actor in the destination instance. ZoneLoaded later
/// clones this when it builds the real player spawn, so the gauge/cooldowns/pet survive the move.
pub fn restore_carried_combat_state(
    target_instance: &mut Instance,
    actor_id: ObjectId,
    carried_combat_state: Option<PlayerCombatState>,
) {
    if let Some(state) = carried_combat_state
        && let Some(NetworkedActor::Player { combat_state, .. }) =
            target_instance.find_actor_mut(actor_id)
    {
        *combat_state = state;
    }
}

/// Picks a random position out of a pop range, so that everyone arriving at the same aetheryte
/// doesn't pile up on one spot. Most pop ranges offer several positions; the object's own
/// translation is the fallback when it offers none.
fn pick_point_in_pop_range(object: &InstanceObject) -> Position {
    let base = Position(glam::Vec3::from_array(object.transform.translation));

    let LayerEntryData::PopRange(pop_range) = &object.data else {
        return base;
    };

    let Some(offset) = fastrand::choice(&pop_range.positions) else {
        return base;
    };

    Position(glam::Vec3::from_array(object.transform.translation) + glam::Vec3::from_slice(offset))
}

pub fn change_zone_warp_to_pop_range(
    data: &mut WorldServer,
    network: &mut NetworkState,
    game_data: &mut GameData,
    destination_zone_id: Option<u16>,
    destination_instance_id: u32,
    actor_id: ObjectId,
    from_id: ClientId,
    warp_type: WarpType,
    param4: u8,
    hide_character: u8,
    unk1: u8,
) {
    let (target_instance, needs_init_zone) = begin_change_zone(
        data,
        network,
        game_data,
        destination_zone_id,
        actor_id,
        warp_type,
        param4,
        hide_character,
        unk1,
    );

    let exit_position;
    let exit_rotation;
    if let Some((destination_object, _)) =
        target_instance.zone.find_pop_range(destination_instance_id)
    {
        let (_, rotation, _) =
            Affine3A::from(destination_object.transform).to_scale_rotation_translation();
        exit_position = Some(pick_point_in_pop_range(destination_object));
        exit_rotation = Some(euler_to_direction(rotation.to_euler(EulerRot::XYZ)));
    } else {
        tracing::warn!(
            "Failed to find pop range {destination_instance_id} in zone {}",
            target_instance.zone.id
        );
        exit_position = None;
        exit_rotation = None;
    }

    do_change_zone(
        network,
        target_instance,
        needs_init_zone,
        exit_position,
        exit_rotation,
        from_id,
        warp_type,
    );
}

/// Sends the needed information to ZoneConnection for a zone change.
pub fn change_zone_warp_to_entrance(
    network: &mut NetworkState,
    target_instance: &mut Instance,
    needs_init_zone: bool,
    from_id: ClientId,
) {
    // The player's facing on zone-in comes from the entrance EventRange (InstanceContent.EntranceRect),
    // which is flagged `entrance` when the instance is created. The entrance *circle* EObj/SGB carries
    // no rotation of its own, so we take the yaw from the rect. Mirrors Sapphire's
    // InstanceContent::movePlayerToEntrance: position from the entrance object, facing from the rect,
    // falling back to due-east (π/2) when no rect is present.
    let entrance_rect = target_instance
        .zone
        .map_ranges
        .iter()
        .find(|r| r.entrance)
        .map(|r| (Position(r.position), r.rotation));

    let exit_position;
    let exit_rotation;
    if let Some(destination_object) = target_instance.zone.find_entrance() {
        exit_position = Some(pick_point_in_pop_range(destination_object));
        exit_rotation = Some(entrance_rect.map_or(FRAC_PI_2, |(_, rot)| rot));
    } else if let Some((pos, rot)) = entrance_rect {
        exit_position = Some(pos);
        exit_rotation = Some(rot);
    } else if let Some(destination_object) = target_instance.zone.find_first_pc_pop_range() {
        // Last resort for instanced content with no entrance circle and no entrance rect (e.g.
        // InstanceContent.LGBEventRange == 0): warp to the first PC pop range, which is a real
        // spawn point. Without this the player warps to (0,0,0) and hangs on infinite loading.
        let (_, rotation, _) =
            Affine3A::from(destination_object.transform).to_scale_rotation_translation();
        tracing::info!(
            "No entrance circle or rect; falling back to first PC pop range in zone {}",
            target_instance.zone.id
        );
        exit_position = Some(pick_point_in_pop_range(destination_object));
        exit_rotation = Some(euler_to_direction(rotation.to_euler(EulerRot::XYZ)));
    } else {
        tracing::warn!(
            "Failed to find instanced content entrance?! This is a bug in Kawari, please report it!"
        );
        exit_position = None;
        exit_rotation = None;
    }

    tracing::info!(
        "Instance entrance: position={:?} rotation={:?} (rect_found={})",
        exit_position,
        exit_rotation,
        entrance_rect.is_some()
    );

    do_change_zone(
        network,
        target_instance,
        needs_init_zone,
        exit_position,
        exit_rotation,
        from_id,
        WarpType::Normal,
    );
}

/// Teleports one player to another.
pub fn change_zone_to_player(
    network: &mut NetworkState,
    data: &mut WorldServer,
    game_data: &mut GameData,
    from_id: ClientId,
    to_actor_id: ObjectId,
) {
    let destination_zone_id;
    {
        let Some(target_instance) = data.find_actor_instance(to_actor_id) else {
            return;
        };

        destination_zone_id = target_instance.zone.id;
    }

    let from_actor_id = network.clients.get(&from_id).unwrap().0.actor_id;

    let (target_instance, needs_init_zone) = begin_change_zone(
        data,
        network,
        game_data,
        Some(destination_zone_id),
        from_actor_id,
        WarpType::Normal,
        0,
        0,
        0,
    );

    let Some(target_actor) = target_instance.find_actor(to_actor_id) else {
        return;
    };

    do_change_zone(
        network,
        target_instance,
        needs_init_zone,
        Some(target_actor.position()),
        Some(target_actor.rotation()),
        from_id,
        WarpType::Normal,
    );
}

/// Sends the needed information to ZoneConnection for a zone change.
fn do_change_zone(
    network: &mut NetworkState,
    target_instance: &mut Instance,
    needs_init_zone: bool,
    exit_position: Option<Position>,
    exit_rotation: Option<f32>,
    from_id: ClientId,
    warp_type: WarpType,
) {
    let actor_id = network.clients.get(&from_id).unwrap().0.actor_id;
    let state = network.get_state_mut(from_id).unwrap();
    let (exit_position, exit_rotation) = if exit_position.is_some() {
        (exit_position, exit_rotation)
    } else if let Some(destination_object) = target_instance.zone.find_entrance() {
        let (_, rotation, translation) =
            Affine3A::from(destination_object.transform).to_scale_rotation_translation();
        (
            Some(Position(translation)),
            Some(euler_to_direction(rotation.to_euler(EulerRot::XYZ))),
        )
    } else if let Some(destination_object) = target_instance.zone.find_first_pc_pop_range() {
        // Last resort for instanced content with no entrance circle and no entrance rect (e.g.
        // InstanceContent.LGBEventRange == 0): the first PC pop range is a real spawn point,
        // which avoids warping the player to (0,0,0) and hanging on infinite loading.
        let (_, rotation, translation) =
            Affine3A::from(destination_object.transform).to_scale_rotation_translation();
        tracing::info!(
            "No entrance circle or rect; falling back to first PC pop range in zone {}",
            target_instance.zone.id
        );
        (
            Some(Position(translation)),
            Some(euler_to_direction(rotation.to_euler(EulerRot::XYZ))),
        )
    } else {
        (exit_position, exit_rotation)
    };

    if needs_init_zone {
        // Clear spawn pools
        state.actor_allocator.clear();
        state.object_allocator.clear();

        let director_vars = target_instance
            .director
            .as_ref()
            .map(|director| director.build_var_segment());

        // now that we have all of the data needed, inform the connection of where they need to be
        let msg = FromServer::ChangeZone(
            target_instance.zone.id,
            target_instance.content_finder_condition_id,
            target_instance.weather_id,
            exit_position.unwrap_or_default(),
            exit_rotation.unwrap_or_default(),
            target_instance.zone.to_lua_zone(target_instance.weather_id),
            false,
            director_vars,
        );
        network.send_to(from_id, msg, DestinationNetwork::ZoneClients);
    } else {
        // Same-zone warp: no re-init/respawn. Relocate the actor in place. We delay this to give
        // the client time to fade out (the warping player) / finish the vanish effect (observers).
        let segment = ServerZoneIpcSegment::new(ServerZoneIpcData::ActorSetPos(ActorSetPos {
            position: exit_position.unwrap_or_default(),
            rotation: exit_rotation.unwrap_or_default(),
            warp_type,
            warp_type_arg: 2, // unknown
            ..Default::default()
        }));
        // Snap the warping player themselves.
        target_instance.insert_task(
            from_id,
            actor_id,
            WARP_DELAY,
            QueuedTaskData::PacketSegment {
                segment: segment.clone(),
            },
        );
        // Snap the actor for everyone else in the zone too, so their (currently faded-out) copy is
        // repositioned to the destination instead of lerping across the map once ZoneIn re-shows it.
        target_instance.insert_task(
            from_id,
            actor_id,
            WARP_DELAY,
            QueuedTaskData::BroadcastPacketSegment { segment },
        );
    }
}

/// Process zone-related messages.
pub fn handle_zone_messages(
    data: Arc<Mutex<WorldServer>>,
    network: Arc<Mutex<NetworkState>>,
    game_data: Arc<Mutex<GameData>>,
    msg: &ToServer,
) -> bool {
    match msg {
        ToServer::ZoneLoaded(from_id, from_actor_id, player_spawn) => {
            tracing::info!(
                "Client {from_id:?} has now loaded into the zone, sending them existing player data."
            );

            let mut data = data.lock();

            // replace the connection's actor in the table
            let instance = data.find_actor_instance_mut(*from_actor_id).unwrap();
            let (
                status_effects,
                teleport_query,
                distance_range,
                conditions,
                executing_gimmick_jump,
                inside_instance_exit,
                parameters,
                dueling_opponent_id,
                remove_cooldowns,
                mut combat_state,
                last_combo_action,
                combo_sequence,
                hated_by,
            ) = match instance.find_actor_mut(*from_actor_id).unwrap() {
                NetworkedActor::Player {
                    status_effects,
                    teleport_query,
                    distance_range,
                    conditions,
                    executing_gimmick_jump,
                    inside_instance_exit,
                    parameters,
                    dueling_opponent_id,
                    remove_cooldowns,
                    combat_state,
                    last_combo_action,
                    combo_sequence,
                    hated_by,
                    ..
                } => (
                    status_effects.clone(),
                    teleport_query.clone(),
                    *distance_range,
                    *conditions,
                    *executing_gimmick_jump,
                    *inside_instance_exit,
                    parameters.clone(),
                    *dueling_opponent_id,
                    *remove_cooldowns,
                    combat_state.clone(),
                    *last_combo_action,
                    *combo_sequence,
                    hated_by.clone(),
                ),
                _ => unreachable!(),
            };

            // Read what we need from the carried combat_state before it's moved into the actor:
            // whether a pet was summoned, the pet snapshot to re-instate (if any), and (for jobs
            // with one) the job-gauge bytes to re-send so the gauge shows immediately instead of
            // staying blank until the next action.
            let had_pet = combat_state.summoner.carbuncle_summoned;
            let carried_pet = combat_state.summoner.carried_pet.take();
            // A demi/primal is deliberately not carried across a zone (see
            // `take_combat_state_and_despawn_pets`), so `carried_pet` is None but the demi/primal
            // timers are still set in the carried state. Detect that mid-demi transition here: retail
            // drops the demi, resets the gauge (SummonTimer/ReturnSummon/arcanum/Aetherflow cleared,
            // next-demi bit kept) and fades a FRESH carbuncle in. Reset the state BEFORE the gauge is
            // built below so the re-sent gauge already reflects the reset.
            let was_mid_demi = carried_pet.is_none()
                && (combat_state.summoner.demi_expires_at.is_some()
                    || combat_state.summoner.primal_summon_expires_at.is_some());
            if was_mid_demi {
                crate::server::jobs::summoner::reset_summoner_state_for_demi_zone(
                    &mut combat_state.summoner,
                );
            }
            let class_job = player_spawn.common.class_job;
            let gauge_data = job_for(class_job).and_then(|job| {
                job.build_gauge_data(&combat_state, player_spawn.common.level)
                    .map(|data| (job.gauge_class_job_id(class_job), data))
            });

            *instance.find_actor_mut(*from_actor_id).unwrap() = NetworkedActor::Player {
                spawn: player_spawn.clone(),
                status_effects,
                teleport_query,
                distance_range,
                conditions,
                executing_gimmick_jump,
                inside_instance_exit,
                parameters,
                dueling_opponent_id,
                remove_cooldowns,
                combat_state,
                last_combo_action,
                combo_sequence,
                hated_by,
                // Reset on zone change — no enmity carries into the new instance.
                last_enmity_sent: Vec::new(),
            };

            // Now that the player actor exists at its new position, restore their pet beside them —
            // unless one is already present (a same-zone reload won't have despawned it). Prefer
            // re-instating the carried pet (same object id, fade-in, NO birth animation); only fall
            // back to a fresh summon (birth animation) when nothing was carried across.
            let pet_already_present = instance.actors.values().any(|actor| {
                matches!(
                    actor,
                    NetworkedActor::Npc { spawn, .. }
                        if spawn.common.owner_id == *from_actor_id
                )
            });
            if !pet_already_present
                && let Some(actors) = job_for(class_job).and_then(Job::persistent_actors)
            {
                if let Some(carried) = carried_pet {
                    actors.reinstate_carried_pet(
                        network.clone(),
                        instance,
                        *from_actor_id,
                        carried,
                    );
                } else if was_mid_demi {
                    // Demi/primal dropped by the zone: fade a fresh carbuncle in (cat267), no birth
                    // reveal. The gauge state was already reset above.
                    actors.reinstate_carbuncle_after_demi_zone(
                        network.clone(),
                        instance,
                        *from_actor_id,
                    );
                } else if had_pet {
                    actors.apply_summon_pet_effect(network.clone(), instance, *from_actor_id);
                }
            }

            // Re-send the job gauge so the carried-over state shows immediately (the zone-in setup
            // sends a blank gauge, which would otherwise leave it empty until the next action).
            if let Some((classjob_id, data)) = gauge_data {
                let ipc =
                    ServerZoneIpcSegment::new(ServerZoneIpcData::ActorGauge { classjob_id, data });
                let mut network = network.lock();
                network.send_to_by_actor_id(
                    *from_actor_id,
                    FromServer::PacketSegment(ipc, *from_actor_id),
                    DestinationNetwork::ZoneClients,
                );
            }

            true
        }
        ToServer::ChangeZone(
            from_id,
            actor_id,
            zone_id,
            new_position,
            new_rotation,
            warp_type_info,
        ) => {
            tracing::info!("{from_id:?} is requesting to go to zone {zone_id}");

            let mut data = data.lock();
            let mut network = network.lock();
            let mut game_data = game_data.lock();

            let (warp_type, param4, hide_character, unk1) =
                if let Some((w_type, param, hide, unk)) = warp_type_info {
                    (*w_type, *param, *hide, *unk)
                } else {
                    (WarpType::Normal, 0, 0, 0)
                };

            let (target_instance, needs_init_zone) = begin_change_zone(
                &mut data,
                &mut network,
                &mut game_data,
                Some(*zone_id),
                *actor_id,
                warp_type,
                param4,
                hide_character,
                unk1,
            );
            do_change_zone(
                &mut network,
                target_instance,
                needs_init_zone,
                *new_position,
                *new_rotation,
                *from_id,
                warp_type,
            );

            true
        }
        ToServer::EnterZoneJump(from_id, actor_id, exitbox_id, warp_type_info) => {
            let mut data = data.lock();
            let mut network = network.lock();

            // first, find the zone jump in the current zone
            let mut destination_zone_id;
            let destination_instance_id;
            if let Some(current_instance) = data.find_actor_instance(*actor_id) {
                let Some((_, new_exit_box)) = current_instance.zone.find_exit_box(*exitbox_id)
                else {
                    tracing::warn!("Couldn't find exit box {exitbox_id}?!");
                    return true;
                };
                destination_zone_id = new_exit_box.territory_type;

                // Seen when attempting to enter underwater portals in Ruby Sea
                if new_exit_box.territory_type == 0
                    && new_exit_box.zone_id == 0
                    && new_exit_box.exit_type == physis::layer::ExitType::Invisible
                {
                    destination_zone_id = current_instance.zone.id;
                }

                destination_instance_id = new_exit_box.destination_instance_id;
            } else {
                tracing::warn!("Actor isn't in the instance it was expected in. This is a bug!");
                return true;
            }

            let (warp_type, param4, hide_character, unk1) =
                if let Some((w_type, param, hide, unk)) = warp_type_info {
                    (*w_type, *param, *hide, *unk)
                } else {
                    (WarpType::Normal, 0, 0, 0)
                };

            let mut game_data = game_data.lock();
            change_zone_warp_to_pop_range(
                &mut data,
                &mut network,
                &mut game_data,
                Some(destination_zone_id),
                destination_instance_id,
                *actor_id,
                *from_id,
                warp_type,
                param4,
                hide_character,
                unk1,
            );

            true
        }
        ToServer::Warp(from_id, actor_id, warp_id) => {
            let mut data = data.lock();
            let mut network = network.lock();
            let mut game_data = game_data.lock();

            // first, find the warp and it's destination
            let (destination_instance_id, destination_zone_id) = game_data
                .get_warp(*warp_id)
                .expect("Failed to find the warp!");

            change_zone_warp_to_pop_range(
                &mut data,
                &mut network,
                &mut game_data,
                Some(destination_zone_id),
                destination_instance_id,
                *actor_id,
                *from_id,
                WarpType::Normal,
                0,
                0,
                0,
            );

            true
        }
        ToServer::WarpAetheryte(from_id, actor_id, aetheryte_id, housing_aethernet) => {
            let mut data = data.lock();
            let mut network = network.lock();
            let mut game_data = game_data.lock();

            // first, find the warp and it's destination
            let (destination_instance_id, destination_zone_id) = game_data
                .get_aetheryte(*aetheryte_id, *housing_aethernet)
                .expect("Failed to find the aetheryte!");

            // Aetheryte teleports use WarpType::Teleport (4), NOT Normal (1). The client echoes this
            // in PrepareZoning/ActorSetPos/FinishZoning and it selects the teleport-specific arrival
            // transition; with Normal, the teleport-out animation never gets cleared and the caster
            // stays stuck in the teleport pose (for themselves and for observers of the broadcast
            // ActorSetPos). Matches retail captures.
            change_zone_warp_to_pop_range(
                &mut data,
                &mut network,
                &mut game_data,
                Some(destination_zone_id),
                destination_instance_id,
                *actor_id,
                *from_id,
                WarpType::Teleport,
                0,
                0,
                0,
            );

            true
        }
        ToServer::WarpPopRange(from_id, from_actor_id, territory_id, pop_range_id) => {
            let mut data = data.lock();
            let mut network = network.lock();
            let mut game_data = game_data.lock();

            change_zone_warp_to_pop_range(
                &mut data,
                &mut network,
                &mut game_data,
                Some(*territory_id),
                *pop_range_id,
                *from_actor_id,
                *from_id,
                WarpType::Normal,
                0,
                0,
                0,
            );

            true
        }
        ToServer::ZoneIn(from_id, from_actor_id, is_teleport) => {
            tracing::info!("Player {from_id:?} has finally zoned in, informing other players...");

            // Inform all clients to play the zone in animation
            let mut data = data.lock();
            let mut network = network.lock();
            let mut to_remove = Vec::new();
            for (id, (handle, _)) in &mut network.clients {
                let id = *id;

                let category = ActorControlCategory::ZoneIn {
                    warp_finish_anim: 1,
                    raise_anim: 0,
                    unk1: if *is_teleport { 110 } else { 0 },
                };

                if id == *from_id {
                    let msg = FromServer::ActorControlSelf(category);

                    if handle.send(msg).is_err() {
                        to_remove.push(id);
                    }
                } else {
                    let msg = FromServer::ActorControl(*from_actor_id, category);

                    if handle.send(msg).is_err() {
                        to_remove.push(id);
                    }
                }
            }
            network.to_remove.append(&mut to_remove);

            // Then update the PlayerSpawn so respawning this player doesn't appear invisible again
            if let Some(instance) = data.find_actor_instance_mut(*from_actor_id)
                && let Some(actor) = instance.find_actor_mut(*from_actor_id)
            {
                actor
                    .get_common_spawn_mut()
                    .display_flags
                    .remove(DisplayFlag::INVISIBLE);
            }

            true
        }
        ToServer::MoveToPopRange(from_id, from_actor_id, id, fade_out) => {
            let zone_id;
            {
                let data = data.lock();
                let Some(instance) = data.find_actor_instance(*from_actor_id) else {
                    return false;
                };

                zone_id = instance.zone.id;
            }

            let mut data = data.lock();
            let mut network = network.lock();
            let mut game_data = game_data.lock();
            change_zone_warp_to_pop_range(
                &mut data,
                &mut network,
                &mut game_data,
                Some(zone_id),
                *id,
                *from_actor_id,
                *from_id,
                if *fade_out {
                    WarpType::Normal
                } else {
                    WarpType::None
                },
                0,
                0,
                0,
            );

            true
        }
        ToServer::NewLocationDiscovered(from_id, layout_id, _pos, zone_id) => {
            let data = data.lock();
            let mut network = network.lock();

            for instance in &data.instances {
                if instance.zone.id == *zone_id {
                    for range in &instance.zone.map_ranges {
                        if range.instance_id == *layout_id
                            && let Some(discovery_id) = range.discovery_id
                        {
                            // TODO: Check if the player is actually in this range?
                            // TODO: This is the "old" style of map discovery where every chunk is revealed one by one as the player runs into them. It's currently unclear how retail reveals the entire map at once. As an example, for North Shroud, retail sends map_part_id 164, which reveals its entire map. When we enter North Shroud from Old Gridania, Kawari currently sends 1.
                            let mut game_data = game_data.lock();
                            let Some(map_id) = game_data.get_territory_info_map_data(*zone_id)
                            else {
                                tracing::error!(
                                    "Unable to get Map column data from TerritoryInfo sheet for zone id {zone_id}"
                                );
                                return true;
                            };

                            let msg =
                                FromServer::LocationDiscovered(map_id.into(), discovery_id.into());
                            network.send_to(*from_id, msg, DestinationNetwork::ZoneClients);
                            return true;
                        }
                    }

                    // If we somehow didn't get any discoverable ranges, exit early. Is that even possible?
                    break;
                }
            }

            true
        }
        ToServer::PlaceFurniture(
            from_actor_id,
            container,
            slot,
            catalog_id,
            stain,
            position,
            indoors,
            rotation,
            plot_index,
        ) => {
            let data = data.lock();
            let mut network = network.lock();

            let Some(instance) = data.find_actor_instance(*from_actor_id) else {
                return true;
            };

            let msg = FromServer::FurniturePlaced(
                *container,
                *slot,
                *catalog_id,
                *stain,
                *position,
                *indoors,
                *rotation,
                *plot_index,
            );

            // We *do* want to include the sender here
            network.send_in_range_inclusive_instance(
                *from_actor_id,
                instance,
                msg,
                DestinationNetwork::ZoneClients,
            );

            true
        }
        ToServer::TranslateFurniture(
            from_actor_id,
            plot_info,
            slot,
            position,
            rotation,
            indoors,
        ) => {
            let data = data.lock();
            let mut network = network.lock();

            let Some(instance) = data.find_actor_instance(*from_actor_id) else {
                return true;
            };

            let msg =
                FromServer::FurnitureTranslated(*plot_info, *slot, *position, *rotation, *indoors);

            // We *don't* want to include the sender here
            network.send_in_range_instance(
                *from_actor_id,
                instance,
                msg,
                DestinationNetwork::ZoneClients,
            );

            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use super::*;
    use crate::StatusEffects;
    use crate::server::actor::NpcState;
    use kawari::common::Timeline;
    use physis::layer::{Layer, LayerHeader, Transformation};
    use physis::lgb::LayerChunk;

    /// Insert an owned live pet directly (bypassing `insert_npc`, which loads a timeline file from
    /// disk that isn't present in the unit-test environment).
    fn insert_owned_pet(instance: &mut Instance, id: ObjectId, owner: ObjectId) {
        insert_owned_pet_with_state(instance, id, owner, NpcState::Follow);
    }

    /// Like [`insert_owned_pet`] but with an explicit `NpcState`; used to plant a Dead owned pet.
    fn insert_owned_pet_with_state(
        instance: &mut Instance,
        id: ObjectId,
        owner: ObjectId,
        state: NpcState,
    ) {
        let mut spawn = SpawnNpc::default();
        spawn.common.owner_id = owner;
        if state == NpcState::Dead {
            spawn.common.health_points = 0;
        }
        instance.actors.insert(
            id,
            NetworkedActor::Npc {
                state,
                navmesh_path: VecDeque::default(),
                navmesh_path_lerp: 0.0,
                navmesh_target: None,
                last_position: None,
                spawn_position: spawn.common.position.0,
                spawn,
                timeline: Timeline {
                    autoattack_action_id: 0,
                    timeline_always_plays: false,
                    timepoints: Vec::new(),
                    on_death: Vec::new(),
                },
                timeline_position: 0,
                hate_list: HashMap::new(),
                currently_invulnerable: false,
                ai_paused: false,
                targetable: true,
                visible: true,
                cast_locked: false,
                status_effects: StatusEffects::default(),
            },
        );
    }

    /// A player with a pet out: the returned combat state carries the pet snapshot with the pet's
    /// map id and owning owner id, so the destination can re-instate it with the same object id.
    #[test]
    fn take_combat_state_carries_the_owned_pet() {
        let owner = ObjectId(1);
        let pet = ObjectId(2);
        let mut instance = Instance::default();
        let mut network = NetworkState::default();

        instance.insert_empty_actor(owner);
        insert_owned_pet(&mut instance, pet, owner);

        let carried = take_combat_state_and_despawn_pets(&mut instance, &mut network, owner)
            .expect("player exists so combat state is returned");
        let carried_pet = carried
            .summoner
            .carried_pet
            .expect("an owned pet must be carried");
        assert_eq!(carried_pet.actor_id, pet);
        assert_eq!(carried_pet.spawn.common.owner_id, owner);
    }

    /// A player with no pet out carries nothing.
    #[test]
    fn take_combat_state_carries_nothing_without_a_pet() {
        let owner = ObjectId(1);
        let mut instance = Instance::default();
        let mut network = NetworkState::default();

        instance.insert_empty_actor(owner);

        let carried = take_combat_state_and_despawn_pets(&mut instance, &mut network, owner)
            .expect("player exists so combat state is returned");
        assert!(carried.summoner.carried_pet.is_none());
    }

    /// A Dead owned pet (the fading carbuncle after a summon/demi cast) is never carried; with no
    /// live pet present the carry stays `None`.
    #[test]
    fn take_combat_state_does_not_carry_a_dead_pet() {
        let owner = ObjectId(1);
        let dead_pet = ObjectId(2);
        let mut instance = Instance::default();
        let mut network = NetworkState::default();

        instance.insert_empty_actor(owner);
        insert_owned_pet_with_state(&mut instance, dead_pet, owner, NpcState::Dead);

        let carried = take_combat_state_and_despawn_pets(&mut instance, &mut network, owner)
            .expect("player exists so combat state is returned");
        assert!(carried.summoner.carried_pet.is_none());
    }

    /// Builds a PopRange InstanceObject at `translation` with the given pop type. Mirrors the
    /// minimal shape `find_first_pc_pop_range` inspects (pop_type + object translation).
    fn make_pop_range(instance_id: u32, pop_type: PopType, translation: [f32; 3]) -> InstanceObject {
        InstanceObject {
            instance_id,
            transform: Transformation {
                translation,
                ..Default::default()
            },
            data: LayerEntryData::PopRange(PopRangeInstanceObject {
                pop_type,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Wraps `objects` in the single-chunk LGB layout `Zone` iterates over. A default `LayerHeader`
    /// reports `has_layer_set(_) == true` (referenced type `All`), matching `Zone::default()`'s
    /// `layer_set == 0`.
    fn zone_with_objects(objects: Vec<InstanceObject>) -> Zone {
        let layer = Layer {
            header: LayerHeader::default(),
            objects,
        };
        Zone {
            layer_groups: vec![Lgb {
                chunks: vec![LayerChunk {
                    layer_group_id: 0,
                    name: String::new(),
                    layers: vec![layer],
                }],
            }],
            ..Default::default()
        }
    }

    /// Instanced content with no entrance EObj and no `.entrance` map range (e.g. territory 1359
    /// "w1en", InstanceContent.LGBEventRange == 0) still carries PC PopRanges: the last-resort
    /// fallback resolves to the first PC PopRange with its real (non-origin) position, rather than
    /// warping the player to (0,0,0).
    #[test]
    fn find_first_pc_pop_range_returns_the_first_pc_spawn() {
        // A non-PC pop range first, so the filter has to skip it and pick the PC one behind it.
        let zone = zone_with_objects(vec![
            make_pop_range(10, PopType::Content, [1.0, 2.0, 3.0]),
            make_pop_range(11, PopType::PC, [4.0, 5.0, 6.0]),
        ]);

        let object = zone
            .find_first_pc_pop_range()
            .expect("a PC pop range must be found");
        assert_eq!(object.instance_id, 11);

        let position = pick_point_in_pop_range(object);
        assert_ne!(position, Position::default());
        assert_eq!(position.0, Vec3::new(4.0, 5.0, 6.0));
    }

    /// A zone with only non-PC pop ranges yields nothing, leaving the existing warn path intact.
    #[test]
    fn find_first_pc_pop_range_ignores_non_pc_ranges() {
        let zone = zone_with_objects(vec![make_pop_range(10, PopType::Npc, [1.0, 2.0, 3.0])]);
        assert!(zone.find_first_pc_pop_range().is_none());
    }

    /// While a demi is active the pet is not carried (INTERIM exclusion): ZoneLoaded falls back to
    /// the fresh-summon path instead of re-instating a demi actor with no volley tasks.
    #[test]
    fn take_combat_state_does_not_carry_while_demi_active() {
        let owner = ObjectId(1);
        let pet = ObjectId(2);
        let mut instance = Instance::default();
        let mut network = NetworkState::default();

        instance.insert_empty_actor(owner);
        insert_owned_pet(&mut instance, pet, owner);
        if let Some(NetworkedActor::Player { combat_state, .. }) = instance.find_actor_mut(owner) {
            combat_state.summoner.demi_expires_at =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(10));
        }

        let carried = take_combat_state_and_despawn_pets(&mut instance, &mut network, owner)
            .expect("player exists so combat state is returned");
        assert!(carried.summoner.carried_pet.is_none());
    }
}
