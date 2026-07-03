//! Resolves an ordered mod list into immutable room content.
//!
//! Base game content is itself a mod (`base`), loaded first by convention.
//! ADR-0004: the mod set is resolved once per server, not per room, for MVP.

use std::path::Path;

use parcello_engine::GameContent;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::plugin::{ModPlugin, TomlModPlugin};
use crate::{ModError, ModInfo, RegistryBuilder};

/// Content plus the mod metadata pushed to joining clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedContent {
    pub content: GameContent,
    pub mods: Vec<ModInfo>,
}

/// Load `mod_ids` from `mods_dir/<id>` in order and merge them.
pub fn resolve(mods_dir: &Path, mod_ids: &[String]) -> Result<ResolvedContent, ModError> {
    let mut registries = RegistryBuilder::new();
    let mut mods = Vec::with_capacity(mod_ids.len());
    for id in mod_ids {
        let root = mods_dir.join(id);
        if !root.is_dir() {
            return Err(ModError::NotFound(root.display().to_string()));
        }
        let plugin = TomlModPlugin::open(&root)?;
        info!(mod_id = plugin.id(), version = plugin.version(), "loading mod");
        plugin.on_load(&mut registries)?;
        mods.push(ModInfo::from(plugin.manifest()));
    }
    let content = registries.build()?;
    Ok(ResolvedContent { content, mods })
}
