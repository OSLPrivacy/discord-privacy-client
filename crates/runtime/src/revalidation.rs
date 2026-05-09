//! 5-minute re-validation cycle for visible wrapped-key blobs.
//!
//! Spec: `docs/design/key-server-api.md` § "Re-validation".
//!
//! ## Responsibilities
//!
//! - Track which `content_id`s are currently in the visible viewport.
//! - On each tick of the configured interval (default 5 min), probe
//!   each tracked content_id against the key server.
//! - Detect state transitions (`Present → Burned`, `Present →
//!   Tombstoned`, etc.) and fire a callback so the integration layer
//!   can re-render or zero local cache for that row.
//!
//! ## Non-responsibilities
//!
//! - HTTP. The caller supplies a [`Probe`] (typically backed by
//!   [`keystore::KeyServerClient`]). Tests substitute a deterministic
//!   `Vec<WrappedKeyState>` queue.
//! - Window-focus / interaction triggers. Those are higher-level
//!   integration signals; this module only owns the time trigger.
//!   Caller can force a probe round at any moment via
//!   [`RevalidationLoop::probe_now`].
//!
//! ## Design notes
//!
//! - Tick is **caller-driven** so unit tests don't need a real wall
//!   clock. The integration layer (in `src-tauri`) wires
//!   [`RevalidationLoop::poll`] to a tokio interval timer.
//! - Cover-text fallback semantics (per design doc § "404 fallback
//!   semantics") live alongside in [`render_decision`] so the
//!   integration layer always has one place to look.

use crate::clock::Clock;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Three-way result of a wrapped-key probe.
///
/// Mirrors the server's `GET /v1/wrapped-keys/:content_id` outcomes
/// (`200`, `404`, `410`) without coupling this module to HTTP types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrappedKeyState {
    /// Server returned `200 OK`.
    Present,
    /// Server returned `404 Not Found` — burned-or-never-existed.
    Burned,
    /// Server returned `410 Gone` — past expires_at, lazy-tombstoned.
    Tombstoned,
}

impl WrappedKeyState {
    /// Whether this state means "no longer renderable".
    pub fn is_gone(self) -> bool {
        matches!(self, WrappedKeyState::Burned | WrappedKeyState::Tombstoned)
    }
}

/// Coarse content-type categories matching the server's
/// `content_type` field. Per
/// `docs/design/key-server-api.md` § "404 fallback semantics".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Text,
    Attachment,
    System,
}

/// What the UI should render given a probe result and a content kind.
/// Only consulted on `Burned` / `Tombstoned`; the caller renders the
/// real plaintext on `Present`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderDecision {
    /// `text` — render the original Discord cover text as-is. NO
    /// "[deleted]" or "[unavailable]" marker (observer cannot
    /// distinguish burned from cover-text-only messages).
    KeepCoverText,
    /// `attachment` — show subtle `[attachment no longer available]`
    /// placeholder.
    AttachmentUnavailable,
    /// `system` — show `[notification no longer available]`. Not
    /// cover text: system messages have a known semantic role.
    SystemUnavailable,
}

/// Decide what to render for a wrapped-key row given probe state and
/// content kind. Hard-coded per design doc Table.
pub fn render_decision(state: WrappedKeyState, kind: ContentKind) -> Option<RenderDecision> {
    if !state.is_gone() {
        return None;
    }
    Some(match kind {
        ContentKind::Text => RenderDecision::KeepCoverText,
        ContentKind::Attachment => RenderDecision::AttachmentUnavailable,
        ContentKind::System => RenderDecision::SystemUnavailable,
    })
}

/// Strategy by which the loop probes the key server. Implementations
/// are typically thin wrappers over `KeyServerClient`. Tests use the
/// in-memory [`MockProbe`].
pub trait Probe: Send + Sync {
    fn probe(&self, content_id: &str) -> Result<WrappedKeyState, ProbeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("probe transport: {0}")]
    Transport(String),
    #[error("probe server status: {0}")]
    Status(u16),
}

/// Fired when a tracked content's state transitions.
pub type TransitionCallback = Box<dyn FnMut(&str, WrappedKeyState) + Send>;

#[derive(Debug, Clone)]
pub struct RevalidationConfig {
    /// 5-minute timer per design doc.
    pub interval: Duration,
}

impl Default for RevalidationConfig {
    fn default() -> Self {
        RevalidationConfig {
            interval: Duration::from_secs(5 * 60),
        }
    }
}

/// Per-content tracking entry. `kind` is retained so the integration
/// layer can pair the tracked content with the correct
/// [`render_decision`] when the loop reports a transition.
#[derive(Debug, Clone)]
struct Entry {
    kind: ContentKind,
    last_state: WrappedKeyState,
}

pub struct RevalidationLoop {
    clock: Box<dyn Clock>,
    probe: Box<dyn Probe>,
    config: RevalidationConfig,
    entries: BTreeMap<String, Entry>,
    last_tick: Instant,
}

