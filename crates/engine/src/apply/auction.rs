//! Sealed-bid auctions on every landing (ADR-0018): bid collection
//! and window resolution (floors, the discoverer's rebate, ties).
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::{CommandError, DISCOVERER_REFUND_PCT, Event, Exec, MarketEffect, TurnPhase};

impl Exec<'_> {
    pub(super) fn submit_blind_bid(&mut self, p: usize, amount: i64) -> Result<(), CommandError> {
        let TurnPhase::BlindAuction { tile, ref bids } = self.st.turn else {
            return Err(CommandError::WrongPhase);
        };
        if bids[p].is_some() {
            return Err(CommandError::AlreadyBid);
        }
        if !(0..=self.st.players[p].cash).contains(&amount) {
            return Err(CommandError::InsufficientFunds);
        }
        let floor = self
            .content
            .property(tile)
            .expect("BlindAuction always targets a property")
            .price;
        if p == self.st.current && amount != 0 && amount < floor {
            return Err(CommandError::BidBelowFloor);
        }
        let TurnPhase::BlindAuction { bids, .. } = &mut self.st.turn else {
            unreachable!()
        };
        bids[p] = Some(amount);
        self.ev.push(Event::BlindBidSubmitted { player: p });
        self.maybe_resolve_blind_auction();
        Ok(())
    }

    /// Resolves the open sealed-bid window once every living seat has bid.
    /// A no-op otherwise. Highest effective bid wins (the discoverer's
    /// silent/zero bid is substituted with the list price if they can
    /// afford it); ties favour the discoverer, then the lowest seat.
    pub(super) fn maybe_resolve_blind_auction(&mut self) {
        let TurnPhase::BlindAuction { tile, ref bids } = self.st.turn else {
            return;
        };
        if !self.st.alive_players().all(|s| bids[s].is_some()) {
            return;
        }
        let discoverer = self.st.current;
        let floor = self
            .content
            .property(tile)
            .expect("BlindAuction always targets a property")
            .price;
        let raw: Vec<i64> = {
            let TurnPhase::BlindAuction { bids, .. } = &self.st.turn else {
                unreachable!()
            };
            (0..self.st.players.len())
                .map(|i| bids[i].unwrap_or(0))
                .collect()
        };
        let effective = |s: usize| -> i64 {
            if s == discoverer && raw[s] == 0 && self.st.players[discoverer].cash >= floor {
                floor
            } else {
                raw[s]
            }
        };
        let winner = self
            .st
            .alive_players()
            .filter(|&s| effective(s) > 0)
            .max_by_key(|&s| (effective(s), s == discoverer, std::cmp::Reverse(s)));
        match winner {
            Some(w) => {
                // Everyone pays their winning bid in full, discoverer included
                // (ADR-0018 amended): the reward is a rebate afterwards, not a
                // quieter price, so the table watches the full amount leave.
                let settlement =
                    self.apply_market_multiplier(MarketEffect::AcquisitionMultiplier, effective(w));
                self.st.players[w].cash -= settlement;
                self.st.tiles[tile].owner = Some(w);
                self.ev.push(Event::BlindAuctionResolved {
                    tile,
                    discoverer,
                    winner: Some(w),
                    amount: settlement,
                    bids: raw,
                });
                if w == discoverer {
                    let refund = settlement * DISCOVERER_REFUND_PCT / 100;
                    if refund > 0 {
                        self.st.players[w].cash += refund;
                        self.ev.push(Event::DiscovererRefunded {
                            player: w,
                            tile,
                            amount: refund,
                        });
                    }
                }
            }
            None => {
                self.ev.push(Event::BlindAuctionResolved {
                    tile,
                    discoverer,
                    winner: None,
                    amount: 0,
                    bids: raw,
                });
            }
        }
        self.st.turn = TurnPhase::AwaitEnd;
    }
}
