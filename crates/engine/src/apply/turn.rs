//! Turn flow and game-over checks: EndTurn/Resign, turn advance with
//! its forecast/spotlight ticks, the round bonus (ADR-0020), and the
//! win conditions (ADR-0013/0019/0020/0021).
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::*;

impl<'e> Exec<'e> {
    pub(super) fn end_turn(&mut self) -> Result<(), CommandError> {
        if self.st.turn != TurnPhase::AwaitEnd {
            return Err(CommandError::WrongPhase);
        }
        self.advance_turn();
        Ok(())
    }

    pub(super) fn resign(&mut self, p: usize) -> Result<(), CommandError> {
        self.ev.push(Event::PlayerResigned { player: p });
        self.bankrupt(p, None);
        // Bankruptcy already excluded `p` from `alive_players()`, so this
        // may complete a sealed-bid window still waiting on `p` - including
        // the discoverer resigning while other seats haven't bid yet.
        if matches!(self.st.phase, GamePhase::Active)
            && matches!(self.st.turn, TurnPhase::BlindAuction { .. })
        {
            self.maybe_resolve_blind_auction();
        }
        Ok(())
    }

    pub(super) fn advance_turn(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        let n = self.st.players.len();
        let mut next = self.st.current;
        for _ in 0..n {
            next = (next + 1) % n;
            if !self.st.players[next].bankrupt {
                break;
            }
        }
        self.st.current = next;
        self.st.turn = TurnPhase::AwaitMove;
        self.st.turn_count += 1;
        self.ev.push(Event::TurnStarted { player: next });
        self.tick_forecast();
        self.tick_spotlight();
    }

    /// Round number (ADR-0020): the minimum hands fully cycled across
    /// surviving players (`maybe_refill_hand` ticks this once per refill,
    /// not once per turn - a hand can span several turns).
    pub(super) fn round_number(&self) -> u32 {
        self.st
            .alive_players()
            .map(|p| self.st.players[p].hands_cycled)
            .min()
            .unwrap_or(0)
    }

    /// Round bonus (ADR-0020): the strictly-highest-cash alive player (ties
    /// to the lowest seat, `alive_players` yields in seat order) banks +2
    /// permanent victory points.
    pub(super) fn award_round_bonus(&mut self) {
        let winner = self
            .st
            .alive_players()
            .map(|p| (p, self.st.players[p].cash))
            .reduce(|best, cur| if cur.1 > best.1 { cur } else { best })
            .map(|(p, _)| p);
        if let Some(p) = winner {
            self.st.players[p].round_bonus_vp += ROUND_BONUS_VP;
            // Announced explicitly (2026-07 playtest feedback): the round
            // bonus was the one VP source with zero visible trace - not
            // even a log line - so nobody understood where those points
            // came from.
            self.ev.push(Event::RoundBonusAwarded {
                player: p,
                points: ROUND_BONUS_VP,
            });
        }
    }

    /// Turn-transition tick for the public forecast: expires the active
    /// effect if its window closed, activates the next scheduled event if
    /// it's due (a `WealthTax` resolves instantly here and never becomes
    /// "active" - nothing to expire), then refills the queue back to 3.
    /// Naturally a no-op when the content ships no market events: the
    /// queue can never hold anything to activate, and `draw_next` itself
    /// no-ops on an empty pool - no need for an explicit early return, and
    /// none here on purpose so an `active` effect (however it got there)
    /// always still expires on schedule.
    pub(super) fn tick_forecast(&mut self) {
        if let Some(active) = &self.st.forecast.active
            && self.st.turn_count >= active.ends_at_turn
        {
            let event_id = active.event_id.clone();
            self.st.forecast.active = None;
            self.ev.push(Event::MarketEventExpired { event_id });
        }
        let due = self
            .st
            .forecast
            .queue
            .first()
            .is_some_and(|next| self.st.turn_count >= next.starts_at_turn);
        if self.st.forecast.active.is_none() && due {
            let scheduled = self.st.forecast.queue.remove(0);
            if let Some(def) = self.content.market_event(&scheduled.event_id) {
                let effect = def.effect;
                let magnitude_pct = def.magnitude_pct;
                self.ev.push(Event::MarketEventActivated {
                    event_id: scheduled.event_id.clone(),
                    effect,
                    magnitude_pct,
                    duration_turns: scheduled.duration,
                });
                if effect == MarketEffect::WealthTax {
                    self.apply_wealth_tax(magnitude_pct, &scheduled.event_id);
                } else {
                    self.st.forecast.active = Some(ActiveMarketEvent {
                        event_id: scheduled.event_id,
                        effect,
                        magnitude_pct,
                        ends_at_turn: self.st.turn_count + scheduled.duration,
                    });
                }
            }
            self.st
                .forecast
                .draw_next(self.content, &mut self.st.rng, self.st.turn_count);
        }
    }

