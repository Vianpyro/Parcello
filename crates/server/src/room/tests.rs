//! Session-layer tests for the room task: lifecycle, timers (AFK/bank/
//! windows/animation gates), bots, reconnect tokens, private trades,
//! feedback. Split out of `room.rs` for module size (2026-07); same
//! `mod tests` as before, just in its own file.

use super::*;
use crate::history::MemoryHistory;
use parcello_engine::Event;
use std::path::Path;

#[test]
fn room_codes_are_pronounceable() {
    const CONSONANTS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
    const VOWELS: &[u8] = b"AEIOUY";
    for _ in 0..300 {
        let code = random_code();
        let b = code.as_bytes();
        assert_eq!(b.len(), 5, "{code}");
        for i in [0, 2, 4] {
            assert!(
                CONSONANTS.contains(&b[i]),
                "pos {i} not a consonant: {code}"
            );
        }
        for i in [1, 3] {
            assert!(VOWELS.contains(&b[i]), "pos {i} not a vowel: {code}");
        }
    }
}

/// Post-game survey rules: rejected while the game runs, accepted once
/// finished, stored in history, one per seat.
#[tokio::test]
async fn feedback_only_after_game_end_and_once_per_seat() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let memory = Arc::new(MemoryHistory::new());
    let history: Arc<dyn GameHistory> = memory.clone();
    let code = create_room(&rooms, content, history, None, None, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");

    let feedback = |rating: u8| RoomCmd::Feedback {
        player_id: "guest:alice".into(),
        rating,
        comment: Some("  great game  ".into()),
    };
    // Mid-game: rejected.
    room.send(feedback(5)).await.expect("room task alive");
    // Bob resigns: alice wins, the game is finished.
    room.send(RoomCmd::Game {
        player_id: "guest:bob".into(),
        cmd: CommandKind::Resign,
    })
    .await
    .expect("room task alive");
    room.send(feedback(0)).await.expect("room task alive"); // invalid rating
    room.send(feedback(5)).await.expect("room task alive"); // accepted
    room.send(feedback(3)).await.expect("room task alive"); // duplicate

    tokio::time::sleep(Duration::from_millis(200)).await;
    let recorded: Vec<String> = memory
        .dump(&code)
        .into_iter()
        .filter(|l| l.starts_with("feedback"))
        .collect();
    assert_eq!(
        recorded,
        vec![r#"feedback player=guest:alice rating=5 comment=Some("great game")"#],
        "exactly one survey answer must be recorded, trimmed"
    );
    let errors = |rx: &mut mpsc::UnboundedReceiver<ServerMessage>| {
        let mut n = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, ServerMessage::Error { .. }) {
                n += 1;
            }
        }
        n
    };
    assert_eq!(
        errors(&mut client_rxs[0]),
        3,
        "mid-game, invalid rating, and duplicate must each error"
    );
}

/// After a game ends, `PlayAgain` restarts it in the same room for the
/// players still connected.
#[tokio::test]
async fn play_again_restarts_the_game() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(&rooms, content, history, None, None, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");
    // Bob resigns -> alice wins -> the game is finished.
    room.send(RoomCmd::Game {
        player_id: "guest:bob".into(),
        cmd: CommandKind::Resign,
    })
    .await
    .expect("room task alive");
    tokio::time::sleep(Duration::from_millis(150)).await;
    for rx in &mut rxs {
        while rx.try_recv().is_ok() {} // drain up to the finish
    }

    room.send(RoomCmd::PlayAgain {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");
    tokio::time::sleep(Duration::from_millis(150)).await;
    let mut new_start = None;
    for (i, rx) in rxs.iter_mut().enumerate() {
        let mut restarted = false;
        while let Ok(msg) = rx.try_recv() {
            if let ServerMessage::GameStarted { view, .. } = msg {
                restarted = true;
                new_start = Some(view.current);
            }
        }
        assert!(restarted, "seat {i} must be pulled into the new game");
    }
    let start = new_start.expect("GameStarted carries the new starter");
    let ids = ["guest:alice", "guest:bob"];

    // The new game is live: a move from the acting player is accepted.
    room.send(RoomCmd::Game {
        player_id: ids[start].into(),
        cmd: CommandKind::PlayMovementCard { value: 2 },
    })
    .await
    .expect("room task alive");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut moved = false;
    while let Ok(msg) = rxs[1 - start].try_recv() {
        if let ServerMessage::Update { events, .. } = msg
            && events
                .iter()
                .any(|e| matches!(e, Event::MovementCardPlayed { .. }))
        {
            moved = true;
        }
    }
    assert!(moved, "the replayed game must accept commands");
}

/// A guest seat can only be re-taken with the reconnect token issued at
/// first join (ADR-0008): knowing the name is no longer enough.
#[tokio::test]
async fn guest_seat_rejoin_requires_the_reconnect_token() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(&rooms, content, history, None, None, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let alice = || Identity {
        player_id: "guest:alice".to_string(),
        name: "alice".to_string(),
        spoofable: true,
    };
    let join = |reconnect: Option<String>| {
        let room = room.clone();
        let identity = alice();
        async move {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let (reply, joined) = oneshot::channel();
            room.send(RoomCmd::Join {
                identity,
                reconnect,
                tx,
                reply,
            })
            .await
            .expect("room task alive");
            let result = joined.await.expect("reply sent");
            let token = match rx.try_recv() {
                Ok(ServerMessage::Joined { reconnect, .. }) => reconnect,
                _ => None,
            };
            (result, token)
        }
    };

    let (result, token) = join(None).await;
    result.expect("first join accepted");
    let token = token.expect("token issued on join");

    let (hijack, _) = join(None).await;
    assert!(hijack.is_err(), "name alone must not re-take the seat");
    let (bad, _) = join(Some("wrong-token".into())).await;
    assert!(bad.is_err(), "wrong token must not re-take the seat");
    let (rejoin, _) = join(Some(token)).await;
    rejoin.expect("correct token re-takes the seat");
}

/// A third party must receive neither the trade event nor the offer in
/// its view (ADR-0007): per-seat routing through the real room task.
#[tokio::test]
async fn trade_offers_are_invisible_to_third_parties() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(&rooms, content, history, None, None, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob", "carol"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");
    room.send(RoomCmd::Game {
        player_id: "guest:alice".into(),
        cmd: CommandKind::ProposeTrade {
            to: "guest:bob".into(),
            give_cash: 50,
            give_tiles: vec![],
            receive_cash: 0,
            receive_tiles: vec![],
        },
    })
    .await
    .expect("room task alive");

    // First Update after GameStarted is the accepted trade proposal.
    let mut update_for = |seat: usize| {
        let rx = &mut client_rxs[seat];
        loop {
            match rx.try_recv() {
                Ok(ServerMessage::Update { events, view, .. }) => break (events, view),
                Ok(_) => {}
                Err(e) => panic!("seat {seat} never received an Update: {e}"),
            }
        }
    };
    // Give the room task time to process both commands.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (events, view) = update_for(1);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TradeProposed { .. })),
        "recipient must see the proposal"
    );
    assert_eq!(view.pending_trades.len(), 1);

    let (events, view) = update_for(2);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::TradeProposed { .. })),
        "third party must not see the trade event"
    );
    assert!(
        view.pending_trades.is_empty(),
        "third party must not see the offer"
    );
}

