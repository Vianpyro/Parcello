//! Core engine flow: movement and cards, turn/win conditions (points,
//! doom clock, groups, time), forecast scheduling, the Exposition
//! spotlight, views, wire formats, and replay determinism.
//! (`same_seed_produces_identical_games` lives here - extend it when
//! adding phases.) Fixtures: `tests/common/mod.rs`.

mod common;
use common::*;

#[test]
fn money_card_adjusts_cash() {
    let card = CardDef {
        id: "dividend".into(),
        text: "Bank pays you 50.".into(),
        effect: CardEffect::Money { amount: 50 },
    };
    let engine = engine_with(card_board(vec![card]));
    let st = two_players(&engine);

    let (st, ev) = play(&engine, &st, "p0", 3);
    assert!(ev.iter().any(|e| matches!(e, Event::CardDrawn { .. })));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::CashAdjusted {
            player: 0,
            delta: 50,
            ..
        }
    )));
    assert_eq!(st.players[0].cash, 1550);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
}

#[test]
fn move_to_card_collects_salary_and_resolves_landing() {
    let card = CardDef {
        id: "advance_go".into(),
        text: "Advance to Go.".into(),
        effect: CardEffect::MoveTo {
            tile: "go".into(),
            collect_go: true,
        },
    };
    let engine = engine_with(card_board(vec![card]));
    let st = two_players(&engine);

    let (st, _) = play(&engine, &st, "p0", 3);
    assert_eq!(st.players[0].position, 0);
    assert_eq!(st.players[0].cash, 1700);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
}

/// A "go directly to X, do not pass Go" card (`collect_go: false`) must
/// never pay salary even when the target sits "behind" the traveler on
/// the board (the wrap-detection condition `to <= from` that would
/// normally trigger it) - `collect_go` gates `passed_go` unconditionally.
/// This is what the Flutter client's `passed_go` field relies on to
/// decide whether to hop the pawn through Go or glide it straight there
/// (2026-07 playtest feedback): the client must never show a false
/// "crossed Go" hop for a card that explicitly skips it.
#[test]
fn move_to_card_without_collect_go_never_pays_salary_even_when_wrapping() {
    let card = CardDef {
        id: "goto_ave_a_no_go".into(),
        text: "Go directly to Ave A. Do not pass Go.".into(),
        effect: CardEffect::MoveTo {
            tile: "ave_a".into(),
            collect_go: false,
        },
    };
    let engine = engine_with(card_board(vec![card]));
    let st = two_players(&engine);

    // p0: Go(0) -> Chance(3) draws the card -> Ave A(1). The target sits
    // behind the chance tile (to=1 <= from=3), the exact geometry that
    // pays salary when collect_go is true (see the sibling test above).
    let (st, ev) = play(&engine, &st, "p0", 3);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::Moved {
            player: 0,
            from: 3,
            to: 1,
            passed_go: false,
        }
    )));
    assert!(
        !ev.iter().any(|e| matches!(e, Event::SalaryPaid { .. })),
        "collect_go: false must never pay salary, wrap or not"
    );
    assert_eq!(st.players[0].position, 1);
    assert_eq!(st.players[0].cash, 1500, "no salary collected");
}

#[test]
fn same_seed_produces_identical_games() {
    // Drive both runs with state-derived canonical actions so the script
    // never depends on dice outcomes; only the seed differs between runs.
    let run = |seed: u64| {
        let engine = Engine::new(Arc::new(plain_board())).expect("valid content");
        let mut st = engine.new_game(
            vec![("p0".into(), "P0".into()), ("p1".into(), "P1".into())],
            seed,
        );
        for _ in 0..40 {
            if matches!(st.phase, GamePhase::Finished { .. }) {
                break;
            }
            let (actor, kind) = match &st.turn {
                TurnPhase::AwaitMove => {
                    let p = &st.players[st.current];
                    if let Some(route) = &p.jail_route {
                        // On a locked route, only its front value is legal,
                        // whether or not `jailed` is still set (it clears
                        // the instant the route is chosen).
                        (
                            st.current,
                            CommandKind::PlayMovementCard { value: route[0] },
                        )
                    } else if p.jailed {
                        let rules = &engine.content().rules;
                        let order: Vec<u8> = (rules.velocity_min..=rules.velocity_max).collect();
                        (st.current, CommandKind::ChooseLegalRoute { order })
                    } else {
                        let value = *p.hand.iter().min().expect("hand never empty in AwaitMove");
                        (st.current, CommandKind::PlayMovementCard { value })
                    }
                }
                TurnPhase::AwaitEnd => (st.current, CommandKind::EndTurn),
                TurnPhase::BlindAuction { bids, .. } => {
                    let seat = st
                        .alive_players()
                        .find(|&s| bids[s].is_none())
                        .expect("a phase stays BlindAuction only while someone is pending");
                    (seat, CommandKind::SubmitBlindBid { amount: 0 })
                }
                TurnPhase::BribeVote { briber, votes, .. } => {
                    let seat = st
                        .alive_players()
                        .find(|&s| s != *briber && votes[s].is_none())
                        .expect("a phase stays BribeVote only while someone is pending");
                    (seat, CommandKind::VoteOnBribe { accept: false })
                }
            };
            let actor = st.players[actor].id.clone();
            st = step(&engine, &st, cmd(&actor, kind)).0;
        }
        serde_json::to_string(&st).expect("state serializes")
    };
    assert_eq!(run(42), run(42), "same seed must replay identically");
    assert_ne!(run(42), run(43), "different seeds should diverge");
}

