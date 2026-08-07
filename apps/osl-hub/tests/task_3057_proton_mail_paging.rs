#![cfg(feature = "core")]

use osl_privacy_hub::scrub_hosted::proton_mail::{
    read_proton_mailbox_folder_page_through_for_scrub,
    read_proton_mailbox_folder_page_through_for_scrub_with_stop, PROTON_MAIL_PAGE_SIZE,
};
use osl_privacy_hub::scrub_hosted::reader::{
    PolitePace, SharedReaderOptions, SharedReaderStopReason,
};
use osl_privacy_hub::services::{
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};

const OWNER: &str = "osl_task_3057_owner";
const ACCOUNT: &str = "acct-task-3057-proton";
const FOLDER: &str = "Sent";
const SET_PAUSE_MS: u64 = 25;

fn seeded_proton_mailbox_with_120_folder_messages() -> MailboxReaderSnapshot {
    let messages = (1..=120).map(|index| {
        MailboxMessageCandidate::new(
            FOLDER,
            format!("proton-3057-{index:03}"),
            format!("Proton page-through fixture {index:03}"),
            1_786_200_000 + index,
            "scrub.owner@proton.test",
            format!("Proton page-through body {index:03}."),
        )
    });

    MailboxReaderSnapshot::new(
        [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new(FOLDER, FOLDER),
            MailboxFolderCandidate::new("Archive", "Archive"),
            MailboxFolderCandidate::new("Trash", "Trash"),
        ],
        messages,
    )
}

fn every_action_used_set_pause(
    run_pause_ms: u64,
    run: &osl_privacy_hub::scrub_hosted::reader::SharedReaderRun,
) -> bool {
    run.action_log
        .iter()
        .all(|entry| entry.pause_ms == run_pause_ms)
}

fn inter_action_gaps(run: &osl_privacy_hub::scrub_hosted::reader::SharedReaderRun) -> Vec<u64> {
    run.action_log
        .windows(2)
        .map(|pair| pair[1].start_ms.saturating_sub(pair[0].end_ms))
        .collect()
}

#[test]
fn task_3057_proton_pages_folder_with_pause_and_honors_stop_during_page_two() {
    let mailbox = seeded_proton_mailbox_with_120_folder_messages();
    let folder_message_count = mailbox
        .messages
        .iter()
        .filter(|message| message.folder_id == FOLDER)
        .count();

    let mut full_pace = PolitePace::new(SET_PAUSE_MS);
    let full = read_proton_mailbox_folder_page_through_for_scrub(
        OWNER,
        ACCOUNT,
        FOLDER,
        &mailbox,
        SharedReaderOptions { max_pages: 4 },
        &mut full_pace,
    )
    .expect("read Proton Mail folder over shared pages");
    let full_gaps = inter_action_gaps(&full);
    let full_every_gap_is_set_pause = full_gaps.iter().all(|gap| *gap >= SET_PAUSE_MS);
    let full_action_names = full
        .action_log
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect::<Vec<_>>()
        .join(",");

    let mut stop_pace = PolitePace::new(SET_PAUSE_MS);
    let mut stop_requested_during_run = false;
    let stop = read_proton_mailbox_folder_page_through_for_scrub_with_stop(
        OWNER,
        ACCOUNT,
        FOLDER,
        &mailbox,
        SharedReaderOptions { max_pages: 4 },
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
    .expect("stop Proton Mail folder read during page two");
    let stop_read_between_40_and_80 = stop.messages.len() > 40 && stop.messages.len() < 80;
    let stop_action_names = stop
        .action_log
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect::<Vec<_>>()
        .join(",");

    println!("TASK3057_PROVIDER=Proton Mail");
    println!("TASK3057_SHARED_PAGING=read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace");
    println!("TASK3057_FOLDER_ID={FOLDER}");
    println!("TASK3057_FOLDER_MESSAGE_COUNT={folder_message_count}");
    println!("TASK3057_PAGE_SIZE={PROTON_MAIL_PAGE_SIZE}");
    println!("TASK3057_SET_PAUSE_MS={SET_PAUSE_MS}");
    println!("TASK3057_FULL_READ_MESSAGE_COUNT={}", full.messages.len());
    println!("TASK3057_FULL_PAGE_COUNT={}", full.pages_read);
    println!("TASK3057_FULL_STOP_REASON={}", full.stop_reason.as_str());
    println!("TASK3057_FULL_ACTION_NAMES={full_action_names}");
    println!(
        "TASK3057_FULL_EVERY_ACTION_USED_SET_PAUSE={}",
        every_action_used_set_pause(SET_PAUSE_MS, &full)
    );
    println!("TASK3057_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3057_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={full_every_gap_is_set_pause}"
    );
    println!("TASK3057_STOP_REQUESTED_DURING_PAGE=2");
    println!("TASK3057_STOP_REQUESTED_DURING_RUN={stop_requested_during_run}");
    println!("TASK3057_STOP_REASON={}", stop.stop_reason.as_str());
    println!("TASK3057_STOPPED_ON_PAGE_NUMBER={}", stop.pages_read);
    println!("TASK3057_STOP_READ_MESSAGE_COUNT={}", stop.messages.len());
    println!("TASK3057_STOP_PAGE_COUNT={}", stop.pages_read);
    println!("TASK3057_STOP_ACTION_NAMES={stop_action_names}");
    println!("TASK3057_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={stop_read_between_40_and_80}");

    assert_eq!(folder_message_count, 120);
    assert_eq!(full.messages.len(), 120);
    assert!(full.pages_read >= 3);
    assert_eq!(full.pages_read, 4);
    assert_eq!(full.stop_reason, SharedReaderStopReason::PageLimitReached);
    assert!(every_action_used_set_pause(SET_PAUSE_MS, &full));
    assert!(full_every_gap_is_set_pause);
    assert_eq!(stop.stop_reason, SharedReaderStopReason::StopRequested);
    assert!(stop_requested_during_run);
    assert_eq!(stop.pages_read, 2);
    assert_eq!(stop.messages.len(), 60);
    assert!(stop_read_between_40_and_80);
}