impl RevalidationLoop {
    pub fn new(
        clock: Box<dyn Clock>,
        probe: Box<dyn Probe>,
        config: RevalidationConfig,
    ) -> Self {
        let last = clock.now();
        RevalidationLoop {
            clock,
            probe,
            config,
            entries: BTreeMap::new(),
            last_tick: last,
        }
    }

    /// Begin tracking `content_id` with its content kind. Caller
    /// supplies the initial state observed at fetch time (typically
    /// `Present`). No-op if already tracked.
    pub fn track(&mut self, content_id: impl Into<String>, kind: ContentKind, initial: WrappedKeyState) {
        let id = content_id.into();
        self.entries.entry(id).or_insert(Entry {
            kind,
            last_state: initial,
        });
    }

    /// Stop tracking `content_id`. Called when the row scrolls out of
    /// the viewport or the conversation is closed.
    pub fn untrack(&mut self, content_id: &str) {
        self.entries.remove(content_id);
    }

    pub fn is_tracked(&self, content_id: &str) -> bool {
        self.entries.contains_key(content_id)
    }

    pub fn tracked_count(&self) -> usize {
        self.entries.len()
    }

    pub fn last_state(&self, content_id: &str) -> Option<WrappedKeyState> {
        self.entries.get(content_id).map(|e| e.last_state)
    }

    /// Look up the [`ContentKind`] registered for `content_id`. Used
    /// by the integration layer to pair a transition callback with
    /// [`render_decision`].
    pub fn kind_of(&self, content_id: &str) -> Option<ContentKind> {
        self.entries.get(content_id).map(|e| e.kind)
    }

    /// Caller-driven tick. If the configured interval has elapsed
    /// since the last probe round, probes every tracked content and
    /// fires `cb` on each state transition. No-op if not yet due.
    /// Returns `true` iff a probe round actually ran.
    pub fn poll(&mut self, cb: &mut TransitionCallback) -> bool {
        let now = self.clock.now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        if elapsed < self.config.interval {
            return false;
        }
        self.last_tick = now;
        self.run_probe_round(cb);
        true
    }

    /// Force a probe round regardless of timer state. Wired to
    /// visibility-change and user-interaction triggers per design doc
    /// § "Re-validation triggers".
    pub fn probe_now(&mut self, cb: &mut TransitionCallback) {
        self.last_tick = self.clock.now();
        self.run_probe_round(cb);
    }

