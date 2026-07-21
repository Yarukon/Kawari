// ! The friend system.
use crate::{DutyRelation, ToServer, ZoneConnection};
use kawari::{
    common::ObjectId,
    ipc::zone::{
        OnlineStatus, PlayerEntry, ServerZoneIpcData, ServerZoneIpcSegment, SocialListRequestType,
    },
};

/// Set the viewer-relative duty bits (`InDuty` / `AnotherWorld`) onto the matching entries.
///
/// **Channel B ONLY.** This is the single place `InDuty`/`AnotherWorld` are written into any
/// `OnlineStatusMask`. These bits outrank `Online` by priority, so they must never reach the
/// nametag channel's mask (which is built by `determine_base_online_status_mask`) or they would
/// stamp a duty icon on the nameplate. Pure over the entry slice so it can be unit-tested without a
/// live `ZoneConnection`. Shared by the friend and party appliers (`pub(super)` so
/// `party.rs` can route through it — the sole-producer invariant is preserved regardless of caller
/// count).
///
/// When a duty bit is set, the conflicting party/leader/sprout bits (`PartyLeader` / `PartyMember` /
/// `NewAdventurer`) are cleared on that entry. Party entries always carry `PartyMember`, and the
/// client's list-icon selector would otherwise pick `PartyMember` over `AnotherWorld` except via an
/// undocumented early-40 shortcut; clearing makes AnotherWorld render deterministically. It never
/// changes an `InDuty` outcome (InDuty already wins on priority) and only touches entries that
/// receive a duty bit -- `None` relations leave the mask untouched. Higher-precedence statuses
/// retail intentionally shows (`Busy`/`AFK`/`ViewingCutscene`/...) are left intact.
pub(super) fn apply_duty_relations_to_entries(
    entries: &mut [PlayerEntry],
    relations: &[(u64, DutyRelation)],
) {
    for (content_id, relation) in relations {
        let status = match relation {
            DutyRelation::InDuty => OnlineStatus::InDuty,
            DutyRelation::AnotherWorld => OnlineStatus::AnotherWorld,
            DutyRelation::None => continue,
        };
        if let Some(entry) = entries.iter_mut().find(|e| e.content_id == *content_id) {
            let mask = &mut entry.online_status_mask;
            // Clear conflicting list-icon bits before setting the duty bit. `remove_status` XORs, so
            // it must only be called on a bit that is currently set.
            for conflicting in [
                OnlineStatus::PartyLeader,
                OnlineStatus::PartyMember,
                OnlineStatus::NewAdventurer,
            ] {
                if mask.has_status(conflicting) {
                    mask.remove_status(conflicting);
                }
            }
            mask.set_status(status);
        }
    }
}

impl ZoneConnection {
    pub async fn refresh_friend_list(&mut self) {
        // Rebuild the base friend list from the DB. Only called for a "first page" request
        // (`next_index == 0`) now, so an unconditional rebuild is correct; subsequent pages reuse
        // the already-enriched `friend_results`.
        let mut db = self.database.lock();
        {
            let mut game_data = self.gamedata.lock();
            self.friend_results =
                db.find_friend_list(&mut game_data, self.player_data.character.content_id);
        }
        self.friend_index = 0;

        // Resolve (content_id, actor_id) pairs for currently-online friends only. This
        // `character.actor_id` is the linchpin: it is the same id that keys `Instance.actors`, so
        // online friends resolve into their live instance server-side. Offline friends are trimmed
        // here (smaller packet); server-side they would resolve to `None` anyway (harmless).
        let online_content_ids: Vec<u64> = self
            .friend_results
            .iter()
            .filter(|e| e.content_id != 0 && e.online_status_mask.has_status(OnlineStatus::Online))
            .map(|e| e.content_id)
            .collect();
        self.friend_enrich_pairs = online_content_ids
            .into_iter()
            .map(|cid| (cid, db.find_actor_id(cid)))
            .collect();
    }

    /// Ask the server loop to compute viewer-relative duty relations for `targets` on the given
    /// social list (`list_type`, echoed back in the response). Returns whether the outbound request
    /// was accepted by the world-server channel.
    ///
    /// NOTE: this deliberately uses the non-blocking [`ServerHandle::try_send`] rather than
    /// `ServerHandle::send` (which blocks on a full channel and panics on a closed one). The caller
    /// relies on a truthful success/failure so it can degrade to the un-enriched base list instead
    /// of hanging the social window when the request cannot be delivered. Not `async`: `try_send`
    /// is synchronous.
    pub fn request_social_list_duty_enrichment(
        &mut self,
        list_type: SocialListRequestType,
        sequence: u8,
        targets: Vec<(u64, ObjectId)>,
    ) -> bool {
        self.handle.try_send(ToServer::SocialListDutyRequest(
            self.player_data.character.actor_id,
            list_type,
            sequence,
            targets,
        ))
    }

