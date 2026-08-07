use std::collections::BTreeSet;

pub const MUTUAL_DISCOVERY_FIXED_CARD_COUNT: usize = 8;
pub const ONE_SIDED_DISCOVERY_LABEL: &str = "one-sided discovery label";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishedDiscoveryCard {
    pub writer_handle: String,
    pub subject_handle: String,
    pub label: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MutualDiscoveryScan {
    pub machine_handle: String,
    pub peer_handle: String,
    pub received_card_count: usize,
    pub matching_cards: Vec<PublishedDiscoveryCard>,
}

impl MutualDiscoveryScan {
    pub fn status(&self) -> &'static str {
        if self.matching_cards.is_empty() {
            "no match"
        } else {
            "matched"
        }
    }
}

pub fn mutual_discovery_label(first_handle: &str, second_handle: &str) -> Result<String, String> {
    validate_discovery_handle(first_handle)?;
    validate_discovery_handle(second_handle)?;
    if first_handle == second_handle {
        return Err("OSL mutual discovery needs two different handles".to_owned());
    }
    let (first, second) = if first_handle < second_handle {
        (first_handle, second_handle)
    } else {
        (second_handle, first_handle)
    };
    Ok(format!("osl-mutual-discovery-v1:{first}|{second}"))
}

pub fn one_sided_discovery_label(handle: &str) -> Result<String, String> {
    validate_discovery_handle(handle)?;
    Ok(format!("osl-mutual-discovery-v1:{handle}"))
}

pub fn publish_discovery_card(
    writer_handle: &str,
    subject_handle: &str,
) -> Result<PublishedDiscoveryCard, String> {
    Ok(PublishedDiscoveryCard {
        writer_handle: writer_handle.to_owned(),
        subject_handle: subject_handle.to_owned(),
        label: mutual_discovery_label(writer_handle, subject_handle)?,
    })
}

pub fn fixed_discovery_deck(
    mut published_cards: Vec<PublishedDiscoveryCard>,
) -> Result<Vec<PublishedDiscoveryCard>, String> {
    if published_cards.len() > MUTUAL_DISCOVERY_FIXED_CARD_COUNT {
        return Err("OSL mutual discovery deck is over the fixed size".to_owned());
    }
    let mut decoy_index = 0usize;
    while published_cards.len() < MUTUAL_DISCOVERY_FIXED_CARD_COUNT {
        let writer = format!("decoy-writer-{decoy_index:02}");
        let subject = format!("decoy-subject-{decoy_index:02}");
        published_cards.push(publish_discovery_card(&writer, &subject)?);
        decoy_index += 1;
    }
    Ok(published_cards)
}

pub fn scan_mutual_discovery_cards(
    machine_handle: &str,
    peer_handle: &str,
    cards: &[PublishedDiscoveryCard],
) -> Result<MutualDiscoveryScan, String> {
    validate_discovery_handle(machine_handle)?;
    validate_discovery_handle(peer_handle)?;
    if machine_handle == peer_handle {
        return Err("OSL mutual discovery needs two different handles".to_owned());
    }
    if cards.len() != MUTUAL_DISCOVERY_FIXED_CARD_COUNT {
        return Err("OSL mutual discovery requires the fixed card count".to_owned());
    }
    let expected_label = mutual_discovery_label(machine_handle, peer_handle)?;
    let mut directions = BTreeSet::new();
    for card in cards.iter().filter(|card| card.label == expected_label) {
        directions.insert((card.writer_handle.as_str(), card.subject_handle.as_str()));
    }
    let has_local_card = directions.contains(&(machine_handle, peer_handle));
    let has_peer_card = directions.contains(&(peer_handle, machine_handle));
    let matching_cards = if has_local_card && has_peer_card {
        cards
            .iter()
            .filter(|card| {
                card.label == expected_label
                    && card.writer_handle == peer_handle
                    && card.subject_handle == machine_handle
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    Ok(MutualDiscoveryScan {
        machine_handle: machine_handle.to_owned(),
        peer_handle: peer_handle.to_owned(),
        received_card_count: cards.len(),
        matching_cards,
    })
}

fn validate_discovery_handle(handle: &str) -> Result<(), String> {
    if handle.is_empty()
        || handle.len() > 128
        || handle
            .bytes()
            .any(|byte| !byte.is_ascii_graphic() || byte == b'|')
    {
        return Err("OSL mutual discovery handle is invalid".to_owned());
    }
    Ok(())
}
