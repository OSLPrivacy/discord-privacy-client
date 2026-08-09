#![cfg(feature = "core")]

use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::services::{
    read_telegram_desktop_shared_messages, save_messaging_risk_agreement,
    telegram_desktop_chat_page_place_for_scrub, TelegramDesktopMachine, TelegramDesktopMessage,
    TelegramDesktopPlace, TelegramDesktopPlaceKind,
};
use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace,
    SharedConversationScrollGate, SharedConversationScrollGateState, SharedConversationScrollPace,
    SharedConversationScrollStop,
};

const PASSWORD: &str = "task-3018-telegram-scroll-stop-password";
const OWNER_LABEL: &str = "task-3018-owner";
const ACCOUNT: &str = "telegram-task-3018-ticked";
const CHAT: &str = "telegram-direct-task-3018";
const PAGE_SIZE: usize = 40;
const SET_PAUSE_MS: u64 = 25;

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3018-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
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

impl Drop for Storage {
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

fn seeded_telegram_chat() -> TelegramDesktopMachine {
    TelegramDesktopMachine::new([TelegramDesktopPlace::new(
        CHAT,
        "Task 3018 Telegram chat",
        TelegramDesktopPlaceKind::DirectChat,
    )])
    // Provider order is reversed: the Telegram reader must establish time order
    // before the shared reader exposes its first 40-message screen.
    .with_messages((1..=120).rev().map(|number| {
        TelegramDesktopMessage::new(
            CHAT,
            format!("telegram-task-3018-{number:03}"),
            format!("Telegram task 3018 message {number:03}"),
            1_786_600_000 + number as i64,
            number % 2 == 0,
        )
    }))
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
fn task_3018_telegram_chat_pages_with_pause_and_stops_during_page_two() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "telegram", ACCOUNT).expect("tick Telegram account");
    let telegram = seeded_telegram_chat();

    let consent_gated_messages =
        read_telegram_desktop_shared_messages(&owner, ACCOUNT, CHAT, &telegram)
            .expect("read selected Telegram chat through its consent gate");
    let mut full_place =
        telegram_desktop_chat_page_place_for_scrub(&owner, ACCOUNT, CHAT, &telegram, PAGE_SIZE)
            .expect("seeded Telegram chat creates a consent-gated paged place");
    let mut full_pace = SharedConversationScrollPace::new(SET_PAUSE_MS);
    let running_gate = StopFlagGate {
        stop_requested: Rc::new(Cell::new(false)),
    };
    let full = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_place,
        10,
        &running_gate,
        &mut full_pace,
    )
    .expect("Telegram chat reads across every visible page");
    let full_gaps = inter_action_gaps_ms(&full);
    let every_full_action_used_set_pause = full
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == SET_PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= SET_PAUSE_MS);

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = Rc::clone(&stop_requested);
    let mut stopped_place =
        telegram_desktop_chat_page_place_for_scrub(&owner, ACCOUNT, CHAT, &telegram, PAGE_SIZE)
            .expect("same Telegram chat creates an independent consent-gated paged place");
    stopped_place.request_stop_when_reading_page(2, move || {
        stop_callback_flag.set(true);
        Ok(())
    });
    let stop_gate = StopFlagGate {
        stop_requested: Rc::clone(&stop_requested),
    };
    let mut stop_pace = SharedConversationScrollPace::new(SET_PAUSE_MS);
    let stopped = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut stopped_place,
        10,
        &stop_gate,
        &mut stop_pace,
    )
    .expect("Telegram chat ends after the page-two stop request");

    println!("TASK3018_PROVIDER=Telegram");
    println!("TASK3018_CHAT_ID={CHAT}");
    println!("TASK3018_SEEDED_MESSAGE_COUNT=120");
    println!(
        "TASK3018_CONSENT_GATED_MESSAGE_COUNT={}",
        consent_gated_messages.len()
    );
    println!("TASK3018_PAGE_SIZE={PAGE_SIZE}");
    println!("TASK3018_SET_PAUSE_MS={SET_PAUSE_MS}");
    println!("TASK3018_FULL_READ_MESSAGE_COUNT={}", full.message_count());
    println!("TASK3018_FULL_PAGE_COUNT={}", full.page_count());
    println!("TASK3018_FULL_STOP_REASON={:?}", full.stop_reason);
    println!(
        "TASK3018_FULL_ONE_SCREEN_SCROLLS={}",
        full_place.one_screen_scroll_count()
    );
    println!("TASK3018_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3018_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3018_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3018_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3018_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3018_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3018_STOP_REASON={:?}", stopped.stop_reason);
    println!(
        "TASK3018_STOPPED_ON_PAGE_NUMBER={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3018_STOP_READ_MESSAGE_COUNT={}",
        stopped.message_count()
    );
    println!("TASK3018_STOP_PAGE_COUNT={}", stopped.page_count());
    println!(
        "TASK3018_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped.message_count())
    );
    println!(
        "TASK3018_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_place.one_screen_scroll_count()
    );

    assert_eq!(consent_gated_messages.len(), 120);
    assert_eq!(full_place.message_count(), 120);
    assert_eq!(full.message_count(), 120);
    assert!(
        full.page_count() >= 3,
        "120 Telegram messages must use at least three pages"
    );
    assert_eq!(full.stop_reason, SharedConversationScrollStop::EndOfPlace);
    assert!(full
        .page_log
        .iter()
        .all(|page| page.messages_on_screen == PAGE_SIZE));
    assert!(every_full_action_used_set_pause);
    assert!(every_full_gap_used_set_pause);
    assert_eq!(full_pace.max_parallel_actions(), 1);
    assert_eq!(
        full.messages
            .first()
            .map(|message| message.message_id.as_str()),
        Some("telegram-task-3018-001")
    );
    assert_eq!(
        full.messages
            .last()
            .map(|message| message.message_id.as_str()),
        Some("telegram-task-3018-120")
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
        "a page-two stop must read between 40 and 80 Telegram messages"
    );
    assert_eq!(stopped.message_count(), 80);
    assert_eq!(stopped_place.one_screen_scroll_count(), 1);
}
