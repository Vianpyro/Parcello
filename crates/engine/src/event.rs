//! Typed events emitted after each state transition (Observer pattern).
//!
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TurnStarted {
        player: usize,
    },
    /// A movement card was played (ADR-0017), replacing dice: `value`
    /// tiles forward, then the landing resolves normally.
    MovementCardPlayed {
        player: usize,
        value: u8,
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
    /// A sealed-bid window opened (ADR-0018): landed on an unowned
    /// property. `discoverer` gets an implicit floor bid of `floor` (list
    /// price) if they stay silent and can afford it.
    BlindAuctionOpened {
        tile: usize,
        discoverer: usize,
        floor: i64,
    },
    /// A seat submitted its bid; the amount stays hidden until resolution
    /// (ADR-0018 secrecy - `ClientView::for_seat` masks it too).
    BlindBidSubmitted {
        player: usize,
    },
    /// Every living seat has bid: the window resolved. `winner = None`
    /// means every effective bid was zero and the tile stays unsold.
    /// `bids` reveals every seat's raw submission (`0` where unset).
    BlindAuctionResolved {
        tile: usize,
        discoverer: usize,
        winner: Option<usize>,
        amount: i64,
        bids: Vec<i64>,
    },
    /// The discoverer won its own auction and the bank rebates part of what
    /// it just paid (ADR-0018 amended). Always follows the
    /// `BlindAuctionResolved` that charged the full price: the rebate is a
    /// second, visible motion rather than a quieter number on the first.
    DiscovererRefunded {
        player: usize,
        tile: usize,
        amount: i64,
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
    /// Improved tiles liquidate on seizure (ADR-0022): `liquidated` levels
    /// were stripped, paying `from` `liquidation_refund` on top of `cost`'s
    /// usual compensation; both are 0 for a bare tile.
    Expropriated {
        player: usize,
        from: usize,
        tile: usize,
        cost: i64,
        liquidated: u8,
        liquidation_refund: i64,
    },
    /// A tile's rent was boosted one step (ADR-0012).
    RentBoosted {
        player: usize,
        tile: usize,
        boosts: u8,
        cost: i64,
    },
    /// The first rent collected at a boosted rate consumed the whole boost
    /// (ADR-0012, amended 2026-07: boosts are one-shot traps). Follows the
    /// `RentPaid` that sprang it.
    RentBoostConsumed {
        tile: usize,
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
    /// `from` is the tile the player stood on when sent (the Go To Jail
    /// corner or the tile where the card was drawn) - clients animate the
    /// jail hop from it instead of remembering last known positions
    /// themselves (ADR-0028).
    WentToJail {
        player: usize,
        #[serde(default)]
        from: usize,
    },
    /// A get-out-of-jail-free card entered the player's hand.
    JailCardReceived {
        player: usize,
    },
    /// A held card was spent to leave jail (ADR-0024: unconditional,
    /// immediate).
    JailCardUsed {
        player: usize,
    },
    LeftJail {
        player: usize,
    },
    /// Legal Route chosen (ADR-0024): `order` is the full locked,
    /// public plan (the hand was discarded to build it); its first value
    /// plays in the same command that emits this event.
    LegalRouteChosen {
        player: usize,
        order: Vec<u8>,
    },
    /// Corruption: a jailed player offered `amount` to the table instead
    /// of moving, opening a 5s vote among living opponents.
    BribeOffered {
        player: usize,
        amount: i64,
    },
    /// A vote was cast; the accept/reject choice stays hidden until
    /// `BribeResolved` (ADR-0024: "individual votes stay secret").
    BribeVoteCast {
        player: usize,
    },
    /// The bribe vote closed. On success, `amount` split (floor
    /// division; the remainder stays with the briber) among the `total`
    /// living opponents and the briber leaves jail with a live hand; on
    /// failure no cash moves and the briber's turn degrades to
    /// `AwaitEnd`. `accepts`/`total` are the only tally ever revealed.
    BribeResolved {
        briber: usize,
        amount: i64,
        succeeded: bool,
        accepts: usize,
        total: usize,
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
    /// Instant win by controlling `groups` complete colour groups (ADR-0013).
    WonByGroups {
        winner: usize,
        groups: u8,
    },
    /// Reached the victory-point target (ADR-0020, `rules.win_victory_points`).
    WonByPoints {
        player: usize,
        points: i64,
    },
    /// The shared conglomerate pool ran dry with nobody at the target yet
    /// (ADR-0019/0020 "doom clock"): highest score wins, ties by net worth
    /// then the lowest seat.
    WonByPoolExhaustion {
        winner: usize,
    },
    /// A scheduled market event fired (ADR-0021). `WealthTax` never gets a
    /// matching `MarketEventExpired` - it resolves in the same instant.
    MarketEventActivated {
        event_id: String,
        effect: crate::content::MarketEffect,
        magnitude_pct: i64,
        duration_turns: u32,
    },
    MarketEventExpired {
        event_id: String,
    },
    /// The strictly-richest surviving player banked the permanent per-round
    /// victory-point bonus (ADR-0020) - announced so the one non-reversible
    /// VP source is visible to the table.
    RoundBonusAwarded {
        player: usize,
        points: i64,
    },
    /// The Exposition corner (ADR-0026) put a property in the spotlight.
    /// Landing again while one is active first emits `SpotlightEnded` for
    /// the bumped tile (even if the redraw lands on the same one), then
    /// this. `duration_turns <= 0` means permanent: only the next
    /// Exposition landing replaces it.
    SpotlightStarted {
        tile: usize,
        rent_pct: i64,
        duration_turns: i64,
    },
    SpotlightEnded {
        tile: usize,
    },
}
