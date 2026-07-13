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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthPayload {
    /// Global player JWT issued by the Identity Service (future work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Guest display name, accepted only with `--insecure-guest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_name: Option<String>,
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
    Ping,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
                token: None,
                guest_name: Some("vianney".into()),
                reconnect: None,
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
    }
}
