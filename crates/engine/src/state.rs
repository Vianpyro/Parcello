//! Authoritative game state. Fully serializable: a `(GameState, command log)`
//! pair is sufficient to replay or audit a game.
//!
//! Hidden information (PRNG seed, deck order) lives here and must never reach
//! clients; see `view::ClientView` for the public projection.

use serde::{Deserialize, Serialize};

use crate::content::GameContent;
use crate::content::RuleParams;
use crate::rng;

/// Global player identity issued by the identity service ("provider:sub")
/// or a guest id in insecure mode.
pub type PlayerId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameState {
    pub phase: GamePhase,
    /// Seating order, fixed at game start. Bankrupt players stay in the
    /// vector (flagged) so indices remain stable in events and views.
    pub players: Vec<Player>,
    /// Index into `players` of the acting player.
    pub current: usize,
    pub turn: TurnPhase,
    /// Dynamic tile state, parallel to `GameContent::board`.
    pub tiles: Vec<TileState>,
    pub chance_deck: DeckState,
    pub community_deck: DeckState,
    /// SplitMix64 PRNG state. Part of the state on purpose: replay-safe.
    /// Never expose to clients (dice would become predictable).
    pub rng: u64,
    /// Completed turn transitions; used for stats/history.
    pub turn_count: u32,
    /// Open trade offers, visible to all players. Asset validity is
    /// re-checked at acceptance time; stale offers reject without mutating.
    #[serde(default)]
    pub pending_trades: Vec<TradeOffer>,
    #[serde(default)]
    pub trade_seq: u32,
}

/// A standing offer: `from` gives `give_*` and receives `receive_*`.
/// Tiles transfer with their mortgage status as-is (same rule as
/// bankruptcy); groups must be house-free on both sides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TradeOffer {
    pub id: u32,
    pub from: usize,
    pub to: usize,
    pub give_cash: i64,
    pub give_tiles: Vec<usize>,
    pub receive_cash: i64,
    pub receive_tiles: Vec<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GamePhase {
    Active,
    Finished { winner: usize },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TurnPhase {
    /// Waiting for the current player to roll (or pay the jail fine).
    AwaitRoll,
    /// Landed on an unowned property; waiting for buy/decline.
    AwaitBuy { tile: usize },
    /// Declined purchase: round-robin auction. `turn` is the seat expected
    /// to bid or pass; `active` is a bitmask of seats still in the auction.
    /// The high bidder is skipped until someone outbids them.
    Auction {
        tile: usize,
        high_bid: i64,
        high_bidder: Option<usize>,
        turn: usize,
        active: u8,
    },
    /// Movement resolved; building allowed; waiting for end of turn.
    AwaitEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    /// `Some(n)` while jailed; `n` = failed escape rolls so far.
    pub jail_turns: Option<u8>,
    /// Consecutive doubles this turn; 3 sends the player to jail.
    pub doubles_streak: u8,
    /// Get-out-of-jail-free cards held. A count, not card identities: the
    /// decks are immutable cyclic shuffles, so drawn cards never leave the
    /// rotation (documented simplification).
    #[serde(default)]
    pub jail_cards: u8,
    pub bankrupt: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileState {
    /// Index into `players`, if owned.
    pub owner: Option<usize>,
    /// 0..=5, where 5 renders as a hotel.
    pub houses: u8,
    /// Mortgaged tiles collect no rent; ownership still counts for groups.
    #[serde(default)]
    pub mortgaged: bool,
}

/// Cyclic deck: cards are drawn in shuffled order and recycled without
/// reshuffling (deterministic and sufficient for the base game).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeckState {
    pub order: Vec<u16>,
    pub next: usize,
}

impl DeckState {
    fn shuffled(len: usize, rng: &mut u64) -> Self {
        let mut order: Vec<u16> = (0..len as u16).collect();
        rng::shuffle(&mut order, rng);
        Self { order, next: 0 }
    }

    /// Returns the content index of the next card and advances the cursor.
    pub fn draw(&mut self) -> Option<usize> {
        if self.order.is_empty() {
            return None;
        }
        let card = self.order[self.next] as usize;
        self.next = (self.next + 1) % self.order.len();
        Some(card)
    }
}

impl GameState {
    pub(crate) fn new(
        content: &GameContent,
        players: Vec<(PlayerId, String)>,
        seed: u64,
        rules: &RuleParams,
    ) -> Self {
        assert!(players.len() >= 2, "a game requires at least two players");
        let mut rng = seed;
        let chance_deck = DeckState::shuffled(content.chance.len(), &mut rng);
        let community_deck = DeckState::shuffled(content.community.len(), &mut rng);
        Self {
            phase: GamePhase::Active,
            players: players
                .into_iter()
                .map(|(id, name)| Player {
                    id,
                    name,
                    cash: rules.starting_balance,
                    position: 0,
                    jail_turns: None,
                    doubles_streak: 0,
                    jail_cards: 0,
                    bankrupt: false,
                })
                .collect(),
            current: 0,
            turn: TurnPhase::AwaitRoll,
            tiles: vec![TileState::default(); content.board.len()],
            chance_deck,
            community_deck,
            rng,
            turn_count: 0,
            pending_trades: Vec::new(),
            trade_seq: 0,
        }
    }

    pub fn alive_players(&self) -> impl Iterator<Item = usize> + '_ {
        self.players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.bankrupt)
            .map(|(i, _)| i)
    }

    /// True when `player` owns every tile of `group` (monopoly).
    pub fn owns_full_group(&self, content: &GameContent, player: usize, group: &str) -> bool {
        let tiles = content.group_tiles(group);
        !tiles.is_empty() && tiles.iter().all(|&t| self.tiles[t].owner == Some(player))
    }

    /// Total assets of `player`: cash plus property equity. An unmortgaged
    /// property counts its full price; a mortgaged one counts price/2 (the
    /// owner already took the other half in cash, so mortgaging is net-worth
    /// neutral); each house counts its build cost. Clients mirror this to
    /// rank players in timed games - keep the two in step. See
    /// `docs/business-tour-direction.md`.
    pub fn net_worth(&self, content: &GameContent, player: usize) -> i64 {
        let mut worth = self.players[player].cash;
        for (i, tile) in self.tiles.iter().enumerate() {
            if tile.owner != Some(player) {
                continue;
            }
            if let Some(prop) = content.property(i) {
                worth += if tile.mortgaged {
                    prop.price / 2
                } else {
                    prop.price
                };
                worth += tile.houses as i64 * prop.house_cost;
            }
        }
        worth
    }
}
