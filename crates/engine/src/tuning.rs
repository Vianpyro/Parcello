//! Fixed engine-policy numbers, grouped so a balance pass reads one file
//! instead of hunting literals (2026-07 refactor). Everything here is
//! deliberate game policy that is NOT mod-configurable - the mod-facing
//! knobs live in `RuleParams`. Promote a value there (with an ADR) rather
//! than tweaking it here if mods ever need to override it.

/// Card chains ("advance to X" landing on another card tile) are bounded
/// to keep resolution finite regardless of mod content.
pub const MAX_CARD_CHAIN_DEPTH: u8 = 4;

/// Anti-spam cap on standing trade offers per proposer (ADR-0007).
pub const MAX_OPEN_TRADES_PER_PLAYER: usize = 4;

/// Rent-boost level cap and per-step rent increase (ADR-0012).
pub const MAX_RENT_BOOSTS: u8 = 3;
pub const RENT_BOOST_STEP_PCT: i64 = 50;

/// A discoverer that wins its own auction pays in full, then the bank hands
/// back this percent of what it paid (ADR-0018 amended 2026-07) - the reward
/// for having landed there, and its only one. Paid back rather than discounted
/// so the table sees both halves: the full price leaving, then the rebate
/// arriving.
pub const DISCOVERER_REFUND_PCT: i64 = 10;

/// A mortgage advances this percent of list price - and it is also what a
/// mortgaged tile is worth (net worth), sells back at, and buys out at on
/// landing (ADR-0022 amended).
pub const MORTGAGE_VALUE_PCT: i64 = 50;

/// Redeeming a mortgage costs the principal plus this percent, floored.
pub const MORTGAGE_INTEREST_PCT: i64 = 10;

/// Selling or liquidating a building refunds this percent of build cost.
pub const HOUSE_REFUND_PCT: i64 = 50;

/// Victory-point weights (ADR-0020): the race to `rules.win_victory_points`.
pub const VP_PER_FULL_GROUP: i64 = 3;
pub const VP_PER_CONGLOMERATE: i64 = 2;
pub const VP_PER_GROUP_SCALED: i64 = 1;

/// Permanent VP banked by the round's strictly-richest survivor (ADR-0020).
pub const ROUND_BONUS_VP: i64 = 2;

/// Public market forecast queue length (ADR-0021).
pub const FORECAST_QUEUE_LEN: usize = 3;

/// `Spotlight::expires_at_turn` sentinel for a permanent spotlight
/// (ADR-0026 amended: `spotlight_duration_turns <= 0`). The Flutter client
/// mirrors this check when rendering "until replaced".
pub const SPOTLIGHT_NO_EXPIRY: u32 = u32::MAX;
