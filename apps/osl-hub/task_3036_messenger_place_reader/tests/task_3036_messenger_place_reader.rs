#[path = "../../src/messenger_place_reader.rs"]
mod messenger_place_reader;

use messenger_place_reader::{
    read_messenger_places_for_scrub, MessengerBrowserConversation, MessengerPlaceKind,
};

fn seeded_rows() -> Vec<MessengerBrowserConversation> {
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

#[test]
fn task_3036_seeded_messenger_browser_rows_are_read_only_for_ticked_account() {
    let places = read_messenger_places_for_scrub(true, "messenger-scrub", seeded_rows())
        .expect("ticked account must return browser rows");
    let unticked = read_messenger_places_for_scrub(false, "messenger-scrub", seeded_rows())
        .expect("unticked account must be empty");

    println!("TASK3036_TICKED_PLACE_COUNT={}", places.len());
    for place in &places {
        println!(
            "TASK3036_PLACE label={} kind={}",
            place.label,
            place.place_kind.as_str()
        );
    }
    println!("TASK3036_UNTICKED_PLACE_COUNT={}", unticked.len());

    assert_eq!(places.len(), 3);
    assert!(places.iter().any(|place| place.label == "SCRUB-M"));
    assert_eq!(places[0].place_kind, MessengerPlaceKind::DirectChat);
    assert_eq!(places[1].place_kind, MessengerPlaceKind::GroupChat);
    assert_eq!(places[2].place_kind, MessengerPlaceKind::Community);
    assert!(unticked.is_empty());
}

#[test]
fn task_3036_duplicate_browser_id_is_refused_when_the_account_is_ticked() {
    let repeated =
        MessengerBrowserConversation::new("dm-scrub-m", "SCRUB-M", MessengerPlaceKind::DirectChat);
    let error =
        read_messenger_places_for_scrub(true, "messenger-scrub", vec![repeated.clone(), repeated])
            .expect_err("duplicate browser rows must not become ambiguous scrub places");
    assert_eq!(
        error,
        "Messenger browser returned a duplicate conversation id"
    );
}
