#![cfg(feature = "core")]

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::scrub_hosted::reader::{
    PolitePace, SharedReaderOptions, SharedReaderRun, SharedReaderStopReason,
};
use osl_privacy_hub::scrub_hosted::x_thread::{
    read_x_thread_page_through_for_scrub, read_x_thread_page_through_for_scrub_with_stop,
    X_THREAD_PAGE_SIZE,
};
use osl_privacy_hub::services::{
    save_messaging_risk_agreement, XBrowserMachine, XBrowserMessage, XBrowserPlace,
    XBrowserPlaceKind,
};

const PASSWORD: &str = "task-3030-x-thread-paging-password";
const OWNER_LABEL: &str = "task-3030-owner";
const ACCOUNT: &str = "x-task-3030-ticked";
const THREAD: &str = "x-dm-task-3030";
const SET_PAUSE_MS: u64 = 25;

struct Storage(PathBuf);

impl Storage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3030-{}-{}",
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

fn x_thread_with_messages(total_messages: usize) -> XBrowserMachine {
    XBrowserMachine::new([XBrowserPlace::new(
        THREAD,
        "SCRUB-X-3030",
        XBrowserPlaceKind::DirectMessage,
    )])
    .with_messages((1..=total_messages).map(|index| {
        XBrowserMessage::new(
            THREAD,
            format!("x-thread-3030-{index:03}"),
            format!("SCRUB-X-3030 message {index:03}"),
            1_786_000_000 + index as i64,
            index % 2 == 0,
        )
    }))
}

fn inter_action_gaps_ms(run: &SharedReaderRun) -> Vec<u64> {
    run.action_log
        .windows(2)
        .map(|pair| pair[1].start_ms.saturating_sub(pair[0].end_ms))
        .collect()
}

#[test]
fn task_3030_x_thread_pages_with_pause_and_stops_during_page_two() {
    let storage = Storage::new();
    keystore::set_active_account_dir(Some(storage.owner_dir()));
    let owner = keystore::generate_identity(OWNER_LABEL.to_owned())
        .user_id
        .clone();
    save_messaging_risk_agreement(&owner, "x", ACCOUNT).expect("tick X account");
    let browser = x_thread_with_messages(120);

    let mut full_pace = PolitePace::new(SET_PAUSE_MS);
    let full = read_x_thread_page_through_for_scrub(
        &owner,
        ACCOUNT,
        THREAD,
        &browser,
        SharedReaderOptions { max_pages: 10 },
        &mut full_pace,
    )
    .expect("read all X thread pages");
    let full_gaps = inter_action_gaps_ms(&full);
    let every_full_action_used_set_pause = full
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == SET_PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= SET_PAUSE_MS);
    let full_action_names = full
        .action_log
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect::<Vec<_>>()
        .join(",");

    let mut stop_pace = PolitePace::new(SET_PAUSE_MS);
    let mut stop_requested_during_run = false;
    let stopped = read_x_thread_page_through_for_scrub_with_stop(
        &owner,
        ACCOUNT,
        THREAD,
        &browser,
        SharedReaderOptions { max_pages: 10 },
        &mut stop_pace,
        |progress| {
            if progress.page == 2 {
                stop_requested_during_run = true;
                true
            } else {
                false
            }
        },
    )
    .expect("stop X thread during page two");

    println!("TASK3030_PROVIDER=X");
    println!("TASK3030_THREAD_ID={THREAD}");
    println!("TASK3030_SEEDED_MESSAGE_COUNT=120");
    println!("TASK3030_PAGE_SIZE={X_THREAD_PAGE_SIZE}");
    println!("TASK3030_SET_PAUSE_MS={SET_PAUSE_MS}");
    println!("TASK3030_FULL_READ_MESSAGE_COUNT={}", full.messages.len());
    println!("TASK3030_FULL_PAGE_COUNT={}", full.pages_read);
    println!("TASK3030_FULL_STOP_REASON={}", full.stop_reason.as_str());
    println!("TASK3030_FULL_ACTION_NAMES={full_action_names}");
    println!("TASK3030_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3030_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3030_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3030_FULL_MAX_PARALLEL_ACTIONS={}",
        full.max_parallel_actions
    );
    println!("TASK3030_STOP_REQUESTED_DURING_PAGE=2");
    println!("TASK3030_STOP_REQUESTED_DURING_RUN={stop_requested_during_run}");
    println!("TASK3030_STOP_REASON={}", stopped.stop_reason.as_str());
    println!("TASK3030_STOPPED_ON_PAGE_NUMBER={}", stopped.pages_read);
    println!(
        "TASK3030_STOP_READ_MESSAGE_COUNT={}",
        stopped.messages.len()
    );
    println!("TASK3030_STOP_PAGE_COUNT={}", stopped.pages_read);
    println!(
        "TASK3030_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        stopped.messages.len() > 40 && stopped.messages.len() < 80
    );

    assert_eq!(full.messages.len(), 120);
    assert!(
        full.pages_read >= 3,
        "120 X messages must use at least three pages"
    );
    assert_eq!(full.stop_reason, SharedReaderStopReason::EndOfPlace);
    assert!(full
        .page_log
        .iter()
        .all(|page| page.messages_on_screen == X_THREAD_PAGE_SIZE));
    assert!(every_full_action_used_set_pause);
    assert!(every_full_gap_used_set_pause);
    assert_eq!(full.max_parallel_actions, 1);

    assert!(stop_requested_during_run);
    assert_eq!(stopped.stop_reason, SharedReaderStopReason::StopRequested);
    assert_eq!(stopped.pages_read, 2);
    assert!(
        stopped.messages.len() > 40 && stopped.messages.len() < 80,
        "a page-two stop must read between 40 and 80 X messages"
    );
}
