//! Golden wire-format fixtures for `ClientMessage`/`ServerMessage`
//! (protocol duplication audit, Strategy S1, `docs/protocol-duplication-audit.md`).
//!
//! Same discipline as `crates/engine/tests/protocol_fixtures.rs`: an
//! exhaustive `match` (no wildcard) names every variant, so a newly added
//! one is a compile error here until it is either given a fixture or an
//! explicit deferred-with-reason arm. Fixtures are grouped into one JSON
//! object per enum (`client_message.json`, `server_message.json`), keyed by
//! variant name, rather than one file per variant.
//!
//! Four `ServerMessage` variants (`Joined`, `Spectating`, `GameStarted`,
//! `Update`) are deferred: they embed `ClientView`/`ResolvedContent`, whose
//! Dart mirror is a set of typed `.fromJson` constructors (a different,
//! already-tested duplication mechanism - see `clients/flutter/test/protocol_test.dart`),
//! not a `switch` with a silent default. Covering them here is S2's job
//! (`docs/protocol-duplication-audit.md` section 3, S2) once the Dart
//! mirror stops being hand-written.
//!
//! To add a new variant: add a match arm below with a representative
//! instance, then run
//! `cargo test -p parcello-protocol --test protocol_fixtures -- --ignored regenerate_fixtures`
//! to write it into the fixture file, and commit the change.

use parcello_engine::{CommandError, CommandKind, RuleParams};
use parcello_protocol::{AuthPayload, ClientMessage, RatingChange, RoomSettings, ServerMessage};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol-fixtures")
        .join(format!("{name}.json"))
}

fn read_fixtures(name: &str) -> Map<String, Value> {
    let path = fixture_path(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()));
    match serde_json::from_str(&text).unwrap() {
        Value::Object(map) => map,
        other => panic!("{}: expected a JSON object, got {other:?}", path.display()),
    }
}

fn write_fixtures(name: &str, entries: &[(&str, Value)]) {
    let map: Map<String, Value> = entries
        .iter()
        .cloned()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let json = serde_json::to_string_pretty(&Value::Object(map)).unwrap();
    fs::write(fixture_path(name), format!("{json}\n")).unwrap();
}

fn assert_fixture<T>(fixtures: &Map<String, Value>, name: &str, instance: &T)
where
    T: Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let stored = fixtures
        .get(name)
        .unwrap_or_else(|| panic!("no fixture entry for {name:?} - run `regenerate_fixtures`"));
    let from_fixture: T = serde_json::from_value(stored.clone())
        .unwrap_or_else(|e| panic!("fixture for {name:?} does not deserialize: {e}"));
    assert_eq!(&from_fixture, instance, "fixture drift for {name}");
    let serialized = serde_json::to_value(instance).unwrap();
    assert_eq!(&serialized, stored, "fixture drift for {name}");
}

fn client_message_name(m: &ClientMessage) -> &'static str {
    match m {
        ClientMessage::Create { .. } => "create",
        ClientMessage::Join { .. } => "join",
        ClientMessage::Spectate { .. } => "spectate",
        ClientMessage::AddBot => "add_bot",
        ClientMessage::RemoveBot => "remove_bot",
        ClientMessage::Configure { .. } => "configure",
        ClientMessage::Start => "start",
        ClientMessage::PlayAgain => "play_again",
        ClientMessage::Leave => "leave",
        ClientMessage::Cmd { .. } => "cmd",
        ClientMessage::Feedback { .. } => "feedback",
        ClientMessage::AnimationDone { .. } => "animation_done",
        ClientMessage::ListMods => "list_mods",
        ClientMessage::QueueRanked { .. } => "queue_ranked",
        ClientMessage::CancelQueue => "cancel_queue",
        ClientMessage::GetRating { .. } => "get_rating",
        ClientMessage::Ping => "ping",
    }
}