    /// Apply the server's viewer-relative duty relations onto this connection's friend list
    /// (Channel B). See [`apply_duty_relations_to_entries`].
    pub fn apply_friend_duty_relations(&mut self, relations: &[(u64, DutyRelation)]) {
        apply_duty_relations_to_entries(&mut self.friend_results, relations);
    }

    pub fn add_to_friend_list(&mut self, friend_content_id: u64, pending: i32) {
        let mut db = self.database.lock();
        db.add_to_friend_list(
            friend_content_id as i64,
            self.player_data.character.content_id,
            pending,
        );
    }

    pub async fn remove_from_friend_list(&mut self, their_content_id: u64, their_name: String) {
        let their_actor_id;
        {
            let mut db = self.database.lock();
            their_actor_id = db.find_actor_id(their_content_id);

            // If we can't find them for some reason, don't proceed.
            if their_actor_id == ObjectId::default() {
                tracing::warn!(
                    "Unable to find {}'s actor id (it was {:#?})! What happened?)",
                    their_content_id,
                    ObjectId::default()
                );
                return;
            }

            // NOTE: This removes each other on both sides, so the receiver doesn't need to do this
            db.remove_from_friend_list(
                their_content_id as i64,
                self.player_data.character.content_id,
            );
        }

        self.handle
            .send(ToServer::FriendRemoved(
                self.player_data.character.actor_id,
                self.player_data.character.content_id as u64,
                self.player_data.character.name.clone(),
                their_actor_id,
                their_content_id,
                their_name,
            ))
            .await;
    }

