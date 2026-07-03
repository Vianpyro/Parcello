//! Wire protocol: JSON message envelopes exchanged over the WebSocket.
//!
//! Shared by the server and any Rust client (the test CLI today; the Flutter
//! client mirrors these shapes in Dart). Externally tagged with `type` in
//! snake_case, matching the engine's command/event wire format.

use parcello_engine::{ClientView, CommandError, CommandKind, Event};
use parcello_mods::ResolvedContent;
use serde::{Deserialize, Serialize};

/// Identity presented on connect. MVP (ADR-0003): guest names only unless
/// the server is started with a JWT secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthPayload {
    /// Global player JWT issued by the Identity Service (future work).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Guest display name, accepted only with `--insecure-guest`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Host only, from the Lobby: start the game.
    Start,
    /// In-game player command, relayed verbatim to the engine.
    Cmd {
        cmd: CommandKind,
    },
    Ping,
}

/// Public lobby info for one seat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeatInfo {
    pub seat: usize,
    pub player_id: String,
    pub name: String,
    pub connected: bool,
}

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
    },
    /// Broadcast on lobby membership or connection changes.
    Lobby {
        players: Vec<SeatInfo>,
    },
    GameStarted {
        view: Box<ClientView>,
    },
    /// Broadcast after every accepted command: what happened, then the new
    /// authoritative projection.
    Update {
        events: Vec<Event>,
        view: Box<ClientView>,
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
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"join","code":"ABCDE","auth":{"guest_name":"vianney"}}"#
        );

        let cmd: ClientMessage =
            serde_json::from_str(r#"{"type":"cmd","cmd":{"type":"roll"}}"#).unwrap();
        assert!(matches!(
            cmd,
            ClientMessage::Cmd {
                cmd: parcello_engine::CommandKind::Roll
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
