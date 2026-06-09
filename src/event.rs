use image::DynamicImage;
use ratatui_image::thread::ResizeResponse;

use crate::api::SearchResult;

/// Messages sent from async tasks back to the UI loop.
pub enum AppEvent {
    SearchDone {
        query_id: u64,
        result: Result<SearchResult, String>,
    },
    ImageDone {
        card_id: String,
        result: Result<DynamicImage, String>,
    },
    /// A resize+encode completed off-thread (stale ones are dropped by id).
    ImageEncoded(ResizeResponse),
}
