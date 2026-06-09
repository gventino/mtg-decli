use serde::{Deserialize, Serialize};

/// A Magic: The Gathering card as returned by api.magicthegathering.io/v1.
/// Unknown fields are ignored; only fields the app uses are modeled.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub names: Option<Vec<String>>,
    pub layout: Option<String>,
    pub mana_cost: Option<String>,
    pub cmc: Option<f64>,
    pub colors: Option<Vec<String>>,
    pub color_identity: Option<Vec<String>>,
    /// Full type line, e.g. "Legendary Creature — Elf Druid".
    #[serde(rename = "type")]
    pub type_line: Option<String>,
    pub supertypes: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
    pub subtypes: Option<Vec<String>>,
    pub rarity: Option<String>,
    pub set: Option<String>,
    pub set_name: Option<String>,
    pub text: Option<String>,
    pub flavor: Option<String>,
    pub artist: Option<String>,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    pub image_url: Option<String>,
    pub legalities: Option<Vec<Legality>>,
    pub printings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Legality {
    pub format: String,
    pub legality: String,
}

impl Card {
    pub fn is_legal_in_commander(&self) -> bool {
        self.legalities
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|l| l.format == "Commander" && l.legality == "Legal")
    }

    /// True if the card has the "Basic" supertype (exempt from singleton rule).
    pub fn is_basic(&self) -> bool {
        self.supertypes
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|s| s == "Basic")
    }

    /// True if this card can be a commander (legendary creature).
    pub fn can_be_commander(&self) -> bool {
        let supertypes = self.supertypes.as_deref().unwrap_or_default();
        let types = self.types.as_deref().unwrap_or_default();
        let legendary = supertypes.iter().any(|s| s == "Legendary");
        let creature = types.iter().any(|t| t == "Creature");
        let text_allows = self
            .text
            .as_deref()
            .is_some_and(|t| t.contains("can be your commander"));
        (legendary && creature) || text_allows
    }

    /// Color identity as single-letter codes (W/U/B/R/G), empty for colorless.
    pub fn identity(&self) -> Vec<String> {
        self.color_identity.clone().unwrap_or_default()
    }
}

#[derive(Debug, Deserialize)]
pub struct CardsResponse {
    pub cards: Vec<Card>,
}
