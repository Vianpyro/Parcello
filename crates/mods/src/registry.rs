//! Mutable registries populated by `ModPlugin::on_load`, then frozen into a
//! validated `GameContent` (Registry pattern).
//!
//! Merge rules (architecture section 7.1.1):
//! - collections (tiles, cards): additive by key; duplicate keys replace
//!   in place, last-loaded-wins, conflict logged at WARN;
//! - scalar rule parameters: last-loaded-wins, conflict logged at WARN.

use std::collections::BTreeMap;

use parcello_engine::{CardDef, GameContent, MarketEventDef, RuleParams, TileDef};
use tracing::warn;

use crate::ModError;

#[derive(Debug, Default)]
pub struct RegistryBuilder {
    board: Vec<TileDef>,
    chance: Vec<CardDef>,
    community: Vec<CardDef>,
    rules: BTreeMap<String, i64>,
    market_events: Vec<MarketEventDef>,
    forecast_gap_turns: Option<u32>,
}

impl RegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a tile by id. Replacement keeps the original board
    /// position so mods can retheme tiles without reshuffling the track.
    pub fn upsert_tile(&mut self, mod_id: &str, tile: TileDef) {
        match self.board.iter_mut().find(|t| t.id == tile.id) {
            Some(existing) => {
                warn!(mod_id, tile = %tile.id, "tile override (last-loaded-wins)");
                *existing = tile;
            }
            None => self.board.push(tile),
        }
    }

    pub fn upsert_chance(&mut self, mod_id: &str, card: CardDef) {
        upsert_card(&mut self.chance, mod_id, "chance", card);
    }

    pub fn upsert_community(&mut self, mod_id: &str, card: CardDef) {
        upsert_card(&mut self.community, mod_id, "community", card);
    }

    pub fn set_rule(&mut self, mod_id: &str, key: &str, value: i64) {
        if let Some(old) = self.rules.insert(key.to_string(), value) {
            warn!(
                mod_id,
                key,
                old,
                new = value,
                "rule override (last-loaded-wins)"
            );
        }
    }

    /// Insert or replace a market event by id (ADR-0021), same merge rule
    /// as tiles/cards.
    pub fn upsert_market_event(&mut self, mod_id: &str, event: MarketEventDef) {
        match self.market_events.iter_mut().find(|e| e.id == event.id) {
            Some(existing) => {
                warn!(mod_id, event = %event.id, "market event override (last-loaded-wins)");
                *existing = event;
            }
            None => self.market_events.push(event),
        }
    }

    /// Sets the gap between scheduled market events (ADR-0021), same
    /// last-loaded-wins/WARN-on-conflict rule as `set_rule` - except a mod
    /// that simply doesn't mention `gap_turns` never counts as a conflict
    /// (that's the whole reason this takes `Option<u32>` instead of `u32`).
    pub fn set_forecast_gap_turns(&mut self, mod_id: &str, value: u32) {
        if let Some(old) = self.forecast_gap_turns.replace(value) {
            warn!(
                mod_id,
                old,
                new = value,
                "forecast gap_turns override (last-loaded-wins)"
            );
        }
    }

    /// Freeze into validated content. Unknown rule keys are ignored with a
    /// WARN so future keys do not hard-break older game versions.
    pub fn build(self) -> Result<GameContent, ModError> {
        let mut rules = RuleParams::default();
        for (key, value) in &self.rules {
            match key.as_str() {
                "starting_balance" => rules.starting_balance = *value,
                "go_salary" => rules.go_salary = *value,
                "jail_fine" => rules.jail_fine = *value,
                "max_houses_per_property" => {
                    rules.max_houses_per_property = (*value).clamp(0, u8::MAX as i64) as u8;
                }
                "bankruptcy_threshold" => rules.bankruptcy_threshold = *value,
                "expropriation" => rules.expropriation = *value,
                "rent_boost" => rules.rent_boost = *value,
                "win_full_groups" => rules.win_full_groups = *value,
                "subsidiary_pool_factor" => rules.subsidiary_pool_factor = *value,
                "conglomerate_pool_factor" => rules.conglomerate_pool_factor = *value,
                _ => warn!(key, value, "unknown rule key ignored"),
            }
        }
        let content = GameContent {
            board: self.board,
            chance: self.chance,
            community: self.community,
            rules,
            market_events: self.market_events,
            forecast_gap_turns: self.forecast_gap_turns.unwrap_or(0),
        };
        content.validate()?;
        Ok(content)
    }
}

fn upsert_card(deck: &mut Vec<CardDef>, mod_id: &str, deck_name: &str, card: CardDef) {
    match deck.iter_mut().find(|c| c.id == card.id) {
        Some(existing) => {
            warn!(mod_id, deck = deck_name, card = %card.id, "card override (last-loaded-wins)");
            *existing = card;
        }
        None => deck.push(card),
    }
}
