//! Live smoke test for the MTG API client + image pipeline.
//! Run with: cargo run --example api_smoke -- "sol ring"

use mtg_deck_builder::api::client::{MtgClient, SearchQuery};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "sol ring".into());
    let client = MtgClient::new();

    println!("Searching for: {name:?}");
    let result = client.search(&SearchQuery::by_name(&name)).await?;
    println!(
        "Got {} unique cards (total printings: {:?})",
        result.cards.len(),
        result.total_count
    );

    let Some(card) = result.cards.first() else {
        println!("No cards found.");
        return Ok(());
    };

    println!("\nFirst card:");
    println!("  name:       {}", card.name);
    println!("  type:       {}", card.type_line.as_deref().unwrap_or("-"));
    println!("  mana cost:  {}", card.mana_cost.as_deref().unwrap_or("-"));
    println!("  cmc:        {:?}", card.cmc);
    println!("  identity:   {:?}", card.identity());
    println!("  set:        {}", card.set_name.as_deref().unwrap_or("-"));
    println!("  cmd legal:  {}", card.is_legal_in_commander());
    println!("  image url:  {}", card.image_url.as_deref().unwrap_or("-"));
    println!(
        "  text:       {}",
        card.text.as_deref().unwrap_or("-").replace('\n', "\n              ")
    );

    if let Some(url) = &card.image_url {
        println!("\nDownloading image...");
        let bytes = client.download_image(url).await?;
        println!("  downloaded {} bytes", bytes.len());
        let img = image::load_from_memory(&bytes)?;
        println!("  decoded image: {}x{} px", img.width(), img.height());
    }

    Ok(())
}
