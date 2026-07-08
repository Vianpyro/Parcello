//! Autopilot heuristics for playtesting and server-side bot seats: simple,
//! safe decisions over the public `ClientView`. Pure and synchronous like
//! the rest of the engine (no I/O, no rand, no clock) - it only reads a view
//! and the content and returns a command, so the server stays authoritative
//! and a buggy bot can at worst get its commands rejected. See ADR-0014 for
//! why this lives in the engine rather than a client.

use crate::{ClientView, CommandKind, GameContent, GamePhase, RentModel, TileKind, TurnPhase};

/// Cash the bot refuses to dip under when buying or bidding.
const RESERVE: i64 = 100;
/// Extra comfort required before sinking money into houses.
const BUILD_RESERVE: i64 = 300;
/// Extra comfort required before redeeming mortgages.
const REDEEM_RESERVE: i64 = 500;
/// Maximum per-tile boost level enforced by the engine.
const MAX_RENT_BOOSTS: u8 = 3;
/// Minimum trade edge before the bot accepts an offer.
const TRADE_MARGIN: i64 = 25;

/// What the bot wants to do right now, if anything. Pure and idempotent:
/// called after every server update, returns `None` whenever it is not this
/// seat's move.
pub fn decide(content: &GameContent, view: &ClientView, me: usize) -> Option<CommandKind> {
    if matches!(view.phase, GamePhase::Finished { .. }) || view.players[me].bankrupt {
        return None;
    }

    let bot = Bot { content, view, me };
    if let Some(command) = bot.trade_response() {
        return Some(command);
    }

    match view.turn {
        TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            ..
        } => bot.auction_action(tile, high_bid, high_bidder, turn),
        _ if view.current != me => None,
        TurnPhase::AwaitRoll => {
            if view.players[me].in_jail && view.players[me].jail_cards > 0 {
                Some(CommandKind::UseJailCard)
            } else if view.players[me].in_jail && bot.cash_after(content.rules.jail_fine) >= RESERVE
            {
                Some(CommandKind::PayJailFine)
            } else if !view.players[me].in_jail {
                bot.asset_action().or(Some(CommandKind::Roll))
            } else {
                Some(CommandKind::Roll)
            }
        }
        TurnPhase::AwaitBuy { tile } => bot.buy_action(tile),
        TurnPhase::AwaitEnd => bot.asset_action().or(Some(CommandKind::EndTurn)),
    }
}

struct Bot<'a> {
    content: &'a GameContent,
    view: &'a ClientView,
    me: usize,
}

