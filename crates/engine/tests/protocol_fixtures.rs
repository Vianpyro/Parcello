//! Golden wire-format fixtures for `CommandKind`, `Event`, and `CommandError`
//! (protocol duplication audit, Strategy S1, `docs/protocol-duplication-audit.md`).
//!
//! Every variant of these three enums is the one Dart hand-mirrors with a
//! `switch` that can fall back to a silent default (`describeEvent`,
//! `rejectReason` in `clients/flutter/lib/protocol.dart`, and the ad hoc
//! `Map` literals built for outgoing commands). The exhaustive `match` below
//! has no wildcard arm, so adding a new variant without updating this file
//! is a compile error - the fixture cannot be "forgotten" the way a runtime
//! `default:` case in Dart can.
//!
//! Fixtures live in `protocol-fixtures/` (repo root, shared with the Flutter
//! test suite: `clients/flutter/test/protocol_fixtures_test.dart`): one JSON
//! object per enum, keyed by variant name, value the canonical wire shape.
//! Grouping keeps the count at five files total instead of one per variant
//!   - same guarantees (every variant is still individually named, looked up,
//!     and compared), far less repository noise.
//!
//! To add a new variant: add a match arm below with a representative
//! instance, then run
//! `cargo test -p parcello-engine --test protocol_fixtures -- --ignored regenerate_fixtures`
//! to write it into the fixture file, and commit the change.

use parcello_engine::{CommandError, CommandKind, DeckKind, Event, MarketEffect};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../protocol-fixtures")
        .join(format!("{name}.json"))
}

fn read_fixtures(name: &str) -> Map<String, Value> {
    let path = fixture_path(name);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing fixture {}: {e}", path.display()));
    match serde_json::from_str(&text).unwrap() {
        Value::Object(map) => map,
        other => panic!("{}: expected a JSON object, got {other:?}", path.display()),
    }
}

fn write_fixtures(name: &str, entries: &[(&str, Value)]) {
    let map: Map<String, Value> = entries
        .iter()
        .cloned()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    let json = serde_json::to_string_pretty(&Value::Object(map)).unwrap();
    fs::write(fixture_path(name), format!("{json}\n")).unwrap();
}

/// Looks up `name` in the fixture map, deserializes it as `T`, and asserts
/// it round-trips to exactly `instance` (both directions: the checked-in
/// JSON deserializes to `instance`, and `instance` serializes back to the
/// same JSON value).
fn assert_fixture<T>(fixtures: &Map<String, Value>, name: &str, instance: &T)
where
    T: Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let stored = fixtures
        .get(name)
        .unwrap_or_else(|| panic!("no fixture entry for {name:?} - run `write_fixtures`"));
    let from_fixture: T = serde_json::from_value(stored.clone())
        .unwrap_or_else(|e| panic!("fixture for {name:?} does not deserialize: {e}"));
    assert_eq!(&from_fixture, instance, "fixture drift for {name}");
    let serialized = serde_json::to_value(instance).unwrap();
    assert_eq!(&serialized, stored, "fixture drift for {name}");
}

/// Every `CommandKind` variant, named for its fixture entry. No wildcard
/// arm: a new variant is a compile error here until it gets a name and a
/// case in `command_kind_fixtures` below.
const fn command_kind_name(k: &CommandKind) -> &'static str {
    match k {
        CommandKind::PlayMovementCard { .. } => "play_movement_card",
        CommandKind::Build { .. } => "build",
        CommandKind::ProposeTrade { .. } => "propose_trade",
        CommandKind::AcceptTrade { .. } => "accept_trade",
        CommandKind::DeclineTrade { .. } => "decline_trade",
        CommandKind::CancelTrade { .. } => "cancel_trade",
        CommandKind::SubmitBlindBid { .. } => "submit_blind_bid",
        CommandKind::SellHouse { .. } => "sell_house",
        CommandKind::Expropriate { .. } => "expropriate",
        CommandKind::BoostRent { .. } => "boost_rent",
        CommandKind::Mortgage { .. } => "mortgage",
        CommandKind::Unmortgage { .. } => "unmortgage",
        CommandKind::ChooseLegalRoute { .. } => "choose_legal_route",
        CommandKind::OfferBribe { .. } => "offer_bribe",
        CommandKind::VoteOnBribe { .. } => "vote_on_bribe",
        CommandKind::UseJailCard => "use_jail_card",
        CommandKind::EndTurn => "end_turn",
        CommandKind::Resign => "resign",
    }
}

