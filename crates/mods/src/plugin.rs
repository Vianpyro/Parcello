//! Plugin pattern: stable integration point for all mod kinds.
//!
//! V1 ships `TomlModPlugin` only. V2 will add a Wasmtime-backed plugin behind
//! the same trait; the loader is the only component aware of the lifecycle.

use std::fs;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::manifest::{parse_version, ModManifest};
use crate::raw::{CardsFile, PropertiesFile, RulesFile};
use crate::{ModError, RegistryBuilder};

pub trait ModPlugin {
    fn id(&self) -> &str;
    fn version(&self) -> &str;
    /// Populate registries. Called once at room creation, in load order.
    fn on_load(&self, registries: &mut RegistryBuilder) -> Result<(), ModError>;
    /// Called at room teardown. Data-only mods have nothing to release.
    fn on_unload(&self) {}
}

/// A V1 data-only mod backed by a directory of TOML files. All data files
/// are optional; a mod may override only rules, only cards, etc.
pub struct TomlModPlugin {
    manifest: ModManifest,
    root: PathBuf,
}

impl TomlModPlugin {
    pub fn open(root: &Path) -> Result<Self, ModError> {
        let manifest_path = root.join("manifest.toml");
        let raw = fs::read_to_string(&manifest_path).map_err(|source| ModError::Io {
            mod_id: root.display().to_string(),
            source,
        })?;
        let manifest: ModManifest = toml::from_str(&raw).map_err(|source| ModError::Parse {
            path: manifest_path.display().to_string(),
            source,
        })?;
        check_min_version(&manifest)?;
        Ok(Self {
            manifest,
            root: root.to_path_buf(),
        })
    }

    pub fn manifest(&self) -> &ModManifest {
        &self.manifest
    }

    fn read_data<T: serde::de::DeserializeOwned + Default>(
        &self,
        file: &str,
    ) -> Result<T, ModError> {
        let path = self.root.join("data").join(file);
        if !path.exists() {
            return Ok(T::default());
        }
        let raw = fs::read_to_string(&path).map_err(|source| ModError::Io {
            mod_id: self.manifest.id.clone(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| ModError::Parse {
            path: path.display().to_string(),
            source,
        })
    }
}

impl ModPlugin for TomlModPlugin {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn version(&self) -> &str {
        &self.manifest.version
    }

    fn on_load(&self, registries: &mut RegistryBuilder) -> Result<(), ModError> {
        let properties: PropertiesFile = self.read_data("properties.toml")?;
        for raw in properties.tiles {
            let tile = raw.into_def(&self.manifest.id)?;
            registries.upsert_tile(&self.manifest.id, tile);
        }
        let cards: CardsFile = self.read_data("cards.toml")?;
        for card in cards.chance {
            registries.upsert_chance(&self.manifest.id, card);
        }
        for card in cards.community {
            registries.upsert_community(&self.manifest.id, card);
        }
        let rules: RulesFile = self.read_data("rules.toml")?;
        for (key, value) in rules.rules {
            registries.set_rule(&self.manifest.id, &key, value);
        }
        Ok(())
    }
}

fn check_min_version(manifest: &ModManifest) -> Result<(), ModError> {
    let Some(required) = &manifest.min_game_version else {
        return Ok(());
    };
    let running = env!("CARGO_PKG_VERSION");
    match (parse_version(required), parse_version(running)) {
        (Some(req), Some(run)) if req > run => Err(ModError::IncompatibleVersion {
            mod_id: manifest.id.clone(),
            required: required.clone(),
            running: running.to_string(),
        }),
        (None, _) => {
            warn!(mod_id = %manifest.id, min_game_version = %required, "unparseable min_game_version, ignoring");
            Ok(())
        }
        _ => Ok(()),
    }
}
