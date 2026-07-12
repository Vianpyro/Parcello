//! Estate and economy rules: building under the even rule and shared
//! pools (ADR-0019), mortgages and the mortgaged buyout (ADR-0022),
//! boosts (ADR-0012), takeovers, liquidation/bankruptcy, taxes
//! (ADR-0029), and market multipliers (ADR-0021).
//! Fixtures: `tests/common/mod.rs`.

mod common;
use common::*;

#[test]
fn monopoly_doubles_unimproved_rent_and_allows_building() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);

    // Building is allowed pre-roll on the owner's turn.
    let (st2, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Build {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseBuilt {
            tile: 2,
            houses: 1,
            ..
        }
    )));
    assert_eq!(st2.players[0].cash, 1500 - 50);

    // Group incomplete after losing a tile: build must be rejected.
    let mut broken = st2.clone();
    broken.tiles[3].owner = Some(1);
    let err = engine
        .apply(
            &broken,
            &cmd(
                "p0",
                CommandKind::Build {
                    tile: "ave_a".into(),
                },
            ),
        )
        .unwrap_err();
    assert_eq!(err, CommandError::GroupIncomplete);

    // Opponent lands on the unimproved half of a full group: double rent.
    st.current = 1;
    st.turn = TurnPhase::AwaitMove;
    let (st3, ev) = play(&engine, &st, "p1", 3);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentPaid {
            tile: 3,
            amount: 8,
            ..
        }
    )));
    assert_eq!(st3.players[1].cash, 1500 - 8);
}

#[test]
fn unpayable_rent_bankrupts_and_ends_the_game() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0);
    st.players[1].cash = 5;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PlayerBankrupt {
            player: 1,
            creditor: Some(0)
        }
    )));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::GameEnded { winner: 0 }))
    );
    assert!(st.players[1].bankrupt);
    assert_eq!(
        st.players[0].cash, 1505,
        "creditor receives the remaining cash"
    );
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::PlayMovementCard { value: 1 }))
            .unwrap_err(),
        CommandError::GameFinished
    );
}

#[test]
fn liquidation_sells_houses_before_bankruptcy() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    // Tile 6 is the only navy tile: owning it is a (singleton) full group,
    // so unimproved rent doubles to 20.
    st.tiles[6].owner = Some(0);
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 2; // 2 * (50 / 2) = 50 recoverable
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    let sold = ev
        .iter()
        .filter(|e| matches!(e, Event::HouseSold { .. }))
        .count();
    assert_eq!(sold, 1, "one house sale covers the 20 debt");
    assert!(!st.players[1].bankrupt);
    assert_eq!(st.players[1].cash, 25 - 20);
    assert_eq!(st.tiles[2].houses, 1);
}

#[test]
fn resign_transfers_assets_to_bank_and_can_end_game() {
    let engine = engine_with(plain_board());
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "Alice".into()),
            ("p1".into(), "Bob".into()),
            ("p2".into(), "Carol".into()),
        ],
        7,
    );
    st.tiles[6].owner = Some(1);

    // Resigning out of turn is allowed.
    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Resign));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PropertyTransferred {
            tile: 6,
            to: None,
            ..
        }
    )));
    assert!(st.players[1].bankrupt);
    assert_eq!(st.tiles[6].owner, None);
    assert_eq!(st.phase, GamePhase::Active);
    assert_eq!(st.current, 0);

    let (st, ev) = step(&engine, &st, cmd("p2", CommandKind::Resign));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::GameEnded { winner: 0 }))
    );
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn resigning_current_player_advances_the_turn() {
    let engine = engine_with(plain_board());
    let st = engine.new_game(
        vec![
            ("p0".into(), "Alice".into()),
            ("p1".into(), "Bob".into()),
            ("p2".into(), "Carol".into()),
        ],
        7,
    );
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Resign));
    assert_eq!(st.current, 1);
    assert_eq!(st.turn, TurnPhase::AwaitMove);
}

