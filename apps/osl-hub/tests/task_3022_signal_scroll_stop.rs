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
use osl_privacy_hub::signal_message_reader::{
    SignalOpenScreenMessage, SignalOpenScreenSnapshot, SignalOpenScreenSource,
    SignalScreenReadAction,
};
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};
use osl_privacy_hub::signal_scroll_reader::{
    SignalOneScreenScrollSource, SignalScrubChatPagePlace,
};

const PASSWORD: &str = "task-3022-signal-scroll-stop-password";
const OWNER_LABEL: &str = "task-3022-owner";
const ACCOUNT: &str = "signal-task-3022-ticked";
const SIGNED_IN_SENDER: &str = "signal-task-3022-owner";
const PAGE_SIZE: usize = 40;
const SET_PAUSE_MS: u64 = 25;

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3022-{}-{}",
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

struct SeededSignalChat {
    pages: Vec<SignalOpenScreenSnapshot>,
    current_page: usize,
    action_log: Vec<SignalScreenReadAction>,
}

impl SeededSignalChat {
    fn with_messages(place_id: &str, total_messages: usize) -> Self {
        let pages = (1..=total_messages)
            .map(|number| {
                SignalOpenScreenMessage::new(
                    format!("signal-task-3022-{number:03}"),
                    format!("Signal task 3022 message {number:03}"),
                    1_786_700_000 + number as i64,
                    if number % 2 == 0 {
                        SIGNED_IN_SENDER
                    } else {
                        "signal-task-3022-friend"
                    },
                )
            })
            .collect::<Vec<_>>();
        Self {
            pages: pages
                .chunks(PAGE_SIZE)
                .map(|page| SignalOpenScreenSnapshot::new(place_id, page.iter().cloned()))
                .collect(),
            current_page: 0,
            action_log: Vec::new(),
        }
    }
}

impl SignalOpenScreenSource for SeededSignalChat {
    fn read_open_screen(&mut self) -> Result<SignalOpenScreenSnapshot, String> {
        self.action_log.push(SignalScreenReadAction::ReadOpenScreen);
        self.pages
            .get(self.current_page)
            .cloned()
            .ok_or_else(|| "Signal seeded chat has no current screen".to_owned())
    }

    fn action_log(&self) -> &[SignalScreenReadAction] {
        &self.action_log
    }
}

impl SignalOneScreenScrollSource for SeededSignalChat {
    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        self.action_log
            .push(SignalScreenReadAction::ScrollOneScreen);
        if self.current_page + 1 >= self.pages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        Ok(true)
    }
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
fn task_3022_signal_chat_pages_with_pause_and_stops_during_page_two() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "signal", ACCOUNT).expect("tick Signal account");
    let places =
        read_signal_conversation_places(&owner, ACCOUNT, &seeded_signal_conversation_places())
            .expect("read seeded Signal places");
    let chat = places
        .iter()
        .find(|place| place.label == "SCRUB-S")
        .expect("find the seeded Signal chat");

    let mut full_source = SeededSignalChat::with_messages(&chat.place_id, 120);
    let mut full_place =
        SignalScrubChatPagePlace::new(&owner, ACCOUNT, chat, SIGNED_IN_SENDER, &mut full_source);
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
    .expect("Signal chat reads every screen one page at a time");
    let full_gaps = inter_action_gaps_ms(&full);
    let every_full_action_used_set_pause = full
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == SET_PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= SET_PAUSE_MS);
    let full_source_actions = full_place.source_action_log();

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = Rc::clone(&stop_requested);
    let mut stop_source = SeededSignalChat::with_messages(&chat.place_id, 120);
    let mut stopped_place =
        SignalScrubChatPagePlace::new(&owner, ACCOUNT, chat, SIGNED_IN_SENDER, &mut stop_source);
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
    .expect("Signal chat stops after the page-two request");
    let stopped_source_actions = stopped_place.source_action_log();

    println!("TASK3022_PROVIDER=Signal");
    println!("TASK3022_CHAT_ID={}", chat.place_id);
    println!("TASK3022_SEEDED_MESSAGE_COUNT=120");
    println!("TASK3022_PAGE_SIZE={PAGE_SIZE}");
    println!("TASK3022_SET_PAUSE_MS={SET_PAUSE_MS}");
    println!("TASK3022_FULL_READ_MESSAGE_COUNT={}", full.message_count());
    println!("TASK3022_FULL_PAGE_COUNT={}", full.page_count());
    println!("TASK3022_FULL_STOP_REASON={:?}", full.stop_reason);
    println!(
        "TASK3022_FULL_ONE_SCREEN_SCROLLS={}",
        full_place.one_screen_scroll_count()
    );
    println!("TASK3022_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3022_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3022_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3022_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3022_FULL_SOURCE_ACTIONS={full_source_actions:?}");
    println!("TASK3022_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3022_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3022_STOP_REASON={:?}", stopped.stop_reason);
    println!(
        "TASK3022_STOPPED_ON_PAGE_NUMBER={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3022_STOP_READ_MESSAGE_COUNT={}",
        stopped.message_count()
    );
    println!("TASK3022_STOP_PAGE_COUNT={}", stopped.page_count());
    println!(
        "TASK3022_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped.message_count())
    );
    println!(
        "TASK3022_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_place.one_screen_scroll_count()
    );
    println!("TASK3022_STOP_SOURCE_ACTIONS={stopped_source_actions:?}");

    assert_eq!(places.len(), 3);
    assert_eq!(full.message_count(), 120);
    assert!(
        full.page_count() >= 3,
        "120 Signal messages must use at least three pages"
    );
    assert_eq!(full.stop_reason, SharedConversationScrollStop::EndOfPlace);
    assert!(full
        .page_log
        .iter()
        .all(|page| page.messages_on_screen == PAGE_SIZE));
    assert!(every_full_action_used_set_pause);
    assert!(every_full_gap_used_set_pause);
    assert_eq!(full_pace.max_parallel_actions(), 1);
    assert_eq!(full_place.one_screen_scroll_count(), 2);
    assert_eq!(full_place.pages_read(), 3);
    assert_eq!(
        full_source_actions,
        [
            SignalScreenReadAction::ReadOpenScreen,
            SignalScreenReadAction::ScrollOneScreen,
            SignalScreenReadAction::ReadOpenScreen,
            SignalScreenReadAction::ScrollOneScreen,
            SignalScreenReadAction::ReadOpenScreen,
            SignalScreenReadAction::ScrollOneScreen,
        ]
    );
    assert_eq!(
        full.messages
            .first()
            .map(|message| message.message_id.as_str()),
        Some("signal-task-3022-001")
    );
    assert_eq!(
        full.messages
            .last()
            .map(|message| message.message_id.as_str()),
        Some("signal-task-3022-120")
    );

    assert!(stop_requested.get());
    assert_eq!(
        stopped.stop_reason,
        SharedConversationScrollStop::StopRequested
    );
    assert_eq!(stopped.stopped_on_page_number, Some(2));
    assert_eq!(stopped.page_count(), 2);
    assert!((40..=80).contains(&stopped.message_count()));
    assert_eq!(stopped.message_count(), 80);
    assert_eq!(stopped_place.one_screen_scroll_count(), 1);
    assert_eq!(stopped_place.pages_read(), 2);
    assert_eq!(
        stopped_source_actions,
        [
            SignalScreenReadAction::ReadOpenScreen,
            SignalScreenReadAction::ScrollOneScreen,
            SignalScreenReadAction::ReadOpenScreen,
        ]
    );
}
