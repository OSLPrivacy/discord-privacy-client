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

fn yahoo_mailbox_with_folder(total_messages: usize) -> SharedMailboxReader {
    let account_id = "acct-task-3060-yahoo";
    let folder = SharedMailboxFolder::yahoo_folder("task-3060-yahoo-sent", account_id);
    let mut messages_by_folder = BTreeMap::new();
    messages_by_folder.insert(
        folder.id.clone(),
        (1..=total_messages)
            .map(|index| {
                SharedMailboxMessage::new(
                    SharedMailboxMessageSummary::new(
                        format!("yahoo-task-3060-{index:03}"),
                        format!("Task 3060 Yahoo Mail message {index:03}"),
                        1_786_400_000 + index as i64,
                        "owner@yahoo.example",
                    ),
                    format!("body for task 3060 Yahoo Mail message {index:03}"),
                )
            })
            .collect(),
    );
    SharedMailboxReader::new(vec![folder], messages_by_folder).expect("Yahoo fixture is valid")
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
fn task_3060_yahoo_folder_pages_through_with_pause_and_stops_during_page_two() {
    let mailbox = yahoo_mailbox_with_folder(120);
    let folders = mailbox.list_yahoo_folders();
    let mut full_folder = mailbox
        .yahoo_folder_page_place("task-3060-yahoo-sent", 40)
        .expect("Yahoo Mail folder exists");
    let folder_message_count = full_folder.message_count();
    let mut full_pace = SharedConversationScrollPace::new(25);
    let running_gate = StopFlagGate {
        stop_requested: Rc::new(Cell::new(false)),
    };

    let full_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_folder,
        10,
        &running_gate,
        &mut full_pace,
    )
    .expect("shared reader pages through the Yahoo Mail folder");
    let full_gaps = inter_action_gaps_ms(&full_read);
    let every_full_action_used_set_pause = full_read
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == 25);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= 25);

    println!("TASK3060_SHARED_HELPER=read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace");
    println!("TASK3060_YAHOO_FOLDER_HELPER=yahoo_folder_page_place");
    println!("TASK3060_YAHOO_FOLDER_COUNT={}", folders.len());
    println!("TASK3060_YAHOO_FOLDER_ID={}", full_folder.folder().id);
    println!(
        "TASK3060_YAHOO_FOLDER_SERVICE={}",
        full_folder.folder().service
    );
    println!("TASK3060_YAHOO_FOLDER_MESSAGE_COUNT={folder_message_count}");
    println!("TASK3060_SET_PAUSE_MS={}", full_pace.pause_ms());
    println!(
        "TASK3060_FULL_READ_MESSAGE_COUNT={}",
        full_read.message_count()
    );
    println!("TASK3060_FULL_PAGE_COUNT={}", full_read.page_count());
    println!("TASK3060_FULL_STOP_REASON={:?}", full_read.stop_reason);
    println!(
        "TASK3060_FULL_ONE_SCREEN_SCROLLS={}",
        full_folder.one_screen_scroll_count()
    );
    println!("TASK3060_FULL_ACTION_COUNT={}", full_read.action_log.len());
    println!("TASK3060_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3060_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3060_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3060_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3060_FULL_TOTAL_RUN_MS={}", full_pace.elapsed_ms());

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = stop_requested.clone();
    let mut stopped_folder = mailbox
        .yahoo_folder_page_place("task-3060-yahoo-sent", 40)
        .expect("Yahoo Mail folder exists for stop run");
    stopped_folder.request_stop_when_reading_page(2, move || {
        stop_callback_flag.set(true);
        Ok(())
    });
    let stop_gate = StopFlagGate {
        stop_requested: stop_requested.clone(),
    };
    let mut stop_pace = SharedConversationScrollPace::new(25);
    let stopped_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut stopped_folder,
        10,
        &stop_gate,
        &mut stop_pace,
    )
    .expect("shared reader stops the Yahoo Mail folder after page two");

    println!("TASK3060_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3060_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3060_STOP_REASON={:?}", stopped_read.stop_reason);
    println!(
        "TASK3060_STOPPED_ON_PAGE_NUMBER={}",
        stopped_read.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3060_STOP_LOG_RECORDED_PAGE={}",
        stopped_read
            .page_log
            .last()
            .map(|page| page.page_number)
            .unwrap_or_default()
    );
    println!(
        "TASK3060_STOP_GATE_AFTER_PAGE={:?}",
        stopped_read
            .page_log
            .last()
            .map(|page| page.gate_after_page)
            .unwrap_or(SharedConversationScrollGateState::Running)
    );
    println!(
        "TASK3060_STOP_READ_MESSAGE_COUNT={}",
        stopped_read.message_count()
    );
    println!("TASK3060_STOP_PAGE_COUNT={}", stopped_read.page_count());
    println!(
        "TASK3060_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped_read.message_count())
    );
    println!(
        "TASK3060_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_folder.one_screen_scroll_count()
    );

    assert_eq!(folders.len(), 1);
    assert_eq!(full_folder.folder().service, "yahoo");
    assert_eq!(folder_message_count, 120);
    assert_eq!(full_read.message_count(), 120);
    assert!(
        full_read.page_count() >= 3,
        "120 Yahoo Mail messages must be read over at least three pages"
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
