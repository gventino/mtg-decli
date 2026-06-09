use anyhow::{Context, Result};
use async_trait::async_trait;
use std::time::Duration;

use super::models::{Card, CardsResponse};
use super::{CardSource, SearchResult, SourceKind};

const BASE_URL: &str = "https://api.magicthegathering.io/v1";

/// Query parameters for the /cards endpoint.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub name: Option<String>,
    /// e.g. "Creature" or "Instant|Sorcery"
    pub types: Option<String>,
    /// e.g. "red,white" (AND) or "red|white" (OR)
    pub colors: Option<String>,
    pub color_identity: Option<String>,
    pub text: Option<String>,
    pub supertypes: Option<String>,
    pub rarity: Option<String>,
    pub set: Option<String>,
    /// e.g. "Commander" — implies legality=Legal on the API side.
    pub game_format: Option<String>,
    pub page: u32,
    pub page_size: u32,
    /// Only return cards that have an imageUrl.
    pub require_image: bool,
}

impl SearchQuery {
    pub fn by_name(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            page: 1,
            page_size: 50,
            require_image: true,
            game_format: Some("Commander".to_string()),
            ..Default::default()
        }
    }

    /// Parse a search string with optional filter tokens:
    /// `t:creature c:rg id:wub o:"draw a card" r:rare s:cmd f:any goblin king`
    /// Bare words become the name filter. Default format filter: Commander.
    pub fn parse(input: &str) -> Self {
        let mut q = Self {
            page: 1,
            page_size: 50,
            require_image: true,
            game_format: Some("Commander".to_string()),
            ..Default::default()
        };
        let mut name_parts: Vec<String> = Vec::new();
        for token in tokenize(input) {
            match token.split_once(':') {
                Some(("t", v)) => q.types = Some(capitalize_words(v)),
                Some(("c", v)) => q.colors = Some(expand_colors(v, false)),
                Some(("id", v)) => q.color_identity = Some(expand_colors(v, true)),
                Some(("o", v)) => q.text = Some(v.to_string()),
                Some(("r", v)) => q.rarity = Some(capitalize_words(v)),
                Some(("s", v)) => q.set = Some(v.to_uppercase()),
                Some(("f", v)) => {
                    q.game_format = match v.to_lowercase().as_str() {
                        "any" | "none" | "all" => None,
                        other => Some(capitalize_words(other)),
                    };
                }
                _ => name_parts.push(token),
            }
        }
        if !name_parts.is_empty() {
            q.name = Some(name_parts.join(" "));
        }
        q
    }

    fn to_params(&self) -> Vec<(&'static str, String)> {
        let mut p: Vec<(&'static str, String)> = Vec::new();
        let mut push_opt = |key: &'static str, val: &Option<String>| {
            if let Some(v) = val
                && !v.trim().is_empty() {
                    p.push((key, v.trim().to_string()));
                }
        };
        push_opt("name", &self.name);
        push_opt("types", &self.types);
        push_opt("colors", &self.colors);
        push_opt("colorIdentity", &self.color_identity);
        push_opt("text", &self.text);
        push_opt("supertypes", &self.supertypes);
        push_opt("rarity", &self.rarity);
        push_opt("set", &self.set);
        push_opt("gameFormat", &self.game_format);
        p.push(("page", self.page.max(1).to_string()));
        p.push(("pageSize", self.page_size.clamp(1, 100).to_string()));
        // NOTE: orderBy=name triggers HTTP 500 on the live API; sorting is done client-side.
        if self.require_image {
            p.push(("contains", "imageUrl".to_string()));
        }
        p
    }
}

#[derive(Debug, Clone)]
pub struct MtgClient {
    http: reqwest::Client,
    base: String,
}

impl Default for MtgClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MtgClient {
    pub fn new() -> Self {
        Self::with_base(BASE_URL)
    }

