//! Pure state machines for atomic logical-message publication and truthful receipts.
//!
//! This crate performs no I/O and grants no authority to send, fetch, decrypt, or
//! delete anything. Callers must durably store each returned snapshot before acting
//! on the next mutation. No API accepts plaintext or provider credentials.
//! Structural snapshot validation is not authentication: the caller must store
//! serialized snapshots inside an integrity-protected, account-bound envelope.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MANIFEST_DOMAIN: &[u8] = b"osl-logical-message-manifest-v1";
pub const HARD_MAX_PARTS: u16 = 4_096;
pub const HARD_MAX_PART_BYTES: u32 = 64 * 1024;
pub const HARD_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LifecycleLimits {
    pub max_parts: u16,
    pub max_part_bytes: u32,
    pub max_total_bytes: u64,
}

impl LifecycleLimits {
    pub fn validate(self) -> Result<Self, LifecycleError> {
        if self.max_parts == 0
            || self.max_parts > HARD_MAX_PARTS
            || self.max_part_bytes == 0
            || self.max_part_bytes > HARD_MAX_PART_BYTES
            || self.max_total_bytes == 0
            || self.max_total_bytes > HARD_MAX_TOTAL_BYTES
        {
            return Err(LifecycleError::InvalidLimits);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReceiptStatus {
    Prepared,
    RelayAccepted,
    Received,
    Opened,
    Expired,
    Failed,
    Burned,
}

impl ReceiptStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Expired | Self::Failed | Self::Burned)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum FailureCode {
    RelayRejected,
    PartConflict,
    BoundsExceeded,
    ManifestRejected,
    LocalPersistenceFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcceptedPart {
    pub index: u16,
    pub sealed_bytes: u32,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransitionEvidence {
    /// A content-free digest of the durable external receipt.
    pub digest: [u8; 32],
    pub observed_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogicalMessageLifecycle {
    message_id: [u8; 32],
    scope_digest: [u8; 32],
    expected_parts: u16,
    created_at: u64,
    expires_at: u64,
    limits: LifecycleLimits,
    parts: Vec<AcceptedPart>,
    status: ReceiptStatus,
    manifest_digest: Option<[u8; 32]>,
    relay_evidence: Option<TransitionEvidence>,
    received_evidence: Option<TransitionEvidence>,
    opened_evidence: Option<TransitionEvidence>,
    failure: Option<FailureCode>,
}

impl LogicalMessageLifecycle {
    pub fn prepare(
        message_id: [u8; 32],
        scope_digest: [u8; 32],
        expected_parts: u16,
        created_at: u64,
        expires_at: u64,
        limits: LifecycleLimits,
    ) -> Result<Self, LifecycleError> {
        let limits = limits.validate()?;
        if message_id == [0; 32]
            || scope_digest == [0; 32]
            || expected_parts == 0
            || expected_parts > limits.max_parts
            || expires_at <= created_at
        {
            return Err(LifecycleError::InvalidPreparation);
        }
        Ok(Self {
            message_id,
            scope_digest,
            expected_parts,
            created_at,
            expires_at,
            limits,
            parts: Vec::with_capacity(usize::from(expected_parts.min(64))),
            status: ReceiptStatus::Prepared,
            manifest_digest: None,
            relay_evidence: None,
            received_evidence: None,
            opened_evidence: None,
            failure: None,
        })
    }

    pub fn status(&self) -> ReceiptStatus {
        self.status
    }

    pub fn accepted_parts(&self) -> &[AcceptedPart] {
        &self.parts
    }

    pub fn manifest_digest(&self) -> Option<[u8; 32]> {
        self.manifest_digest
    }

    pub fn failure(&self) -> Option<FailureCode> {
        self.failure
    }

    /// Records a relay-accepted sealed part. Identical retries are idempotent.
    /// A conflicting retry fails the whole logical message closed.
    pub fn accept_part(
        &mut self,
        part: AcceptedPart,
        now: u64,
    ) -> Result<Mutation, LifecycleError> {
        self.require_prepared(now)?;
        if self.manifest_digest.is_some() {
            return Err(LifecycleError::ManifestAlreadyPrepared);
        }
        if part.index >= self.expected_parts || part.sealed_bytes == 0 {
            return self.fail_with(FailureCode::BoundsExceeded, LifecycleError::PartOutOfBounds);
        }
        if let Some(existing) = self.parts.iter().find(|value| value.index == part.index) {
            if existing == &part {
                return Ok(Mutation::Unchanged);
            }
            return self.fail_with(FailureCode::PartConflict, LifecycleError::PartConflict);
        }
        if part.sealed_bytes > self.limits.max_part_bytes
            || self
                .parts
                .iter()
                .map(|value| u64::from(value.sealed_bytes))
                .sum::<u64>()
                .saturating_add(u64::from(part.sealed_bytes))
                > self.limits.max_total_bytes
        {
            return self.fail_with(FailureCode::BoundsExceeded, LifecycleError::BoundsExceeded);
        }
        self.parts.push(part);
        self.parts.sort_unstable_by_key(|value| value.index);
        Ok(Mutation::Changed)
    }

    /// Freezes a content-bound manifest after every sealed part is staged.
    /// This does not claim relay acceptance and grants no publication authority.
    pub fn prepare_manifest(&mut self, now: u64) -> Result<[u8; 32], LifecycleError> {
        if self.status == ReceiptStatus::Prepared && self.manifest_digest.is_some() {
            if now >= self.expires_at {
                let _ = self.expire(now);
                return Err(LifecycleError::Expired);
            }
            return self.manifest_digest.ok_or(LifecycleError::CorruptSnapshot);
        }
        self.require_prepared(now)?;
        if self.parts.len() != usize::from(self.expected_parts)
            || self
                .parts
                .iter()
                .enumerate()
                .any(|(index, part)| usize::from(part.index) != index)
        {
            return Err(LifecycleError::IncompleteParts);
        }
        let digest = compute_manifest_digest(
            &self.message_id,
            &self.scope_digest,
            self.created_at,
            self.expires_at,
            &self.parts,
        );
        self.manifest_digest = Some(digest);
        Ok(digest)
    }

    /// Records the relay's durable acceptance only after the frozen manifest
    /// has actually been acknowledged. Identical retries are idempotent.
    pub fn record_relay_accepted(
        &mut self,
        evidence: TransitionEvidence,
        now: u64,
    ) -> Result<Mutation, LifecycleError> {
        if self.manifest_digest.is_none() {
            return Err(LifecycleError::ManifestNotPrepared);
        }
        self.transition(
            ReceiptStatus::Prepared,
            ReceiptStatus::RelayAccepted,
            EvidenceKind::Relay,
            evidence,
            now,
        )
    }

    pub fn record_received(
        &mut self,
        evidence: TransitionEvidence,
        now: u64,
    ) -> Result<Mutation, LifecycleError> {
        self.transition(
            ReceiptStatus::RelayAccepted,
            ReceiptStatus::Received,
            EvidenceKind::Received,
            evidence,
            now,
        )
    }

    pub fn record_opened(
        &mut self,
        evidence: TransitionEvidence,
        now: u64,
    ) -> Result<Mutation, LifecycleError> {
        self.transition(
            ReceiptStatus::Received,
            ReceiptStatus::Opened,
            EvidenceKind::Opened,
            evidence,
            now,
        )
    }

    pub fn expire(&mut self, now: u64) -> Result<Mutation, LifecycleError> {
        if self.status == ReceiptStatus::Opened || self.status.is_terminal() {
            return if self.status == ReceiptStatus::Expired {
                Ok(Mutation::Unchanged)
            } else {
                Err(LifecycleError::TerminalState(self.status))
            };
        }
        if now < self.expires_at {
            return Err(LifecycleError::NotExpired);
        }
        self.status = ReceiptStatus::Expired;
        Ok(Mutation::Changed)
    }

    pub fn fail(&mut self, code: FailureCode) -> Result<Mutation, LifecycleError> {
        if self.status == ReceiptStatus::Failed && self.failure == Some(code) {
            return Ok(Mutation::Unchanged);
        }
        if self.status.is_terminal() || self.status == ReceiptStatus::Opened {
            return Err(LifecycleError::TerminalState(self.status));
        }
        self.status = ReceiptStatus::Failed;
        self.failure = Some(code);
        Ok(Mutation::Changed)
    }

    pub fn burn(&mut self) -> Result<Mutation, LifecycleError> {
        if self.status == ReceiptStatus::Burned {
            return Ok(Mutation::Unchanged);
        }
        self.status = ReceiptStatus::Burned;
        self.parts.clear();
        self.relay_evidence = None;
        self.received_evidence = None;
        self.opened_evidence = None;
        Ok(Mutation::Changed)
    }

    /// Validates a deserialized crash-recovery snapshot before it is trusted.
    pub fn validate_snapshot(&self) -> Result<(), LifecycleError> {
        self.limits.validate()?;
        if self.message_id == [0; 32]
            || self.scope_digest == [0; 32]
            || self.expected_parts == 0
            || self.expected_parts > self.limits.max_parts
            || self.expires_at <= self.created_at
            || self.parts.len() > usize::from(self.expected_parts)
        {
            return Err(LifecycleError::CorruptSnapshot);
        }
        let mut total = 0u64;
        for (expected_index, part) in self.parts.iter().enumerate() {
            if usize::from(part.index) != expected_index
                || part.sealed_bytes == 0
                || part.sealed_bytes > self.limits.max_part_bytes
            {
                return Err(LifecycleError::CorruptSnapshot);
            }
            total = total.saturating_add(u64::from(part.sealed_bytes));
        }
        if total > self.limits.max_total_bytes {
            return Err(LifecycleError::CorruptSnapshot);
        }
        if self.status == ReceiptStatus::Failed && self.failure.is_none() {
            return Err(LifecycleError::CorruptSnapshot);
        }
        if !matches!(self.status, ReceiptStatus::Failed | ReceiptStatus::Burned)
            && self.failure.is_some()
        {
            return Err(LifecycleError::CorruptSnapshot);
        }
        match self.status {
            ReceiptStatus::Prepared => {
                if self.relay_evidence.is_some()
                    || self.received_evidence.is_some()
                    || self.opened_evidence.is_some()
                {
                    return Err(LifecycleError::CorruptSnapshot);
                }
            }
            ReceiptStatus::RelayAccepted => {
                if self.relay_evidence.is_none()
                    || self.received_evidence.is_some()
                    || self.opened_evidence.is_some()
                {
                    return Err(LifecycleError::CorruptSnapshot);
                }
            }
            ReceiptStatus::Received => {
                if self.relay_evidence.is_none()
                    || self.received_evidence.is_none()
                    || self.opened_evidence.is_some()
                {
                    return Err(LifecycleError::CorruptSnapshot);
                }
            }
            ReceiptStatus::Opened => {
                if self.relay_evidence.is_none()
                    || self.received_evidence.is_none()
                    || self.opened_evidence.is_none()
                {
                    return Err(LifecycleError::CorruptSnapshot);
                }
            }
            ReceiptStatus::Expired | ReceiptStatus::Failed | ReceiptStatus::Burned => {}
        }
        if let Some(manifest) = self.manifest_digest {
            if self.status == ReceiptStatus::Burned && self.parts.is_empty() {
                return Ok(());
            }
            if self.parts.len() != usize::from(self.expected_parts)
                || manifest
                    != compute_manifest_digest(
                        &self.message_id,
                        &self.scope_digest,
                        self.created_at,
                        self.expires_at,
                        &self.parts,
                    )
            {
                return Err(LifecycleError::CorruptSnapshot);
            }
        } else if matches!(
            self.status,
            ReceiptStatus::RelayAccepted | ReceiptStatus::Received | ReceiptStatus::Opened
        ) {
            return Err(LifecycleError::CorruptSnapshot);
        }
        Ok(())
    }

    fn require_prepared(&mut self, now: u64) -> Result<(), LifecycleError> {
        if now >= self.expires_at {
            let _ = self.expire(now);
            return Err(LifecycleError::Expired);
        }
        if self.status != ReceiptStatus::Prepared {
            return Err(LifecycleError::InvalidTransition {
                from: self.status,
                to: ReceiptStatus::Prepared,
            });
        }
        Ok(())
    }

    fn transition(
        &mut self,
        from: ReceiptStatus,
        to: ReceiptStatus,
        evidence_kind: EvidenceKind,
        evidence: TransitionEvidence,
        now: u64,
    ) -> Result<Mutation, LifecycleError> {
        if evidence.digest == [0; 32] || evidence.observed_at > now {
            return Err(LifecycleError::InvalidEvidence);
        }
        if now >= self.expires_at {
            let _ = self.expire(now);
            return Err(LifecycleError::Expired);
        }
        if self.status.is_terminal() {
            return Err(LifecycleError::TerminalState(self.status));
        }
        let prior_observed_at = match evidence_kind {
            EvidenceKind::Relay => None,
            EvidenceKind::Received => self.relay_evidence.as_ref().map(|value| value.observed_at),
            EvidenceKind::Opened => self
                .received_evidence
                .as_ref()
                .map(|value| value.observed_at),
        };
        let stored_evidence = match evidence_kind {
            EvidenceKind::Relay => &mut self.relay_evidence,
            EvidenceKind::Received => &mut self.received_evidence,
            EvidenceKind::Opened => &mut self.opened_evidence,
        };
        if self.status == to || receipt_rank(self.status) > receipt_rank(to) {
            return if stored_evidence.as_ref() == Some(&evidence) {
                Ok(Mutation::Unchanged)
            } else {
                Err(LifecycleError::EvidenceConflict)
            };
        }
        if self.status != from {
            return Err(LifecycleError::InvalidTransition {
                from: self.status,
                to,
            });
        }
        if prior_observed_at.is_some_and(|prior| evidence.observed_at < prior) {
            return Err(LifecycleError::StaleEvidence);
        }
        self.status = to;
        *stored_evidence = Some(evidence);
        Ok(Mutation::Changed)
    }

    fn fail_with<T>(
        &mut self,
        code: FailureCode,
        error: LifecycleError,
    ) -> Result<T, LifecycleError> {
        self.status = ReceiptStatus::Failed;
        self.failure = Some(code);
        Err(error)
    }
}

#[derive(Clone, Copy)]
enum EvidenceKind {
    Relay,
    Received,
    Opened,
}

fn receipt_rank(status: ReceiptStatus) -> u8 {
    match status {
        ReceiptStatus::Prepared => 0,
        ReceiptStatus::RelayAccepted => 1,
        ReceiptStatus::Received => 2,
        ReceiptStatus::Opened => 3,
        ReceiptStatus::Expired | ReceiptStatus::Failed | ReceiptStatus::Burned => 4,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mutation {
    Changed,
    Unchanged,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LifecycleError {
    #[error("lifecycle limits are invalid")]
    InvalidLimits,
    #[error("logical message preparation is invalid")]
    InvalidPreparation,
    #[error("part is outside the declared message")]
    PartOutOfBounds,
    #[error("part retry conflicts with the accepted receipt")]
    PartConflict,
    #[error("logical message bounds were exceeded")]
    BoundsExceeded,
    #[error("not every declared part has been accepted")]
    IncompleteParts,
    #[error("the logical-message manifest is already frozen")]
    ManifestAlreadyPrepared,
    #[error("the logical-message manifest has not been frozen")]
    ManifestNotPrepared,
    #[error("logical message has expired")]
    Expired,
    #[error("logical message has not expired")]
    NotExpired,
    #[error("transition evidence is invalid")]
    InvalidEvidence,
    #[error("transition retry conflicts with existing evidence")]
    EvidenceConflict,
    #[error("transition evidence predates the preceding receipt")]
    StaleEvidence,
    #[error("invalid receipt transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: ReceiptStatus,
        to: ReceiptStatus,
    },
    #[error("receipt is terminal in state {0:?}")]
    TerminalState(ReceiptStatus),
    #[error("serialized lifecycle snapshot is invalid")]
    CorruptSnapshot,
}

fn compute_manifest_digest(
    message_id: &[u8; 32],
    scope_digest: &[u8; 32],
    created_at: u64,
    expires_at: u64,
    parts: &[AcceptedPart],
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(MANIFEST_DOMAIN);
    hash.update(message_id);
    hash.update(scope_digest);
    hash.update(created_at.to_be_bytes());
    hash.update(expires_at.to_be_bytes());
    hash.update((parts.len() as u64).to_be_bytes());
    for part in parts {
        hash.update(part.index.to_be_bytes());
        hash.update(part.sealed_bytes.to_be_bytes());
        hash.update(part.digest);
    }
    hash.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> LifecycleLimits {
        LifecycleLimits {
            max_parts: 4,
            max_part_bytes: 1_024,
            max_total_bytes: 4_096,
        }
    }

    fn prepared(expected_parts: u16) -> LogicalMessageLifecycle {
        LogicalMessageLifecycle::prepare([1; 32], [2; 32], expected_parts, 10, 100, limits())
            .unwrap()
    }

    fn part(index: u16, byte: u8) -> AcceptedPart {
        AcceptedPart {
            index,
            sealed_bytes: 100,
            digest: [byte; 32],
        }
    }

    fn evidence(byte: u8, observed_at: u64) -> TransitionEvidence {
        TransitionEvidence {
            digest: [byte; 32],
            observed_at,
        }
    }

    #[test]
    fn commit_requires_all_parts_and_is_retry_safe() {
        let mut state = prepared(2);
        state.accept_part(part(1, 4), 20).unwrap();
        assert_eq!(
            state.prepare_manifest(21),
            Err(LifecycleError::IncompleteParts)
        );
        state.accept_part(part(0, 3), 22).unwrap();
        let digest = state.prepare_manifest(23).unwrap();
        assert_eq!(state.status(), ReceiptStatus::Prepared);
        assert_eq!(state.prepare_manifest(24).unwrap(), digest);
        state.record_relay_accepted(evidence(6, 24), 24).unwrap();
        assert_eq!(state.status(), ReceiptStatus::RelayAccepted);
        state.validate_snapshot().unwrap();
    }

    #[test]
    fn identical_part_retry_is_idempotent_but_conflict_fails_closed() {
        let mut state = prepared(1);
        assert_eq!(state.accept_part(part(0, 3), 20), Ok(Mutation::Changed));
        assert_eq!(state.accept_part(part(0, 3), 21), Ok(Mutation::Unchanged));
        assert_eq!(
            state.accept_part(part(0, 4), 22),
            Err(LifecycleError::PartConflict)
        );
        assert_eq!(state.status(), ReceiptStatus::Failed);
        assert_eq!(state.failure(), Some(FailureCode::PartConflict));
    }

    #[test]
    fn receipt_order_and_evidence_are_strict() {
        let mut state = prepared(1);
        state.accept_part(part(0, 3), 20).unwrap();
        state.prepare_manifest(21).unwrap();
        state.record_relay_accepted(evidence(6, 21), 21).unwrap();
        assert!(matches!(
            state.record_opened(evidence(8, 22), 22),
            Err(LifecycleError::InvalidTransition { .. })
        ));
        let received = evidence(7, 22);
        assert_eq!(
            state.record_received(received.clone(), 22),
            Ok(Mutation::Changed)
        );
        assert_eq!(state.record_received(received, 23), Ok(Mutation::Unchanged));
        assert_eq!(
            state.record_received(evidence(9, 22), 23),
            Err(LifecycleError::EvidenceConflict)
        );
        state.record_opened(evidence(8, 24), 24).unwrap();
        assert_eq!(state.status(), ReceiptStatus::Opened);
    }

    #[test]
    fn expiry_and_burn_are_terminal_and_idempotent() {
        let mut expired = prepared(1);
        assert_eq!(expired.expire(99), Err(LifecycleError::NotExpired));
        assert_eq!(expired.expire(100), Ok(Mutation::Changed));
        assert_eq!(expired.expire(101), Ok(Mutation::Unchanged));
        assert_eq!(expired.burn(), Ok(Mutation::Changed));
        assert_eq!(expired.status(), ReceiptStatus::Burned);

        let mut burned = prepared(1);
        burned.accept_part(part(0, 3), 20).unwrap();
        burned.prepare_manifest(21).unwrap();
        burned.record_relay_accepted(evidence(6, 21), 21).unwrap();
        assert_eq!(burned.burn(), Ok(Mutation::Changed));
        assert_eq!(burned.burn(), Ok(Mutation::Unchanged));
        assert!(burned.accepted_parts().is_empty());
    }

    #[test]
    fn bounds_fail_closed_without_large_allocation() {
        let mut state = prepared(1);
        let mut oversized = part(0, 3);
        oversized.sealed_bytes = 1_025;
        assert_eq!(
            state.accept_part(oversized, 20),
            Err(LifecycleError::BoundsExceeded)
        );
        assert_eq!(state.status(), ReceiptStatus::Failed);
    }

    #[test]
    fn configurable_limits_cannot_exceed_immutable_memory_bounds() {
        assert_eq!(
            LifecycleLimits {
                max_parts: HARD_MAX_PARTS + 1,
                max_part_bytes: 1,
                max_total_bytes: 1,
            }
            .validate(),
            Err(LifecycleError::InvalidLimits)
        );
        assert_eq!(
            LifecycleLimits {
                max_parts: 1,
                max_part_bytes: HARD_MAX_PART_BYTES,
                max_total_bytes: HARD_MAX_TOTAL_BYTES + 1,
            }
            .validate(),
            Err(LifecycleError::InvalidLimits)
        );
    }

    #[test]
    fn manifest_binds_scope_order_size_and_digest() {
        let mut first = prepared(2);
        first.accept_part(part(0, 3), 20).unwrap();
        first.accept_part(part(1, 4), 20).unwrap();
        let first_digest = first.prepare_manifest(21).unwrap();

        let mut changed = prepared(2);
        changed.accept_part(part(0, 3), 20).unwrap();
        changed.accept_part(part(1, 5), 20).unwrap();
        assert_ne!(first_digest, changed.prepare_manifest(21).unwrap());
    }

    #[test]
    fn preparing_manifest_never_claims_relay_acceptance() {
        let mut state = prepared(1);
        state.accept_part(part(0, 3), 20).unwrap();
        state.prepare_manifest(21).unwrap();
        assert_eq!(state.status(), ReceiptStatus::Prepared);
        state.record_relay_accepted(evidence(6, 22), 22).unwrap();
        assert_eq!(state.status(), ReceiptStatus::RelayAccepted);
    }

    #[test]
    fn precommit_terminal_snapshots_survive_structural_validation() {
        let mut failed = prepared(2);
        failed.fail(FailureCode::RelayRejected).unwrap();
        failed.validate_snapshot().unwrap();

        let mut expired = prepared(2);
        expired.expire(100).unwrap();
        expired.validate_snapshot().unwrap();

        let mut burned = prepared(2);
        burned.burn().unwrap();
        burned.validate_snapshot().unwrap();
    }

    #[test]
    fn receipt_evidence_must_be_chronological() {
        let mut state = prepared(1);
        state.accept_part(part(0, 3), 20).unwrap();
        state.prepare_manifest(21).unwrap();
        state.record_relay_accepted(evidence(6, 30), 30).unwrap();
        assert_eq!(
            state.record_received(evidence(7, 29), 31),
            Err(LifecycleError::StaleEvidence)
        );
    }

    #[test]
    fn late_receipt_retry_is_idempotent_after_open() {
        let mut state = prepared(1);
        state.accept_part(part(0, 3), 20).unwrap();
        state.prepare_manifest(21).unwrap();
        let relay = evidence(6, 21);
        let received = evidence(7, 22);
        state.record_relay_accepted(relay.clone(), 21).unwrap();
        state.record_received(received.clone(), 22).unwrap();
        state.record_opened(evidence(8, 23), 23).unwrap();
        assert_eq!(
            state.record_relay_accepted(relay, 24),
            Ok(Mutation::Unchanged)
        );
        assert_eq!(state.record_received(received, 24), Ok(Mutation::Unchanged));
    }
}
