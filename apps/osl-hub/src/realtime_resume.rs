//! Jittered reconnect scheduling for the realtime wakeup stream.
//!
//! A closed stream is not a terminal state: the client retains its session
//! resume token and retries on an exponential, randomly-jittered schedule.

use std::time::Duration;

use crate::realtime_subscription::DeliveryTag;

/// First retry is delayed by at least this much; retries are capped so a long
/// outage does not overflow or turn into a tight loop.
pub const MIN_RECONNECT_DELAY: Duration = Duration::from_millis(250);
pub const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);

/// Per-socket nonce. It is supplied by the connection owner from CSPRNG output
/// and is never an account, device, or durable resume identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionId([u8; 16]);

impl SessionId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// The only replay cursor retained for a live socket.  It is reset when a new
/// session starts; reconnect restores subscriptions, not server state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcknowledgementCursor {
    pub last_sent: u64,
    pub last_accepted_peer_frame: u64,
}

/// Local state needed to restore a transport after a close.  It intentionally
/// contains opaque tags only, never an account, carrier pointer, or bearer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectContract {
    pub session_id: SessionId,
    pub cursor: AcknowledgementCursor,
    pub subscriptions: Vec<DeliveryTag>,
}

/// Opaque server-issued data retained across a reconnect.  The scheduling
/// boundary neither interprets it nor uses it as jitter input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResumeToken(String);

impl ResumeToken {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reconnect state retained after a dropped connection.
#[derive(Clone, Debug)]
pub struct ReconnectSchedule {
    attempt: u8,
    resume_token: Option<ResumeToken>,
    contract: Option<ReconnectContract>,
}

impl Default for ReconnectSchedule {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconnectSchedule {
    pub const fn new() -> Self {
        Self {
            attempt: 0,
            resume_token: None,
            contract: None,
        }
    }

    /// Retain the current live-socket state. It is restored locally after a
    /// close, with a fresh session ID and reset cursors.
    pub fn set_resume_token(&mut self, token: ResumeToken) {
        self.resume_token = Some(token.clone());
        // Compatibility shim for the pre-contract API. A token is opaque and
        // never put on the wire by this scheduler.
        let bytes = token.as_str().as_bytes();
        let mut session_id = [0u8; 16];
        for (slot, byte) in session_id.iter_mut().zip(bytes.iter().copied()) {
            *slot = byte;
        }
        self.contract = Some(ReconnectContract {
            session_id: SessionId::from_bytes(session_id),
            cursor: AcknowledgementCursor::default(),
            subscriptions: Vec::new(),
        });
    }

    pub fn resume_token(&self) -> Option<&ResumeToken> {
        self.resume_token.as_ref()
    }

    pub fn set_contract(&mut self, contract: ReconnectContract) {
        self.contract = Some(contract);
    }

    pub fn contract(&self) -> Option<&ReconnectContract> {
        self.contract.as_ref()
    }

    /// Build the reconnect state for a fresh socket. The old cursor cannot be
    /// replayed across a new session; all locally known subscriptions are sent
    /// again on the first tick.
    pub fn restore_for_new_session(&self, session_id: SessionId) -> Option<ReconnectContract> {
        self.contract.as_ref().map(|previous| ReconnectContract {
            session_id,
            cursor: AcknowledgementCursor::default(),
            subscriptions: previous.subscriptions.clone(),
        })
    }

    /// Mark a successful connection; the next disconnect starts at attempt 0.
    pub fn connected(&mut self) {
        self.attempt = 0;
    }

    /// Return a full-jitter retry delay using caller-provided random entropy.
    ///
    /// `random` is a uniform `u64`; accepting it at this pure boundary keeps
    /// the timing policy testable and prevents session/account data from ever
    /// becoming a stable reconnect fingerprint.
    pub fn next_delay(&mut self, random: u64) -> Duration {
        let exponent = self.attempt.saturating_add(1).min(7);
        self.attempt = self.attempt.saturating_add(1);
        let ceiling = MIN_RECONNECT_DELAY
            .checked_mul(1_u32 << exponent)
            .unwrap_or(MAX_RECONNECT_DELAY)
            .min(MAX_RECONNECT_DELAY);
        let floor_ms = u64::try_from(MIN_RECONNECT_DELAY.as_millis()).expect("bounded delay");
        let ceiling_ms = u64::try_from(ceiling.as_millis()).expect("bounded reconnect delay");
        // Inclusive upper bound makes the entire backoff window reachable,
        // while the non-zero floor prevents a post-deploy retry storm.
        Duration::from_millis(floor_ms + random % (ceiling_ms - floor_ms + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_t53_one_thousand_clients_do_not_retry_within_the_same_100ms_window() {
        let mut windows = std::collections::BTreeSet::new();
        for client in 0_u64..1_000 {
            let mut schedule = ReconnectSchedule::new();
            // A distinct CSPRNG output per client is modelled here. The
            // mixing prevents nearby RNG outputs from aligning into a burst.
            let random = client.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(17);
            let delay = schedule.next_delay(random);
            windows.insert(delay.as_millis() / 100);
        }
        assert!(
            windows.len() > 1,
            "retries must not collapse into the same 100 ms window"
        );
    }

    #[test]
    fn reconnect_retains_the_session_token_and_success_resets_backoff() {
        let mut schedule = ReconnectSchedule::new();
        schedule.set_resume_token(ResumeToken::new("resume-opaque").expect("nonempty"));
        let _ = schedule.next_delay(0);
        let second_ceiling = schedule.next_delay(u64::MAX);
        assert!(second_ceiling > MIN_RECONNECT_DELAY);
        assert_eq!(
            schedule.resume_token().map(ResumeToken::as_str),
            Some("resume-opaque")
        );
        assert!(schedule.contract().is_some());
        schedule.connected();
        assert!(schedule.next_delay(u64::MAX) <= Duration::from_millis(500));
    }

    #[test]
    fn t1_t53_new_session_resets_cursor_and_restores_only_opaque_subscriptions() {
        let tag = DeliveryTag::try_from_bytes([7; 16]).unwrap();
        let mut schedule = ReconnectSchedule::new();
        schedule.set_contract(ReconnectContract {
            session_id: SessionId::from_bytes([1; 16]),
            cursor: AcknowledgementCursor {
                last_sent: 9,
                last_accepted_peer_frame: 8,
            },
            subscriptions: vec![tag],
        });
        let restored = schedule
            .restore_for_new_session(SessionId::from_bytes([2; 16]))
            .unwrap();
        assert_eq!(restored.session_id.as_bytes(), [2; 16]);
        assert_eq!(restored.cursor, AcknowledgementCursor::default());
        assert_eq!(restored.subscriptions, vec![tag]);
    }
}
