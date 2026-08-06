#![cfg(feature = "core")]

use osl_privacy_hub::service_connections::{
    read_outlook_web_shared_mailbox_messages_paged, SharedMailLabel, SharedMailMessageRecord,
    SharedMailboxPagedRead, SharedMailboxPagingConfig, SharedMailboxPagingPause,
    SharedMailboxPagingStop, SharedMailboxSnapshot,
};

const SIGNED_IN: &str = "scrub-owner@outlook.test";
const PAGE_SIZE: usize = 40;
const PAUSE_MS: u64 = 25;

#[derive(Default)]
struct RecordingPause {
    calls: Vec<(usize, u64)>,
}

impl SharedMailboxPagingPause for RecordingPause {
    fn pause_after_page(&mut self, page_number: usize, pause_ms: u64) {
        self.calls.push((page_number, pause_ms));
    }
}

#[derive(Default)]
struct NoStop;

impl SharedMailboxPagingStop for NoStop {
    fn should_stop(
        &mut self,
        _folder_id: &str,
        _page_number: usize,
        _messages_read: usize,
    ) -> bool {
        false
    }
}

struct StopAfterRead {
    limit: usize,
    requested_on_page: Option<usize>,
}

impl SharedMailboxPagingStop for StopAfterRead {
    fn should_stop(&mut self, _folder_id: &str, page_number: usize, messages_read: usize) -> bool {
        if messages_read >= self.limit {
            self.requested_on_page.get_or_insert(page_number);
            true
        } else {
            false
        }
    }
}

fn seeded_outlook_web_test_mailbox() -> SharedMailboxSnapshot {
    let sent = (0..120).map(|index| {
        SharedMailMessageRecord::new(
            "Sent",
            format!("sent-outlook-web-{index:03}"),
            format!("Outlook web scrub message {index:03}"),
            1_786_360_000 + i64::from(index) * 60,
            SIGNED_IN,
            format!("Seeded Outlook web scrub body {index:03}."),
        )
    });
    let inbox = [
        SharedMailMessageRecord::new(
            "Inbox",
            "inbox-outlook-web-one",
            "Friend Outlook cleanup request",
            1_786_367_200,
            "friend@outlook.test",
            "Inbox fixture outside the paged Sent folder.",
        ),
        SharedMailMessageRecord::new(
            "Inbox",
            "inbox-outlook-web-two",
            "Team Outlook scrub note",
            1_786_367_260,
            "team@outlook.test",
            "Second inbox fixture outside the paged Sent folder.",
        ),
    ];

    SharedMailboxSnapshot::new(
        SIGNED_IN,
        [
            SharedMailLabel::new("Inbox", "Inbox"),
            SharedMailLabel::new("Sent", "Sent"),
            SharedMailLabel::new("Archive", "Archive"),
            SharedMailLabel::new("Deleted", "Deleted"),
        ],
        sent.chain(inbox),
    )
}

fn page_lengths(read: &SharedMailboxPagedRead) -> Vec<usize> {
    read.pages.iter().map(|page| page.messages.len()).collect()
}

#[test]
fn task_3051_outlook_web_pages_with_pause_and_stops_during_page_two() {
    let mailbox = seeded_outlook_web_test_mailbox();
    let config = SharedMailboxPagingConfig::new(PAGE_SIZE, PAUSE_MS);

    let mut full_pause = RecordingPause::default();
    let mut no_stop = NoStop;
    let full = read_outlook_web_shared_mailbox_messages_paged(
        &mailbox,
        "Sent",
        config,
        &mut full_pause,
        &mut no_stop,
    )
    .expect("Outlook web full folder read pages");

    let mut stop_pause = RecordingPause::default();
    let mut stop = StopAfterRead {
        limit: 60,
        requested_on_page: None,
    };
    let stopped = read_outlook_web_shared_mailbox_messages_paged(
        &mailbox,
        "Sent",
        config,
        &mut stop_pause,
        &mut stop,
    )
    .expect("Outlook web stopped folder read pages");

    let sent_folder_message_count = mailbox
        .messages
        .iter()
        .filter(|message| message.label_id == "Sent")
        .count();
    let stopped_between_40_and_80 = (40..=80).contains(&stopped.messages_read);
    let page_two_read = stopped
        .pages
        .iter()
        .find(|page| page.page_number == 2)
        .map(|page| page.messages.len())
        .unwrap_or_default();

    println!("TASK3051 direct_reader=outlook_web_shared_mailbox_paged_reader");
    println!("TASK3051 folder=Sent folder_message_count={sent_folder_message_count}");
    println!("TASK3051 page_size={PAGE_SIZE}");
    println!("TASK3051 configured_pause_ms={PAUSE_MS}");
    println!("TASK3051 full_read.page_count={}", full.pages.len());
    println!("TASK3051 full_read.messages_read={}", full.messages_read);
    println!("TASK3051 full_read.page_lengths={:?}", page_lengths(&full));
    println!("TASK3051 full_read.stopped={}", full.stopped);
    println!("TASK3051 full_read.pause_count={}", full_pause.calls.len());
    for (page, ms) in &full_pause.calls {
        println!("TASK3051 full_read.pause_after_page={page} pause_ms={ms}");
    }
    println!(
        "TASK3051 stop_run.stop_requested_during_page={}",
        stop.requested_on_page.unwrap_or_default()
    );
    println!("TASK3051 stop_run.stopped={}", stopped.stopped);
    println!(
        "TASK3051 stop_run.stopped_on_page={}",
        stopped.stopped_on_page.unwrap_or_default()
    );
    println!("TASK3051 stop_run.messages_read={}", stopped.messages_read);
    println!("TASK3051 stop_run.messages_read_between_40_and_80={stopped_between_40_and_80}");
    println!("TASK3051 stop_run.page_count={}", stopped.pages.len());
    println!(
        "TASK3051 stop_run.page_lengths={:?}",
        page_lengths(&stopped)
    );
    println!("TASK3051 stop_run.page_2_messages_read={page_two_read}");
    println!("TASK3051 stop_run.pause_count={}", stop_pause.calls.len());
    for (page, ms) in &stop_pause.calls {
        println!("TASK3051 stop_run.pause_after_page={page} pause_ms={ms}");
    }

    assert_eq!(sent_folder_message_count, 120);
    assert_eq!(full.messages_read, 120);
    assert!(full.pages.len() >= 3);
    assert_eq!(page_lengths(&full), vec![40, 40, 40]);
    assert!(!full.stopped);
    assert_eq!(full_pause.calls, vec![(1, PAUSE_MS), (2, PAUSE_MS)]);
    assert_eq!(
        full.pages
            .iter()
            .map(|page| page.pause_after_page_ms)
            .collect::<Vec<_>>(),
        vec![Some(PAUSE_MS), Some(PAUSE_MS), None]
    );

    assert!(stopped.stopped);
    assert_eq!(stop.requested_on_page, Some(2));
    assert_eq!(stopped.stopped_on_page, Some(2));
    assert_eq!(stopped.messages_read, 60);
    assert!(stopped_between_40_and_80);
    assert_eq!(page_lengths(&stopped), vec![40, 20]);
    assert_eq!(stop_pause.calls, vec![(1, PAUSE_MS)]);
}
