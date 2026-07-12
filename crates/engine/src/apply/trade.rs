//! Asynchronous trade offers (ADR-0007): lifecycle, validation, and
//! the auction/vote cash-freeze guard shared with the estate commands.
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::*;

impl<'e> Exec<'e> {
    pub(super) fn propose_trade(
        &mut self,
        p: usize,
        to_id: &str,
        give_cash: i64,
        give_tiles: &[String],
        receive_cash: i64,
        receive_tiles: &[String],
    ) -> Result<(), CommandError> {
        self.reject_during_auction()?;
        let to = self
            .st
            .players
            .iter()
            .position(|pl| pl.id == to_id)
            .ok_or(CommandError::UnknownPlayer)?;
        if to == p || self.st.players[to].bankrupt {
            return Err(CommandError::TradeInvalid);
        }
        let open_from_p = self
            .st
            .pending_trades
            .iter()
            .filter(|t| t.from == p)
            .count();
        if open_from_p >= MAX_OPEN_TRADES_PER_PLAYER {
            return Err(CommandError::TradeLimit);
        }
        if give_cash < 0 || receive_cash < 0 {
            return Err(CommandError::TradeInvalid);
        }
        let empty = give_cash == 0
            && receive_cash == 0
            && give_tiles.is_empty()
            && receive_tiles.is_empty();
        if empty {
            return Err(CommandError::TradeInvalid);
        }

        let offer = TradeOffer {
            id: self.st.trade_seq,
            from: p,
            to,
            give_cash,
            give_tiles: self.resolve_trade_tiles(give_tiles)?,
            receive_cash,
            receive_tiles: self.resolve_trade_tiles(receive_tiles)?,
        };
        self.validate_trade_assets(&offer)?;

        self.st.trade_seq += 1;
        self.ev.push(Event::TradeProposed {
            trade: offer.id,
            from: p,
            to,
        });
        self.st.pending_trades.push(offer);
        Ok(())
    }

    pub(super) fn accept_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        self.reject_during_auction()?;
        let idx = self.trade_index(id)?;
        let offer = self.st.pending_trades[idx].clone();
        if offer.to != p {
            return Err(CommandError::NotTradeParty);
        }
        // Ownership or cash may have shifted since the proposal. A stale
        // offer rejects here without mutating (ADR-0001); the recipient can
        // decline it to clear it out.
        self.validate_trade_assets(&offer)?;

        self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeAccepted {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        self.st.players[offer.from].cash += offer.receive_cash - offer.give_cash;
        self.st.players[offer.to].cash += offer.give_cash - offer.receive_cash;
        for &tile in &offer.give_tiles {
            self.st.tiles[tile].owner = Some(offer.to);
            self.st.tiles[tile].boosts = 0;
            self.ev.push(Event::PropertyTransferred {
                tile,
                from: offer.from,
                to: Some(offer.to),
            });
        }
        for &tile in &offer.receive_tiles {
            self.st.tiles[tile].owner = Some(offer.from);
            self.st.tiles[tile].boosts = 0;
            self.ev.push(Event::PropertyTransferred {
                tile,
                from: offer.to,
                to: Some(offer.from),
            });
        }
        Ok(())
    }

    pub(super) fn decline_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        let idx = self.trade_index(id)?;
        if self.st.pending_trades[idx].to != p {
            return Err(CommandError::NotTradeParty);
        }
        let offer = self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeDeclined {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        Ok(())
    }

    pub(super) fn cancel_trade(&mut self, p: usize, id: u32) -> Result<(), CommandError> {
        let idx = self.trade_index(id)?;
        if self.st.pending_trades[idx].from != p {
            return Err(CommandError::NotTradeParty);
        }
        let offer = self.st.pending_trades.remove(idx);
        self.ev.push(Event::TradeCancelled {
            trade: id,
            from: offer.from,
            to: offer.to,
        });
        Ok(())
    }

    pub(super) fn trade_index(&self, id: u32) -> Result<usize, CommandError> {
        self.st
            .pending_trades
            .iter()
            .position(|t| t.id == id)
            .ok_or(CommandError::TradeNotFound)
    }

    pub(super) fn reject_during_auction(&self) -> Result<(), CommandError> {
        match self.st.turn {
            TurnPhase::BlindAuction { .. } | TurnPhase::BribeVote { .. } => {
                Err(CommandError::WrongPhase)
            }
            _ => Ok(()),
        }
    }

    pub(super) fn resolve_trade_tiles(&self, ids: &[String]) -> Result<Vec<usize>, CommandError> {
        let mut tiles = Vec::with_capacity(ids.len());
        for id in ids {
            let tile = self
                .content
                .tile_index(id)
                .ok_or_else(|| CommandError::UnknownTile { tile: id.clone() })?;
            if tiles.contains(&tile) {
                return Err(CommandError::TradeInvalid);
            }
            tiles.push(tile);
        }
        Ok(tiles)
    }

    /// Full asset check, run both at proposal and at acceptance time.
    pub(super) fn validate_trade_assets(&self, offer: &TradeOffer) -> Result<(), CommandError> {
        for (&owner, tiles) in [
            (&offer.from, &offer.give_tiles),
            (&offer.to, &offer.receive_tiles),
        ] {
            for &tile in tiles {
                let prop = self
                    .content
                    .property(tile)
                    .ok_or(CommandError::NotAProperty)?;
                if self.st.tiles[tile].owner != Some(owner) {
                    return Err(CommandError::NotOwner);
                }
                if self
                    .content
                    .group_tiles(&prop.group)
                    .iter()
                    .any(|&t| self.st.tiles[t].houses > 0)
                {
                    return Err(CommandError::HousesInGroup);
                }
            }
        }
        if self.st.players[offer.from].cash < offer.give_cash
            || self.st.players[offer.to].cash < offer.receive_cash
        {
            return Err(CommandError::InsufficientFunds);
        }
        Ok(())
    }
}