#[test]
fn net_worth_counts_cash_property_and_houses() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    // p0: 1500 cash + ave_a (60) unmortgaged + ave_b (60) with 2 houses (50 each).
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[3].houses = 2;
    // p0 also has blvd (100) mortgaged -> counts price/2 = 50.
    st.tiles[6].owner = Some(0);
    st.tiles[6].mortgaged = true;
    let content = plain_board();
    // 1500 + 60 + 60 + 2*50 + 50 = 1770.
    assert_eq!(st.net_worth(&content, 0), 1770);
    assert_eq!(st.net_worth(&content, 1), 1500, "p1 owns nothing yet");
    // Mortgaging is net-worth neutral: cash up price/2, property down price/2.
    st.players[0].cash += 30; // as if ave_a (60) were just mortgaged
    st.tiles[2].mortgaged = true;
    assert_eq!(st.net_worth(&content, 0), 1770 + 30 - 30);
}

#[test]
fn expropriation_transfers_and_compensates() {
    let engine = engine_with_rules(|r| r.expropriation = 200);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1); // p1 owns ave_a (price 60)
    // Takeover only fires on the landing tile, at end of turn (ADR-0022).
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 2;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::Expropriated {
            player: 0,
            from: 1,
            tile: 2,
            cost: 120,
            liquidated: 0,
            liquidation_refund: 0,
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(0), "the tile changes hands");
    assert_eq!(st.players[0].cash, 1500 - 120, "seizer pays 2x price");
    assert_eq!(st.players[1].cash, 1500 + 60, "former owner gets 1x price");
}

#[test]
fn expropriation_is_gated() {
    // Disabled by default.
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1);
    st.turn = TurnPhase::AwaitEnd;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::ExpropriationDisabled
    );

    // Wrong phase / off the landing tile reject before anything else.
    let engine = engine_with_rules(|r| r.expropriation = 200);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::WrongPhase
    );
    st.turn = TurnPhase::AwaitEnd;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::NotOnTile,
        "p0 is still at position 0, not on ave_a"
    );
    st.players[0].position = 2;
    // Own tile, mortgaged tile (the takeover shield), and broke seizer all
    // reject. Improved tiles are legal targets now (ADR-0022) - covered by
    // `takeover_liquidates_improved_tile_and_refunds_old_owner` below.
    st.tiles[2].owner = Some(0);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::NotExpropriable
    );
    // A mortgaged rival tile is no longer the takeover shield: it is now
    // buyable at the flat mortgage price instead (ADR-0022, amended
    // 2026-07) - covered by its own test below, so no rejection here.
    st.tiles[2].owner = Some(1);
    st.players[0].cash = 10;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::InsufficientFunds
    );
}

#[test]
fn rent_boost_raises_rent_and_is_capped() {
    let engine = engine_with_rules(|r| r.rent_boost = 100);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // blvd, singleton navy -> full group, rent 20

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::BoostRent {
                tile: "blvd".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentBoosted {
            tile: 6,
            boosts: 1,
            cost: 100,
            ..
        }
    )));
    assert_eq!(st.players[0].cash, 1500 - 100);

    // p1 lands on blvd (pos 3 + 3 = 6) and pays boosted rent: 20 * 1.5 = 30.
    let mut st = st;
    st.current = 1;
    st.players[1].position = 3;
    st.turn = TurnPhase::AwaitMove;
    let (_st, ev) = play(&engine, &st, "p1", 3);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 30, .. })),
        "20 base rent x1.5 boost"
    );
}

#[test]
fn rent_boost_is_gated_and_bounded() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::BoostRent {
                        tile: "blvd".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::RentBoostDisabled
    );

    let engine = engine_with_rules(|r| r.rent_boost = 10);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0);
    // Three boosts allowed, the fourth is capped.
    for _ in 0..3 {
        st = step(
            &engine,
            &st,
            cmd(
                "p0",
                CommandKind::BoostRent {
                    tile: "blvd".into(),
                },
            ),
        )
        .0;
    }
    assert_eq!(st.tiles[6].boosts, 3);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::BoostRent {
                        tile: "blvd".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::BoostLimit
    );
}

