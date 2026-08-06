//! Shared one-page-at-a-time reader for service conversation places.
//!
//! Service adapters own the provider-specific accessibility or DOM work. This
//! helper owns the common bounded loop: read the visible screen, log one page,
//! advance exactly one screen, and stop at the caller's page limit or the end of
//! the place.

use std::collections::HashSet;

use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedPlaceMessage {
    pub message_id: String,
    pub text: String,
}

impl SharedPlaceMessage {
    pub fn new(message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
            text: text.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedConversationScrollStop {
    EndOfPlace,
    NoNewMessages,
    PageLimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedConversationScrollPageLog {
    pub page_number: usize,
    pub messages_on_screen: usize,
    pub new_messages_read: usize,
    pub cumulative_messages_read: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedConversationScrollRead {
    pub messages: Vec<SharedPlaceMessage>,
    pub page_log: Vec<SharedConversationScrollPageLog>,
    pub stop_reason: SharedConversationScrollStop,
}

impl SharedConversationScrollRead {
    pub fn page_count(&self) -> usize {
        self.page_log.len()
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }
}

pub trait SharedConversationScrollablePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String>;
    fn scroll_one_screen(&mut self) -> Result<bool, String>;
}

pub fn read_shared_conversation_messages_one_page_at_a_time<P>(
    place: &mut P,
    page_limit: usize,
) -> Result<SharedConversationScrollRead, String>
where
    P: SharedConversationScrollablePlace,
{
    if page_limit == 0 {
        return Err("shared conversation scroll page limit must be at least one".to_owned());
    }

    let mut messages = Vec::new();
    let mut page_log = Vec::new();
    let mut seen_message_ids = HashSet::new();

    for page_number in 1..=page_limit {
        let screen = place.read_current_screen()?;
        let messages_on_screen = screen.len();
        let mut new_messages_read = 0usize;

        for message in screen {
            validate_message(&message)?;
            if seen_message_ids.insert(message.message_id.clone()) {
                messages.push(message);
                new_messages_read += 1;
            }
        }

        page_log.push(SharedConversationScrollPageLog {
            page_number,
            messages_on_screen,
            new_messages_read,
            cumulative_messages_read: messages.len(),
        });

        if page_number == page_limit {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                stop_reason: SharedConversationScrollStop::PageLimitReached,
            });
        }

        if new_messages_read == 0 {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                stop_reason: SharedConversationScrollStop::NoNewMessages,
            });
        }

        if !place.scroll_one_screen()? {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                stop_reason: SharedConversationScrollStop::EndOfPlace,
            });
        }
    }

    unreachable!("the loop always returns from its page-limit branch");
}

fn validate_message(message: &SharedPlaceMessage) -> Result<(), String> {
    if message.message_id.trim().is_empty() {
        return Err("shared conversation message id must not be empty".to_owned());
    }
    if message.message_id.chars().any(char::is_control) {
        return Err("shared conversation message id must be printable".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        read_shared_conversation_messages_one_page_at_a_time, SharedConversationScrollStop,
        SharedConversationScrollablePlace, SharedPlaceMessage,
    };

    struct FixedPagePlace {
        messages: Vec<SharedPlaceMessage>,
        current_page: usize,
        page_size: usize,
        scrolls: usize,
    }

    impl FixedPagePlace {
        fn with_messages(total: usize, page_size: usize) -> Self {
            Self {
                messages: (1..=total)
                    .map(|index| {
                        SharedPlaceMessage::new(
                            format!("task-3003-message-{index:03}"),
                            format!("test message {index:03}"),
                        )
                    })
                    .collect(),
                current_page: 0,
                page_size,
                scrolls: 0,
            }
        }
    }

    impl SharedConversationScrollablePlace for FixedPagePlace {
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
            self.scrolls += 1;
            self.current_page += 1;
            Ok(true)
        }
    }

    #[test]
    fn reads_each_page_until_the_place_ends() {
        let mut place = FixedPagePlace::with_messages(120, 40);
        let read = read_shared_conversation_messages_one_page_at_a_time(&mut place, 10)
            .expect("bounded scroll should read the full place");

        assert_eq!(read.message_count(), 120);
        assert_eq!(read.page_count(), 3);
        assert_eq!(read.page_log.len(), 3);
        assert_eq!(read.stop_reason, SharedConversationScrollStop::EndOfPlace);
        assert_eq!(place.scrolls, 2);
        assert!(read
            .page_log
            .iter()
            .all(|entry| entry.messages_on_screen == 40 && entry.new_messages_read == 40));
    }

    #[test]
    fn stops_at_the_page_limit() {
        let mut place = FixedPagePlace::with_messages(120, 40);
        let read = read_shared_conversation_messages_one_page_at_a_time(&mut place, 2)
            .expect("bounded scroll should stop at the caller page limit");

        assert_eq!(read.message_count(), 80);
        assert_eq!(read.page_count(), 2);
        assert_eq!(
            read.stop_reason,
            SharedConversationScrollStop::PageLimitReached
        );
        assert_eq!(place.scrolls, 1);
    }
}
