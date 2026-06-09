use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::Duration;

use super::models::{Card, Legality};
use super::{CardSource, SearchResult, SourceKind};

const BASE_URL: &str = "https://api.scryfall.com";

const KNOWN_SUPERTYPES: &[&str] = &[
    "Basic", "Legendary", "Snow", "World", "Ongoing", "Elite", "Host",
];

#[derive(Debug, Clone)]
pub struct ScryfallClient {
    http: reqwest::Client,
    base: String,
}

impl Default for ScryfallClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ScryfallClient {
    pub fn new() -> Self {
        Self::with_base(BASE_URL)
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        // Scryfall requires an accurate User-Agent and an Accept header.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let http = reqwest::Client::builder()
            .user_agent(concat!("mtg-deck-builder/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("failed to build http client");
        Self {
            http,
            base: base.into(),
        }
    }
}

#[async_trait]
impl CardSource for ScryfallClient {
    fn kind(&self) -> SourceKind {
        SourceKind::Scryfall
    }

    async fn search(&self, raw_query: &str, page: u32) -> Result<SearchResult> {
        let q = build_query(raw_query);
        let url = format!("{}/cards/search", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("q", q.as_str()),
                ("unique", "cards"),
                ("page", &page.max(1).to_string()),
            ])
            .send()
            .await
            .context("request to Scryfall failed")?;

        // Scryfall answers 404 with an error object when nothing matches.
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(SearchResult {
                cards: Vec::new(),
                total_count: Some(0),
                page,
                has_more: Some(false),
            });
        }
        let resp = resp
            .error_for_status()
            .context("Scryfall returned an error status")?;
        let body: ScryfallList = resp
            .json()
            .await
            .context("failed to decode Scryfall response")?;

        Ok(SearchResult {
            cards: body.data.into_iter().map(map_card).collect(),
            total_count: body.total_cards,
            page,
            has_more: Some(body.has_more),
        })
    }

    async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .context("image request failed")?
            .error_for_status()
            .context("image server returned an error status")?;
        let bytes = resp.bytes().await.context("failed to read image bytes")?;
        Ok(bytes.to_vec())
    }
}

/// Build the Scryfall fulltext query from the user's raw input.
/// Scryfall syntax is a superset of our filter tokens, so input passes
/// through as-is except: `f:any` lifts the default Commander constraint,
/// `f:xxx` becomes `format:xxx`, and `format:commander` is appended when
/// no format/legality constraint is present.
pub fn build_query(input: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut has_format_constraint = false;
    let mut suppress_default = false;

    for token in input.split_whitespace() {
        let lower = token.to_lowercase();
        if let Some(v) = lower.strip_prefix("f:") {
            match v {
                "any" | "none" | "all" => suppress_default = true,
                _ => {
                    parts.push(format!("format:{v}"));
                    has_format_constraint = true;
                }
            }
            continue;
        }
        if lower.starts_with("format:") || lower.starts_with("legal:") || lower.starts_with("banned:") || lower.starts_with("restricted:") {
            has_format_constraint = true;
        }
        parts.push(token.to_string());
    }

    if !has_format_constraint && !suppress_default {
        parts.push("format:commander".to_string());
    }
    parts.join(" ")
}

// ----- Scryfall JSON -> unified Card mapping ------------------------------

