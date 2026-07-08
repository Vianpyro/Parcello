//! Engine behavior tests. Dice are injected through a scripted `DicePolicy`
//! so every scenario is fully deterministic. A few tests reach into public
//! state fields to set up scenarios directly; this is a test-only shortcut.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use parcello_engine::strategy::StandardRent;
use parcello_engine::{
    ActiveMarketEvent, CardDef, CardEffect, ClientView, CommandError, CommandKind, DicePolicy,
    Engine, Event, GameContent, GamePhase, GameState, MarketEffect, MarketEventDef, PlayerCommand,
    PropertyDef, RentCalculator, RentModel, RuleParams, TileDef, TileKind, TurnPhase,
};

struct FixedDice(Mutex<VecDeque<(u8, u8)>>);

impl FixedDice {
    fn new(rolls: &[(u8, u8)]) -> Box<Self> {
        Box::new(Self(Mutex::new(rolls.iter().copied().collect())))
    }
}

impl DicePolicy for FixedDice {
    fn roll(&self, _rng: &mut u64) -> (u8, u8) {
        self.0
            .lock()
            .expect("dice script mutex")
            .pop_front()
            .expect("dice script exhausted")
    }
}

fn tile(id: &str, name: &str, kind: TileKind) -> TileDef {
    TileDef {
        id: id.into(),
        name: name.into(),
        kind,
    }
}

fn prop(group: &str, price: i64, house_cost: i64, rents: [i64; 6]) -> TileKind {
    TileKind::Property(PropertyDef {
        group: group.into(),
        price,
        house_cost,
        rents,
        rent_model: RentModel::Houses,
    })
}

fn scaled_prop(group: &str, price: i64, rents: [i64; 6], rent_model: RentModel) -> TileKind {
    TileKind::Property(PropertyDef {
        group: group.into(),
        price,
        house_cost: 0,
        rents,
        rent_model,
    })
}

/// 0 go, 1 park, 2-3 transit pair (group-scaled), 4 works (dice-scaled), 5 jail.
fn transit_board() -> GameContent {
    GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile("park", "Park", TileKind::FreeParking),
            tile(
                "station_a",
                "Station A",
                scaled_prop(
                    "transit",
                    200,
                    [25, 50, 100, 200, 0, 0],
                    RentModel::GroupScaled,
                ),
            ),
            tile(
                "station_b",
                "Station B",
                scaled_prop(
                    "transit",
                    200,
                    [25, 50, 100, 200, 0, 0],
                    RentModel::GroupScaled,
                ),
            ),
            tile(
                "works_a",
                "Works A",
                scaled_prop("works", 150, [4, 10, 0, 0, 0, 0], RentModel::DiceScaled),
            ),
            tile("jail", "Jail", TileKind::Jail),
        ],
        chance: vec![],
        community: vec![],
        rules: RuleParams::default(),
        market_events: vec![],
        forecast_gap_turns: 0,
    }
}

/// 9-tile board without card tiles: deterministic without deck control.
/// 0 go, 1 tax(100), 2-3 brown pair, 4 parking, 5 jail, 6 navy, 7 go-to-jail, 8 parking.
fn plain_board() -> GameContent {
    GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile("tax", "City Tax", TileKind::Tax { amount: 100 }),
            tile(
                "ave_a",
                "Ave A",
                prop("brown", 60, 50, [2, 10, 30, 90, 160, 250]),
            ),
            tile(
                "ave_b",
                "Ave B",
                prop("brown", 60, 50, [4, 20, 60, 180, 320, 450]),
            ),
            tile("park_1", "Park", TileKind::FreeParking),
            tile("jail", "Jail", TileKind::Jail),
            tile(
                "blvd",
                "Blvd",
                prop("navy", 100, 50, [10, 50, 150, 450, 625, 750]),
            ),
            tile("gtj", "Go To Jail", TileKind::GoToJail),
            tile("park_2", "Park", TileKind::FreeParking),
        ],
        chance: vec![],
        community: vec![],
        rules: RuleParams::default(),
        market_events: vec![],
        forecast_gap_turns: 0,
    }
}

fn engine_with(content: GameContent, rolls: &[(u8, u8)]) -> Engine {
    Engine::new(Arc::new(content))
        .expect("valid test content")
        .with_dice(FixedDice::new(rolls))
}

fn two_players(engine: &Engine) -> GameState {
    engine.new_game(
        vec![("p0".into(), "Alice".into()), ("p1".into(), "Bob".into())],
        42,
    )
}

