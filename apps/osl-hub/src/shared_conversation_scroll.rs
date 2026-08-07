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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedPlaceMessage {
    pub id: String,
    pub text: String,
}

impl SharedPlaceMessage {
    pub fn new(message_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            message_id: message_id.into(),
    pub fn new(id: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
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
    PauseRequested,
    StopRequested,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedConversationScrollPageLog {
    pub page_number: usize,
    pub messages_on_screen: usize,
    pub new_messages_read: usize,
    pub cumulative_messages_read: usize,
    pub gate_after_page: SharedConversationScrollGateState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedConversationScrollActionKind {
    Open,
    Scroll,
}

impl SharedConversationScrollActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Scroll => "scroll",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedConversationScrollActionLog {
    pub kind: SharedConversationScrollActionKind,
    pub page_number: usize,
    pub pause_ms: u64,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedConversationScrollPace {
    pause_ms: u64,
    clock_ms: u64,
    active_actions: usize,
    max_parallel_actions: usize,
}

impl SharedConversationScrollPace {
    pub fn new(pause_ms: u64) -> Self {
        Self {
            pause_ms,
            clock_ms: 0,
            active_actions: 0,
            max_parallel_actions: 0,
        }
    }

    pub fn immediate() -> Self {
        Self::new(0)
    }

    pub fn pause_ms(&self) -> u64 {
        self.pause_ms
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.clock_ms
    }

    pub fn max_parallel_actions(&self) -> usize {
        self.max_parallel_actions
    }

    fn paced_action(
        &mut self,
        kind: SharedConversationScrollActionKind,
        page_number: usize,
    ) -> SharedConversationScrollActionLog {
        self.clock_ms = self.clock_ms.saturating_add(self.pause_ms);
        self.active_actions = self.active_actions.saturating_add(1);
        self.max_parallel_actions = self.max_parallel_actions.max(self.active_actions);
        let started_at_ms = self.clock_ms;
        self.clock_ms = self.clock_ms.saturating_add(1);
        self.active_actions = self.active_actions.saturating_sub(1);
        SharedConversationScrollActionLog {
            kind,
            page_number,
            pause_ms: self.pause_ms,
            started_at_ms,
            finished_at_ms: self.clock_ms,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedConversationScrollRead {
    pub messages: Vec<SharedPlaceMessage>,
    pub page_log: Vec<SharedConversationScrollPageLog>,
    pub action_log: Vec<SharedConversationScrollActionLog>,
    pub stop_reason: SharedConversationScrollStop,
    pub stopped_on_page_number: Option<usize>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedConversationScrollGateState {
    Running,
    PauseAfterCurrentPage,
    StopAfterCurrentPage,
}

pub trait SharedConversationScrollGate {
    fn state_between_pages(&self) -> Result<SharedConversationScrollGateState, String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoSharedConversationScrollGate;

impl SharedConversationScrollGate for NoSharedConversationScrollGate {
    fn state_between_pages(&self) -> Result<SharedConversationScrollGateState, String> {
        Ok(SharedConversationScrollGateState::Running)
    }
}

pub fn read_shared_conversation_messages_one_page_at_a_time<P>(
    place: &mut P,
    page_limit: usize,
) -> Result<SharedConversationScrollRead, String>
where
    P: SharedConversationScrollablePlace,
{
    read_shared_conversation_messages_one_page_at_a_time_with_gate(
        place,
        page_limit,
        &NoSharedConversationScrollGate,
    )
}

pub fn read_shared_conversation_messages_one_page_at_a_time_with_gate<P, G>(
    place: &mut P,
    page_limit: usize,
    gate: &G,
) -> Result<SharedConversationScrollRead, String>
where
    P: SharedConversationScrollablePlace,
    G: SharedConversationScrollGate + ?Sized,
{
    let mut pace = SharedConversationScrollPace::immediate();
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        place, page_limit, gate, &mut pace,
    )
}

pub fn read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace<P, G>(
    place: &mut P,
    page_limit: usize,
    gate: &G,
    pace: &mut SharedConversationScrollPace,
) -> Result<SharedConversationScrollRead, String>
where
    P: SharedConversationScrollablePlace,
    G: SharedConversationScrollGate + ?Sized,
{
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
    let mut page_log = Vec::new();
    let mut action_log = Vec::new();
    let mut seen_message_ids = HashSet::new();

    for page_number in 1..=page_limit {
        action_log.push(pace.paced_action(SharedConversationScrollActionKind::Open, page_number));
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

        let gate_after_page = gate.state_between_pages()?;
        page_log.push(SharedConversationScrollPageLog {
            page_number,
            messages_on_screen,
            new_messages_read,
            cumulative_messages_read: messages.len(),
            gate_after_page,
        });

        match gate_after_page {
            SharedConversationScrollGateState::Running => {}
            SharedConversationScrollGateState::PauseAfterCurrentPage => {
                return Ok(SharedConversationScrollRead {
                    messages,
                    page_log,
                    action_log,
                    stop_reason: SharedConversationScrollStop::PauseRequested,
                    stopped_on_page_number: Some(page_number),
                });
            }
            SharedConversationScrollGateState::StopAfterCurrentPage => {
                return Ok(SharedConversationScrollRead {
                    messages,
                    page_log,
                    action_log,
                    stop_reason: SharedConversationScrollStop::StopRequested,
                    stopped_on_page_number: Some(page_number),
                });
            }
        }

        });

        if page_number == page_limit {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                action_log,
                stop_reason: SharedConversationScrollStop::PageLimitReached,
                stopped_on_page_number: None,
                stop_reason: SharedConversationScrollStop::PageLimitReached,
            });
        }

        if new_messages_read == 0 {
            return Ok(SharedConversationScrollRead {
                messages,
                page_log,
                action_log,
                stop_reason: SharedConversationScrollStop::NoNewMessages,
                stopped_on_page_number: None,
                stop_reason: SharedConversationScrollStop::NoNewMessages,
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
                action_log,
                stop_reason: SharedConversationScrollStop::EndOfPlace,
                stopped_on_page_number: None,
            });
        }
        action_log.push(pace.paced_action(SharedConversationScrollActionKind::Scroll, page_number));
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
        read_shared_conversation_messages_one_page_at_a_time, SharedConversationScrollGateState,
        SharedConversationScrollStop, SharedConversationScrollablePlace, SharedPlaceMessage,
        read_shared_conversation_messages_one_page_at_a_time, SharedConversationScrollStop,
        SharedConversationScrollablePlace, SharedPlaceMessage,
    };
    use super::*;

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
            self.scrolls += 1;
            self.current_page += 1;
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
            .all(|entry| entry.messages_on_screen == 40
                && entry.new_messages_read == 40
                && entry.gate_after_page == SharedConversationScrollGateState::Running));
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
        assert_eq!(read.stopped_on_page_number, None);
    }
}
