//! Mod manifest (`manifest.toml`) and the public info pushed to clients.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    /// Minimum game version, "x.y.z". Checked leniently: unparseable
    /// versions log a warning instead of failing the load.
    #[serde(default)]
    pub min_game_version: Option<String>,
}

/// Subset of the manifest serialized to joining clients (mod distribution
/// MVP: host pushes the resolved bundle over the WebSocket).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModInfo {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
}

impl From<&ModManifest> for ModInfo {
    fn from(m: &ModManifest) -> Self {
        Self {
            id: m.id.clone(),
            version: m.version.clone(),
            author: m.author.clone(),
        }
    }
}

/// Parse "x.y.z" (missing parts default to 0). Returns `None` on garbage.
pub(crate) fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}