fn cmd(player: &str, kind: CommandKind) -> PlayerCommand {
    PlayerCommand {
        player: player.into(),
        kind,
    }
}

fn step(engine: &Engine, st: &GameState, c: PlayerCommand) -> (GameState, Vec<Event>) {
    engine.apply(st, &c).expect("command accepted")
}

#[test]
fn discoverer_wins_at_floor_when_uncontested_then_pays_rent() {
    let engine = engine_with(plain_board(), &[(1, 2), (1, 2)]);
    let st = two_players(&engine);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
    assert_eq!(st.tiles[3].owner, Some(0));
    assert_eq!(st.players[0].cash, 1500 - 60);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.current, 1);

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
    assert_eq!(st.players[0].cash, 1500 - 60 + 4);
}

#[test]
fn monopoly_doubles_unimproved_rent_and_allows_building() {
    let engine = engine_with(plain_board(), &[(1, 2)]);
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
    st.turn = TurnPhase::AwaitRoll;
    let (st3, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
fn three_doubles_send_to_jail() {
    let engine = engine_with(plain_board(), &[(2, 2), (3, 3), (1, 1)]);
    let st = two_players(&engine);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll)); // -> park (4)
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn)); // doubles: extra roll
    assert_eq!(st.current, 0);
    assert_eq!(st.turn, TurnPhase::AwaitRoll);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll)); // wraps to tax (1)
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::SalaryPaid {
            player: 0,
            amount: 200
        }
    )));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::TaxPaid { amount: 100, .. }))
    );
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll)); // third double
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::WentToJail { player: 0 }))
    );
    assert_eq!(st.players[0].position, 5);
    assert_eq!(st.players[0].jail_turns, Some(0));

    // The jailing double grants no extra roll.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.current, 1);
}

#[test]
fn jail_pay_fine_then_roll() {
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.players[0].position = 5;
    st.players[0].jail_turns = Some(0);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::PayJailFine));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailFinePaid { amount: 50, .. }))
    );
    assert_eq!(st.players[0].jail_turns, None);
    assert_eq!(st.players[0].cash, 1450);
    assert_eq!(st.turn, TurnPhase::AwaitRoll);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert_eq!(st.players[0].position, 8);
}

#[test]
fn jail_third_failed_roll_forces_fine_and_moves() {
    let engine = engine_with(plain_board(), &[(1, 2), (2, 3), (1, 2)]);
    let mut st = two_players(&engine);
    st.players[0].position = 5;
    st.players[0].jail_turns = Some(0);

    for expected_turns in 1..=2u8 {
        let (next, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
        assert_eq!(next.players[0].jail_turns, Some(expected_turns));
        assert_eq!(next.players[0].position, 5);
        st = next;
        // Test shortcut: hand the turn straight back instead of simulating p1.
        st.current = 0;
        st.turn = TurnPhase::AwaitRoll;
    }

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailFinePaid { amount: 50, .. }))
    );
    assert_eq!(st.players[0].jail_turns, None);
    assert_eq!(st.players[0].position, 8);
    assert_eq!(st.players[0].cash, 1450);
}

#[test]
fn jail_card_is_held_then_spent_to_leave_jail() {
    let card = CardDef {
        id: "jail_free".into(),
        text: "Get out of jail free.".into(),
        effect: CardEffect::GetOutOfJail,
    };
    let engine = engine_with(card_board(vec![card]), &[(1, 2), (1, 2)]);
    let st = two_players(&engine);

    // Landing on chance banks the card instead of resolving an effect.
    let (mut st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailCardReceived { player: 0 }))
    );
    assert_eq!(st.players[0].jail_cards, 1);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);

    // Test shortcut: place p0 in jail with the turn back in hand.
    st.players[0].position = 4;
    st.players[0].jail_turns = Some(0);
    st.turn = TurnPhase::AwaitRoll;

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
    assert_eq!(st.players[0].jail_turns, None);
    assert_eq!(st.players[0].cash, 1500, "the card costs nothing");
    assert_eq!(st.turn, TurnPhase::AwaitRoll, "player still rolls normally");

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert_eq!(st.players[0].position, 2);
}

#[test]
fn jail_card_rejections_never_mutate() {
    let engine = engine_with(plain_board(), &[]);
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
    st.players[0].jail_turns = Some(0);
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::UseJailCard))
            .unwrap_err(),
        CommandError::NoJailCard
    );
    assert_eq!(st.players[0].jail_turns, Some(0));
}