#[derive(Debug, Deserialize)]
struct ScryfallList {
    #[serde(default)]
    data: Vec<ScryfallCard>,
    #[serde(default)]
    has_more: bool,
    total_cards: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ScryfallCard {
    id: String,
    name: String,
    layout: Option<String>,
    mana_cost: Option<String>,
    cmc: Option<f64>,
    colors: Option<Vec<String>>,
    color_identity: Option<Vec<String>>,
    type_line: Option<String>,
    oracle_text: Option<String>,
    flavor_text: Option<String>,
    power: Option<String>,
    toughness: Option<String>,
    loyalty: Option<String>,
    rarity: Option<String>,
    set: Option<String>,
    set_name: Option<String>,
    artist: Option<String>,
    legalities: Option<BTreeMap<String, String>>,
    image_uris: Option<ImageUris>,
    card_faces: Option<Vec<CardFace>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ImageUris {
    normal: Option<String>,
    large: Option<String>,
    small: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CardFace {
    name: Option<String>,
    mana_cost: Option<String>,
    type_line: Option<String>,
    oracle_text: Option<String>,
    flavor_text: Option<String>,
    power: Option<String>,
    toughness: Option<String>,
    loyalty: Option<String>,
    image_uris: Option<ImageUris>,
}

impl ImageUris {
    fn best(&self) -> Option<String> {
        self.normal
            .clone()
            .or_else(|| self.large.clone())
            .or_else(|| self.small.clone())
    }
}

fn map_card(c: ScryfallCard) -> Card {
    let faces = c.card_faces.as_deref().unwrap_or_default();
    let face0 = faces.first();

    let type_line = c
        .type_line
        .clone()
        .or_else(|| face0.and_then(|f| f.type_line.clone()));
    let (supertypes, types, subtypes) = parse_type_line(type_line.as_deref().unwrap_or(""));

    let image_url = c
        .image_uris
        .as_ref()
        .and_then(|i| i.best())
        .or_else(|| face0.and_then(|f| f.image_uris.as_ref()).and_then(|i| i.best()));

    let text = c.oracle_text.clone().or_else(|| {
        let joined: Vec<String> = faces
            .iter()
            .filter_map(|f| {
                f.oracle_text
                    .as_ref()
                    .map(|t| match &f.name {
                        Some(n) => format!("{n}:\n{t}"),
                        None => t.clone(),
                    })
            })
            .collect();
        if joined.is_empty() {
            None
        } else {
            Some(joined.join("\n—\n"))
        }
    });

    let names: Option<Vec<String>> = if faces.len() > 1 {
        Some(faces.iter().filter_map(|f| f.name.clone()).collect())
    } else {
        None
    };

    let pick = |top: &Option<String>, from_face: fn(&CardFace) -> &Option<String>| -> Option<String> {
        top.clone()
            .filter(|v| !v.is_empty())
            .or_else(|| face0.and_then(|f| from_face(f).clone()))
    };

    Card {
        id: c.id,
        name: c.name,
        names,
        layout: c.layout,
        mana_cost: pick(&c.mana_cost, |f| &f.mana_cost),
        cmc: c.cmc,
        colors: c.colors,
        color_identity: c.color_identity,
        type_line,
        supertypes: Some(supertypes),
        types: Some(types),
        subtypes: if subtypes.is_empty() { None } else { Some(subtypes) },
        rarity: c.rarity.map(map_rarity),
        set: c.set.map(|s| s.to_uppercase()),
        set_name: c.set_name,
        text,
        flavor: pick(&c.flavor_text, |f| &f.flavor_text),
        artist: c.artist,
        power: pick(&c.power, |f| &f.power),
        toughness: pick(&c.toughness, |f| &f.toughness),
        loyalty: pick(&c.loyalty, |f| &f.loyalty),
        image_url,
        legalities: c.legalities.map(map_legalities),
        printings: None,
    }
}

fn map_rarity(r: String) -> String {
    match r.as_str() {
        "common" => "Common".to_string(),
        "uncommon" => "Uncommon".to_string(),
        "rare" => "Rare".to_string(),
        "mythic" => "Mythic Rare".to_string(),
        "special" => "Special".to_string(),
        "bonus" => "Bonus".to_string(),
        other => capitalize(other),
    }
}

fn map_legalities(map: BTreeMap<String, String>) -> Vec<Legality> {
    map.into_iter()
        .map(|(format, status)| Legality {
            format: capitalize(&format),
            legality: match status.as_str() {
                "legal" => "Legal".to_string(),
                "not_legal" => "Not Legal".to_string(),
                "restricted" => "Restricted".to_string(),
                "banned" => "Banned".to_string(),
                other => capitalize(other),
            },
        })
        .collect()
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Split a Scryfall type line ("Legendary Creature — Elf Druid") into
/// supertypes, types and subtypes. Multiface lines ("Instant // Sorcery")
/// are parsed from their first face.
fn parse_type_line(line: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let first_face = line.split(" // ").next().unwrap_or("");
    let (left, right) = match first_face.split_once('—') {
        Some((l, r)) => (l.trim(), r.trim()),
        None => (first_face.trim(), ""),
    };
    let mut supertypes = Vec::new();
    let mut types = Vec::new();
    for word in left.split_whitespace() {
        if KNOWN_SUPERTYPES.contains(&word) {
            supertypes.push(word.to_string());
        } else {
            types.push(word.to_string());
        }
    }
    let subtypes = right
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    (supertypes, types, subtypes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_line_parsing() {
        let (sup, ty, sub) = parse_type_line("Legendary Creature — Elf Druid");
        assert_eq!(sup, vec!["Legendary"]);
        assert_eq!(ty, vec!["Creature"]);
        assert_eq!(sub, vec!["Elf", "Druid"]);

        let (sup, ty, sub) = parse_type_line("Basic Land — Forest");
        assert_eq!(sup, vec!["Basic"]);
        assert_eq!(ty, vec!["Land"]);
        assert_eq!(sub, vec!["Forest"]);

        let (sup, ty, sub) = parse_type_line("Instant");
        assert!(sup.is_empty());
        assert_eq!(ty, vec!["Instant"]);
        assert!(sub.is_empty());

        let (_, ty, _) = parse_type_line("Instant // Sorcery");
        assert_eq!(ty, vec!["Instant"]);
    }

    #[test]
    fn query_appends_commander_by_default() {
        assert_eq!(build_query("sol ring"), "sol ring format:commander");
        assert_eq!(
            build_query("t:creature c:rg"),
            "t:creature c:rg format:commander"
        );
    }

    #[test]
    fn query_format_token_handling() {
        assert_eq!(build_query("f:any goblin"), "goblin");
        assert_eq!(build_query("f:modern bolt"), "format:modern bolt");
        assert_eq!(
            build_query("format:legacy daze"),
            "format:legacy daze"
        );
        assert_eq!(build_query("banned:commander x"), "banned:commander x");
    }

    #[test]
    fn maps_scryfall_card() {
        let json = r#"{
            "id": "uuid-1",
            "name": "Atraxa, Praetors' Voice",
            "layout": "normal",
            "mana_cost": "{G}{W}{U}{B}",
            "cmc": 4.0,
            "colors": ["W","U","B","G"],
            "color_identity": ["B","G","U","W"],
            "type_line": "Legendary Creature — Phyrexian Angel Horror",
            "oracle_text": "Flying, vigilance, deathtouch, lifelink",
            "power": "4",
            "toughness": "4",
            "rarity": "mythic",
            "set": "c16",
            "set_name": "Commander 2016",
            "artist": "Victor Adame Minguez",
            "legalities": {"commander": "legal", "modern": "not_legal"},
            "image_uris": {"normal": "https://cards.scryfall.io/normal/x.jpg"}
        }"#;
        let sc: ScryfallCard = serde_json::from_str(json).unwrap();
        let card = map_card(sc);
        assert_eq!(card.name, "Atraxa, Praetors' Voice");
        assert_eq!(card.supertypes.as_deref(), Some(&["Legendary".to_string()][..]));
        assert_eq!(card.types.as_deref(), Some(&["Creature".to_string()][..]));
        assert_eq!(
            card.subtypes.as_deref().map(|s| s.len()),
            Some(3)
        );
        assert!(card.can_be_commander());
        assert!(card.is_legal_in_commander());
        assert_eq!(card.rarity.as_deref(), Some("Mythic Rare"));
        assert_eq!(card.set.as_deref(), Some("C16"));
        assert_eq!(
            card.image_url.as_deref(),
            Some("https://cards.scryfall.io/normal/x.jpg")
        );
        assert_eq!(card.identity().len(), 4);
    }

    #[test]
    fn maps_double_faced_card() {
        let json = r#"{
            "id": "uuid-2",
            "name": "Delver of Secrets // Insectile Aberration",
            "layout": "transform",
            "cmc": 1.0,
            "color_identity": ["U"],
            "type_line": "Creature — Human Wizard // Creature — Human Insect",
            "rarity": "common",
            "legalities": {"commander": "legal"},
            "card_faces": [
                {
                    "name": "Delver of Secrets",
                    "mana_cost": "{U}",
                    "type_line": "Creature — Human Wizard",
                    "oracle_text": "At the beginning of your upkeep...",
                    "power": "1",
                    "toughness": "1",
                    "image_uris": {"normal": "https://cards.scryfall.io/normal/front.jpg"}
                },
                {
                    "name": "Insectile Aberration",
                    "type_line": "Creature — Human Insect",
                    "oracle_text": "Flying",
                    "power": "3",
                    "toughness": "2",
                    "image_uris": {"normal": "https://cards.scryfall.io/normal/back.jpg"}
                }
            ]
        }"#;
        let sc: ScryfallCard = serde_json::from_str(json).unwrap();
        let card = map_card(sc);
        assert_eq!(card.mana_cost.as_deref(), Some("{U}"));
        assert_eq!(card.power.as_deref(), Some("1"));
        assert_eq!(
            card.image_url.as_deref(),
            Some("https://cards.scryfall.io/normal/front.jpg")
        );
        assert_eq!(card.names.as_ref().map(|n| n.len()), Some(2));
        assert!(card.text.as_deref().unwrap().contains("Flying"));
        assert_eq!(card.types.as_deref(), Some(&["Creature".to_string()][..]));
    }

    #[test]
    fn empty_search_is_not_error() {
        // 404 handling is in search(); here we check the list decode default.
        let body: ScryfallList = serde_json::from_str(r#"{"object":"list","data":[]}"#).unwrap();
        assert!(body.data.is_empty());
        assert!(!body.has_more);
    }
}
