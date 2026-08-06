#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedPlaceMessage {
    pub id: String,
    pub text: String,
}

impl SharedPlaceMessage {
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

pub trait SharedConversationScrollablePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String>;
    fn scroll_one_screen(&mut self) -> Result<bool, String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedConversationScrollStop {
    EndOfPlace,
    PageLimitReached,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedConversationPageLog {
    pub page_number: usize,
    pub messages_on_screen: usize,
    pub new_messages_read: usize,
    pub cumulative_messages_read: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedConversationScrollRead {
    pub messages: Vec<SharedPlaceMessage>,
    pub page_log: Vec<SharedConversationPageLog>,
    pub stop_reason: SharedConversationScrollStop,
}

impl SharedConversationScrollRead {
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn page_count(&self) -> usize {
        self.page_log.len()
    }
}

pub fn read_shared_conversation_messages_one_page_at_a_time(
    place: &mut impl SharedConversationScrollablePlace,
    page_limit: usize,
) -> Result<SharedConversationScrollRead, String> {
    if page_limit == 0 {
        return Err("shared conversation scroll page limit must be at least one".to_owned());
    }

    let mut messages = Vec::new();
    let mut seen_ids = std::collections::BTreeSet::new();
    let mut page_log = Vec::new();

    loop {
        let page_number = page_log.len() + 1;
        let screen = place.read_current_screen()?;
        let messages_on_screen = screen.len();
        let before = messages.len();

        for message in screen {
            if seen_ids.insert(message.id.clone()) {
                messages.push(message);
            }
        }

        page_log.push(SharedConversationPageLog {
            page_number,
            messages_on_screen,
            new_messages_read: messages.len() - before,
            cumulative_messages_read: messages.len(),
        });

        if page_log.len() >= page_limit {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                stop_reason: SharedConversationScrollStop::PageLimitReached,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedPagePlace {
        messages: Vec<SharedPlaceMessage>,
        current_page: usize,
        page_size: usize,
        scrolls: usize,
    }

    impl FixedPagePlace {
        fn with_messages(total_messages: usize, page_size: usize) -> Self {
            Self {
                messages: (1..=total_messages)
                    .map(|index| {
                        SharedPlaceMessage::new(
                            format!("message-{index:03}"),
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
            self.current_page += 1;
            self.scrolls += 1;
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
