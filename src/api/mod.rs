pub mod models;
pub mod mtgapi;
pub mod scryfall;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use models::Card;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    #[default]
    Scryfall,
    Mtgapi,
}

impl SourceKind {
    pub fn name(&self) -> &'static str {
        match self {
            SourceKind::Scryfall => "scryfall",
            SourceKind::Mtgapi => "mtg-api",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SourceKind::Scryfall => SourceKind::Mtgapi,
            SourceKind::Mtgapi => SourceKind::Scryfall,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "scryfall" => Some(SourceKind::Scryfall),
            "mtgapi" | "mtg-api" | "mtg" | "magicthegathering" => Some(SourceKind::Mtgapi),
            _ => None,
        }
    }
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug)]
pub struct SearchResult {
    pub cards: Vec<Card>,
    /// Total matches reported by the API (Scryfall: unique cards;
    /// MTG API: printings, pre-dedupe).
    pub total_count: Option<u64>,
    pub page: u32,
    /// Whether another page exists, when the API reports it.
    pub has_more: Option<bool>,
}

/// A searchable card database. Implementations interpret the user's raw
/// search input with their own syntax (shared tokens: t/c/id/o/r/s/f).
#[async_trait]
pub trait CardSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn search(&self, raw_query: &str, page: u32) -> Result<SearchResult>;
    async fn download_image(&self, url: &str) -> Result<Vec<u8>>;
}

pub fn make_source(kind: SourceKind) -> Arc<dyn CardSource> {
    match kind {
        SourceKind::Scryfall => Arc::new(scryfall::ScryfallClient::new()),
        SourceKind::Mtgapi => Arc::new(mtgapi::MtgClient::new()),
    }
}
