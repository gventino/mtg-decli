use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::api::SourceKind;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub source: SourceKind,
}

fn config_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "mtg-decli")
        .map(|d| d.config_dir().join("config.json"))
        .unwrap_or_else(|| PathBuf::from(".mtg-decli-config.json"))
}

/// Load the config, falling back to defaults on any error.
pub fn load() -> Config {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the config; errors are non-fatal (best effort).
pub fn save(config: &Config) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrip_serde() {
        let c = Config {
            source: SourceKind::Mtgapi,
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"mtgapi\""));
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, SourceKind::Mtgapi);
        // unknown/empty -> default Scryfall
        let d: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(d.source, SourceKind::Scryfall);
    }
}
