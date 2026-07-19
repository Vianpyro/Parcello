//! Wire protocol: JSON message envelopes exchanged over the WebSocket.
//!
//! Shared by the server and any Rust client (the test CLI today; the Flutter
//! client mirrors these shapes in Dart). Externally tagged with `type` in
//! `snake_case`, matching the engine's command/event wire format.

use parcello_engine::{ClientView, CommandError, CommandKind, Event, RuleParams};
use parcello_mods::ResolvedContent;
use serde::{Deserialize, Serialize};

/// Per-room game settings the host edits in the lobby (ADR-0015): the two
/// time limits plus the full effective rule set.
///
/// Initialised from the room's
/// mod content and the server's default timers, then overridden live. The
/// server clamps every field before applying it - the wire values are
/// untrusted host input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomSettings {
    /// Total game length in seconds; `None` = untimed (no game clock).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_seconds: Option<u64>,
    /// Per-turn limit in seconds; `None` = no turn limit (never auto-skip a
    /// connected player). A disconnected player is still skipped after the
    /// fixed grace regardless.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_seconds: Option<u64>,
    /// Personal reserve in seconds a connected acting seat may draw on to
    /// overrun `turn_seconds`, for the whole match, never refilled
    /// (ADR-0023). `None`/`0` disables it - the turn limit then hard-stops
    /// with no overrun.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_bank_seconds: Option<u64>,
    /// The effective rule scalars used when the game starts.
    pub rules: RuleParams,
}

/// Identity presented on connect. MVP (ADR-0003): guest names only unless
/// the server is started with a JWT secret.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthPayload {
    /// Global player JWT issued by the Identity Service (future work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Guest display name, accepted only with `--insecure-guest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_name: Option<String>,
    /// Public in-game handle a *token*-authenticated player chooses to be
    /// shown as (ADR-0033). Identity still comes from the token's `sub`; this
    /// only overrides the displayed name, re-sanitized server-side. Ignored
    /// for guests (their `guest_name` already is the display name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Per-seat reconnect token issued in `Joined` (ADR-0008). Required to
    /// re-take a seat held by a spoofable (guest) identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Create a room and join it as host (seat 0). `mods` selects the
    /// room's ordered mod list (ADR-0006); omitted or empty = the server's
    /// boot-time default set.
    Create {
        auth: AuthPayload,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mods: Option<Vec<String>>,
    },
    /// Join an existing room by code. Rejoining with the same identity
    /// reattaches to the original seat.
    Join {
        code: String,
        auth: AuthPayload,
    },
    /// Watch a room without taking a seat (ADR-0035). Same authentication
    /// as `Join`. With a `code`, watch that room; without one, the server
    /// picks the most watch-worthy game (most connected humans, else the
    /// bots showcase). Answered with `Spectating`; a spectator can only
    /// watch and `Leave` - every game command is refused.
    Spectate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<String>,
        auth: AuthPayload,
    },
    /// Host only, from the Lobby: add a server-driven bot seat. Bots fill
    /// empty seats but yield to humans - joining a full room evicts one
    /// (ADR-0014).
    AddBot,
    /// Host only, from the Lobby: drop the most recently added bot seat.
    RemoveBot,
    /// Host only, from the Lobby: replace the room's settings (timers +
    /// rules, ADR-0015). The server clamps and broadcasts the applied values
    /// back in `Lobby`.
    Configure {
        settings: RoomSettings,
    },
    /// Host only, from the Lobby: start the game.
    Start,
    /// After a game ends, replay in the same room: the first sender restarts
    /// the game for everyone still connected; players who left are dropped.
    PlayAgain,
    /// Leave the current room but keep the connection open, returning to the
    /// menu. The same socket can then create or join another room.
    Leave,
    /// In-game player command, relayed verbatim to the engine.
    Cmd {
        cmd: CommandKind,
    },
    /// Post-game survey (opens when the game ends): 1-5 rating plus an
    /// optional comment, stored in the server's history. One per player
    /// per game; entirely optional.
    Feedback {
        rating: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
    /// This client has finished rendering every `Update` up to and
    /// including `through_seq` (ADR-0028). The server's animation-sensitive
    /// timers (sealed-bid window, turn clock, bot pacing) wait for these
    /// acks - bounded by a hard cap, so a silent client can never stall
    /// the table. Clients with no animations (the CLI) send it immediately
    /// on every Update.
    AnimationDone {
        through_seq: u64,
    },
    /// Ask which mod ids this server can resolve (the subdirectories of its
    /// mods dir), so a client can offer a picker instead of free-text ids.
    /// Connection-scoped like `Ping`: valid before any room exists, because
    /// the answer feeds room *creation* (ADR-0006).
    ListMods,
    /// Enter the ranked queue (ADR-0034). Connection-scoped like `ListMods`
    /// (a queued connection is in no room); requires a token identity -
    /// spoofable guests cannot carry a persistent rating. The server answers
    /// `Queued`, then `MatchFound` once a table forms; the client joins the
    /// given room with a normal `Join`.
    QueueRanked {
        auth: AuthPayload,
    },
    /// Leave the ranked queue. Creating/joining a room or disconnecting
    /// removes the entry too.
    CancelQueue,
    /// Ask for the caller's ladder record on this server (ADR-0034); feeds
    /// the menu player card. Connection-scoped; requires a token identity.
    GetRating {
        auth: AuthPayload,
    },
    Ping,
}

