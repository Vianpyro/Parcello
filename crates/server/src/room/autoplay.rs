//! The room playing for absent humans: the canonical AFK action
//! (ADR-0017/0024), silent-seat auction bids and bribe votes (auto-abstain
//! and auto-reject), and server-driven bot seats (ADR-0014).

use parcello_engine::{ClientView, CommandKind, PlayerId, TurnPhase};
use tracing::info;

use super::{Phase, Room};

impl Room {
    /// The action the game is waiting for, per `TurnPhase`. Never invalid
    /// for the returned player, so applying it always advances a stalled
    /// game. A plain move is chosen by the bot heuristic (2026-07) so a
    /// timed-out seat moves smartly, not just with its lowest card; a
    /// jailed seat's action is the Legal Route in ascending order
    /// (ADR-0024); end of turn is `EndTurn`. Movement/route/end only - an
    /// AFK auto-play never spends the player's cash.
    pub(super) fn afk_command(&self) -> Option<(PlayerId, CommandKind)> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        let seat = self.acting_seat()?;
        let player = &st.players[seat];
        let kind = match st.turn {
            TurnPhase::AwaitMove => {
                if let Some(route) = &player.jail_route {
                    CommandKind::PlayMovementCard { value: route[0] }
                } else if player.jailed {
                    let rules = &self.engine.content().rules;
                    let order: Vec<u8> = (rules.velocity_min..=rules.velocity_max).collect();
                    CommandKind::ChooseLegalRoute { order }
                } else {
                    // Auto-play the *movement* with bot smarts (2026-07):
                    // a timed-out seat gets its best-scoring card, not the
                    // dumb lowest one - it should feel like the bot stepped
                    // in, not like a forfeit. Movement only: no auto-spend
                    // of their cash. Falls back to the lowest card if the
                    // heuristic declines (it never should in AwaitMove).
                    let view = ClientView::for_seat(st, self.engine.content(), seat);
                    let value =
                        parcello_engine::bot::movement_card(self.engine.content(), &view, seat)
                            .unwrap_or_else(|| {
                                *player
                                    .hand
                                    .iter()
                                    .min()
                                    .expect("hand never empty in AwaitMove")
                            });
                    CommandKind::PlayMovementCard { value }
                }
            }
            TurnPhase::AwaitEnd => CommandKind::EndTurn,
            // acting_seat() already excludes these phases (no single
            // actor); kept for exhaustiveness.
            TurnPhase::BlindAuction { .. } | TurnPhase::BribeVote { .. } => return None,
        };
        Some((st.players[seat].id.clone(), kind))
    }

    /// Auto-abstains every seat that hasn't bid by the sealed-bid window's
    /// deadline (ADR-0018) - the multi-seat equivalent of `afk_command`'s
    /// single-actor auto-play. Submitting through the normal `handle_game`
    /// path means the last injection's own resolution naturally clears
    /// `bid_deadline` via the transition detection there; cleared again
    /// here regardless, defensively, so the timer can never spin.
    pub(super) fn inject_silent_bids(&mut self) {
        let Phase::Active(state) = &self.phase else {
            self.bid_deadline = None;
            return;
        };
        let TurnPhase::BlindAuction { bids, .. } = &state.turn else {
            self.bid_deadline = None;
            return;
        };
        let silent: Vec<PlayerId> = state
            .alive_players()
            .filter(|&p| bids[p].is_none())
            .map(|p| state.players[p].id.clone())
            .collect();
        for player_id in silent {
            info!(room = %self.code, player = %player_id,
                  "sealed-bid window closed, abstaining");
            self.handle_game(&player_id, CommandKind::SubmitBlindBid { amount: 0 });
        }
        self.bid_deadline = None;
    }

    /// Auto-rejects every living opponent who hasn't voted by the Corruption
    /// bribe vote's deadline (ADR-0024) - `inject_silent_bids`'s structural
    /// twin for the other simultaneous multi-seat phase.
    pub(super) fn inject_silent_votes(&mut self) {
        let Phase::Active(state) = &self.phase else {
            self.vote_deadline = None;
            return;
        };
        let TurnPhase::BribeVote { briber, votes, .. } = &state.turn else {
            self.vote_deadline = None;
            return;
        };
        let briber = *briber;
        let silent: Vec<PlayerId> = state
            .alive_players()
            .filter(|&p| p != briber && votes[p].is_none())
            .map(|p| state.players[p].id.clone())
            .collect();
        for player_id in silent {
            info!(room = %self.code, player = %player_id,
                  "bribe vote window closed, rejecting");
            self.handle_game(&player_id, CommandKind::VoteOnBribe { accept: false });
        }
        self.vote_deadline = None;
    }

    /// The first bot seat with something to do right now and the command it
    /// wants, using the shared engine heuristic over that seat's own view
    /// (ADR-0014). `None` when no bot is waiting - covers turns, auctions,
    /// and declining trades offered to a bot.
    pub(super) fn next_bot_action(&self) -> Option<(PlayerId, CommandKind)> {
        let Phase::Active(st) = &self.phase else {
            return None;
        };
        for (i, seat) in self.seats.iter().enumerate() {
            if !seat.is_bot {
                continue;
            }
            let view = ClientView::for_seat(st, self.engine.content(), i);
            // The engine's content carries the effective rules after
            // start_game rebuilds it (ADR-0015), so the bot plays by the
            // room's actual settings.
            // Fresh noise per decision (bid jitter): randomness lives in
            // the session layer; the engine heuristic stays pure given it.
            if let Some(kind) =
                parcello_engine::bot::decide(self.engine.content(), &view, i, rand::random())
            {
                return Some((st.players[i].id.clone(), kind));
            }
        }
        None
    }
}