impl Bot<'_> {
    fn cash(&self) -> i64 {
        self.view.players[self.me].cash
    }

    fn cash_after(&self, cost: i64) -> i64 {
        self.cash() - cost
    }

    fn trade_response(&self) -> Option<CommandKind> {
        let offer = self.view.pending_trades.iter().find(|t| t.to == self.me)?;
        if !self.can_fulfill_trade_payment(offer.receive_cash, &offer.receive_tiles) {
            return Some(CommandKind::DeclineTrade { trade: offer.id });
        }

        let incoming = offer.give_cash + self.tiles_value(&offer.give_tiles);
        let outgoing = offer.receive_cash + self.tiles_value(&offer.receive_tiles);
        if incoming >= outgoing + TRADE_MARGIN {
            Some(CommandKind::AcceptTrade { trade: offer.id })
        } else {
            Some(CommandKind::DeclineTrade { trade: offer.id })
        }
    }

    fn auction_action(
        &self,
        tile: usize,
        high_bid: i64,
        high_bidder: Option<usize>,
        turn: usize,
    ) -> Option<CommandKind> {
        if turn != self.me || high_bidder == Some(self.me) {
            return None;
        }
        let price = self.content.property(tile)?.price;
        let max_bid = if self.completes_group(tile) {
            price * 9 / 10
        } else {
            price * 6 / 10
        };
        let bid = high_bid + 10;
        if bid <= max_bid && self.cash_after(bid) >= RESERVE {
            Some(CommandKind::Bid { amount: bid })
        } else {
            Some(CommandKind::Pass)
        }
    }

    fn buy_action(&self, tile: usize) -> Option<CommandKind> {
        let price = self.content.property(tile)?.price;
        let reserve = if self.completes_group(tile) {
            RESERVE / 2
        } else {
            RESERVE
        };
        if self.cash_after(price) >= reserve {
            Some(CommandKind::Buy)
        } else {
            Some(CommandKind::Decline)
        }
    }

    fn asset_action(&self) -> Option<CommandKind> {
        self.sell_house_for_liquidity()
            .or_else(|| self.mortgage_for_liquidity())
            .or_else(|| self.unmortgage_best_tile())
            .or_else(|| self.build_best_tile())
            .or_else(|| self.expropriate_group_completer())
            .or_else(|| self.boost_best_tile())
    }

    fn sell_house_for_liquidity(&self) -> Option<CommandKind> {
        if self.cash() >= RESERVE {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let TileKind::Property(prop) = &self.content.board[i].kind else {
                    return false;
                };
                self.view.tiles[i].houses > 0 && self.can_sell_evenly(i, &prop.group)
            })
            .max_by_key(|&(i, _)| self.view.tiles[i].houses)?;
        Some(CommandKind::SellHouse { tile: id })
    }

    fn mortgage_for_liquidity(&self) -> Option<CommandKind> {
        if self.cash() >= RESERVE {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let TileKind::Property(prop) = &self.content.board[i].kind else {
                    return false;
                };
                !self.view.tiles[i].mortgaged && self.group_has_no_houses(&prop.group)
            })
            .max_by_key(|&(i, _)| self.content.property(i).map_or(0, |p| p.price))?;
        Some(CommandKind::Mortgage { tile: id })
    }

    fn unmortgage_best_tile(&self) -> Option<CommandKind> {
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                let cost = mortgage_redeem_cost(prop.price);
                self.view.tiles[i].mortgaged && self.cash_after(cost) >= REDEEM_RESERVE
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::Unmortgage { tile: id })
    }

    fn build_best_tile(&self) -> Option<CommandKind> {
        let cap = self.content.rules.max_houses_per_property.min(5);
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                prop.rent_model == RentModel::Houses
                    && self.view.tiles[i].houses < cap
                    && self.cash_after(prop.house_cost) >= BUILD_RESERVE
                    && self.owns_full_clean_group(&prop.group)
                    && self.can_build_evenly(i, &prop.group)
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::Build { tile: id })
    }

    fn boost_best_tile(&self) -> Option<CommandKind> {
        if self.content.rules.rent_boost <= 0 {
            return None;
        }
        let (_, id) = self
            .owned_properties()
            .filter(|&(i, _)| {
                let Some(prop) = self.content.property(i) else {
                    return false;
                };
                let cost = prop.price * self.content.rules.rent_boost / 100;
                !self.view.tiles[i].mortgaged
                    && self.view.tiles[i].boosts < MAX_RENT_BOOSTS
                    && self.cash_after(cost) >= BUILD_RESERVE
                    && self.tile_base_rent(i) > 0
            })
            .max_by_key(|&(i, _)| self.tile_priority(i))?;
        Some(CommandKind::BoostRent { tile: id })
    }

    /// Takeover only applies to the tile the bot just landed on (ADR-0022),
    /// so unlike the other asset actions this checks a single tile instead
    /// of scanning the board.
    fn expropriate_group_completer(&self) -> Option<CommandKind> {
        if self.content.rules.expropriation <= 0 || !matches!(self.view.turn, TurnPhase::AwaitEnd) {
            return None;
        }
        let i = self.view.players[self.me].position;
        let def = self.content.board.get(i)?;
        let prop = self.content.property(i)?;
        let owner = self.view.tiles[i].owner?;
        let cost = prop.price * self.content.rules.expropriation / 100;
        (owner != self.me
            && !self.view.players[owner].bankrupt
            && !self.view.tiles[i].mortgaged
            && self.cash_after(cost) >= BUILD_RESERVE
            && self.completes_group(i))
        .then(|| CommandKind::Expropriate {
            tile: def.id.clone(),
        })
    }

    fn owned_properties(&self) -> impl Iterator<Item = (usize, String)> + '_ {
        self.content
            .board
            .iter()
            .enumerate()
            .filter(move |&(i, _)| self.view.tiles[i].owner == Some(self.me))
            .filter_map(|(i, def)| self.content.property(i).map(|_| (i, def.id.clone())))
    }

    fn can_fulfill_trade_payment(&self, cash: i64, tiles: &[usize]) -> bool {
        self.cash() >= cash
            && tiles.iter().all(|&tile| {
                self.view
                    .tiles
                    .get(tile)
                    .is_some_and(|t| t.owner == Some(self.me))
            })
    }

    fn tiles_value(&self, tiles: &[usize]) -> i64 {
        tiles.iter().map(|&tile| self.tile_net_value(tile)).sum()
    }

    fn tile_net_value(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        let mortgage_value = if self.view.tiles[tile].mortgaged {
            prop.price / 2
        } else {
            prop.price
        };
        let improvement_value = self.view.tiles[tile].houses as i64 * prop.house_cost;
        let strategic = if self.completes_group(tile) {
            prop.price / 2
        } else {
            0
        };
        mortgage_value + improvement_value + strategic
    }

    fn tile_priority(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        let group_bonus = if self.owns_full_group(&prop.group) {
            prop.price / 2
        } else {
            0
        };
        self.tile_base_rent(tile) + group_bonus + prop.price / 10
    }

    fn tile_base_rent(&self, tile: usize) -> i64 {
        let Some(prop) = self.content.property(tile) else {
            return 0;
        };
        match prop.rent_model {
            RentModel::Houses => prop.rents[self.view.tiles[tile].houses as usize],
            RentModel::GroupScaled => {
                let owned = self
                    .content
                    .group_tiles(&prop.group)
                    .iter()
                    .filter(|&&t| self.view.tiles[t].owner == Some(self.me))
                    .count();
                prop.rents[owned.saturating_sub(1).min(5)]
            }
            RentModel::DiceScaled => prop.rents[0] * 7,
        }
    }

    fn completes_group(&self, tile: usize) -> bool {
        let Some(prop) = self.content.property(tile) else {
            return false;
        };
        self.content
            .group_tiles(&prop.group)
            .iter()
            .all(|&t| t == tile || self.view.tiles[t].owner == Some(self.me))
    }

    fn owns_full_group(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .iter()
            .all(|&t| self.view.tiles[t].owner == Some(self.me))
    }

    fn owns_full_clean_group(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .iter()
            .all(|&t| self.view.tiles[t].owner == Some(self.me) && !self.view.tiles[t].mortgaged)
    }

    fn group_has_no_houses(&self, group: &str) -> bool {
        self.content
            .group_tiles(group)
            .iter()
            .all(|&t| self.view.tiles[t].houses == 0)
    }

    fn can_build_evenly(&self, tile: usize, group: &str) -> bool {
        let min = self
            .content
            .group_tiles(group)
            .iter()
            .map(|&t| self.view.tiles[t].houses)
            .min()
            .unwrap_or(0);
        self.view.tiles[tile].houses == min
    }

    fn can_sell_evenly(&self, tile: usize, group: &str) -> bool {
        let max = self
            .content
            .group_tiles(group)
            .iter()
            .map(|&t| self.view.tiles[t].houses)
            .max()
            .unwrap_or(0);
        self.view.tiles[tile].houses == max
    }
}