/// One player's rating movement from a finished rated game (ADR-0034).
///
/// Broadcast to the room in `RatingsUpdated`. `display` is the shown ladder
/// number (a scaled conservative ordinal); matching and updates use
/// `mu`/`sigma`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RatingChange {
    pub player_id: String,
    pub mu: f64,
    pub sigma: f64,
    pub display: i64,
    pub display_delta: i64,
}

/// Public lobby info for one seat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeatInfo {
    pub seat: usize,
    pub player_id: String,
    pub name: String,
    pub connected: bool,
    /// A server-driven bot seat (ADR-0014), not a human connection. Clients
    /// label it as such instead of showing it as an offline player.
    #[serde(default)]
    pub is_bot: bool,
}

// No `Eq`: rating payloads carry `f64` (ADR-0034).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    RoomCreated {
        code: String,
    },
    /// Sent to the joining client: full room context, including the resolved
    /// mod bundle (mod distribution MVP) and, mid-game, a state snapshot.
    Joined {
        code: String,
        seat: usize,
        players: Vec<SeatInfo>,
        content: Box<ResolvedContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Box<ClientView>>,
        /// Keep this to rejoin the seat after a disconnect (ADR-0008).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reconnect: Option<String>,
        /// Seconds left before a time-boxed game ends by net worth
        /// (ADR-0010); absent for untimed games. Set only mid-game.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_remaining: Option<u64>,
        /// Per-turn time limit in seconds when the server enables the AFK
        /// timer (`--turn-timeout`); absent when off. Clients show a local
        /// per-turn countdown, reset on each Update.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_seconds: Option<u64>,
        /// Configured personal time bank in seconds (ADR-0023); absent when
        /// off. The live per-seat remaining amount rides `Update.banks`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_bank_seconds: Option<u64>,
        /// Current room settings (timers + rules) for the lobby UI (ADR-0015).
        settings: RoomSettings,
        /// This room is a ranked match (ADR-0034): matchmaker-created,
        /// host powers disabled, auto-started, rated at the end. Omitted
        /// (= false) for ordinary rooms, so old clients are unaffected.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        ranked: bool,
    },
    /// The spectator's mirror of `Joined` (ADR-0035): full room context,
    /// no seat, no reconnect token (a spectator holds nothing to protect).
    /// Subsequent `Lobby`/`GameStarted`/`Update` messages flow as for
    /// players, with the seatless spectator view.
    Spectating {
        code: String,
        players: Vec<SeatInfo>,
        content: Box<ResolvedContent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        view: Option<Box<ClientView>>,
        /// Seconds left of a time-boxed game (ADR-0010); mid-game only.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_remaining: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_seconds: Option<u64>,
        settings: RoomSettings,
    },
    /// Broadcast on lobby membership, connection, or settings changes.
    Lobby {
        players: Vec<SeatInfo>,
        /// Current room settings so joiners see them and the host's edits
        /// propagate live (ADR-0015).
        settings: RoomSettings,
    },
    GameStarted {
        view: Box<ClientView>,
        /// Total game length in seconds for a time-boxed game (ADR-0010);
        /// absent for untimed games. Clients run a local countdown.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_remaining: Option<u64>,
        /// Per-turn time limit in seconds when the server enables the AFK
        /// timer (`--turn-timeout`); absent when off. Clients show a local
        /// per-turn countdown, reset on each Update.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_seconds: Option<u64>,
        /// Configured personal time bank in seconds (ADR-0023); absent when
        /// off. The live per-seat remaining amount rides `Update.banks`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time_bank_seconds: Option<u64>,
    },
    /// Broadcast after every accepted command: what happened, then the new
    /// authoritative projection.
    Update {
        /// Monotonic per-room sequence number (ADR-0028): clients ack
        /// "rendered through N" with `ClientMessage::AnimationDone` so the
        /// server's animation-sensitive timers can wait for the table.
        #[serde(default)]
        seq: u64,
        events: Vec<Event>,
        view: Box<ClientView>,
        /// Live per-seat remaining time bank (ADR-0023), `None` when the
        /// room has no time bank configured. Server/session-layer display
        /// data - deliberately not part of `ClientView`, since the engine
        /// has no clock.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        banks: Option<Vec<u64>>,
    },
    /// Sent only to the offending client on a rejected command.
    Rejected {
        error: CommandError,
    },
    Error {
        message: String,
    },
    /// Reply to `ListMods`: the mod ids available on this server, sorted.
    /// Ids only - a client that wants the content resolves it by creating a
    /// room with them (ADR-0006); this is just enough to fill a picker.
    Mods {
        ids: Vec<String>,
    },
    /// Queue confirmation and size updates while waiting (ADR-0034).
    Queued {
        size: usize,
    },
    /// A ranked table formed (ADR-0034): join this room code with a normal
    /// `Join` to take your seat. The room only admits the matched players
    /// and auto-starts once everyone (or after a short grace, enough of
    /// everyone) has arrived.
    MatchFound {
        code: String,
    },
    /// Reply to `GetRating`: the caller's ladder record on this server.
    Rating {
        player_id: String,
        mu: f64,
        sigma: f64,
        games: u64,
        wins: u64,
        display: i64,
    },
    /// Broadcast to a ranked room when its game ends: every seat's rating
    /// movement (ADR-0034).
    RatingsUpdated {
        changes: Vec<RatingChange>,
    },
    Pong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_message_wire_format_is_stable() {
        let msg = ClientMessage::Join {
            code: "ABCDE".into(),
            auth: AuthPayload {
                guest_name: Some("vianney".into()),
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"join","code":"ABCDE","auth":{"guest_name":"vianney"}}"#
        );

        // Reconnect token (ADR-0008) rides in the auth payload.
        let msg: ClientMessage = serde_json::from_str(
            r#"{"type":"join","code":"ABCDE","auth":{"guest_name":"v","reconnect":"tok"}}"#,
        )
        .unwrap();
        assert!(matches!(
            msg,
            ClientMessage::Join { auth: AuthPayload { reconnect: Some(t), .. }, .. } if t == "tok"
        ));

        let cmd: ClientMessage =
            serde_json::from_str(r#"{"type":"cmd","cmd":{"type":"play_movement_card","value":3}}"#)
                .unwrap();
        assert!(matches!(
            cmd,
            ClientMessage::Cmd {
                cmd: parcello_engine::CommandKind::PlayMovementCard { value: 3 }
            }
        ));

        let cmd: ClientMessage =
            serde_json::from_str(r#"{"type":"cmd","cmd":{"type":"use_jail_card"}}"#).unwrap();
        assert!(matches!(
            cmd,
            ClientMessage::Cmd {
                cmd: parcello_engine::CommandKind::UseJailCard
            }
        ));

        let again: ClientMessage = serde_json::from_str(r#"{"type":"play_again"}"#).unwrap();
        assert!(matches!(again, ClientMessage::PlayAgain));

        let leave: ClientMessage = serde_json::from_str(r#"{"type":"leave"}"#).unwrap();
        assert!(matches!(leave, ClientMessage::Leave));

        let add: ClientMessage = serde_json::from_str(r#"{"type":"add_bot"}"#).unwrap();
        assert!(matches!(add, ClientMessage::AddBot));
        let rm: ClientMessage = serde_json::from_str(r#"{"type":"remove_bot"}"#).unwrap();
        assert!(matches!(rm, ClientMessage::RemoveBot));

        // Configure carries the timers plus a full rule set; omitted timers
        // deserialize to None (untimed / no per-turn limit).
        let cfg: ClientMessage = serde_json::from_str(
            r#"{"type":"configure","settings":{"turn_seconds":12,"time_bank_seconds":45,"rules":{
                "starting_balance":1500,"go_salary":200,"velocity_min":1,"velocity_max":5,
                "max_houses_per_property":5,"bankruptcy_threshold":0,
                "expropriation":200,"rent_boost":50,
                "win_full_groups":3}}}"#,
        )
        .unwrap();
        assert!(matches!(
            cfg,
            ClientMessage::Configure { settings }
                if settings.turn_seconds == Some(12)
                    && settings.time_bank_seconds == Some(45)
                    && settings.game_seconds.is_none()
                    && settings.rules.win_full_groups == 3
        ));

        let fb: ClientMessage =
            serde_json::from_str(r#"{"type":"feedback","rating":4,"comment":"gg"}"#).unwrap();
        assert!(matches!(
            fb,
            ClientMessage::Feedback { rating: 4, comment: Some(c) } if c == "gg"
        ));

        // Animation ack (ADR-0028): rendered through Update seq N.
        let ack: ClientMessage =
            serde_json::from_str(r#"{"type":"animation_done","through_seq":7}"#).unwrap();
        assert!(matches!(
            ack,
            ClientMessage::AnimationDone { through_seq: 7 }
        ));

        // Pre-ADR-0006 clients omit `mods`; the field must stay optional.
        let create: ClientMessage =
            serde_json::from_str(r#"{"type":"create","auth":{"guest_name":"v"}}"#).unwrap();
        assert!(matches!(create, ClientMessage::Create { mods: None, .. }));
        let create: ClientMessage = serde_json::from_str(
            r#"{"type":"create","auth":{"guest_name":"v"},"mods":["base","x"]}"#,
        )
        .unwrap();
        assert!(
            matches!(create, ClientMessage::Create { mods: Some(m), .. } if m == ["base", "x"])
        );

        // Mod discovery for the client's picker: bare request, id-list reply.
        let list: ClientMessage = serde_json::from_str(r#"{"type":"list_mods"}"#).unwrap();
        assert!(matches!(list, ClientMessage::ListMods));
        let mods = ServerMessage::Mods {
            ids: vec!["base".into(), "highroller".into()],
        };
        assert_eq!(
            serde_json::to_string(&mods).unwrap(),
            r#"{"type":"mods","ids":["base","highroller"]}"#
        );
    }

    #[test]
    fn spectate_wire_format_is_stable() {
        // Watch a specific room, or let the server pick (ADR-0035).
        let s: ClientMessage =
            serde_json::from_str(r#"{"type":"spectate","code":"BAKUZ","auth":{"guest_name":"v"}}"#)
                .unwrap();
        assert!(matches!(
            s,
            ClientMessage::Spectate { code: Some(c), .. } if c == "BAKUZ"
        ));
        let any: ClientMessage =
            serde_json::from_str(r#"{"type":"spectate","auth":{"guest_name":"v"}}"#).unwrap();
        assert!(matches!(any, ClientMessage::Spectate { code: None, .. }));
    }

    #[test]
    fn ranked_wire_format_is_stable() {
        // Queue entry/exit and the rating query (ADR-0034).
        let q: ClientMessage =
            serde_json::from_str(r#"{"type":"queue_ranked","auth":{"token":"jwt"}}"#).unwrap();
        assert!(matches!(
            q,
            ClientMessage::QueueRanked { auth: AuthPayload { token: Some(t), .. } } if t == "jwt"
        ));
        let c: ClientMessage = serde_json::from_str(r#"{"type":"cancel_queue"}"#).unwrap();
        assert!(matches!(c, ClientMessage::CancelQueue));
        let g: ClientMessage =
            serde_json::from_str(r#"{"type":"get_rating","auth":{"token":"jwt"}}"#).unwrap();
        assert!(matches!(g, ClientMessage::GetRating { .. }));

        assert_eq!(
            serde_json::to_string(&ServerMessage::Queued { size: 3 }).unwrap(),
            r#"{"type":"queued","size":3}"#
        );
        assert_eq!(
            serde_json::to_string(&ServerMessage::MatchFound {
                code: "BAKUZ".into()
            })
            .unwrap(),
            r#"{"type":"match_found","code":"BAKUZ"}"#
        );
        let updated = ServerMessage::RatingsUpdated {
            changes: vec![RatingChange {
                player_id: "id:u1".into(),
                mu: 27.5,
                sigma: 7.5,
                display: 1200,
                display_delta: 200,
            }],
        };
        assert_eq!(
            serde_json::to_string(&updated).unwrap(),
            r#"{"type":"ratings_updated","changes":[{"player_id":"id:u1","mu":27.5,"sigma":7.5,"display":1200,"display_delta":200}]}"#
        );
        let rating = ServerMessage::Rating {
            player_id: "id:u1".into(),
            mu: 25.0,
            sigma: 8.0,
            games: 4,
            wins: 1,
            display: 1040,
        };
        let json = serde_json::to_string(&rating).unwrap();
        assert!(json.starts_with(r#"{"type":"rating","player_id":"id:u1""#));
    }
}
