use osl_privacy_hub::shared_conversation_scroll::{
    read_shared_conversation_messages_one_page_at_a_time, SharedConversationScrollStop,
    SharedConversationScrollablePlace, SharedPlaceMessage,
};

struct TestConversationPlace {
    messages: Vec<SharedPlaceMessage>,
    current_page: usize,
    page_size: usize,
    one_screen_scrolls: usize,
}

impl TestConversationPlace {
    fn holding_messages(total_messages: usize, page_size: usize) -> Self {
        Self {
            messages: (1..=total_messages)
                .map(|index| {
                    SharedPlaceMessage::new(
                        format!("task-3003-message-{index:03}"),
                        format!("task 3003 test message {index:03}"),
                    )
                })
                .collect(),
            current_page: 0,
            page_size,
            one_screen_scrolls: 0,
        }
    }

    fn held_message_count(&self) -> usize {
        self.messages.len()
    }

    fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls
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
        self.one_screen_scrolls += 1;
        self.current_page += 1;
        Ok(true)
    }
}

#[test]
fn task_3003_reads_120_messages_over_three_pages_and_honors_page_limit() {
    let mut full_place = TestConversationPlace::holding_messages(120, 40);
    let full_place_count = full_place.held_message_count();
    let full_read = read_shared_conversation_messages_one_page_at_a_time(&mut full_place, 10)
        .expect("full bounded scroll should read the test place");

    println!("TASK3003_DIRECT_SCROLL=read_shared_conversation_messages_one_page_at_a_time");
    println!("TASK3003_SCROLL_MODE=one_screen_at_a_time");
    println!("TASK3003_FULL_PLACE_MESSAGE_COUNT={full_place_count}");
    println!(
        "TASK3003_FULL_READ_MESSAGE_COUNT={}",
        full_read.message_count()
    );
    println!("TASK3003_FULL_PAGE_COUNT={}", full_read.page_count());
    println!("TASK3003_FULL_LOG_PAGE_COUNT={}", full_read.page_log.len());
    println!("TASK3003_FULL_STOP_REASON={:?}", full_read.stop_reason);
    println!(
        "TASK3003_FULL_ONE_SCREEN_SCROLLS={}",
        full_place.one_screen_scroll_count()
    );

    for page in &full_read.page_log {
        println!(
            "TASK3003_PAGE page={} messages_on_screen={} new_messages_read={} cumulative_messages_read={}",
            page.page_number,
            page.messages_on_screen,
            page.new_messages_read,
            page.cumulative_messages_read
        );
    }

    let mut limited_place = TestConversationPlace::holding_messages(120, 40);
    let limited_read = read_shared_conversation_messages_one_page_at_a_time(&mut limited_place, 2)
        .expect("limited bounded scroll should stop at two pages");

    println!("TASK3003_LIMIT_PAGE_COUNT=2");
    println!(
        "TASK3003_LIMIT_READ_MESSAGE_COUNT={}",
        limited_read.message_count()
    );
    println!(
        "TASK3003_LIMIT_LOG_PAGE_COUNT={}",
        limited_read.page_log.len()
    );
    println!("TASK3003_LIMIT_STOP_REASON={:?}", limited_read.stop_reason);
    println!(
        "TASK3003_LIMIT_RETURNS_FEWER_THAN_FULL={}",
        limited_read.message_count() < full_place_count
    );

    assert_eq!(full_place_count, 120);
    assert_eq!(full_read.message_count(), 120);
    assert!(
        full_read.page_count() >= 3,
        "120 messages must be read over at least three pages"
    );
    assert_eq!(full_read.page_count(), full_read.page_log.len());
    assert_eq!(
        full_read.stop_reason,
        SharedConversationScrollStop::EndOfPlace
    );
    assert!(full_read
        .page_log
        .iter()
        .all(|page| page.messages_on_screen == 40 && page.new_messages_read == 40));

    assert_eq!(limited_read.page_count(), 2);
    assert_eq!(limited_read.page_log.len(), 2);
    assert_eq!(
        limited_read.stop_reason,
        SharedConversationScrollStop::PageLimitReached
    );
    assert!(limited_read.message_count() < full_place_count);
}