/// A disconnected player's turn is auto-played after the grace period
/// even with no `--turn-timeout` set, so an AFK player never stalls the
/// table. (We assert alice's roll is auto-played; whether her turn then
/// fully hands off depends on the game - an auction, say, legitimately
/// waits on the still-connected bob.)
#[tokio::test(start_paused = true)]
async fn disconnected_player_is_skipped_after_grace() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    // No --turn-timeout: the smart grace alone must skip the AFK seat.
    let code = create_room(&rooms, content, history, None, None, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");
    // The (seed-drawn) acting player drops off.
    let start = starting_seat(&mut client_rxs[0]).await;
    let ids = ["guest:alice", "guest:bob"];
    room.send(RoomCmd::Disconnect {
        player_id: ids[start].into(),
    })
    .await
    .expect("room task alive");

    // With no turn timeout, the grace alone must auto-play their move;
    // the other seat sees it without sending anything.
    let auto_played = tokio::time::timeout(Duration::from_mins(5), async {
        while let Some(msg) = client_rxs[1 - start].recv().await {
            if let ServerMessage::Update { events, .. } = msg
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MovementCardPlayed { .. }))
            {
                return true;
            }
        }
        false
    })
    .await
    .expect("an update must arrive before the mock-clock timeout");
    assert!(
        auto_played,
        "the disconnected player's turn must be auto-played after the grace"
    );
}

/// A time-boxed game ends on its own when the clock expires: both
/// players start equal (net worth tie), so seat 0 wins, and the game
/// becomes Finished (ADR-0010).
#[tokio::test(start_paused = true)]
async fn game_clock_finishes_by_net_worth() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(
        &rooms,
        content,
        history,
        None,
        None,
        Some(Duration::from_mins(1)),
    )
    .await
    .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    // Confirm the countdown reaches the clients on GameStarted.
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");

    let outcome = tokio::time::timeout(Duration::from_mins(10), async {
        let mut saw_countdown = false;
        while let Some(msg) = client_rxs[1].recv().await {
            match msg {
                ServerMessage::GameStarted { time_remaining, .. } => {
                    saw_countdown = time_remaining == Some(60);
                }
                ServerMessage::Update { events, .. } => {
                    if let Some(Event::TimeUp { winner }) =
                        events.iter().find(|e| matches!(e, Event::TimeUp { .. }))
                    {
                        return (saw_countdown, *winner);
                    }
                }
                _ => {}
            }
        }
        (saw_countdown, usize::MAX)
    })
    .await
    .expect("the game clock must fire before the mock-clock timeout");
    assert!(outcome.0, "GameStarted must carry the 60s countdown");
    assert_eq!(
        outcome.1, 0,
        "an equal-net-worth tie awards the lowest seat"
    );

    // The game is now finished: further play is rejected.
    room.send(RoomCmd::Game {
        player_id: "guest:alice".into(),
        cmd: CommandKind::PlayMovementCard { value: 1 },
    })
    .await
    .expect("room task alive");
    let rejected = tokio::time::timeout(Duration::from_mins(1), async {
        while let Some(msg) = client_rxs[0].recv().await {
            if matches!(msg, ServerMessage::Rejected { .. }) {
                return true;
            }
        }
        false
    })
    .await
    .expect("a rejection must arrive");
    assert!(rejected, "the game must be finished after the time limit");
}

