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
    /// Play one card from the hand (ADR-0017), moving that many tiles and
    /// resolving the landing. While `jail_route` is `Some`, only its front
    /// value is accepted - a jailed player uses `ChooseLegalRoute`,
    /// `OfferBribe`, or `UseJailCard` instead of this directly.
    PlayMovementCard {
        value: u8,
    },
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
    /// Sealed-bid auction (ADR-0018): submit one bid for the open
    /// `BlindAuction` window. `0` abstains. The discoverer's implicit floor
    /// is list price; an explicit non-zero discoverer bid must meet it.
    SubmitBlindBid {
        amount: i64,
    },
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
    /// Legal Route exit (ADR-0024): `order` must be exactly a permutation
    /// of the full `velocity_min..=velocity_max` hand. Discards the
    /// current hand, leaves jail immediately, locks the route (public via
    /// `PlayerView.jail_route`), and plays its first card this same
    /// command. Rent income freezes on this player's tiles until the
    /// route empties.
    ChooseLegalRoute {
        order: Vec<u8>,
    },
    /// Corruption exit (ADR-0024): offers `amount` (1..=cash) to the
    /// table instead of moving, opening a 5s simultaneous vote among
    /// living opponents (`VoteOnBribe`).
    OfferBribe {
        amount: i64,
    },
    /// Vote on an open bribe (ADR-0024); the briber never votes on their
    /// own offer. Individual votes stay secret until resolution.
    VoteOnBribe {
        accept: bool,
    },
    /// Spend a held get-out-of-jail-free card: immediate unconditional
    /// exit, then a normal `PlayMovementCard` the same turn.
    UseJailCard,
    EndTurn,
    /// Forfeit: assets return to the bank. Allowed at any time.
    Resign,
}