    /// One-shot wealth tax (ADR-0021): every alive player pays `net_worth *
    /// pct / 100` through the normal charge/bankruptcy machinery, mirroring
    /// `CardEffect::CollectFromEach`/`PayEach`.
    pub(super) fn apply_wealth_tax(&mut self, pct: i64, event_id: &str) {
        for p in self.st.alive_players().collect::<Vec<_>>() {
            let amount = (self.st.net_worth(self.content, p) * pct / 100).max(0);
            self.ev.push(Event::CashAdjusted {
                player: p,
                delta: -amount,
                reason: event_id.to_string(),
            });
            self.charge(p, None, amount);
            if matches!(self.st.phase, GamePhase::Finished { .. }) {
                return;
            }
        }
    }

    pub(super) fn check_win(&mut self) {
        let winner = {
            let mut alive = self.st.alive_players();
            match (alive.next(), alive.next()) {
                (Some(winner), None) => Some(winner),
                _ => None,
            }
        };
        if let Some(winner) = winner {
            self.st.phase = GamePhase::Finished { winner };
            self.ev.push(Event::GameEnded { winner });
        }
    }

    /// Instant win by controlling `rules.win_full_groups` complete colour
    /// groups (ADR-0013). Lowest seat wins if two qualify at once (a trade).
    pub(super) fn check_group_win(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        let need = self.content.rules.win_full_groups;
        if need <= 0 {
            return;
        }
        for p in self.st.alive_players().collect::<Vec<_>>() {
            let owned = self.st.full_groups_owned(self.content, p);
            if owned as i64 >= need {
                self.st.phase = GamePhase::Finished { winner: p };
                self.ev.push(Event::WonByGroups {
                    winner: p,
                    groups: owned.min(u8::MAX as usize) as u8,
                });
                return;
            }
        }
    }

    /// Instant win by reaching `rules.win_victory_points` (ADR-0020).
    pub(super) fn check_points_win(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        let target = self.content.rules.win_victory_points;
        if target <= 0 {
            return;
        }
        for p in self.st.alive_players().collect::<Vec<_>>() {
            let points = self.st.victory_points(self.content, p);
            if points >= target {
                self.st.phase = GamePhase::Finished { winner: p };
                self.ev.push(Event::WonByPoints { player: p, points });
                return;
            }
        }
    }

    /// Doom clock (ADR-0020): once the shared conglomerate pool runs dry
    /// with nobody having crossed the point target (checked first, by
    /// `check_points_win`), the game ends immediately - highest score
    /// wins, ties broken by net worth then the lowest seat. Only relevant
    /// to the points ruleset: a no-op when `win_victory_points` is off.
    pub(super) fn check_pool_exhaustion_win(&mut self) {
        if !matches!(self.st.phase, GamePhase::Active) {
            return;
        }
        if self.content.rules.win_victory_points <= 0 {
            return;
        }
        if self.st.conglomerates_available != Some(0) {
            return;
        }
        let winner = self
            .st
            .alive_players()
            .map(|p| {
                (
                    p,
                    self.st.victory_points(self.content, p),
                    self.st.net_worth(self.content, p),
                )
            })
            .reduce(|best, cur| {
                if (cur.1, cur.2) > (best.1, best.2) {
                    cur
                } else {
                    best
                }
            })
            .map(|(p, ..)| p);
        if let Some(winner) = winner {
            self.st.phase = GamePhase::Finished { winner };
            self.ev.push(Event::WonByPoolExhaustion { winner });
        }
    }
}