fn command_kind_fixtures() -> Vec<CommandKind> {
    vec![
        CommandKind::PlayMovementCard { value: 3 },
        CommandKind::Build {
            tile: "ave_a".into(),
        },
        CommandKind::ProposeTrade {
            to: "guest:bob".into(),
            give_cash: 50,
            give_tiles: vec!["ave_a".into()],
            receive_cash: 0,
            receive_tiles: vec![],
        },
        CommandKind::AcceptTrade { trade: 4 },
        CommandKind::DeclineTrade { trade: 4 },
        CommandKind::CancelTrade { trade: 4 },
        CommandKind::SubmitBlindBid { amount: 60 },
        CommandKind::SellHouse {
            tile: "ave_a".into(),
        },
        CommandKind::Expropriate {
            tile: "ave_a".into(),
        },
        CommandKind::BoostRent {
            tile: "ave_a".into(),
        },
        CommandKind::Mortgage {
            tile: "ave_a".into(),
        },
        CommandKind::Unmortgage {
            tile: "ave_a".into(),
        },
        CommandKind::ChooseLegalRoute {
            order: vec![1, 2, 3, 4, 5],
        },
        CommandKind::OfferBribe { amount: 90 },
        CommandKind::VoteOnBribe { accept: true },
        CommandKind::UseJailCard,
        CommandKind::EndTurn,
        CommandKind::Resign,
    ]
}

/// Every `Event` variant. Same no-wildcard discipline as `command_kind_name`.
const fn event_name(e: &Event) -> &'static str {
    match e {
        Event::TurnStarted { .. } => "turn_started",
        Event::MovementCardPlayed { .. } => "movement_card_played",
        Event::Moved { .. } => "moved",
        Event::SalaryPaid { .. } => "salary_paid",
        Event::BlindAuctionOpened { .. } => "blind_auction_opened",
        Event::BlindBidSubmitted { .. } => "blind_bid_submitted",
        Event::BlindAuctionResolved { .. } => "blind_auction_resolved",
        Event::DiscovererRefunded { .. } => "discoverer_refunded",
        Event::TradeProposed { .. } => "trade_proposed",
        Event::TradeAccepted { .. } => "trade_accepted",
        Event::TradeDeclined { .. } => "trade_declined",
        Event::TradeCancelled { .. } => "trade_cancelled",
        Event::RentPaid { .. } => "rent_paid",
        Event::TaxPaid { .. } => "tax_paid",
        Event::CardDrawn { .. } => "card_drawn",
        Event::CashAdjusted { .. } => "cash_adjusted",
        Event::HouseBuilt { .. } => "house_built",
        Event::HouseSold { .. } => "house_sold",
        Event::Expropriated { .. } => "expropriated",
        Event::RentBoosted { .. } => "rent_boosted",
        Event::RentBoostConsumed { .. } => "rent_boost_consumed",
        Event::PropertyMortgaged { .. } => "property_mortgaged",
        Event::PropertyUnmortgaged { .. } => "property_unmortgaged",
        Event::WentToJail { .. } => "went_to_jail",
        Event::JailCardReceived { .. } => "jail_card_received",
        Event::JailCardUsed { .. } => "jail_card_used",
        Event::LeftJail { .. } => "left_jail",
        Event::LegalRouteChosen { .. } => "legal_route_chosen",
        Event::BribeOffered { .. } => "bribe_offered",
        Event::BribeVoteCast { .. } => "bribe_vote_cast",
        Event::BribeResolved { .. } => "bribe_resolved",
        Event::PropertyTransferred { .. } => "property_transferred",
        Event::PlayerBankrupt { .. } => "player_bankrupt",
        Event::PlayerResigned { .. } => "player_resigned",
        Event::GameEnded { .. } => "game_ended",
        Event::TimeUp { .. } => "time_up",
        Event::WonByGroups { .. } => "won_by_groups",
        Event::WonByPoints { .. } => "won_by_points",
        Event::WonByPoolExhaustion { .. } => "won_by_pool_exhaustion",
        Event::MarketEventActivated { .. } => "market_event_activated",
        Event::MarketEventExpired { .. } => "market_event_expired",
        Event::RoundBonusAwarded { .. } => "round_bonus_awarded",
        Event::SpotlightStarted { .. } => "spotlight_started",
        Event::SpotlightEnded { .. } => "spotlight_ended",
    }
}

