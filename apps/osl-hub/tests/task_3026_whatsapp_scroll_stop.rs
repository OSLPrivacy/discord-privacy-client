#![cfg(feature = "core")]

use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace,
    SharedConversationScrollGate, SharedConversationScrollGateState, SharedConversationScrollPace,
    SharedConversationScrollStop,
};
use osl_privacy_hub::whatsapp_message_reader::{
    whatsapp_conversation_page_place_for_scrub, WhatsAppBrowserMessage,
};

const PASSWORD: &str = "task-3026-whatsapp-scroll-stop";
const OWNER_LABEL: &str = "task-3026-owner";
const ACCOUNT: &str = "whatsapp-task-3026-ticked";
const PLACE: &str = "whatsapp-dm-task-3026";
const SIGNED_IN_AUTHOR: &str = "whatsapp-task-3026-owner";
const PAGE_SIZE: usize = 40;
const PAUSE_MS: u64 = 25;

struct AccountStorage(PathBuf);

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3026-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, PASSWORD).expect("set main password");
        Self(root)
    }

    fn owner_dir(&self) -> PathBuf {
        let path = self.0.join("owner");
        fs::create_dir(&path).expect("create owner directory");
        path
    }
}

impl Drop for AccountStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct StopFlagGate {
    stop_requested: Rc<Cell<bool>>,
}

impl SharedConversationScrollGate for StopFlagGate {
    fn state_between_pages(&self) -> Result<SharedConversationScrollGateState, String> {
        Ok(if self.stop_requested.get() {
            SharedConversationScrollGateState::StopAfterCurrentPage
        } else {
            SharedConversationScrollGateState::Running
        })
    }
}

fn seeded_whatsapp_rows(total_messages: usize) -> Vec<WhatsAppBrowserMessage> {
    // Reverse provider order to prove that the WhatsApp reader establishes the
    // chronology before the shared page reader starts exposing screens.
    (1..=total_messages)
        .rev()
        .map(|number| {
            WhatsAppBrowserMessage::new(
                PLACE,
                format!("whatsapp-task-3026-{number:03}"),
                format!("WhatsApp task 3026 message {number:03}"),
                1_786_600_000 + number as i64,
                if number % 2 == 0 {
                    SIGNED_IN_AUTHOR
                } else {
                    "whatsapp-task-3026-friend"
                },
            )
        })
        .collect()
}

fn inter_action_gaps_ms(
    read: &osl_privacy_hub::shared_conversation_scroll::SharedConversationScrollRead,
) -> Vec<u64> {
    read.action_log
        .windows(2)
        .map(|pair| pair[1].started_at_ms.saturating_sub(pair[0].finished_at_ms))
        .collect()
}

#[test]
fn task_3026_whatsapp_pages_with_pause_and_stops_during_page_two() {
    let storage = AccountStorage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "whatsapp", ACCOUNT).expect("tick WhatsApp account");

    let mut full_place = whatsapp_conversation_page_place_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        seeded_whatsapp_rows(120),
        PAGE_SIZE,
    )
    .expect("seeded WhatsApp chat creates a paged place");
    let seeded_message_count = full_place.message_count();
    let running_gate = StopFlagGate {
        stop_requested: Rc::new(Cell::new(false)),
    };
    let mut full_pace = SharedConversationScrollPace::new(PAUSE_MS);
    let full = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_place,
        10,
        &running_gate,
        &mut full_pace,
    )
    .expect("WhatsApp pages through the full seeded chat");
    let full_gaps = inter_action_gaps_ms(&full);
    let every_full_action_used_set_pause = full
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= PAUSE_MS);

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = Rc::clone(&stop_requested);
    let mut stopped_place = whatsapp_conversation_page_place_for_scrub(
        &owner,
        ACCOUNT,
        PLACE,
        true,
        SIGNED_IN_AUTHOR,
        seeded_whatsapp_rows(120),
        PAGE_SIZE,
    )
    .expect("same WhatsApp chat creates an independent paged place");
    stopped_place.request_stop_when_reading_page(2, move || {
        stop_callback_flag.set(true);
        Ok(())
    });
    let stop_gate = StopFlagGate {
        stop_requested: Rc::clone(&stop_requested),
    };
    let mut stop_pace = SharedConversationScrollPace::new(PAUSE_MS);
    let stopped = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut stopped_place,
        10,
        &stop_gate,
        &mut stop_pace,
    )
    .expect("WhatsApp stops after the page-two stop request");

    println!("TASK3026_SHARED_HELPER=read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace");
    println!("TASK3026_WHATSAPP_HELPER=whatsapp_conversation_page_place_for_scrub");
    println!("TASK3026_SEEDED_MESSAGE_COUNT={seeded_message_count}");
    println!("TASK3026_PAGE_SIZE={PAGE_SIZE}");
    println!("TASK3026_SET_PAUSE_MS={}", full_pace.pause_ms());
    println!("TASK3026_FULL_READ_MESSAGE_COUNT={}", full.message_count());
    println!("TASK3026_FULL_PAGE_COUNT={}", full.page_count());
    println!("TASK3026_FULL_STOP_REASON={:?}", full.stop_reason);
    println!(
        "TASK3026_FULL_ONE_SCREEN_SCROLLS={}",
        full_place.one_screen_scroll_count()
    );
    println!("TASK3026_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3026_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3026_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3026_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3026_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3026_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3026_STOP_REASON={:?}", stopped.stop_reason);
    println!(
        "TASK3026_STOPPED_ON_PAGE_NUMBER={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3026_STOP_READ_MESSAGE_COUNT={}",
        stopped.message_count()
    );
    println!("TASK3026_STOP_PAGE_COUNT={}", stopped.page_count());
    println!(
        "TASK3026_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped.message_count())
    );
    println!(
        "TASK3026_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_place.one_screen_scroll_count()
    );

    assert_eq!(seeded_message_count, 120);
    assert_eq!(full.message_count(), 120);
    assert!(
        full.page_count() >= 3,
        "120 WhatsApp messages must use at least three pages"
    );
    assert_eq!(full.stop_reason, SharedConversationScrollStop::EndOfPlace);
    assert_eq!(full_pace.pause_ms(), PAUSE_MS);
    assert!(every_full_action_used_set_pause);
    assert!(every_full_gap_used_set_pause);
    assert_eq!(full_pace.max_parallel_actions(), 1);
    assert_eq!(
        full.messages
            .first()
            .map(|message| message.message_id.as_str()),
        Some("whatsapp-task-3026-001")
    );
    assert_eq!(
        full.messages
            .last()
            .map(|message| message.message_id.as_str()),
        Some("whatsapp-task-3026-120")
    );

    assert!(stop_requested.get());
    assert_eq!(
        stopped.stop_reason,
        SharedConversationScrollStop::StopRequested
    );
    assert_eq!(stopped.stopped_on_page_number, Some(2));
    assert_eq!(stopped.page_count(), 2);
    assert!(
        (40..=80).contains(&stopped.message_count()),
        "stop during page two must end with between 40 and 80 messages read"
    );
    assert_eq!(stopped.message_count(), 80);
    assert_eq!(stopped_place.one_screen_scroll_count(), 1);
}
