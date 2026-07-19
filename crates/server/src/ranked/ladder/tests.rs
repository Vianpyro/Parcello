//! Unit tests for the pure ladder math (ADR-0034): display scaling, update
//! direction/magnitude, and placement derivation on real engine states.

use super::*;
use parcello_engine::Engine;
use std::path::Path;
use std::sync::Arc;

fn base_content() -> Arc<GameContent> {
    let resolved = parcello_mods::resolve(
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../mods")),
        &["base".to_string()],
    )
    .expect("base mod loads");
    Arc::new(resolved.content)
}

fn four_player_state(content: &Arc<GameContent>) -> GameState {
    let engine = Engine::new(Arc::clone(content)).expect("valid content");
    let players = (0..4).map(|i| (format!("p{i}"), format!("P{i}"))).collect();
    engine.new_game(players, 42)
}

#[test]
fn new_player_displays_1000_and_certainty_raises_it() {
    let fresh = Rating::default();
    assert_eq!(display(fresh), 1000, "mu - 3*sigma is 0 for the defaults");

    let confident = Rating {
        mu: fresh.mu,
        sigma: 1.0,
    };
    assert!(
        display(confident) > display(fresh),
        "same mu with lower sigma must display higher"
    );

    let hopeless = Rating {
        mu: 0.0,
        sigma: 9.0,
    };
    assert_eq!(display(hopeless), 0, "display floors at 0, never negative");
}

#[test]
fn winner_gains_loser_drops_and_sigma_shrinks() {
    let before = vec![Rating::default(); 2];
    let after = rate(&before);
    assert!(after[0].mu > before[0].mu, "placement 1 gains mu");
    assert!(after[1].mu < before[1].mu, "last place loses mu");
    for (b, a) in before.iter().zip(&after) {
        assert!(a.sigma < b.sigma, "every game reduces uncertainty");
    }
}

#[test]
fn four_player_updates_are_monotonic_in_placement() {
    let before = vec![Rating::default(); 4];
    let after = rate(&before);
    for pair in after.windows(2) {
        assert!(
            pair[0].mu > pair[1].mu,
            "equal priors: a better placement must land a higher mu"
        );
    }
}

#[test]
fn repeated_wins_climb_the_display_ladder() {
    let mut grinder = Rating::default();
    let mut victim = Rating::default();
    let start = display(grinder);
    for _ in 0..20 {
        let after = rate(&[grinder, victim]);
        grinder = after[0];
        victim = after[1];
    }
    assert!(
        display(grinder) > start + 200,
        "20 straight wins must move the shown number substantially \
         (got {} from {start})",
        display(grinder)
    );
}

#[test]
fn upset_wins_move_more_than_expected_wins() {
    let strong = Rating {
        mu: 30.0,
        sigma: 3.0,
    };
    let weak = Rating {
        mu: 20.0,
        sigma: 3.0,
    };
    let expected = rate(&[strong, weak]); // favourite wins
    let upset = rate(&[weak, strong]); // underdog wins
    assert!(
        (upset[0].mu - weak.mu) > (expected[0].mu - strong.mu),
        "an upset must pay the winner more than a formality"
    );
}

#[test]
fn placements_winner_first_survivors_by_score_then_reverse_elimination() {
    let content = base_content();
    let mut state = four_player_state(&content);
    // Seat 2 wins; seats 0 and 1 survive with 0 and 3 both eliminated...
    state.players[3].bankrupt = true;
    // ...and seat 1 is richer than seat 0 (no properties, so net worth =
    // cash and victory points are equal).
    state.players[1].cash = state.players[0].cash + 500;

    let order = placements(&state, &content, 2, &[3]);
    assert_eq!(order, vec![2, 1, 0, 3]);
}

#[test]
fn placements_rank_late_eliminations_above_early_ones() {
    let content = base_content();
    let mut state = four_player_state(&content);
    state.players[1].bankrupt = true;
    state.players[2].bankrupt = true;
    state.players[3].bankrupt = true;

    // 3 fell first, then 1, then 2: last standing wins, latest fall places
    // highest among the fallen.
    let order = placements(&state, &content, 0, &[3, 1, 2]);
    assert_eq!(order, vec![0, 2, 1, 3]);
}

#[test]
fn placements_cover_every_seat_even_without_elimination_events() {
    let content = base_content();
    let mut state = four_player_state(&content);
    state.players[2].bankrupt = true;

    // Defensive branch: a bankrupt seat missing from `eliminated` still
    // places (last), and nobody appears twice.
    let mut order = placements(&state, &content, 0, &[]);
    assert_eq!(order.len(), 4);
    assert_eq!(order[3], 2, "untracked bankrupt seat places last");
    order.sort_unstable();
    assert_eq!(order, vec![0, 1, 2, 3]);
}