#[test]
fn doom_clock_ties_break_by_net_worth_then_lowest_seat() {
    let engine = engine_with_rules(|r| {
        r.conglomerate_pool_factor = 1;
        r.win_victory_points = 100;
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0); // ave_a
    st.tiles[3].owner = Some(0); // ave_b: brown complete, 3 points
    st.tiles[6].owner = Some(1); // blvd: navy (singleton) complete, 3 points
    st.players[1].cash += 50; // p1 pulls ahead on net worth despite the tie
    st.conglomerates_available = Some(0); // pool already dry (test-only shortcut)
    st.turn = TurnPhase::AwaitEnd;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::WonByPoolExhaustion { winner: 1 }))
    );
    assert_eq!(
        st.phase,
        GamePhase::Finished { winner: 1 },
        "net worth breaks the points tie"
    );
}

#[test]
fn expropriation_requires_landing_on_the_tile() {
    // Rival-owned, unimproved, unmortgaged, and otherwise perfectly legal -
    // but the seizer is standing elsewhere (ADR-0022: takeover only applies
    // to the tile just landed on).
    let engine = engine_with_rules(|r| r.expropriation = 200);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1);
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 5;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Expropriate {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::NotOnTile
    );
}

#[test]
fn scaled_rent_models_follow_group_ownership() {
    let content = transit_board();
    let engine = engine_with(content.clone());
    let mut st = two_players(&engine);

    st.tiles[2].owner = Some(0);
    assert_eq!(StandardRent.rent(&content, &st, 2), 25, "one station owned");
    st.tiles[3].owner = Some(0);
    assert_eq!(
        StandardRent.rent(&content, &st, 2),
        50,
        "two stations owned"
    );

    // Wiring check: the calculator's table value reaches the actual charge.
    st.current = 1;
    let (st, ev) = play(&engine, &st, "p1", 2);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentPaid {
            tile: 2,
            amount: 50,
            ..
        }
    )));
    assert_eq!(st.players[1].cash, 1500 - 50);
}

#[test]
fn scaled_rent_tiles_reject_building() {
    let engine = engine_with(transit_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0); // full group: only the rent model blocks it
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Build {
                        tile: "station_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::NotBuildable
    );
}

#[test]
fn mortgaged_tile_collects_no_rent_and_redeeming_costs_interest() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Mortgage {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PropertyMortgaged {
            player: 0,
            tile: 2,
            value: 30
        }
    )));
    assert!(st.tiles[2].mortgaged);
    assert_eq!(st.players[0].cash, 1530);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Mortgage {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::AlreadyMortgaged
    );

    // p1 lands on the mortgaged tile: no rent changes hands.
    let mut st = st;
    st.current = 1;
    st.players[1].position = 8; // 8 + 3 wraps to 2, collecting Go salary
    let (st, ev) = play(&engine, &st, "p1", 3);
    assert!(!ev.iter().any(|e| matches!(e, Event::RentPaid { .. })));
    assert_eq!(st.players[0].cash, 1530);
    assert_eq!(st.players[1].cash, 1700);

    // Redeeming costs principal + 10% (floored): 30 + 3.
    let mut st = st;
    st.current = 0;
    st.turn = TurnPhase::AwaitMove;
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Unmortgage {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PropertyUnmortgaged {
            player: 0,
            tile: 2,
            cost: 33
        }
    )));
    assert!(!st.tiles[2].mortgaged);
    assert_eq!(st.players[0].cash, 1530 - 33);
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Unmortgage {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::NotMortgaged
    );
}

#[test]
fn mortgage_and_build_enforce_group_constraints() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);

    st.tiles[3].houses = 1;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Mortgage {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::HousesInGroup
    );

    st.tiles[3].houses = 0;
    st.tiles[3].mortgaged = true;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Build {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::MortgagedInGroup
    );
}

#[test]
fn liquidation_mortgages_properties_after_houses() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy monopoly: rent 20
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PropertyMortgaged {
            player: 1,
            tile: 2,
            value: 30
        }
    )));
    assert!(!st.players[1].bankrupt, "one mortgage covers the 20 debt");
    assert!(st.tiles[2].mortgaged);
    assert!(
        !st.tiles[3].mortgaged,
        "no more assets than the debt requires"
    );
    assert_eq!(st.players[1].cash, 10);
    assert_eq!(st.players[0].cash, 1520);
}

