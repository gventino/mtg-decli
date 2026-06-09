use anyhow::{Context, Result};
use std::time::Duration;

use super::models::{Card, CardsResponse};

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

    fn to_params(&self) -> Vec<(&'static str, String)> {
        let mut p: Vec<(&'static str, String)> = Vec::new();
        let mut push_opt = |key: &'static str, val: &Option<String>| {
            if let Some(v) = val {
                if !v.trim().is_empty() {
                    p.push((key, v.trim().to_string()));
                }
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

#[derive(Debug)]
pub struct SearchResult {
    pub cards: Vec<Card>,
    /// Total matching printings reported by the API (header `total-count`).
    pub total_count: Option<u64>,
    pub page: u32,
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
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResult> {
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
            total_count,
            page: query.page,
        })
    }

    /// Download raw image bytes for a card image URL.
    pub async fn download_image(&self, url: &str) -> Result<Vec<u8>> {
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
}