    pub async fn friend_removed(&mut self, their_content_id: u64, their_name: String) {
        let ipc = ServerZoneIpcSegment::new(ServerZoneIpcData::FriendRemoved {
            content_id: their_content_id,
            name: their_name.clone(),
            unk1: 1,
        });

        self.send_ipc_self(ipc).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kawari::ipc::zone::OnlineStatusMask;

    fn online_entry(content_id: u64) -> PlayerEntry {
        PlayerEntry {
            content_id,
            online_status_mask: OnlineStatusMask::from_online_status(OnlineStatus::Online),
            ..Default::default()
        }
    }

    fn entry_with_statuses(content_id: u64, statuses: &[OnlineStatus]) -> PlayerEntry {
        let mut mask = OnlineStatusMask::default();
        for status in statuses {
            mask.set_status(*status);
        }
        PlayerEntry {
            content_id,
            online_status_mask: mask,
            ..Default::default()
        }
    }

    /// P4: a duty bit on a party entry clears the conflicting party/leader/sprout bits so the
    /// client's list-icon selector renders the duty icon deterministically (AnotherWorld no longer
    /// depends on the client's early-40 shortcut; InDuty already wins on priority). `Online` and
    /// other statuses are preserved; a `None` relation leaves the entry untouched.
    #[test]
    fn duty_bits_clear_conflicting_party_and_sprout_bits() {
        use OnlineStatus::*;
        let mut entries = vec![
            entry_with_statuses(100, &[Online, PartyLeader, PartyMember, NewAdventurer]),
            entry_with_statuses(200, &[Online, PartyLeader, PartyMember, NewAdventurer]),
            entry_with_statuses(300, &[Online, PartyMember]),
        ];
        apply_duty_relations_to_entries(
            &mut entries,
            &[
                (100, DutyRelation::AnotherWorld),
                (200, DutyRelation::InDuty),
                (300, DutyRelation::None),
            ],
        );

        // AnotherWorld: party/leader/sprout cleared -> {Online, AnotherWorld}.
        let m0 = &entries[0].online_status_mask;
        assert!(m0.has_status(AnotherWorld));
        assert!(m0.has_status(Online));
        assert!(!m0.has_status(PartyLeader));
        assert!(!m0.has_status(PartyMember));
        assert!(!m0.has_status(NewAdventurer));
        assert!(!m0.has_status(InDuty));

        // InDuty: same clearing behavior -> {Online, InDuty}.
        let m1 = &entries[1].online_status_mask;
        assert!(m1.has_status(InDuty));
        assert!(m1.has_status(Online));
        assert!(!m1.has_status(PartyLeader));
        assert!(!m1.has_status(PartyMember));
        assert!(!m1.has_status(NewAdventurer));
        assert!(!m1.has_status(AnotherWorld));

        // None: mask untouched (party bit stays, no duty bit, no clearing).
        let m2 = &entries[2].online_status_mask;
        assert!(!m2.has_status(InDuty));
        assert!(!m2.has_status(AnotherWorld));
        assert!(m2.has_status(Online));
        assert!(m2.has_status(PartyMember));
    }

    #[test]
    fn apply_sets_duty_bits_on_matching_entries_only() {
        let mut entries = vec![online_entry(100), online_entry(200), online_entry(300)];
        apply_duty_relations_to_entries(
            &mut entries,
            &[
                (100, DutyRelation::InDuty),
                (200, DutyRelation::AnotherWorld),
                (300, DutyRelation::None),
                (999, DutyRelation::InDuty), // no such entry -> ignored
            ],
        );

        assert!(
            entries[0]
                .online_status_mask
                .has_status(OnlineStatus::InDuty)
        );
        assert!(
            !entries[0]
                .online_status_mask
                .has_status(OnlineStatus::AnotherWorld)
        );

        assert!(
            entries[1]
                .online_status_mask
                .has_status(OnlineStatus::AnotherWorld)
        );
        assert!(
            !entries[1]
                .online_status_mask
                .has_status(OnlineStatus::InDuty)
        );

        // `None` leaves the entry untouched (no duty bit).
        assert!(
            !entries[2]
                .online_status_mask
                .has_status(OnlineStatus::InDuty)
        );
        assert!(
            !entries[2]
                .online_status_mask
                .has_status(OnlineStatus::AnotherWorld)
        );

        // The plain-online bit is preserved everywhere.
        for e in &entries {
            assert!(e.online_status_mask.has_status(OnlineStatus::Online));
        }
    }

    /// STRUCTURAL / DOCUMENTATION check — NOT a live Channel-A path test.
    ///
    /// HONESTY NOTE: the real regression this guards against is someone adding
    /// `set_status(OnlineStatus::InDuty | AnotherWorld)` into a nametag producer
    /// (`determine_base_online_status_mask`, `get_actual_online_status`, `nametag_online_status`).
    /// Those producers need a live `WorldDatabase`/`ZoneConnection` (a real `world.db`, seeded
    /// `volatile`/`search_info` rows), which can't be cheaply built in a unit test — so this test
    /// canNOT catch that leak. **The actual regression net is the grep gate** (PLAN.md §6-G):
    /// production `set_status(OnlineStatus::InDuty | AnotherWorld)` must appear ONLY in
    /// `apply_duty_relations_to_entries`.
    ///
    /// What this DOES prove cheaply, with teeth: the nametag channel has no runtime *collapse* for
    /// these bits. `for_nametag` only turns plain `Online` into `Offline`; InDuty/AnotherWorld pass
    /// straight through it, and since they outrank `Online` by priority they WOULD be stamped on the
    /// nameplate if they ever reached the nametag mask. Hence the invariant is enforced solely by
    /// keeping them in Channel B — there is no second line of defense downstream. (This assertion
    /// fails if someone later makes `for_nametag` collapse the duty bits, which would silently mask
    /// the hazard and must be a deliberate, reviewed change.)
    #[test]
    fn duty_bits_have_no_nametag_collapse_and_are_set_only_in_channel_b() {
        // The nametag transform does NOT rescue these bits (no downstream safety net).
        assert_eq!(OnlineStatus::InDuty.for_nametag(), OnlineStatus::InDuty);
        assert_eq!(
            OnlineStatus::AnotherWorld.for_nametag(),
            OnlineStatus::AnotherWorld
        );

        // Channel B: `apply_duty_relations_to_entries` is the ONLY producer, and it writes the bit
        // onto a friend `PlayerEntry` mask (never the player's own nametag mask).
        let mut entries = vec![online_entry(100)];
        apply_duty_relations_to_entries(&mut entries, &[(100, DutyRelation::InDuty)]);
        assert!(
            entries[0]
                .online_status_mask
                .has_status(OnlineStatus::InDuty)
        );
    }
}
