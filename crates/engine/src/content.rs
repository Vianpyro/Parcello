//! Static game content, resolved once at room creation by the mod layer.
//!
//! The engine never hardcodes game data: every tile, card, and rule parameter
//! is resolved through this structure (Registry pattern). Immutable for the
//! lifetime of a room.

use serde::{Deserialize, Serialize};

use crate::error::ContentError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameContent {
    /// Board tiles in play order. Invariant: `board[0]` is the Go tile.
    pub board: Vec<TileDef>,
    pub chance: Vec<CardDef>,
    pub community: Vec<CardDef>,
    pub rules: RuleParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TileDef {
    /// Stable string key. Mods replace tiles by id (last-loaded-wins).
    pub id: String,
    pub name: String,
    pub kind: TileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TileKind {
    Go,
    Property(PropertyDef),
    Chance,
    Community,
    Tax { amount: i64 },
    Jail,
    GoToJail,
    FreeParking,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyDef {
    /// Color group key. Owning a full group doubles unimproved rent and
    /// unlocks building (StandardRent / build rules).
    pub group: String,
    pub price: i64,
    /// Ignored (and building rejected) unless `rent_model` is `Houses`.
    pub house_cost: i64,
    /// Meaning depends on `rent_model`:
    /// - Houses: rent by house count, `rents[0]` unimproved .. `rents[5]` hotel;
    /// - GroupScaled: `rents[n-1]` where n = tiles of the group owned;
    /// - DiceScaled: dice total times `rents[n-1]`.
    pub rents: [i64; 6],
    #[serde(default)]
    pub rent_model: RentModel,
}

/// How rent is computed (stations and utilities use the scaled models).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RentModel {
    #[default]
    Houses,
    GroupScaled,
    DiceScaled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardDef {
    pub id: String,
    pub text: String,
    pub effect: CardEffect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CardEffect {
    /// Positive: bank pays the player. Negative: player pays the bank.
    Money { amount: i64 },
    MoveTo { tile: String, collect_go: bool },
    /// Relative move; negative steps move backward (no Go salary backward).
    MoveBy { steps: i8 },
    GoToJail,
    CollectFromEach { amount: i64 },
    PayEach { amount: i64 },
}

/// Named rule parameters, resolved from `RuleRegistry` keys (V1 hook points).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuleParams {
    pub starting_balance: i64,
    pub go_salary: i64,
    pub jail_fine: i64,
    /// Build limit per property; house level 5 renders as a hotel.
    pub max_houses_per_property: u8,
    /// Cash floor after liquidation below which a player goes bankrupt.
    pub bankruptcy_threshold: i64,
    /// When true (default), declining a purchase starts an auction among all
    /// solvent players instead of leaving the tile with the bank.
    pub auction_on_decline: bool,
}

impl Default for RuleParams {
    fn default() -> Self {
        Self {
            starting_balance: 1500,
            go_salary: 200,
            jail_fine: 50,
            max_houses_per_property: 5,
            bankruptcy_threshold: 0,
            auction_on_decline: true,
        }
    }
}

impl GameContent {
    /// Structural invariants required by `Engine::apply`. Checked once at
    /// room creation so the hot path can index without re-validating.
    pub fn validate(&self) -> Result<(), ContentError> {
        if self.board.is_empty() {
            return Err(ContentError::EmptyBoard);
        }
        if !matches!(self.board[0].kind, TileKind::Go) {
            return Err(ContentError::FirstTileNotGo);
        }
        let jail_count = self
            .board
            .iter()
            .filter(|t| matches!(t.kind, TileKind::Jail))
            .count();
        if jail_count != 1 {
            return Err(ContentError::JailTileCount(jail_count));
        }
        let has_go_to_jail = self
            .board
            .iter()
            .any(|t| matches!(t.kind, TileKind::GoToJail));
        let jail_card = self
            .chance
            .iter()
            .chain(self.community.iter())
            .any(|c| matches!(c.effect, CardEffect::GoToJail));
        if (has_go_to_jail || jail_card) && jail_count == 0 {
            return Err(ContentError::JailTileCount(0));
        }
        if self
            .board
            .iter()
            .any(|t| matches!(t.kind, TileKind::Chance))
            && self.chance.is_empty()
        {
            return Err(ContentError::EmptyDeck("chance"));
        }
        if self
            .board
            .iter()
            .any(|t| matches!(t.kind, TileKind::Community))
            && self.community.is_empty()
        {
            return Err(ContentError::EmptyDeck("community"));
        }
        let mut seen = std::collections::HashSet::new();
        for t in &self.board {
            if !seen.insert(t.id.as_str()) {
                return Err(ContentError::DuplicateTileId(t.id.clone()));
            }
            if let TileKind::Property(p) = &t.kind {
                let bad_house_cost =
                    matches!(p.rent_model, RentModel::Houses) && p.house_cost <= 0;
                if p.price <= 0 || bad_house_cost {
                    return Err(ContentError::InvalidProperty(t.id.clone()));
                }
            }
        }
        for c in self.chance.iter().chain(self.community.iter()) {
            if let CardEffect::MoveTo { tile, .. } = &c.effect {
                if !self.board.iter().any(|t| &t.id == tile) {
                    return Err(ContentError::CardTargetsUnknownTile {
                        card: c.id.clone(),
                        tile: tile.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn tile_index(&self, id: &str) -> Option<usize> {
        self.board.iter().position(|t| t.id == id)
    }

    /// Position of the (unique, validated) jail tile.
    pub fn jail_position(&self) -> usize {
        self.board
            .iter()
            .position(|t| matches!(t.kind, TileKind::Jail))
            .expect("validated content has exactly one jail tile")
    }

    pub fn property(&self, tile: usize) -> Option<&PropertyDef> {
        match &self.board.get(tile)?.kind {
            TileKind::Property(p) => Some(p),
            _ => None,
        }
    }

    /// Indices of every property belonging to `group`.
    pub fn group_tiles(&self, group: &str) -> Vec<usize> {
        self.board
            .iter()
            .enumerate()
            .filter_map(|(i, t)| match &t.kind {
                TileKind::Property(p) if p.group == group => Some(i),
                _ => None,
            })
            .collect()
    }
}