/// `--turn-timeout` is surfaced to clients as `turn_seconds` on
/// `GameStarted` so they can show a per-turn countdown; off by default.
#[tokio::test(start_paused = true)]
async fn game_started_carries_turn_seconds() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(
        &rooms,
        content,
        history,
        Some(Duration::from_secs(30)),
        Some(Duration::from_secs(45)),
        None,
    )
    .await
    .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");

    let started = tokio::time::timeout(Duration::from_mins(1), async {
        while let Some(msg) = client_rxs[1].recv().await {
            if let ServerMessage::GameStarted {
                turn_seconds,
                time_remaining,
                time_bank_seconds,
                ..
            } = msg
            {
                return (turn_seconds, time_remaining, time_bank_seconds);
            }
        }
        (None, None, None)
    })
    .await
    .expect("GameStarted must arrive");
    assert_eq!(started.0, Some(30), "turn timer must reach the client");
    assert_eq!(started.1, None, "untimed game carries no game clock");
    assert_eq!(started.2, Some(45), "time bank must reach the client");
}

fn base_content() -> Arc<ResolvedContent> {
    Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    )
}

fn test_room(content: Arc<ResolvedContent>) -> Room {
    let engine = Engine::new(Arc::new(content.content.clone())).expect("engine builds");
    let settings = RoomSettings {
        game_seconds: None,
        turn_seconds: None,
        time_bank_seconds: None,
        rules: content.content.rules.clone(),
    };
    Room {
        code: "TESTS".into(),
        content,
        engine,
        seats: Vec::new(),
        phase: Phase::Lobby,
        history: Arc::new(MemoryHistory::new()),
        settings,
        game_deadline: None,
        bot_counter: 0,
        banks: Vec::new(),
        bid_deadline: None,
        vote_deadline: None,
        seq: 0,
        acked: Vec::new(),
        anim_broadcast_at: tokio::time::Instant::now(),
        table_settled_at: None,
        acting_settled_at: None,
        bid_gate: false,
        vote_gate: false,
    }
}

fn human_seat(id: &str) -> Seat {
    Seat {
        identity: Identity {
            player_id: id.into(),
            name: id.into(),
            spoofable: true,
        },
        tx: None,
        reconnect: String::new(),
        feedback_given: false,
        is_bot: false,
    }
}

/// A bot seat produces the engine heuristic's move for its own turn: a
/// fresh game opens on seat 0 by playing a movement card (ADR-0014,
/// ADR-0017).
#[test]
fn bot_seat_acts_on_its_turn() {
    let content = base_content();
    let engine = Engine::new(Arc::new(content.content.clone())).unwrap();
    let mut state = engine.new_game(
        vec![
            ("bot:1".into(), "Bot 1".into()),
            ("guest:al".into(), "Al".into()),
        ],
        7,
    );
    state.current = 0; // seed-drawn starter (2026-07); the test wants the bot
    let mut room = test_room(content);
    room.seats = vec![human_seat("bot:1"), human_seat("guest:al")];
    room.seats[0].is_bot = true;
    room.phase = Phase::Active(state);
    assert!(matches!(
        room.next_bot_action(),
        Some((id, CommandKind::PlayMovementCard { .. })) if id == "bot:1"
    ));
}

/// Bots fill a room but never lock a human out: joining a full room
/// evicts a bot to seat the newcomer (ADR-0014).
#[test]
fn bots_yield_their_seat_to_a_joining_human() {
    let mut room = test_room(base_content());
    room.seats.push(human_seat("guest:host"));
    while room.seats.len() < MAX_PLAYERS {
        room.handle_add_bot("guest:host");
    }
    assert_eq!(room.seats.len(), MAX_PLAYERS);
    let bots_before = room.seats.iter().filter(|s| s.is_bot).count();

    let (tx, _rx) = mpsc::unbounded_channel();
    room.handle_join(
        Identity {
            player_id: "guest:new".into(),
            name: "New".into(),
            spoofable: true,
        },
        None,
        &tx,
    )
    .expect("a human must displace a bot in a full room");

    assert_eq!(room.seats.len(), MAX_PLAYERS, "room stays at capacity");
    assert!(
        room.seats
            .iter()
            .any(|s| s.identity.player_id == "guest:new"),
        "the human took a seat"
    );
    assert_eq!(
        room.seats.iter().filter(|s| s.is_bot).count(),
        bots_before - 1,
        "exactly one bot yielded"
    );
    assert!(!room.seats[0].is_bot, "the host keeps seat 0");
}

/// Only the host may add bots, and only in the lobby.
#[test]
fn add_bot_is_host_and_lobby_gated() {
    let mut room = test_room(base_content());
    room.seats.push(human_seat("guest:host"));
    room.seats.push(human_seat("guest:other"));
    room.handle_add_bot("guest:other"); // not the host
    assert!(room.seats.iter().all(|s| !s.is_bot), "non-host cannot add");
    room.handle_add_bot("guest:host");
    assert_eq!(room.seats.iter().filter(|s| s.is_bot).count(), 1);
    room.handle_remove_bot("guest:host");
    assert!(room.seats.iter().all(|s| !s.is_bot), "host removed the bot");
}

