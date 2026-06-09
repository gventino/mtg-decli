use super::Deck;

#[derive(Debug, Default)]
pub struct DeckStats {
    /// Mana curve buckets for CMC 0..=7+ (nonland cards), indexed by CMC.
    pub curve: [u32; 8],
    /// Counts of colored mana symbols across mana costs: W, U, B, R, G, C/other.
    pub color_pips: [u32; 6],
    /// (category, card count) pairs.
    pub type_counts: Vec<(String, u32)>,
    pub lands: u32,
    pub nonlands: u32,
    pub avg_cmc: f64,
}

pub const PIP_LABELS: [&str; 6] = ["W", "U", "B", "R", "G", "C"];

pub fn compute(deck: &Deck) -> DeckStats {
    let mut stats = DeckStats::default();
    let mut cmc_total = 0.0_f64;

    for entry in &deck.entries {
        let card = &entry.card;
        let qty = entry.quantity;
        let is_land = card
            .types
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|t| t == "Land");

        if is_land {
            stats.lands += qty;
        } else {
            stats.nonlands += qty;
            let cmc = card.cmc.unwrap_or(0.0);
            cmc_total += cmc * qty as f64;
            let bucket = (cmc as usize).min(7);
            stats.curve[bucket] += qty;
        }

        if let Some(cost) = &card.mana_cost {
            for _ in 0..qty {
                count_pips(cost, &mut stats.color_pips);
            }
        }
    }

    if stats.nonlands > 0 {
        stats.avg_cmc = cmc_total / stats.nonlands as f64;
    }

    stats.type_counts = deck
        .grouped()
        .into_iter()
        .map(|(name, list)| (name, list.iter().map(|e| e.quantity).sum::<u32>()))
        .filter(|(_, n)| *n > 0)
        .collect();

    stats
}

/// Count colored pips in a mana cost string like "{2}{W}{W}{G/U}".
fn count_pips(cost: &str, pips: &mut [u32; 6]) {
    for symbol in cost.split(['{', '}']).filter(|s| !s.is_empty()) {
        // Hybrid symbols like "G/U" or "2/W" count once per colored letter.
        let mut any_color = false;
        for part in symbol.split('/') {
            match part {
                "W" => { pips[0] += 1; any_color = true; }
                "U" => { pips[1] += 1; any_color = true; }
                "B" => { pips[2] += 1; any_color = true; }
                "R" => { pips[3] += 1; any_color = true; }
                "G" => { pips[4] += 1; any_color = true; }
                _ => {}
            }
        }
        if !any_color && symbol == "C" {
            pips[5] += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Card;
    use crate::deck::DeckEntry;

    fn card(name: &str, types: &[&str], cmc: f64, cost: &str) -> Card {
        Card {
            id: format!("id-{name}"),
            name: name.to_string(),
            types: Some(types.iter().map(|s| s.to_string()).collect()),
            cmc: Some(cmc),
            mana_cost: if cost.is_empty() {
                None
            } else {
                Some(cost.to_string())
            },
            ..Default::default()
        }
    }

    #[test]
    fn curve_and_pips() {
        let mut deck = Deck::new("test");
        deck.entries.push(DeckEntry::new(card("Bolt", &["Instant"], 1.0, "{R}")));
        deck.entries.push(DeckEntry::new(card("Counter", &["Instant"], 2.0, "{U}{U}")));
        deck.entries.push(DeckEntry::new(card("Big", &["Creature"], 9.0, "{7}{G}{G}")));
        deck.entries.push(DeckEntry::new(card("Wastes", &["Land"], 0.0, "")));

        let s = compute(&deck);
        assert_eq!(s.curve[1], 1);
        assert_eq!(s.curve[2], 1);
        assert_eq!(s.curve[7], 1); // 9 cmc clamps to 7+ bucket
        assert_eq!(s.lands, 1);
        assert_eq!(s.nonlands, 3);
        assert_eq!(s.color_pips[3], 1); // R
        assert_eq!(s.color_pips[1], 2); // U
        assert_eq!(s.color_pips[4], 2); // G
        assert!((s.avg_cmc - 4.0).abs() < 1e-9);
    }

    #[test]
    fn hybrid_pips() {
        let mut pips = [0u32; 6];
        count_pips("{G/U}{2/W}{C}", &mut pips);
        assert_eq!(pips[4], 1); // G
        assert_eq!(pips[1], 1); // U
        assert_eq!(pips[0], 1); // W
        assert_eq!(pips[5], 1); // C
    }
}
