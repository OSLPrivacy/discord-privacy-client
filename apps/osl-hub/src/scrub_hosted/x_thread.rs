//! X thread paging for Scrub.
//!
//! The X browser reader supplies only already-reviewed rows. This adapter turns
//! one selected thread into a shared source that exposes one visible page at a
//! time; advancing it is the sole way the next page becomes visible.

use crate::scrub_hosted::reader::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace, PolitePace,
    SharedReaderMessage, SharedReaderOptions, SharedReaderProgress, SharedReaderRun,
    SharedReaderSource,
};
use crate::services::{read_x_shared_messages, XBrowserMachine};

/// X virtualizes its transcript, so Scrub requests the next 30 rows only after
/// the previous screen has been processed.
pub const X_THREAD_PAGE_SIZE: usize = 30;

pub fn read_x_thread_page_through_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    thread_id: &str,
    browser_machine: &XBrowserMachine,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
) -> Result<SharedReaderRun, String> {
    read_x_thread_page_through_for_scrub_with_stop(
        owner_osl_user_id,
        account_id,
        thread_id,
        browser_machine,
        options,
        pace,
        |_| false,
    )
}

pub fn read_x_thread_page_through_for_scrub_with_stop<Stop>(
    owner_osl_user_id: &str,
    account_id: &str,
    thread_id: &str,
    browser_machine: &XBrowserMachine,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
    stop_requested: Stop,
) -> Result<SharedReaderRun, String>
where
    Stop: FnMut(SharedReaderProgress) -> bool,
{
    let messages =
        read_x_shared_messages(owner_osl_user_id, account_id, thread_id, browser_machine)?
            .into_iter()
            .map(|message| SharedReaderMessage {
                id: message.message_id,
                text: message.text,
            })
            .collect();
    let mut source = XThreadPageSource::new(messages);
    Ok(
        read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
            &mut source,
            options,
            pace,
            stop_requested,
        ),
    )
}

struct XThreadPageSource {
    messages: Vec<SharedReaderMessage>,
    current_offset: usize,
}

impl XThreadPageSource {
    fn new(messages: Vec<SharedReaderMessage>) -> Self {
        Self {
            messages,
            current_offset: 0,
        }
    }
}

impl SharedReaderSource for XThreadPageSource {
    fn open_visible_page(&mut self) -> Vec<SharedReaderMessage> {
        let end = self
            .current_offset
            .saturating_add(X_THREAD_PAGE_SIZE)
            .min(self.messages.len());
        self.messages[self.current_offset..end].to_vec()
    }

    fn scroll_one_page(&mut self) -> bool {
        let next_offset = self.current_offset.saturating_add(X_THREAD_PAGE_SIZE);
        if next_offset >= self.messages.len() {
            return false;
        }
        self.current_offset = next_offset;
        true
    }
}
