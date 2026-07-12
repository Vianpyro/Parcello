//! Jail entry and the three exits (ADR-0024): jail cards, Legal Routes
//! and their rent freeze, Corruption bribes and vote windows.
//! Fixtures: `tests/common/mod.rs`.

mod common;
use common::*;

#[test]
fn jail_card_is_held_then_spent_to_leave_jail() {
    let card = CardDef {
        id: "jail_free".into(),
        text: "Get out of jail free.".into(),
        effect: CardEffect::GetOutOfJail,
    };
    let engine = engine_with(card_board(vec![card]));
    let st = two_players(&engine);

    // Landing on chance banks the card instead of resolving an effect.
    let (mut st, ev) = play(&engine, &st, "p0", 3);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailCardReceived { player: 0 }))
    );
    assert_eq!(st.players[0].jail_cards, 1);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);

    // Test shortcut: place p0 in jail with the turn back in hand.
    st.players[0].position = 4;
    st.players[0].jailed = true;
    st.turn = TurnPhase::AwaitMove;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::UseJailCard));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailCardUsed { player: 0 }))
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::LeftJail { player: 0 }))
    );
    assert_eq!(st.players[0].jail_cards, 0);
    assert!(!st.players[0].jailed);
    assert_eq!(st.players[0].cash, 1500, "the card costs nothing");
    assert_eq!(
        st.turn,
        TurnPhase::AwaitMove,
        "player still plays a card normally"
    );

    // Card 3 is already spent from the first move; 2 is still in hand.
    let (st, _) = play(&engine, &st, "p0", 2);
    assert_eq!(st.players[0].position, 1);
}

#[test]
fn jail_card_rejections_never_mutate() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);

    // Not in jail.
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::UseJailCard))
            .unwrap_err(),
        CommandError::NotInJail
    );
    // In jail without a card.
    st.players[0].position = 5;
    st.players[0].jailed = true;
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::UseJailCard))
            .unwrap_err(),
        CommandError::NoJailCard
    );
    assert!(st.players[0].jailed);
}

#[test]
fn legal_route_rejects_a_non_permutation_order_without_mutating() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].position = 5; // jail tile
    st.players[0].jailed = true;

    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::ChooseLegalRoute {
                        order: vec![1, 2, 3, 4] // missing 5: not a full permutation
                    }
                )
            )
            .unwrap_err(),
        CommandError::InvalidRoute
    );
    assert!(st.players[0].jailed, "rejection must not mutate");
    assert_eq!(st.players[0].jail_route, None);
}

#[test]
fn legal_route_freezes_rent_and_freeze_ends_when_route_completes() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // p0's navy tile: singleton full group, rent 20
    st.players[0].position = 5; // jail tile
    st.players[0].jailed = true;

    let order: Vec<u8> = (1..=5).collect();
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::ChooseLegalRoute {
                order: order.clone(),
            },
        ),
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::LeftJail { player: 0 }))
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::LegalRouteChosen { player: 0, order: o } if *o == order
    )));
    assert!(!st.players[0].jailed);
    assert_eq!(st.players[0].jail_route, Some(vec![2, 3, 4, 5]));
    assert_eq!(
        st.players[0].position, 6,
        "route front (1) moved p0 onto their own navy tile"
    );

    // Hand off to p1: while p0's route is active, p1 pays no rent landing
    // on p0's tile.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    let mut st = st;
    st.players[1].hand = vec![5];
    st.players[1].position = 1; // 1 + 5 = 6
    let (st, ev) = play(&engine, &st, "p1", 5);
    assert!(
        !ev.iter().any(|e| matches!(e, Event::RentPaid { .. })),
        "visitors play free while the owner is mid-route"
    );
    assert_eq!(st.players[1].cash, 1500, "no rent charged");
    assert_eq!(st.turn, TurnPhase::AwaitEnd);

    // Once the route ends, rent resumes normally.
    let mut st = st;
    st.players[0].jail_route = None;
    st.players[1].hand = vec![5];
    st.players[1].position = 1;
    st.turn = TurnPhase::AwaitMove;
    let (st, ev) = play(&engine, &st, "p1", 5);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentPaid {
            tile: 6,
            amount: 20,
            ..
        }
    )));
    assert_eq!(st.players[1].cash, 1500 - 20);
}

#[test]
fn corruption_bribe_succeeds_when_the_lone_opponent_accepts() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].position = 5; // jail tile
    st.players[0].jailed = true;

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::OfferBribe { amount: 100 }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BribeOffered {
            player: 0,
            amount: 100
        }
    )));
    assert!(matches!(
        st.turn,
        TurnPhase::BribeVote {
            briber: 0,
            amount: 100,
            ..
        }
    ));

    // The briber cannot vote on their own bribe.
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::VoteOnBribe { accept: true }))
            .unwrap_err(),
        CommandError::NotYourTurn
    );

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::VoteOnBribe { accept: true }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BribeResolved {
            briber: 0,
            amount: 100,
            succeeded: true,
            accepts: 1,
            total: 1,
        }
    )));
    assert!(!st.players[0].jailed);
    assert_eq!(st.turn, TurnPhase::AwaitMove);
    assert_eq!(
        st.players[0].cash,
        1500 - 100,
        "the lone opponent takes the whole amount"
    );
    assert_eq!(st.players[1].cash, 1500 + 100);
}