#[test]
fn houses_build_and_sell_evenly_with_half_cost_refund() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);

    let build = |t: &str| cmd("p0", CommandKind::Build { tile: t.into() });
    let sell = |t: &str| cmd("p0", CommandKind::SellHouse { tile: t.into() });

    let (st, _) = step(&engine, &st, build("ave_a"));
    assert_eq!(
        engine.apply(&st, &build("ave_a")).unwrap_err(),
        CommandError::UnevenBuild,
        "second house on ave_a before ave_b has one"
    );
    let (st, _) = step(&engine, &st, build("ave_b"));
    let (st, _) = step(&engine, &st, build("ave_a")); // 2-1 is allowed

    assert_eq!(
        engine.apply(&st, &sell("ave_b")).unwrap_err(),
        CommandError::UnevenBuild,
        "must sell from the tallest tile first"
    );
    let (st, ev) = step(&engine, &st, sell("ave_a"));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseSold {
            tile: 2,
            houses: 1,
            refund: 25,
            ..
        }
    )));
    // 3 built (-150), 1 sold (+25).
    assert_eq!(st.players[0].cash, 1500 - 150 + 25);

    let mut st = st;
    st.tiles[2].houses = 0;
    st.tiles[3].houses = 0;
    assert_eq!(
        engine.apply(&st, &sell("ave_a")).unwrap_err(),
        CommandError::NoHouses
    );
}

#[test]
fn forced_liquidation_respects_even_sell() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy: rent 20 owed on landing
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 2;
    st.tiles[3].houses = 1;
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    // One sale (25) covers the 20 debt; it must come from the taller tile.
    let sales: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            Event::HouseSold { tile, houses, .. } => Some((*tile, *houses)),
            _ => None,
        })
        .collect();
    assert_eq!(sales, vec![(2, 1)]);
    assert_eq!(st.tiles[3].houses, 1, "shorter tile untouched");
    assert!(!st.players[1].bankrupt);
}

#[test]
fn build_consumes_and_sell_returns_subsidiary_pool() {
    let engine = engine_with_rules(|r| r.subsidiary_pool_factor = 1); // pool = 1 for 2 players
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0); // ave_a
    st.tiles[3].owner = Some(0); // ave_b
    assert_eq!(st.subsidiaries_available, Some(1));

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
    assert_eq!(st.subsidiaries_available, Some(0));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseBuilt {
            tile: 2,
            houses: 1,
            ..
        }
    )));

    // Pool exhausted: ave_b is still at group_min (0), legal for even-build,
    // but there is no subsidiary left to draw.
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Build {
                        tile: "ave_b".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::PoolExhausted
    );
    assert_eq!(
        st.subsidiaries_available,
        Some(0),
        "rejection never mutates"
    );

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::SellHouse {
                tile: "ave_a".into(),
            },
        ),
    );
    assert_eq!(st.subsidiaries_available, Some(1));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseSold {
            tile: 2,
            houses: 0,
            ..
        }
    )));
}

#[test]
fn conglomerate_build_releases_subsidiaries_and_consumes_one_conglomerate() {
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 1; // pool = 1 for 2 players
        r.conglomerate_pool_factor = 1; // pool = 1 for 2 players
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[2].houses = 4;
    st.tiles[3].houses = 4;
    st.players[0].cash = 1_000;
    assert_eq!(st.conglomerates_available, Some(1));
    assert_eq!(st.subsidiaries_available, Some(1));

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
    assert_eq!(st.tiles[2].houses, 5, "reaches the conglomerate level");
    assert_eq!(
        st.conglomerates_available,
        Some(0),
        "one conglomerate consumed"
    );
    assert_eq!(
        st.subsidiaries_available,
        Some(1 + 4),
        "the tile's 4 subsidiaries return to the pool"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseBuilt {
            tile: 2,
            houses: 5,
            ..
        }
    )));

    // Conglomerate pool now empty: a second top-level build (blvd, singleton
    // navy, already a full group by itself) rejects.
    let mut st = st;
    st.tiles[6].owner = Some(0);
    st.tiles[6].houses = 4;
    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::Build {
                        tile: "blvd".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::PoolExhausted
    );
}

