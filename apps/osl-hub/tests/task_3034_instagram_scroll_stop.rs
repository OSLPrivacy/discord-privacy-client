#![cfg(feature = "core")]

use std::cell::Cell;
use std::rc::Rc;

use osl_privacy_hub::services::{
    InstagramBrowserMachine, InstagramBrowserMessage, InstagramBrowserPlace,
    InstagramBrowserPlaceKind,
};
use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace,
    SharedConversationScrollGate, SharedConversationScrollGateState, SharedConversationScrollPace,
    SharedConversationScrollStop,
};

const PLACE: &str = "instagram-dm-task-3034";
const PAGE_SIZE: usize = 40;
const PAUSE_MS: u64 = 25;

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

fn seeded_direct_message() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([InstagramBrowserPlace::new(
        PLACE,
        "Task 3034 direct message",
        InstagramBrowserPlaceKind::DirectMessage,
    )])
    // Reverse the input order to prove the adapter, rather than the fixture,
    // establishes the chronological one-page scroll order.
    .with_messages((1..=120).rev().map(|number| {
        InstagramBrowserMessage::new(
            PLACE,
            format!("instagram-task-3034-{number:03}"),
            format!("Instagram task 3034 message {number:03}"),
            1_786_500_000 + number as i64,
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
fn task_3034_instagram_direct_message_pages_with_pause_and_stops_during_page_two() {
    let browser = seeded_direct_message();
    let mut full_place = browser
        .direct_message_page_place(PLACE, PAGE_SIZE)
        .expect("seeded Instagram direct message creates a paged place");
    let full_message_count = full_place.message_count();
    let mut full_pace = SharedConversationScrollPace::new(PAUSE_MS);
    let running_gate = StopFlagGate {
        stop_requested: Rc::new(Cell::new(false)),
    };
    let full = read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        &mut full_place,
        10,
        &running_gate,
        &mut full_pace,
    )
    .expect("Instagram direct message reads across every visible page");
    let full_gaps = inter_action_gaps_ms(&full);
    let every_full_action_used_set_pause = full
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == PAUSE_MS);
    let every_full_gap_used_set_pause = full_gaps.iter().all(|gap| *gap >= PAUSE_MS);

    let stop_requested = Rc::new(Cell::new(false));
    let stop_callback_flag = Rc::clone(&stop_requested);
    let mut stopped_place = browser
        .direct_message_page_place(PLACE, PAGE_SIZE)
        .expect("same Instagram direct message creates an independent paged place");
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
    .expect("Instagram direct message ends after the page-two stop request");

    println!("TASK3034_SHARED_HELPER=read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace");
    println!("TASK3034_INSTAGRAM_HELPER=direct_message_page_place");
    println!("TASK3034_SEEDED_DIRECT_MESSAGE_COUNT={full_message_count}");
    println!("TASK3034_SET_PAUSE_MS={}", full_pace.pause_ms());
    println!("TASK3034_FULL_READ_MESSAGE_COUNT={}", full.message_count());
    println!("TASK3034_FULL_PAGE_COUNT={}", full.page_count());
    println!("TASK3034_FULL_STOP_REASON={:?}", full.stop_reason);
    println!(
        "TASK3034_FULL_ONE_SCREEN_SCROLLS={}",
        full_place.one_screen_scroll_count()
    );
    println!("TASK3034_FULL_EVERY_ACTION_USED_SET_PAUSE={every_full_action_used_set_pause}");
    println!("TASK3034_FULL_INTER_ACTION_GAPS_MS={full_gaps:?}");
    println!(
        "TASK3034_FULL_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_full_gap_used_set_pause}"
    );
    println!(
        "TASK3034_FULL_MAX_PARALLEL_ACTIONS={}",
        full_pace.max_parallel_actions()
    );
    println!("TASK3034_STOP_REQUESTED_DURING_PAGE=2");
    println!(
        "TASK3034_STOP_REQUESTED_DURING_RUN={}",
        stop_requested.get()
    );
    println!("TASK3034_STOP_REASON={:?}", stopped.stop_reason);
    println!(
        "TASK3034_STOPPED_ON_PAGE_NUMBER={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3034_STOP_READ_MESSAGE_COUNT={}",
        stopped.message_count()
    );
    println!("TASK3034_STOP_PAGE_COUNT={}", stopped.page_count());
    println!(
        "TASK3034_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
        (40..=80).contains(&stopped.message_count())
    );
    println!(
        "TASK3034_STOP_ONE_SCREEN_SCROLLS={}",
        stopped_place.one_screen_scroll_count()
    );

    assert_eq!(full_message_count, 120);
    assert_eq!(full.message_count(), 120);
    assert!(
        full.page_count() >= 3,
        "120 Instagram messages must be read over at least three pages"
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
        Some("instagram-task-3034-001")
    );
    assert_eq!(
        full.messages
            .last()
            .map(|message| message.message_id.as_str()),
        Some("instagram-task-3034-120")
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
