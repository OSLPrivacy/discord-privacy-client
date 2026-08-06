use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use osl_privacy_hub::scrub_imap::{
    read_tuta_shared_folder_paged, ScrubImapError, SharedMailboxAuthorship, SharedMailboxMessage,
    SharedMailboxMessagePage, SharedMailboxPagedReader, SharedMailboxPagingCompletion,
    SharedMailboxPagingOptions, SharedMailboxPagingStop,
};

struct SeededTutaPagedMailbox {
    folder: String,
    messages: Vec<SharedMailboxMessage>,
    stop: SharedMailboxPagingStop,
    stop_during_page: Option<usize>,
    stop_requested_during_page: AtomicUsize,
}

impl SeededTutaPagedMailbox {
    fn folder_with_120_messages(
        folder: &str,
        stop: SharedMailboxPagingStop,
        stop_during_page: Option<usize>,
    ) -> Self {
        let messages = (1..=120)
            .map(|ordinal| SharedMailboxMessage {
                subject: format!("TUTA-SCRUB-{ordinal:03}"),
                time: format!("2026-08-06T12:{:02}:00Z", (ordinal - 1) % 60),
                sender: format!("sender-{ordinal:03}@example.test"),
                authorship: SharedMailboxAuthorship::NotYours,
            })
            .collect();
        Self {
            folder: folder.to_string(),
            messages,
            stop,
            stop_during_page,
            stop_requested_during_page: AtomicUsize::new(0),
        }
    }

    fn stop_requested_during_page(&self) -> usize {
        self.stop_requested_during_page.load(Ordering::Acquire)
    }
}

impl SharedMailboxPagedReader for SeededTutaPagedMailbox {
    fn list_folders(&self) -> Result<Vec<String>, ScrubImapError> {
        Ok(vec![self.folder.clone()])
    }

    fn list_message_page(
        &self,
        folder: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<SharedMailboxMessagePage, ScrubImapError> {
        if folder != self.folder || limit == 0 {
            return Err(ScrubImapError::SharedMailboxReadFailed);
        }
        let offset = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| ScrubImapError::SharedMailboxReadFailed)?;
        if offset > self.messages.len() {
            return Err(ScrubImapError::SharedMailboxReadFailed);
        }

        let page_number = (offset / limit) + 1;
        if self.stop_during_page == Some(page_number) {
            self.stop_requested_during_page
                .store(page_number, Ordering::Release);
            self.stop.request_stop();
        }

        let end = offset.saturating_add(limit).min(self.messages.len());
        Ok(SharedMailboxMessagePage {
            messages: self.messages[offset..end].to_vec(),
            next_cursor: (end < self.messages.len()).then(|| end.to_string()),
        })
    }
}

#[test]
fn task_3075_tuta_shared_paging_reads_three_pages_and_safe_stop_ends_page_two() {
    let folder = "Tuta Archive";
    let options = SharedMailboxPagingOptions::new(30, Duration::from_millis(1));
    let full_stop = SharedMailboxPagingStop::default();
    let full_reader =
        SeededTutaPagedMailbox::folder_with_120_messages(folder, full_stop.clone(), None);

    let full = read_tuta_shared_folder_paged(&full_reader, folder, options.clone(), &full_stop)
        .expect("Tuta shared folder full paged read");
    assert_eq!(full.provider, "tuta");
    assert_eq!(full.folder, folder);
    assert_eq!(full.total_messages, 120);
    assert!(
        full.pages_read >= 3,
        "a 120-message folder must be read over at least three pages"
    );
    assert_eq!(full.pages_read, 4);
    assert_eq!(full.pause_millis, 1);
    assert_eq!(full.pauses_observed, 3);
    assert_eq!(full.completion, SharedMailboxPagingCompletion::Complete);

    let stopping_stop = SharedMailboxPagingStop::default();
    let stopping_reader =
        SeededTutaPagedMailbox::folder_with_120_messages(folder, stopping_stop.clone(), Some(2));
    let stopped = read_tuta_shared_folder_paged(&stopping_reader, folder, options, &stopping_stop)
        .expect("Tuta shared folder stopped paged read");

    println!("TASK3075_FULL_PROVIDER={}", full.provider);
    println!("TASK3075_FULL_FOLDER={}", full.folder);
    println!("TASK3075_FULL_FOLDER_MESSAGE_COUNT={}", full.total_messages);
    println!("TASK3075_FULL_PAGES_READ={}", full.pages_read);
    println!("TASK3075_FULL_SET_PAUSE_MS={}", full.pause_millis);
    println!("TASK3075_FULL_PAUSES_OBSERVED={}", full.pauses_observed);
    println!(
        "TASK3075_STOP_REQUESTED_DURING_PAGE={}",
        stopping_reader.stop_requested_during_page()
    );
    println!("TASK3075_STOP_COMPLETION={}", stopped.completion.label());
    println!("TASK3075_STOP_MESSAGES_READ={}", stopped.total_messages);
    println!("TASK3075_STOP_PAGES_READ={}", stopped.pages_read);
    println!("TASK3075_STOP_SET_PAUSE_MS={}", stopped.pause_millis);

    assert_eq!(stopping_reader.stop_requested_during_page(), 2);
    assert_eq!(stopped.completion, SharedMailboxPagingCompletion::Stopped);
    assert_eq!(stopped.pages_read, 2);
    assert!(
        (40..=80).contains(&stopped.total_messages),
        "stop during page two must retain a bounded safe-page result"
    );
    assert_eq!(stopped.total_messages, 60);
}
