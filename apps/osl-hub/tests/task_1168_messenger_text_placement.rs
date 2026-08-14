#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use osl_privacy_hub::website_driver::{
    discover_messenger_browser_conversation, WebsiteConversationBrowserSnapshot,
};
use serde::Deserialize;
use shared_place_text::{place_read_back_and_clear, SharedTextActions};

const DISCOVERY_FIXTURE: &str = include_str!("fixtures/task_1166/browser_conversations.json");
const MARK_FIXTURE: &str = include_str!("fixtures/task_1168/messenger_marked_text.json");
const ORIGINAL_MARKED_TEXT: &str = "[OSL-1168] Messenger exact bytes: cafe + lock";
const SHARED_PLACE_TEXT_SOURCE: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryRecord {
    id: String,
    browser: WebsiteConversationBrowserSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MarkFixture {
    marked_text: String,
}

struct FixtureComposerActions {
    discovered_composer: String,
    connected_composer: String,
    current_text: String,
    placed_snapshot: Vec<u8>,
    place_calls: usize,
    read_calls: usize,
    clear_calls: usize,
}

impl SharedTextActions for FixtureComposerActions {
    fn read_back_text(&mut self) -> Result<String, String> {
        self.read_calls += 1;
        Ok(self.current_text.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        if self.connected_composer != self.discovered_composer {
            return Err(
                "shared place-text target does not equal the discovered composer".to_owned(),
            );
        }
        self.place_calls += 1;
        self.current_text.clear();
        self.current_text.push_str(text);
        self.placed_snapshot = self.current_text.as_bytes().to_vec();
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        if self.connected_composer != self.discovered_composer {
            return Err("shared clear target does not equal the discovered composer".to_owned());
        }
        self.clear_calls += 1;
        self.current_text.clear();
        Ok(())
    }
}

#[test]
fn task_1168_connects_messenger_to_shared_exact_place_readback_and_clear() {
    let records: Vec<DiscoveryRecord> =
        serde_json::from_str(DISCOVERY_FIXTURE).expect("TASK 1166 discovery fixture parses");
    let messenger = records
        .into_iter()
        .find(|record| record.id == "messenger-direct-message")
        .expect("TASK 1166 Messenger record exists");
    let discovery = discover_messenger_browser_conversation(&messenger.browser)
        .expect("TASK 1166 Messenger composer is discovered");
    assert_eq!(discovery.browser_title, "Messenger — Ada Lovelace");
    assert_eq!(discovery.place_kind, "messenger:direct_message");
    assert_eq!(discovery.composer, "Message");

    let mark: MarkFixture =
        serde_json::from_str(MARK_FIXTURE).expect("TASK 1168 marked-text fixture parses");
    assert_eq!(
        mark.marked_text.as_bytes(),
        ORIGINAL_MARKED_TEXT.as_bytes(),
        "TASK 1168 fixture bytes changed"
    );

    let mut actions = FixtureComposerActions {
        discovered_composer: discovery.composer.clone(),
        connected_composer: discovery.composer.clone(),
        current_text: String::new(),
        placed_snapshot: Vec::new(),
        place_calls: 0,
        read_calls: 0,
        clear_calls: 0,
    };
    let receipt = place_read_back_and_clear(&mut actions, &mark.marked_text)
        .expect("shared place-text cycle succeeds for the discovered Messenger composer");

    assert_eq!(actions.placed_snapshot, mark.marked_text.as_bytes());
    assert_eq!(receipt.placed_bytes, mark.marked_text.len());
    assert_eq!(receipt.readback_bytes, mark.marked_text.len());
    assert_eq!(receipt.clear_bytes, 0);
    assert_eq!(actions.current_text.as_bytes().len(), 0);
    assert_eq!(actions.place_calls, 1);
    assert_eq!(actions.read_calls, 3);
    assert_eq!(actions.clear_calls, 1);

    let production = SHARED_PLACE_TEXT_SOURCE
        .split("#[cfg(test)]")
        .next()
        .expect("shared production source exists");
    assert!(production.contains("place_read_back_and_clear(&mut actions, &text)"));
    assert!(production.contains("composer_name.as_deref()"));
    assert!(production.contains("stage_clipboard_text(&text)"));
    assert!(production.contains("send_ctrl_v()?"));
    assert!(production.contains("send_ctrl_a_delete()?"));
    assert!(production.contains("clear_readback.as_bytes().is_empty()"));
    assert!(
        !production.to_ascii_lowercase().contains("messenger"),
        "the shared 3406 job must remain provider-neutral"
    );

    println!("TASK1168_BROWSER_TITLE={}", discovery.browser_title);
    println!("TASK1168_PLACE_KIND={}", discovery.place_kind);
    println!("TASK1168_COMPOSER={}", discovery.composer);
    println!("TASK1168_PLACED_BYTES={}", receipt.placed_bytes);
    println!("TASK1168_READBACK_BYTES={}", receipt.readback_bytes);
    println!("TASK1168_EXACT_READBACK=true");
    println!("TASK1168_CLEAR_BYTES={}", receipt.clear_bytes);
    println!("TASK1168_PLACE_CALLS={}", actions.place_calls);
    println!("TASK1168_READ_CALLS={}", actions.read_calls);
    println!("TASK1168_CLEAR_CALLS={}", actions.clear_calls);
    println!("TASK1168_MESSENGER_ONLY_SHARED_CODE_COUNT=0");
}
