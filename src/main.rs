use mtg_deck_builder::app::App;
use ratatui_image::picker::Picker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Query the terminal for graphics protocol + font size before entering
    // the alternate screen; fall back to unicode halfblocks anywhere else.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let terminal = ratatui::init();
    let result = App::new(picker).run(terminal).await;
    ratatui::restore();
    result
}
