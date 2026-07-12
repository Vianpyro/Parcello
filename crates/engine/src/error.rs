//! Errors returned by the validate step of the command pipeline and by
//! content validation at room creation.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Command rejection. Rejections never mutate state; the transport layer
/// forwards them to the issuing player only.
#[derive(Debug, Clone, Error, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum CommandError {
    #[error("game is finished")]
    GameFinished,
    #[error("player is not part of this game")]
    UnknownPlayer,
    #[error("player is bankrupt")]
    Bankrupt,
    #[error("not this player's turn")]
    NotYourTurn,
    #[error("command not valid in the current turn phase")]
    WrongPhase,
    #[error("unknown tile id: {tile}")]
    UnknownTile { tile: String },
    #[error("tile is not a property")]
    NotAProperty,
    #[error("property is not owned by this player")]
    NotOwner,
    #[error("full color group required to build")]
    GroupIncomplete,
    #[error("build limit reached on this tile")]
    BuildLimit,
    #[error("this property's rent model does not support houses")]
    NotBuildable,
    #[error("houses must be built and sold evenly across the group")]
    UnevenBuild,
    #[error("no houses to sell on this tile")]
    NoHouses,
    #[error("cannot build while a tile of the group is mortgaged")]
    MortgagedInGroup,
    #[error("tile is already mortgaged")]
    AlreadyMortgaged,
    #[error("tile is not mortgaged")]
    NotMortgaged,
    #[error("sell all houses in the group first")]
    HousesInGroup,
    #[error("no such trade offer")]
    TradeNotFound,
    #[error("only the offer's recipient or proposer may act on it")]
    NotTradeParty,
    #[error("trade offer is malformed or no longer valid")]
    TradeInvalid,
    #[error("too many open offers from this player")]
    TradeLimit,
    #[error("insufficient funds")]
    InsufficientFunds,
    #[error("already submitted a bid for this window")]
    AlreadyBid,
    #[error("the discoverer's bid must be at least the list price")]
    BidBelowFloor,
    #[error("player is not in jail")]
    NotInJail,
    #[error("no get-out-of-jail card held")]
    NoJailCard,
    #[error("expropriation is disabled in this game")]
    ExpropriationDisabled,
    #[error("tile is not a rival's seizable property")]
    NotExpropriable,
    #[error("takeover only applies to the tile you just landed on")]
    NotOnTile,
    #[error("the shared building pool has no stock left")]
    PoolExhausted,
    #[error("rent boosting is disabled in this game")]
    RentBoostDisabled,
    #[error("this tile's rent boost is already maxed out")]
    BoostLimit,
    #[error("that card is not playable right now")]
    CardNotPlayable,
    #[error("the route must be a permutation of the full hand")]
    InvalidRoute,
    #[error("already voted on this bribe")]
    AlreadyVoted,
}

/// Content invariant violations, detected once at room creation.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ContentError {
    #[error("board has no tiles")]
    EmptyBoard,
    #[error("board[0] must be the Go tile")]
    FirstTileNotGo,
    #[error("board must contain exactly one jail tile, found {0}")]
    JailTileCount(usize),
    #[error("{0} deck is empty but a matching tile exists on the board")]
    EmptyDeck(&'static str),
    #[error("duplicate tile id: {0}")]
    DuplicateTileId(String),
    #[error("property {0} has a non-positive price or house cost")]
    InvalidProperty(String),
    #[error("card {card} targets unknown tile {tile}")]
    CardTargetsUnknownTile { card: String, tile: String },
    #[error("velocity_min must be >= 1 and velocity_max must be > velocity_min")]
    InvalidVelocityRange,
    #[error("net-worth tax tile {0} needs 1 <= min_pct <= max_pct <= 100")]
    InvalidNetWorthTax(String),
}
