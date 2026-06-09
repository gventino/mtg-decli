use image::DynamicImage;

use crate::api::client::SearchResult;

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
}