#[test]
fn sell_house_off_conglomerate_needs_free_subsidiaries_or_rejects() {
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 1; // pool = 1 for 2 players, short of cap-1 = 4
        r.conglomerate_pool_factor = 1;
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[2].houses = 5;
    st.tiles[3].houses = 5;
    assert_eq!(st.subsidiaries_available, Some(1));
    assert_eq!(st.conglomerates_available, Some(1));

    assert_eq!(
        engine
            .apply(
                &st,
                &cmd(
                    "p0",
                    CommandKind::SellHouse {
                        tile: "ave_a".into()
                    }
                )
            )
            .unwrap_err(),
        CommandError::PoolExhausted,
        "cap-1 = 4 subsidiaries needed but only 1 is free"
    );

    st.subsidiaries_available = Some(4);
    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::SellHouse {
                tile: "ave_a".into(),
            },
        ),
    );
    assert_eq!(st.tiles[2].houses, 4);
    assert_eq!(
        st.subsidiaries_available,
        Some(0),
        "4 re-issued down to zero"
    );
    assert_eq!(
        st.conglomerates_available,
        Some(2),
        "the tile's conglomerate returns"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::HouseSold {
            tile: 2,
            houses: 4,
            ..
        }
    )));
}

#[test]
fn pool_sizes_scale_with_player_count() {
    // (players, expected subsidiaries at factor 6, expected conglomerates at factor 3)
    let expected = [(2, 8, 4), (3, 10, 5), (4, 12, 6), (5, 13, 7), (6, 15, 7)];
    for (n, subs, congs) in expected {
        let mut content = plain_board();
        content.rules.subsidiary_pool_factor = 6;
        content.rules.conglomerate_pool_factor = 3;
        let engine = Engine::new(Arc::new(content)).expect("valid content");
        let players = (0..n).map(|i| (format!("p{i}"), format!("P{i}"))).collect();
        let st = engine.new_game(players, 1);
        assert_eq!(st.subsidiaries_available, Some(subs), "players={n}");
        assert_eq!(st.conglomerates_available, Some(congs), "players={n}");
    }
}

#[test]
fn zero_pool_factor_is_unlimited() {
    let engine = engine_with(plain_board()); // RuleParams::default(): factors 0
    let mut st = two_players(&engine);
    assert_eq!(st.subsidiaries_available, None);
    assert_eq!(st.conglomerates_available, None);

    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.players[0].cash = 10_000;
    for _ in 0..5 {
        st = step(
            &engine,
            &st,
            cmd(
                "p0",
                CommandKind::Build {
                    tile: "ave_a".into(),
                },
            ),
        )
        .0;
        st = step(
            &engine,
            &st,
            cmd(
                "p0",
                CommandKind::Build {
                    tile: "ave_b".into(),
                },
            ),
        )
        .0;
    }
    assert_eq!(st.tiles[2].houses, 5);
    assert_eq!(st.tiles[3].houses, 5);
    assert_eq!(st.subsidiaries_available, None, "still unlimited");
    assert_eq!(st.conglomerates_available, None, "still unlimited");
}

#[test]
fn forced_liquidation_steps_normally_when_pool_has_room() {
    // Default rules: unlimited pools, so a top-level tile steps down by one
    // level exactly like `forced_liquidation_respects_even_sell`, not a
    // full strip.
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy: rent doubled to 20
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 5; // at cap
    st.tiles[3].houses = 4;
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    let sales: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            Event::HouseSold { tile, houses, .. } => Some((*tile, *houses)),
            _ => None,
        })
        .collect();
    assert_eq!(sales, vec![(2, 4)], "single-level step off the top");
    assert_eq!(st.tiles[3].houses, 4, "shorter tile untouched");
    assert!(!st.players[1].bankrupt);
}