#[test]
fn finish_on_time_awards_the_richest_and_breaks_ties_low() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].cash = 900;
    st.players[1].cash = 1200;
    let (done, ev) = engine.finish_on_time(&st);
    assert_eq!(done.phase, GamePhase::Finished { winner: 1 });
    assert!(ev.iter().any(|e| matches!(e, Event::TimeUp { winner: 1 })));

    // Tie -> lowest seat wins.
    st.players[1].cash = 900;
    let (tie, _) = engine.finish_on_time(&st);
    assert_eq!(tie.phase, GamePhase::Finished { winner: 0 });

    // Already finished -> no-op, no event.
    let (again, ev2) = engine.finish_on_time(&done);
    assert_eq!(again.phase, done.phase);
    assert!(ev2.is_empty());
}

#[test]
fn win_by_controlling_full_groups() {
    // Two full groups win. p0 owns brown (ave_a, ave_b); seizing blvd (the
    // singleton navy group) completes a second group -> instant win.
    let engine = engine_with_rules(|r| {
        r.win_full_groups = 2;
        r.expropriation = 100;
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[6].owner = Some(1);
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 6;
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "blvd".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::WonByGroups {
            winner: 0,
            groups: 2
        }
    )));
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn group_win_is_off_by_default() {
    // Same holdings, but no win threshold: seizing must not end the game.
    let engine = engine_with_rules(|r| r.expropriation = 100);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[6].owner = Some(1);
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 6;
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "blvd".into(),
            },
        ),
    );
    assert!(!ev.iter().any(|e| matches!(e, Event::WonByGroups { .. })));
    assert_eq!(st.phase, GamePhase::Active);
}

#[test]
fn victory_points_score_groups_and_conglomerates() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    let content = engine.content();

    // A full brown group: +3.
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    assert_eq!(st.victory_points(content, 0), 3);

    // One tile built to the conglomerate level (5 houses, plain_board's
    // default cap): +2 more.
    st.tiles[2].houses = 5;
    assert_eq!(st.victory_points(content, 0), 5);

    // Losing the group (a rival buys ave_b) drops the group bonus; the
    // conglomerate tile is still owned, so its bonus stays.
    st.tiles[3].owner = Some(1);
    assert_eq!(
        st.victory_points(content, 0),
        2,
        "group lost, conglomerate tile kept"
    );

    // Losing the conglomerate tile itself drops to zero: fully reversible.
    st.tiles[2].owner = None;
    assert_eq!(st.victory_points(content, 0), 0);
}

#[test]
fn victory_points_score_resorts_and_round_bonus() {
    let engine = engine_with(transit_board());
    let mut st = two_players(&engine);
    let content = engine.content();

    st.tiles[2].owner = Some(0); // station_a, group-scaled
    assert_eq!(st.victory_points(content, 0), 1, "one resort owned");

    st.tiles[3].owner = Some(0); // station_b: completes the "transit" group too
    assert_eq!(
        st.victory_points(content, 0),
        3 + 2,
        "group complete (+3) plus both resorts (+1 each)"
    );

    st.tiles[2].owner = None; // lose one resort and the group completion
    assert_eq!(
        st.victory_points(content, 0),
        1,
        "reversible: down to the one remaining resort"
    );

    // Round bonus is the one stored, non-reversible term.
    st.players[0].round_bonus_vp = 4;
    assert_eq!(st.victory_points(content, 0), 1 + 4);
}

