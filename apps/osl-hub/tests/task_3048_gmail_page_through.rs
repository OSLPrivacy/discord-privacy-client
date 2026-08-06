use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace,
    SharedConversationScrollGate, SharedConversationScrollGateState, SharedConversationScrollPace,
    SharedConversationScrollStop,
};
use osl_privacy_hub::shared_mailbox_reader::{
    SharedMailboxFolder, SharedMailboxMessage, SharedMailboxMessageSummary, SharedMailboxReader,
};

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

fn gmail_mailbox_with_label(total_messages: usize) -> SharedMailboxReader {
    let account_id = "acct-task-3048-gmail";
    let label = SharedMailboxFolder::gmail_label("task-3048-label", account_id);
    let mut messages_by_folder = BTreeMap::new();
    messages_by_folder.insert(
        label.id.clone(),
        (1..=total_messages)
            .map(|index| {
                SharedMailboxMessage::new(
                    SharedMailboxMessageSummary::new(
                        format!("gmail-task-3048-{index:03}"),
                        format!("Task 3048 Gmail label message {index:03}"),
                        1_786_200_000 + index as i64,
                        "owner@gmail.example",
                    ),
                    format!("body for task 3048 Gmail message {index:03}"),
                )
            })
            .collect(),
    );
    SharedMailboxReader::new(vec![label], messages_by_folder).expect("Gmail fixture is valid")
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
fn task_3048_gmail_label_pages_through_with_pause_and_stops_during_page_two() {
    let mailbox = gmail_mailbox_with_label(120);
    let labels = mailbox.list_gmail_labels();
    let mut full_label = mailbox
        .gmail_label_page_place("task-3048-label", 40)
        .expect("Gmail label exists");
    let label_message_count = full_label.message_count();
    let mut full_pace = SharedConversationScrollPace::new(25);
    let running_gate = StopFlagGate {
        stop_requested: Rc::new(Cell::new(false)),
    };

    let full_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_label,
        10,
        &running_gate,
        &mut full_pace,
    )
    .expect("shared reader pages through the Gmail label");
    let full_gaps = inter_action_gaps_ms(&full_read);
    let every_full_action_used_set_pause = full_read
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == 25);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= 25);

    println!("TASK3048_SHARED_HELPER=read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace");
    println!("TASK3048_GMAIL_LABEL_HELPER=gmail_label_page_place");
    println!("TASK3048_GMAIL_LABEL_COUNT={}", labels.len());
    println!("TASK3048_GMAIL_LABEL_ID={}", full_label.folder().id);
    println!(
        "TASK3048_GMAIL_LABEL_SERVICE={}",
        full_label.folder().service
    );
    println!("TASK3048_GMAIL_LABEL_MESSAGE_COUNT={label_message_count}");
    println!("TASK3048_SET_PAUSE_MS={}", full_pace.pause_ms());
    println!(
        "TASK3048_FULL_READ_MESSAGE_COUNT={}",
        full_read.message_count()
    );
    println!("TASK3048_FULL_PAGE_COUNT={}", full_read.page_count());
    println!("TASK3048_FULL_STOP_REASON={:?}", full_read.stop_reason);
    println!(
        "TASK3048_FULL_ONE_SCREEN_SCROLLS={}",
        full_label.one_screen_scroll_count()
    );
    println!("TASK3048_FULL_ACTION_COUNT={}", full_read.action_log.len());
    println!("TASK3048_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3048_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3048_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3048_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3048_FULL_TOTAL_RUN_MS={}", full_pace.elapsed_ms());

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = stop_requested.clone();
    let mut stopped_label = mailbox
        .gmail_label_page_place("task-3048-label", 40)
        .expect("Gmail label exists for stop run");
    stopped_label.request_stop_when_reading_page(2, move || {
        stop_callback_flag.set(true);
        Ok(())
    });
    let stop_gate = StopFlagGate {
        stop_requested: stop_requested.clone(),
    };
    let mut stop_pace = SharedConversationScrollPace::new(25);
    let stopped_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut stopped_label,
        10,
        &stop_gate,
        &mut stop_pace,
    )
    .expect("shared reader stops the Gmail label after page two");

    println!("TASK3048_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3048_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3048_STOP_REASON={:?}", stopped_read.stop_reason);
    println!(
        "TASK3048_STOPPED_ON_PAGE_NUMBER={}",
        stopped_read.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3048_STOP_LOG_RECORDED_PAGE={}",
        stopped_read
            .page_log
            .last()
            .map(|page| page.page_number)
            .unwrap_or_default()
    );
    println!(
        "TASK3048_STOP_GATE_AFTER_PAGE={:?}",
        stopped_read
            .page_log
            .last()
            .map(|page| page.gate_after_page)
            .unwrap_or(SharedConversationScrollGateState::Running)
    );
    println!(
        "TASK3048_STOP_READ_MESSAGE_COUNT={}",
        stopped_read.message_count()
    );
    println!("TASK3048_STOP_PAGE_COUNT={}", stopped_read.page_count());
    println!(
        "TASK3048_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped_read.message_count())
    );
    println!(
        "TASK3048_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_label.one_screen_scroll_count()
    );

    assert_eq!(labels.len(), 1);
    assert_eq!(full_label.folder().service, "gmail");
    assert_eq!(label_message_count, 120);
    assert_eq!(full_read.message_count(), 120);
    assert!(
        full_read.page_count() >= 3,
        "120 Gmail messages must be read over at least three pages"
    );
    assert_eq!(
        full_read.stop_reason,
        SharedConversationScrollStop::EndOfPlace
    );
    assert_eq!(full_pace.pause_ms(), 25);
    assert!(every_full_action_used_set_pause);
    assert!(every_full_gap_used_set_pause);
    assert_eq!(full_pace.max_parallel_actions(), 1);

    assert!(stop_requested.get());
    assert_eq!(
        stopped_read.stop_reason,
        SharedConversationScrollStop::StopRequested
    );
    assert_eq!(stopped_read.stopped_on_page_number, Some(2));
    assert_eq!(stopped_read.page_count(), 2);
    assert!(
        (40..=80).contains(&stopped_read.message_count()),
        "stop during page two must end with between 40 and 80 messages read"
    );
}