#[test]
fn forced_liquidation_full_strips_when_subsidiary_pool_exhausted() {
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 1;
        r.conglomerate_pool_factor = 3;
    });
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0);
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 5;
    st.tiles[3].houses = 4;
    st.subsidiaries_available = Some(0); // exhausted: can't re-issue cap-1 = 4
    let conglomerates_before = st.conglomerates_available;
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = play(&engine, &st, "p1", 3);
    let sales: Vec<_> = ev
        .iter()
        .filter_map(|e| match e {
            Event::HouseSold {
                tile,
                houses,
                refund,
                ..
            } => Some((*tile, *houses, *refund)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sales,
        vec![(2, 0, 125)],
        "full strip in one motion: 5 levels * 25 refund"
    );
    assert_eq!(st.tiles[2].houses, 0);
    assert_eq!(
        st.subsidiaries_available,
        Some(0),
        "no subsidiary touch - the tile held none at the top level"
    );
    assert_eq!(
        st.conglomerates_available,
        conglomerates_before.map(|n| n + 1),
        "the tile's conglomerate returns"
    );
    assert!(!st.players[1].bankrupt);
}

#[test]
fn bankrupt_releases_pool_units_on_resignation() {
    // Resignation wipes assets directly (bypassing charge()/liquidate()),
    // so it is the reachable path for `bankrupt()`'s own pool release -
    // debt-driven bankruptcy always fully sells houses first, leaving none
    // for `bankrupt()` to touch.
    let engine = engine_with_rules(|r| {
        r.subsidiary_pool_factor = 6; // 8 for 2 players
        r.conglomerate_pool_factor = 3; // 4 for 2 players
    });
    let mut st = two_players(&engine);
    let initial_subs = st.subsidiaries_available;
    let initial_congs = st.conglomerates_available;

    st.tiles[2].owner = Some(0);
    st.tiles[3].owner = Some(0);
    st.tiles[2].houses = 5; // conglomerate level
    st.tiles[3].houses = 3; // subsidiary level
    // Model these levels as actually drawn from the pool.
    st.subsidiaries_available = initial_subs.map(|n| n - 3);
    st.conglomerates_available = initial_congs.map(|n| n - 1);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Resign));
    assert!(st.players[0].bankrupt);
    assert_eq!(st.tiles[2].houses, 0);
    assert_eq!(st.tiles[3].houses, 0);
    assert_eq!(
        st.subsidiaries_available, initial_subs,
        "conserved: released back on resignation"
    );
    assert_eq!(
        st.conglomerates_available, initial_congs,
        "conserved: released back on resignation"
    );
}

#[test]
fn takeover_liquidates_improved_tile_and_refunds_old_owner() {
    let engine = engine_with_rules(|r| {
        r.expropriation = 200;
        r.subsidiary_pool_factor = 6; // 8 for 2 players
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1); // p1 owns ave_a (price 60, house_cost 50)
    st.tiles[2].houses = 3;
    st.subsidiaries_available = st.subsidiaries_available.map(|n| n - 3); // drawn for those 3
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 2;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::Expropriated {
            player: 0,
            from: 1,
            tile: 2,
            cost: 120,
            liquidated: 3,
            liquidation_refund: 75, // 3 * (50 / 2)
        }
    )));
    assert_eq!(st.tiles[2].houses, 0, "the taker gets a bare tile");
    assert_eq!(
        st.players[0].cash,
        1500 - 120,
        "seizer pays the flat cost only"
    );
    assert_eq!(
        st.players[1].cash,
        1500 + 60 + 75,
        "former owner gets compensation plus the liquidation refund"
    );
    assert_eq!(
        st.subsidiaries_available,
        Some(8),
        "the 3 liquidated levels return to the pool"
    );
}

#[test]
fn takeover_of_conglomerate_tile_returns_one_conglomerate() {
    let engine = engine_with_rules(|r| {
        r.expropriation = 200;
        r.conglomerate_pool_factor = 3; // 4 for 2 players
    });
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1);
    st.tiles[2].houses = 5; // conglomerate level
    st.conglomerates_available = st.conglomerates_available.map(|n| n - 1);
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 2;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::Expropriated {
            liquidated: 5,
            liquidation_refund: 125, // 5 * (50 / 2)
            ..
        }
    )));
    assert_eq!(st.tiles[2].houses, 0);
    assert_eq!(
        st.conglomerates_available,
        Some(4),
        "the tile's conglomerate returns, not subsidiaries"
    );
}

#[test]
fn rent_multiplier_composes_with_rent_boost() {
    let engine = engine_with(plain_board()); // value 2 lands on ave_a (index 2)
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(0); // p0 owns ave_a (price 60, rents[0] = 2)
    st.tiles[2].boosts = 1; // ADR-0012: base 2 -> boosted 3
    st.forecast.active = Some(ActiveMarketEvent {
        event_id: "crash".into(),
        effect: MarketEffect::RentMultiplier,
        magnitude_pct: -50,
        ends_at_turn: 10,
    });
    st.current = 1;
    st.players[1].position = 0;
    st.turn = TurnPhase::AwaitMove;

    let (_, ev) = play(&engine, &st, "p1", 2);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 1, .. })),
        "boosted rent 3, then -50% market crash -> 1"
    );
}