/// Only the host may change settings, and the server clamps absurd wire
/// values (ADR-0015).
#[test]
fn configure_is_host_gated_and_clamped() {
    let mut room = test_room(base_content());
    room.seats.push(human_seat("guest:host"));
    room.seats.push(human_seat("guest:other"));
    let before = room.settings.rules.starting_balance;

    let mut s = room.settings.clone();
    s.rules.starting_balance = 5000;
    room.handle_configure("guest:other", s); // not the host
    assert_eq!(
        room.settings.rules.starting_balance, before,
        "a non-host must not change settings"
    );

    let mut s = room.settings.clone();
    s.rules.starting_balance = i64::MAX;
    s.rules.max_houses_per_property = 200;
    s.rules.velocity_min = 0; // below the floor (GameContent::validate needs >= 1)
    s.rules.velocity_max = 0; // must end up strictly above velocity_min
    s.rules.win_victory_points = 999_999;
    s.game_seconds = Some(1); // below the 60s floor
    s.turn_seconds = Some(25);
    s.time_bank_seconds = Some(10_000); // above the 600s ceiling
    room.handle_configure("guest:host", s);
    assert_eq!(room.settings.rules.starting_balance, 1_000_000, "clamped");
    assert_eq!(room.settings.rules.max_houses_per_property, 5, "house cap");
    assert_eq!(room.settings.rules.velocity_min, 1, "velocity floor");
    assert!(
        room.settings.rules.velocity_max > room.settings.rules.velocity_min,
        "velocity_max must stay strictly above velocity_min"
    );
    assert_eq!(
        room.settings.rules.win_victory_points, 500,
        "victory point ceiling"
    );
    assert_eq!(room.settings.game_seconds, Some(60), "game floor");
    assert_eq!(room.settings.turn_seconds, Some(25));
    assert_eq!(room.settings.time_bank_seconds, Some(600), "bank ceiling");
}

/// The game starts with the host's chosen rules: a rebuilt engine deals
/// the configured starting balance (ADR-0015).
#[test]
fn start_game_applies_the_host_rules() {
    let mut room = test_room(base_content());
    room.seats.push(human_seat("guest:host"));
    room.seats.push(human_seat("guest:bob"));
    room.settings.rules.starting_balance = 777;
    assert!(room.start_game("guest:host"), "game must start");
    let Phase::Active(st) = &room.phase else {
        panic!("game should be active");
    };
    assert!(
        st.players.iter().all(|p| p.cash == 777),
        "every player starts with the host's balance"
    );
}

/// With a paused clock and zero player input, the room task must play
/// canonical actions on its own once the per-turn deadline passes.
#[tokio::test(start_paused = true)]
async fn afk_timer_plays_canonical_actions() {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(
        &rooms,
        content,
        history,
        Some(Duration::from_secs(30)),
        None, // no time bank: a plain hard stop at the turn limit
        None,
    )
    .await
    .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");

    let auto_played = tokio::time::timeout(Duration::from_mins(5), async {
        while let Some(msg) = client_rxs[1].recv().await {
            if let ServerMessage::Update { events, .. } = msg
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MovementCardPlayed { .. }))
            {
                return true;
            }
        }
        false
    })
    .await
    .expect("an update must arrive before the mock-clock timeout");
    assert!(auto_played, "AFK timer should move for the idle player");
}

/// Setup shared by the time-bank tests below: a two-player room with the
/// given turn limit and bank, started and ready for seat 0 (alice) to
/// act.
async fn started_room_with_bank(
    turn_timeout: Option<Duration>,
    time_bank: Option<Duration>,
) -> (
    mpsc::Sender<RoomCmd>,
    Vec<mpsc::UnboundedReceiver<ServerMessage>>,
    usize,
) {
    let content = Arc::new(
        parcello_mods::resolve(
            Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
            &["base".to_string()],
        )
        .expect("base mod loads"),
    );
    let rooms = Rooms::default();
    let history: Arc<dyn GameHistory> = Arc::new(MemoryHistory::new());
    let code = create_room(&rooms, content, history, turn_timeout, time_bank, None)
        .await
        .expect("room created");
    let room = rooms.read().await.get(&code).cloned().expect("room handle");

    let mut client_rxs = Vec::new();
    for name in ["alice", "bob"] {
        let (tx, rx) = mpsc::unbounded_channel();
        let (reply, joined) = oneshot::channel();
        room.send(RoomCmd::Join {
            identity: Identity {
                player_id: format!("guest:{name}"),
                name: name.to_string(),
                spoofable: true,
            },
            reconnect: None,
            tx,
            reply,
        })
        .await
        .expect("room task alive");
        joined.await.expect("reply sent").expect("join accepted");
        client_rxs.push(rx);
    }
    room.send(RoomCmd::Start {
        player_id: "guest:alice".into(),
    })
    .await
    .expect("room task alive");
    let start = starting_seat(&mut client_rxs[0]).await;
    (room, client_rxs, start)
}

