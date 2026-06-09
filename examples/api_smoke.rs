//! Live smoke test for both card sources + image pipeline.
//! Run with: cargo run --example api_smoke -- [scryfall|mtgapi] "sol ring"

use mtg_decli::api::{SourceKind, make_source};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let kind = match args.first().and_then(|a| SourceKind::parse(a)) {
        Some(k) => {
            args.remove(0);
            k
        }
        None => SourceKind::Scryfall,
    };
    let name = args.first().cloned().unwrap_or_else(|| "sol ring".into());
    let source = make_source(kind);

    println!("Source: {} | searching: {name:?}", kind.name());
    let result = source.search(&name, 1).await?;
    println!(
        "Got {} cards (total: {:?}, has_more: {:?})",
        result.cards.len(),
        result.total_count,
        result.has_more
    );

    let Some(card) = result.cards.first() else {
        println!("No cards found.");
        return Ok(());
    };

    println!("\nFirst card:");
    println!("  name:       {}", card.name);
    println!("  type:       {}", card.type_line.as_deref().unwrap_or("-"));
    println!("  supertypes: {:?}", card.supertypes);
    println!("  types:      {:?}", card.types);
    println!("  mana cost:  {}", card.mana_cost.as_deref().unwrap_or("-"));
    println!("  cmc:        {:?}", card.cmc);
    println!("  identity:   {:?}", card.identity());
    println!("  rarity:     {}", card.rarity.as_deref().unwrap_or("-"));
    println!("  set:        {}", card.set_name.as_deref().unwrap_or("-"));
    println!("  cmd legal:  {}", card.is_legal_in_commander());
    println!("  image url:  {}", card.image_url.as_deref().unwrap_or("-"));

    if let Some(url) = &card.image_url {
        println!("\nDownloading image...");
        let bytes = source.download_image(url).await?;
        println!("  downloaded {} bytes", bytes.len());
        let img = image::load_from_memory(&bytes)?;
        println!("  decoded image: {}x{} px", img.width(), img.height());
    }

    Ok(())
}