    fn run_probe_round(&mut self, cb: &mut TransitionCallback) {
        let ids: Vec<String> = self.entries.keys().cloned().collect();
        for id in ids {
            let probed = match self.probe.probe(&id) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let entry = match self.entries.get_mut(&id) {
                Some(e) => e,
                None => continue,
            };
            if entry.last_state != probed {
                entry.last_state = probed;
                cb(&id, probed);
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Test probe driven by a per-content_id queue. Each call to
    /// [`Probe::probe`] pops the front of the queue for that id; if
    /// the queue is exhausted it sticks at the last value.
    pub struct MockProbe {
        inner: Mutex<MockProbeInner>,
    }

    struct MockProbeInner {
        queues: HashMap<String, Vec<WrappedKeyState>>,
        last: HashMap<String, WrappedKeyState>,
    }

    impl MockProbe {
        pub fn new() -> Self {
            MockProbe {
                inner: Mutex::new(MockProbeInner {
                    queues: HashMap::new(),
                    last: HashMap::new(),
                }),
            }
        }

        pub fn enqueue(&self, content_id: impl Into<String>, states: Vec<WrappedKeyState>) {
            let mut g = self.inner.lock().unwrap();
            g.queues
                .entry(content_id.into())
                .or_default()
                .extend(states);
        }
    }

    impl Default for MockProbe {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Probe for MockProbe {
        fn probe(&self, content_id: &str) -> Result<WrappedKeyState, ProbeError> {
            let mut g = self.inner.lock().unwrap();
            if let Some(q) = g.queues.get_mut(content_id) {
                if let Some(state) = q.first().copied() {
                    q.remove(0);
                    g.last.insert(content_id.to_string(), state);
                    return Ok(state);
                }
            }
            Ok(*g.last
                .get(content_id)
                .unwrap_or(&WrappedKeyState::Present))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::MockProbe;
    use super::*;
    use crate::clock::MockClock;
    use std::sync::Arc;

    /// Newtype so an `Arc<MockClock>` (held by the test) can be
    /// shared with the loop — the loop takes a `Box<dyn Clock>`.
    struct ClockProxy(Arc<MockClock>);
    impl Clock for ClockProxy {
        fn now(&self) -> Instant {
            self.0.now()
        }
    }

    fn no_op_cb() -> TransitionCallback {
        Box::new(|_id, _state| {})
    }

    fn collect_cb(events: Arc<std::sync::Mutex<Vec<(String, WrappedKeyState)>>>) -> TransitionCallback {
        Box::new(move |id, state| {
            events.lock().unwrap().push((id.to_string(), state));
        })
    }

    #[test]
    fn render_decision_matches_design_doc_table() {
        assert_eq!(
            render_decision(WrappedKeyState::Burned, ContentKind::Text),
            Some(RenderDecision::KeepCoverText)
        );
        assert_eq!(
            render_decision(WrappedKeyState::Burned, ContentKind::Attachment),
            Some(RenderDecision::AttachmentUnavailable)
        );
        assert_eq!(
            render_decision(WrappedKeyState::Burned, ContentKind::System),
            Some(RenderDecision::SystemUnavailable)
        );
        assert_eq!(
            render_decision(WrappedKeyState::Tombstoned, ContentKind::Text),
            Some(RenderDecision::KeepCoverText)
        );
        // Present → no fallback decision.
        assert_eq!(
            render_decision(WrappedKeyState::Present, ContentKind::Text),
            None
        );
    }

    fn build_loop(
        clock: Arc<MockClock>,
        probe: MockProbe,
        config: RevalidationConfig,
    ) -> RevalidationLoop {
        RevalidationLoop::new(Box::new(ClockProxy(clock)), Box::new(probe), config)
    }

    #[test]
    fn poll_below_interval_is_noop() {
        let clock = Arc::new(MockClock::new());
        let mut loop_ = build_loop(
            clock,
            MockProbe::new(),
            RevalidationConfig::default(),
        );
        loop_.track("c1", ContentKind::Text, WrappedKeyState::Present);
        let ran = loop_.poll(&mut no_op_cb());
        assert!(!ran);
    }

    #[test]
    fn poll_at_interval_fires_callback_on_transition() {
        let clock = Arc::new(MockClock::new());
        let probe = MockProbe::new();
        probe.enqueue("c1", vec![WrappedKeyState::Burned]);
        probe.enqueue("c2", vec![WrappedKeyState::Present]);
        let mut loop_ = build_loop(
            clock.clone(),
            probe,
            RevalidationConfig {
                interval: Duration::from_secs(60),
            },
        );
        loop_.track("c1", ContentKind::Text, WrappedKeyState::Present);
        loop_.track("c2", ContentKind::Attachment, WrappedKeyState::Present);

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut cb = collect_cb(events.clone());

        // 0s — not due yet.
        assert!(!loop_.poll(&mut cb));
        clock.advance(Duration::from_secs(60));
        assert!(loop_.poll(&mut cb));
        let got = events.lock().unwrap().clone();
        // c1 transitioned, c2 didn't.
        assert_eq!(got, vec![("c1".to_string(), WrappedKeyState::Burned)]);
    }

    #[test]
    fn probe_now_fires_immediately() {
        let clock = Arc::new(MockClock::new());
        let probe = MockProbe::new();
        probe.enqueue("c1", vec![WrappedKeyState::Burned]);
        let mut loop_ = build_loop(clock, probe, RevalidationConfig::default());
        loop_.track("c1", ContentKind::Text, WrappedKeyState::Present);

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut cb = collect_cb(events.clone());
        loop_.probe_now(&mut cb);
        assert_eq!(
            events.lock().unwrap().clone(),
            vec![("c1".to_string(), WrappedKeyState::Burned)]
        );
        // Sticks at Burned: another probe_now with no queue change → no transition.
        events.lock().unwrap().clear();
        loop_.probe_now(&mut cb);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn untrack_stops_probing() {
        let clock = Arc::new(MockClock::new());
        let probe = MockProbe::new();
        probe.enqueue("c1", vec![WrappedKeyState::Burned]);
        let mut loop_ = build_loop(clock, probe, RevalidationConfig::default());
        loop_.track("c1", ContentKind::Text, WrappedKeyState::Present);
        loop_.untrack("c1");
        assert_eq!(loop_.tracked_count(), 0);

        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut cb = collect_cb(events.clone());
        loop_.probe_now(&mut cb);
        assert!(events.lock().unwrap().is_empty());
    }

    #[test]
    fn last_state_query_reflects_latest_probe() {
        let clock = Arc::new(MockClock::new());
        let probe = MockProbe::new();
        probe.enqueue("c1", vec![WrappedKeyState::Tombstoned]);
        let mut loop_ = build_loop(clock, probe, RevalidationConfig::default());
        loop_.track("c1", ContentKind::System, WrappedKeyState::Present);
        loop_.probe_now(&mut no_op_cb());
        assert_eq!(loop_.last_state("c1"), Some(WrappedKeyState::Tombstoned));
    }

    #[test]
    fn double_track_is_idempotent() {
        let clock = Arc::new(MockClock::new());
        let mut loop_ = build_loop(
            clock,
            MockProbe::new(),
            RevalidationConfig::default(),
        );
        loop_.track("c1", ContentKind::Text, WrappedKeyState::Present);
        loop_.track("c1", ContentKind::Attachment, WrappedKeyState::Burned);
        // Idempotent: first registration wins; subsequent calls don't
        // overwrite (the integration layer must untrack first to
        // re-classify).
        assert_eq!(loop_.last_state("c1"), Some(WrappedKeyState::Present));
    }
}