/// A connected seat that overruns the plain turn window but acts before
/// its bank is dry is NOT auto-played; the overage is permanently
/// deducted from its bank instead (ADR-0023).
#[tokio::test(start_paused = true)]
async fn time_bank_absorbs_overrun_without_auto_play() {
    let (room, mut client_rxs, start) =
        started_room_with_bank(Some(Duration::from_secs(5)), Some(Duration::from_secs(20))).await;
    let ids = ["guest:alice", "guest:bob"];
    let other = 1 - start;

    // The starter stalls 7s (5s over the turn limit, well inside the
    // 20s bank) then moves - the bank absorbs the 2s overage.
    tokio::time::sleep(Duration::from_secs(7)).await;
    room.send(RoomCmd::Game {
        player_id: ids[start].into(),
        cmd: CommandKind::PlayMovementCard { value: 2 },
    })
    .await
    .expect("room task alive");

    let (played_herself, banks) = tokio::time::timeout(Duration::from_mins(1), async {
        while let Some(msg) = client_rxs[other].recv().await {
            if let ServerMessage::Update { events, banks, .. } = msg {
                let played = events
                    .iter()
                    .any(|e| matches!(e, Event::MovementCardPlayed { .. }));
                if played {
                    return (true, banks);
                }
            }
        }
        (false, None)
    })
    .await
    .expect("an update must arrive");
    assert!(played_herself, "the starter's own move must be accepted");
    let mut expected = vec![20u64, 20];
    expected[start] = 18;
    assert_eq!(
        banks,
        Some(expected),
        "the starter's bank drains by the 2s overage; the other is untouched"
    );
}

/// Once the plain turn window AND the whole bank are exhausted, the
/// canonical action auto-plays and the bank reads zero (ADR-0023).
#[tokio::test(start_paused = true)]
async fn time_bank_hard_stops_when_exhausted() {
    let (_room, mut client_rxs, start) =
        started_room_with_bank(Some(Duration::from_secs(5)), Some(Duration::from_secs(3))).await;
    let other = 1 - start;

    let (auto_played, banks) = tokio::time::timeout(Duration::from_mins(5), async {
        while let Some(msg) = client_rxs[other].recv().await {
            if let ServerMessage::Update { events, banks, .. } = msg
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MovementCardPlayed { .. }))
            {
                return (true, banks);
            }
        }
        (false, None)
    })
    .await
    .expect("an update must arrive before the mock-clock timeout");
    assert!(auto_played, "AFK timer should move once the bank is dry");
    let mut expected = vec![3u64, 3];
    expected[start] = 0;
    assert_eq!(banks, Some(expected), "the starter's bank is fully spent");
}

/// A disconnected seat is skipped after `DISCONNECTED_GRACE` alone; the
/// time bank never extends that (ADR-0023: pulling the plug earns no
/// extra time).
#[tokio::test(start_paused = true)]
async fn disconnected_seat_ignores_the_time_bank() {
    let (room, mut client_rxs, start) = started_room_with_bank(
        Some(Duration::from_secs(5)),
        Some(Duration::from_secs(1000)),
    )
    .await;
    let ids = ["guest:alice", "guest:bob"];
    let other = 1 - start;
    room.send(RoomCmd::Disconnect {
        player_id: ids[start].into(),
    })
    .await
    .expect("room task alive");

    let auto_played = tokio::time::timeout(Duration::from_mins(2), async {
        while let Some(msg) = client_rxs[other].recv().await {
            if let ServerMessage::Update { events, .. } = msg
                && events
                    .iter()
                    .any(|e| matches!(e, Event::MovementCardPlayed { .. }))
            {
                return true;
            }
        }
        false
    })
    .await
    .expect("must move within DISCONNECTED_GRACE, far short of the 1000s bank");
    assert!(auto_played, "a disconnected seat must not draw on its bank");
}

/// Reads this client's stream until `GameStarted` and returns the
/// seed-drawn starting seat (2026-07: no longer always the host) -
/// fixtures can no longer assume seat 0 opens the game.
async fn starting_seat(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> usize {
    tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::GameStarted { view, .. } =
                rx.recv().await.expect("room task alive")
            {
                return view.current;
            }
        }
    })
    .await
    .expect("GameStarted must arrive")
}

/// Waits for the next `Update` and returns its view, skipping any other
/// message type (e.g. a stray `Rejected`).
async fn next_view(rx: &mut mpsc::UnboundedReceiver<ServerMessage>) -> Box<ClientView> {
    loop {
        if let ServerMessage::Update { view, .. } = rx.recv().await.expect("room task alive") {
            return view;
        }
    }
}

/// Plays movement cards/ends turns until someone lands on an unowned
/// property and a sealed-bid window opens (ADR-0018); the base mod
/// board is almost entirely properties, so this converges in very few
/// attempts. Also steers around jail (ADR-0024: ascending Legal Route,
/// then the route's locked front card) since it is otherwise reachable
/// while crawling low card values. Returns the seat whose turn it now
/// is (the discoverer).
async fn roll_until_blind_auction_opens(
    room: &mpsc::Sender<RoomCmd>,
    rx: &mut mpsc::UnboundedReceiver<ServerMessage>,
    ids: [&str; 2],
    mut current: usize,
) -> usize {
    tokio::time::timeout(Duration::from_mins(5), async {
        // A fresh game deals every seat the full hand; the base mod's
        // velocity_min (currently 2, mods/base/data/rules.toml) is
        // always available for the very first move - keep these two
        // literals in step with that file if it ever changes.
        let mut jailed = false;
        let mut jail_route: Option<Vec<u8>> = None;
        let mut value = 2u8;
        loop {
            let cmd = if jailed {
                CommandKind::ChooseLegalRoute {
                    order: vec![2, 3, 4, 5, 6],
                }
            } else if let Some(route) = &jail_route {
                CommandKind::PlayMovementCard { value: route[0] }
            } else {
                CommandKind::PlayMovementCard { value }
            };
            room.send(RoomCmd::Game {
                player_id: ids[current].into(),
                cmd,
            })
            .await
            .expect("room task alive");
            let view = next_view(rx).await;
            if matches!(view.turn, TurnPhase::BlindAuction { .. }) {
                return current;
            }
            current = view.current;
            room.send(RoomCmd::Game {
                player_id: ids[current].into(),
                cmd: CommandKind::EndTurn,
            })
            .await
            .expect("room task alive");
            let view = next_view(rx).await;
            current = view.current;
            jailed = view.players[current].in_jail;
            jail_route = view.players[current].jail_route.clone();
            value = *view.players[current].hand.first().unwrap_or(&2);
        }
    })
    .await
    .expect("must land on an unowned property within the timeout")
}

