//! Sender-key rotation triggers.
//!
//! Spec: `docs/design/sender-keys.md` "Rotation" + "Suspicious-event
//! auto-rotation" + "DoS / spoof cap" subsections.
//!
//! The controller is a per-(sender, group) state machine. The caller
//! is responsible for actually invoking
//! [`crypto::sender_keys::SenderChain::rotate`] when
//! [`RotationController::check_for_rotation`] returns `Some(reason)`,
//! then signalling completion via
//! [`RotationController::note_rotation_completed`].
//!
//! ## Triggers (per design doc)
//!
//! | Trigger              | Threshold                                  | Cap-exempt |
//! | ---                  | ---                                        | ---        |
//! | Time                 | 1 hour since last rotation                 | yes        |
//! | Message count        | 500 messages on the current chain          | yes        |
//! | Membership change    | Any add / remove                           | yes        |
//! | Recipient request    | "rotate now" message from any recipient    | yes        |
//! | Suspicious event     | See `SuspiciousEventKind`                  | **no**     |
//!
//! Cap on suspicious events: at most 1 rotation per 5-minute window;
//! further suspicious events queue and trigger one rotation when the
//! cooldown elapses. Time + message-count rotations are explicitly
//! exempt from this cap so attackers cannot suppress legitimate
//! rotations by spamming suspicious signals.
//!
//! ## Idle ("≥ 6 h inactivity") trigger
//!
//! Per the design doc, idle is a *suspicious* event (subject to the
//! 5-minute cap, not the time-based exemption). The controller
//! synthesises an `Idle` event in [`Self::check_for_rotation`] when
//! the configured `idle_trigger` has elapsed since the last
//! `note_message_sent` call **and** at least one message has been
//! sent on the current chain. The synthesised event is enqueued once
//! per chain — it doesn't keep firing every check.

use crate::clock::Clock;
use std::collections::VecDeque;
use std::time::Duration;

/// Configuration for [`RotationController`]. All durations are
/// inclusive lower bounds — e.g. exactly 1 hour since last rotation
/// fires the time trigger.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Hard ceiling on the rotation interval. Default: 1 hour.
    pub time_trigger: Duration,
    /// Cap on messages per chain. Default: 500.
    pub message_count_trigger: u32,
    /// Idle threshold (suspicious-event flavour). Default: 6 hours.
    pub idle_trigger: Duration,
    /// Suspicious-event DoS cap. Default: 5 minutes.
    pub suspicious_cap: Duration,
}