#[test]
fn round_bonus_favors_highest_cash_and_ties_to_lowest_seat() {
    // The round metronome is now a hand refill (ADR-0017), not a raw
    // EndTurn - force every seat down to a single card that lands on Park
    // (tile 4, no side effects) so playing it both moves the seat and
    // empties/refills the hand in the same command.
    let engine = engine_with_rules(|r| r.win_victory_points = 1000);
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
        ],
        7,
    );
    st.players[1].cash += 500; // p1 uniquely richest for round 1
    for p in &mut st.players {
        p.hand = vec![4];
        p.position = 0;
    }
    st.turn = TurnPhase::AwaitMove;

    let (next, _) = play(&engine, &st, "p0", 4);
    let (next, _) = step(&engine, &next, cmd("p0", CommandKind::EndTurn));
    let (next, _) = play(&engine, &next, "p1", 4);
    let (next, _) = step(&engine, &next, cmd("p1", CommandKind::EndTurn));
    assert!(
        next.players.iter().all(|p| p.round_bonus_vp == 0),
        "round 1 isn't complete until p2 also goes"
    );
    let (next, _) = play(&engine, &next, "p2", 4);
    let (next, _) = step(&engine, &next, cmd("p2", CommandKind::EndTurn));
    assert_eq!(next.players[1].round_bonus_vp, 2, "p1 was uniquely richest");
    assert_eq!(next.players[0].round_bonus_vp, 0);
    assert_eq!(next.players[2].round_bonus_vp, 0);

    // Round 2: p0 and p2 are now tied for richest (both above p1) - the
    // lowest seat (p0) must win the tie.
    let mut next = next;
    next.players[0].cash = 10_000;
    next.players[2].cash = 10_000;
    for p in &mut next.players {
        p.hand = vec![4];
        p.position = 0;
    }
    next.turn = TurnPhase::AwaitMove;
    let (next, _) = play(&engine, &next, "p0", 4);
    let (next, _) = step(&engine, &next, cmd("p0", CommandKind::EndTurn));
    let (next, _) = play(&engine, &next, "p1", 4);
    let (next, _) = step(&engine, &next, cmd("p1", CommandKind::EndTurn));
    let (next, _) = play(&engine, &next, "p2", 4);
    let (next, _) = step(&engine, &next, cmd("p2", CommandKind::EndTurn));
    assert_eq!(
        next.players[0].round_bonus_vp, 2,
        "p0 wins the round-2 tie (round 1's bonus went to p1)"
    );
    assert_eq!(next.players[1].round_bonus_vp, 2, "unchanged from round 1");
    assert_eq!(
        next.players[2].round_bonus_vp, 0,
        "never strictly richest in either round"
    );
}

#[test]
fn points_win_fires_exactly_at_the_target() {
    // Three players so p0's and p1's single EndTurn each don't complete a
    // full round (p2 hasn't gone yet) - keeps this test isolated from the
    // round-bonus mechanism, covered separately.
    let engine = engine_with_rules(|r| r.win_victory_points = 3); // one full group's worth
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
        ],
        7,
    );
    st.tiles[2].owner = Some(0); // ave_a only: one tile short of a full group
    st.turn = TurnPhase::AwaitEnd;
    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert!(!ev.iter().any(|e| matches!(e, Event::WonByPoints { .. })));
    assert_eq!(st.phase, GamePhase::Active);

    let mut st = st;
    st.tiles[3].owner = Some(0); // completes the brown group -> 3 points
    st.turn = TurnPhase::AwaitEnd;
    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::EndTurn));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::WonByPoints {
            player: 0,
            points: 3
        }
    )));
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn points_win_takes_priority_over_the_doom_clock_on_the_same_command() {
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 1; // pool = 1 for 2 players
        r.conglomerate_pool_factor = 1; // pool = 1 for 2 players
        r.win_victory_points = 5; // exactly what this build reaches
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[2].houses = 4;
    st.tiles[3].houses = 4;
    st.players[0].cash = 1_000;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Build {
                tile: "ave_a".into(),
            },
        ),
    );
    assert_eq!(
        st.conglomerates_available,
        Some(0),
        "the pool also hit zero on this same command"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::WonByPoints {
            player: 0,
            points: 5
        }
    )));
    assert!(
        !ev.iter()
            .any(|e| matches!(e, Event::WonByPoolExhaustion { .. }))
    );
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn doom_clock_ends_the_game_when_nobody_has_reached_the_target() {
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 1;
        r.conglomerate_pool_factor = 1;
        r.win_victory_points = 100; // far out of reach
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[2].houses = 4;
    st.tiles[3].houses = 4;
    st.players[0].cash = 1_000;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Build {
                tile: "ave_a".into(),
            },
        ),
    );
    assert_eq!(st.conglomerates_available, Some(0));
    assert!(!ev.iter().any(|e| matches!(e, Event::WonByPoints { .. })));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::WonByPoolExhaustion { winner: 0 }))
    );
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn view_hides_rng_and_deck_order() {
    let engine = engine_with(plain_board());
    let st = two_players(&engine);
    let view = ClientView::of(&st, engine.content());
    let json = serde_json::to_string(&view).expect("view serializes");
    assert!(!json.contains("rng"));
    assert!(!json.contains("deck"));
    assert_eq!(view.players.len(), 2);
}