/// A silent seat is auto-abstained once the sealed-bid window's own 5s
/// deadline fires (ADR-0018) - a separate, parallel timer from the turn
/// clock/time bank (`acting_seat()` returns `None` for the whole
/// phase, so neither of those is touched while it's armed).
#[tokio::test(start_paused = true)]
async fn sealed_bid_window_auto_abstains_a_silent_seat() {
    let (room, mut client_rxs, start) = started_room_with_bank(None, None).await;
    let ids = ["guest:alice", "guest:bob"];

    let discoverer = roll_until_blind_auction_opens(&room, &mut client_rxs[1], ids, start).await;

    // The discoverer bids; the other seat stays silent.
    room.send(RoomCmd::Game {
        player_id: ids[discoverer].into(),
        cmd: CommandKind::SubmitBlindBid { amount: 0 },
    })
    .await
    .expect("room task alive");

    let resolved = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BlindAuctionResolved { .. }))
            {
                return true;
            }
        }
    })
    .await
    .expect("must resolve within the mock-clock timeout");
    assert!(
        resolved,
        "the silent seat must be auto-abstained at the deadline"
    );
}

/// When every living seat bids before the window's own deadline, it
/// resolves immediately as a direct result of the last bid - the 5s
/// timer is a fallback, not a wait (ADR-0018).
#[tokio::test(start_paused = true)]
async fn sealed_bid_window_resolves_early_once_everyone_has_bid() {
    let (room, mut client_rxs, start) = started_room_with_bank(None, None).await;
    let ids = ["guest:alice", "guest:bob"];

    let discoverer = roll_until_blind_auction_opens(&room, &mut client_rxs[1], ids, start).await;
    let other = 1 - discoverer;

    room.send(RoomCmd::Game {
        player_id: ids[discoverer].into(),
        cmd: CommandKind::SubmitBlindBid { amount: 0 },
    })
    .await
    .expect("room task alive");
    let _ = next_view(&mut client_rxs[1]).await;
    room.send(RoomCmd::Game {
        player_id: ids[other].into(),
        cmd: CommandKind::SubmitBlindBid { amount: 0 },
    })
    .await
    .expect("room task alive");

    // A tight bound with no room for the 5s fallback to have
    // contributed: the mock clock only auto-advances when nothing else
    // is ready, so resolving inside it proves the window didn't wait.
    let resolved = tokio::time::timeout(Duration::from_millis(50), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BlindAuctionResolved { .. }))
            {
                return true;
            }
        }
    })
    .await
    .expect("must resolve immediately, not after the fallback timer");
    assert!(resolved);
}

/// Acks "rendered through `through_seq`" for `player` (ADR-0028).
/// `u64::MAX` acks everything sent so far - the server clamps.
async fn ack(room: &mpsc::Sender<RoomCmd>, player: &str) {
    room.send(RoomCmd::AnimationDone {
        player_id: player.into(),
        through_seq: u64::MAX,
    })
    .await
    .expect("room task alive");
}

/// Nobody acks: the sealed-bid window's 5s clock must not start until
/// the animation-ack hard cap passes (ADR-0028) - auto-abstain
/// resolution lands at cap + window, not at window.
#[tokio::test(start_paused = true)]
async fn bid_window_waits_for_animation_acks_before_its_clock_starts() {
    let (room, mut client_rxs, start) = started_room_with_bank(None, None).await;
    let ids = ["guest:alice", "guest:bob"];

    roll_until_blind_auction_opens(&room, &mut client_rxs[1], ids, start).await;
    let opened_at = tokio::time::Instant::now();

    let resolved_at = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BlindAuctionResolved { .. }))
            {
                return tokio::time::Instant::now();
            }
        }
    })
    .await
    .expect("must resolve within the mock-clock timeout");
    let elapsed = resolved_at - opened_at;
    assert!(
        elapsed
            >= (ANIM_ACK_CAP + BID_WINDOW)
                .checked_sub(Duration::from_millis(200))
                .unwrap(),
        "window clock must wait for the ack cap, resolved after {elapsed:?}"
    );
}

