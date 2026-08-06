//! Shared hosted reader for bounded, page-at-a-time scans.
//!
//! The reader owns no provider-specific selectors. Provider adapters supply the
//! current page and the one-page scroll primitive; this module enforces the
//! common pacing and action ordering before either primitive is called.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedReaderMessage {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedReaderOptions {
    pub max_pages: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedReaderStopReason {
    EndOfPlace,
    PageLimitReached,
    StopRequested,
}

impl SharedReaderStopReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EndOfPlace => "EndOfPlace",
            Self::PageLimitReached => "PageLimitReached",
            Self::StopRequested => "StopRequested",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedReaderActionKind {
    Open,
    Scroll,
}

impl SharedReaderActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Scroll => "scroll",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedReaderActionLog {
    pub kind: SharedReaderActionKind,
    pub page: usize,
    pub pause_ms: u64,
    pub start_ms: u64,
    pub end_ms: u64,
}

impl fmt::Display for SharedReaderActionLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:page={}:pause={}ms:{}-{}",
            self.kind.as_str(),
            self.page,
            self.pause_ms,
            self.start_ms,
            self.end_ms
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedReaderPageLog {
    pub page: usize,
    pub messages_on_screen: usize,
    pub new_messages_read: usize,
    pub cumulative_messages_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedReaderRun {
    pub messages: Vec<SharedReaderMessage>,
    pub pages_read: usize,
    pub stop_reason: SharedReaderStopReason,
    pub action_log: Vec<SharedReaderActionLog>,
    pub page_log: Vec<SharedReaderPageLog>,
    pub total_run_ms: u64,
    pub max_parallel_actions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedReaderProgress {
    pub page: usize,
    pub messages_read: usize,
}

/// The single pacing rule used by shared hosted reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolitePace {
    pause_ms: u64,
    now_ms: u64,
    active_actions: usize,
    max_parallel_actions: usize,
}

impl PolitePace {
    pub const fn new(pause_ms: u64) -> Self {
        Self {
            pause_ms,
            now_ms: 0,
            active_actions: 0,
            max_parallel_actions: 0,
        }
    }

    pub const fn pause_ms(&self) -> u64 {
        self.pause_ms
    }

    pub const fn elapsed_ms(&self) -> u64 {
        self.now_ms
    }

    pub const fn max_parallel_actions(&self) -> usize {
        self.max_parallel_actions
    }

    fn paced_action(&mut self, kind: SharedReaderActionKind, page: usize) -> SharedReaderActionLog {
        self.now_ms = self.now_ms.saturating_add(self.pause_ms);
        self.active_actions += 1;
        self.max_parallel_actions = self.max_parallel_actions.max(self.active_actions);
        let start_ms = self.now_ms;
        let end_ms = start_ms.saturating_add(1);
        self.now_ms = end_ms;
        self.active_actions -= 1;
        SharedReaderActionLog {
            kind,
            page,
            pause_ms: self.pause_ms,
            start_ms,
            end_ms,
        }
    }
}

pub trait SharedReaderSource {
    fn open_visible_page(&mut self) -> Vec<SharedReaderMessage>;
    fn scroll_one_page(&mut self) -> bool;
}

pub fn read_shared_conversation_messages_one_page_at_a_time<S>(
    source: &mut S,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
) -> SharedReaderRun
where
    S: SharedReaderSource,
{
    read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace(
        source,
        options,
        pace,
        |_| false,
    )
}

pub fn read_shared_conversation_messages_one_page_at_a_time_with_gate_and_pace<S, Stop>(
    source: &mut S,
    options: SharedReaderOptions,
    pace: &mut PolitePace,
    mut stop_requested: Stop,
) -> SharedReaderRun
where
    S: SharedReaderSource,
    Stop: FnMut(SharedReaderProgress) -> bool,
{
    let mut messages = Vec::new();
    let mut action_log = Vec::new();
    let mut page_log = Vec::new();
    let mut pages_read = 0;
    let stop_reason;

    if options.max_pages == 0 {
        return SharedReaderRun {
            messages,
            pages_read,
            stop_reason: SharedReaderStopReason::PageLimitReached,
            action_log,
            page_log,
            total_run_ms: pace.elapsed_ms(),
            max_parallel_actions: pace.max_parallel_actions(),
        };
    }

    loop {
        let page = pages_read + 1;
        action_log.push(pace.paced_action(SharedReaderActionKind::Open, page));
        let visible = source.open_visible_page();
        let before = messages.len();
        messages.extend(visible);
        pages_read += 1;
        page_log.push(SharedReaderPageLog {
            page,
            messages_on_screen: messages.len() - before,
            new_messages_read: messages.len() - before,
            cumulative_messages_read: messages.len(),
        });

        if stop_requested(SharedReaderProgress {
            page,
            messages_read: messages.len(),
        }) {
            stop_reason = SharedReaderStopReason::StopRequested;
            break;
        }

        if pages_read >= options.max_pages {
            stop_reason = SharedReaderStopReason::PageLimitReached;
            break;
        }

        action_log.push(pace.paced_action(SharedReaderActionKind::Scroll, page));
        if !source.scroll_one_page() {
            stop_reason = SharedReaderStopReason::EndOfPlace;
            break;
        }
    }

    SharedReaderRun {
        messages,
        pages_read,
        stop_reason,
        action_log,
        page_log,
        total_run_ms: pace.elapsed_ms(),
        max_parallel_actions: pace.max_parallel_actions(),
    }
}
