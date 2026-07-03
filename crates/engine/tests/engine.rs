//! Engine behavior tests. Dice are injected through a scripted `DicePolicy`
//! so every scenario is fully deterministic. A few tests reach into public
//! state fields to set up scenarios directly; this is a test-only shortcut.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use parcello_engine::strategy::StandardRent;
use parcello_engine::{
    CardDef, CardEffect, ClientView, CommandError, CommandKind, DicePolicy, Engine, Event,
    GameContent, GamePhase, GameState, PlayerCommand, PropertyDef, RentCalculator, RentModel,
    RuleParams, TileDef, TileKind, TurnPhase,
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
fn buy_then_pay_rent() {
    let engine = engine_with(plain_board(), &[(1, 2), (1, 2)]);
    let st = two_players(&engine);

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    assert_eq!(st.turn, TurnPhase::AwaitBuy { tile: 3 });
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::PurchaseOffered { tile: 3, .. })));

    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Buy));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TaxPaid { amount: 100, .. })));
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::EndTurn));

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Roll)); // third double
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::WentToJail { player: 0 })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::JailFinePaid { amount: 50, .. })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::JailFinePaid { amount: 50, .. })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::JailCardReceived { player: 0 })));
    assert_eq!(st.players[0].jail_cards, 1);
    assert_eq!(st.turn, TurnPhase::AwaitEnd);

    // Test shortcut: place p0 in jail with the turn back in hand.
    st.players[0].position = 4;
    st.players[0].jail_turns = Some(0);
    st.turn = TurnPhase::AwaitRoll;

    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::UseJailCard));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::JailCardUsed { player: 0 })));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::LeftJail { player: 0 })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::JailCardUsed { player: 0 })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::LeftJail { player: 0 })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::GameEnded { winner: 0 })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::GameEnded { winner: 0 })));
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
            let (actor, kind) = match st.turn {
                TurnPhase::AwaitRoll => (st.current, CommandKind::Roll),
                TurnPhase::AwaitBuy { .. } => (st.current, CommandKind::Decline),
                TurnPhase::AwaitEnd => (st.current, CommandKind::EndTurn),
                TurnPhase::Auction { turn, .. } => (turn, CommandKind::Pass),
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
fn declined_purchase_goes_to_auction_and_highest_bid_wins() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let st = two_players(&engine);

    // p0 lands on ave_a (tile 2) and declines: auction opens, p1 speaks first.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Decline));
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::AuctionStarted { tile: 2 })));
    assert!(matches!(
        st.turn,
        TurnPhase::Auction {
            tile: 2,
            high_bid: 0,
            high_bidder: None,
            turn: 1,
            ..
        }
    ));

    // Out-of-turn and invalid bids are rejected.
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::Bid { amount: 10 }))
            .unwrap_err(),
        CommandError::NotYourTurn
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::Bid { amount: 0 }))
            .unwrap_err(),
        CommandError::BidTooLow
    );
    assert_eq!(
        engine
            .apply(&st, &cmd("p1", CommandKind::Bid { amount: 9999 }))
            .unwrap_err(),
        CommandError::InsufficientFunds
    );

    // p1 bids 10, p0 raises to 25 (below the 60 list price), p1 passes.
    let (st, _) = step(&engine, &st, cmd("p1", CommandKind::Bid { amount: 10 }));
    assert_eq!(
        engine
            .apply(&st, &cmd("p0", CommandKind::Bid { amount: 10 }))
            .unwrap_err(),
        CommandError::BidTooLow
    );
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Bid { amount: 25 }));
    let (st, ev) = step(&engine, &st, cmd("p1", CommandKind::Pass));

    assert!(ev.iter().any(|e| matches!(
        e,
        Event::AuctionEnded {
            tile: 2,
            winner: Some(0),
            amount: 25
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(0));
    assert_eq!(st.players[0].cash, 1475);
    assert_eq!(st.turn, TurnPhase::AwaitEnd, "turn stays with the decliner");
    assert_eq!(st.current, 0);
}

#[test]
fn auction_with_no_bids_leaves_the_tile_unsold() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Decline));
    let (st, _) = step(&engine, &st, cmd("p1", CommandKind::Pass));
    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Pass));

    assert!(ev.iter().any(|e| matches!(
        e,
        Event::AuctionEnded {
            tile: 2,
            winner: None,
            amount: 0
        }
    )));
    assert_eq!(st.tiles[2].owner, None);
    assert_eq!(st.players[0].cash, 1500);
    assert_eq!(st.players[1].cash, 1500);
}

#[test]
fn high_bidder_resigning_reopens_the_auction() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let players = vec![
        ("p0".to_string(), "Alice".to_string()),
        ("p1".to_string(), "Bob".to_string()),
        ("p2".to_string(), "Carol".to_string()),
    ];
    let st = engine.new_game(players, 42);
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Decline));

    // p1 takes the high bid, then resigns while p2 is on the clock.
    let (st, _) = step(&engine, &st, cmd("p1", CommandKind::Bid { amount: 40 }));
    let (st, _) = step(&engine, &st, cmd("p1", CommandKind::Resign));
    assert!(matches!(
        st.turn,
        TurnPhase::Auction {
            high_bid: 0,
            high_bidder: None,
            ..
        }
    ));

    // Bidding reopened from zero: p2 takes it for 1.
    let (st, _) = step(&engine, &st, cmd("p2", CommandKind::Bid { amount: 1 }));
    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Pass));
    assert!(ev.iter().any(|e| matches!(
        e,
        Event::AuctionEnded {
            winner: Some(2),
            amount: 1,
            ..
        }
    )));
    assert_eq!(st.tiles[2].owner, Some(2));
}

#[test]
fn auction_rule_can_be_disabled() {
    let mut content = plain_board();
    content.rules.auction_on_decline = false;
    let engine = engine_with(content, &[(1, 1)]);
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    let (st, ev) = step(&engine, &st, cmd("p0", CommandKind::Decline));
    assert!(!ev.iter().any(|e| matches!(e, Event::AuctionStarted { .. })));
    assert_eq!(st.turn, TurnPhase::AwaitEnd);
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TradeAccepted { trade: 0, .. })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TradeDeclined { trade: 0, .. })));
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
    assert!(ev
        .iter()
        .any(|e| matches!(e, Event::TradeCancelled { trade: 0, .. })));
    assert!(st.pending_trades.is_empty());
}

#[test]
fn trades_are_blocked_during_auctions_and_purged_on_bankruptcy() {
    let engine = engine_with(plain_board(), &[(1, 1)]);
    let st = two_players(&engine);
    let (st, _) = step(&engine, &st, cmd("p0", offer("p1", 25, &[], 0, &[])));

    // Enter an auction: all trade actions reject.
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Roll));
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Decline));
    assert!(matches!(st.turn, TurnPhase::Auction { .. }));
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

    // Close the auction, then the proposer resigns: the offer is purged.
    let (st, _) = step(&engine, &st, cmd("p1", CommandKind::Pass));
    let (st, _) = step(&engine, &st, cmd("p0", CommandKind::Pass));
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
