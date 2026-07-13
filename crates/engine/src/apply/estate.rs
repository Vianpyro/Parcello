//! Owner actions on tiles: building under the even rule and shared
//! pools (ADR-0019), mortgages, rent boosts (ADR-0012), and takeovers
//! including the mortgaged-tile buyout (ADR-0011/0022).
//!
//! Split from `apply.rs` (2026-07) purely for module size; all methods
//! stay on `Exec` and are `pub(super)` - the command pipeline in
//! `apply.rs` is still the only entry point.

use super::{
    CommandError, Event, Exec, GameContent, HOUSE_REFUND_PCT, MAX_RENT_BOOSTS,
    MORTGAGE_INTEREST_PCT, MORTGAGE_VALUE_PCT, MarketEffect, PropertyDef, RentModel, TurnPhase,
};

// The named lifetime is load-bearing here (unlike the sibling modules):
// `owned_property` hands back a `&'e PropertyDef` borrowed from `content`,
// NOT from `&self`, so callers can keep the def across later `self`
// mutations. Eliding it would tie the return to the `&self` borrow.
impl<'e> Exec<'e> {
    pub(super) fn build(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitMove | TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let tile = self
            .content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        let prop = self
            .content
            .property(tile)
            .ok_or(CommandError::NotAProperty)?;
        if self.st.tiles[tile].owner != Some(p) {
            return Err(CommandError::NotOwner);
        }
        if prop.rent_model != RentModel::Houses {
            return Err(CommandError::NotBuildable);
        }
        if !self.st.owns_full_group(self.content, p, &prop.group) {
            return Err(CommandError::GroupIncomplete);
        }
        if self
            .content
            .group_tiles(&prop.group)
            .any(|t| self.st.tiles[t].mortgaged)
        {
            return Err(CommandError::MortgagedInGroup);
        }
        let cap = self.content.rules.max_houses_per_property.min(5);
        if self.st.tiles[tile].houses >= cap {
            return Err(CommandError::BuildLimit);
        }
        let group_min = self
            .content
            .group_tiles(&prop.group)
            .map(|t| self.st.tiles[t].houses)
            .min()
            .unwrap_or(0);
        if self.st.tiles[tile].houses > group_min {
            return Err(CommandError::UnevenBuild);
        }
        if self.st.players[p].cash < prop.house_cost {
            return Err(CommandError::InsufficientFunds);
        }
        // Shared building pools (ADR-0019): the top level draws a
        // conglomerate and, in the same motion, releases the max-1
        // subsidiaries the tile held (the classic house-to-hotel
        // conversion); any other level draws a plain subsidiary.
        let becomes_top = self.st.tiles[tile].houses + 1 == cap;
        if becomes_top {
            self.st
                .take_conglomerate()
                .map_err(|()| CommandError::PoolExhausted)?;
        } else {
            self.st
                .take_subsidiary()
                .map_err(|()| CommandError::PoolExhausted)?;
        }
        self.st.players[p].cash -= prop.house_cost;
        self.st.tiles[tile].houses += 1;
        if becomes_top {
            self.st.return_subsidiaries(u64::from(cap - 1));
        }
        self.ev.push(Event::HouseBuilt {
            player: p,
            tile,
            houses: self.st.tiles[tile].houses,
            cost: prop.house_cost,
        });
        Ok(())
    }

    pub(super) fn sell_house(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if prop.rent_model != RentModel::Houses {
            return Err(CommandError::NotBuildable);
        }
        if self.st.tiles[tile].houses == 0 {
            return Err(CommandError::NoHouses);
        }
        let group_max = self
            .content
            .group_tiles(&prop.group)
            .map(|t| self.st.tiles[t].houses)
            .max()
            .unwrap_or(0);
        if self.st.tiles[tile].houses < group_max {
            return Err(CommandError::UnevenBuild);
        }
        // Shared building pools (ADR-0019): stepping down off the top level
        // returns the conglomerate but must re-issue max-1 subsidiaries -
        // rejected if the bank can't lend that many right now (mortgaging
        // remains the liquidity valve). Any other level just returns one
        // subsidiary, which can never fail.
        let cap = self.content.rules.max_houses_per_property.min(5);
        let steps_off_top = self.st.tiles[tile].houses == cap;
        if steps_off_top {
            let subsidiaries_needed = u64::from(cap - 1);
            if !self.st.subsidiaries_free(subsidiaries_needed) {
                return Err(CommandError::PoolExhausted);
            }
        }
        let refund = prop.house_cost * HOUSE_REFUND_PCT / 100;
        self.st.tiles[tile].houses -= 1;
        self.st.players[p].cash += refund;
        if steps_off_top {
            self.st.return_conglomerate();
            self.st.consume_subsidiaries(u64::from(cap - 1));
        } else {
            self.st.return_subsidiaries(1);
        }
        self.ev.push(Event::HouseSold {
            player: p,
            tile,
            houses: self.st.tiles[tile].houses,
            refund,
        });
        Ok(())
    }