#[test]
fn rent_multiplier_expires_exactly_at_its_scheduled_turn() {
    let engine = engine_with(plain_board());
    let mut st = two_players(&engine);
    st.forecast.active = Some(ActiveMarketEvent {
        event_id: "crash".into(),
        effect: MarketEffect::RentMultiplier,
        magnitude_pct: -50,
        ends_at_turn: 1,
    });
    st.turn = TurnPhase::AwaitEnd;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.turn_count, 1);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::MarketEventExpired { event_id } if event_id == "crash"))
    );
    assert!(st.forecast.active.is_none());
}

#[test]
fn acquisition_multiplier_scales_takeover_cost_and_compensation() {
    let engine = engine_with_rules(|r| r.expropriation = 200);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1); // p1 owns ave_a (price 60)
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 2;
    st.forecast.active = Some(ActiveMarketEvent {
        event_id: "bubble".into(),
        effect: MarketEffect::AcquisitionMultiplier,
        magnitude_pct: -60,
        ends_at_turn: 10,
    });

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "ave_a".into(),
            },
        ),
    );
    // base cost = 60 * 200 / 100 = 120; -60% market discount -> 48, below
    // the bare price (60), so compensation drops with it (min(price, cost)).
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::Expropriated { cost: 48, .. }))
    );
    assert_eq!(
        st.players[0].cash,
        1500 - 48,
        "seizer pays the discounted cost"
    );
    assert_eq!(
        st.players[1].cash,
        1500 + 48,
        "compensation caps at the discounted cost, not the bare price"
    );
}

#[test]
fn wealth_tax_charges_every_alive_player_via_bankruptcy_path() {
    let events = vec![market_event("audit", MarketEffect::WealthTax, 90, 0)];
    let engine = engine_with_forecast(events, 1, |_| {});
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
        ],
        1,
    );
    st.current = 0; // seed-drawn starter (2026-07); the script needs p0
    st.tiles[2].owner = Some(2); // p2 owns ave_a (price 60), cash-poor
    st.players[2].cash = 5;
    st.turn = TurnPhase::AwaitEnd;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert!(
        ev.iter().any(
            |e| matches!(e, Event::MarketEventActivated { event_id, .. } if event_id == "audit")
        )
    );
    // p0/p1: net worth 1500, no properties -> 90% = 1350, fully payable.
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::CashAdjusted {
            player: 0,
            delta: -1350,
            ..
        }
    )));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::CashAdjusted {
            player: 1,
            delta: -1350,
            ..
        }
    )));
    assert_eq!(st.players[0].cash, 150);
    assert_eq!(st.players[1].cash, 150);
    // p2: net worth 65 (5 cash + 60 equity) -> 90% = 58; only 5 cash, and
    // mortgaging ave_a (+30) still falls short -> forced bankrupt.
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::CashAdjusted {
            player: 2,
            delta: -58,
            ..
        }
    )));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::PropertyMortgaged { tile: 2, .. }))
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PlayerBankrupt {
            player: 2,
            creditor: None
        }
    )));
    assert!(st.players[2].bankrupt);
    assert_eq!(st.players[2].cash, 0);
    assert_eq!(st.tiles[2].owner, None, "bank repossesses");
    assert!(!st.tiles[2].mortgaged, "bank refurbishes on repossession");
    assert!(
        st.forecast.active.is_none(),
        "wealth tax never occupies the active slot"
    );
    assert!(
        !ev.iter()
            .any(|e| matches!(e, Event::MarketEventExpired { .. }))
    );
    assert_eq!(st.phase, GamePhase::Active, "2 of 3 players remain");
}

#[test]
fn wealth_tax_can_end_the_game() {
    let events = vec![market_event("audit", MarketEffect::WealthTax, 100, 0)];
    let engine = engine_with_forecast(events, 1, |_| {});
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1); // p1 owns ave_a (price 60)
    st.players[1].cash = 10;
    st.turn = TurnPhase::AwaitEnd;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::PlayerBankrupt {
            player: 1,
            creditor: None
        }
    )));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::GameEnded { winner: 0 }))
    );
    assert_eq!(st.phase, GamePhase::Finished { winner: 0 });
}

