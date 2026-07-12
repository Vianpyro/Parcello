//! Velocity-deck movement (ADR-0017): playing a card, hand refills
//! (the ADR-0020 round metronome), board motion and Go salary.
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::*;

impl<'e> Exec<'e> {
    pub(super) fn play_movement_card(&mut self, p: usize, value: u8) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitMove {
            return Err(CommandError::WrongPhase);
        }
        if self.st.players[p].jailed {
            // Jailed players choose an exit (ChooseLegalRoute / OfferBribe
            // / UseJailCard) instead of playing a card directly.
            return Err(CommandError::CardNotPlayable);
        }
        if self.st.players[p].jail_route.is_some() {
            let route = self.st.players[p]
                .jail_route
                .as_ref()
                .expect("checked Some");
            if route.first() != Some(&value) {
                return Err(CommandError::CardNotPlayable);
            }
            self.play_route_front(p);
            return Ok(());
        }
        let Some(idx) = self.st.players[p].hand.iter().position(|&v| v == value) else {
            return Err(CommandError::CardNotPlayable);
        };
        self.st.players[p].hand.remove(idx);
        self.ev.push(Event::MovementCardPlayed { player: p, value });
        self.maybe_refill_hand(p);
        self.move_forward(p, value as usize);
        self.resolve_landing(p, 0);
        Ok(())
    }

    /// Plays the front card of an active Legal Route (ADR-0024): shared by
    /// `play_movement_card`'s route branch and `choose_legal_route`'s
    /// same-command first move. Identical tail to normal play, just a
    /// different card source.
    pub(super) fn play_route_front(&mut self, p: usize) {
        let route = self.st.players[p]
            .jail_route
            .as_mut()
            .expect("play_route_front requires an active route");
        let value = route.remove(0);
        let route_done = route.is_empty();
        if route_done {
            self.st.players[p].jail_route = None;
        }
        self.ev.push(Event::MovementCardPlayed { player: p, value });
        // The hand stays empty (cleared by `choose_legal_route`) for the
        // whole route - refilling here only when it finishes is what makes
        // the refill (and its `hands_cycled` tick) happen exactly once per
        // route, not once per route step.
        if route_done {
            self.maybe_refill_hand(p);
        }
        self.move_forward(p, value as usize);
        self.resolve_landing(p, 0);
    }

    /// Refills `p`'s hand the instant it empties (ADR-0017) and ticks
    /// `hands_cycled` - the ADR-0020 round metronome, checked here (not
    /// `advance_turn`) since a hand can span several turns.
    pub(super) fn maybe_refill_hand(&mut self, p: usize) {
        if !self.st.players[p].hand.is_empty() {
            return;
        }
        let round_before = self.round_number();
        self.st.players[p].hand =
            (self.content.rules.velocity_min..=self.content.rules.velocity_max).collect();
        self.st.players[p].hands_cycled += 1;
        if self.content.rules.win_victory_points > 0 && self.round_number() > round_before {
            self.award_round_bonus();
        }
    }

    pub(super) fn move_forward(&mut self, p: usize, steps: usize) {
        let len = self.content.board.len();
        let from = self.st.players[p].position;
        let raw = from + steps;
        let passed_go = raw >= len;
        let to = raw % len;
        self.st.players[p].position = to;
        self.ev.push(Event::Moved {
            player: p,
            from,
            to,
            passed_go,
        });
        if passed_go {
            self.pay_salary(p);
        }
    }

    /// Direct placement (cards). Salary is granted only for forward wraps
    /// when the card says so; backward moves never pay.
    pub(super) fn teleport(&mut self, p: usize, to: usize, collect_go: bool) {
        let from = self.st.players[p].position;
        let passed_go = collect_go && to <= from && to != from;
        let passed_go = passed_go || (collect_go && to == 0 && from != 0);
        self.st.players[p].position = to;
        self.ev.push(Event::Moved {
            player: p,
            from,
            to,
            passed_go,
        });
        if passed_go {
            self.pay_salary(p);
        }
    }

    pub(super) fn pay_salary(&mut self, p: usize) {
        let amount = self.content.rules.go_salary;
        self.st.players[p].cash += amount;
        self.ev.push(Event::SalaryPaid { player: p, amount });
    }
}
