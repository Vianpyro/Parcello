//! Unit tests for the autopilot heuristic (split out of `bot.rs` for
//! module size, mirroring `room/tests.rs`).

use super::*;
use crate::{
    GameContent, MarketForecast, PlayerView, PropertyDef, RentModel, RuleParams, TileDef, TileState,
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
        market_events: vec![],
        forecast_gap_turns: 0,
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
        market_events: vec![],
        forecast_gap_turns: 0,
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
        victory_points: 0,
        hand: vec![1, 2, 3, 4, 5],
        jail_route: None,
        hands_cycled: 0,
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
        forecast: MarketForecast::default(),
        spotlight: None,
    }
}

fn advanced_view(cash: i64, turn: TurnPhase) -> ClientView {
    let mut v = view(cash, turn);
    v.tiles = vec![TileState::default(); 4];
    v
}

#[test]
fn high_bids_are_rare_not_impossible() {
    // The jitter is a descending triangle (like the Audit's brackets): a bot
    // mostly bids near list price and only occasionally reaches high. Without
    // the weighting this was uniform, which made bots relentless bidders.
    let c = content();
    let mut low = 0; // 100-150% of the 100 price
    let mut high = 0; // >150%
    for noise in 0..512u64 {
        let mut v = view(
            1000,
            TurnPhase::BlindAuction {
                tile: 1,
                bids: vec![None, None],
            },
        );
        v.current = 1;
        if let Some(CommandKind::SubmitBlindBid { amount }) = decide(&c, &v, 0, noise) {
            if amount <= 150 {
                low += 1;
            } else {
                high += 1;
            }
        }
    }
    assert!(
        low > high * 2,
        "expected the low half to dominate a descending triangle, got low={low} high={high}"
    );
    assert!(high > 0, "high bids must stay possible, just rare");
}

#[test]
fn discoverer_bids_at_least_the_floor_when_affordable_else_abstains() {
    let c = content();
    let auction = |bids| TurnPhase::BlindAuction { tile: 1, bids };
    // Property-based across many noise words: the jittered bid always
    // sits in [floor, cash] for a solvent discoverer (price 100).
    for noise in 0..64u64 {
        let rich = view(1000, auction(vec![None, None]));
        match decide(&c, &rich, 0, noise) {
            Some(CommandKind::SubmitBlindBid { amount }) => {
                assert!(
                    (100..=1000).contains(&amount),
                    "discoverer bid {amount} outside [floor, cash] for noise {noise}"
                );
            }
            other => panic!("expected a bid, got {other:?}"),
        }
    }
    let broke = view(50, auction(vec![None, None]));
    assert!(matches!(
        decide(&c, &broke, 0, 7),
        Some(CommandKind::SubmitBlindBid { amount: 0 })
    ));
}

#[test]
fn non_discoverer_bids_jittered_or_abstains_then_stays_quiet_once_bid() {
    let c = content();
    // A bidder never goes under the list price: a sub-floor bid cannot beat
    // the discoverer's implicit floor, but WOULD win against an insolvent one
    // and buy the tile under list. So the bid stays in [price, cash - RESERVE]
    // (price 100), whatever the noise word.
    for noise in 0..64u64 {
        let mut v = view(
            1000,
            TurnPhase::BlindAuction {
                tile: 1,
                bids: vec![None, None],
            },
        );
        v.current = 1; // someone else discovered it; seat 0 is a bidder
        match decide(&c, &v, 0, noise) {
            Some(CommandKind::SubmitBlindBid { amount }) => {
                assert!(
                    (100..=900).contains(&amount),
                    "bid {amount} outside [price, cash - reserve] for noise {noise}"
                );
            }
            other => panic!("expected a bid, got {other:?}"),
        }
    }

    let mut v = view(
        120,
        TurnPhase::BlindAuction {
            tile: 1,
            bids: vec![None, None],
        },
    );
    v.current = 1;
    // cash - RESERVE = 20 < price = 100: abstain rather than lowball.
    assert!(matches!(
        decide(&c, &v, 0, 7),
        Some(CommandKind::SubmitBlindBid { amount: 0 })
    ));

    v.players[0].cash = 1000;
    v.turn = TurnPhase::BlindAuction {
        tile: 1,
        bids: vec![Some(60), None],
    };
    assert!(decide(&c, &v, 0, 7).is_none());
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::Build { tile }) if tile == "b"
    ));
    // Without the full group, no building: just end the turn.
    v.tiles[2].owner = Some(1);
    assert!(matches!(decide(&c, &v, 0, 7), Some(CommandKind::EndTurn)));
}

