//! Typed events emitted after each state transition (Observer pattern).
//! This is the primary mod hook surface (V1 passive, V2 reactive) and the
//! animation feed for clients. Player fields are seating indices; clients
//! resolve names through the view.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeckKind {
    Chance,
    Community,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TurnStarted {
        player: usize,
    },
    DiceRolled {
        player: usize,
        d1: u8,
        d2: u8,
    },
    Moved {
        player: usize,
        from: usize,
        to: usize,
        passed_go: bool,
    },
    SalaryPaid {
        player: usize,
        amount: i64,
    },
    PurchaseOffered {
        player: usize,
        tile: usize,
        price: i64,
    },
    PropertyPurchased {
        player: usize,
        tile: usize,
        price: i64,
    },
    PurchaseDeclined {
        player: usize,
        tile: usize,
    },
    /// Offer details live in the view's `pending_trades`.
    TradeProposed {
        trade: u32,
        from: usize,
        to: usize,
    },
    /// Followed by one `PropertyTransferred` per tile that changed hands.
    TradeAccepted {
        trade: u32,
        from: usize,
        to: usize,
    },
    /// `from`/`to` let the session layer route trade lifecycle events to
    /// the two parties only (ADR-0007: offers are private).
    TradeDeclined {
        trade: u32,
        from: usize,
        to: usize,
    },
    TradeCancelled {
        trade: u32,
        from: usize,
        to: usize,
    },
    AuctionStarted {
        tile: usize,
    },
    BidPlaced {
        player: usize,
        tile: usize,
        amount: i64,
    },
    AuctionPassed {
        player: usize,
        tile: usize,
    },
    /// `winner = None` means nobody bid; the tile stays with the bank.
    AuctionEnded {
        tile: usize,
        winner: Option<usize>,
        amount: i64,
    },
    RentPaid {
        from: usize,
        to: usize,
        tile: usize,
        amount: i64,
    },
    TaxPaid {
        player: usize,
        tile: usize,
        amount: i64,
    },
    CardDrawn {
        player: usize,
        deck: DeckKind,
        card: String,
        text: String,
    },
    /// Money moved to/from the bank outside of rent/tax (card effects).
    CashAdjusted {
        player: usize,
        delta: i64,
        reason: String,
    },
    HouseBuilt {
        player: usize,
        tile: usize,
        houses: u8,
        cost: i64,
    },
    HouseSold {
        player: usize,
        tile: usize,
        houses: u8,
        refund: i64,
    },
    /// A rival's property was seized (ADR-0011). `from` is the former
    /// owner, `player` the new one; `cost` is what the seizer paid.
    Expropriated {
        player: usize,
        from: usize,
        tile: usize,
        cost: i64,
    },
    /// A tile's rent was boosted one step (ADR-0012).
    RentBoosted {
        player: usize,
        tile: usize,
        boosts: u8,
        cost: i64,
    },
    PropertyMortgaged {
        player: usize,
        tile: usize,
        value: i64,
    },
    PropertyUnmortgaged {
        player: usize,
        tile: usize,
        cost: i64,
    },
    WentToJail {
        player: usize,
    },
    JailFinePaid {
        player: usize,
        amount: i64,
    },
    /// A get-out-of-jail-free card entered the player's hand.
    JailCardReceived {
        player: usize,
    },
    /// A held card was spent to leave jail (voluntarily or on the third
    /// failed escape roll, where it replaces the forced fine).
    JailCardUsed {
        player: usize,
    },
    LeftJail {
        player: usize,
    },
    /// `to = None` means the property returned to the bank.
    PropertyTransferred {
        tile: usize,
        from: usize,
        to: Option<usize>,
    },
    PlayerBankrupt {
        player: usize,
        creditor: Option<usize>,
    },
    PlayerResigned {
        player: usize,
    },
    GameEnded {
        winner: usize,
    },
    /// A time-boxed game hit its limit; `winner` has the highest net worth
    /// (ties break to the lowest seat). Followed by the game being Finished.
    TimeUp {
        winner: usize,
    },
}
