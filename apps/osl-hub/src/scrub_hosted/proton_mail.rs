//! Proton Mail fill-in for the shared mailbox reader and page-through helper.

use crate::scrub_hosted::reader::{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace, PolitePace,
    SharedReaderMessage, SharedReaderOptions, SharedReaderProgress, SharedReaderRun,
    SharedReaderSource,
};
use crate::services::{read_shared_mailbox_messages, MailboxReaderSnapshot};

pub const PROTON_MAIL_SERVICE_ID: &str = "proton";
pub const PROTON_MAIL_PAGE_SIZE: usize = 30;

pub fn read_proton_mailbox_folder_page_through_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    folder_id: &str,
    proton_filled_mailbox: &MailboxReaderSnapshot,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
) -> Result<SharedReaderRun, String> {
    read_proton_mailbox_folder_page_through_for_scrub_with_stop(
        owner_osl_user_id,
        account_id,
        folder_id,
        proton_filled_mailbox,
        options,
        pace,
        |_| false,
    )
}

pub fn read_proton_mailbox_folder_page_through_for_scrub_with_stop<Stop>(
    owner_osl_user_id: &str,
    account_id: &str,
    folder_id: &str,
    proton_filled_mailbox: &MailboxReaderSnapshot,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
    stop_requested: Stop,
) -> Result<SharedReaderRun, String>
where
    Stop: FnMut(SharedReaderProgress) -> bool,
{
    let messages = read_shared_mailbox_messages(
        owner_osl_user_id,
        PROTON_MAIL_SERVICE_ID,
        account_id,
        folder_id,
        proton_filled_mailbox,
    )?
    .into_iter()
    .map(|summary| SharedReaderMessage {
        id: summary.message_id,
        text: summary.subject,
    })
    .collect();
    let mut source = ProtonMailboxFolderPageSource::new(messages, PROTON_MAIL_PAGE_SIZE);
    Ok(
        read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
            &mut source,
            options,
            pace,
            stop_requested,
        ),
    )
}

struct ProtonMailboxFolderPageSource {
    messages: Vec<SharedReaderMessage>,
    current_offset: usize,
    page_size: usize,
}

impl ProtonMailboxFolderPageSource {
    fn new(messages: Vec<SharedReaderMessage>, page_size: usize) -> Self {
        Self {
            messages,
            current_offset: 0,
            page_size,
        }
    }
}

impl SharedReaderSource for ProtonMailboxFolderPageSource {
    fn open_visible_page(&mut self) -> Vec<SharedReaderMessage> {
        let end = self
            .current_offset
            .saturating_add(self.page_size)
            .min(self.messages.len());
        self.messages[self.current_offset..end].to_vec()
    }

    fn scroll_one_page(&mut self) -> bool {
        let next_offset = self.current_offset.saturating_add(self.page_size);
        if next_offset >= self.messages.len() {
            return false;
        }
        self.current_offset = next_offset;
        true
    }
}