#[test]
fn jail_triage_prefers_card_then_bribe_then_route() {
    let c = content();
    let mut v = view(1000, TurnPhase::AwaitMove);
    v.players[0].in_jail = true;

    // Rich and no card: bribe.
    assert!(matches!(
        decide(&c, &v, 0, 7),
        Some(CommandKind::OfferBribe { amount: 200 })
    ));

    // Holding a card takes priority over everything else.
    v.players[0].jail_cards = 1;
    assert!(matches!(
        decide(&c, &v, 0, 7),
        Some(CommandKind::UseJailCard)
    ));

    // Too poor to comfortably bribe and no card: the safe default, an
    // ascending Legal Route.
    v.players[0].jail_cards = 0;
    v.players[0].cash = 150;
    assert!(matches!(
        decide(&c, &v, 0, 7),
        Some(CommandKind::ChooseLegalRoute { order }) if order == vec![1, 2, 3, 4, 5]
    ));
}

#[test]
fn accepts_only_profitable_incoming_trades() {
    let c = advanced_content();
    let mut v = advanced_view(1000, TurnPhase::AwaitMove);
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::AcceptTrade { trade: 1 })
    ));

    v.pending_trades[0].id = 2;
    v.pending_trades[0].give_cash = 10;
    v.pending_trades[0].receive_cash = 100;
    assert!(matches!(
        decide(&c, &v, 0, 7),
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::SellHouse { tile }) if tile == "a" || tile == "b"
    ));

    v.tiles[1].houses = 0;
    v.tiles[2].houses = 0;
    assert!(matches!(
        decide(&c, &v, 0, 7),
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
        decide(&c, &v, 0, 7),
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
        decide(&c, &v, 0, 7),
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::Expropriate { tile }) if tile == "b"
    ));

    v.tiles[1].owner = None;
    assert!(matches!(decide(&c, &v, 0, 7), Some(CommandKind::EndTurn)));
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
        decide(&c, &v, 0, 7),
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::Expropriate { .. })
    ));
}

#[test]
fn stays_quiet_when_not_its_move_and_declines_trades() {
    let c = content();
    let mut v = view(1000, TurnPhase::AwaitMove);
    v.current = 1;
    assert!(decide(&c, &v, 0, 7).is_none());
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
        decide(&c, &v, 0, 7),
        Some(CommandKind::DeclineTrade { trade: 7 })
    ));
}

#[test]
fn movement_card_picks_the_best_scoring_card_only_for_a_plain_move() {
    let c = content();
    // From Go (0), cards 1/2/4/5 all land on a buyable property (top
    // score); the tie-break takes the lowest, so card 1.
    let v = view(1500, TurnPhase::AwaitMove);
    assert_eq!(movement_card(&c, &v, 0), Some(1));

    // Not this seat's turn: no movement.
    let mut other = view(1500, TurnPhase::AwaitMove);
    other.current = 1;
    assert_eq!(movement_card(&c, &other, 0), None);

    // Jailed or mid-route: the caller must use the jail exit / route
    // front instead, so movement_card declines.
    let mut jailed = view(1500, TurnPhase::AwaitMove);
    jailed.players[0].in_jail = true;
    assert_eq!(movement_card(&c, &jailed, 0), None);

    let mut routed = view(1500, TurnPhase::AwaitMove);
    routed.players[0].jail_route = Some(vec![2, 1]);
    assert_eq!(movement_card(&c, &routed, 0), None);

    // Wrong phase (waiting to end the turn): no movement.
    let ended = view(1500, TurnPhase::AwaitEnd);
    assert_eq!(movement_card(&c, &ended, 0), None);
}

