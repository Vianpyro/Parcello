//! Static game content, resolved once at room creation by the mod layer.
//!
//! The engine never hardcodes game data: every tile, card, and rule parameter
//! is resolved through this structure (Registry pattern). Immutable for the
//! lifetime of a room.

use serde::{Deserialize, Serialize};

use crate::error::ContentError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameContent {
    /// Board tiles in play order. Invariant: `board[0]` is the Go tile.
    pub board: Vec<TileDef>,
    pub chance: Vec<CardDef>,
    pub community: Vec<CardDef>,
    pub rules: RuleParams,
    /// Pool of market events the public forecast draws from (ADR-0021); an
    /// empty pool leaves the forecast fully inert.
    #[serde(default)]
    pub market_events: Vec<MarketEventDef>,
    /// Turns between one scheduled market event and the next; meaningless
    /// (and unused) while `market_events` is empty.
    #[serde(default)]
    pub forecast_gap_turns: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileDef {
    /// Stable string key. Mods replace tiles by id (last-loaded-wins).
    pub id: String,
    pub name: String,
    pub kind: TileKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TileKind {
    Go,
    Property(PropertyDef),
    Chance,
    Community,
    Tax {
        amount: i64,
    },
    Jail,
    GoToJail,
    FreeParking,
    /// The Exposition corner (ADR-0026): landing here puts a random
    /// property tile in the spotlight (boosted rent for a while).
    Spotlight,
    /// Progressive audit (ADR-0029): the lander pays a seeded-random
    /// percent of their net worth in `min_pct..=max_pct`, weighted so the
    /// heavier brackets are the rarer ones.
    NetWorthTax {
        min_pct: u8,
        max_pct: u8,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertyDef {
    /// Color group key. Owning a full group doubles unimproved rent and
    /// unlocks building (`StandardRent` / build rules).
    pub group: String,
    pub price: i64,
    /// Ignored (and building rejected) unless `rent_model` is `Houses`.
    pub house_cost: i64,
    /// Meaning depends on `rent_model`:
    /// - `Houses`: rent by house count, `rents[0]` unimproved .. `rents[5]` hotel;
    /// - `GroupScaled`: `rents[n-1]` where n = tiles of the group owned.
    pub rents: [i64; 6],
    #[serde(default)]
    pub rent_model: RentModel,
}

/// How rent is computed (stations use the scaled model).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RentModel {
    #[default]
    Houses,
    GroupScaled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CardDef {
    pub id: String,
    pub text: String,
    pub effect: CardEffect,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CardEffect {
    /// Positive: bank pays the player. Negative: player pays the bank.
    Money {
        amount: i64,
    },
    MoveTo {
        tile: String,
        collect_go: bool,
    },
    /// Relative move; negative steps move backward (no Go salary backward).
    MoveBy {
        steps: i8,
    },
    GoToJail,
    /// Holdable: increments the drawer's `jail_cards` count instead of
    /// resolving immediately. Spent via `CommandKind::UseJailCard`.
    GetOutOfJail,
    CollectFromEach {
        amount: i64,
    },
    PayEach {
        amount: i64,
    },
}

/// A scheduled market event definition (ADR-0021). Calibration
/// (`magnitude_pct`, `duration_turns`) is data-only - mods edit TOML, never
/// the engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketEventDef {
    /// Stable string key. Mods replace events by id (last-loaded-wins).
    pub id: String,
    pub name: String,
    pub effect: MarketEffect,
    /// Percent applied to the affected amount; negative for a
    /// discount/cut, positive for a surcharge. Meaning depends on `effect`.
    pub magnitude_pct: i64,
    /// How many turns the effect stays active once it fires; `0` marks a
    /// one-shot effect (only meaningful for `WealthTax` today).
    pub duration_turns: u32,
}

/// What a market event does while active (ADR-0021).
///
/// Unit variants only -
/// unlike `CardEffect`, none carry per-variant data (the shared
/// `magnitude_pct`/`duration_turns` on `MarketEventDef` cover all three),
/// so this serializes as a bare string (`effect = "rent_multiplier"`),
/// friendlier for hand-written TOML than `CardEffect`'s tagged shape.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MarketEffect {
    /// Scales rent in `resolve_landing`, composing with the ADR-0012 boost.
    RentMultiplier,
    /// Scales takeover cost (ADR-0022) - and, once it exists, sealed-bid
    /// settlement prices (ADR-0018).
    AcquisitionMultiplier,
    /// One-shot: every alive player pays `net_worth * magnitude_pct / 100`
    /// through the normal charge/bankruptcy machinery.
    WealthTax,
}

/// Named rule parameters, resolved from `RuleRegistry` keys (V1 hook points).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleParams {
    pub starting_balance: i64,
    pub go_salary: i64,
    /// Build limit per property; house level 5 renders as a hotel.
    pub max_houses_per_property: u8,
    /// Cash floor after liquidation below which a player goes bankrupt.
    pub bankruptcy_threshold: i64,
    /// Expropriation cost as a percent of the target's price (ADR-0011);
    /// 0 disables the mechanic. E.g. 200 = pay 2x price to seize a rival's
    /// unimproved property; the former owner is compensated its price.
    #[serde(default)]
    pub expropriation: i64,
    /// Rent-boost cost as a percent of the tile's price, per boost
    /// (ADR-0012); 0 disables. Each boost raises that tile's rent by a
    /// fixed step, capped.
    #[serde(default)]
    pub rent_boost: i64,
    /// Instant win by owning this many complete colour groups (ADR-0013);
    /// 0 disables. "Control all cities of N colours."
    #[serde(default)]
    pub win_full_groups: i64,
    /// Race to this many victory points (ADR-0020, `GameState::victory_points`);
    /// 0 disables. Also gates the round bonus and the conglomerate-pool
    /// "doom clock" - both are meaningless without an active points race.
    #[serde(default)]
    pub win_victory_points: i64,
    /// Shared subsidiary-pool sizing factor (ADR-0019): the table-wide
    /// stock of levels 1..max-1 is `round(factor * sqrt(players))` at game
    /// start; 0 disables pooling (unlimited stock, like today).
    #[serde(default)]
    pub subsidiary_pool_factor: i64,
    /// Shared conglomerate-pool sizing factor (ADR-0019): same formula,
    /// for the top build level; 0 disables pooling.
    #[serde(default)]
    pub conglomerate_pool_factor: i64,
    /// Rent bonus percent while a property is in the Exposition corner's
    /// spotlight (ADR-0026); 0 disables (a mod with no `Spotlight` tile on
    /// its board never triggers this regardless).
    #[serde(default)]
    pub spotlight_rent_pct: i64,
    /// How many turns a spotlight stays active once drawn (ADR-0026);
    /// `<= 0` = permanent, replaced only by the next Exposition landing
    /// (2026-07 amendment - the mechanic's off switch is
    /// `spotlight_rent_pct = 0`).
    #[serde(default)]
    pub spotlight_duration_turns: i64,
    /// Velocity deck (ADR-0017): the movement hand is every integer in
    /// `velocity_min..=velocity_max`, dealt full at game start and
    /// refilled the instant it empties. Unlike every other scalar here,
    /// `0` is not a valid "off" sentinel - an empty/degenerate hand would
    /// break movement outright, so these get non-zero serde defaults and
    /// `GameContent::validate` rejects an invalid range.
    #[serde(default = "default_velocity_min")]
    pub velocity_min: u8,
    #[serde(default = "default_velocity_max")]
    pub velocity_max: u8,
}

const fn default_velocity_min() -> u8 {
    1
}

const fn default_velocity_max() -> u8 {
    5
}

impl Default for RuleParams {
    fn default() -> Self {
        Self {
            starting_balance: 1500,
            go_salary: 200,
            max_houses_per_property: 5,
            bankruptcy_threshold: 0,
            expropriation: 0,
            rent_boost: 0,
            win_full_groups: 0,
            win_victory_points: 0,
            subsidiary_pool_factor: 0,
            conglomerate_pool_factor: 0,
            spotlight_rent_pct: 0,
            spotlight_duration_turns: 0,
            velocity_min: default_velocity_min(),
            velocity_max: default_velocity_max(),
        }
    }
}

impl GameContent {
    /// Structural invariants required by `Engine::apply`. Checked once at
    /// room creation so the hot path can index without re-validating.
    ///
    /// # Errors
    /// Returns the first violated invariant (empty board, missing/duplicate
    /// tiles, invalid property or velocity numbers, dangling card targets).
    pub fn validate(&self) -> Result<(), ContentError> {
        if self.board.is_empty() {
            return Err(ContentError::EmptyBoard);
        }
        if !matches!(self.board[0].kind, TileKind::Go) {
            return Err(ContentError::FirstTileNotGo);
        }
        if self.rules.velocity_min < 1 || self.rules.velocity_max <= self.rules.velocity_min {
            return Err(ContentError::InvalidVelocityRange);
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
                let bad_house_cost = matches!(p.rent_model, RentModel::Houses) && p.house_cost <= 0;
                if p.price <= 0 || bad_house_cost {
                    return Err(ContentError::InvalidProperty(t.id.clone()));
                }
            }
            if let TileKind::NetWorthTax { min_pct, max_pct } = t.kind
                && (min_pct < 1 || min_pct > max_pct || max_pct > 100)
            {
                return Err(ContentError::InvalidNetWorthTax(t.id.clone()));
            }
        }
        for c in self.chance.iter().chain(self.community.iter()) {
            if let CardEffect::MoveTo { tile, .. } = &c.effect
                && !self.board.iter().any(|t| &t.id == tile)
            {
                return Err(ContentError::CardTargetsUnknownTile {
                    card: c.id.clone(),
                    tile: tile.clone(),
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn tile_index(&self, id: &str) -> Option<usize> {
        self.board.iter().position(|t| t.id == id)
    }

    /// Position of the (unique, validated) jail tile.
    ///
    /// # Panics
    /// If called on content that never passed [`GameContent::validate`]
    /// (which guarantees exactly one jail tile).
    #[must_use]
    pub fn jail_position(&self) -> usize {
        self.board
            .iter()
            .position(|t| matches!(t.kind, TileKind::Jail))
            .expect("validated content has exactly one jail tile")
    }

    #[must_use]
    pub fn property(&self, tile: usize) -> Option<&PropertyDef> {
        match &self.board.get(tile)?.kind {
            TileKind::Property(p) => Some(p),
            _ => None,
        }
    }

    /// Indices of every property belonging to `group`. Lazy on purpose:
    /// rent, build, and VP checks walk groups on every landing, and most
    /// callers only need `all`/`any`/`filter` - no Vec required.
    pub fn group_tiles<'content>(
        &'content self,
        group: &'content str,
    ) -> impl Iterator<Item = usize> + 'content {
        self.board
            .iter()
            .enumerate()
            .filter_map(move |(i, t)| match &t.kind {
                TileKind::Property(p) if p.group == group => Some(i),
                _ => None,
            })
    }

    /// Looks up a market event definition by id (ADR-0021).
    #[must_use]
    pub fn market_event(&self, id: &str) -> Option<&MarketEventDef> {
        self.market_events.iter().find(|e| e.id == id)
    }

    /// Indices of every property tile, any group - the Exposition corner's
    /// random draw (ADR-0026).
    #[must_use]
    pub fn property_tiles(&self) -> Vec<usize> {
        self.board
            .iter()
            .enumerate()
            .filter_map(|(i, t)| matches!(t.kind, TileKind::Property(_)).then_some(i))
            .collect()
    }
}
