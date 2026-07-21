use std::{
    sync::Arc,
    time::{Instant, SystemTime},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use crate::{
    Content, GameData, Recipe, Unlock,
    database::{
        AetherCurrent, Aetheryte, Character, ClassJob, Companion, Friends, GrandCompany, Mentor,
        Quest, SearchInfo, Volatile,
    },
    lua::{KawariLua, LuaTask},
};
use kawari::{
    common::{HandlerId, ObjectId, Position, timestamp_secs},
    config::WorldConfig,
    ipc::zone::{
        ApartmentList, ApartmentListEntry, CWLSMemberListEntry, ClientTriggerCommand,
        ClientZoneIpcSegment, Condition, Conditions, DutyFinderSetting, DyeInformation,
        GrandCompany as IpcGrandCompany, LetterPreview, PlayerEntry, ServerZoneIpcData,
        ServerZoneIpcSegment, build_cf_pop,
    },
    opcodes::ServerZoneIpcType,
    packet::{
        CompressionType, ConnectionState, ConnectionType, IpcSegmentHeader, PacketSegment,
        SegmentData, SegmentType, ServerIpcSegmentHeader, parse_packet, send_packet,
    },
};
use physis::{
    savedata::chardat::CustomizeData,
};

use super::{
    WorldDatabase,
    common::{ClientId, ServerHandle},
    inventory::{BuyBackList, GlamourStorage, HousingInventory, Inventory, PlateStorage},
};

mod actor;
mod chat;
mod effect;
mod event;
mod friends;
mod item;
mod linkshell;
mod lua;
mod mail;
mod party;
mod quest;
mod social;
pub mod spawn_allocator;
mod stats;
pub use stats::{BaseParameters, DamageRollModifiers};
mod unlock;
mod zone;

/// The level the player's actions, gating, morphs, gauges, and combat display should use.
/// In level-synced content this is the synced (capped) level; otherwise the true EXP-array level.
/// Progression (XP / level-up / persistence) is unaffected: those read `classjob.levels` directly,
/// never `common_spawn.level`.
pub(crate) fn effective_level(synced_level: Option<u8>, true_level: u8) -> u8 {
    synced_level.unwrap_or(true_level)
}

#[derive(Debug, Default, Clone)]
pub struct TeleportQuery {
    pub aetheryte_id: u16,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum TeleportReason {
    #[default]
    NotSpecified,
    /// Teleporting/Returning to an Aetheryte or shared
    Aetheryte,
}

/// Quest information stored in the database.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PersistentQuest {
    /// ID of the quest.
    pub id: u16,
    /// Sequence in the quest.
    pub sequence: u8,
}

/// Persistent player data.
/// Please try to keep fields in here to a minimum, specifically stuff that may be persistent and/or accessed from Lua.
#[derive(Debug, Default, Clone)]
pub struct PlayerData {
    pub character: Character,
    pub classjob: ClassJob,
    pub subrace: u8,
    pub volatile: Volatile,
    pub inventory: Inventory,
    pub city_state: u8,
    pub mentor: Mentor,
    pub search_info: SearchInfo,

    pub teleport_query: TeleportQuery,
    pub gm_invisible: bool,

    pub item_sequence: u32,
    /// Transaction-batch id for InventoryTransaction/InventoryTransactionFinish packets. Unlike
    /// `item_sequence` (which increments per UpdateInventorySlot/ContainerInfo packet), every
    /// InventoryTransaction within a single logical operation (e.g. one gearset equip) shares ONE
    /// id, and the trailing InventoryTransactionFinish repeats it. This is how the client correlates
    /// a finish with its transactions; mismatched ids leave it stuck "syncing with the server".
    pub transaction_sequence: u32,
    pub shop_sequence: u32,
    /// The server-side copy of NPC shop buyback lists.
    pub buyback_list: BuyBackList,
    pub unlock: Unlock,
    pub content: Content,
    pub aetheryte: Aetheryte,
    pub aether_current: AetherCurrent,
    pub companion: Companion,
    pub quest: Quest,
    pub saw_inn_wakeup: bool,
    pub friends: Friends,
    pub grand_company: GrandCompany,
    pub glamour: GlamourStorage,
    pub plate: PlateStorage,
    // TODO: These inventories need to be made persistent and also vary per property, this is just for initial support
    pub house_inventory: HousingInventory,
}

/// Various obsfucation-related bits like the seeds and keys for this connection.
#[derive(Debug, Default, Clone)]
pub struct ObsfucationData {
    pub seed1: u8,
    pub seed2: u8,
    pub seed3: u32,
}

/// Represents a single connection between an instance of the client and the zone portion of the world server.
pub struct ZoneConnection {
    pub config: WorldConfig,
    pub socket: TcpStream,

    pub state: ConnectionState,
    pub player_data: PlayerData,

    pub id: ClientId,
    pub handle: ServerHandle,

    pub database: Arc<Mutex<WorldDatabase>>,
    pub lua: Arc<Mutex<KawariLua>>,
    pub gamedata: Arc<Mutex<GameData>>,

    pub teleport_reason: TeleportReason,
    pub active_minion: u32,
    /// The player's party id number, used for networking party-related events
    pub party_id: u64,
    pub is_party_leader: bool,
    /// Whether the player is currently watching a cutscene, driving the `ViewingCutscene` online
    /// status (nametag icon + social mask bit) and the party-UI cutscene marker.
    ///
    /// Deliberately connection-local and in-memory only: it is transient state that is meaningless
    /// once the connection is gone. Persisting it would strand a stuck cutscene icon on the
    /// character after a crash or a hard disconnect, which is exactly the failure this flag is
    /// supposed to avoid. Never write it to the database or into
    /// `Database::determine_base_online_status_mask`.
    pub watching_cutscene: bool,
    /// The player's status when connecting/reconnecting. If true, they need to rejoin their party.
    pub rejoining_party: bool,
    /// The player's currently active quests.
    pub login_time: Option<SystemTime>,
    pub zone_initialize_sent: bool,
    pub glamour_information: Option<ClientTriggerCommand>,
    pub pending_aesthetician_customize: Option<CustomizeData>,

    pub last_keep_alive: Instant,

    /// Whether the player was gracefully logged out
    pub gracefully_logged_out: bool,

    pub obsfucation_data: ObsfucationData,

    // TODO: support more than one content in the queue
    pub queued_content: Option<u16>,
    pub content_settings: Option<DutyFinderSetting>,
    pub current_instance_id: Option<u16>,

    pub conditions: Conditions,

    /// List of queued tasks from the server.
    pub queued_tasks: Vec<LuaTask>,

    /// Information from before we entered the content.
    pub old_zone_id: u16,
    pub old_position: Position,
    pub old_rotation: f32,

    /// Information about the current content.
    pub content_handler_id: HandlerId,
    /// Non-content zone directors, kept separate so player spawns don't inherit their handler.
    pub zone_director_handler_id: HandlerId,
    pub event_handler_id: Option<HandlerId>,
    pub recipe: Option<Recipe>,
    pub synced_level: Option<u8>,
    /// Player Search results.
    pub search_results: Vec<PlayerEntry>,
    /// The Player Search's current sequence value. Increases by 10 every time the client requests more results.
    pub search_index: usize,
    /// Friend list results.
    pub friend_results: Vec<PlayerEntry>,
    /// The friend list's current sequence value. Increases by 10 every time the client requests more results.
    pub friend_index: usize,
    /// CWLS member results sent when the player opens the CWLS menu or picks a different linkshell in the same menu.
    pub cwls_results: Vec<CWLSMemberListEntry>,
    /// CWLS member index. Increases by 8 every time the client requests more results.
    pub cwls_index: usize,
    /// The player's Moogle Delivery Service previews. Used to populate the Moogle Delivery Service window when interacting with a Delivery Moogle or a Letter Box.
    pub mail_results: Vec<LetterPreview>,
    /// The current index into the mailbox previews. Increases by 5 every time the clint requests more previews.
    pub mail_index: usize,
    /// Whether the player is spawned in or not.
    pub spawned_in: bool,
    /// Whether the client has acknowledged the initial zone-in fade for the current zone.
    pub zone_in_confirmed: bool,
    /// The last teleport offered to this player. Only one can be kept at a time.
    pub offered_teleport: Option<TeleportQuery>,
    /// Whether the player is trading with another player or not.
    pub is_trading: bool,
    /// For the crappy setup_director() function right now.
    pub director_vars: Option<ServerZoneIpcSegment>,
    /// Current dye action information.
    pub dyeing_information: Option<DyeInformation>,
    /// True while a friend-list duty-enrichment round-trip is in flight (Channel B). Guards against
    /// firing a second request (or draining an un-enriched page) if the client reopens/re-requests
    /// the first page before the response arrives.
    pub friend_enrich_pending: bool,
    /// The sequence of the most recent first-page friend-list request; the enrichment response is
    /// answered with this (it may have advanced past the in-flight request's sequence if the list
    /// was reopened while the round-trip was pending).
    pub friend_enrich_sequence: u8,
    /// The (content_id, actor_id) pairs of currently-online friends, resolved in
    /// `refresh_friend_list` and sent to the server loop for duty enrichment.
    pub friend_enrich_pairs: Vec<(u64, ObjectId)>,
    /// True while a party-list duty-enrichment round-trip is in flight (Channel B). Independent of
    /// `friend_enrich_pending` so a Party and a Friends round-trip can be in flight concurrently
    /// (tab-switch under latency) without cross-contaminating each other's list.
    pub party_enrich_pending: bool,
    /// The sequence of the most recent first-page party-list request; the enrichment response is
    /// answered with this (it may have advanced past the in-flight request's sequence if the list
    /// was reopened while the round-trip was pending).
    pub party_enrich_sequence: u8,
    /// The (content_id, actor_id) pairs of currently-online, non-self party members, resolved in
    /// `refresh_party_list` and sent to the server loop for duty enrichment.
    pub party_enrich_pairs: Vec<(u64, ObjectId)>,
    /// The base party-member entries stashed by `refresh_party_list` to survive the enrichment
    /// round-trip await; `apply_party_duty_relations` decorates them and the deferred send drains
    /// them (the party list is single-page, so this is taken wholesale, not paginated).
    pub party_enrich_entries: Vec<PlayerEntry>,
}

impl ZoneConnection {
    pub fn parse_packet(&mut self, data: &[u8]) -> Vec<PacketSegment<ClientZoneIpcSegment>> {
        parse_packet(data, &mut self.state)
    }

    /// Sends an IPC segment to the player, where the source actor is also the player.
    pub async fn send_ipc_self(&mut self, ipc: ServerZoneIpcSegment) {
        // This is meant to protect against stack-smashing in nested futures
        Box::pin(self.send_ipc_from(self.player_data.character.actor_id, ipc)).await;
    }

    /// Sends an IPC segment to the player, where the source actor can be specified.
    pub async fn send_ipc_from(&mut self, source_actor: ObjectId, ipc: ServerZoneIpcSegment) {
        let segment = PacketSegment {
            source_actor,
            target_actor: self.player_data.character.actor_id,
            segment_type: SegmentType::Ipc,
            data: SegmentData::Ipc(ipc),
        };

        // Ditto from above
        Box::pin(self.send_segment(segment)).await;
    }

    pub async fn send_segment(&mut self, segment: PacketSegment<ServerZoneIpcSegment>) {
        // Ditto as above
        Box::pin(send_packet(
            &mut self.socket,
            &mut self.state,
            ConnectionType::Zone,
            if self.config.enable_packet_compression {
                CompressionType::Oodle
            } else {
                CompressionType::Uncompressed
            },
            &[segment],
        ))
        .await;
    }

    pub fn prepare_zone_session(&mut self, actor_id: ObjectId) {
        self.player_data.item_sequence = 0;
        self.player_data.transaction_sequence = 0;
        self.player_data.shop_sequence = 0;
        self.zone_initialize_sent = false;

        tracing::info!("Client {actor_id} zone setup complete");
    }

    pub async fn send_initial_keep_alive(&mut self) {
        self.send_segment(PacketSegment::<ServerZoneIpcSegment> {
            segment_type: SegmentType::KeepAliveRequest,
            data: SegmentData::KeepAliveRequest {
                id: 0xE0037603u32,
                timestamp: timestamp_secs(),
            },
            ..Default::default()
        })
        .await;
    }

    pub async fn send_initialize(&mut self) -> bool {
        if self.zone_initialize_sent {
            return false;
        }

        let actor_id = self.player_data.character.actor_id;

        self.send_segment(PacketSegment::<ServerZoneIpcSegment> {
            segment_type: SegmentType::Initialize,
            data: SegmentData::Initialize {
                actor_id,
                timestamp: timestamp_secs(),
            },
            ..Default::default()
        })
        .await;

        self.zone_initialize_sent = true;
        tracing::info!("Client {actor_id} initialization packet sent");
        true
    }

    pub async fn begin_log_out(&mut self) {
        // Drop the cutscene status before anything else, so observers stop seeing the icon on a
        // player who dropped out mid-cutscene. The barrier this has to stay in front of is
        // `commit_player_data` below, not the `is_online` assignment right after: the mask is
        // computed from the DB (`determine_base_online_status_mask`), so once the commit has
        // written `is_online = false` the mask no longer carries `Online` and the cutscene bit,
        // which is only ever layered on top of it, can no longer be cleared from it. This is the
        // funnel for both a graceful logout and a forced one after a network drop.
        //
        // The inn bed IS one of these: retail keeps the player online for the whole sleep
        // animation -- it carries the `ViewingCutscene` status like any other cutscene -- and only
        // sends LogOutComplete afterwards, between that scene and the log out scene. Our script
        // matches that ordering, so this call is what takes the icon back down there.
        self.end_watching_cutscene().await;

        // Mark the player as offline in the db.
        self.player_data.volatile.is_online = false;

        // If we were last in an instance, tell the server we're outside of it so we don't get stuck/crash.
        if self.conditions.has_condition(Condition::BoundByDuty) {
            self.player_data.volatile.zone_id = self.old_zone_id as i32;
            self.player_data.volatile.position = self.old_position;
            self.player_data.volatile.rotation = self.old_rotation as f64;
        }

        // Update playtime
        {
            // By default, just write back the original playtime if something goes wrong.
            let mut database = self.database.lock();
            let mut time_played_minutes =
                database.find_playtime(self.player_data.character.content_id as u64);
            if let Some(login_time) = self.login_time {
                match SystemTime::now().duration_since(login_time) {
                    Ok(session_length) => {
                        time_played_minutes += (session_length.as_secs() / 60) as i64;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Unable to update the session's playtime, due to the following error: {e}",
                        );
                    }
                }
            }
            self.player_data.character.time_played_minutes = time_played_minutes;
        }

        // Write the player back to the database
        {
            let mut database = self.database.lock();
            database.commit_player_data(&self.player_data);
        }

        // Don't bother sending these if the client forcefully D/C'd.
        if self.gracefully_logged_out {
            // Set the client's conditions for logout preparation
            self.conditions.set_condition(Condition::LoggingOut);
            self.send_conditions().await;

            // Tell the client we're ready to disconnect at any moment
            {
                let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::LogOutComplete {
                    unk: [1, 0, 0, 0, 0, 0, 0, 0],
                });
                self.send_ipc_self(ipc).await;
            }
        }
    }

    pub async fn send_arbitrary_packet(&mut self, op_code: u16, data: Vec<u8>) {
        let ipc = ServerZoneIpcSegment {
            header: ServerIpcSegmentHeader::from_opcode(ServerZoneIpcType::Unknown(op_code)),
            data: ServerZoneIpcData::Unknown { unk: data },
        };
        self.send_ipc_self(ipc).await;
    }

    pub async fn send_keep_alive(&mut self, id: u32, timestamp: u32) {
        self.send_segment(PacketSegment {
            segment_type: SegmentType::KeepAliveResponse,
            data: SegmentData::KeepAliveResponse { id, timestamp },
            ..Default::default()
        })
        .await;
    }

    pub async fn register_for_content(&mut self, content_ids: [u16; 5]) {
        self.queued_content = Some(content_ids[0]);

        // The `DutyFinderSetting` mode word the client renders the ready-popup mode icon from.
        // `to_ready_mode_word` echoes the actual queued settings (instead of the old hardcode that
        // left the Explorer bit permanently set) while forcing bit 0x20, the server-authored
        // context bit that suppresses the false withdraw-penalty dialog.
        let settings_word = self
            .content_settings
            .unwrap_or_default()
            .to_ready_mode_word();

        // TODO: store what they registered with, because it can change
        let classjob_id = self.player_data.classjob.current_class as u8;
        let (update, found) = build_cf_pop(settings_word, classjob_id, content_ids, 1);

        self.send_ipc_self(ServerZoneIpcSegment::new(update)).await;
        self.send_ipc_self(ServerZoneIpcSegment::new(found)).await;
    }

    pub async fn send_playtime(&mut self) {
        if let Some(login_time) = self.login_time {
            let time_played_minutes;
            {
                let mut database = self.database.lock();
                time_played_minutes =
                    database.find_playtime(self.player_data.character.content_id as u64);
            }

            // In case something goes wrong with calculating the current session's playtime, we'll send the old total by default.
            let mut total_play_time = time_played_minutes;
            match SystemTime::now().duration_since(login_time) {
                Ok(session_length) => {
                    total_play_time = (session_length.as_secs() / 60) as i64 + time_played_minutes;

                    // Retail doesn't do this, but it's a nice QoL thing to have.
                    self.send_notice(
                        &format!(
                            "Total Play Time this Session: {} hours, {} minutes, {} seconds",
                            session_length.as_secs() / 3600,
                            (session_length.as_secs() / 60) % 60,
                            session_length.as_secs() % 60
                        )
                        .to_string(),
                    )
                    .await;
                }
                Err(e) => {
                    tracing::error!(
                        "Unable to determine the current session's playtime, due to an error: {e}",
                    );
                }
            }

            let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::Playtime {
                duration: total_play_time as u32,
            });

            self.send_ipc_self(ipc).await;
        }
    }

    // TODO: break this out into its own housing file eventually
    pub async fn send_apartment_list(&mut self, starting_index: u32) {
        let mut apartments = vec![ApartmentListEntry::default(); ApartmentListEntry::COUNT];
        apartments[0] = ApartmentListEntry {
            resident_online_status_mask: self.get_online_status_mask(),
            resident_zone_id: self.player_data.volatile.zone_id as u16,
            resident_name: self.player_data.character.name.clone(),
            apartment_description: "A test apartment provided to you by Kawari!".to_string(),
            ..Default::default()
        };

        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::ApartmentList(ApartmentList {
            content_id: self.player_data.character.content_id as u64,
            flags: 128,
            ward_id: 0,
            zone_id: self.player_data.volatile.zone_id as u16, // TODO: Apartment lists can be requested from apartments themselves, so this wouldn't make sense there!
            world_id: self.config.world_id,
            list_index: starting_index,
            apartments,
        }));
        self.send_ipc_self(ipc).await;
    }

    pub async fn send_grand_company_info(&mut self) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::GrandCompanyInfo {
            active_company_id: self.player_data.grand_company.active_company,
            maelstrom_rank: self.grand_company_rank(IpcGrandCompany::Maelstrom).unwrap(),
            twin_adder_rank: self.grand_company_rank(IpcGrandCompany::Adders).unwrap(),
            immortal_flames_rank: self.grand_company_rank(IpcGrandCompany::Flames).unwrap(),
        });
        self.send_ipc_self(ipc).await;
    }

    pub fn grand_company_rank(&self, company: IpcGrandCompany) -> Option<u8> {
        if company != IpcGrandCompany::None {
            let company_index = company as usize - 1;
            return Some(self.player_data.grand_company.company_ranks.0[company_index]);
        }

        None
    }

    pub fn set_grand_company_rank(&mut self, rank: u8) {
        if self.player_data.grand_company.active_company != IpcGrandCompany::None {
            let company_index = self.player_data.grand_company.active_company as usize - 1;
            self.player_data.grand_company.company_ranks.0[company_index] = rank;
            {
                let mut db = self.database.lock();
                db.commit_grand_companies(&self.player_data);
            }
        }
    }

    pub fn set_grand_company(&mut self, company: IpcGrandCompany) {
        self.player_data.grand_company.active_company = company;

        // If the player has no grand company after the above, this is a no-op.
        if self.grand_company_rank(company).unwrap_or_default() == 0 {
            self.set_grand_company_rank(1);
        }

        {
            let mut db = self.database.lock();
            db.commit_grand_companies(&self.player_data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::effective_level;

    #[test]
    fn effective_level_uses_true_level_when_not_synced() {
        assert_eq!(effective_level(None, 90), 90);
    }

    #[test]
    fn effective_level_uses_synced_level_when_synced() {
        assert_eq!(effective_level(Some(50), 90), 50);
    }

    #[test]
    fn effective_level_synced_equal_to_true_is_stable() {
        assert_eq!(effective_level(Some(50), 50), 50);
    }
}
