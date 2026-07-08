use std::env;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use parcello_engine::{
    CardDef, CardEffect, CommandKind, Engine, Event, GameContent, GamePhase, GameState,
    PlayerCommand, PropertyDef, RentModel, RuleParams, TileDef, TileKind, TurnPhase,
};

const DEFAULT_ITERATIONS: usize = 1_000;
const STEPS_PER_GAME: usize = 250;

#[derive(Debug, Clone)]
struct FuzzRng(u64);

impl FuzzRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        debug_assert!(bound > 0);
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

#[test]
fn fuzz_random_valid_game_states() {
    let iterations = env::var("PARCELLO_FUZZ_ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_ITERATIONS);
    let base_seed = env::var("PARCELLO_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0xC0DE_2026_0705);

    let content = Arc::new(fuzz_content());
    let engine = Engine::new(content.clone()).expect("valid fuzz content");

    for iteration in 0..iterations {
        let seed = base_seed ^ ((iteration as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
        run_one_game(&engine, &content, seed, iteration);
    }
}

fn run_one_game(engine: &Engine, content: &GameContent, seed: u64, iteration: usize) {
    let mut rng = FuzzRng::new(seed);
    let player_count = 2 + rng.below(3);
    let players = (0..player_count)
        .map(|i| (format!("p{i}"), format!("Player {i}")))
        .collect();
    let mut state = engine.new_game(players, rng.next());

    assert_invariants(content, &state, "initial", seed, iteration, 0);

    for step in 0..STEPS_PER_GAME {
        if matches!(state.phase, GamePhase::Finished { .. }) {
            break;
        }

        let command = next_valid_command(content, &state, &mut rng);
        let before_cash = total_cash(&state);
        let result = catch_unwind(AssertUnwindSafe(|| engine.apply(&state, &command)));
        let (next, events) = match result {
            Ok(Ok(applied)) => applied,
            Ok(Err(err)) => panic!(
                "fuzzer generated rejected command: seed={seed} iteration={iteration} step={step} command={command:?} error={err:?}"
            ),
            Err(payload) => panic!(
                "engine panicked during fuzz run: seed={seed} iteration={iteration} step={step} command={command:?} panic={payload:?}"
            ),
        };

        assert_money_delta(
            content,
            before_cash,
            total_cash(&next),
            &events,
            seed,
            iteration,
            step,
        );
        state = next;
        assert_invariants(content, &state, "after command", seed, iteration, step);
    }
}

fn next_valid_command(
    content: &GameContent,
    state: &GameState,
    rng: &mut FuzzRng,
) -> PlayerCommand {
    let (actor, kind) = match &state.turn {
        TurnPhase::AwaitRoll => {
            let player = &state.players[state.current];
            if player.jail_turns.is_some() && player.jail_cards > 0 && rng.chance(3) {
                (state.current, CommandKind::UseJailCard)
            } else if player.jail_turns.is_some()
                && player.cash >= content.rules.jail_fine
                && rng.chance(3)
            {
                (state.current, CommandKind::PayJailFine)
            } else if let Some(kind) = random_asset_command(content, state, state.current, rng) {
                (state.current, kind)
            } else {
                (state.current, CommandKind::Roll)
            }
        }
        TurnPhase::AwaitEnd => {
            if let Some(kind) = random_asset_command(content, state, state.current, rng) {
                (state.current, kind)
            } else {
                (state.current, CommandKind::EndTurn)
            }
        }
        TurnPhase::BlindAuction { tile, bids } => {
            let floor = content
                .property(*tile)
                .expect("BlindAuction targets property")
                .price;
            let pending: Vec<usize> = state
                .alive_players()
                .filter(|&s| bids[s].is_none())
                .collect();
            let seat = pending[rng.below(pending.len())];
            let cash = state.players[seat].cash;
            let is_discoverer = seat == state.current;
            let amount = if is_discoverer && cash >= floor && rng.chance(2) {
                floor + rng.below((cash - floor + 1) as usize) as i64
            } else if !is_discoverer && cash > 0 && rng.chance(2) {
                rng.below((cash + 1) as usize) as i64
            } else {
                0
            };
            (seat, CommandKind::SubmitBlindBid { amount })
        }
    };

    PlayerCommand {
        player: state.players[actor].id.clone(),
        kind,
    }
}

fn random_asset_command(
    content: &GameContent,
    state: &GameState,
    player: usize,
    rng: &mut FuzzRng,
) -> Option<CommandKind> {
    let mut choices = Vec::new();
    for (tile, def) in content.board.iter().enumerate() {
        let Some(prop) = content.property(tile) else {
            continue;
        };
        let tile_state = state.tiles[tile];
        if tile_state.owner == Some(player) {
            if tile_state.houses == 0
                && !tile_state.mortgaged
                && group_has_no_houses(content, state, &prop.group)
            {
                choices.push(CommandKind::Mortgage {
                    tile: def.id.clone(),
                });
            }
            if tile_state.mortgaged {
                let principal = prop.price / 2;
                let cost = principal + principal / 10;
                if state.players[player].cash >= cost {
                    choices.push(CommandKind::Unmortgage {
                        tile: def.id.clone(),
                    });
                }
            }
            let cap = content.rules.max_houses_per_property.min(5);
            if prop.rent_model == RentModel::Houses
                && state.owns_full_group(content, player, &prop.group)
                && group_has_no_mortgages(content, state, &prop.group)
                && tile_state.houses < cap
                && state.players[player].cash >= prop.house_cost
                && can_build_evenly(content, state, tile, &prop.group)
            {
                // Shared building pools (ADR-0019): only offer a Build the
                // matching pool can actually satisfy right now, same as the
                // cash check above - `next_valid_command` must always be
                // accepted.
                let becomes_top = tile_state.houses + 1 == cap;
                let pool_ok = if becomes_top {
                    state.conglomerates_available != Some(0)
                } else {
                    state.subsidiaries_available != Some(0)
                };
                if pool_ok {
                    choices.push(CommandKind::Build {
                        tile: def.id.clone(),
                    });
                }
            }
            if tile_state.houses > 0 && can_sell_evenly(content, state, tile, &prop.group) {
                // Stepping off the top level re-issues cap-1 subsidiaries;
                // only offer it when the pool can cover that right now.
                let steps_off_top = tile_state.houses == cap;
                let pool_ok = !steps_off_top
                    || state
                        .subsidiaries_available
                        .is_none_or(|n| n >= (cap - 1) as u64);
                if pool_ok {
                    choices.push(CommandKind::SellHouse {
                        tile: def.id.clone(),
                    });
                }
            }
            let boost_cost = prop.price * content.rules.rent_boost / 100;
            if content.rules.rent_boost > 0
                && !tile_state.mortgaged
                && tile_state.boosts < 3
                && state.players[player].cash >= boost_cost
            {
                choices.push(CommandKind::BoostRent {
                    tile: def.id.clone(),
                });
            }
        } else if let Some(owner) = tile_state.owner
            && owner != player
            && !tile_state.mortgaged
            // ADR-0022: takeover only applies to the tile just landed on;
            // improved tiles are legal targets too (liquidated on seizure).
            && matches!(state.turn, TurnPhase::AwaitEnd)
            && tile == state.players[player].position
        {
            let cost = prop.price * content.rules.expropriation / 100;
            if content.rules.expropriation > 0 && state.players[player].cash >= cost {
                choices.push(CommandKind::Expropriate {
                    tile: def.id.clone(),
                });
            }
        }
    }

    if choices.is_empty() || !rng.chance(4) {
        None
    } else {
        Some(choices.swap_remove(rng.below(choices.len())))
    }
}

fn assert_invariants(
    content: &GameContent,
    state: &GameState,
    context: &str,
    seed: u64,
    iteration: usize,
    step: usize,
) {
    let fail = |message: &str| {
        panic!("{message}: context={context} seed={seed} iteration={iteration} step={step}")
    };

    if state.players.len() < 2 || state.players.len() > 8 {
        fail("invalid player count");
    }
    if state.current >= state.players.len() {
        fail("current player index out of bounds");
    }
    if state.tiles.len() != content.board.len() {
        fail("tile state length does not match board");
    }
    if state.chance_deck.next > state.chance_deck.order.len() {
        fail("chance deck cursor out of bounds");
    }
    if state.community_deck.next > state.community_deck.order.len() {
        fail("community deck cursor out of bounds");
    }

    match state.phase {
        GamePhase::Active => {
            if state.players[state.current].bankrupt {
                fail("active game points at bankrupt current player");
            }
            let alive = state.players.iter().filter(|p| !p.bankrupt).count();
            if alive < 2 {
                fail("active game has fewer than two alive players");
            }
        }
        GamePhase::Finished { winner } => {
            if winner >= state.players.len() || state.players[winner].bankrupt {
                fail("finished game has invalid winner");
            }
        }
    }

    match &state.turn {
        TurnPhase::AwaitRoll | TurnPhase::AwaitEnd => {}
        TurnPhase::BlindAuction { tile, bids } => {
            let tile = *tile;
            if tile >= state.tiles.len() || content.property(tile).is_none() {
                fail("BlindAuction references a non-property tile");
            }
            if state.tiles[tile].owner.is_some() {
                fail("BlindAuction references an owned tile");
            }
            if bids.len() != state.players.len() {
                fail("BlindAuction bids vector size mismatches player count");
            }
            for (i, bid) in bids.iter().enumerate() {
                if let Some(amount) = bid
                    && (*amount < 0 || *amount > state.players[i].cash)
                {
                    fail("BlindAuction bid is negative or exceeds cash");
                }
            }
        }
    }

    for (idx, player) in state.players.iter().enumerate() {
        if player.cash < content.rules.bankruptcy_threshold && !player.bankrupt {
            fail("solvent player cash is below bankruptcy threshold");
        }
        if content.rules.bankruptcy_threshold >= 0 && player.cash < 0 {
            fail("negative cash with non-negative bankruptcy threshold");
        }
        if player.position >= content.board.len() || player.doubles_streak > 2 {
            fail("player position or doubles streak is invalid");
        }
        if player.bankrupt
            && (player.jail_turns.is_some() || player.doubles_streak != 0 || player.jail_cards != 0)
        {
            fail("bankrupt player retains turn-only state");
        }
        for trade in &state.pending_trades {
            if (trade.from == idx || trade.to == idx) && player.bankrupt {
                fail("bankrupt player is party to a pending trade");
            }
        }
    }

    let cap = content.rules.max_houses_per_property.min(5);
    for tile in &state.tiles {
        if let Some(owner) = tile.owner
            && (owner >= state.players.len() || state.players[owner].bankrupt)
        {
            fail("tile owner is invalid");
        }
        if tile.houses > cap || tile.boosts > 3 {
            fail("tile improvement state is out of range");
        }
    }

    // Shared building pools (ADR-0019): the fixed total (computed from the
    // same formula `GameState::new` uses) must always equal what's still in
    // the pool plus what's currently checked out across every tile.
    if content.rules.subsidiary_pool_factor > 0 {
        let total = pool_size(content.rules.subsidiary_pool_factor, state.players.len());
        let in_use: u64 = state
            .tiles
            .iter()
            .filter(|t| t.houses > 0 && t.houses < cap)
            .map(|t| t.houses as u64)
            .sum();
        if state.subsidiaries_available.unwrap_or(0) + in_use != total {
            fail("subsidiary pool is not conserved");
        }
    }
    if content.rules.conglomerate_pool_factor > 0 {
        let total = pool_size(content.rules.conglomerate_pool_factor, state.players.len());
        let in_use = state.tiles.iter().filter(|t| t.houses == cap).count() as u64;
        if state.conglomerates_available.unwrap_or(0) + in_use != total {
            fail("conglomerate pool is not conserved");
        }
    }
}

/// Mirrors the pool-sizing formula in `GameState::new` (ADR-0019):
/// `round(factor * sqrt(players))`.
fn pool_size(factor: i64, players: usize) -> u64 {
    (factor as f64 * (players as f64).sqrt()).round() as u64
}

fn assert_money_delta(
    content: &GameContent,
    before: i64,
    after: i64,
    events: &[Event],
    seed: u64,
    iteration: usize,
    step: usize,
) {
    let bankrupted: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            Event::PlayerBankrupt { player, .. } => Some(*player),
            _ => None,
        })
        .collect();

    // Every event type has an exactly-known cash effect except a debt paid
    // to the bank (Tax/JailFine, or a negative card CashAdjusted - none
    // have a creditor) that triggers the payer's bankruptcy: the
    // partial-settlement path (`charge()` in apply.rs) deducts only
    // whatever cash liquidation could raise, not the full nominal amount
    // the event logs - so the true contribution is only known to lie
    // somewhere in `[-amount, 0]`, not a fixed value.
    let mut min_expected = 0i64;
    let mut max_expected = 0i64;
    for event in events {
        let (lo, hi) = match event {
            Event::SalaryPaid { amount, .. } => (*amount, *amount),
            Event::BlindAuctionResolved {
                winner: Some(_),
                amount,
                ..
            } => (-*amount, -*amount),
            Event::TaxPaid { player, amount, .. } => {
                if bankrupted.contains(player) {
                    (-*amount, 0)
                } else {
                    (-*amount, -*amount)
                }
            }
            Event::CashAdjusted { player, delta, .. } => {
                if *delta < 0 && bankrupted.contains(player) {
                    (*delta, 0)
                } else {
                    (*delta, *delta)
                }
            }
            Event::HouseBuilt { cost, .. } => (-*cost, -*cost),
            Event::HouseSold { refund, .. } => (*refund, *refund),
            Event::Expropriated {
                tile,
                cost,
                liquidation_refund,
                ..
            } => {
                let compensation = content.property(*tile).expect("property").price.min(*cost);
                let delta = compensation + liquidation_refund - cost;
                (delta, delta)
            }
            Event::RentBoosted { cost, .. } => (-*cost, -*cost),
            Event::PropertyMortgaged { value, .. } => (*value, *value),
            Event::PropertyUnmortgaged { cost, .. } => (-*cost, -*cost),
            Event::JailFinePaid { player, amount, .. } => {
                if bankrupted.contains(player) {
                    (-*amount, 0)
                } else {
                    (-*amount, -*amount)
                }
            }
            _ => (0, 0),
        };
        min_expected += lo;
        max_expected += hi;
    }

    let actual = after - before;
    assert!(
        actual >= min_expected && actual <= max_expected,
        "cash delta out of range: seed={seed} iteration={iteration} step={step} actual={actual} expected=[{min_expected}, {max_expected}] events={events:?}"
    );
}

fn total_cash(state: &GameState) -> i64 {
    state.players.iter().map(|p| p.cash).sum()
}

fn group_has_no_houses(content: &GameContent, state: &GameState, group: &str) -> bool {
    content
        .group_tiles(group)
        .iter()
        .all(|&tile| state.tiles[tile].houses == 0)
}

fn group_has_no_mortgages(content: &GameContent, state: &GameState, group: &str) -> bool {
    content
        .group_tiles(group)
        .iter()
        .all(|&tile| !state.tiles[tile].mortgaged)
}

fn can_build_evenly(content: &GameContent, state: &GameState, tile: usize, group: &str) -> bool {
    let group_min = content
        .group_tiles(group)
        .iter()
        .map(|&t| state.tiles[t].houses)
        .min()
        .unwrap_or(0);
    state.tiles[tile].houses <= group_min
}

fn can_sell_evenly(content: &GameContent, state: &GameState, tile: usize, group: &str) -> bool {
    let group_max = content
        .group_tiles(group)
        .iter()
        .map(|&t| state.tiles[t].houses)
        .max()
        .unwrap_or(0);
    state.tiles[tile].houses >= group_max
}

fn fuzz_content() -> GameContent {
    GameContent {
        board: vec![
            tile("go", "Go", TileKind::Go),
            tile(
                "brown_1",
                "Brown 1",
                property(
                    "brown",
                    60,
                    50,
                    [2, 10, 30, 90, 160, 250],
                    RentModel::Houses,
                ),
            ),
            tile("chance_1", "Chance", TileKind::Chance),
            tile(
                "brown_2",
                "Brown 2",
                property(
                    "brown",
                    60,
                    50,
                    [4, 20, 60, 180, 320, 450],
                    RentModel::Houses,
                ),
            ),
            tile("tax_1", "Tax", TileKind::Tax { amount: 100 }),
            tile("jail", "Jail", TileKind::Jail),
            tile(
                "rail_1",
                "Rail 1",
                property(
                    "rail",
                    200,
                    0,
                    [25, 50, 100, 200, 0, 0],
                    RentModel::GroupScaled,
                ),
            ),
            tile("community_1", "Community", TileKind::Community),
            tile(
                "green_1",
                "Green 1",
                property(
                    "green",
                    220,
                    150,
                    [18, 90, 250, 700, 875, 1050],
                    RentModel::Houses,
                ),
            ),
            tile("go_to_jail", "Go To Jail", TileKind::GoToJail),
            tile(
                "rail_2",
                "Rail 2",
                property(
                    "rail",
                    200,
                    0,
                    [25, 50, 100, 200, 0, 0],
                    RentModel::GroupScaled,
                ),
            ),
            tile("parking", "Free Parking", TileKind::FreeParking),
            tile(
                "green_2",
                "Green 2",
                property(
                    "green",
                    220,
                    150,
                    [18, 90, 250, 700, 875, 1050],
                    RentModel::Houses,
                ),
            ),
        ],
        chance: vec![
            card("chance_cash", CardEffect::Money { amount: 50 }),
            card(
                "chance_go",
                CardEffect::MoveTo {
                    tile: "go".into(),
                    collect_go: true,
                },
            ),
            card("chance_back", CardEffect::MoveBy { steps: -3 }),
            card("chance_jail", CardEffect::GoToJail),
        ],
        community: vec![
            card("community_fee", CardEffect::Money { amount: -50 }),
            card("community_cash", CardEffect::Money { amount: 100 }),
            card("community_forward", CardEffect::MoveBy { steps: 2 }),
            card("community_jail_card", CardEffect::GetOutOfJail),
        ],
        rules: RuleParams {
            expropriation: 200,
            rent_boost: 25,
            // Small on purpose (ADR-0019): tight enough that build/sell
            // exhaustion actually gets exercised across a run.
            subsidiary_pool_factor: 2,
            conglomerate_pool_factor: 2,
            ..RuleParams::default()
        },
        market_events: vec![],
        forecast_gap_turns: 0,
    }
}

fn tile(id: &str, name: &str, kind: TileKind) -> TileDef {
    TileDef {
        id: id.into(),
        name: name.into(),
        kind,
    }
}

fn property(
    group: &str,
    price: i64,
    house_cost: i64,
    rents: [i64; 6],
    rent_model: RentModel,
) -> TileKind {
    TileKind::Property(PropertyDef {
        group: group.into(),
        price,
        house_cost,
        rents,
        rent_model,
    })
}

fn card(id: &str, effect: CardEffect) -> CardDef {
    CardDef {
        id: id.into(),
        text: id.into(),
        effect,
    }
}
