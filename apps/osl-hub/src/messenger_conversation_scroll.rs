//! Messenger's one-screen-at-a-time adapter for the shared Scrub scroll.
//!
//! Authorization and browser observation belong to `messenger_message_reader`.
//! This type receives its resulting rows for one conversation and exposes only
//! the current screen to the generic reader; advancing it always means exactly
//! one Messenger screen.

use crate::messenger_message_reader::SharedMessengerMessage;
use crate::shared_conversation_scroll::{SharedConversationScrollablePlace, SharedPlaceMessage};

pub struct MessengerConversationPagePlace {
    conversation_id: String,
    messages: Vec<SharedMessengerMessage>,
    current_page: usize,
    page_size: usize,
    one_screen_scrolls: usize,
    stop_when_reading_page: Option<usize>,
    stop_request_callback: Option<Box<dyn Fn() -> Result<(), String>>>,
}

impl std::fmt::Debug for MessengerConversationPagePlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessengerConversationPagePlace")
            .field("conversation_id", &self.conversation_id)
            .field("message_count", &self.messages.len())
            .field("current_page", &self.current_page)
            .field("page_size", &self.page_size)
            .field("one_screen_scrolls", &self.one_screen_scrolls)
            .field("stop_when_reading_page", &self.stop_when_reading_page)
            .finish()
    }
}

impl MessengerConversationPagePlace {
    pub fn new(
        conversation_id: impl Into<String>,
        messages: Vec<SharedMessengerMessage>,
        page_size: usize,
    ) -> Result<Self, String> {
        let conversation_id = conversation_id.into();
        if conversation_id.trim().is_empty() || conversation_id.chars().any(char::is_control) {
            return Err("Messenger conversation id is invalid".to_owned());
        }
        if page_size == 0 {
            return Err("Messenger screen size must be at least one".to_owned());
        }
        if messages
            .iter()
            .any(|message| message.place_id != conversation_id)
        {
            return Err("Messenger messages do not belong to the selected conversation".to_owned());
        }
        Ok(Self {
            conversation_id,
            messages,
            current_page: 0,
            page_size,
            one_screen_scrolls: 0,
            stop_when_reading_page: None,
            stop_request_callback: None,
        })
    }

    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls
    }

    pub fn request_stop_when_reading_page(
        &mut self,
        page_number: usize,
        callback: impl Fn() -> Result<(), String> + 'static,
    ) {
        self.stop_when_reading_page = Some(page_number);
        self.stop_request_callback = Some(Box::new(callback));
    }
}

impl SharedConversationScrollablePlace for MessengerConversationPagePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        if self.stop_when_reading_page == Some(self.current_page.saturating_add(1)) {
            if let Some(callback) = &self.stop_request_callback {
                callback()?;
            }
        }
        let start = self.current_page.saturating_mul(self.page_size);
        let end = start
            .saturating_add(self.page_size)
            .min(self.messages.len());
        Ok(self.messages[start..end]
            .iter()
            .map(|message| {
                SharedPlaceMessage::new(message.message_id.clone(), message.text.clone())
            })
            .collect())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let next_start = (self.current_page + 1).saturating_mul(self.page_size);
        if next_start >= self.messages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        self.one_screen_scrolls += 1;
        Ok(true)
    }
}