/// The round metronome (ADR-0020) is public: clients derive the round
/// number (the minimum across survivors) and its progress from it, which
/// is what makes the `+2` round bonus's timing legible.
#[test]
fn view_exposes_hands_cycled_for_round_progress() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.players[0].hands_cycled = 3;
    st.players[1].hands_cycled = 2;

    let view = ClientView::of(&st, engine.content());
    assert_eq!(view.players[0].hands_cycled, 3);
    assert_eq!(view.players[1].hands_cycled, 2);
    // The round is the laggard's count: p0 is one hand ahead, so the table
    // is still waiting on p1 before the bonus can fire.
    let round = view
        .players
        .iter()
        .filter(|p| !p.bankrupt)
        .map(|p| p.hands_cycled)
        .min()
        .expect("someone is alive");
    assert_eq!(round, 2);

    let json = serde_json::to_string(&view).expect("view serializes");
    assert!(json.contains("hands_cycled"), "wire field must be present");
}

#[test]
fn command_wire_format_is_stable() {
    let c = cmd(
        "p0",
        CommandKind::Build {
            tile: "ave_a".into(),
        },
    );
    let json = serde_json::to_string(&c).expect("serializes");
    assert_eq!(json, r#"{"player":"p0","type":"build","tile":"ave_a"}"#);
    let back: PlayerCommand = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(back, c);
}

#[test]
fn forecast_seeded_at_new_game_is_deterministic_and_chained() {
    let events = vec![
        market_event("bubble", MarketEffect::AcquisitionMultiplier, -30, 5),
        market_event("crash", MarketEffect::RentMultiplier, -50, 4),
    ];
    let engine = engine_with_forecast(events, 5, |_| {});
    let players = || {
        vec![
            ("p0".to_string(), "P0".to_string()),
            ("p1".to_string(), "P1".to_string()),
        ]
    };
    let st1 = engine.new_game(players(), 42);
    let st2 = engine.new_game(players(), 42);
    assert_eq!(
        st1.forecast, st2.forecast,
        "same seed schedules identically"
    );
    assert_eq!(st1.forecast.queue.len(), 3);
    let starts: Vec<u32> = st1
        .forecast
        .queue
        .iter()
        .map(|s| s.starts_at_turn)
        .collect();
    assert_eq!(starts, vec![5, 10, 15], "chained gap_turns apart");
    assert!(st1.forecast.active.is_none());
}

#[test]
fn forecast_is_inert_without_market_events() {
    let engine = engine_with(plain_board()); // plain_board ships no events
    let mut st = two_players(&engine);
    assert!(st.forecast.queue.is_empty());
    assert!(st.forecast.active.is_none());
    for i in 0..6 {
        let actor = if st.current == 0 { "p0" } else { "p1" };
        st.turn = TurnPhase::AwaitEnd;
        let (next, _) = step(&engine, &st, cmd(actor, CommandKind::EndTurn));
        st = next;
        assert!(st.forecast.queue.is_empty(), "iteration {i}");
        assert!(st.forecast.active.is_none(), "iteration {i}");
    }
}

#[test]
fn spotlight_activates_on_landing_and_boosts_rent() {
    let engine = spotlight_engine(|r| {
        r.spotlight_rent_pct = 100;
        r.spotlight_duration_turns = 8;
    });
    let st = two_players(&engine);

    let (st, ev) = play(&engine, &st, "p0", 3); // 0 -> 3, the Exposition
    let spotlit = st.spotlight.expect("a spotlight starts on landing");
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::SpotlightStarted { tile, rent_pct: 100, duration_turns: 8 }
            if *tile == spotlit.tile
    )));
    assert_eq!(spotlit.expires_at_turn, st.turn_count + 8);

    // Own only the spotlit tile - not its groupmate too, or the unrelated
    // full-group monopoly double would also kick in and muddy the
    // assertion - then have p1 land on it and pay boosted rent.
    let mut st = st;
    st.tiles[spotlit.tile].owner = Some(0);
    st.current = 1;
    st.players[1].position = 0;
    st.turn = TurnPhase::AwaitMove;
    let base = if spotlit.tile == 1 { 2 } else { 4 }; // ave_a/ave_b rents[0]
    let (_, ev) = play(&engine, &st, "p1", spotlit.tile as u8);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount, .. } if *amount == base * 2)),
        "+100% spotlight doubles the base rent"
    );
}