#[test]
fn corruption_bribe_succeeds_with_majority_and_splits_by_floor_division() {
    let engine = engine_with(plain_board());
    let st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
            ("p3".into(), "P3".into()),
        ],
        7,
    );
    let mut st = st;
    st.current = 0; // seed-drawn starter (2026-07); the script needs p0
    st.players[0].position = 5;
    st.players[0].jailed = true;

    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::OfferBribe { amount: 100 }),
    );

    // Double-voting rejects without mutating the tally.
    let (st, _) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::VoteOnBribe { accept: true }),
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::VoteOnBribe { accept: false }))
            .unwrap_err(),
        CommandError::AlreadyVoted
    );

    let (st, _) = step(
        &engine,
        &st,
        cmd("p2", CommandKind::VoteOnBribe { accept: false }),
    );
    // 2 of 3 opponents accept: strictly more than half, success without unanimity.
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p3", CommandKind::VoteOnBribe { accept: true }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BribeResolved {
            briber: 0,
            amount: 100,
            succeeded: true,
            accepts: 2,
            total: 3,
        }
    )));
    assert!(!st.players[0].jailed);
    // 100 / 3 = 33 floored; only 99 leaves the briber, the remainder stays.
    assert_eq!(st.players[0].cash, 1500 - 99);
    assert_eq!(st.players[1].cash, 1500 + 33);
    assert_eq!(
        st.players[2].cash,
        1500 + 33,
        "rejecting voters still get a share"
    );
    assert_eq!(st.players[3].cash, 1500 + 33);
}

#[test]
fn corruption_bribe_fails_without_majority_and_degrades_to_await_end() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].position = 5;
    st.players[0].jailed = true;

    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::OfferBribe { amount: 100 }),
    );
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::VoteOnBribe { accept: false }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BribeResolved {
            briber: 0,
            amount: 100,
            succeeded: false,
            accepts: 0,
            total: 1,
        }
    )));
    assert!(st.players[0].jailed, "the bribe failing does not un-jail");
    assert_eq!(
        st.turn,
        TurnPhase::AwaitEnd,
        "turn degrades, retry available next turn"
    );
    assert_eq!(st.players[0].cash, 1500, "no cash moves on a failed bribe");
    assert_eq!(st.players[1].cash, 1500);

    // Retry next turn: the same jailed player can offer again once back in
    // AwaitMove - the failure does not lock them out.
    let mut st = st;
    st.turn = TurnPhase::AwaitMove;
    let (_st, ev) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::OfferBribe { amount: 50 }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BribeOffered {
            player: 0,
            amount: 50
        }
    )));
}

#[test]
fn a_route_landing_on_go_to_jail_revokes_parole_and_refills_the_hand() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].position = 5; // jail tile
    st.players[0].jailed = true;

    // Front card (2) moves p0 from jail (5) straight onto go-to-jail (7).
    let order = vec![2, 1, 3, 4, 5];
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::ChooseLegalRoute {
                order: order.clone(),
            },
        ),
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::WentToJail { player: 0, .. }))
    );
    assert!(
        st.players[0].jailed,
        "landing back on go-to-jail mid-route re-jails the player"
    );
    assert_eq!(
        st.players[0].jail_route, None,
        "the unfinished route must not survive re-imprisonment"
    );
    assert_eq!(st.players[0].position, 5);
    assert_eq!(
        st.players[0].hand,
        vec![1, 2, 3, 4, 5],
        "a normal hand is waiting for whichever jail exit comes next"
    );
    assert_eq!(
        st.players[0].hands_cycled, 1,
        "the abandoned route still ticks exactly one refill"
    );
}

#[test]
fn bankruptcy_during_a_route_purges_the_freeze_state_cleanly() {
    let card = CardDef {
        id: "crushing_debt".into(),
        text: "Pay a crushing debt.".into(),
        effect: CardEffect::Money { amount: -1000 },
    };
    let engine = engine_with(card_board(vec![card]));
    let mut st = two_players(&engine);
    st.tiles[1].owner = Some(0); // p0 owns ave_a before going bankrupt
    st.players[0].position = 4; // jail tile
    st.players[0].jailed = true;
    st.players[0].cash = 5;

    // A custom (non-ascending) route whose front card lands p0 straight on
    // the chance tile: 4 (jail) + 4 = 3 (mod 5).
    let order = vec![4, 1, 2, 3, 5];
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::ChooseLegalRoute {
                order: order.clone(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PlayerBankrupt {
            player: 0,
            creditor: None
        }
    )));
    assert!(st.players[0].bankrupt);
    assert_eq!(
        st.players[0].jail_route, None,
        "the freeze state must not survive a mid-route bankruptcy"
    );
    assert!(!st.players[0].jailed);
    assert_eq!(st.tiles[1].owner, None, "the tile returns to the bank");
}
