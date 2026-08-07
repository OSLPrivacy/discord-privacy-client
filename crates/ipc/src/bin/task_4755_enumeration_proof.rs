use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const ATTACKER_HANDLE: &str = "viewer4755@osl.test";
const HANDLE_COUNT: usize = 10_000;
const FIXED_CARD_COUNT: usize = 40;
const DRAWER_COUNT: usize = 4096;
const SOURCE_NOTE: &str = "captured-list-unavailable-fixed-fixture";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LabelMode {
    Pair,
    OneSided,
}

#[derive(Clone, Debug)]
struct Card {
    label: String,
}

#[derive(Debug)]
struct RunReport {
    handles: usize,
    fixed_card_count: usize,
    drawers_fetched: usize,
    cards_looked: usize,
    expected_cards_looked: usize,
    max_drawer_real_cards: usize,
    identified: Vec<String>,
}

fn main() {
    let mode = match std::env::args().nth(1).as_deref() {
        None => LabelMode::Pair,
        Some("--one-sided") => LabelMode::OneSided,
        Some(other) => {
            eprintln!("usage: task_4755_enumeration_proof [--one-sided], got {other}");
            std::process::exit(2);
        }
    };

    let report = run(mode);
    print_report(&report);

    if report.identified.is_empty() {
        std::process::exit(0);
    }
    std::process::exit(1);
}

fn run(mode: LabelMode) -> RunReport {
    let handles = captured_handles();
    let mut drawers: BTreeMap<usize, Vec<Card>> = BTreeMap::new();
    let mut wanted_labels = HashMap::with_capacity(handles.len());

    for handle in &handles {
        let drawer = drawer_for(handle);
        let published_label = match mode {
            LabelMode::Pair => {
                let room_peer = shared_room_peer_for(handle);
                pair_label(handle, &room_peer)
            }
            LabelMode::OneSided => one_sided_label(handle),
        };
        drawers.entry(drawer).or_default().push(Card {
            label: published_label,
        });

        let attacker_label = match mode {
            LabelMode::Pair => pair_label(ATTACKER_HANDLE, handle),
            LabelMode::OneSided => one_sided_label(handle),
        };
        wanted_labels.insert(attacker_label, handle.clone());
    }

    let max_drawer_real_cards = drawers.values().map(Vec::len).max().unwrap_or(0);
    assert!(
        max_drawer_real_cards <= FIXED_CARD_COUNT,
        "drawer overflow would invalidate fixed-card proof: max {max_drawer_real_cards}, fixed {FIXED_CARD_COUNT}"
    );

    let query_drawers = handles
        .iter()
        .map(|handle| drawer_for(handle))
        .collect::<BTreeSet<_>>();
    let mut cards_looked = 0;
    let mut identified = BTreeSet::new();

    for drawer in &query_drawers {
        let cards = padded_drawer(
            drawers.get(drawer).map(Vec::as_slice).unwrap_or(&[]),
            *drawer,
        );
        cards_looked += cards.len();
        for card in cards {
            if let Some(handle) = wanted_labels.get(&card.label) {
                identified.insert(handle.clone());
            }
        }
    }

    RunReport {
        handles: handles.len(),
        fixed_card_count: FIXED_CARD_COUNT,
        drawers_fetched: query_drawers.len(),
        cards_looked,
        expected_cards_looked: FIXED_CARD_COUNT * query_drawers.len(),
        max_drawer_real_cards,
        identified: identified.into_iter().collect(),
    }
}

fn print_report(report: &RunReport) {
    println!("TASK4755 source={SOURCE_NOTE}");
    println!("TASK4755 handles={}", report.handles);
    println!("TASK4755 fixed_card_count={}", report.fixed_card_count);
    println!("TASK4755 drawers_fetched={}", report.drawers_fetched);
    println!(
        "TASK4755 cards_looked={} expected={}",
        report.cards_looked, report.expected_cards_looked
    );
    println!(
        "TASK4755 max_drawer_real_cards={}",
        report.max_drawer_real_cards
    );
    println!(
        "TASK4755 identified {} of {}",
        report.identified.len(),
        report.handles
    );
    if !report.identified.is_empty() {
        let first = report
            .identified
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        println!("TASK4755 first5={first}");
    }
    println!("PLUM-4755 {}", report.identified.len());
}

fn captured_handles() -> Vec<String> {
    const NAMES: &[&str] = &[
        "alex", "amy", "andrea", "anton", "ari", "avery", "bea", "ben", "blair", "cameron",
        "casey", "chris", "dakota", "devon", "drew", "eden", "eli", "emery", "finley", "fran",
        "gray", "harper", "hayden", "jamie", "jules", "kai", "kelly", "kendall", "lee", "logan",
        "morgan", "nico", "parker", "quinn", "reese", "riley", "river", "rowan", "sage", "sam",
        "shawn", "sky", "taylor", "terry",
    ];
    const DOMAINS: &[&str] = &[
        "discord.example",
        "signal.example",
        "telegram.example",
        "instagram.example",
        "whatsapp.example",
        "messenger.example",
        "mail.example",
    ];

    (0..HANDLE_COUNT)
        .map(|index| {
            let name = NAMES[index % NAMES.len()];
            let domain = DOMAINS[(index / NAMES.len()) % DOMAINS.len()];
            let suffix = index + 1;
            format!("{name}.{suffix:05}@{domain}")
        })
        .collect()
}

fn drawer_for(handle: &str) -> usize {
    let digest = Sha256::digest(handle.as_bytes());
    let raw = u16::from_be_bytes([digest[0], digest[1]]) as usize;
    raw % DRAWER_COUNT
}

fn shared_room_peer_for(handle: &str) -> String {
    let drawer = drawer_for(handle);
    format!("room-peer-{drawer:03x}@osl.test")
}

fn one_sided_label(handle: &str) -> String {
    digest_hex("osl-discovery-one-sided-v1", &[handle])
}

fn pair_label(a: &str, b: &str) -> String {
    let (left, right) = if a <= b { (a, b) } else { (b, a) };
    digest_hex("osl-mutual-discovery-v1", &[left, right])
}

fn padded_drawer(real_cards: &[Card], drawer: usize) -> Vec<Card> {
    let mut cards = real_cards.to_vec();
    for index in cards.len()..FIXED_CARD_COUNT {
        cards.push(Card {
            label: digest_hex(
                "osl-discovery-decoy-v1",
                &[&format!("{drawer:03x}"), &index.to_string()],
            ),
        });
    }
    cards
}

fn digest_hex(domain: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update((part.len() as u32).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