    pub fn with_base(base: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("mtg-deck-builder/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("failed to build http client");
        Self {
            http,
            base: base.into(),
        }
    }

    /// Search cards. Results are deduplicated by card name (the API returns
    /// one record per printing), preferring printings that have an image.
    pub async fn search_query(&self, query: &SearchQuery) -> Result<SearchResult> {
        let url = format!("{}/cards", self.base);
        let resp = self
            .http
            .get(&url)
            .query(&query.to_params())
            .send()
            .await
            .context("request to MTG API failed")?
            .error_for_status()
            .context("MTG API returned an error status")?;

        let total_count = resp
            .headers()
            .get("total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok());

        let body: CardsResponse = resp
            .json()
            .await
            .context("failed to decode MTG API response")?;

        Ok(SearchResult {
            cards: {
                let mut cards = dedupe_by_name(body.cards);
                cards.sort_by(|a, b| a.name.cmp(&b.name));
                cards
            },
            // total-count counts printings; pagination runs over printings,
            // so a next API page exists iff total > page * pageSize.
            has_more: total_count
                .map(|t| t > query.page as u64 * query.page_size.clamp(1, 100) as u64),
            total_count,
            page: query.page,
        })
    }

    /// Download raw image bytes for a card image URL.
    pub async fn fetch_image(&self, url: &str) -> Result<Vec<u8>> {
        // Gatherer serves http:// URLs; upgrade to https when possible.
        let url = url.replacen("http://", "https://", 1);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("image request failed")?
            .error_for_status()
            .context("image server returned an error status")?;
        let bytes = resp.bytes().await.context("failed to read image bytes")?;
        Ok(bytes.to_vec())
    }
}

#[async_trait]
impl CardSource for MtgClient {
    fn kind(&self) -> SourceKind {
        SourceKind::Mtgapi
    }

    async fn search(&self, raw_query: &str, page: u32) -> Result<SearchResult> {
        let mut query = SearchQuery::parse(raw_query);
        query.page = page;
        self.search_query(&query).await
    }

    async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
        self.fetch_image(url).await
    }
}

/// Keep one entry per card name, preferring entries that have an imageUrl.
fn dedupe_by_name(cards: Vec<Card>) -> Vec<Card> {
    let mut out: Vec<Card> = Vec::with_capacity(cards.len());
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for card in cards {
        match index.get(&card.name) {
            Some(&i) => {
                if out[i].image_url.is_none() && card.image_url.is_some() {
                    out[i] = card;
                }
            }
            None => {
                index.insert(card.name.clone(), out.len());
                out.push(card);
            }
        }
    }
    out
}

/// Split on whitespace, keeping `prefix:"quoted strings"` together.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn capitalize_words(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Expand color shorthand like "rg" or "r|g" into API values.
/// `colors` uses full names ("red,green"); `colorIdentity` uses codes ("R,G").
fn expand_colors(s: &str, as_identity: bool) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let mapped: Option<&str> = match ch.to_ascii_lowercase() {
            'w' => Some(if as_identity { "W" } else { "white" }),
            'u' => Some(if as_identity { "U" } else { "blue" }),
            'b' => Some(if as_identity { "B" } else { "black" }),
            'r' => Some(if as_identity { "R" } else { "red" }),
            'g' => Some(if as_identity { "G" } else { "green" }),
            'c' => Some(if as_identity { "C" } else { "colorless" }),
            ',' | '|' => {
                out.push(ch);
                None
            }
            _ => None,
        };
        if let Some(name) = mapped {
            if !out.is_empty() && !out.ends_with([',', '|']) {
                out.push(',');
            }
            out.push_str(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, image: Option<&str>) -> Card {
        Card {
            id: format!("id-{name}-{}", image.is_some()),
            name: name.to_string(),
            image_url: image.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn dedupe_prefers_image() {
        let cards = vec![
            card("Sol Ring", None),
            card("Sol Ring", Some("http://x/1.png")),
            card("Arcane Signet", Some("http://x/2.png")),
            card("Sol Ring", Some("http://x/3.png")),
        ];
        let out = dedupe_by_name(cards);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "Sol Ring");
        assert_eq!(out[0].image_url.as_deref(), Some("http://x/1.png"));
        assert_eq!(out[1].name, "Arcane Signet");
    }

    #[test]
    fn query_params_built() {
        let q = SearchQuery::by_name("nissa");
        let params = q.to_params();
        assert!(params.contains(&("name", "nissa".to_string())));
        assert!(params.contains(&("gameFormat", "Commander".to_string())));
        assert!(params.contains(&("contains", "imageUrl".to_string())));
        assert!(params.contains(&("page", "1".to_string())));
        assert!(!params.iter().any(|(k, _)| *k == "orderBy"));
    }

    #[test]
    fn parse_filters() {
        let q = SearchQuery::parse(r#"t:creature c:rg id:wub o:"draw a card" goblin king"#);
        assert_eq!(q.types.as_deref(), Some("Creature"));
        assert_eq!(q.colors.as_deref(), Some("red,green"));
        assert_eq!(q.color_identity.as_deref(), Some("W,U,B"));
        assert_eq!(q.text.as_deref(), Some("draw a card"));
        assert_eq!(q.name.as_deref(), Some("goblin king"));
        assert_eq!(q.game_format.as_deref(), Some("Commander"));
    }

    #[test]
    fn parse_format_override_and_or_colors() {
        let q = SearchQuery::parse("f:any c:r|g bolt");
        assert_eq!(q.game_format, None);
        assert_eq!(q.colors.as_deref(), Some("red|green"));
        assert_eq!(q.name.as_deref(), Some("bolt"));
        let q = SearchQuery::parse("r:mythic rare");
        assert_eq!(q.rarity.as_deref(), Some("Mythic"));
        assert_eq!(q.name.as_deref(), Some("rare"));
    }
}