#[test]
fn boost_is_consumed_by_the_first_paid_rent() {
    let engine = engine_with_rules(|r| r.rent_boost = 100);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // blvd, singleton navy -> full group, rent 20
    st.tiles[6].boosts = 2; // +100%: rent 40

    st.current = 1;
    st.players[1].position = 3;
    st.turn = TurnPhase::AwaitMove;
    let (st, ev) = play(&engine, &st, "p1", 3);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 40, .. })),
        "the first landing still pays the boosted rate"
    );
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentBoostConsumed { tile: 6 })),
        "the paid boost is announced as consumed"
    );
    assert_eq!(st.tiles[6].boosts, 0, "one payment clears the whole boost");

    // The next landing pays the plain rate again.
    let mut st = st;
    st.current = 1;
    st.players[1].position = 3;
    st.players[1].hand = vec![3, 4]; // the first landing spent the 3
    st.turn = TurnPhase::AwaitMove;
    let (_, ev) = play(&engine, &st, "p1", 3);
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 20, .. })),
        "the trap is spent: back to base rent"
    );
}

#[test]
fn mortgaged_tile_buys_out_at_half_price_and_stays_mortgaged() {
    let engine = engine_with_rules(|r| r.expropriation = 200);
    let mut st = two_players(&engine);
    st.tiles[2].owner = Some(1); // p1 owns ave_a (price 60)...
    st.tiles[2].mortgaged = true; // ...mortgaged: the cheap-buyout weak point
    st.turn = TurnPhase::AwaitEnd;
    st.players[0].position = 2;

    let (st, ev) = step(
        &engine,
        &st,
        cmd(
            "p0",
            CommandKind::Expropriate {
                tile: "ave_a".into(),
            },
        ),
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::Expropriated {
            player: 0,
            from: 1,
            tile: 2,
            cost: 30, // flat mortgage value: price/2, not the 200% takeover
            liquidated: 0,
            liquidation_refund: 0,
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(0));
    assert!(
        st.tiles[2].mortgaged,
        "transfers as-is; buyer redeems at +10%"
    );
    assert_eq!(st.players[0].cash, 1500 - 30);
    assert_eq!(
        st.players[1].cash,
        1500 + 30,
        "the owner gets the full price/2"
    );
}

#[test]
fn networth_tax_takes_a_seeded_bracket_of_net_worth() {
    let engine = engine_with(audit_board());
    let mut st = two_players(&engine);
    st.tiles[1].owner = Some(0); // ave_a: +60 equity -> net worth 1560
    let net_worth = 1500 + 60;

    let (st, ev) = play(&engine, &st, "p0", 2); // 0 -> 2, the Audit
    let amount = ev
        .iter()
        .find_map(|e| match e {
            Event::TaxPaid { amount, .. } => Some(*amount),
            _ => None,
        })
        .expect("landing on the audit taxes the lander");
    let pct = (5..=25)
        .find(|p| net_worth * p / 100 == amount)
        .expect("the amount must be an exact 5-25% bracket of the lander's net worth");
    assert!(
        (5..=25).contains(&pct),
        "bracket {pct} outside the configured range"
    );
    assert_eq!(st.players[0].cash, 1500 - amount);

    // Same seed, same draw: the bracket comes from the game RNG.
    let engine2 = engine_with(audit_board());
    let mut st2 = two_players(&engine2);
    st2.tiles[1].owner = Some(0);
    let (_, ev2) = play(&engine2, &st2, "p0", 2);
    assert!(
        ev2.iter()
            .any(|e| matches!(e, Event::TaxPaid { amount: a, .. } if *a == amount)),
        "same seed must draw the same bracket"
    );
}

#[test]
fn networth_tax_validation_rejects_bad_brackets() {
    let mut content = audit_board();
    content.board[2] = tile(
        "audit",
        "The Audit",
        TileKind::NetWorthTax {
            min_pct: 30,
            max_pct: 10,
        },
    );
    assert!(matches!(
        content.validate(),
        Err(parcello_engine::ContentError::InvalidNetWorthTax(id)) if id == "audit"
    ));
}
