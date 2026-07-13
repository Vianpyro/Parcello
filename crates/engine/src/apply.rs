//! The command pipeline: validate -> apply -> emit events.
//!
//! `apply` works on an owned clone of the caller's state; rejected commands
//! therefore never leak partial mutations. All movement, payment, and
//! bankruptcy logic lives here; content lookups go through the registries
//! and rule fragments go through the injected strategies.

use crate::command::{CommandKind, PlayerCommand};
use crate::content::{CardEffect, GameContent, MarketEffect, PropertyDef, RentModel, TileKind};
use crate::error::CommandError;
use crate::event::{DeckKind, Event};
use crate::state::{ActiveMarketEvent, GamePhase, GameState, Spotlight, TradeOffer, TurnPhase};
use crate::{Engine, Strategies};

use crate::tuning::{
    CONTESTED_WIN_PAY_PCT, HOUSE_REFUND_PCT, MAX_CARD_CHAIN_DEPTH, MAX_OPEN_TRADES_PER_PLAYER,
    MAX_RENT_BOOSTS, MORTGAGE_INTEREST_PCT, MORTGAGE_VALUE_PCT, RENT_BOOST_STEP_PCT,
    ROUND_BONUS_VP, SPOTLIGHT_NO_EXPIRY,
};

pub fn apply(
    engine: &Engine,
    state: &GameState,
    cmd: &PlayerCommand,
) -> Result<(GameState, Vec<Event>), CommandError> {
    if matches!(state.phase, GamePhase::Finished { .. }) {
        return Err(CommandError::GameFinished);
    }
    let player = state
        .players
        .iter()
        .position(|p| p.id == cmd.player)
        .ok_or(CommandError::UnknownPlayer)?;
    if state.players[player].bankrupt {
        return Err(CommandError::Bankrupt);
    }
    let any_turn = matches!(
        cmd.kind,
        CommandKind::Resign
            | CommandKind::ProposeTrade { .. }
            | CommandKind::AcceptTrade { .. }
            | CommandKind::DeclineTrade { .. }
            | CommandKind::CancelTrade { .. }
    );
    // A sealed-bid window (ADR-0018) or a bribe vote (ADR-0024) has no
    // single actor: any living seat may act while one is open, regardless
    // of whose turn it nominally is.
    let in_open_bid = matches!(cmd.kind, CommandKind::SubmitBlindBid { .. })
        && matches!(state.turn, TurnPhase::BlindAuction { .. });
    let in_open_vote = matches!(cmd.kind, CommandKind::VoteOnBribe { .. })
        && matches!(state.turn, TurnPhase::BribeVote { .. });
    if !any_turn && !in_open_bid && !in_open_vote && player != state.current {
        return Err(CommandError::NotYourTurn);
    }

    let mut exec = Exec {
        content: engine.content(),
        strategies: engine.strategies(),
        st: state.clone(),
        ev: Vec::new(),
    };

    match &cmd.kind {
        CommandKind::PlayMovementCard { value } => exec.play_movement_card(player, *value)?,
        CommandKind::SubmitBlindBid { amount } => exec.submit_blind_bid(player, *amount)?,
        CommandKind::ProposeTrade {
            to,
            give_cash,
            give_tiles,
            receive_cash,
            receive_tiles,
        } => exec.propose_trade(
            player,
            to,
            *give_cash,
            give_tiles,
            *receive_cash,
            receive_tiles,
        )?,
        CommandKind::AcceptTrade { trade } => exec.accept_trade(player, *trade)?,
        CommandKind::DeclineTrade { trade } => exec.decline_trade(player, *trade)?,
        CommandKind::CancelTrade { trade } => exec.cancel_trade(player, *trade)?,
        CommandKind::Build { tile } => exec.build(player, tile)?,
        CommandKind::SellHouse { tile } => exec.sell_house(player, tile)?,
        CommandKind::Expropriate { tile } => exec.expropriate(player, tile)?,
        CommandKind::BoostRent { tile } => exec.boost_rent(player, tile)?,
        CommandKind::Mortgage { tile } => exec.mortgage(player, tile)?,
        CommandKind::Unmortgage { tile } => exec.unmortgage(player, tile)?,
        CommandKind::ChooseLegalRoute { order } => {
            exec.choose_legal_route(player, order.clone())?;
        }
        CommandKind::OfferBribe { amount } => exec.offer_bribe(player, *amount)?,
        CommandKind::VoteOnBribe { accept } => exec.vote_on_bribe(player, *accept)?,
        CommandKind::UseJailCard => exec.use_jail_card(player)?,
        CommandKind::EndTurn => exec.end_turn()?,
        CommandKind::Resign => exec.resign(player),
    }

    // A player can go bankrupt during their own turn (jail fine, card debt).
    // The turn must then move on without requiring further input from them -
    // but not while a sealed-bid window is still open (ADR-0018): other
    // seats may still need to bid, and advancing here would wipe out an
    // in-progress window out from under them. This fires correctly on the
    // next command once resolution moves `turn` off `BlindAuction`.
    if matches!(exec.st.phase, GamePhase::Active)
        && exec.st.players[exec.st.current].bankrupt
        && !matches!(
            exec.st.turn,
            TurnPhase::BlindAuction { .. } | TurnPhase::BribeVote { .. }
        )
    {
        exec.advance_turn();
    }

    // Instant win by controlling enough full groups (ADR-0013), checked
    // after any holdings-changing command.
    exec.check_group_win();
    // Victory-point race and doom clock (ADR-0020); order matters - a
    // Build that both crosses the target and empties the pool is a
    // points win, not a pool-exhaustion win.
    exec.check_points_win();
    exec.check_pool_exhaustion_win();

    Ok((exec.st, exec.ev))
}

struct Exec<'e> {
    content: &'e GameContent,
    strategies: Strategies<'e>,
    st: GameState,
    ev: Vec<Event>,
}

mod auction;
mod cash;
mod estate;
mod jail;
mod landing;
mod movement;
mod trade;
mod turn;
