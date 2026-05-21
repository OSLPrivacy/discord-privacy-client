//! Auto-recovery throttle + replay guard (in-memory).
//!
//! Backs the SKDM_REQUEST / SESSION_RESET control messages
//! (`crate::control_messages`, wire types 0x06 / 0x07). The recovery
//! requests are a deliberate denial-of-service surface: anyone who can
//! post into a channel could spam "re-send your key" / "reset our
//! session" to force endless re-handshakes so messages never decrypt.
//! This guard is the mitigation layer. It holds no secret material and
//! is intentionally NOT persisted — a relaunch resets it, whose worst
//! case is one extra (idempotent, self-correcting) recovery round.
//!
//! Defenses, applied by the recv handlers in `commands.rs`:
//!
//! 1. **Staleness** — a request whose `requested_at` is outside
//!    [`RECOVERY_FRESHNESS_SECS`] of now (past OR future) is dropped.
//! 2. **Replay dedupe** — a `nonce` already seen within the freshness
//!    window is dropped.
//! 3. **Honor throttle** — at most one honored request per
//!    (peer, kind) per [`RECOVERY_MIN_INTERVAL_SECS`], symmetric with
//!    the outbound emit throttle so a peer pair can't ping-pong.
//! 4. **Act-on-symptom** (SESSION_RESET) — DOWNGRADED to an
//!    observability signal, no longer a hard gate. A SESSION_RESET is
//!    only dispatched here after it has been `wire_v2`-decrypted (PQ-
//!    hybrid wrapped to our identity using the peer's identity secret),
//!    so a channel-poster who cannot produce a decryptable wire cannot
//!    forge one — that authentication, plus defenses 1–3, closes the
//!    spam surface for resets. Requiring an additional local failure
//!    deadlocked the common one-directional desync (the side that must
//!    reset has no symptom), so [`Self::had_recent_v4_failure`] is now
//!    logged (`corroborated`) for forensics but does not gate honoring.
//!    [`Self::note_v4_failure`] still records the symptom for that log.
//!    (SKDM_REQUEST is unaffected — it keeps its own handling.)

use std::collections::HashMap;

/// Max age (seconds, absolute) of a recovery request's `requested_at`
/// relative to local now before it is rejected as stale/clock-skewed.
pub const RECOVERY_FRESHNESS_SECS: i64 = 300;

/// Minimum spacing (seconds) between two emitted — or two honored —
/// recovery actions for the same (peer, kind). Bounds amplification.
///
/// Lowered from 120 → 30 so a desynced DM re-syncs promptly. The old
/// 120s window meant a session-reset whose delivery failed (e.g. the
/// keyserver inbox was down) couldn't be retried for two minutes,
/// which is what left a desync stuck. 30s is still ample anti-
/// amplification: the freshness window + replay-nonce dedupe +
/// inbound honor-throttle all still apply.
pub const RECOVERY_MIN_INTERVAL_SECS: i64 = 30;

/// How long a recorded v=4 decrypt failure counts as a live "symptom"
/// authorizing a SESSION_RESET to be honored (act-on-symptom gate).
pub const RECOVERY_FAILURE_SYMPTOM_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryKind {
    SkdmRequest,
    SessionReset,
}

/// In-memory only; `#[derive(Default)]` so `AppState`'s derive keeps
/// working without touching its constructor.
#[derive(Default)]
pub struct RecoveryGuard {
    /// (peer, kind) -> last time WE emitted this request outbound.
    last_emit: HashMap<(String, RecoveryKind), i64>,
    /// (peer, kind) -> last time we HONORED an inbound such request.
    last_honor: HashMap<(String, RecoveryKind), i64>,
    /// nonce -> time first seen (replay dedupe within freshness window).
    seen_nonces: HashMap<[u8; 16], i64>,
    /// peer -> time of most recent observed v=4 decrypt failure.
    v4_failures: HashMap<String, i64>,
}

impl RecoveryGuard {
    /// Outbound throttle: may we emit a `kind` request to `peer` now?
    /// Records the emit time when it returns `true`.
    pub fn should_emit(&mut self, peer: &str, kind: RecoveryKind, now: i64) -> bool {
        let key = (peer.to_string(), kind);
        if let Some(&t) = self.last_emit.get(&key) {
            if now.saturating_sub(t) < RECOVERY_MIN_INTERVAL_SECS {
                return false;
            }
        }
        self.last_emit.insert(key, now);
        true
    }

