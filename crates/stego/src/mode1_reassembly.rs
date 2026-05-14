//! Phase 9-B1 Task 4: Mode 1 receive-side reassembly buffer.
//!
//! Sessions are keyed by `session_id` (the random `u32` the sender
//! stamped on every chunk via [`crate::chunk_payload`]). A session
//! collects chunks until `total_chunks` distinct indices have
//! arrived, at which point reassembly emits the concatenated wire
//! payload back to the caller. Sessions older than
//! [`SESSION_TIMEOUT_SECS`] are evicted; a fixed cap of
//! [`MAX_CONCURRENT_SESSIONS`] is enforced via FIFO eviction of the
//! oldest session.
//!
//! The buffer never persists across restarts — Discord's eventual
//! re-delivery semantics mean partial sessions across crashes would
//! re-fire from the start anyway, and on-disk persistence would
//! recreate the reorder hazards we lock down at the chunk-HMAC layer.

use std::collections::{BTreeMap, HashMap, VecDeque};

/// Max age of an in-flight session before it gets evicted. Five
/// minutes follows the locked policy in the B1 spec.
pub const SESSION_TIMEOUT_SECS: u64 = 5 * 60;

/// Concurrent sessions ceiling per buffer instance. FIFO eviction
/// when exceeded.
pub const MAX_CONCURRENT_SESSIONS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReassemblyComplete {
    pub session_id: u32,
    pub wire_bytes: Vec<u8>,
    pub total_chunks: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    /// Session is still incomplete; carries the count of chunks
    /// received so far and the declared total.
    Incomplete { received: u8, total: u8 },
    /// All chunks for this session have arrived. The session is
    /// removed from the buffer.
    Complete(ReassemblyComplete),
    /// The caller's chunk conflicts with a prior chunk in this
    /// session (e.g. announces a different `total`, or duplicates an
    /// index with different payload bytes). The session is dropped.
    Conflict,
}

#[derive(Debug, Default)]
pub struct ReassemblyBuffer {
    sessions: HashMap<u32, Session>,
    /// FIFO order for eviction: session_id push order. We rebuild
    /// this on completion / conflict to keep things simple.
    fifo: VecDeque<u32>,
}

#[derive(Debug, Clone)]
struct Session {
    total: u8,
    /// indexed by chunk_index; absent = not yet received.
    chunks: BTreeMap<u8, Vec<u8>>,
    /// monotonic clock-second of last chunk push for this session.
    last_push_at: u64,
}

