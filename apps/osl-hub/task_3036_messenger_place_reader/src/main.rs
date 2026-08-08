//! TASK 3036 browser-machine check for Messenger's shared place reader.
//!
//! Run with:
//! `cargo run --manifest-path apps/osl-hub/task_3036_messenger_place_reader/Cargo.toml`

#[path = "../../src/messenger_place_reader.rs"]
mod messenger_place_reader;

use messenger_place_reader::{
    read_messenger_places_for_scrub, MessengerBrowserConversation, MessengerPlaceKind,
};

const ACCOUNT_ID: &str = "messenger-scrub";

fn seeded_browser_rows() -> Vec<MessengerBrowserConversation> {
    vec![
        MessengerBrowserConversation::new("dm-scrub-m", "SCRUB-M", MessengerPlaceKind::DirectChat),
        MessengerBrowserConversation::new(
            "group-scrub-m",
            "SCRUB-M group",
            MessengerPlaceKind::GroupChat,
        ),
        MessengerBrowserConversation::new(
            "community-scrub-m",
            "SCRUB-M community",
            MessengerPlaceKind::Community,
        ),
    ]
}

fn main() {
    let ticked = read_messenger_places_for_scrub(true, ACCOUNT_ID, seeded_browser_rows())
        .expect("seeded ticked Messenger account must read");
    let unticked = read_messenger_places_for_scrub(false, ACCOUNT_ID, seeded_browser_rows())
        .expect("seeded unticked Messenger account must be empty");

    println!("TASK3036_BROWSER_MACHINE=messenger");
    println!("TASK3036_TICKED_PLACE_COUNT={}", ticked.len());
    for place in &ticked {
        println!(
            "TASK3036_PLACE id={} label={} kind={} service={} account={}",
            place.place_id,
            place.label,
            place.place_kind.as_str(),
            place.service_id,
            place.account_id,
        );
    }
    println!("TASK3036_UNTICKED_PLACE_COUNT={}", unticked.len());

    let passes = ticked.len() == 3
        && ticked.iter().any(|place| {
            place.label == "SCRUB-M" && place.place_kind == MessengerPlaceKind::DirectChat
        })
        && ticked
            .iter()
            .any(|place| place.place_kind == MessengerPlaceKind::GroupChat)
        && ticked
            .iter()
            .any(|place| place.place_kind == MessengerPlaceKind::Community)
        && unticked.is_empty();
    if passes {
        println!("TASK3036_RESULT=ok");
    } else {
        eprintln!("TASK3036_RESULT=failed");
        std::process::exit(1);
    }
}