fn client_message_fixtures() -> Vec<ClientMessage> {
    let guest = |name: &str| AuthPayload {
        guest_name: Some(name.into()),
        ..Default::default()
    };
    vec![
        ClientMessage::Create {
            auth: guest("vianney"),
            mods: Some(vec!["base".into()]),
        },
        ClientMessage::Join {
            code: "ABCDE".into(),
            auth: guest("vianney"),
        },
        ClientMessage::Spectate {
            code: Some("ABCDE".into()),
            auth: guest("vianney"),
        },
        ClientMessage::AddBot,
        ClientMessage::RemoveBot,
        ClientMessage::Configure {
            settings: RoomSettings {
                game_seconds: Some(3600),
                turn_seconds: Some(25),
                time_bank_seconds: None,
                rules: RuleParams::default(),
            },
        },
        ClientMessage::Start,
        ClientMessage::PlayAgain,
        ClientMessage::Leave,
        ClientMessage::Cmd {
            cmd: CommandKind::Build {
                tile: "ave_a".into(),
            },
        },
        ClientMessage::Feedback {
            rating: 4,
            comment: Some("gg".into()),
        },
        ClientMessage::AnimationDone { through_seq: 7 },
        ClientMessage::ListMods,
        ClientMessage::QueueRanked {
            auth: AuthPayload {
                token: Some("jwt".into()),
                ..Default::default()
            },
        },
        ClientMessage::CancelQueue,
        ClientMessage::GetRating {
            auth: AuthPayload {
                token: Some("jwt".into()),
                ..Default::default()
            },
        },
        ClientMessage::Ping,
    ]
}

/// `None` marks a variant deferred to S2 (see module doc): named, but with
/// no fixture instance constructed here.
fn server_message_name(m: &ServerMessage) -> Option<&'static str> {
    match m {
        ServerMessage::RoomCreated { .. } => Some("room_created"),
        ServerMessage::Joined { .. } => None,
        ServerMessage::Spectating { .. } => None,
        ServerMessage::Lobby { .. } => Some("lobby"),
        ServerMessage::GameStarted { .. } => None,
        ServerMessage::Update { .. } => None,
        ServerMessage::Rejected { .. } => Some("rejected"),
        ServerMessage::Error { .. } => Some("error"),
        ServerMessage::Mods { .. } => Some("mods"),
        ServerMessage::Queued { .. } => Some("queued"),
        ServerMessage::MatchFound { .. } => Some("match_found"),
        ServerMessage::Rating { .. } => Some("rating"),
        ServerMessage::RatingsUpdated { .. } => Some("ratings_updated"),
        ServerMessage::Pong => Some("pong"),
    }
}

fn server_message_fixtures() -> Vec<ServerMessage> {
    vec![
        ServerMessage::RoomCreated {
            code: "ABCDE".into(),
        },
        ServerMessage::Lobby {
            players: vec![],
            settings: RoomSettings {
                game_seconds: None,
                turn_seconds: Some(25),
                time_bank_seconds: None,
                rules: RuleParams::default(),
            },
        },
        ServerMessage::Rejected {
            error: CommandError::BidBelowFloor,
        },
        ServerMessage::Error {
            message: "boom".into(),
        },
        ServerMessage::Mods {
            ids: vec!["base".into(), "highroller".into()],
        },
        ServerMessage::Queued { size: 3 },
        ServerMessage::MatchFound {
            code: "ABCDE".into(),
        },
        ServerMessage::Rating {
            player_id: "id:u1".into(),
            mu: 25.0,
            sigma: 8.0,
            games: 4,
            wins: 1,
            display: 1040,
        },
        ServerMessage::RatingsUpdated {
            changes: vec![RatingChange {
                player_id: "id:u1".into(),
                mu: 27.5,
                sigma: 7.5,
                display: 1200,
                display_delta: 200,
            }],
        },
        ServerMessage::Pong,
    ]
}

#[test]
fn client_message_wire_format_matches_fixtures() {
    let fixtures = read_fixtures("client_message");
    for msg in client_message_fixtures() {
        assert_fixture(&fixtures, client_message_name(&msg), &msg);
    }
}

#[test]
fn server_message_wire_format_matches_fixtures() {
    let fixtures = read_fixtures("server_message");
    for msg in server_message_fixtures() {
        let Some(name) = server_message_name(&msg) else {
            unreachable!("server_message_fixtures() must only build named variants")
        };
        assert_fixture(&fixtures, name, &msg);
    }
}

/// Not part of CI: regenerates the committed fixture files. Run after
/// adding a new variant + match arm:
/// `cargo test -p parcello-protocol --test protocol_fixtures -- --ignored regenerate_fixtures`
#[test]
#[ignore]
fn regenerate_fixtures() {
    write_fixtures(
        "client_message",
        &client_message_fixtures()
            .iter()
            .map(|m| (client_message_name(m), serde_json::to_value(m).unwrap()))
            .collect::<Vec<_>>(),
    );
    write_fixtures(
        "server_message",
        &server_message_fixtures()
            .iter()
            .filter_map(|m| server_message_name(m).map(|n| (n, serde_json::to_value(m).unwrap())))
            .collect::<Vec<_>>(),
    );
}