impl Default for RotationConfig {
    fn default() -> Self {
        RotationConfig {
            time_trigger: Duration::from_secs(60 * 60),
            message_count_trigger: 500,
            idle_trigger: Duration::from_secs(6 * 60 * 60),
            suspicious_cap: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspiciousEventKind {
    /// Hourly process scan detected a screen-recorder.
    ScreenRecorder,
    /// Win32 device-notification API saw a USB Video Class device
    /// with a capture descriptor (see `sender-keys.md` USB classes
    /// subsection).
    UsbCaptureDevice,
    /// Win32 power notification: app suspend / resume.
    Suspend,
    /// `idle_trigger` elapsed without an outbound message.
    Idle,
    /// Duress passphrase entered (forwarded from the unlock layer).
    Duress,
    /// Generic suspicious-event reporter, for triggers we add later
    /// without re-versioning the public enum.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationReason {
    Time,
    MessageCount,
    Membership,
    Recipient,
    Suspicious(SuspiciousEventKind),
}

impl RotationReason {
    pub fn is_cap_exempt(&self) -> bool {
        matches!(
            self,
            RotationReason::Time
                | RotationReason::MessageCount
                | RotationReason::Membership
                | RotationReason::Recipient,
        )
    }
}

/// State machine driving sender-key rotation.
///
/// Lifecycle:
/// 1. Construct with [`RotationController::new`]; the constructor
///    captures `clock.now()` as `last_rotation_at`.
/// 2. Caller drives:
///    - [`Self::note_message_sent`] before / after each outbound send.
///    - [`Self::note_suspicious_event`] when an OS-level signal fires.
///    - [`Self::note_membership_change`] / [`Self::note_recipient_request`]
///      for the unconditional triggers.
/// 3. Caller polls [`Self::check_for_rotation`] (e.g. once per send,
///    or via a periodic tick). When it returns `Some(reason)`, the
///    caller invokes the actual `SenderChain::rotate()` and then
///    calls [`Self::note_rotation_completed`] to reset counters.
pub struct RotationController {
    clock: Box<dyn Clock>,
    config: RotationConfig,

    last_rotation_at: std::time::Instant,
    last_message_at: Option<std::time::Instant>,
    messages_since_rotation: u32,

    /// `(detected_at, kind)` queue of suspicious events that haven't
    /// yet driven a rotation (either because the cooldown is active,
    /// or because they're enqueued and waiting for a `check`).
    queued_suspicious: VecDeque<(std::time::Instant, SuspiciousEventKind)>,

    /// Rolling cap window: timestamp of the most recent
    /// suspicious-event rotation, used with `config.suspicious_cap`.
    last_suspicious_rotation_at: Option<std::time::Instant>,

    /// Cap-exempt forced rotations (membership / recipient). FIFO so
    /// callers can stack two events; `check_for_rotation` drains one
    /// per call.
    forced_pending: VecDeque<RotationReason>,

    /// Idle is synthesised at most once per chain; this flag flips on
    /// after the synthesis to prevent repeated emissions every poll.
    idle_event_emitted: bool,
}

impl RotationController {
    pub fn new(clock: Box<dyn Clock>, config: RotationConfig) -> Self {
        let now = clock.now();
        RotationController {
            clock,
            config,
            last_rotation_at: now,
            last_message_at: None,
            messages_since_rotation: 0,
            queued_suspicious: VecDeque::new(),
            last_suspicious_rotation_at: None,
            forced_pending: VecDeque::new(),
            idle_event_emitted: false,
        }
    }

    pub fn config(&self) -> &RotationConfig {
        &self.config
    }

    pub fn messages_since_rotation(&self) -> u32 {
        self.messages_since_rotation
    }

    pub fn queued_suspicious_count(&self) -> usize {
        self.queued_suspicious.len()
    }

    pub fn forced_pending_count(&self) -> usize {
        self.forced_pending.len()
    }

    pub fn note_message_sent(&mut self) {
        let now = self.clock.now();
        self.last_message_at = Some(now);
        self.messages_since_rotation = self.messages_since_rotation.saturating_add(1);
        // A fresh message arms idle detection again; idle should fire
        // only after a NEW span of inactivity following this message.
        self.idle_event_emitted = false;
    }

    pub fn note_rotation_completed(&mut self) {
        self.last_rotation_at = self.clock.now();
        self.messages_since_rotation = 0;
        self.last_message_at = None;
        self.idle_event_emitted = false;
        // Anything queued at the moment of rotation is considered
        // serviced — we just rotated.
        self.queued_suspicious.clear();
        // Note: forced_pending is left intact — if a membership
        // change arrived during the rotation window, the caller wants
        // it serviced too. The rotation we just completed may have
        // been for a different reason.
    }

    pub fn note_suspicious_event(&mut self, kind: SuspiciousEventKind) {
        let now = self.clock.now();
        self.queued_suspicious.push_back((now, kind));
    }

    pub fn note_membership_change(&mut self) {
        self.forced_pending
            .push_back(RotationReason::Membership);
    }

    pub fn note_recipient_request(&mut self) {
        self.forced_pending.push_back(RotationReason::Recipient);
    }

    /// Returns the reason a rotation should fire now, if any.
    /// The caller is responsible for actually rotating the
    /// [`crypto::sender_keys::SenderChain`] and then calling
    /// [`Self::note_rotation_completed`].
    ///
    /// Order of precedence:
    /// 1. Cap-exempt forced rotations (membership / recipient).
    /// 2. Time elapsed since last rotation.
    /// 3. Message count exceeded.
    /// 4. Suspicious events (capped at 1 per `suspicious_cap`).
    pub fn check_for_rotation(&mut self) -> Option<RotationReason> {
        let now = self.clock.now();

        // 1. Forced (cap-exempt).
        if let Some(reason) = self.forced_pending.pop_front() {
            return Some(reason);
        }

        // 2. Time-based (cap-exempt).
        if now.duration_since(self.last_rotation_at) >= self.config.time_trigger {
            return Some(RotationReason::Time);
        }

        // 3. Message-count (cap-exempt).
        if self.messages_since_rotation >= self.config.message_count_trigger {
            return Some(RotationReason::MessageCount);
        }

        // 4. Synthesise idle event if the threshold has elapsed since
        //    the last outbound message.
        if !self.idle_event_emitted {
            if let Some(last_msg) = self.last_message_at {
                if now.duration_since(last_msg) >= self.config.idle_trigger {
                    self.queued_suspicious
                        .push_back((now, SuspiciousEventKind::Idle));
                    self.idle_event_emitted = true;
                }
            }
        }

        // 5. Suspicious events (subject to DoS cap).
        if self.queued_suspicious.is_empty() {
            return None;
        }
        let in_cooldown = self
            .last_suspicious_rotation_at
            .is_some_and(|t| now.duration_since(t) < self.config.suspicious_cap);
        if in_cooldown {
            return None;
        }
        // Drain the queue: per the design, "further suspicious events
        // within the 5-minute window queue and trigger ONE rotation
        // at the end of the window." We collapse the queue into a
        // single rotation regardless of how many events accumulated.
        let (_when, kind) = self
            .queued_suspicious
            .pop_front()
            .expect("non-empty queue checked above");
        self.queued_suspicious.clear();
        self.last_suspicious_rotation_at = Some(now);
        Some(RotationReason::Suspicious(kind))
    }
}
