//! Signal fill-in for the common one-screen Scrub reader.
//!
//! A page is still read exclusively by the narrow open-screen reader.  This
//! module adds the one explicit, bounded navigation edge required between
//! pages; it cannot type, focus, open a conversation, or press a key.

use std::cell::{Cell, RefCell};

use crate::services::SharedConversationPlace;
use crate::shared_conversation_scroll::{SharedConversationScrollablePlace, SharedPlaceMessage};
use crate::signal_message_reader::{
    read_signal_messages_for_scrub, SignalOpenScreenSource, SignalScreenReadAction,
};

/// Source for an already-open Signal chat that can move history by exactly one
/// visible screen.  Reading remains the prerequisite's `SignalOpenScreenSource`
/// operation; scroll is intentionally a distinct operation so it can only run
/// after the shared reader has accepted a complete page.
pub trait SignalOneScreenScrollSource: SignalOpenScreenSource {
    fn scroll_one_screen(&mut self) -> Result<bool, String>;
}

/// A Signal chat bound to one selected Scrub place and exposed as one visible
/// page at a time to the shared reader.
pub struct SignalScrubChatPagePlace<'a> {
    owner_osl_user_id: &'a str,
    account_id: &'a str,
    selected_place: &'a SharedConversationPlace,
    signed_in_sender_id: &'a str,
    source: RefCell<&'a mut dyn SignalOneScreenScrollSource>,
    one_screen_scrolls: Cell<usize>,
    pages_read: Cell<usize>,
    stop_when_reading_page: Cell<Option<usize>>,
    stop_request_callback: RefCell<Option<Box<dyn Fn() -> Result<(), String> + 'a>>>,
}

impl<'a> SignalScrubChatPagePlace<'a> {
    pub fn new(
        owner_osl_user_id: &'a str,
        account_id: &'a str,
        selected_place: &'a SharedConversationPlace,
        signed_in_sender_id: &'a str,
        source: &'a mut dyn SignalOneScreenScrollSource,
    ) -> Self {
        Self {
            owner_osl_user_id,
            account_id,
            selected_place,
            signed_in_sender_id,
            source: RefCell::new(source),
            one_screen_scrolls: Cell::new(0),
            pages_read: Cell::new(0),
            stop_when_reading_page: Cell::new(None),
            stop_request_callback: RefCell::new(None),
        }
    }

    pub fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls.get()
    }

    pub fn pages_read(&self) -> usize {
        self.pages_read.get()
    }

    pub fn request_stop_when_reading_page(
        &self,
        page_number: usize,
        callback: impl Fn() -> Result<(), String> + 'a,
    ) {
        self.stop_when_reading_page.set(Some(page_number));
        self.stop_request_callback.replace(Some(Box::new(callback)));
    }

    pub fn source_action_log(&self) -> Vec<SignalScreenReadAction> {
        self.source.borrow().action_log().to_vec()
    }
}

impl SharedConversationScrollablePlace for SignalScrubChatPagePlace<'_> {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        let page_number = self.pages_read.get().saturating_add(1);
        let read = read_signal_messages_for_scrub(
            self.owner_osl_user_id,
            self.account_id,
            self.selected_place,
            self.signed_in_sender_id,
            &mut **self.source.borrow_mut(),
        )?;
        self.pages_read.set(page_number);
        if self.stop_when_reading_page.get() == Some(page_number) {
            if let Some(callback) = self.stop_request_callback.borrow().as_ref() {
                callback()?;
            }
        }
        Ok(read
            .messages
            .into_iter()
            .map(|message| SharedPlaceMessage::new(message.message_id, message.text))
            .collect())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let advanced = self.source.get_mut().scroll_one_screen()?;
        if advanced {
            self.one_screen_scrolls
                .set(self.one_screen_scrolls.get().saturating_add(1));
        }
        Ok(advanced)
    }
}