fn event_fixtures() -> Vec<Event> {
    let mut events = movement_event_fixtures();
    events.extend(auction_event_fixtures());
    events.extend(trade_event_fixtures());
    events.extend(cash_event_fixtures());
    events.extend(estate_event_fixtures());
    events.extend(jail_event_fixtures());
    events.extend(corruption_event_fixtures());
    events.extend(transfer_and_win_event_fixtures());
    events.extend(market_and_world_event_fixtures());
    events
}

fn movement_event_fixtures() -> Vec<Event> {
    vec![
        Event::TurnStarted { player: 0 },
        Event::MovementCardPlayed {
            player: 0,
            value: 4,
        },
        Event::Moved {
            player: 0,
            from: 0,
            to: 4,
            passed_go: false,
        },
        Event::SalaryPaid {
            player: 0,
            amount: 200,
        },
    ]
}

fn auction_event_fixtures() -> Vec<Event> {
    vec![
        Event::BlindAuctionOpened {
            tile: 3,
            discoverer: 0,
            floor: 60,
        },
        Event::BlindBidSubmitted { player: 0 },
        Event::BlindAuctionResolved {
            tile: 3,
            discoverer: 0,
            winner: Some(1),
            amount: 60,
            bids: vec![0, 60],
        },
        Event::DiscovererRefunded {
            player: 0,
            tile: 3,
            amount: 6,
        },
    ]
}

fn trade_event_fixtures() -> Vec<Event> {
    vec![
        Event::TradeProposed {
            trade: 4,
            from: 0,
            to: 1,
        },
        Event::TradeAccepted {
            trade: 4,
            from: 0,
            to: 1,
        },
        Event::TradeDeclined {
            trade: 4,
            from: 0,
            to: 1,
        },
        Event::TradeCancelled {
            trade: 4,
            from: 0,
            to: 1,
        },
    ]
}

fn cash_event_fixtures() -> Vec<Event> {
    vec![
        Event::RentPaid {
            from: 1,
            to: 0,
            tile: 3,
            amount: 10,
        },
        Event::TaxPaid {
            player: 0,
            tile: 8,
            amount: 100,
        },
        Event::CardDrawn {
            player: 0,
            deck: DeckKind::Chance,
            card: "advance_go".into(),
            text: "Advance to Go.".into(),
        },
        Event::CashAdjusted {
            player: 0,
            delta: 50,
            reason: "advance_go".into(),
        },
    ]
}

