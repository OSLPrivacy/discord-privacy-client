use osl_privacy_hub::scrub_hosted::reader::{
    read_shared_conversation_messages_one_page_at_a_time, PolitePace, SharedReaderMessage,
    SharedReaderOptions, SharedReaderSource, SharedReaderStopReason,
};

struct ThreePagePlace {
    pages: Vec<Vec<SharedReaderMessage>>,
    current_page: usize,
}

impl ThreePagePlace {
    fn new() -> Self {
        let pages = (0..3)
            .map(|page| {
                (0..40)
                    .map(|row| SharedReaderMessage {
                        id: format!("page-{}-row-{}", page + 1, row + 1),
                        text: format!("fixture message {} {}", page + 1, row + 1),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Self {
            pages,
            current_page: 0,
        }
    }
}

impl SharedReaderSource for ThreePagePlace {
    fn open_visible_page(&mut self) -> Vec<SharedReaderMessage> {
        self.pages
            .get(self.current_page)
            .cloned()
            .unwrap_or_default()
    }

    fn scroll_one_page(&mut self) -> bool {
        if self.current_page + 1 >= self.pages.len() {
            return false;
        }
        self.current_page += 1;
        true
    }
}

#[test]
fn task_3004_shared_reader_scrolls_and_opens_at_the_polite_pace() {
    let mut place = ThreePagePlace::new();
    let mut pace = PolitePace::new(25);

    let run = read_shared_conversation_messages_one_page_at_a_time(
        &mut place,
        SharedReaderOptions { max_pages: 3 },
        &mut pace,
    );

    let action_trace = run
        .action_log
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" -> ");
    let action_names = run
        .action_log
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let minimum_runtime_ms = run.action_log.len() as u64 * pace.pause_ms();
    let inter_action_gaps = run
        .action_log
        .windows(2)
        .map(|pair| pair[1].start_ms.saturating_sub(pair[0].end_ms))
        .collect::<Vec<_>>();
    let every_gap_is_set_pause = inter_action_gaps.iter().all(|gap| *gap >= pace.pause_ms());
    let every_action_used_set_pause = run
        .action_log
        .iter()
        .all(|entry| entry.pause_ms == pace.pause_ms());

    assert_eq!(run.pages_read, 3);
    assert_eq!(run.messages.len(), 120);
    assert_eq!(run.stop_reason, SharedReaderStopReason::PageLimitReached);
    assert_eq!(action_names, "open,scroll,open,scroll,open");
    assert_eq!(run.max_parallel_actions, 1);
    assert!(every_action_used_set_pause);
    assert!(every_gap_is_set_pause);
    assert!(run.total_run_ms >= minimum_runtime_ms);

    println!("TASK3004_SHARED_READER_RUN=read_shared_conversation_messages_one_page_at_a_time");
    println!("TASK3004_SET_PAUSE_MS={}", pace.pause_ms());
    println!("TASK3004_PAGE_COUNT={}", run.pages_read);
    println!("TASK3004_MESSAGE_COUNT={}", run.messages.len());
    println!("TASK3004_ACTION_COUNT={}", run.action_log.len());
    println!("TASK3004_ACTION_NAMES={action_names}");
    println!("TASK3004_ACTION_TRACE={action_trace}");
    println!("TASK3004_MAX_PARALLEL_ACTIONS={}", run.max_parallel_actions);
    println!("TASK3004_EVERY_ACTION_USED_SET_PAUSE={every_action_used_set_pause}");
    println!("TASK3004_INTER_ACTION_GAPS_MS={inter_action_gaps:?}");
    println!("TASK3004_EVERY_INTER_ACTION_GAP_AT_LEAST_SET_PAUSE={every_gap_is_set_pause}");
    println!("TASK3004_TOTAL_RUN_MS={}", run.total_run_ms);
    println!("TASK3004_MINIMUM_RUNTIME_MS={minimum_runtime_ms}");
}
