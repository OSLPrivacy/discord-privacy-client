#![cfg(feature = "core")]

use osl_privacy_hub::native_telegram_adapter::{
    find_active_telegram_conversation_window, TelegramConversationWindowRecord,
};

fn window(
    window_id: u64,
    process_name: &str,
    conversation_name: &str,
    is_open: bool,
    is_foreground: bool,
) -> TelegramConversationWindowRecord {
    TelegramConversationWindowRecord {
        window_id,
        process_name: process_name.to_owned(),
        conversation_name: conversation_name.to_owned(),
        is_open,
        is_foreground,
    }
}

fn prepared_fixture(open_conversation_name: &str) -> Vec<TelegramConversationWindowRecord> {
    vec![
        window(11, "Telegram.exe", "Archived discussion", true, false),
        window(12, "Telegram", open_conversation_name, true, true),
        window(13, "Telegram", "Closed conversation", false, true),
        window(14, "not-telegram", "Other app", true, true),
    ]
}

#[test]
fn task_1000_finds_the_fixture_open_telegram_conversation_and_tracks_a_rename() {
    let original_name = "Task 1000 — Élodie & Martín";
    let renamed_name = "Task 1000 — renamed conversation";

    let records = prepared_fixture(original_name);
    let matches = records
        .iter()
        .filter(|record| {
            (record.process_name == "Telegram" || record.process_name == "Telegram.exe")
                && record.is_open
                && record.is_foreground
                && !record.conversation_name.trim().is_empty()
        })
        .count();
    let found = find_active_telegram_conversation_window(&records)
        .expect("prepared fixture has exactly one open Telegram conversation");

    println!("TASK1000_COMMAND=find_active_telegram_conversation_window");
    println!("TASK1000_OPEN_CONVERSATION={found}");
    println!("TASK1000_MATCHING_RECORDS={matches}");
    assert_eq!(found, original_name);
    assert_eq!(matches, 1);

    let renamed_records = prepared_fixture(renamed_name);
    let renamed_found = find_active_telegram_conversation_window(&renamed_records)
        .expect("renamed fixture still has exactly one open Telegram conversation");
    println!("TASK1000_RENAMED_OPEN_CONVERSATION={renamed_found}");
    assert_eq!(renamed_found, renamed_name);
    assert_ne!(renamed_found, found);
}
