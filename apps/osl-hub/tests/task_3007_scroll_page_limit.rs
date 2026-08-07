use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time, SharedConversationScrollStop,
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
                        format!("task-3007-message-{index:03}"),
                        format!("task 3007 test message {index:03}"),
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

#[test]
fn task_3007_scroll_helper_stops_at_its_page_limit() {
    let mut full_place = TestConversationPlace::holding_messages(120, 40);
    let full = read_shared_conversation_messages_one_page_at_a_time(&mut full_place, usize::MAX)
        .expect("unlimited read should end by itself");

    let mut limited_place = TestConversationPlace::holding_messages(120, 40);
    let limited = read_shared_conversation_messages_one_page_at_a_time(&mut limited_place, 2)
        .expect("two-page read should end by itself");

    println!("TASK3007_FULL_READ_MESSAGE_COUNT={}", full.message_count());
    println!(
        "TASK3007_LIMIT_READ_MESSAGE_COUNT={}",
        limited.message_count()
    );
    println!("TASK3007_LIMIT_LOG_PAGE_COUNT={}", limited.page_log.len());
    println!("TASK3007_LIMIT_STOP_REASON={:?}", limited.stop_reason);
    println!("TASK3007_RUN_ENDED_WITHOUT_ERROR=true");
    for page in &limited.page_log {
        println!(
            "TASK3007_LIMIT_PAGE page={} messages_on_screen={} new_messages_read={} cumulative_messages_read={}",
            page.page_number,
            page.messages_on_screen,
            page.new_messages_read,
            page.cumulative_messages_read
        );
    }

    assert_eq!(full.message_count(), 120);
    assert_eq!(full.stop_reason, SharedConversationScrollStop::EndOfPlace);
    assert!(limited.message_count() < 120);
    assert_eq!(limited.page_log.len(), 2);
    assert_eq!(
        limited.stop_reason,
        SharedConversationScrollStop::PageLimitReached
    );
}