fn estate_event_fixtures() -> Vec<Event> {
    vec![
        Event::HouseBuilt {
            player: 0,
            tile: 3,
            houses: 1,
            cost: 50,
        },
        Event::HouseSold {
            player: 0,
            tile: 3,
            houses: 1,
            refund: 25,
        },
        Event::Expropriated {
            player: 0,
            from: 1,
            tile: 3,
            cost: 120,
            liquidated: 0,
            liquidation_refund: 0,
        },
        Event::RentBoosted {
            player: 0,
            tile: 3,
            boosts: 1,
            cost: 30,
        },
        Event::RentBoostConsumed { tile: 3 },
        Event::PropertyMortgaged {
            player: 0,
            tile: 3,
            value: 30,
        },
        Event::PropertyUnmortgaged {
            player: 0,
            tile: 3,
            cost: 33,
        },
    ]
}

fn jail_event_fixtures() -> Vec<Event> {
    vec![
        Event::WentToJail {
            player: 0,
            from: 12,
        },
        Event::JailCardReceived { player: 0 },
        Event::JailCardUsed { player: 0 },
        Event::LeftJail { player: 0 },
        Event::LegalRouteChosen {
            player: 0,
            order: vec![1, 2, 3, 4, 5],
        },
    ]
}

fn corruption_event_fixtures() -> Vec<Event> {
    vec![
        Event::BribeOffered {
            player: 0,
            amount: 90,
        },
        Event::BribeVoteCast { player: 1 },
        Event::BribeResolved {
            briber: 0,
            amount: 90,
            succeeded: true,
            accepts: 2,
            total: 3,
        },
    ]
}

fn transfer_and_win_event_fixtures() -> Vec<Event> {
    vec![
        Event::PropertyTransferred {
            tile: 3,
            from: 0,
            to: Some(1),
        },
        Event::PlayerBankrupt {
            player: 0,
            creditor: Some(1),
        },
        Event::PlayerResigned { player: 0 },
        Event::GameEnded { winner: 0 },
        Event::TimeUp { winner: 0 },
        Event::WonByGroups {
            winner: 0,
            groups: 3,
        },
        Event::WonByPoints {
            player: 0,
            points: 20,
        },
        Event::WonByPoolExhaustion { winner: 0 },
    ]
}

fn market_and_world_event_fixtures() -> Vec<Event> {
    vec![
        Event::MarketEventActivated {
            event_id: "rent_spike".into(),
            effect: MarketEffect::RentMultiplier,
            magnitude_pct: 50,
            duration_turns: 4,
        },
        Event::MarketEventExpired {
            event_id: "rent_spike".into(),
        },
        Event::RoundBonusAwarded {
            player: 0,
            points: 2,
        },
        Event::SpotlightStarted {
            tile: 5,
            rent_pct: 100,
            duration_turns: 0,
        },
        Event::SpotlightEnded { tile: 5 },
    ]
}

/// Every `CommandError` variant. Same no-wildcard discipline.
const fn command_error_name(e: &CommandError) -> &'static str {
    match e {
        CommandError::GameFinished => "game_finished",
        CommandError::UnknownPlayer => "unknown_player",
        CommandError::Bankrupt => "bankrupt",
        CommandError::NotYourTurn => "not_your_turn",
        CommandError::WrongPhase => "wrong_phase",
        CommandError::UnknownTile { .. } => "unknown_tile",
        CommandError::NotAProperty => "not_a_property",
        CommandError::NotOwner => "not_owner",
        CommandError::GroupIncomplete => "group_incomplete",
        CommandError::BuildLimit => "build_limit",
        CommandError::NotBuildable => "not_buildable",
        CommandError::UnevenBuild => "uneven_build",
        CommandError::NoHouses => "no_houses",
        CommandError::MortgagedInGroup => "mortgaged_in_group",
        CommandError::AlreadyMortgaged => "already_mortgaged",
        CommandError::NotMortgaged => "not_mortgaged",
        CommandError::HousesInGroup => "houses_in_group",
        CommandError::TradeNotFound => "trade_not_found",
        CommandError::NotTradeParty => "not_trade_party",
        CommandError::TradeInvalid => "trade_invalid",
        CommandError::TradeLimit => "trade_limit",
        CommandError::InsufficientFunds => "insufficient_funds",
        CommandError::AlreadyBid => "already_bid",
        CommandError::BidBelowFloor => "bid_below_floor",
        CommandError::NotInJail => "not_in_jail",
        CommandError::NoJailCard => "no_jail_card",
        CommandError::ExpropriationDisabled => "expropriation_disabled",
        CommandError::NotExpropriable => "not_expropriable",
        CommandError::NotOnTile => "not_on_tile",
        CommandError::PoolExhausted => "pool_exhausted",
        CommandError::RentBoostDisabled => "rent_boost_disabled",
        CommandError::BoostLimit => "boost_limit",
        CommandError::CardNotPlayable => "card_not_playable",
        CommandError::InvalidRoute => "invalid_route",
        CommandError::AlreadyVoted => "already_voted",
    }
}

