//! Autopilot for playtesting without volunteers (`--bot`): simple, safe
//! heuristics over the public `ClientView`. The server stays authoritative,
//! so a buggy bot can at worst get its commands rejected.

use parcello_engine::{ClientView, CommandKind, GameContent, GamePhase, TileKind, TurnPhase};

/// Cash the bot refuses to dip under when buying or bidding.
const RESERVE: i64 = 100;
/// Extra comfort required before sinking money into houses.
const BUILD_RESERVE: i64 = 300;

/// What the bot wants to do right now, if anything. Pure and idempotent:
/// called after every server update, returns `None` whenever it is not this
/// seat's move.
pub fn decide(content: &GameContent, view: &ClientView, me: usize) -> Option<CommandKind> {
    if matches!(view.phase, GamePhase::Finished { .. }) || view.players[me].bankrupt {
        return None;
    }
    // Incoming trade offers: this bot does not negotiate.
    if let Some(offer) = view.pending_trades.iter().find(|t| t.to == me) {
        return Some(CommandKind::DeclineTrade { trade: offer.id });
    }

    let cash = view.players[me].cash;
    match view.turn {
        TurnPhase::Auction {
            tile,
            high_bid,
            high_bidder,
            turn,
            ..
        } => {
            if turn != me || high_bidder == Some(me) {
                return None;
            }
            // Worth up to 60% of list price at auction.
            let price = content.property(tile)?.price;
            let bid = high_bid + 10;
            if bid <= price * 6 / 10 && cash - bid >= RESERVE {
                Some(CommandKind::Bid { amount: bid })
            } else {
                Some(CommandKind::Pass)
            }
        }
        _ if view.current != me => None,
        TurnPhase::AwaitRoll => {
            if view.players[me].in_jail && view.players[me].jail_cards > 0 {
                Some(CommandKind::UseJailCard)
            } else {
                Some(CommandKind::Roll)
            }
        }
        TurnPhase::AwaitBuy { tile } => {
            let price = content.property(tile)?.price;
            if cash - price >= RESERVE {
                Some(CommandKind::Buy)
            } else {
                Some(CommandKind::Decline)
            }
        }
        TurnPhase::AwaitEnd => match buildable_tile(content, view, me, cash) {
            Some(tile) => Some(CommandKind::Build { tile }),
            None => Some(CommandKind::EndTurn),
        },
    }
}

/// First tile where one more house is legal (full unmortgaged group, even
/// rule, under the cap) and affordable. Mirrors the engine's checks so the
/// bot rarely gets rejected; the engine still has the final say.
fn buildable_tile(
    content: &GameContent,
    view: &ClientView,
    me: usize,
    cash: i64,
) -> Option<String> {
    let cap = content.rules.max_houses_per_property.min(5);
    for (i, def) in content.board.iter().enumerate() {
        let TileKind::Property(prop) = &def.kind else {
            continue;
        };
        if prop.rent_model != parcello_engine::RentModel::Houses
            || view.tiles[i].owner != Some(me)
            || view.tiles[i].houses >= cap
            || cash - prop.house_cost < BUILD_RESERVE
        {
            continue;
        }
        let group = content.group_tiles(&prop.group);
        let full_and_clean = group
            .iter()
            .all(|&t| view.tiles[t].owner == Some(me) && !view.tiles[t].mortgaged);
        if !full_and_clean {
            continue;
        }
        let min = group
            .iter()
            .map(|&t| view.tiles[t].houses)
            .min()
            .unwrap_or(0);
        if view.tiles[i].houses == min {
            return Some(def.id.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcello_engine::{
        GameContent, PlayerView, PropertyDef, RentModel, RuleParams, TileDef, TileState,
    };

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
        }
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
            mortgaged: false,
        };
        v.tiles[2] = TileState {
            owner: Some(0),
            houses: 0,
            mortgaged: false,
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
    fn stays_quiet_when_not_its_move_and_declines_trades() {
        let c = content();
        let mut v = view(1000, TurnPhase::AwaitRoll);
        v.current = 1;
        assert!(decide(&c, &v, 0).is_none());
        v.pending_trades.push(parcello_engine::TradeOffer {
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
