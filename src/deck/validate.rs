use std::collections::HashSet;

use super::Deck;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    NoCommander,
    CommanderNotLegendaryCreature,
    WrongCardCount { count: u32 },
    DuplicateNonBasic { name: String },
    OutsideColorIdentity { name: String },
    NotCommanderLegal { name: String },
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::NoCommander => write!(f, "No commander selected"),
            Issue::CommanderNotLegendaryCreature => {
                write!(f, "Commander is not a legendary creature")
            }
            Issue::WrongCardCount { count } => write!(f, "Deck has {count}/100 cards"),
            Issue::DuplicateNonBasic { name } => write!(f, "Duplicate non-basic: {name}"),
            Issue::OutsideColorIdentity { name } => {
                write!(f, "Outside commander color identity: {name}")
            }
            Issue::NotCommanderLegal { name } => write!(f, "Not legal in Commander: {name}"),
        }
    }
}

/// Validate a deck against Commander deck-construction rules.
pub fn validate(deck: &Deck) -> Vec<Issue> {
    let mut issues = Vec::new();

    let commander_identity: Option<HashSet<String>> = match &deck.commander {
        Some(cmdr) => {
            if !cmdr.can_be_commander() {
                issues.push(Issue::CommanderNotLegendaryCreature);
            }
            if has_legalities(deck) && !cmdr.is_legal_in_commander() {
                issues.push(Issue::NotCommanderLegal {
                    name: cmdr.name.clone(),
                });
            }
            Some(cmdr.identity().into_iter().collect())
        }
        None => {
            issues.push(Issue::NoCommander);
            None
        }
    };

    let count = deck.card_count();
    if count != 100 {
        issues.push(Issue::WrongCardCount { count });
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for entry in &deck.entries {
        let card = &entry.card;
        let basic = card.is_basic();

        if !basic && (entry.quantity > 1 || !seen.insert(card.name.as_str())) {
            issues.push(Issue::DuplicateNonBasic {
                name: card.name.clone(),
            });
        }

        if let Some(identity) = &commander_identity
            && !card.identity().iter().all(|c| identity.contains(c)) {
                issues.push(Issue::OutsideColorIdentity {
                    name: card.name.clone(),
                });
            }

        // Only flag legality when the API provided legality data for the card.
        if card.legalities.is_some() && !card.is_legal_in_commander() {
            issues.push(Issue::NotCommanderLegal {
                name: card.name.clone(),
            });
        }
    }

    issues
}

fn has_legalities(deck: &Deck) -> bool {
    deck.commander
        .as_ref()
        .is_some_and(|c| c.legalities.is_some())
}

pub fn is_valid(deck: &Deck) -> bool {
    validate(deck).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::Card;

    fn card(name: &str, types: &[&str], supertypes: &[&str], identity: &[&str]) -> Card {
        Card {
            id: format!("id-{name}"),
            name: name.to_string(),
            types: Some(types.iter().map(|s| s.to_string()).collect()),
            supertypes: Some(supertypes.iter().map(|s| s.to_string()).collect()),
            color_identity: Some(identity.iter().map(|s| s.to_string()).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn flags_missing_commander_and_count() {
        let deck = Deck::new("test");
        let issues = validate(&deck);
        assert!(issues.contains(&Issue::NoCommander));
        assert!(issues.contains(&Issue::WrongCardCount { count: 0 }));
    }

    #[test]
    fn flags_color_identity_violation() {
        let mut deck = Deck::new("test");
        deck.set_commander(card("Krenko", &["Creature"], &["Legendary"], &["R"]));
        deck.add_card(card("Counterspell", &["Instant"], &[], &["U"]));
        let issues = validate(&deck);
        assert!(issues.contains(&Issue::OutsideColorIdentity {
            name: "Counterspell".to_string()
        }));
    }

    #[test]
    fn valid_100_card_mono_deck() {
        let mut deck = Deck::new("test");
        deck.set_commander(card("Krenko", &["Creature"], &["Legendary"], &["R"]));
        deck.add_card(card("Shock", &["Instant"], &[], &["R"]));
        let mountain = card("Mountain", &["Land"], &["Basic"], &[]);
        for _ in 0..98 {
            deck.add_card(mountain.clone());
        }
        assert_eq!(deck.card_count(), 100);
        assert!(is_valid(&deck), "issues: {:?}", validate(&deck));
    }

    #[test]
    fn flags_non_legendary_commander() {
        let mut deck = Deck::new("test");
        deck.set_commander(card("Grizzly Bears", &["Creature"], &[], &["G"]));
        assert!(validate(&deck).contains(&Issue::CommanderNotLegendaryCreature));
    }
}
