use osl_privacy_hub::website_driver::{
    discover_browser_conversation, WebsiteConversationBrowserSnapshot, WebsiteConversationDiscovery,
};
use serde::Deserialize;

const FIXTURE: &str = include_str!("fixtures/task_1166/browser_conversations.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureRecord {
    id: String,
    browser: WebsiteConversationBrowserSnapshot,
    expected: Option<ExpectedDiscovery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExpectedDiscovery {
    browser_title: String,
    place_kind: String,
    composer: String,
}

#[test]
fn task_1166_discovers_only_the_accessible_messenger_direct_message_fixture() {
    let records: Vec<FixtureRecord> = serde_json::from_str(FIXTURE)
        .expect("TASK 1166 fixture must be a serialized browser discovery oracle");
    let fixture_count = records.len();
    let mut discoveries = Vec::new();

    for record in records {
        match (
            discover_browser_conversation(&record.browser),
            record.expected,
        ) {
            (Ok(discovery), Some(expected)) => {
                assert_discovery(&discovery, &expected);
                discoveries.push((record.id, discovery));
            }
            (Err(_), None) => {}
            (Ok(discovery), None) => panic!(
                "fixture without an oracle must fail closed, got {}",
                discovery.place_kind
            ),
            (Err(error), Some(expected)) => panic!(
                "fixture for {} must discover {}, got {error}",
                expected.browser_title, expected.place_kind
            ),
        }
    }

    assert_eq!(
        discoveries.len(),
        2,
        "two complete fixture conversations must be discovered"
    );
    assert_eq!(
        discoveries
            .iter()
            .filter(|(_, discovery)| discovery.place_kind == "messenger:direct_message")
            .count(),
        1,
        "among all fixture records exactly one may be Messenger direct-message"
    );
    let messenger = discoveries
        .iter()
        .find(|(id, _)| id == "messenger-direct-message")
        .map(|(_, discovery)| discovery)
        .expect("prepared Messenger fixture must be discovered");
    let instagram = discoveries
        .iter()
        .find(|(id, _)| id == "instagram-direct-message")
        .map(|(_, discovery)| discovery)
        .expect("prepared Instagram fixture must be discovered");
    let other_messenger_count = discoveries
        .iter()
        .filter(|(id, discovery)| {
            id != "messenger-direct-message" && discovery.place_kind == "messenger:direct_message"
        })
        .count();

    assert_eq!(instagram.place_kind, "instagram:direct_message");
    assert_ne!(instagram.place_kind, messenger.place_kind);
    assert_eq!(other_messenger_count, 0);

    println!("TASK1166_BROWSER_TITLE={}", messenger.browser_title);
    println!("TASK1166_PLACE_KIND={}", messenger.place_kind);
    println!("TASK1166_COMPOSER={}", messenger.composer);
    println!("TASK1166_INSTAGRAM_PLACE_KIND={}", instagram.place_kind);
    println!("TASK1166_FIXTURE_COUNT={fixture_count}");
    println!("TASK1166_MESSENGER_PLACE_KIND_COUNT=1");
    println!("TASK1166_OTHER_FIXTURE_MESSENGER_PLACE_KIND_COUNT={other_messenger_count}");
}

fn assert_discovery(discovery: &WebsiteConversationDiscovery, expected: &ExpectedDiscovery) {
    assert_eq!(discovery.browser_title, expected.browser_title);
    assert_eq!(discovery.place_kind, expected.place_kind);
    assert_eq!(discovery.composer, expected.composer);
}