#[test]
fn jail_third_failed_roll_spends_card_instead_of_fine() {
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.players[0].position = 5;
    st.players[0].jail_turns = Some(2); // two failed escapes already
    st.players[0].jail_cards = 1;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::JailCardUsed { player: 0 }))
    );
    assert!(
        !ev.iter().any(|e| matches!(e, Event::JailFinePaid { .. })),
        "the card replaces the forced fine"
    );
    assert_eq!(st.players[0].jail_cards, 0);
    assert_eq!(st.players[0].jail_turns, None);
    assert_eq!(st.players[0].cash, 1500);
    assert_eq!(st.players[0].position, 8);
}

#[test]
fn jail_escape_with_doubles_moves_and_grants_no_extra_roll() {
    let engine = engine_with(plain_board(), &[(2, 2)]);
    let mut st = two_players(&engine);
    st.players[0].position = 5;
    st.players[0].jail_turns = Some(0);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::LeftJail { player: 0 }))
    );
    // 5 + 4 wraps to Go: salary applies.
    assert_eq!(st.players[0].position, 0);
    assert_eq!(st.players[0].cash, 1700);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));
    assert_eq!(st.current, 1, "escape doubles must not grant an extra roll");
}

#[test]
fn unpayable_rent_bankrupts_and_ends_the_game() {
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0);
    st.players[1].cash = 5;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
            .apply(&st, &cmd("p0", CommandKind::Roll))
            .unwrap_err(),
        CommandError::GameFinished
    );
}

#[test]
fn liquidation_sells_houses_before_bankruptcy() {
    let engine = engine_with(plain_board(), &[(1, 2)]);
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

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
    let sold = ev
        .iter()
        .filter(|e| matches!(e, Event::HouseSold { .. }))
        .count();
    assert_eq!(sold, 1, "one house sale covers the 20 debt");
    assert!(!st.players[1].bankrupt);
    assert_eq!(st.players[1].cash, 25 - 20);
    assert_eq!(st.tiles[2].houses, 1);
}

fn card_board(chance: Vec<CardDef>) -> GameContent {
    GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile(
                "ave_a",
                "Ave A",
                prop("brown", 60, 50, [2, 10, 30, 90, 160, 250]),
            ),
            tile(
                "ave_b",
                "Ave B",
                prop("brown", 60, 50, [4, 20, 60, 180, 320, 450]),
            ),
            tile("chance", "Chance", TileKind::Chance),
            tile("jail", "Jail", TileKind::Jail),
        ],
        chance,
        community: vec![],
        rules: RuleParams::default(),
        market_events: vec![],
        forecast_gap_turns: 0,
    }
}

#[test]
fn money_card_adjusts_cash() {
    let card = CardDef {
        id: "dividend".into(),
        text: "Bank pays you 50.".into(),
        effect: CardEffect::Money { amount: 50 },
    };
    let engine = engine_with(card_board(vec![card]), &[(1, 2)]);
    let st = two_players(&engine);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
    let engine = engine_with(card_board(vec![card]), &[(1, 2)]);
    let st = two_players(&engine);

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert_eq!(st.players[0].position, 0);
    assert_eq!(st.players[0].cash, 1700);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
}

#[test]
fn resign_transfers_assets_to_bank_and_can_end_game() {
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[]);
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
    assert_eq!(st.turn, TurnPhase::AwaitRoll);
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
                TurnPhase::AwaitRoll => (st.current, CommandKind::Roll),
                TurnPhase::AwaitEnd => (st.current, CommandKind::EndTurn),
                TurnPhase::BlindAuction { bids, .. } => {
                    let seat = st
                        .alive_players()
                        .find(|&s| bids[s].is_none())
                        .expect("a phase stays BlindAuction only while someone is pending");
                    (seat, CommandKind::SubmitBlindBid { amount: 0 })
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
fn net_worth_counts_cash_property_and_houses() {
    let engine = engine_with(plain_board(), &[]);
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
fn finish_on_time_awards_the_richest_and_breaks_ties_low() {
    let engine = engine_with(plain_board(), &[]);
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

fn engine_with_rules(rolls: &[(u8, u8)], set: impl FnOnce(&mut RuleParams)) -> Engine {
    let mut content = plain_board();
    set(&mut content.rules);
    Engine::new(Arc::new(content))
        .expect("valid content")
        .with_dice(FixedDice::new(rolls))
}

/// A single market event definition, for tests that need exactly one.
fn market_event(
    id: &str,
    effect: MarketEffect,
    magnitude_pct: i64,
    duration_turns: u32,
) -> MarketEventDef {
    MarketEventDef {
        id: id.into(),
        name: id.into(),
        effect,
        magnitude_pct,
        duration_turns,
    }
}

fn engine_with_forecast(
    rolls: &[(u8, u8)],
    events: Vec<MarketEventDef>,
    gap_turns: u32,
    set_rules: impl FnOnce(&mut RuleParams),
) -> Engine {
    let mut content = plain_board();
    content.market_events = events;
    content.forecast_gap_turns = gap_turns;
    set_rules(&mut content.rules);
    Engine::new(Arc::new(content))
        .expect("valid content")
        .with_dice(FixedDice::new(rolls))
}

#[test]
fn expropriation_transfers_and_compensates() {
    let engine = engine_with_rules(&[], |r| r.expropriation = 200);
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
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with_rules(&[], |r| r.expropriation = 200);
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
    st.tiles[2].owner = Some(1);
    st.tiles[2].mortgaged = true;
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
        CommandError::NotExpropriable,
        "mortgaged tiles stay the takeover shield"
    );
    st.tiles[2].mortgaged = false;
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
    let engine = engine_with_rules(&[(1, 2)], |r| r.rent_boost = 100);
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
    st.turn = TurnPhase::AwaitRoll;
    let (_st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 30, .. })),
        "20 base rent x1.5 boost"
    );
}

