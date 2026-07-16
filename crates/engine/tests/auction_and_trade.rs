//! Sealed-bid auctions (ADR-0018) and asynchronous trades (ADR-0007):
//! floors, contested discounts, window survival, offer lifecycle and
//! validation. Fixtures: `tests/common/mod.rs`.

mod common;
use common::*;

#[test]
fn discoverer_wins_at_floor_when_uncontested_then_pays_rent() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);

    let (st, ev) = play(&engine, &st, "p0", 3);
    assert!(matches!(st.turn, TurnPhase::BlindAuction { tile: 3, .. }));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionOpened {
            tile: 3,
            discoverer: 0,
            floor: 60
        }
    )));

    // p1 abstains; p0 (discoverer) stays silent too - the implicit floor
    // bid wins uncontested, no discount.
    let (st, _) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionResolved {
            tile: 3,
            winner: Some(0),
            amount: 60,
            ..
        }
    )));
    // Winning its own auction rebates 10% of what it paid (ADR-0018 amended):
    // charged in full above, handed back here, as its own event.
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::DiscovererRefunded {
            player: 0,
            tile: 3,
            amount: 6, // 10% of the 60 paid
        }
    )));
    assert_eq!(st.tiles[3].owner, Some(0));
    assert_eq!(st.players[0].cash, 1500 - 60 + 6);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.current, 1);

    let (st, ev) = play(&engine, &st, "p1", 3);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentPaid {
            from: 1,
            to: 0,
            tile: 3,
            amount: 4
        }
    )));
    assert_eq!(st.players[1].cash, 1500 - 4);
    assert_eq!(st.players[0].cash, 1500 - 60 + 6 + 4);
}

#[test]
fn seat_view_shows_only_own_trade_offers() {
    // 3 players; p0 offers to p1. p2's view must not contain the offer,
    // the omniscient view keeps it (ADR-0007).
    let engine = engine_with(plain_board());
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
        ],
        42,
    );
    st.tiles[2].owner = Some(0);
    let (st, _) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::ProposeTrade {
                to: "p1".into(),
                give_cash: 0,
                give_tiles: vec!["ave_a".into()],
                receive_cash: 100,
                receive_tiles: vec![],
            },
        ),
    );
    assert_eq!(
        ClientView::of(&st, engine.content()).pending_trades.len(),
        1
    );
    assert_eq!(
        ClientView::for_seat(&st, engine.content(), 0)
            .pending_trades
            .len(),
        1
    );
    assert_eq!(
        ClientView::for_seat(&st, engine.content(), 1)
            .pending_trades
            .len(),
        1
    );
    assert!(
        ClientView::for_seat(&st, engine.content(), 2)
            .pending_trades
            .is_empty()
    );
}

#[test]
fn discoverer_pays_its_winning_bid_in_full_then_gets_the_rebate() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);

    // p0 lands on ave_a (tile 2, floor 60): the window opens for both seats.
    let (st, _) = play(&engine, &st, "p0", 2);
    assert!(matches!(st.turn, TurnPhase::BlindAuction { tile: 2, .. }));

    // A discoverer bid below the floor is rejected; an unaffordable bid too.
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::SubmitBlindBid { amount: 10 }))
            .unwrap_err(),
        CommandError::BidBelowFloor
    );
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd("p1", CommandKind::SubmitBlindBid { amount: 9999 })
            )
            .unwrap_err(),
        CommandError::InsufficientFunds
    );

    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::SubmitBlindBid { amount: 80 }),
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::SubmitBlindBid { amount: 90 }))
            .unwrap_err(),
        CommandError::AlreadyBid
    );

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 50 }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionResolved {
            tile: 2,
            winner: Some(0),
            amount: 80, // the winning bid, charged in full - no discount
            ..
        }
    )));
    // The reward is a separate, visible rebate, not a quieter price.
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::DiscovererRefunded {
            player: 0,
            tile: 2,
            amount: 8, // 10% of the 80 paid
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(0));
    assert_eq!(st.players[0].cash, 1500 - 80 + 8);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
    assert_eq!(st.current, 0);
}