fn command_error_fixtures() -> Vec<CommandError> {
    vec![
        CommandError::GameFinished,
        CommandError::UnknownPlayer,
        CommandError::Bankrupt,
        CommandError::NotYourTurn,
        CommandError::WrongPhase,
        CommandError::UnknownTile {
            tile: "ave_a".into(),
        },
        CommandError::NotAProperty,
        CommandError::NotOwner,
        CommandError::GroupIncomplete,
        CommandError::BuildLimit,
        CommandError::NotBuildable,
        CommandError::UnevenBuild,
        CommandError::NoHouses,
        CommandError::MortgagedInGroup,
        CommandError::AlreadyMortgaged,
        CommandError::NotMortgaged,
        CommandError::HousesInGroup,
        CommandError::TradeNotFound,
        CommandError::NotTradeParty,
        CommandError::TradeInvalid,
        CommandError::TradeLimit,
        CommandError::InsufficientFunds,
        CommandError::AlreadyBid,
        CommandError::BidBelowFloor,
        CommandError::NotInJail,
        CommandError::NoJailCard,
        CommandError::ExpropriationDisabled,
        CommandError::NotExpropriable,
        CommandError::NotOnTile,
        CommandError::PoolExhausted,
        CommandError::RentBoostDisabled,
        CommandError::BoostLimit,
        CommandError::CardNotPlayable,
        CommandError::InvalidRoute,
        CommandError::AlreadyVoted,
    ]
}

#[test]
fn command_kind_wire_format_matches_fixtures() {
    let fixtures = read_fixtures("command_kind");
    for kind in command_kind_fixtures() {
        assert_fixture(&fixtures, command_kind_name(&kind), &kind);
    }
}

#[test]
fn event_wire_format_matches_fixtures() {
    let fixtures = read_fixtures("event");
    for event in event_fixtures() {
        assert_fixture(&fixtures, event_name(&event), &event);
    }
}

#[test]
fn command_error_wire_format_matches_fixtures() {
    let fixtures = read_fixtures("command_error");
    for err in command_error_fixtures() {
        assert_fixture(&fixtures, command_error_name(&err), &err);
    }
}

/// Not part of CI: regenerates the committed fixture files from the
/// canonical instances above. Run after adding a new variant + match arm:
/// `cargo test -p parcello-engine --test protocol_fixtures -- --ignored regenerate_fixtures`
#[test]
#[ignore = "run explicitly with --ignored to regenerate the committed fixtures"]
fn regenerate_fixtures() {
    write_fixtures(
        "command_kind",
        &command_kind_fixtures()
            .iter()
            .map(|k| (command_kind_name(k), serde_json::to_value(k).unwrap()))
            .collect::<Vec<_>>(),
    );
    write_fixtures(
        "event",
        &event_fixtures()
            .iter()
            .map(|e| (event_name(e), serde_json::to_value(e).unwrap()))
            .collect::<Vec<_>>(),
    );
    write_fixtures(
        "command_error",
        &command_error_fixtures()
            .iter()
            .map(|e| (command_error_name(e), serde_json::to_value(e).unwrap()))
            .collect::<Vec<_>>(),
    );
}
