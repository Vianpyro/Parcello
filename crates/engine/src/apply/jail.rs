//! Jail entry and the three exits (ADR-0024): Legal Route, Corruption
//! bribe + vote window, and the jail card.
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::*;

impl<'e> Exec<'e> {
    pub(super) fn choose_legal_route(
        &mut self,
        p: usize,
        order: Vec<u8>,
    ) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitMove {
            return Err(CommandError::WrongPhase);
        }
        if !self.st.players[p].jailed {
            return Err(CommandError::NotInJail);
        }
        let mut expected: Vec<u8> =
            (self.content.rules.velocity_min..=self.content.rules.velocity_max).collect();
        let mut got = order.clone();
        expected.sort_unstable();
        got.sort_unstable();
        if expected != got {
            return Err(CommandError::InvalidRoute);
        }
        self.st.players[p].hand.clear();
        self.st.players[p].jailed = false;
        self.ev.push(Event::LeftJail { player: p });
        self.ev.push(Event::LegalRouteChosen {
            player: p,
            order: order.clone(),
        });
        self.st.players[p].jail_route = Some(order);
        self.play_route_front(p);
        Ok(())
    }

    pub(super) fn offer_bribe(&mut self, p: usize, amount: i64) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitMove {
            return Err(CommandError::WrongPhase);
        }
        if !self.st.players[p].jailed {
            return Err(CommandError::NotInJail);
        }
        if !(1..=self.st.players[p].cash).contains(&amount) {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.turn = TurnPhase::BribeVote {
            briber: p,
            amount,
            votes: vec![None; self.st.players.len()],
        };
        self.ev.push(Event::BribeOffered { player: p, amount });
        Ok(())
    }

    pub(super) fn vote_on_bribe(&mut self, p: usize, accept: bool) -> Result<(), CommandError> {
        let TurnPhase::BribeVote {
            briber, ref votes, ..
        } = self.st.turn
        else {
            return Err(CommandError::WrongPhase);
        };
        if p == briber {
            return Err(CommandError::NotYourTurn);
        }
        if votes[p].is_some() {
            return Err(CommandError::AlreadyVoted);
        }
        let TurnPhase::BribeVote { votes, .. } = &mut self.st.turn else {
            unreachable!()
        };
        votes[p] = Some(accept);
        self.ev.push(Event::BribeVoteCast { player: p });
        self.maybe_resolve_bribe_vote();
        Ok(())
    }

    /// Resolves the open bribe vote once every living non-briber has voted.
    /// A no-op otherwise. Strictly more than half must accept (a 2-player
    /// game needs the lone opponent's yes).
    pub(super) fn maybe_resolve_bribe_vote(&mut self) {
        let TurnPhase::BribeVote {
            briber,
            amount,
            ref votes,
        } = self.st.turn
        else {
            return;
        };
        let opponents: Vec<usize> = self.st.alive_players().filter(|&s| s != briber).collect();
        if !opponents.iter().all(|&s| votes[s].is_some()) {
            return;
        }
        let accepts = opponents
            .iter()
            .filter(|&&s| votes[s] == Some(true))
            .count();
        let succeeded = accepts * 2 > opponents.len();
        if succeeded {
            let n = opponents.len() as i64;
            let share = if n > 0 { amount / n } else { 0 };
            self.st.players[briber].cash -= share * n;
            for &o in &opponents {
                self.st.players[o].cash += share;
            }
            self.st.players[briber].jailed = false;
            self.st.turn = TurnPhase::AwaitMove;
        } else {
            self.st.turn = TurnPhase::AwaitEnd;
        }
        self.ev.push(Event::BribeResolved {
            briber,
            amount,
            succeeded,
            accepts,
            total: opponents.len(),
        });
    }

    pub(super) fn use_jail_card(&mut self, p: usize) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitMove {
            return Err(CommandError::WrongPhase);
        }
        if !self.st.players[p].jailed {
            return Err(CommandError::NotInJail);
        }
        if self.st.players[p].jail_cards == 0 {
            return Err(CommandError::NoJailCard);
        }
        self.st.players[p].jail_cards -= 1;
        self.st.players[p].jailed = false;
        self.ev.push(Event::JailCardUsed { player: p });
        self.ev.push(Event::LeftJail { player: p });
        Ok(())
    }

    pub(super) fn go_to_jail(&mut self, p: usize) {
        let from = self.st.players[p].position;
        self.st.players[p].position = self.content.jail_position();
        self.st.players[p].jailed = true;
        // A route landing its holder back on Go To Jail mid-course (ADR-0024
        // doesn't special-case this) has its parole revoked: the freeze
        // must not outlive the route, and a normal hand must be waiting for
        // whichever jail exit comes next.
        if self.st.players[p].jail_route.take().is_some() {
            self.maybe_refill_hand(p);
        }
        self.ev.push(Event::WentToJail { player: p, from });
    }
}
