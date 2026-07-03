//! Raw TOML shapes for mod data files.
//!
//! Tiles use flat optional fields (friendlier to hand-written TOML than an
//! internally-tagged enum) and are converted to `TileDef` with validation.

use parcello_engine::{CardDef, PropertyDef, RentModel, TileDef, TileKind};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::ModError;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PropertiesFile {
    #[serde(default, rename = "tile")]
    pub tiles: Vec<TileRaw>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TileRaw {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    // Property fields (required when type = "property").
    pub group: Option<String>,
    pub price: Option<i64>,
    /// Required for the (default) `houses` rent model, ignored otherwise.
    pub house_cost: Option<i64>,
    pub rents: Option<[i64; 6]>,
    pub rent_model: Option<RentModel>,
    // Tax field (required when type = "tax").
    pub amount: Option<i64>,
}

impl TileRaw {
    pub fn into_def(self, mod_id: &str) -> Result<TileDef, ModError> {
        let invalid = |reason| ModError::InvalidTile {
            mod_id: mod_id.to_string(),
            tile: self.id.clone(),
            reason,
        };
        let kind = match self.kind.as_str() {
            "go" => TileKind::Go,
            "chance" => TileKind::Chance,
            "community" => TileKind::Community,
            "jail" => TileKind::Jail,
            "go_to_jail" => TileKind::GoToJail,
            "free_parking" => TileKind::FreeParking,
            "tax" => TileKind::Tax {
                amount: self
                    .amount
                    .ok_or_else(|| invalid("tax requires `amount`"))?,
            },
            "property" => {
                let rent_model = self.rent_model.unwrap_or_default();
                let house_cost = match rent_model {
                    RentModel::Houses => self
                        .house_cost
                        .ok_or_else(|| invalid("property requires `house_cost`"))?,
                    _ => self.house_cost.unwrap_or(0),
                };
                TileKind::Property(PropertyDef {
                    group: self
                        .group
                        .clone()
                        .ok_or_else(|| invalid("property requires `group`"))?,
                    price: self
                        .price
                        .ok_or_else(|| invalid("property requires `price`"))?,
                    house_cost,
                    rents: self
                        .rents
                        .ok_or_else(|| invalid("property requires `rents` (6 values)"))?,
                    rent_model,
                })
            }
            _ => return Err(invalid("unknown tile type")),
        };
        Ok(TileDef {
            id: self.id,
            name: self.name,
            kind,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct CardsFile {
    #[serde(default)]
    pub chance: Vec<CardDef>,
    #[serde(default)]
    pub community: Vec<CardDef>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RulesFile {
    /// Flat named overrides (V1 hook points, architecture section 7.1.2).
    #[serde(default)]
    pub rules: BTreeMap<String, i64>,
}