#[test]
fn bribe_vote_accepts_a_material_share_and_rejects_a_token_one() {
    let c = content();
    let vote = |amount| TurnPhase::BribeVote {
        briber: 1,
        amount,
        votes: vec![None, None],
    };

    // Two players, one opponent: the whole amount is seat 0's share.
    // RESERVE/2 = 50 is the acceptance floor.
    let generous = view(1500, vote(60));
    assert!(matches!(
        decide(&c, &generous, 0, 0),
        Some(CommandKind::VoteOnBribe { accept: true })
    ));
    let stingy = view(1500, vote(20));
    assert!(matches!(
        decide(&c, &stingy, 0, 0),
        Some(CommandKind::VoteOnBribe { accept: false })
    ));

    // The briber never votes on its own offer.
    assert_eq!(decide(&c, &generous, 1, 0), None);

    // A seat that already voted stays silent.
    let mut voted = view(1500, vote(60));
    let TurnPhase::BribeVote { votes, .. } = &mut voted.turn else {
        unreachable!()
    };
    votes[0] = Some(true);
    assert_eq!(decide(&c, &voted, 0, 0), None);
}

#[test]
fn jailed_bot_triages_card_then_bribe_then_legal_route() {
    let c = content();

    // Holding a jail card: always the simplest exit.
    let mut carded = view(1500, TurnPhase::AwaitMove);
    carded.players[0].in_jail = true;
    carded.players[0].jail_cards = 1;
    assert!(matches!(
        decide(&c, &carded, 0, 0),
        Some(CommandKind::UseJailCard)
    ));

    // Rich and cardless: bribes (2x the reserve).
    let mut rich = view(1500, TurnPhase::AwaitMove);
    rich.players[0].in_jail = true;
    assert!(matches!(
        decide(&c, &rich, 0, 0),
        Some(CommandKind::OfferBribe { amount: 200 })
    ));

    // Broke: the safe default, an ascending Legal Route over the hand.
    let mut broke = view(120, TurnPhase::AwaitMove);
    broke.players[0].in_jail = true;
    let Some(CommandKind::ChooseLegalRoute { order }) = decide(&c, &broke, 0, 0) else {
        panic!("expected a Legal Route");
    };
    assert_eq!(order, vec![1, 2, 3, 4, 5], "ascending over the full hand");
}

#[test]
fn a_seat_serving_a_route_plays_its_front_card() {
    let c = content();
    let mut routed = view(1500, TurnPhase::AwaitMove);
    routed.players[0].jail_route = Some(vec![4, 2, 1]);
    assert!(matches!(
        decide(&c, &routed, 0, 0),
        Some(CommandKind::PlayMovementCard { value: 4 })
    ));
}

#[test]
fn trades_are_accepted_when_the_margin_is_met_and_declined_when_unaffordable() {
    let c = content();

    // Free money above the margin: accept.
    let mut gift = view(1500, TurnPhase::AwaitEnd);
    gift.current = 1; // not our turn - trade responses are turn-independent
    gift.pending_trades.push(crate::TradeOffer {
        id: 9,
        from: 1,
        to: 0,
        give_cash: 100,
        give_tiles: vec![],
        receive_cash: 0,
        receive_tiles: vec![],
    });
    assert!(matches!(
        decide(&c, &gift, 0, 0),
        Some(CommandKind::AcceptTrade { trade: 9 })
    ));

    // Demands more cash than the seat holds: declined as unaffordable.
    let mut broke = view(50, TurnPhase::AwaitEnd);
    broke.current = 1;
    broke.pending_trades.push(crate::TradeOffer {
        id: 3,
        from: 1,
        to: 0,
        give_cash: 500,
        give_tiles: vec![],
        receive_cash: 200,
        receive_tiles: vec![],
    });
    assert!(matches!(
        decide(&c, &broke, 0, 0),
        Some(CommandKind::DeclineTrade { trade: 3 })
    ));
}