    /// Inbound gate (staleness + replay + honor-throttle). Does NOT
    /// cover act-on-symptom — the SESSION_RESET handler additionally
    /// calls [`Self::had_recent_v4_failure`]. Records the nonce + honor
    /// time when it returns `true`.
    pub fn accept_inbound(
        &mut self,
        peer: &str,
        kind: RecoveryKind,
        nonce: &[u8; 16],
        requested_at: i64,
        now: i64,
    ) -> bool {
        if (now - requested_at).abs() > RECOVERY_FRESHNESS_SECS {
            return false;
        }
        self.gc(now);
        if self.seen_nonces.contains_key(nonce) {
            return false;
        }
        let key = (peer.to_string(), kind);
        if let Some(&t) = self.last_honor.get(&key) {
            if now.saturating_sub(t) < RECOVERY_MIN_INTERVAL_SECS {
                return false;
            }
        }
        self.seen_nonces.insert(*nonce, now);
        self.last_honor.insert(key, now);
        true
    }

    /// Record an observed v=4 decrypt failure from `peer` (the
    /// act-on-symptom evidence a later SESSION_RESET needs).
    pub fn note_v4_failure(&mut self, peer: &str, now: i64) {
        self.v4_failures.insert(peer.to_string(), now);
    }

    /// True iff we logged a v=4 failure from `peer` recently enough to
    /// authorize honoring a SESSION_RESET from them.
    pub fn had_recent_v4_failure(&self, peer: &str, now: i64) -> bool {
        self.v4_failures
            .get(peer)
            .is_some_and(|&t| now.saturating_sub(t) <= RECOVERY_FAILURE_SYMPTOM_SECS)
    }

    fn gc(&mut self, now: i64) {
        self.seen_nonces
            .retain(|_, &mut t| now.saturating_sub(t) <= RECOVERY_FRESHNESS_SECS);
        self.v4_failures
            .retain(|_, &mut t| now.saturating_sub(t) <= RECOVERY_FAILURE_SYMPTOM_SECS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: &str = "1502770642930634812";

    #[test]
    fn emit_throttle_blocks_within_interval() {
        let mut g = RecoveryGuard::default();
        assert!(g.should_emit(P, RecoveryKind::SkdmRequest, 1000));
        assert!(!g.should_emit(P, RecoveryKind::SkdmRequest, 1000 + RECOVERY_MIN_INTERVAL_SECS - 1));
        assert!(g.should_emit(P, RecoveryKind::SkdmRequest, 1000 + RECOVERY_MIN_INTERVAL_SECS));
    }

    #[test]
    fn emit_throttle_is_per_kind_and_peer() {
        let mut g = RecoveryGuard::default();
        assert!(g.should_emit(P, RecoveryKind::SkdmRequest, 1000));
        // Different kind, same peer, same instant: independent budget.
        assert!(g.should_emit(P, RecoveryKind::SessionReset, 1000));
        // Different peer: independent budget.
        assert!(g.should_emit("other", RecoveryKind::SkdmRequest, 1000));
    }

    #[test]
    fn inbound_rejects_stale_and_future() {
        let mut g = RecoveryGuard::default();
        let n = [1u8; 16];
        assert!(!g.accept_inbound(P, RecoveryKind::SessionReset, &n, 0, RECOVERY_FRESHNESS_SECS + 1));
        let n2 = [2u8; 16];
        assert!(!g.accept_inbound(P, RecoveryKind::SessionReset, &n2, RECOVERY_FRESHNESS_SECS + 2, 1));
    }

    #[test]
    fn inbound_dedupes_replayed_nonce() {
        let mut g = RecoveryGuard::default();
        let n = [9u8; 16];
        assert!(g.accept_inbound(P, RecoveryKind::SkdmRequest, &n, 1000, 1000));
        // Same nonce again (even far enough that honor-throttle would
        // otherwise allow it) is rejected as a replay.
        assert!(!g.accept_inbound(P, RecoveryKind::SkdmRequest, &n, 1000, 1000 + RECOVERY_MIN_INTERVAL_SECS + 1));
    }

    #[test]
    fn inbound_honor_throttle_per_peer_kind() {
        let mut g = RecoveryGuard::default();
        assert!(g.accept_inbound(P, RecoveryKind::SkdmRequest, &[1u8; 16], 1000, 1000));
        // Fresh nonce, within honor interval -> still throttled.
        let within = 1000 + RECOVERY_MIN_INTERVAL_SECS - 1;
        assert!(!g.accept_inbound(P, RecoveryKind::SkdmRequest, &[2u8; 16], within, within));
        // Past the interval, fresh nonce -> allowed.
        let t = 1000 + RECOVERY_MIN_INTERVAL_SECS + 1;
        assert!(g.accept_inbound(P, RecoveryKind::SkdmRequest, &[3u8; 16], t, t));
    }

    #[test]
    fn act_on_symptom_window() {
        let mut g = RecoveryGuard::default();
        assert!(!g.had_recent_v4_failure(P, 1000));
        g.note_v4_failure(P, 1000);
        assert!(g.had_recent_v4_failure(P, 1000 + RECOVERY_FAILURE_SYMPTOM_SECS));
        assert!(!g.had_recent_v4_failure(P, 1000 + RECOVERY_FAILURE_SYMPTOM_SECS + 1));
    }
}