/// Once every connected seat acks the opening Update, the window's 5s
/// clock starts immediately - well before the hard cap (ADR-0028).
#[tokio::test(start_paused = true)]
async fn bid_window_clock_starts_early_once_everyone_acks() {
    let (room, mut client_rxs, start) = started_room_with_bank(None, None).await;
    let ids = ["guest:alice", "guest:bob"];

    roll_until_blind_auction_opens(&room, &mut client_rxs[1], ids, start).await;
    let opened_at = tokio::time::Instant::now();
    ack(&room, "guest:alice").await;
    ack(&room, "guest:bob").await;

    let resolved_at = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BlindAuctionResolved { .. }))
            {
                return tokio::time::Instant::now();
            }
        }
    })
    .await
    .expect("must resolve within the mock-clock timeout");
    let elapsed = resolved_at - opened_at;
    assert!(
        elapsed
            < (ANIM_ACK_CAP + BID_WINDOW)
                .checked_sub(Duration::from_secs(1))
                .unwrap(),
        "acks must release the window early, resolved after {elapsed:?}"
    );
}

/// The turn clock (blitz/AFK) starts from the acting seat's own render
/// ack, not from the broadcast (ADR-0028): a silent seat's canonical
/// auto-play lands at cap + turn limit.
#[tokio::test(start_paused = true)]
async fn turn_clock_waits_for_the_acting_seats_ack() {
    let (room, mut client_rxs, start) =
        started_room_with_bank(Some(Duration::from_secs(5)), None).await;
    let ids = ["guest:alice", "guest:bob"];

    let discoverer = roll_until_blind_auction_opens(&room, &mut client_rxs[1], ids, start).await;
    let other = 1 - discoverer;
    // Resolve the window fast (both bid); the discoverer then sits in
    // AwaitEnd with the 5s turn clock gated on their (never-sent) ack.
    for seat in [discoverer, other] {
        room.send(RoomCmd::Game {
            player_id: ids[seat].into(),
            cmd: CommandKind::SubmitBlindBid { amount: 0 },
        })
        .await
        .expect("room task alive");
    }
    let resolved_at = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BlindAuctionResolved { .. }))
            {
                return tokio::time::Instant::now();
            }
        }
    })
    .await
    .expect("window resolves once everyone has bid");

    let advanced_at = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::TurnStarted { .. }))
            {
                return tokio::time::Instant::now();
            }
        }
    })
    .await
    .expect("the canonical EndTurn must eventually auto-play");
    let elapsed = advanced_at - resolved_at;
    assert!(
        elapsed
            >= (ANIM_ACK_CAP + Duration::from_secs(5))
                .checked_sub(Duration::from_millis(200))
                .unwrap(),
        "turn clock must wait for the acting seat's ack cap, fired after {elapsed:?}"
    );
}

/// Spawns a room with alice already jailed and ready to offer a bribe.
/// Legal Route/Corruption are deterministic player choices, not RNG
/// outcomes worth re-deriving through a gameplay crawl (unlike
/// `roll_until_blind_auction_opens`), so the state is seeded directly.
fn jailed_room() -> (
    mpsc::Sender<RoomCmd>,
    Vec<mpsc::UnboundedReceiver<ServerMessage>>,
) {
    let content = base_content();
    let engine = Engine::new(Arc::new(content.content.clone())).expect("engine builds");
    let mut state = engine.new_game(
        vec![
            ("guest:alice".into(), "Alice".into()),
            ("guest:bob".into(), "Bob".into()),
        ],
        7,
    );
    state.current = 0; // seed-drawn starter (2026-07); the script needs alice
    state.players[0].jailed = true;
    state.turn = TurnPhase::AwaitMove;

    let settings = RoomSettings {
        game_seconds: None,
        turn_seconds: None,
        time_bank_seconds: None,
        rules: content.content.rules.clone(),
    };
    let mut room = Room {
        code: "TESTS".into(),
        content,
        engine,
        seats: vec![human_seat("guest:alice"), human_seat("guest:bob")],
        phase: Phase::Active(state),
        history: Arc::new(MemoryHistory::new()),
        settings,
        game_deadline: None,
        bot_counter: 0,
        banks: vec![0, 0],
        bid_deadline: None,
        vote_deadline: None,
        seq: 0,
        acked: Vec::new(),
        anim_broadcast_at: tokio::time::Instant::now(),
        table_settled_at: None,
        acting_settled_at: None,
        bid_gate: false,
        vote_gate: false,
    };
    let mut client_rxs = Vec::new();
    for seat in &mut room.seats {
        let (tx, rx) = mpsc::unbounded_channel();
        seat.tx = Some(tx);
        client_rxs.push(rx);
    }
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(room.run(rx, Rooms::default()));
    (tx, client_rxs)
}

