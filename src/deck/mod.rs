pub mod stats;
pub mod storage;
pub mod validate;

use serde::{Deserialize, Serialize};

use crate::api::models::Card;

pub const AUTO_CATEGORIES: &[&str] = &[
    "Creatures",
    "Planeswalkers",
    "Instants",
    "Sorceries",
    "Artifacts",
    "Enchantments",
    "Battles",
    "Lands",
    "Other",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeckEntry {
    pub card: Card,
    /// >1 only allowed for basic lands (singleton format).
    pub quantity: u32,
    /// Custom category assigned by the user; None = automatic by card type.
    pub category: Option<String>,
}

impl DeckEntry {
    pub fn new(card: Card) -> Self {
        Self {
            card,
            quantity: 1,
            category: None,
        }
    }

    /// Effective category: custom override or automatic by primary card type.
    pub fn category_name(&self) -> String {
        if let Some(c) = &self.category {
            return c.clone();
        }
        auto_category(&self.card).to_string()
    }
}

/// Map a card to its automatic category based on its primary type.
pub fn auto_category(card: &Card) -> &'static str {
    let types = card.types.as_deref().unwrap_or_default();
    let has = |t: &str| types.iter().any(|x| x == t);
    // Creature takes precedence (e.g. artifact creatures count as creatures).
    if has("Creature") {
        "Creatures"
    } else if has("Planeswalker") {
        "Planeswalkers"
    } else if has("Instant") {
        "Instants"
    } else if has("Sorcery") {
        "Sorceries"
    } else if has("Artifact") {
        "Artifacts"
    } else if has("Enchantment") {
        "Enchantments"
    } else if has("Battle") {
        "Battles"
    } else if has("Land") {
        "Lands"
    } else {
        "Other"
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Deck {
    pub name: String,
    pub commander: Option<Card>,
    pub entries: Vec<DeckEntry>,
    /// User-created custom categories (shown even when empty).
    #[serde(default)]
    pub custom_categories: Vec<String>,
}

impl Deck {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Total card count including commander and basic land quantities.
    pub fn card_count(&self) -> u32 {
        let commander = if self.commander.is_some() { 1 } else { 0 };
        commander + self.entries.iter().map(|e| e.quantity).sum::<u32>()
    }

    pub fn contains(&self, card_name: &str) -> bool {
        self.entries.iter().any(|e| e.card.name == card_name)
            || self
                .commander
                .as_ref()
                .is_some_and(|c| c.name == card_name)
    }

    /// Add a card. Basics increment quantity; non-basics are singleton.
    /// Returns false if the card was already present (and is not a basic).
    pub fn add_card(&mut self, card: Card) -> bool {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.card.name == card.name) {
            if entry.card.is_basic() {
                entry.quantity += 1;
                return true;
            }
            return false;
        }
        if self
            .commander
            .as_ref()
            .is_some_and(|c| c.name == card.name)
        {
            return false;
        }
        self.entries.push(DeckEntry::new(card));
        true
    }

    /// Remove one copy; removes the entry when quantity reaches zero.
    /// Returns true if something was removed.
    pub fn remove_card(&mut self, card_name: &str) -> bool {
        if let Some(idx) = self.entries.iter().position(|e| e.card.name == card_name) {
            let entry = &mut self.entries[idx];
            if entry.quantity > 1 {
                entry.quantity -= 1;
            } else {
                self.entries.remove(idx);
            }
            return true;
        }
        false
    }

    pub fn set_commander(&mut self, card: Card) {
        // If the card was in the main deck, move it out.
        self.entries.retain(|e| e.card.name != card.name);
        self.commander = Some(card);
    }

    pub fn add_custom_category(&mut self, name: impl Into<String>) {
        let name = name.into();
        let exists = self
            .custom_categories
            .iter()
            .any(|c| c.eq_ignore_ascii_case(&name));
        if !name.trim().is_empty() && !exists {
            self.custom_categories.push(name);
        }
    }

    /// Assign a custom category (or None to revert to automatic).
    pub fn set_category(&mut self, card_name: &str, category: Option<String>) {
        if let Some(c) = &category {
            self.add_custom_category(c.clone());
        }
        if let Some(entry) = self.entries.iter_mut().find(|e| e.card.name == card_name) {
            entry.category = category;
        }
    }

    /// Entries grouped by category, ordered: custom categories first (user
    /// order), then automatic categories in canonical order.
    pub fn grouped(&self) -> Vec<(String, Vec<&DeckEntry>)> {
        let mut order: Vec<String> = self.custom_categories.clone();
        for auto in AUTO_CATEGORIES {
            if !order.iter().any(|c| c == auto) {
                order.push((*auto).to_string());
            }
        }
        let mut groups: Vec<(String, Vec<&DeckEntry>)> =
            order.into_iter().map(|c| (c, Vec::new())).collect();
        for entry in &self.entries {
            let cat = entry.category_name();
            if let Some((_, list)) = groups.iter_mut().find(|(name, _)| *name == cat) {
                list.push(entry);
            }
        }
        for (_, list) in &mut groups {
            list.sort_by(|a, b| a.card.name.cmp(&b.card.name));
        }
        groups.retain(|(name, list)| {
            !list.is_empty() || self.custom_categories.iter().any(|c| c == name)
        });
        groups
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn card(name: &str, types: &[&str], supertypes: &[&str]) -> Card {
        Card {
            id: format!("id-{name}"),
            name: name.to_string(),
            types: Some(types.iter().map(|s| s.to_string()).collect()),
            supertypes: Some(supertypes.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn singleton_enforced_on_add() {
        let mut deck = Deck::new("test");
        assert!(deck.add_card(card("Sol Ring", &["Artifact"], &[])));
        assert!(!deck.add_card(card("Sol Ring", &["Artifact"], &[])));
        assert_eq!(deck.card_count(), 1);
    }

    #[test]
    fn basics_stack_quantity() {
        let mut deck = Deck::new("test");
        assert!(deck.add_card(card("Forest", &["Land"], &["Basic"])));
        assert!(deck.add_card(card("Forest", &["Land"], &["Basic"])));
        assert_eq!(deck.card_count(), 2);
        assert_eq!(deck.entries.len(), 1);
        deck.remove_card("Forest");
        assert_eq!(deck.card_count(), 1);
        deck.remove_card("Forest");
        assert!(deck.entries.is_empty());
    }

    #[test]
    fn auto_category_precedence() {
        assert_eq!(
            auto_category(&card("Karn", &["Artifact", "Creature"], &[])),
            "Creatures"
        );
        assert_eq!(auto_category(&card("Bolt", &["Instant"], &[])), "Instants");
        assert_eq!(
            auto_category(&card("Island", &["Land"], &["Basic"])),
            "Lands"
        );
    }

    #[test]
    fn custom_category_grouping() {
        let mut deck = Deck::new("test");
        deck.add_card(card("Cultivate", &["Sorcery"], &[]));
        deck.set_category("Cultivate", Some("Ramp".to_string()));
        let groups = deck.grouped();
        assert_eq!(groups[0].0, "Ramp");
        assert_eq!(groups[0].1.len(), 1);
        deck.set_category("Cultivate", None);
        let groups = deck.grouped();
        // Ramp stays visible (custom), but Cultivate moved back to Sorceries.
        assert!(groups.iter().any(|(n, l)| n == "Ramp" && l.is_empty()));
        assert!(groups.iter().any(|(n, l)| n == "Sorceries" && l.len() == 1));
    }

    #[test]
    fn commander_moves_out_of_main() {
        let mut deck = Deck::new("test");
        let cmdr = card("Atraxa", &["Creature"], &["Legendary"]);
        deck.add_card(cmdr.clone());
        deck.set_commander(cmdr);
        assert!(deck.entries.is_empty());
        assert_eq!(deck.card_count(), 1);
    }
}