#[test]
fn spotlight_only_boosts_the_spotlighted_tile() {
    let engine = spotlight_engine(|r| {
        r.spotlight_rent_pct = 100;
        r.spotlight_duration_turns = 8;
    });
    let st = two_players(&engine);

    let (st, _) = play(&engine, &st, "p0", 3);
    let spotlit_tile = st.spotlight.expect("spotlight active").tile;
    let other_tile = if spotlit_tile == 1 { 2 } else { 1 };
    let other_base = if other_tile == 1 { 2 } else { 4 };

    // Own only the non-spotlit tile (see the note above on why not both).
    let mut st = st;
    st.tiles[other_tile].owner = Some(0);
    st.current = 1;
    st.players[1].position = 0;
    st.turn = TurnPhase::AwaitMove;
    let (_, ev) = play(&engine, &st, "p1", other_tile as u8);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount, .. } if *amount == other_base)),
        "the non-spotlit tile's rent is unaffected"
    );
}

#[test]
fn spotlight_composes_with_boost_and_forecast_multiplicatively() {
    let engine = spotlight_engine(|r| r.spotlight_rent_pct = 100);
    let mut st = two_players(&engine);
    st.tiles[1].owner = Some(0); // p0 owns ave_a (rents[0] = 2)
    st.tiles[1].boosts = 1; // ADR-0012: base 2 -> boosted 3
    st.forecast.active = Some(ActiveMarketEvent {
        event_id: "crash".into(),
        effect: MarketEffect::RentMultiplier,
        magnitude_pct: -50,
        ends_at_turn: 10,
    });
    st.spotlight = Some(Spotlight {
        tile: 1,
        expires_at_turn: 10,
    });
    st.current = 1;
    st.players[1].position = 0;
    st.turn = TurnPhase::AwaitMove;

    let (_, ev) = play(&engine, &st, "p1", 1); // 0 -> 1 (ave_a)
    // boosted 2 -> 3; -50% forecast -> 1 (floored); +100% spotlight -> 2.
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 2, .. })),
        "boost, forecast, and spotlight compose in order"
    );
}

#[test]
fn spotlight_expires_exactly_at_its_scheduled_turn() {
    let engine = engine_with(spotlight_board());
    let mut st = two_players(&engine);
    st.spotlight = Some(Spotlight {
        tile: 1,
        expires_at_turn: 1,
    });
    st.turn = TurnPhase::AwaitEnd;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.turn_count, 1);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::SpotlightEnded { tile: 1 }))
    );
    assert!(st.spotlight.is_none());
}

#[test]
fn landing_again_rerolls_and_replaces_the_active_spotlight() {
    let engine = spotlight_engine(|r| {
        r.spotlight_rent_pct = 100;
        r.spotlight_duration_turns = 8;
    });
    let mut st = two_players(&engine);
    st.spotlight = Some(Spotlight {
        tile: 1,
        expires_at_turn: 99,
    });

    let (st, ev) = play(&engine, &st, "p0", 3); // lands on the Exposition again
    let ended_idx = ev
        .iter()
        .position(|e| matches!(e, Event::SpotlightEnded { tile: 1 }));
    let started_idx = ev
        .iter()
        .position(|e| matches!(e, Event::SpotlightStarted { .. }));
    assert!(
        ended_idx.is_some() && started_idx.is_some(),
        "the bumped spotlight ends and a new one starts"
    );
    assert!(
        ended_idx.unwrap() < started_idx.unwrap(),
        "the end event fires before the new start"
    );
    assert!(st.spotlight.is_some());
}

