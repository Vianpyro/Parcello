//! Shared fixtures and helpers for the engine integration tests -
//! boards, players, command shorthands. Split out of the original
//! monolithic engine.rs (2026-07). Re-exports the engine types so a
//! test file needs only `use common::*;`.
// Each test binary compiles this module separately and uses a
// different subset of the fixtures and re-exports.
#![allow(dead_code)]
#![allow(unused_imports)]

pub use parcello_engine::strategy::StandardRent;
pub use parcello_engine::{
    ActiveMarketEvent, CardDef, CardEffect, ClientView, CommandError, CommandKind, Engine, Event,
    GameContent, GamePhase, GameState, MarketEffect, MarketEventDef, PlayerCommand, PropertyDef,
    RentCalculator, RentModel, RuleParams, Spotlight, TileDef, TileKind, TurnPhase,
};
pub use std::sync::Arc;

pub fn tile(id: &str, name: &str, kind: TileKind) -> TileDef {
    TileDef {
        id: id.into(),
        name: name.into(),
        kind,
    }
}

pub fn prop(group: &str, price: i64, house_cost: i64, rents: [i64; 6]) -> TileKind {
    TileKind::Property(PropertyDef {
        group: group.into(),
        price,
        house_cost,
        rents,
        rent_model: RentModel::Houses,
    })
}

pub fn scaled_prop(group: &str, price: i64, rents: [i64; 6], rent_model: RentModel) -> TileKind {
    TileKind::Property(PropertyDef {
        group: group.into(),
        price,
        house_cost: 0,
        rents,
        rent_model,
    })
}

/// 0 go, 1 park, 2-3 transit pair (group-scaled), 4 works (dice-scaled), 5 jail.
/// 0 go, 1 park, 2-3 transit pair (group-scaled), 4 jail.
pub fn transit_board() -> GameContent {
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
pub fn plain_board() -> GameContent {
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

pub fn engine_with(content: GameContent) -> Engine {
    Engine::new(Arc::new(content)).expect("valid test content")
}

pub fn two_players(engine: &Engine) -> GameState {
    let mut st = engine.new_game(
        vec![("p0".into(), "Alice".into()), ("p1".into(), "Bob".into())],
        42,
    );
    // The starting player is seed-drawn since the 2026-07 alpha tuning;
    // these tests script p0's moves, so pin the draw back to seat 0.
    st.current = 0;
    st
}

pub fn cmd(player: &str, kind: CommandKind) -> PlayerCommand {
    PlayerCommand {
        player: player.into(),
        kind,
    }
}

// By-value on purpose: ~100 call sites build the command inline and a
// reference would just add `&` noise to every one.
#[allow(clippy::needless_pass_by_value)]
pub fn step(engine: &Engine, st: &GameState, c: PlayerCommand) -> (GameState, Vec<Event>) {
    engine.apply(st, &c).expect("command accepted")
}

/// Plays a movement card for `player` (ADR-0017) - the deterministic
/// replacement for a dice roll; `value` must be in the player's hand.
pub fn play(engine: &Engine, st: &GameState, player: &str, value: u8) -> (GameState, Vec<Event>) {
    step(
        engine,
        st,
        cmd(player, CommandKind::PlayMovementCard { value }),
    )
}

pub fn card_board(chance: Vec<CardDef>) -> GameContent {
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

pub fn engine_with_rules(set: impl FnOnce(&mut RuleParams)) -> Engine {
    let mut content = plain_board();
    set(&mut content.rules);
    Engine::new(Arc::new(content)).expect("valid content")
}

/// A single market event definition, for tests that need exactly one.
pub fn market_event(
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

pub fn engine_with_forecast(
    events: Vec<MarketEventDef>,
    gap_turns: u32,
    set_rules: impl FnOnce(&mut RuleParams),
) -> Engine {
    let mut content = plain_board();
    content.market_events = events;
    content.forecast_gap_turns = gap_turns;
    set_rules(&mut content.rules);
    Engine::new(Arc::new(content)).expect("valid content")
}

pub fn offer(
    to: &str,
    give_cash: i64,
    give_tiles: &[&str],
    receive_cash: i64,
    receive_tiles: &[&str],
) -> CommandKind {
    CommandKind::ProposeTrade {
        to: to.into(),
        give_cash,
        give_tiles: give_tiles
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        receive_cash,
        receive_tiles: receive_tiles
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
    }
}

/// 0 go, 1-2 two owned-property candidates (brown pair), 3 the Exposition
/// corner, 4 jail. A 5-tile ring so a single movement card (1..=5, the
/// default velocity range) can land directly on any tile without wrapping.
pub fn spotlight_board() -> GameContent {
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
            tile("exposition", "The Exposition", TileKind::Spotlight),
            tile("jail", "Jail", TileKind::Jail),
        ],
        chance: vec![],
        community: vec![],
        rules: RuleParams::default(),
        market_events: vec![],
        forecast_gap_turns: 0,
    }
}

pub fn spotlight_engine(set: impl FnOnce(&mut RuleParams)) -> Engine {
    let mut content = spotlight_board();
    set(&mut content.rules);
    Engine::new(Arc::new(content)).expect("valid content")
}

/// 0 go, 1 filler property, 2 the Audit (5-25% of net worth), 3 jail.
pub fn audit_board() -> GameContent {
    GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile(
                "ave_a",
                "Ave A",
                prop("brown", 60, 50, [2, 10, 30, 90, 160, 250]),
            ),
            tile(
                "audit",
                "The Audit",
                TileKind::NetWorthTax {
                    min_pct: 5,
                    max_pct: 25,
                },
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
