use mtg_deck_builder::api::SourceKind;
use mtg_deck_builder::app::App;
use mtg_deck_builder::config;
use ratatui_image::picker::Picker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let source = match parse_source_flag()? {
        Some(kind) => kind, // per-run override, not persisted
        None => config::load().source,
    };

    // Query the terminal for graphics protocol + font size before entering
    // the alternate screen; fall back to unicode halfblocks anywhere else.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let terminal = ratatui::init();
    let result = App::new(picker, source).run(terminal).await;
    ratatui::restore();
    result
}

/// Parse `--source <scryfall|mtgapi>` (or `--source=<...>`) from argv.
fn parse_source_flag() -> anyhow::Result<Option<SourceKind>> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = if arg == "--source" {
            args.next()
        } else if let Some(v) = arg.strip_prefix("--source=") {
            Some(v.to_string())
        } else if arg == "--help" || arg == "-h" {
            println!("mtg-deck-builder — Commander deck builder TUI");
            println!("\nUsage: mtg-deck-builder [--source scryfall|mtgapi]");
            std::process::exit(0);
        } else {
            continue;
        };
        let value = value.ok_or_else(|| anyhow::anyhow!("--source requires a value"))?;
        return SourceKind::parse(&value)
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("unknown source {value:?} (use scryfall or mtgapi)"));
    }
    Ok(None)
}