#[test]
fn a_rival_winning_the_auction_gets_no_rebate() {
    // The rebate rewards having LANDED there, so it is the discoverer's and
    // nobody else's - a rival outbidding them pays every last unit.
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].cash = 10; // broke discoverer: no implicit floor bid
    let (st, _) = play(&engine, &st, "p0", 2);
    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 70 }),
    );

    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionResolved {
            tile: 2,
            winner: Some(1),
            amount: 70,
            ..
        }
    )));
    assert!(
        !ev.iter()
            .any(|e| matches!(e, Event::DiscovererRefunded { .. })),
        "only the discoverer is rebated"
    );
    assert_eq!(st.players[1].cash, 1500 - 70);
}

#[test]
fn all_zero_effective_bids_leave_the_tile_unsold() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].cash = 10; // broke discoverer: no implicit floor
    let (st, _) = play(&engine, &st, "p0", 2);
    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 0 }),
    );

    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionResolved {
            tile: 2,
            winner: None,
            amount: 0,
            ..
        }
    )));
    assert_eq!(st.tiles[2].owner, None);
    assert_eq!(st.players[0].cash, 10);
    assert_eq!(st.players[1].cash, 1500);
}

#[test]
fn discoverer_resigning_mid_window_does_not_abort_the_auction() {
    let engine = engine_with(plain_board());
    let players = vec![
        ("p0".to_string(), "Alice".to_string()),
        ("p1".to_string(), "Bob".to_string()),
        ("p2".to_string(), "Carol".to_string()),
    ];
    let mut st = engine.new_game(players, 42);
    st.current = 0; // seed-drawn starter (2026-07); the script needs p0
    let (st, _) = play(&engine, &st, "p0", 2);
    assert!(matches!(st.turn, TurnPhase::BlindAuction { tile: 2, .. }));

    // p1 bids, then the discoverer (p0) resigns while p2 is still pending -
    // the window must survive (the top-level bankruptcy-advance guard must
    // not fire while a BlindAuction is open).
    let (st, _) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 30 }),
    );
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Resign));
    assert!(
        matches!(st.turn, TurnPhase::BlindAuction { tile: 2, .. }),
        "the window must still be open for p2"
    );
    assert!(st.players[0].bankrupt);

    // p2 abstains: with the discoverer gone (no floor), p1's bid wins at
    // full price. Resolving also completes the deferred turn-advance off
    // the now-bankrupt former discoverer.
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p2", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::BlindAuctionResolved {
            tile: 2,
            winner: Some(1),
            amount: 30,
            ..
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(1));
    assert_eq!(st.current, 1);
    assert_eq!(st.turn, TurnPhase::AwaitMove);
}

#[test]
fn accepted_trade_swaps_tiles_and_cash_out_of_turn() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0); // ave_a
    st.tiles[6].owner = Some(1); // blvd

    // p1 proposes during p0's turn: blvd + 100 for ave_a.
    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", offer("p0", 100, &["blvd"], 0, &["ave_a"])),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::TradeProposed {
            trade: 0,
            from: 1,
            to: 0
        }
    )));
    assert_eq!(st.pending_trades.len(), 1);

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::AcceptTrade { trade: 0 }),
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::TradeAccepted { trade: 0, .. }))
    );
    assert_eq!(st.tiles[2].owner, Some(1));
    assert_eq!(st.tiles[6].owner, Some(0));
    assert_eq!(st.players[0].cash, 1600);
    assert_eq!(st.players[1].cash, 1400);
    assert!(st.pending_trades.is_empty());
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, Event::PropertyTransferred { .. }))
            .count(),
        2
    );
}