fn mortgage_redeem_cost(price: i64) -> i64 {
    let principal = price / 2;
    principal + principal / 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GameContent, PlayerView, PropertyDef, RentModel, RuleParams, TileDef, TileState};

    fn content() -> GameContent {
        let prop = |group: &str| {
            TileKind::Property(PropertyDef {
                group: group.into(),
                price: 100,
                house_cost: 50,
                rents: [5, 10, 20, 40, 80, 160],
                rent_model: RentModel::Houses,
            })
        };
        GameContent {
            board: vec![
                TileDef {
                    id: "go".into(),
                    name: "Go".into(),
                    kind: TileKind::Go,
                },
                TileDef {
                    id: "a".into(),
                    name: "A".into(),
                    kind: prop("brown"),
                },
                TileDef {
                    id: "b".into(),
                    name: "B".into(),
                    kind: prop("brown"),
                },
            ],
            chance: vec![],
            community: vec![],
            rules: RuleParams::default(),
        }
    }

    fn advanced_content() -> GameContent {
        let prop = |group: &str, price, house_cost| {
            TileKind::Property(PropertyDef {
                group: group.into(),
                price,
                house_cost,
                rents: [10, 50, 150, 450, 625, 750],
                rent_model: RentModel::Houses,
            })
        };
        GameContent {
            board: vec![
                TileDef {
                    id: "go".into(),
                    name: "Go".into(),
                    kind: TileKind::Go,
                },
                TileDef {
                    id: "a".into(),
                    name: "A".into(),
                    kind: prop("brown", 100, 50),
                },
                TileDef {
                    id: "b".into(),
                    name: "B".into(),
                    kind: prop("brown", 100, 50),
                },
                TileDef {
                    id: "c".into(),
                    name: "C".into(),
                    kind: prop("green", 300, 100),
                },
            ],
            chance: vec![],
            community: vec![],
            rules: RuleParams {
                expropriation: 200,
                rent_boost: 25,
                ..RuleParams::default()
            },
        }
    }

    fn player(cash: i64) -> PlayerView {
        PlayerView {
            id: "p0".into(),
            name: "P0".into(),
            cash,
            position: 0,
            in_jail: false,
            jail_cards: 0,
            bankrupt: false,
        }
    }

    fn view(cash: i64, turn: TurnPhase) -> ClientView {
        ClientView {
            phase: GamePhase::Active,
            players: vec![player(cash), player(1500)],
            current: 0,
            turn,
            tiles: vec![TileState::default(); 3],
            turn_count: 0,
            pending_trades: vec![],
            subsidiaries_available: None,
            conglomerates_available: None,
        }
    }

    fn advanced_view(cash: i64, turn: TurnPhase) -> ClientView {
        let mut v = view(cash, turn);
        v.tiles = vec![TileState::default(); 4];
        v
    }

    #[test]
    fn buys_when_comfortable_declines_when_broke() {
        let c = content();
        let rich = view(1000, TurnPhase::AwaitBuy { tile: 1 });
        assert!(matches!(decide(&c, &rich, 0), Some(CommandKind::Buy)));
        let broke = view(150, TurnPhase::AwaitBuy { tile: 1 });
        assert!(matches!(decide(&c, &broke, 0), Some(CommandKind::Decline)));
    }

    #[test]
    fn bids_up_to_sixty_percent_then_passes() {
        let c = content();
        let auction = |high_bid| TurnPhase::Auction {
            tile: 1,
            high_bid,
            high_bidder: None,
            turn: 0,
            active: 3,
        };
        assert!(matches!(
            decide(&c, &view(1000, auction(20)), 0),
            Some(CommandKind::Bid { amount: 30 })
        ));
        assert!(matches!(
            decide(&c, &view(1000, auction(60)), 0),
            Some(CommandKind::Pass)
        ));
    }

    #[test]
    fn builds_evenly_on_a_full_group_then_ends_turn() {
        let c = content();
        let mut v = view(1000, TurnPhase::AwaitEnd);
        v.tiles[1] = TileState {
            owner: Some(0),
            houses: 1,
            ..Default::default()
        };
        v.tiles[2] = TileState {
            owner: Some(0),
            houses: 0,
            ..Default::default()
        };
        // Even rule: the 0-house tile of the group must come first.
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Build { tile }) if tile == "b"
        ));
        // Without the full group, no building: just end the turn.
        v.tiles[2].owner = Some(1);
        assert!(matches!(decide(&c, &v, 0), Some(CommandKind::EndTurn)));
    }

    #[test]
    fn pays_jail_fine_when_comfortable() {
        let c = content();
        let mut v = view(1000, TurnPhase::AwaitRoll);
        v.players[0].in_jail = true;
        assert!(matches!(decide(&c, &v, 0), Some(CommandKind::PayJailFine)));

        v.players[0].cash = 100;
        assert!(matches!(decide(&c, &v, 0), Some(CommandKind::Roll)));
    }

    #[test]
    fn accepts_only_profitable_incoming_trades() {
        let c = advanced_content();
        let mut v = advanced_view(1000, TurnPhase::AwaitRoll);
        v.current = 1;
        v.pending_trades.push(crate::TradeOffer {
            id: 1,
            from: 1,
            to: 0,
            give_cash: 100,
            give_tiles: vec![],
            receive_cash: 10,
            receive_tiles: vec![],
        });
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::AcceptTrade { trade: 1 })
        ));

        v.pending_trades[0].id = 2;
        v.pending_trades[0].give_cash = 10;
        v.pending_trades[0].receive_cash = 100;
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::DeclineTrade { trade: 2 })
        ));
    }

    #[test]
    fn sells_houses_then_mortgages_for_liquidity() {
        let c = advanced_content();
        let mut v = advanced_view(50, TurnPhase::AwaitEnd);
        v.tiles[1] = TileState {
            owner: Some(0),
            houses: 1,
            ..Default::default()
        };
        v.tiles[2] = TileState {
            owner: Some(0),
            houses: 1,
            ..Default::default()
        };
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::SellHouse { tile }) if tile == "a" || tile == "b"
        ));

        v.tiles[1].houses = 0;
        v.tiles[2].houses = 0;
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Mortgage { tile }) if tile == "a" || tile == "b"
        ));
    }

    #[test]
    fn redeems_mortgages_before_new_investments() {
        let c = advanced_content();
        let mut v = advanced_view(1000, TurnPhase::AwaitEnd);
        v.tiles[1] = TileState {
            owner: Some(0),
            mortgaged: true,
            ..Default::default()
        };
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Unmortgage { tile }) if tile == "a"
        ));
    }

    #[test]
    fn boosts_owned_rent_when_no_build_is_available() {
        let c = advanced_content();
        let mut v = advanced_view(380, TurnPhase::AwaitEnd);
        v.tiles[3] = TileState {
            owner: Some(0),
            ..Default::default()
        };
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::BoostRent { tile }) if tile == "c"
        ));
    }

    #[test]
    fn seizes_only_when_it_completes_a_group() {
        let c = advanced_content();
        let mut v = advanced_view(1000, TurnPhase::AwaitEnd);
        v.tiles[1].owner = Some(0);
        v.tiles[2].owner = Some(1);
        v.players[0].position = 2; // landed on "b", the rival tile
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Expropriate { tile }) if tile == "b"
        ));

        v.tiles[1].owner = None;
        assert!(matches!(decide(&c, &v, 0), Some(CommandKind::EndTurn)));
    }

    #[test]
    fn seizes_an_improved_tile_that_completes_a_group() {
        // ADR-0022: improved tiles are legal takeover targets too.
        let c = advanced_content();
        let mut v = advanced_view(1000, TurnPhase::AwaitEnd);
        v.tiles[1].owner = Some(0);
        v.tiles[2].owner = Some(1);
        v.tiles[2].houses = 2;
        v.players[0].position = 2;
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Expropriate { tile }) if tile == "b"
        ));
    }

    #[test]
    fn does_not_seize_off_the_landing_tile() {
        // ADR-0022: takeover only applies to the tile just landed on, even
        // when a rival tile elsewhere would complete a group.
        let c = advanced_content();
        let mut v = advanced_view(1000, TurnPhase::AwaitEnd);
        v.tiles[1].owner = Some(0);
        v.tiles[2].owner = Some(1);
        v.players[0].position = 0; // landed on "go", not on "b"
        assert!(!matches!(
            decide(&c, &v, 0),
            Some(CommandKind::Expropriate { .. })
        ));
    }

    #[test]
    fn stays_quiet_when_not_its_move_and_declines_trades() {
        let c = content();
        let mut v = view(1000, TurnPhase::AwaitRoll);
        v.current = 1;
        assert!(decide(&c, &v, 0).is_none());
        v.pending_trades.push(crate::TradeOffer {
            id: 7,
            from: 1,
            to: 0,
            give_cash: 1,
            give_tiles: vec![],
            receive_cash: 0,
            receive_tiles: vec![],
        });
        assert!(matches!(
            decide(&c, &v, 0),
            Some(CommandKind::DeclineTrade { trade: 7 })
        ));
    }
}
