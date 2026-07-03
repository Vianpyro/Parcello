//! Mod layer: loads TOML mod bundles and merges them into a resolved
//! `GameContent` (Registry + Plugin patterns, architecture section 7).
//!
//! V1 mods are data-only. Base game content is itself a mod, always loaded
//! first; mod content layers on top with last-loaded-wins per key.

mod loader;
mod manifest;
mod plugin;
mod raw;
mod registry;

pub use loader::{resolve, ResolvedContent};
pub use manifest::{ModInfo, ModManifest};
pub use plugin::{ModPlugin, TomlModPlugin};
pub use registry::RegistryBuilder;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModError {
    #[error("mod directory not found: {0}")]
    NotFound(String),
    #[error("io error in mod {mod_id}: {source}")]
    Io {
        mod_id: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML in {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("mod {mod_id} requires game version >= {required}, running {running}")]
    IncompatibleVersion {
        mod_id: String,
        required: String,
        running: String,
    },
    #[error("invalid tile {tile} in mod {mod_id}: {reason}")]
    InvalidTile {
        mod_id: String,
        tile: String,
        reason: &'static str,
    },
    #[error("resolved content is invalid: {0}")]
    Content(#[from] parcello_engine::ContentError),
}