    /// Seize a rival's unmortgaged property for a premium (ADR-0011). The
    /// former owner is compensated (min of price and what was paid); the
    /// bank keeps any premium above that. Takeover happens on the landing
    /// tile only (ADR-0022): after rent has resolved, at the end of the
    /// acting player's own turn, on the exact tile they are standing on.
    /// Improved tiles are seizable too (ADR-0022): their buildings
    /// liquidate at `sell_house` pricing, paid to the former owner on top
    /// of the usual compensation, and the stripped units return to the
    /// shared pools; the taker always receives a bare tile.
    pub(super) fn expropriate(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let pct = self.content.rules.expropriation;
        if pct <= 0 {
            return Err(CommandError::ExpropriationDisabled);
        }
        let tile = self
            .content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        if self.st.players[p].position != tile {
            return Err(CommandError::NotOnTile);
        }
        let prop = self
            .content
            .property(tile)
            .ok_or(CommandError::NotAProperty)?;
        let ts = self.st.tiles[tile];
        // Must be a rival's property; improved tiles are legal targets
        // (ADR-0022).
        let from = match ts.owner {
            Some(o) if o != p => o,
            _ => return Err(CommandError::NotExpropriable),
        };
        // A mortgaged tile is bought out at its flat mortgage value
        // (price/2), paid to the owner, and transfers still mortgaged -
        // the buyer redeems at +10% like any other transferee (ADR-0022,
        // amended 2026-07: the mortgage used to be the takeover shield;
        // it is now the cheap-buyout weak point instead). No expropriation
        // percent, no market multiplier: the price is the mortgage price.
        if ts.mortgaged {
            let cost = prop.price * MORTGAGE_VALUE_PCT / 100;
            if self.st.players[p].cash < cost {
                return Err(CommandError::InsufficientFunds);
            }
            self.st.players[p].cash -= cost;
            self.st.players[from].cash += cost;
            self.st.tiles[tile].owner = Some(p);
            self.st.tiles[tile].boosts = 0;
            self.ev.push(Event::Expropriated {
                player: p,
                from,
                tile,
                cost,
                liquidated: 0,
                liquidation_refund: 0,
            });
            return Ok(());
        }
        let cost = self
            .apply_market_multiplier(MarketEffect::AcquisitionMultiplier, prop.price * pct / 100);
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        let compensation = prop.price.min(cost);
        let cap = self.content.rules.max_houses_per_property.min(5);
        let liquidated = ts.houses;
        let liquidation_refund = (prop.house_cost * HOUSE_REFUND_PCT / 100) * i64::from(liquidated);
        self.st.players[p].cash -= cost;
        self.st.players[from].cash += compensation + liquidation_refund;
        self.st.tiles[tile].owner = Some(p);
        self.st.tiles[tile].houses = 0;
        self.st.tiles[tile].boosts = 0;
        self.st.release_tile_pools(liquidated, cap);
        self.ev.push(Event::Expropriated {
            player: p,
            from,
            tile,
            cost,
            liquidated,
            liquidation_refund,
        });
        Ok(())
    }

    /// Raise an owned tile's rent one step for a fee (ADR-0012), up to
    /// `MAX_RENT_BOOSTS`. Mortgaged tiles cannot be boosted.
    pub(super) fn boost_rent(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        let pct = self.content.rules.rent_boost;
        if pct <= 0 {
            return Err(CommandError::RentBoostDisabled);
        }
        if self.st.tiles[tile].mortgaged {
            return Err(CommandError::AlreadyMortgaged);
        }
        if self.st.tiles[tile].boosts >= MAX_RENT_BOOSTS {
            return Err(CommandError::BoostLimit);
        }
        let cost = prop.price * pct / 100;
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= cost;
        self.st.tiles[tile].boosts += 1;
        self.ev.push(Event::RentBoosted {
            player: p,
            tile,
            boosts: self.st.tiles[tile].boosts,
            cost,
        });
        Ok(())
    }

    pub(super) fn mortgage(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if self.st.tiles[tile].mortgaged {
            return Err(CommandError::AlreadyMortgaged);
        }
        // Classic rule: the whole group must be building-free first.
        if self
            .content
            .group_tiles(&prop.group)
            .any(|t| self.st.tiles[t].houses > 0)
        {
            return Err(CommandError::HousesInGroup);
        }
        let value = prop.price * MORTGAGE_VALUE_PCT / 100;
        self.st.tiles[tile].mortgaged = true;
        self.st.players[p].cash += value;
        self.ev.push(Event::PropertyMortgaged {
            player: p,
            tile,
            value,
        });
        Ok(())
    }

    pub(super) fn unmortgage(&mut self, p: usize, tile_id: &str) -> Result<(), CommandError> {
        let (tile, prop) = self.owned_property(p, tile_id)?;
        if !self.st.tiles[tile].mortgaged {
            return Err(CommandError::NotMortgaged);
        }
        let principal = prop.price * MORTGAGE_VALUE_PCT / 100;
        let cost = principal + principal * MORTGAGE_INTEREST_PCT / 100; // floored
        // Voluntary payment never forces liquidation: reject if unaffordable.
        if self.st.players[p].cash < cost {
            return Err(CommandError::InsufficientFunds);
        }
        self.st.players[p].cash -= cost;
        self.st.tiles[tile].mortgaged = false;
        self.ev.push(Event::PropertyUnmortgaged {
            player: p,
            tile,
            cost,
        });
        Ok(())
    }

    /// Shared validation for tile-targeted asset commands (build phases).
    /// Returns the def borrowed from `content` (not `self`) so callers can
    /// keep it across later state mutations.
    pub(super) fn owned_property(
        &self,
        p: usize,
        tile_id: &str,
    ) -> Result<(usize, &'e PropertyDef), CommandError> {
        if !matches!(self.st.turn, TurnPhase::AwaitMove | TurnPhase::AwaitEnd) {
            return Err(CommandError::WrongPhase);
        }
        let content: &'e GameContent = self.content;
        let tile = content
            .tile_index(tile_id)
            .ok_or_else(|| CommandError::UnknownTile {
                tile: tile_id.to_string(),
            })?;
        let prop = content.property(tile).ok_or(CommandError::NotAProperty)?;
        if self.st.tiles[tile].owner != Some(p) {
            return Err(CommandError::NotOwner);
        }
        Ok((tile, prop))
    }
}
