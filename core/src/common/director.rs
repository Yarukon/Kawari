//! Content director related types.

use binrw::binrw;

/// Events are sent by the server (who is acting as the director) to change state.
#[binrw]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DirectorEvent {
    /// Changes the festival phases for Ocean Fishing, but probably used for other things.
    /// In Ocean Fishing, seen with params of 13 and 23 (IKDRoute + 1 and something else unknown.)
    #[brw(magic = 2u32)]
    ChangeFestivalPhases {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Shows the Ocean Fishing scoring window, but probably used for other things.
    /// In Ocean Fishing, seen with a param of 19 (IKDRoute probably.)
    #[brw(magic = 3u32)]
    ShowOceanFishingWindow {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Shows the Variant Dungeon vote window, but probably used for other things.
    #[brw(magic = 0x10000002u32)]
    VariantVoteRoute {
        /// For Variant Dungeons, how many votes are needed
        votes_needed: u32,
        /// For Variant Dungeons, what route the NPC chose.
        npc_route: u32,
    },
    /// Hides the vote window, but probably used for other things.
    #[brw(magic = 0x10000004u32)]
    HideVariantVoteRoute,
    /// Shows "Duty Commenced", and starts the clock ticking down. `arg` is the number of seconds the duty should last.
    #[brw(magic = 0x40000001u32)]
    DutyCommence {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// `arg` is unknown.
    #[brw(magic = 0x40000002u32)]
    DutyCompletedFlyText {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// `arg` is unknown.
    #[brw(magic = 0x40000003u32)]
    DutyCompleted {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// `arg` is unknown.
    #[brw(magic = 0x40000005u32)]
    PartyWipe {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// `arg` is unknown.
    #[brw(magic = 0x40000006u32)]
    DutyRecommence {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Shows "one or more party members have yet to complete this duty" message along with the rewards.
    #[brw(magic = 0x4000000Cu32)]
    DutyFirstTimeCompletionNotice {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Seems to be in response to base director command 0. Arg seems to always be 1.
    ///
    /// Note the asymmetry with the client→server side: the trigger this replies to is named
    /// [`DirectorTrigger::BaseDirectorCommand0`], because no sync semantics could be found for
    /// it in the client. This name is kept as-is: it is a different direction, and there is no
    /// evidence either way for what the server means by it.
    #[brw(magic = 0x80000000u32)]
    SyncResponse {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Sets the current background music.
    #[brw(magic = 0x80000001u32)]
    SetBGM {
        /// Index into the BGM Excel sheet.
        bgm: u32,
    },
    /// Sets the remaining time in the duty. `arg` is the number of seconds.
    #[brw(magic = 0x80000004u32)]
    SetDutyTimeRemaining {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    /// Updates the content gauge.
    #[brw(magic = 0x8000000Cu32)]
    UpdateContentGauge {
        /// Index into the ContentGauge Excel sheet.
        content_gauge: u32,
        /// Progress of this gauge. From 0 to 10000.
        progress: u32,
    },
    /// At least used in The Merchant's Tale. First `arg` is the index into InstanceContextTextData.
    #[brw(magic = 0x80000027u32)]
    NpcYell {
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
    Unknown {
        id: u32,
        arg1: u32,
        arg2: u32,
        arg3: u32,
        arg4: u32,
    },
}

/// Triggers are sent by clients to inform the director of their actions.
///
/// **The magic is not a global enum: it is namespaced by the class of the director the
/// accompanying `handler_id` refers to.** The same value means completely different things
/// depending on the content, so a trigger may only ever be interpreted together with its
/// `handler_id`. The same vtable slot in three director classes sends three different magics:
/// `ContentDirector` sends `0x80000001`, `InstanceContentDirector` sends `0x40000004` and
/// `InstanceContentRaidCrystalTower002` sends `0x00000000`. Roughly:
///
/// * `0x8xxxxxxx` — commands of the `Director`/`ContentDirector` base classes.
/// * `0x4xxxxxxx` — commands of `InstanceContentDirector`.
/// * `0x1xxxxxxx` and `0x0xxxxxxx` — commands of the most-derived director subclass, so these
///   are only meaningful once you know which content is running. For example `0x10000002` is a
///   Variant Dungeon vote *and* a Deep Dungeon "use stone".
///
/// The trailing parameter of the wire format (formerly modeled as `unk3`) is deliberately not
/// part of any variant: the client's command sender writes only the first five dwords of the
/// payload and never initializes the sixth, so what arrives there is leftover client stack
/// memory. The same trigger has been observed carrying 0xB0, 0x243 and 0x00 in three separate
/// captures. **It must never be read.** It is consumed as padding instead, because
/// `ClientTrigger::trigger` is `pad_size_to = 24`, which keeps the wire size unchanged.
#[binrw]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DirectorTrigger {
    /// Command 0 of the most-derived director class, so its meaning depends entirely on the
    /// content. Not Gold Saucer specific despite where we first saw it: there are 17 senders,
    /// spread over the Gold Saucer, Frontline, Air Force One, Crystal Tower, guild order,
    /// Variant Dungeon and public content directors. `GoldSaucerDirector.Initialize` sends
    /// (0, 0), which is where the "seen while GATEs were spawning" observation came from.
    #[brw(magic = 0u32)]
    SubclassCommand0 {
        unk1: u32,
        unk2: u32,
    },
    /// Command 1 of the most-derived director class, see [`Self::SubclassCommand0`]. 21 senders
    /// across the same set of directors. `GFateDirector.vf2` sends (1, 0) when a GATE director
    /// becomes ready, which is what we originally observed.
    #[brw(magic = 1u32)]
    SubclassCommand1 {
        unk1: u32,
        unk2: u32,
    },
    /// Command 2 of the most-derived director class, see [`Self::SubclassCommand0`].
    ///
    /// Known senders, which is exactly why this cannot carry a content-specific name:
    /// * Variant Dungeons: the route vote, where `arg1` is the value of the chosen dialog entry.
    /// * Deep Dungeons: `InstanceContentDeepDungeon.UseStone`.
    #[brw(magic = 0x10000002u32)]
    SubclassCommand2 {
        arg1: u32,
        arg2: u32,
    },
    /// Sent by the client *before* it starts playing a duty's cutscene.
    ///
    /// Despite the name we used to give it, this is not a "finished" notification: the sender is
    /// `InstanceContentDirector.OnStartCutscene`, a method registered to Lua under literally that
    /// name, and it fires ahead of playback. The sibling `OnEndCutscene`/`OnFinishCutscene`
    /// methods send nothing over the network at all, so no "cutscene finished" trigger exists.
    #[brw(magic = 0x40000001u32)]
    StartCutscene {
        /// Row of the `Cutscene` Excel sheet, normally the `Cutscene` of the duty's
        /// `InstanceContent` row. This is what the old "is 174 for Sastasha" note was about:
        /// 174 is Sastasha's `Cutscene` row. Confirmed against captures of Zodiark (2756) and
        /// The Wanderer's Palace (1023) too.
        cutscene: u32,
        unk2: u32,
    },
    /// When the player toggles the striking dummy in an explorer mode instance, from the
    /// tourism menu's "Summon Striking Dummy"/"Remove Striking Dummy" buttons
    /// (`AgentTourismMenu.ReceiveEvent`, Addon rows 13032 and 13033).
    ///
    /// Implementing this needs more than spawning the actor: the client only flips the button's
    /// checked state from a flag set by `InstanceContentDirector.vf325` case 19, i.e. when the
    /// server replies with `DirectorEvent` `0x40000014`. Without that acknowledgement the button
    /// never changes, no matter what is spawned. The dummy itself is BNpcBase 11744, which the
    /// world server's `!dummy` chat command already knows how to spawn.
    #[brw(magic = 0x40000006u32)]
    ToggleStrikingDummy {
        /// 1 to summon the dummy, 0 to remove it. (It is *not* always 1, as we used to note.)
        summon: u32,
        unk2: u32,
    },
    /// Command 0 of the `Director` base class, so it can come from any director at all.
    ///
    /// We used to call this `Sync`, but no sync semantics for it exist anywhere in the client.
    /// It is sent by `EventFramework.LeaveCurrentContent` (to the quest battle director), by
    /// `ContentNpcEventHandler.vf63` and by the Gimmick handlers. `unk1` is not always 0: the
    /// `ContentNpcEventHandler` path passes a field of the event it is handling.
    #[brw(magic = 0x80000000u32)]
    BaseDirectorCommand0 {
        unk1: u32,
        unk2: u32,
    },
    Unknown {
        id: u32,
        unk1: u32,
        unk2: u32,
    },
}
