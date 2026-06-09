use anyhow::{Context, Result};
use std::path::PathBuf;

use super::Deck;

pub fn decks_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "mtg-decli")
        .map(|d| d.data_dir().join("decks"))
        .unwrap_or_else(|| PathBuf::from("decks"))
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "deck".to_string() } else { s }
}

pub fn deck_path(name: &str) -> PathBuf {
    decks_dir().join(format!("{}.json", slug(name)))
}

pub fn save(deck: &Deck) -> Result<PathBuf> {
    let path = deck_path(&deck.name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("failed to create decks directory")?;
    }
    let json = serde_json::to_string_pretty(deck).context("failed to serialize deck")?;
    std::fs::write(&path, json).context("failed to write deck file")?;
    Ok(path)
}

pub fn load(path: &std::path::Path) -> Result<Deck> {
    let data = std::fs::read_to_string(path).context("failed to read deck file")?;
    serde_json::from_str(&data).context("failed to parse deck file")
}

/// List saved decks as (name, path), sorted by name.
pub fn list() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(decks_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json")
            && let Ok(deck) = load(&path) {
                out.push((deck.name, path));
            }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Export in the standard text format (Moxfield/Archidekt compatible):
/// quantity + name, commander last under its own marker.
pub fn export_txt(deck: &Deck) -> String {
    let mut out = String::new();
    for entry in &deck.entries {
        out.push_str(&format!("{} {}\n", entry.quantity, entry.card.name));
    }
    if let Some(cmdr) = &deck.commander {
        out.push_str("\n// Commander\n");
        out.push_str(&format!("1 {}\n", cmdr.name));
    }
    out
}

pub fn export_txt_to_file(deck: &Deck) -> Result<PathBuf> {
    let path = decks_dir().join(format!("{}.txt", slug(&deck.name)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("failed to create decks directory")?;
    }
    std::fs::write(&path, export_txt(deck)).context("failed to write export file")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Card;
    use crate::deck::DeckEntry;

    #[test]
    fn export_format() {
        let mut deck = Deck::new("Test Deck");
        let mut forest = DeckEntry::new(Card {
            name: "Forest".into(),
            ..Default::default()
        });
        forest.quantity = 10;
        deck.entries.push(forest);
        deck.commander = Some(Card {
            name: "Atraxa, Praetors' Voice".into(),
            ..Default::default()
        });
        let txt = export_txt(&deck);
        assert!(txt.contains("10 Forest\n"));
        assert!(txt.contains("// Commander\n1 Atraxa, Praetors' Voice\n"));
    }

    #[test]
    fn roundtrip_save_load() {
        let mut deck = Deck::new("test");
        deck.entries.push(DeckEntry::new(Card {
            name: "Sol Ring".into(),
            ..Default::default()
        }));
        let json = serde_json::to_string(&deck).unwrap();
        let loaded: Deck = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].card.name, "Sol Ring");
    }

    #[test]
    fn slug_sanitizes() {
        assert_eq!(slug("Atraxa, Praetors' Voice!"), "atraxa--praetors--voice");
        assert_eq!(slug("***"), "deck");
    }
}
