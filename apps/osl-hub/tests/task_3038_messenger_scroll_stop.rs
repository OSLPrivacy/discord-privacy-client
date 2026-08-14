use std::cell::Cell;
use std::rc::Rc;

use osl_privacy_hub::messenger_conversation_scroll::MessengerConversationPagePlace;
use osl_privacy_hub::messenger_message_reader::SharedMessengerMessage;
use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace,
    SharedConversationScrollGate, SharedConversationScrollGateState, SharedConversationScrollPace,
    SharedConversationScrollStop,
};

const ACCOUNT: &str = "messenger-task-3038";
const PLACE: &str = "dm-task-3038";
const SET_PAUSE_MS: u64 = 25;

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

fn seeded_messenger_messages() -> Vec<SharedMessengerMessage> {
    (1..=120)
        .map(|index| SharedMessengerMessage {
            service_id: "messenger",
            account_id: ACCOUNT.to_owned(),
            place_id: PLACE.to_owned(),
            message_id: format!("task-3038-message-{index:03}"),
            text: format!("Task 3038 Messenger message {index:03}"),
            time: 1_786_500_000 + index as i64,
            author_id: if index % 2 == 0 {
                "messenger-task-3038-owner".to_owned()
            } else {
                "messenger-friend".to_owned()
            },
            yours: index % 2 == 0,
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
fn task_3038_messenger_chat_scrolls_pages_with_pause_and_stops_during_page_two() {
    let messages = seeded_messenger_messages();
    let mut full_chat = MessengerConversationPagePlace::new(PLACE, messages.clone(), 40)
        .expect("valid Messenger chat");
    let mut full_pace = SharedConversationScrollPace::new(SET_PAUSE_MS);
    let full_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_chat,
        10,
        &StopFlagGate {
            stop_requested: Rc::new(Cell::new(false)),
        },
        &mut full_pace,
    )
    .expect("full Messenger scroll");
    let full_gaps = inter_action_gaps_ms(&full_read);
    let every_full_action_used_set_pause = full_read
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == SET_PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= SET_PAUSE_MS);

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = stop_requested.clone();
    let mut stopped_chat = MessengerConversationPagePlace::new(PLACE, messages, 40)
        .expect("valid Messenger chat for stop run");
    stopped_chat.request_stop_when_reading_page(2, move || {
        stop_callback_flag.set(true);
        Ok(())
    });
    let mut stop_pace = SharedConversationScrollPace::new(SET_PAUSE_MS);
    let stopped_read = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut stopped_chat,
        10,
        &StopFlagGate {
            stop_requested: stop_requested.clone(),
        },
        &mut stop_pace,
    )
    .expect("Messenger stop during page two");

    println!("TASK3038_CHAT_SERVICE=messenger");
    println!(
        "TASK3038_SEEDED_MESSAGE_COUNT={}",
        full_chat.message_count()
    );
    println!("TASK3038_SET_PAUSE_MS={}", full_pace.pause_ms());
    println!(
        "TASK3038_FULL_READ_MESSAGE_COUNT={}",
        full_read.message_count()
    );
    println!("TASK3038_FULL_PAGE_COUNT={}", full_read.page_count());
    println!("TASK3038_FULL_STOP_REASON={:?}", full_read.stop_reason);
    println!("TASK3038_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3038_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3038_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!("TASK3038_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3038_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3038_STOP_REASON={:?}", stopped_read.stop_reason);
    println!(
        "TASK3038_STOPPED_ON_PAGE_NUMBER={}",
        stopped_read.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3038_STOP_READ_MESSAGE_COUNT={}",
        stopped_read.message_count()
    );
    println!("TASK3038_STOP_PAGE_COUNT={}", stopped_read.page_count());
    println!(
        "TASK3038_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped_read.message_count())
    );

    assert_eq!(full_chat.conversation_id(), PLACE);
    assert_eq!(full_chat.message_count(), 120);
    assert_eq!(full_read.message_count(), 120);
    assert!(full_read.page_count() >= 3);
    assert_eq!(
        full_read.stop_reason,
        SharedConversationScrollStop::EndOfPlace
    );
    assert_eq!(full_pace.pause_ms(), SET_PAUSE_MS);
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
    assert!((40..=80).contains(&stopped_read.message_count()));
}