#[test]
fn spotlight_persists_when_the_tile_changes_hands() {
    let engine = engine_with(spotlight_board());
    let mut st = two_players(&engine);
    st.tiles[1].owner = Some(0); // p0 owns ave_a
    st.spotlight = Some(Spotlight {
        tile: 1,
        expires_at_turn: 12,
    });

    let (st, _) = step(
        &engine,
        &st,
        cmd("p1", offer("p0", 100, &[], 0, &["ave_a"])),
    );
    let (st, _) = step(
        &engine,
        &st,
        cmd("p0", CommandKind::AcceptTrade { trade: 0 }),
    );

    assert_eq!(st.tiles[1].owner, Some(1), "the tile changed hands");
    assert_eq!(
        st.spotlight,
        Some(Spotlight {
            tile: 1,
            expires_at_turn: 12
        }),
        "the spotlight is a location fact, not an owner-purchased upgrade - \
         it survives the trade untouched (expropriation/bankruptcy are \
         equivalently safe: neither path touches GameState.spotlight)"
    );
}

#[test]
fn spotlight_is_a_noop_without_any_property_tiles() {
    let content = GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile("exposition", "The Exposition", TileKind::Spotlight),
            tile("jail", "Jail", TileKind::Jail),
        ],
        chance: vec![],
        community: vec![],
        rules: RuleParams {
            spotlight_rent_pct: 100,
            spotlight_duration_turns: 8,
            ..RuleParams::default()
        },
        market_events: vec![],
        forecast_gap_turns: 0,
    };
    let engine = engine_with(content);
    let st = two_players(&engine);

    let (st, ev) = play(&engine, &st, "p0", 1); // 0 -> 1, the Exposition
    assert!(
        !ev.iter()
            .any(|e| matches!(e, Event::SpotlightStarted { .. }))
    );
    assert!(st.spotlight.is_none());
}

#[test]
fn same_seed_schedules_the_same_spotlight_draw() {
    let engine = spotlight_engine(|r| {
        r.spotlight_rent_pct = 100;
        r.spotlight_duration_turns = 8;
    });
    let st1 = two_players(&engine);
    let st2 = two_players(&engine);

    let (st1, _) = play(&engine, &st1, "p0", 3);
    let (st2, _) = play(&engine, &st2, "p0", 3);

    assert_eq!(st1.spotlight, st2.spotlight);
}

#[test]
fn first_player_is_drawn_from_the_seed() {
    let engine = engine_with(plain_board());
    let players = || {
        vec![
            ("p0".to_string(), "P0".to_string()),
            ("p1".to_string(), "P1".to_string()),
            ("p2".to_string(), "P2".to_string()),
        ]
    };
    // Deterministic: same seed, same starter.
    assert_eq!(
        engine.new_game(players(), 5).current,
        engine.new_game(players(), 5).current
    );
    // Actually random across seeds: 32 seeds all landing on one starter
    // out of three has probability 3^-31 - not a flake risk.
    let starters: std::collections::HashSet<usize> = (0..32u64)
        .map(|seed| engine.new_game(players(), seed).current)
        .collect();
    assert!(starters.len() > 1, "the starter must vary with the seed");
}

#[test]
fn spotlight_with_zero_duration_is_permanent_until_replaced() {
    let engine = spotlight_engine(|r| {
        r.spotlight_rent_pct = 100;
        r.spotlight_duration_turns = 0; // permanent (2026-07)
    });
    let st = two_players(&engine);

    let (st, _) = play(&engine, &st, "p0", 3); // 0 -> 3, the Exposition
    let first = st.spotlight.expect("spotlight starts");
    assert_eq!(first.expires_at_turn, u32::MAX, "never expires on its own");

    // Several turn transitions later it is still lit.
    let mut st = st;
    for _ in 0..6 {
        st.turn = TurnPhase::AwaitEnd;
        let (next, ev) = step(
            &engine,
            &st,
            cmd(&st.players[st.current].id.clone(), CommandKind::EndTurn),
        );
        assert!(
            !ev.iter().any(|e| matches!(e, Event::SpotlightEnded { .. })),
            "a permanent spotlight must not expire on a turn tick"
        );
        st = next;
    }
    assert_eq!(st.spotlight, Some(first));

    // Only a fresh Exposition landing replaces it.
    st.current = 0;
    st.players[0].position = 0;
    st.players[0].hand = vec![3];
    st.turn = TurnPhase::AwaitMove;
    let (st, ev) = play(&engine, &st, "p0", 3);
    assert!(ev.iter().any(|e| matches!(e, Event::SpotlightEnded { .. })));
    assert!(st.spotlight.is_some());
}