#[test]
fn trade_proposals_are_validated() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);

    let reject = |st: &GameState, who: &str, kind: CommandKind| {
        engine.apply(st, &cmd(who, kind)).unwrap_err()
    };

    assert_eq!(
        reject(&st, "p0", offer("p0", 10, &[], 0, &[])),
        CommandError::TradeInvalid
    );
    assert_eq!(
        reject(&st, "p0", offer("ghost", 10, &[], 0, &[])),
        CommandError::UnknownPlayer
    );
    assert_eq!(
        reject(&st, "p0", offer("p1", 0, &[], 0, &[])),
        CommandError::TradeInvalid
    );
    assert_eq!(
        reject(&st, "p0", offer("p1", -5, &[], 0, &[])),
        CommandError::TradeInvalid
    );
    assert_eq!(
        reject(&st, "p0", offer("p1", 0, &["blvd"], 0, &[])),
        CommandError::NotOwner,
        "p0 does not own blvd"
    );
    assert_eq!(
        reject(&st, "p0", offer("p1", 0, &["ave_a", "ave_a"], 0, &[])),
        CommandError::TradeInvalid
    );
    assert_eq!(
        reject(&st, "p0", offer("p1", 9999, &[], 0, &[])),
        CommandError::InsufficientFunds
    );

    st.tiles[3].houses = 1;
    assert_eq!(
        reject(&st, "p0", offer("p1", 0, &["ave_a"], 0, &[])),
        CommandError::HousesInGroup
    );
}

#[test]
fn stale_trade_rejects_without_mutation_and_can_be_declined() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);

    let (mut st, _) = step(
        &engine,
        &st,
        cmd("p0", offer("p1", 0, &["ave_a"], 200, &[])),
    );
    st.players[1].cash = 50; // p1 can no longer pay the asked 200

    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::AcceptTrade { trade: 0 }))
            .unwrap_err(),
        CommandError::InsufficientFunds
    );
    assert_eq!(st.pending_trades.len(), 1, "rejection must not mutate");

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::DeclineTrade { trade: 0 }),
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::TradeDeclined { trade: 0, .. }))
    );
    assert!(st.pending_trades.is_empty());
}

#[test]
fn trade_party_rules_and_cancellation() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", offer("p1", 25, &[], 0, &[])));

    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::AcceptTrade { trade: 0 }))
            .unwrap_err(),
        CommandError::NotTradeParty
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::CancelTrade { trade: 0 }))
            .unwrap_err(),
        CommandError::NotTradeParty
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::AcceptTrade { trade: 7 }))
            .unwrap_err(),
        CommandError::TradeNotFound
    );

    let (st, ev) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::CancelTrade { trade: 0 }),
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::TradeCancelled { trade: 0, .. }))
    );
    assert!(st.pending_trades.is_empty());
}

#[test]
fn trades_are_blocked_during_auctions_and_purged_on_bankruptcy() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", offer("p1", 25, &[], 0, &[])));

    // Land on an unowned tile: a sealed-bid window opens, all trade actions reject.
    let (st, _) = play(&engine, &st, "p0", 2);
    assert!(matches!(st.turn, TurnPhase::BlindAuction { .. }));
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::AcceptTrade { trade: 0 }))
            .unwrap_err(),
        CommandError::WrongPhase
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", offer("p0", 5, &[], 0, &[])))
            .unwrap_err(),
        CommandError::WrongPhase
    );

    // Close the window, then the winner resigns: the offer is purged.
    let (st, _) = step(
        &engine,
        &st,
        cmd("p1", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::SubmitBlindBid { amount: 0 }),
    );
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Resign));
    assert!(st.pending_trades.is_empty());
}

#[test]
fn open_offers_per_player_are_capped() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);
    let mut st = st;
    for _ in 0..4 {
        st = step(&engine, &st, cmd("p0", offer("p1", 5, &[], 0, &[]))).0;
    }
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", offer("p1", 5, &[], 0, &[])))
            .unwrap_err(),
        CommandError::TradeLimit
    );
}

#[test]
fn trade_wire_format_is_stable() {
    let cmd = PlayerCommand {
        player: "p0".into(),
        kind: CommandKind::ProposeTrade {
            to: "p1".into(),
            give_cash: 0,
            give_tiles: vec!["ave_a".into()],
            receive_cash: 150,
            receive_tiles: vec![],
        },
    };
    let json = serde_json::to_string(&cmd).expect("serializes");
    assert_eq!(
        json,
        r#"{"player":"p0","type":"propose_trade","to":"p1","give_cash":0,"give_tiles":["ave_a"],"receive_cash":150,"receive_tiles":[]}"#
    );
    let short: PlayerCommand =
        serde_json::from_str(r#"{"player":"p0","type":"propose_trade","to":"p1","give_cash":50}"#)
            .expect("defaults fill missing sides");
    assert!(matches!(
        short.kind,
        CommandKind::ProposeTrade { give_cash: 50, .. }
    ));
}
