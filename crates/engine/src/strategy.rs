//! Strategy pattern: rule fragments the mod layer can substitute at room
//! creation (V2: via WASM export binding). All traits are object-safe and
//! `Send + Sync` so a boxed instance can live inside a room task.
//!
//! Implementations must stay deterministic: any randomness must come from
//! the `&mut u64` PRNG state they are handed, never from ambient sources.

use crate::content::{GameContent, RentModel};
use crate::event::Event;
use crate::rng;
use crate::state::GameState;

pub trait DicePolicy: Send + Sync {
    fn roll(&self, rng: &mut u64) -> (u8, u8);
}

pub trait RentCalculator: Send + Sync {
    /// Rent owed for landing on `tile` (guaranteed: owned property, owner is
    /// not the lander). `dice_total` is provided for dice-scaled variants.
    fn rent(&self, content: &GameContent, state: &GameState, tile: usize, dice_total: u8) -> i64;
}

/// Called when a player cannot cover a debt from cash. May sell assets back
/// to the bank (mutating `state`) and must report what it did via events.
/// The engine bankrupts the player if cash remains below the debt.
pub trait BankruptcyResolver: Send + Sync {
    fn liquidate(
        &self,
        content: &GameContent,
        state: &mut GameState,
        debtor: usize,
        needed: i64,
        events: &mut Vec<Event>,
    );
}

// -- Default implementations -------------------------------------------------

pub struct UniformDice;

impl DicePolicy for UniformDice {
    fn roll(&self, rng: &mut u64) -> (u8, u8) {
        let d1 = 1 + rng::below(rng, 6) as u8;
        let d2 = 1 + rng::below(rng, 6) as u8;
        (d1, d2)
    }
}

/// Classic rules, dispatched on the tile's `RentModel`:
/// - Houses: rent by house level; unimproved rent doubles on a full group;
/// - GroupScaled: rent table indexed by tiles of the group owned (stations);
/// - DiceScaled: same index, multiplied by the dice total (utilities).
pub struct StandardRent;

impl RentCalculator for StandardRent {
    fn rent(&self, content: &GameContent, state: &GameState, tile: usize, dice: u8) -> i64 {
        let prop = content
            .property(tile)
            .expect("rent is only computed on property tiles");
        let owner = state.tiles[tile]
            .owner
            .expect("rent is only computed on owned tiles");
        match prop.rent_model {
            RentModel::Houses => {
                let houses = state.tiles[tile].houses as usize;
                let base = prop.rents[houses.min(prop.rents.len() - 1)];
                if houses == 0 && state.owns_full_group(content, owner, &prop.group) {
                    base * 2
                } else {
                    base
                }
            }
            RentModel::GroupScaled => prop.rents[group_rent_index(content, state, owner, prop)],
            RentModel::DiceScaled => {
                i64::from(dice) * prop.rents[group_rent_index(content, state, owner, prop)]
            }
        }
    }
}

/// `rents` index for the scaled models: tiles of the group owned, minus one.
/// Mortgaged tiles still count as owned (they collect nothing themselves).
fn group_rent_index(
    content: &GameContent,
    state: &GameState,
    owner: usize,
    prop: &crate::content::PropertyDef,
) -> usize {
    let owned = content
        .group_tiles(&prop.group)
        .iter()
        .filter(|&&t| state.tiles[t].owner == Some(owner))
        .count();
    owned.saturating_sub(1).min(5)
}

/// Default liquidation: sells the debtor's houses back to the bank at half
/// cost (most expensive first), then mortgages house-free properties
/// (highest value first), until the debt is covered or assets run out.
pub struct StandardLiquidation;

impl BankruptcyResolver for StandardLiquidation {
    fn liquidate(
        &self,
        content: &GameContent,
        state: &mut GameState,
        debtor: usize,
        needed: i64,
        events: &mut Vec<Event>,
    ) {
        // One house per iteration, always from a tile at its group's max
        // level (even-sell rule, same as the voluntary SellHouse command),
        // best refund first among the eligible tiles.
        while state.players[debtor].cash < needed {
            let candidate = (0..state.tiles.len())
                .filter(|&t| {
                    state.tiles[t].owner == Some(debtor)
                        && state.tiles[t].houses > 0
                        && content.property(t).is_some_and(|p| {
                            let group_max = content
                                .group_tiles(&p.group)
                                .iter()
                                .map(|&g| state.tiles[g].houses)
                                .max()
                                .unwrap_or(0);
                            state.tiles[t].houses == group_max
                        })
                })
                .max_by_key(|&t| content.property(t).map(|p| p.house_cost).unwrap_or(0));
            let Some(tile) = candidate else { break };
            let refund = content
                .property(tile)
                .map(|p| p.house_cost / 2)
                .unwrap_or(0);
            state.tiles[tile].houses -= 1;
            state.players[debtor].cash += refund;
            events.push(Event::HouseSold {
                player: debtor,
                tile,
                houses: state.tiles[tile].houses,
                refund,
            });
        }
        if state.players[debtor].cash >= needed {
            return;
        }

        // Mortgage phase. Only tiles whose whole group is house-free are
        // eligible (classic rule); after the sale loop above, the debtor's
        // remaining houses are exactly the ones the debt did not require.
        let mut mortgageable: Vec<usize> = (0..state.tiles.len())
            .filter(|&t| {
                let owned = state.tiles[t].owner == Some(debtor) && !state.tiles[t].mortgaged;
                owned
                    && content.property(t).is_some_and(|p| {
                        content
                            .group_tiles(&p.group)
                            .iter()
                            .all(|&g| state.tiles[g].houses == 0)
                    })
            })
            .collect();
        mortgageable
            .sort_by_key(|&t| std::cmp::Reverse(content.property(t).map(|p| p.price).unwrap_or(0)));
        for tile in mortgageable {
            if state.players[debtor].cash >= needed {
                break;
            }
            let value = content.property(tile).map(|p| p.price / 2).unwrap_or(0);
            state.tiles[tile].mortgaged = true;
            state.players[debtor].cash += value;
            events.push(Event::PropertyMortgaged {
                player: debtor,
                tile,
                value,
            });
        }
    }
}
