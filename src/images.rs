use anyhow::{Context, Result};
use image::DynamicImage;
use std::path::PathBuf;

use crate::api::client::MtgClient;

pub fn cache_dir() -> PathBuf {
    directories::ProjectDirs::from("", "", "mtg-deck-builder")
        .map(|d| d.cache_dir().join("images"))
        .unwrap_or_else(|| PathBuf::from(".mtg-cache/images"))
}

fn cache_path(card_id: &str) -> PathBuf {
    cache_dir().join(format!("{card_id}.img"))
}

/// Load a card image, hitting the on-disk cache first and falling back to
/// downloading from the URL provided by the API.
pub async fn load_card_image(client: &MtgClient, card_id: &str, url: &str) -> Result<DynamicImage> {
    let path = cache_path(card_id);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let bytes = client.download_image(url).await?;
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(&path, &bytes).await;
            bytes
        }
    };
    image::load_from_memory(&bytes).context("failed to decode card image")
}