/// Same setup as `jailed_room`, but with a plain turn limit shorter
/// than `JAIL_DECISION_SECS` - to prove the floor actually overrides
/// the room's own setting rather than merely being the default.
fn jailed_room_with_short_turn_limit() -> (
    mpsc::Sender<RoomCmd>,
    Vec<mpsc::UnboundedReceiver<ServerMessage>>,
) {
    let content = base_content();
    let engine = Engine::new(Arc::new(content.content.clone())).expect("engine builds");
    let mut state = engine.new_game(
        vec![
            ("guest:alice".into(), "Alice".into()),
            ("guest:bob".into(), "Bob".into()),
        ],
        7,
    );
    state.current = 0; // seed-drawn starter (2026-07); the script needs alice
    state.players[0].jailed = true;
    state.turn = TurnPhase::AwaitMove;

    let settings = RoomSettings {
        game_seconds: None,
        turn_seconds: Some(5),
        time_bank_seconds: None,
        rules: content.content.rules.clone(),
    };
    let mut room = Room {
        code: "TESTS".into(),
        content,
        engine,
        seats: vec![human_seat("guest:alice"), human_seat("guest:bob")],
        phase: Phase::Active(state),
        history: Arc::new(MemoryHistory::new()),
        settings,
        game_deadline: None,
        bot_counter: 0,
        banks: vec![0, 0],
        bid_deadline: None,
        vote_deadline: None,
        seq: 0,
        acked: Vec::new(),
        anim_broadcast_at: tokio::time::Instant::now(),
        table_settled_at: None,
        acting_settled_at: None,
        bid_gate: false,
        vote_gate: false,
    };
    let mut client_rxs = Vec::new();
    for seat in &mut room.seats {
        let (tx, rx) = mpsc::unbounded_channel();
        seat.tx = Some(tx);
        client_rxs.push(rx);
    }
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(room.run(rx, Rooms::default()));
    (tx, client_rxs)
}

/// A jailed seat choosing its exit gets a floored 20s decision window
/// even when the room's plain turn limit is much shorter: the
/// canonical Legal Route must not auto-play at the room's 5s limit,
/// only once the `JAIL_DECISION_SECS` floor passes (2026-07 playtest
/// feedback - the ordinary blitz turn was too short for this decision).
#[tokio::test(start_paused = true)]
async fn jail_decision_gets_the_extended_floor_not_the_room_turn_limit() {
    let (_room, mut client_rxs) = jailed_room_with_short_turn_limit();

    // Nothing must auto-play within the room's plain 5s limit.
    let early = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::LegalRouteChosen { .. }))
            {
                return true;
            }
        }
    })
    .await;
    assert!(
        early.is_err(),
        "must not auto-play the jail decision at the room's plain 5s limit"
    );

    // It does fire once the extended floor passes.
    let resolved = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::LegalRouteChosen { .. }))
            {
                return true;
            }
        }
    })
    .await
    .expect("must auto-play once the extended floor passes");
    assert!(resolved);
}

/// A silent opponent is auto-rejected once the bribe vote's own 5s
/// deadline fires (ADR-0024) - the `BribeVote` equivalent of the
/// sealed-bid window tests above.
#[tokio::test(start_paused = true)]
async fn bribe_vote_window_auto_rejects_a_silent_seat() {
    let (room, mut client_rxs) = jailed_room();

    room.send(RoomCmd::Game {
        player_id: "guest:alice".into(),
        cmd: CommandKind::OfferBribe { amount: 100 },
    })
    .await
    .expect("room task alive");

    let resolved = tokio::time::timeout(Duration::from_mins(1), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BribeResolved { .. }))
            {
                return true;
            }
        }
    })
    .await
    .expect("must resolve within the mock-clock timeout");
    assert!(
        resolved,
        "the silent opponent must be auto-rejected at the deadline"
    );
}

/// When the lone opponent votes before the window's own deadline, it
/// resolves immediately as a direct result of that vote - the 5s timer
/// is a fallback, not a wait (mirrors the sealed-bid equivalent).
#[tokio::test(start_paused = true)]
async fn bribe_vote_window_resolves_early_once_everyone_has_voted() {
    let (room, mut client_rxs) = jailed_room();

    room.send(RoomCmd::Game {
        player_id: "guest:alice".into(),
        cmd: CommandKind::OfferBribe { amount: 100 },
    })
    .await
    .expect("room task alive");
    let _ = next_view(&mut client_rxs[1]).await;

    room.send(RoomCmd::Game {
        player_id: "guest:bob".into(),
        cmd: CommandKind::VoteOnBribe { accept: true },
    })
    .await
    .expect("room task alive");

    let resolved = tokio::time::timeout(Duration::from_millis(50), async {
        loop {
            if let ServerMessage::Update { events, .. } =
                client_rxs[1].recv().await.expect("room task alive")
                && events
                    .iter()
                    .any(|e| matches!(e, Event::BribeResolved { .. }))
            {
                return true;
            }
        }
    })
    .await
    .expect("must resolve immediately, not after the fallback timer");
    assert!(resolved);
}

#[test]
fn sanitize_comment_strips_dangerous_content_and_bounds_length() {
    // Control chars (incl. ESC 0x1b and newlines) are removed - a stored
    // comment can never carry a terminal-escape or log-injection payload.
    assert_eq!(
        sanitize_comment("good\x1b[31mgame\ngg\r\n\0"),
        "good[31mgamegg"
    );
    // Unicode bidi override + zero-width joiner are stripped (display spoof).
    assert_eq!(sanitize_comment("a\u{202E}b\u{200B}c"), "abc");
    // Surrounding whitespace is trimmed; a comment that is only junk collapses
    // to empty (the caller then drops it to None).
    assert_eq!(sanitize_comment("  hi  "), "hi");
    assert_eq!(sanitize_comment("\u{200B}\x1b\n"), "");
    // Length is bounded to COMMENT_MAX_CHARS scalar values.
    let long = "x".repeat(COMMENT_MAX_CHARS + 50);
    assert_eq!(sanitize_comment(&long).chars().count(), COMMENT_MAX_CHARS);
    // Plain multibyte text survives intact (non-ASCII is not "unsafe").
    assert_eq!(sanitize_comment("bien joue !"), "bien joue !");
}
