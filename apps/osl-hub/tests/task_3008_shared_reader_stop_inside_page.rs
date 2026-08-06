use std::sync::atomic::{AtomicBool, Ordering};

use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time,
    read_shared_conversation_messages_one_page_at_a_time_with_gate,
    SharedConversationScrollEventLogEntry, SharedConversationScrollGate,
    SharedConversationScrollGateState, SharedConversationScrollStop,
    SharedConversationScrollablePlace, SharedPlaceMessage,
};

struct TestConversationPlace {
    messages: Vec<SharedPlaceMessage>,
    current_page: usize,
    page_size: usize,
}

impl TestConversationPlace {
    fn holding_messages(total_messages: usize, page_size: usize) -> Self {
        Self {
            messages: (1..=total_messages)
                .map(|index| {
                    SharedPlaceMessage::new(
                        format!("task-3008-message-{index:03}"),
                        format!("task 3008 test message {index:03}"),
                    )
                })
                .collect(),
            current_page: 0,
            page_size,
        }
    }
}

impl SharedConversationScrollablePlace for TestConversationPlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        let start = self.current_page.saturating_mul(self.page_size);
        let end = start
            .saturating_add(self.page_size)
            .min(self.messages.len());
        Ok(self.messages[start..end].to_vec())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let next_start = (self.current_page + 1).saturating_mul(self.page_size);
        if next_start >= self.messages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        Ok(true)
    }
}

struct StopDuringPageTwoGate {
    asked: AtomicBool,
}

impl StopDuringPageTwoGate {
    fn new() -> Self {
        Self {
            asked: AtomicBool::new(false),
        }
    }
}

impl SharedConversationScrollGate for StopDuringPageTwoGate {
    fn state_between_pages(&self) -> Result<SharedConversationScrollGateState, String> {
        Ok(SharedConversationScrollGateState::Running)
    }

    fn state_after_message(
        &self,
        page_number: usize,
        message_index_in_page: usize,
        _cumulative_messages_read: usize,
    ) -> Result<SharedConversationScrollGateState, String> {
        if page_number == 2 && message_index_in_page == 20 {
            self.asked.store(true, Ordering::Release);
        }
        if self.asked.load(Ordering::Acquire) {
            Ok(SharedConversationScrollGateState::StopAfterCurrentPage)
        } else {
            Ok(SharedConversationScrollGateState::Running)
        }
    }
}

#[test]
fn task_3008_reading_stops_inside_page_two_after_stop_and_logs_no_later_reads() {
    let mut unstopped_place = TestConversationPlace::holding_messages(120, 40);
    let unstopped = read_shared_conversation_messages_one_page_at_a_time(&mut unstopped_place, 10)
        .expect("unstopped read should cover the full test place");

    let mut stopped_place = TestConversationPlace::holding_messages(120, 40);
    let stop_gate = StopDuringPageTwoGate::new();
    let stopped = read_shared_conversation_messages_one_page_at_a_time_with_gate(
        &mut stopped_place,
        10,
        &stop_gate,
    )
    .expect("stopped read should honor a stop asked during page two");

    let stop_line_index = stopped
        .event_log
        .iter()
        .position(|entry| {
            matches!(
                entry,
                SharedConversationScrollEventLogEntry::StopRequestedInsidePage { .. }
            )
        })
        .expect("the event log records the stop line");
    let reads_after_stop_line = stopped.event_log[stop_line_index + 1..]
        .iter()
        .filter(|entry| {
            matches!(
                entry,
                SharedConversationScrollEventLogEntry::MessageRead { .. }
            )
        })
        .count();
    let stop_inside_page_two = match stopped.event_log[stop_line_index] {
        SharedConversationScrollEventLogEntry::StopRequestedInsidePage {
            page_number,
            message_index_in_page,
            cumulative_messages_read,
        } => page_number == 2 && message_index_in_page == 20 && cumulative_messages_read == 60,
        _ => false,
    };

    println!(
        "TASK3008_UNSTOPPED_READ_MESSAGE_COUNT={}",
        unstopped.message_count()
    );
    println!("TASK3008_UNSTOPPED_STOP_REASON={:?}", unstopped.stop_reason);
    println!(
        "TASK3008_STOPPED_READ_MESSAGE_COUNT={}",
        stopped.message_count()
    );
    println!("TASK3008_STOPPED_STOP_REASON={:?}", stopped.stop_reason);
    println!(
        "TASK3008_STOPPED_ON_PAGE_NUMBER={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!(
        "TASK3008_STOP_LOG_LINE=stop_requested_inside_page page=2 message_index_in_page=20 cumulative_messages_read=60"
    );
    println!("TASK3008_STOP_INSIDE_PAGE_TWO={stop_inside_page_two}");
    println!("TASK3008_READS_AFTER_STOP_LINE={reads_after_stop_line}");

    assert_eq!(unstopped.message_count(), 120);
    assert_eq!(
        unstopped.stop_reason,
        SharedConversationScrollStop::EndOfPlace
    );
    assert_eq!(
        stopped.stop_reason,
        SharedConversationScrollStop::StopRequested
    );
    assert!(
        stopped.message_count() > 40 && stopped.message_count() < 80,
        "stopped run must stop during page two, not before or after it"
    );
    assert_eq!(stopped.message_count(), 60);
    assert_eq!(stopped.stopped_on_page_number, Some(2));
    assert!(stop_inside_page_two);
    assert_eq!(reads_after_stop_line, 0);
    assert_eq!(
        stopped.page_log[1].stopped_inside_page_after_message,
        Some(20)
    );
}
