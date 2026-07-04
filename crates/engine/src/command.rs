//! Command pattern: every player action is a serializable value that flows
//! through the single `Engine::apply` pipeline. The full command log is the
//! audit trail and the replay source.

use serde::{Deserialize, Serialize};

use crate::state::PlayerId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerCommand {
    pub player: PlayerId,
    #[serde(flatten)]
    pub kind: CommandKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandKind {
    /// Roll and move; while jailed, attempt a doubles escape instead.
    Roll,
    /// Accept the pending purchase offer.
    Buy,
    /// Decline the pending purchase offer.
    Decline,
    /// Build one house on an owned tile (full group required).
    Build {
        tile: String,
    },
    /// Offer a trade to another player. Empty sides default to nothing;
    /// at least one side must be non-empty. Allowed any time outside auctions.
    ProposeTrade {
        to: String,
        #[serde(default)]
        give_cash: i64,
        #[serde(default)]
        give_tiles: Vec<String>,
        #[serde(default)]
        receive_cash: i64,
        #[serde(default)]
        receive_tiles: Vec<String>,
    },
    /// Recipient accepts an open offer (re-validated at this moment).
    AcceptTrade {
        trade: u32,
    },
    /// Recipient declines an open offer.
    DeclineTrade {
        trade: u32,
    },
    /// Proposer withdraws their own offer.
    CancelTrade {
        trade: u32,
    },
    /// Auction: bid strictly above the current high bid (cash-limited).
    Bid {
        amount: i64,
    },
    /// Auction: withdraw. The tile stays unsold if everyone passes.
    Pass,
    /// Sell one house back to the bank for half its cost (even-sell rule).
    SellHouse {
        tile: String,
    },
    /// Seize a rival's unimproved property for a premium (ADR-0011); the
    /// former owner is compensated. Enabled by `rules.expropriation`.
    Expropriate {
        tile: String,
    },
    /// Raise an owned tile's rent one step for a fee (ADR-0012). Enabled by
    /// `rules.rent_boost`.
    BoostRent {
        tile: String,
    },
    /// Mortgage an owned tile for half its price (group must be house-free).
    Mortgage {
        tile: String,
    },
    /// Lift a mortgage for the mortgage value plus 10% interest.
    Unmortgage {
        tile: String,
    },
    /// Pay the fine to leave jail, then roll normally.
    PayJailFine,
    /// Spend a held get-out-of-jail-free card, then roll normally.
    UseJailCard,
    EndTurn,
    /// Forfeit: assets return to the bank. Allowed at any time.
    Resign,
}
