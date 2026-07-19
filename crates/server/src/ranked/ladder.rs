//! Pure rating math and placement derivation for ranked games (ADR-0034).
//!
//! Weng-Lin (the `OpenSkill` model family) via `skillratings`: multiplayer
//! free-for-all native, closed-form, patent-free. Everything here is pure -
//! no I/O, no clock - so the whole ladder is unit-testable; the adapters in
//! `store.rs` call in for the actual updates.

use parcello_engine::{GameContent, GameState};
use skillratings::MultiTeamOutcome;
use skillratings::weng_lin::{WengLinConfig, WengLinRating, weng_lin_multi_team};

/// One player's skill estimate: mean and uncertainty, Weng-Lin's native
/// parameters. New players start at the model defaults (mu 25, sigma 25/3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rating {
    pub mu: f64,
    pub sigma: f64,
}

impl Default for Rating {
    fn default() -> Self {
        let d = WengLinRating::new();
        Self {
            mu: d.rating,
            sigma: d.uncertainty,
        }
    }
}

/// The number shown on the ladder (ADR-0034).
///
/// A conservative ordinal (`mu - 3*sigma`) scaled so a brand-new player
/// reads 1000 and climbs as sigma shrinks, floored at 0. Display only -
/// matching and updates use mu/sigma.
#[must_use]
pub fn display(r: Rating) -> i64 {
    let ordinal = 3.0f64.mul_add(-r.sigma, r.mu);
    (40.0f64.mul_add(ordinal, 1000.0).round()).max(0.0) as i64
}

/// New ratings for one finished game.
///
/// `ordered` is best-to-worst (index 0 won); the result is in the same
/// order. Every player is their own single-seat "team"; placements are
/// strict (the room's derivation never produces ties).
#[must_use]
pub fn rate(ordered: &[Rating]) -> Vec<Rating> {
    let teams: Vec<[WengLinRating; 1]> = ordered
        .iter()
        .map(|r| {
            [WengLinRating {
                rating: r.mu,
                uncertainty: r.sigma,
            }]
        })
        .collect();
    let ranked: Vec<(&[WengLinRating], MultiTeamOutcome)> = teams
        .iter()
        .enumerate()
        .map(|(place, team)| (&team[..], MultiTeamOutcome::new(place + 1)))
        .collect();
    weng_lin_multi_team(&ranked, &WengLinConfig::new())
        .into_iter()
        .map(|team| Rating {
            mu: team[0].rating,
            sigma: team[0].uncertainty,
        })
        .collect()
}

/// Best-to-worst seat ordering of a finished game (ADR-0034):
///
/// 1. the engine's declared winner - always placement 1, whatever the end
///    condition rewarded (points, net worth, survival);
/// 2. remaining survivors by victory points, then net worth, then lowest
///    seat (the house tie-break, ADR-0010/0020);
/// 3. eliminated seats below all survivors, in reverse elimination order
///    (`eliminated` is the room's record of `PlayerBankrupt`/
///    `PlayerResigned` seats, in the order they fell).
///
/// Defensive: a bankrupt seat missing from `eliminated` (which should not
/// happen) is appended last rather than dropped - every seat places.
#[must_use]
pub fn placements(
    state: &GameState,
    content: &GameContent,
    winner: usize,
    eliminated: &[usize],
) -> Vec<usize> {
    let mut order = vec![winner];
    let mut survivors: Vec<usize> = (0..state.players.len())
        .filter(|&s| s != winner && !state.players[s].bankrupt)
        .collect();
    survivors.sort_by_key(|&s| {
        (
            std::cmp::Reverse(state.victory_points(content, s)),
            std::cmp::Reverse(state.net_worth(content, s)),
            s,
        )
    });
    order.extend(survivors);
    order.extend(eliminated.iter().rev().copied().filter(|&s| s != winner));
    // Bankrupt seats the event scan somehow missed still get a placement.
    let missing: Vec<usize> = (0..state.players.len())
        .filter(|s| !order.contains(s))
        .collect();
    order.extend(missing);
    order
}

#[cfg(test)]
mod tests;