#[test]
fn rent_boost_is_gated_and_bounded() {
    let engine = engine_with(plain_board(), &[]);
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

    let engine = engine_with_rules(&[], |r| r.rent_boost = 10);
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
fn win_by_controlling_full_groups() {
    // Two full groups win. p0 owns brown (ave_a, ave_b); seizing blvd (the
    // singleton navy group) completes a second group -> instant win.
    let engine = engine_with_rules(&[], |r| {
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
    let engine = engine_with_rules(&[], |r| r.expropriation = 100);
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
fn expropriation_requires_landing_on_the_tile() {
    // Rival-owned, unimproved, unmortgaged, and otherwise perfectly legal -
    // but the seizer is standing elsewhere (ADR-0022: takeover only applies
    // to the tile just landed on).
    let engine = engine_with_rules(&[], |r| r.expropriation = 200);
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
fn view_hides_rng_and_deck_order() {
    let engine = engine_with(plain_board(), &[]);
    let st = two_players(&engine);
    let view = ClientView::of(&st);
    let json = serde_json::to_string(&view).expect("view serializes");
    assert!(!json.contains("rng"));
    assert!(!json.contains("deck"));
    assert_eq!(view.players.len(), 2);
}

#[test]
fn seat_view_shows_only_own_trade_offers() {
    // 3 players; p0 offers to p1. p2's view must not contain the offer,
    // the omniscient view keeps it (ADR-0007).
    let engine = engine_with(plain_board(), &[]);
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
    assert_eq!(ClientView::of(&st).pending_trades.len(), 1);
    assert_eq!(ClientView::for_seat(&st, 0).pending_trades.len(), 1);
    assert_eq!(ClientView::for_seat(&st, 1).pending_trades.len(), 1);
    assert!(ClientView::for_seat(&st, 2).pending_trades.is_empty());
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
fn scaled_rent_models_follow_group_ownership_and_dice() {
    let content = transit_board();
    let engine = engine_with(content.clone(), &[(1, 2)]);
    let mut st = two_players(&engine);

    st.tiles[2].owner = Some(0);
    assert_eq!(
        StandardRent.rent(&content, &st, 2, 7),
        25,
        "one station owned"
    );
    st.tiles[3].owner = Some(0);
    assert_eq!(
        StandardRent.rent(&content, &st, 2, 7),
        50,
        "two stations owned"
    );

    st.tiles[4].owner = Some(0);
    assert_eq!(
        StandardRent.rent(&content, &st, 4, 7),
        28,
        "dice 7 x table 4"
    );

    // Wiring check: the dice total from the actual roll reaches the calculator.
    st.current = 1;
    st.players[1].position = 1;
    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::RentPaid {
            tile: 4,
            amount: 12,
            ..
        }
    )));
    assert_eq!(st.players[1].cash, 1500 - 12);
}

#[test]
fn scaled_rent_tiles_reject_building() {
    let engine = engine_with(transit_board(), &[]);
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
    let engine = engine_with(plain_board(), &[(1, 2)]);
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
    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
    assert!(!ev.iter().any(|e| matches!(e, Event::RentPaid { .. })));
    assert_eq!(st.players[0].cash, 1530);
    assert_eq!(st.players[1].cash, 1700);

    // Redeeming costs principal + 10% (floored): 30 + 3.
    let mut st = st;
    st.current = 0;
    st.turn = TurnPhase::AwaitRoll;
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
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy monopoly: rent 20
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
fn discoverer_wins_above_floor_with_discount_after_a_contest() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let st = two_players(&engine);

    // p0 lands on ave_a (tile 2, floor 60): the window opens for both seats.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
            amount: 72, // 90% of the 80 winning bid, floored
            ..
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(0));
    assert_eq!(st.players[0].cash, 1500 - 72);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
    assert_eq!(st.current, 0);
}

#[test]
fn all_zero_effective_bids_leave_the_tile_unsold() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let mut st = two_players(&engine);
    st.players[0].cash = 10; // broke discoverer: no implicit floor
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let players = vec![
        ("p0".to_string(), "Alice".to_string()),
        ("p1".to_string(), "Bob".to_string()),
        ("p2".to_string(), "Carol".to_string()),
    ];
    let st = engine.new_game(players, 42);
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
    assert_eq!(st.turn, TurnPhase::AwaitRoll);
}

fn offer(
    to: &str,
    give_cash: i64,
    give_tiles: &[&str],
    receive_cash: i64,
    receive_tiles: &[&str],
) -> CommandKind {
    CommandKind::ProposeTrade {
        to: to.into(),
        give_cash,
        give_tiles: give_tiles.iter().map(|s| s.to_string()).collect(),
        receive_cash,
        receive_tiles: receive_tiles.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn accepted_trade_swaps_tiles_and_cash_out_of_turn() {
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", offer("p1", 25, &[], 0, &[])));

    // Land on an unowned tile: a sealed-bid window opens, all trade actions reject.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
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
    let engine = engine_with(plain_board(), &[]);
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

#[test]
fn houses_build_and_sell_evenly_with_half_cost_refund() {
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy: rent 20 owed on landing
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 2;
    st.tiles[3].houses = 1;
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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

// -- Shared building pools (ADR-0019) ----------------------------------------

#[test]
fn build_consumes_and_sell_returns_subsidiary_pool() {
    let engine = engine_with_rules(&[], |r| r.subsidiary_pool_factor = 1); // pool = 1 for 2 players
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
    let engine = engine_with_rules(&[], |r| {
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
    let engine = engine_with_rules(&[], |r| {
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
    let engine = engine_with(plain_board(), &[]); // RuleParams::default(): factors 0
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
    let engine = engine_with(plain_board(), &[(1, 2)]);
    let mut st = two_players(&engine);
    st.tiles[6].owner = Some(0); // singleton navy: rent doubled to 20
    st.tiles[2].owner = Some(1);
    st.tiles[3].owner = Some(1);
    st.tiles[2].houses = 5; // at cap
    st.tiles[3].houses = 4;
    st.players[1].cash = 0;
    st.players[1].position = 3;
    st.current = 1;

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
    let engine = engine_with_rules(&[(1, 2)], |r| {
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

    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
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
    let engine = engine_with_rules(&[], |r| {
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
    let engine = engine_with_rules(&[], |r| {
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
    let engine = engine_with_rules(&[], |r| {
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

// -- Market forecast (ADR-0021) -----------------------------------------------

#[test]
fn forecast_seeded_at_new_game_is_deterministic_and_chained() {
    let events = vec![
        market_event("bubble", MarketEffect::AcquisitionMultiplier, -30, 5),
        market_event("crash", MarketEffect::RentMultiplier, -50, 4),
    ];
    let engine = engine_with_forecast(&[], events, 5, |_| {});
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
    let engine = engine_with(plain_board(), &[]); // plain_board ships no events
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
fn rent_multiplier_composes_with_rent_boost() {
    let engine = engine_with(plain_board(), &[(1, 1)]); // sum 2 -> lands on ave_a (index 2)
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
    st.turn = TurnPhase::AwaitRoll;

    let (_, ev) = step(&engine, &st, cmd("p1", CommandKind::Roll));
    assert!(
        ev.iter()
            .any(|e| matches!(e, Event::RentPaid { amount: 1, .. })),
        "boosted rent 3, then -50% market crash -> 1"
    );
}

#[test]
fn rent_multiplier_expires_exactly_at_its_scheduled_turn() {
    let engine = engine_with(plain_board(), &[]);
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
    let engine = engine_with_rules(&[], |r| r.expropriation = 200);
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
    let engine = engine_with_forecast(&[], events, 1, |_| {});
    let mut st = engine.new_game(
        vec![
            ("p0".into(), "P0".into()),
            ("p1".into(), "P1".into()),
            ("p2".into(), "P2".into()),
        ],
        1,
    );
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
    let engine = engine_with_forecast(&[], events, 1, |_| {});
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