impl ReassemblyBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Drop every session whose `last_push_at` is older than
    /// `now - SESSION_TIMEOUT_SECS`. Callers should invoke this
    /// before each push to keep the buffer bounded against slow
    /// senders.
    pub fn evict_expired(&mut self, now: u64) {
        let cutoff = now.saturating_sub(SESSION_TIMEOUT_SECS);
        let expired: Vec<u32> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.last_push_at < cutoff)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            self.sessions.remove(&id);
        }
        // Rebuild fifo to keep it tight.
        self.fifo.retain(|id| self.sessions.contains_key(id));
    }

    /// Forget every chunk belonging to `session_id`. Used by burn
    /// integration to flush in-flight cover for a burned scope.
    pub fn drop_session(&mut self, session_id: u32) {
        self.sessions.remove(&session_id);
        self.fifo.retain(|id| *id != session_id);
    }

    /// Clear every session. Used by global cleanup paths (full burn,
    /// stealth re-engage).
    pub fn drop_all(&mut self) {
        self.sessions.clear();
        self.fifo.clear();
    }

    /// Accept one chunk into the buffer. Returns the disposition.
    pub fn push(
        &mut self,
        session_id: u32,
        chunk_index: u8,
        total_chunks: u8,
        payload: Vec<u8>,
        now: u64,
    ) -> PushOutcome {
        self.evict_expired(now);

        // Validate basic header invariants. parse_chunk already
        // catches these but we re-check on the receive side as
        // defense-in-depth: caller may be feeding us bytes from
        // another path.
        if total_chunks == 0 || chunk_index >= total_chunks {
            return PushOutcome::Conflict;
        }

        // Capacity enforcement (only when admitting a *new* session,
        // since updates to an existing one don't grow `sessions`).
        if !self.sessions.contains_key(&session_id)
            && self.sessions.len() >= MAX_CONCURRENT_SESSIONS
        {
            if let Some(victim) = self.fifo.pop_front() {
                self.sessions.remove(&victim);
            }
        }

        let session = self.sessions.entry(session_id).or_insert_with(|| {
            self.fifo.push_back(session_id);
            Session {
                total: total_chunks,
                chunks: BTreeMap::new(),
                last_push_at: now,
            }
        });

        if session.total != total_chunks {
            // sender flipped total mid-session — drop.
            self.sessions.remove(&session_id);
            self.fifo.retain(|id| *id != session_id);
            return PushOutcome::Conflict;
        }
        if let Some(prior) = session.chunks.get(&chunk_index) {
            if prior != &payload {
                self.sessions.remove(&session_id);
                self.fifo.retain(|id| *id != session_id);
                return PushOutcome::Conflict;
            }
            // Duplicate of identical payload — idempotent.
        } else {
            session.chunks.insert(chunk_index, payload);
        }
        session.last_push_at = now;

        if session.chunks.len() == session.total as usize {
            let mut bytes = Vec::new();
            for i in 0..session.total {
                if let Some(p) = session.chunks.get(&i) {
                    bytes.extend_from_slice(p);
                } else {
                    // Should not happen — len == total but a slot is
                    // empty: drop and report incomplete.
                    return PushOutcome::Incomplete {
                        received: session.chunks.len() as u8,
                        total: session.total,
                    };
                }
            }
            let total_chunks_out = session.total;
            self.sessions.remove(&session_id);
            self.fifo.retain(|id| *id != session_id);
            return PushOutcome::Complete(ReassemblyComplete {
                session_id,
                wire_bytes: bytes,
                total_chunks: total_chunks_out,
            });
        }

        PushOutcome::Incomplete {
            received: session.chunks.len() as u8,
            total: session.total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_chunk_session_completes_immediately() {
        let mut buf = ReassemblyBuffer::new();
        let out = buf.push(1, 0, 1, b"hello".to_vec(), 100);
        match out {
            PushOutcome::Complete(c) => {
                assert_eq!(c.session_id, 1);
                assert_eq!(c.wire_bytes, b"hello");
                assert_eq!(c.total_chunks, 1);
            }
            other => panic!("expected Complete, got {other:?}"),
        }
        assert!(buf.is_empty());
    }

    #[test]
    fn multi_chunk_session_completes_after_all_chunks() {
        let mut buf = ReassemblyBuffer::new();
        // Total = 3. Push out of order.
        assert!(matches!(
            buf.push(7, 2, 3, b"third".to_vec(), 100),
            PushOutcome::Incomplete {
                received: 1,
                total: 3
            }
        ));
        assert!(matches!(
            buf.push(7, 0, 3, b"first".to_vec(), 101),
            PushOutcome::Incomplete {
                received: 2,
                total: 3
            }
        ));
        match buf.push(7, 1, 3, b"second".to_vec(), 102) {
            PushOutcome::Complete(c) => {
                assert_eq!(c.wire_bytes, b"firstsecondthird");
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    #[test]
    fn session_timeout_evicts_old_session() {
        let mut buf = ReassemblyBuffer::new();
        buf.push(9, 0, 2, b"part0".to_vec(), 0);
        assert_eq!(buf.len(), 1);
        // Advance past timeout, push something else to trigger evict.
        buf.push(10, 0, 1, b"unrelated".to_vec(), SESSION_TIMEOUT_SECS + 1);
        assert!(
            !buf.sessions.contains_key(&9),
            "session 9 should be evicted after timeout"
        );
    }

    #[test]
    fn concurrent_session_cap_fifo_evicts_oldest() {
        let mut buf = ReassemblyBuffer::new();
        // Fill to cap with 2-chunk sessions inside the session
        // timeout window so timeout eviction doesn't fire — the cap
        // path is what we're exercising.
        let base = 100u64;
        for sid in 0u32..MAX_CONCURRENT_SESSIONS as u32 {
            buf.push(sid, 0, 2, vec![sid as u8], base + sid as u64);
        }
        assert_eq!(buf.len(), MAX_CONCURRENT_SESSIONS);
        // Admit a new session — should evict session 0 (oldest by
        // FIFO insertion order).
        buf.push(999, 0, 2, b"x".to_vec(), base + 50);
        assert_eq!(buf.len(), MAX_CONCURRENT_SESSIONS);
        assert!(
            !buf.sessions.contains_key(&0),
            "session 0 should be evicted"
        );
        assert!(buf.sessions.contains_key(&999));
    }

    #[test]
    fn conflicting_total_drops_session() {
        let mut buf = ReassemblyBuffer::new();
        buf.push(5, 0, 3, b"a".to_vec(), 0);
        match buf.push(5, 1, 4, b"b".to_vec(), 1) {
            PushOutcome::Conflict => {}
            other => panic!("expected Conflict, got {other:?}"),
        }
        assert!(!buf.sessions.contains_key(&5));
    }

    #[test]
    fn duplicate_chunk_same_payload_is_idempotent() {
        let mut buf = ReassemblyBuffer::new();
        buf.push(11, 0, 2, b"hi".to_vec(), 0);
        match buf.push(11, 0, 2, b"hi".to_vec(), 1) {
            PushOutcome::Incomplete { received, total } => {
                assert_eq!(received, 1);
                assert_eq!(total, 2);
            }
            other => panic!("expected Incomplete, got {other:?}"),
        }
        // Different payload at same index → conflict.
        match buf.push(11, 0, 2, b"XX".to_vec(), 2) {
            PushOutcome::Conflict => {}
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn drop_session_evicts_only_that_session() {
        let mut buf = ReassemblyBuffer::new();
        buf.push(1, 0, 2, b"a".to_vec(), 0);
        buf.push(2, 0, 2, b"b".to_vec(), 0);
        buf.drop_session(1);
        assert!(!buf.sessions.contains_key(&1));
        assert!(buf.sessions.contains_key(&2));
    }
}
