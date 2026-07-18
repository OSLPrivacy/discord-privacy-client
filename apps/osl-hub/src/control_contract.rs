//! Offline-safe opaque control and OSL-only opened-receipt contracts.
//!
//! Controls contain no message text, service id, account handle, or chat title.
//! The transport may retry them after reconnect; receiver replay state makes
//! identical delivery idempotent. This module never represents a provider's
//! native "seen" state.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use crate::burn_contract::{
    BurnContractError, BurnScopeCommitment, BurnSignatureVerifier, ExternalCopiesEffect,
    NativeCarrierHistoryEffect, ProductTier,
};

const CONTROL_DOMAIN: &[u8] = b"OSL/opaque-control/v1";
const RECEIPT_CONSENT_DOMAIN: &[u8] = b"OSL/opened-receipt-consent/v1";
const OPEN_DOMAIN: &[u8] = b"OSL/authenticated-local-open/v1";
const MAX_QUEUED_CONTROLS: usize = 4_096;
const MAX_PROCESSED_CONTROLS: usize = 8_192;
const MAX_BATCH: usize = 128;
const MAX_CONTROL_LIFETIME_MS: u64 = 90 * 24 * 60 * 60 * 1_000;
const MAX_TIMER_MS: u64 = 90 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OpaqueControlKind {
    Burn,
    Expire,
    OpenedReceipt,
}

impl OpaqueControlKind {
    fn tag(self) -> u8 {
        match self {
            Self::Burn => 1,
            Self::Expire => 2,
            Self::OpenedReceipt => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OpaqueSignedControl {
    pub version: u8,
    pub kind: OpaqueControlKind,
    pub control_id: [u8; 32],
    pub scope: BurnScopeCommitment,
    pub issuer_identity_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
    pub message_commitment: [u8; 32],
    pub action_commitment: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub sequence: u64,
    pub nonce: [u8; 24],
    pub signature: [u8; 64],
}

impl OpaqueSignedControl {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(290);
        bytes.extend_from_slice(CONTROL_DOMAIN);
        bytes.push(self.version);
        bytes.push(self.kind.tag());
        bytes.extend_from_slice(&self.control_id);
        bytes.push(self.scope.level.tag());
        bytes.extend_from_slice(&self.scope.digest);
        bytes.extend_from_slice(&self.issuer_identity_commitment);
        bytes.extend_from_slice(&self.recipient_identity_commitment);
        bytes.extend_from_slice(&self.message_commitment);
        bytes.extend_from_slice(&self.action_commitment);
        bytes.extend_from_slice(&self.issued_at_ms.to_be_bytes());
        bytes.extend_from_slice(&self.expires_at_ms.to_be_bytes());
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }

    pub fn validate(
        &self,
        now_ms: u64,
        verifier: &impl BurnSignatureVerifier,
    ) -> Result<(), ControlContractError> {
        let expected_id: [u8; 32] = Sha256::digest(self.canonical_without_id()).into();
        if self.version != 1
            || self.control_id != expected_id
            || all_zero(&self.issuer_identity_commitment)
            || all_zero(&self.recipient_identity_commitment)
            || all_zero(&self.message_commitment)
            || all_zero(&self.action_commitment)
            || all_zero(&self.nonce)
            || self.sequence == 0
            || self.issued_at_ms == 0
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms.saturating_sub(self.issued_at_ms) > MAX_CONTROL_LIFETIME_MS
            || now_ms > self.expires_at_ms
        {
            return Err(ControlContractError::InvalidControl);
        }
        if !verifier.verify(
            &self.issuer_identity_commitment,
            &self.canonical_bytes(),
            &self.signature,
        ) {
            return Err(ControlContractError::InvalidSignature);
        }
        Ok(())
    }

    fn canonical_without_id(&self) -> Vec<u8> {
        let mut copy = *self;
        copy.control_id = [0; 32];
        copy.signature = [0; 64];
        copy.canonical_bytes()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QueueInsertDisposition {
    Queued,
    AlreadyQueued,
}

/// Bounded retry queue. Persist only in authenticated encrypted local storage.
#[derive(Debug, Default)]
pub struct OfflineControlQueue {
    pending: BTreeMap<[u8; 32], OpaqueSignedControl>,
}

impl OfflineControlQueue {
    pub fn enqueue(
        &mut self,
        control: OpaqueSignedControl,
        now_ms: u64,
        verifier: &impl BurnSignatureVerifier,
    ) -> Result<QueueInsertDisposition, ControlContractError> {
        control.validate(now_ms, verifier)?;
        if let Some(existing) = self.pending.get(&control.control_id) {
            return if existing == &control {
                Ok(QueueInsertDisposition::AlreadyQueued)
            } else {
                Err(ControlContractError::ReplayRejected)
            };
        }
        if self.pending.len() >= MAX_QUEUED_CONTROLS
            || self
                .pending
                .values()
                .any(|item| item.nonce == control.nonce)
        {
            return Err(if self.pending.len() >= MAX_QUEUED_CONTROLS {
                ControlContractError::QueueFull
            } else {
                ControlContractError::ReplayRejected
            });
        }
        self.pending.insert(control.control_id, control);
        Ok(QueueInsertDisposition::Queued)
    }

    pub fn reconnect_batch(&mut self, now_ms: u64) -> Vec<OpaqueSignedControl> {
        self.pending.retain(|_, item| item.expires_at_ms >= now_ms);
        self.pending.values().take(MAX_BATCH).copied().collect()
    }

    pub fn acknowledge(&mut self, control_id: [u8; 32]) -> bool {
        self.pending.remove(&control_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ReceiveControlDisposition {
    Apply,
    AlreadyApplied,
}

/// Receiver-side replay journal. Entries may be compacted only after controls
/// have expired and authenticated encrypted persistence has committed.
#[derive(Debug, Default)]
pub struct ReceivedControlJournal {
    processed: BTreeMap<[u8; 32], ([u8; 24], u64)>,
    nonces: BTreeSet<[u8; 24]>,
}

impl ReceivedControlJournal {
    pub fn receive(
        &mut self,
        control: &OpaqueSignedControl,
        now_ms: u64,
        verifier: &impl BurnSignatureVerifier,
    ) -> Result<ReceiveControlDisposition, ControlContractError> {
        control.validate(now_ms, verifier)?;
        if self
            .processed
            .get(&control.control_id)
            .is_some_and(|(nonce, _)| nonce == &control.nonce)
        {
            return Ok(ReceiveControlDisposition::AlreadyApplied);
        }
        if self.processed.contains_key(&control.control_id) || self.nonces.contains(&control.nonce)
        {
            return Err(ControlContractError::ReplayRejected);
        }
        if self.processed.len() >= MAX_PROCESSED_CONTROLS {
            return Err(ControlContractError::JournalFull);
        }
        self.processed
            .insert(control.control_id, (control.nonce, control.expires_at_ms));
        self.nonces.insert(control.nonce);
        Ok(ReceiveControlDisposition::Apply)
    }

    pub fn compact_expired(&mut self, now_ms: u64, encrypted_commit_complete: bool) {
        if !encrypted_commit_complete {
            return;
        }
        let expired: Vec<_> = self
            .processed
            .iter()
            .filter_map(|(id, (nonce, expiry))| (*expiry < now_ms).then_some((*id, *nonce)))
            .collect();
        for (id, nonce) in expired {
            self.processed.remove(&id);
            self.nonces.remove(&nonce);
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimedMessageMode {
    FirstAuthenticatedOpen { lifetime_ms: u64 },
    FixedAbsolute { expires_at_ms: u64 },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TimedMessageState {
    pub message_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
    pub mode: TimedMessageMode,
    pub first_authenticated_open_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AuthenticatedLocalOpen {
    pub message_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
    pub opened_at_ms: u64,
    pub nonce: [u8; 24],
    pub signature: [u8; 64],
}

impl AuthenticatedLocalOpen {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(OPEN_DOMAIN);
        bytes.extend_from_slice(&self.message_commitment);
        bytes.extend_from_slice(&self.recipient_identity_commitment);
        bytes.extend_from_slice(&self.opened_at_ms.to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TimedMessageDisposition {
    WaitingForFirstOpen,
    ActiveUntil(u64),
    Expired(ExpiryPlan),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ExpiryPlan {
    pub message_commitment: [u8; 32],
    pub destroy_local_decrypt_keys: bool,
    pub clear_plaintext_and_local_caches: bool,
    pub request_encrypted_blob_deletion: bool,
    pub may_destroy_unread_content: bool,
    pub native_carrier_history: NativeCarrierHistoryEffect,
    pub screenshots_exports_and_external_copies: ExternalCopiesEffect,
}

impl TimedMessageState {
    pub fn new(
        message_commitment: [u8; 32],
        recipient_identity_commitment: [u8; 32],
        mode: TimedMessageMode,
    ) -> Result<Self, ControlContractError> {
        let valid_mode = match mode {
            TimedMessageMode::FirstAuthenticatedOpen { lifetime_ms } => {
                lifetime_ms > 0 && lifetime_ms <= MAX_TIMER_MS
            }
            TimedMessageMode::FixedAbsolute { expires_at_ms } => expires_at_ms > 0,
        };
        if all_zero(&message_commitment) || all_zero(&recipient_identity_commitment) || !valid_mode
        {
            return Err(ControlContractError::InvalidTimer);
        }
        Ok(Self {
            message_commitment,
            recipient_identity_commitment,
            mode,
            first_authenticated_open_ms: None,
        })
    }

    pub fn record_open(
        &mut self,
        opened: &AuthenticatedLocalOpen,
        verifier: &impl BurnSignatureVerifier,
    ) -> Result<bool, ControlContractError> {
        if opened.message_commitment != self.message_commitment
            || opened.recipient_identity_commitment != self.recipient_identity_commitment
            || opened.opened_at_ms == 0
            || all_zero(&opened.nonce)
            || !verifier.verify(
                &opened.recipient_identity_commitment,
                &opened.canonical_bytes(),
                &opened.signature,
            )
        {
            return Err(ControlContractError::InvalidSignature);
        }
        if self.first_authenticated_open_ms.is_none() {
            self.first_authenticated_open_ms = Some(opened.opened_at_ms);
            return Ok(true);
        }
        Ok(false)
    }

    pub fn evaluate(&self, now_ms: u64) -> TimedMessageDisposition {
        let (expiry, may_destroy_unread) = match self.mode {
            TimedMessageMode::FirstAuthenticatedOpen { lifetime_ms } => {
                let Some(opened) = self.first_authenticated_open_ms else {
                    return TimedMessageDisposition::WaitingForFirstOpen;
                };
                (opened.saturating_add(lifetime_ms), false)
            }
            TimedMessageMode::FixedAbsolute { expires_at_ms } => (expires_at_ms, true),
        };
        if now_ms < expiry {
            return TimedMessageDisposition::ActiveUntil(expiry);
        }
        TimedMessageDisposition::Expired(ExpiryPlan {
            message_commitment: self.message_commitment,
            destroy_local_decrypt_keys: true,
            clear_plaintext_and_local_caches: true,
            request_encrypted_blob_deletion: true,
            may_destroy_unread_content: may_destroy_unread
                && self.first_authenticated_open_ms.is_none(),
            native_carrier_history: NativeCarrierHistoryEffect::Unchanged,
            screenshots_exports_and_external_copies: ExternalCopiesEffect::NotControllable,
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OpenedReceiptConsent {
    pub requester_identity_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
    pub scope: BurnScopeCommitment,
    pub grant_id: [u8; 16],
    pub revocation_epoch: u64,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: [u8; 64],
}

impl OpenedReceiptConsent {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(RECEIPT_CONSENT_DOMAIN);
        bytes.extend_from_slice(&self.requester_identity_commitment);
        bytes.extend_from_slice(&self.recipient_identity_commitment);
        bytes.push(self.scope.level.tag());
        bytes.extend_from_slice(&self.scope.digest);
        bytes.extend_from_slice(&self.grant_id);
        bytes.extend_from_slice(&self.revocation_epoch.to_be_bytes());
        bytes.extend_from_slice(&self.issued_at_ms.to_be_bytes());
        bytes.extend_from_slice(&self.expires_at_ms.to_be_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ReceiptPermission<'a> {
    Allowed(&'a OpenedReceiptConsent),
    Declined,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OpenedReceiptStatus {
    Ready,
    PendingOffline,
    UnavailableDeclined,
    UnavailableUnknown,
    UnavailableRevokedOrExpired,
}

pub struct OpenedReceiptQuery<'a> {
    pub requester_tier: ProductTier,
    pub requester_identity: [u8; 32],
    pub recipient_identity: [u8; 32],
    pub scope: BurnScopeCommitment,
    pub permission: ReceiptPermission<'a>,
    pub recipient_online: bool,
    pub now_ms: u64,
    pub revoked_grants: &'a BTreeMap<[u8; 16], u64>,
}

pub fn opened_receipt_status(
    query: OpenedReceiptQuery<'_>,
    verifier: &impl BurnSignatureVerifier,
) -> Result<OpenedReceiptStatus, ControlContractError> {
    if query.requester_tier != ProductTier::Pro {
        return Err(ControlContractError::ProRequesterRequired);
    }
    let consent = match query.permission {
        ReceiptPermission::Declined => return Ok(OpenedReceiptStatus::UnavailableDeclined),
        ReceiptPermission::Unknown => return Ok(OpenedReceiptStatus::UnavailableUnknown),
        ReceiptPermission::Allowed(consent) => consent,
    };
    let active = consent.requester_identity_commitment == query.requester_identity
        && consent.recipient_identity_commitment == query.recipient_identity
        && consent.scope == query.scope
        && consent.issued_at_ms > 0
        && consent.issued_at_ms <= query.now_ms
        && consent.expires_at_ms >= query.now_ms
        && !all_zero(&consent.grant_id)
        && query
            .revoked_grants
            .get(&consent.grant_id)
            .is_none_or(|epoch| *epoch < consent.revocation_epoch)
        && verifier.verify(
            &query.recipient_identity,
            &consent.canonical_bytes(),
            &consent.signature,
        );
    if !active {
        return Ok(OpenedReceiptStatus::UnavailableRevokedOrExpired);
    }
    Ok(if query.recipient_online {
        OpenedReceiptStatus::Ready
    } else {
        OpenedReceiptStatus::PendingOffline
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ControlContractError {
    InvalidControl,
    InvalidSignature,
    ReplayRejected,
    QueueFull,
    JournalFull,
    InvalidTimer,
    ProRequesterRequired,
}

impl From<BurnContractError> for ControlContractError {
    fn from(_: BurnContractError) -> Self {
        Self::InvalidControl
    }
}

fn all_zero<const N: usize>(value: &[u8; N]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::burn_contract::{BurnScopeCommitment, BurnScopeLevel};

    struct TestVerifier;
    impl TestVerifier {
        fn sign(identity: &[u8; 32], message: &[u8]) -> [u8; 64] {
            let first = Sha256::new()
                .chain_update(identity)
                .chain_update(message)
                .finalize();
            let second = Sha256::new()
                .chain_update(message)
                .chain_update(identity)
                .finalize();
            let mut out = [0; 64];
            out[..32].copy_from_slice(&first);
            out[32..].copy_from_slice(&second);
            out
        }
    }
    impl BurnSignatureVerifier for TestVerifier {
        fn verify(&self, identity: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
            *signature == Self::sign(identity, message)
        }
    }

    fn scope() -> BurnScopeCommitment {
        BurnScopeCommitment::derive(
            BurnScopeLevel::CurrentChat,
            b"identity-test-1",
            Some(b"account-test-1"),
            Some(b"chat-test-1"),
        )
        .unwrap()
    }

    fn control() -> OpaqueSignedControl {
        let mut value = OpaqueSignedControl {
            version: 1,
            kind: OpaqueControlKind::Burn,
            control_id: [0; 32],
            scope: scope(),
            issuer_identity_commitment: [1; 32],
            recipient_identity_commitment: [2; 32],
            message_commitment: [3; 32],
            action_commitment: [4; 32],
            issued_at_ms: 100,
            expires_at_ms: 200,
            sequence: 1,
            nonce: [5; 24],
            signature: [0; 64],
        };
        value.control_id = Sha256::digest(value.canonical_without_id()).into();
        value.signature =
            TestVerifier::sign(&value.issuer_identity_commitment, &value.canonical_bytes());
        value
    }

    #[test]
    fn offline_queue_retries_and_receiver_applies_exactly_once() {
        let control = control();
        let mut queue = OfflineControlQueue::default();
        assert_eq!(
            queue.enqueue(control, 110, &TestVerifier),
            Ok(QueueInsertDisposition::Queued)
        );
        assert_eq!(
            queue.enqueue(control, 110, &TestVerifier),
            Ok(QueueInsertDisposition::AlreadyQueued)
        );
        assert_eq!(queue.reconnect_batch(110), vec![control]);
        let mut receiver = ReceivedControlJournal::default();
        assert_eq!(
            receiver.receive(&control, 110, &TestVerifier),
            Ok(ReceiveControlDisposition::Apply)
        );
        assert_eq!(
            receiver.receive(&control, 110, &TestVerifier),
            Ok(ReceiveControlDisposition::AlreadyApplied)
        );
        assert!(queue.acknowledge(control.control_id));
    }

    #[test]
    fn controls_are_opaque_and_tamper_authenticated() {
        let value = control();
        let rendered = String::from_utf8_lossy(&value.canonical_bytes()).into_owned();
        assert!(!rendered.contains("discord") && !rendered.contains("chat-test"));
        let mut tampered = value;
        tampered.sequence += 1;
        assert_eq!(
            tampered.validate(110, &TestVerifier),
            Err(ControlContractError::InvalidControl)
        );
    }

    fn authenticated_open(state: &TimedMessageState, at: u64) -> AuthenticatedLocalOpen {
        let mut opened = AuthenticatedLocalOpen {
            message_commitment: state.message_commitment,
            recipient_identity_commitment: state.recipient_identity_commitment,
            opened_at_ms: at,
            nonce: [9; 24],
            signature: [0; 64],
        };
        opened.signature = TestVerifier::sign(
            &opened.recipient_identity_commitment,
            &opened.canonical_bytes(),
        );
        opened
    }

    #[test]
    fn default_timer_starts_only_after_authenticated_local_open() {
        let mut state = TimedMessageState::new(
            [3; 32],
            [2; 32],
            TimedMessageMode::FirstAuthenticatedOpen { lifetime_ms: 50 },
        )
        .unwrap();
        assert_eq!(
            state.evaluate(1_000),
            TimedMessageDisposition::WaitingForFirstOpen
        );
        assert!(state
            .record_open(&authenticated_open(&state, 1_000), &TestVerifier)
            .unwrap());
        assert_eq!(
            state.evaluate(1_049),
            TimedMessageDisposition::ActiveUntil(1_050)
        );
        let TimedMessageDisposition::Expired(plan) = state.evaluate(1_050) else {
            panic!("expected expiry")
        };
        assert!(plan.destroy_local_decrypt_keys && plan.request_encrypted_blob_deletion);
        assert!(!plan.may_destroy_unread_content);
        assert_eq!(
            plan.native_carrier_history,
            NativeCarrierHistoryEffect::Unchanged
        );
    }

    #[test]
    fn fixed_absolute_expiry_truthfully_can_destroy_unread_content() {
        let state = TimedMessageState::new(
            [3; 32],
            [2; 32],
            TimedMessageMode::FixedAbsolute { expires_at_ms: 100 },
        )
        .unwrap();
        let TimedMessageDisposition::Expired(plan) = state.evaluate(100) else {
            panic!("expected expiry")
        };
        assert!(plan.may_destroy_unread_content);
    }

    fn receipt_consent(requester: [u8; 32], recipient: [u8; 32]) -> OpenedReceiptConsent {
        let mut value = OpenedReceiptConsent {
            requester_identity_commitment: requester,
            recipient_identity_commitment: recipient,
            scope: scope(),
            grant_id: [7; 16],
            revocation_epoch: 2,
            issued_at_ms: 10,
            expires_at_ms: 1_000,
            signature: [0; 64],
        };
        value.signature = TestVerifier::sign(&recipient, &value.canonical_bytes());
        value
    }

    fn receipt_query<'a>(
        tier: ProductTier,
        requester: [u8; 32],
        recipient: [u8; 32],
        permission: ReceiptPermission<'a>,
        online: bool,
        revoked: &'a BTreeMap<[u8; 16], u64>,
    ) -> OpenedReceiptQuery<'a> {
        OpenedReceiptQuery {
            requester_tier: tier,
            requester_identity: requester,
            recipient_identity: recipient,
            scope: scope(),
            permission,
            recipient_online: online,
            now_ms: 100,
            revoked_grants: revoked,
        }
    }

    #[test]
    fn opened_receipt_is_pro_for_requester_but_free_recipient_may_consent() {
        let requester = [1; 32];
        let recipient = [2; 32];
        let consent = receipt_consent(requester, recipient);
        let revoked = BTreeMap::new();
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Pro,
                    requester,
                    recipient,
                    ReceiptPermission::Allowed(&consent),
                    true,
                    &revoked,
                ),
                &TestVerifier
            ),
            Ok(OpenedReceiptStatus::Ready)
        );
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Pro,
                    requester,
                    recipient,
                    ReceiptPermission::Allowed(&consent),
                    false,
                    &revoked,
                ),
                &TestVerifier
            ),
            Ok(OpenedReceiptStatus::PendingOffline)
        );
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Free,
                    requester,
                    recipient,
                    ReceiptPermission::Allowed(&consent),
                    true,
                    &revoked,
                ),
                &TestVerifier
            ),
            Err(ControlContractError::ProRequesterRequired)
        );
    }

    #[test]
    fn declined_unknown_and_revoked_receipts_are_never_inferred() {
        let requester = [1; 32];
        let recipient = [2; 32];
        let consent = receipt_consent(requester, recipient);
        let empty = BTreeMap::new();
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Pro,
                    requester,
                    recipient,
                    ReceiptPermission::Declined,
                    true,
                    &empty,
                ),
                &TestVerifier
            ),
            Ok(OpenedReceiptStatus::UnavailableDeclined)
        );
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Pro,
                    requester,
                    recipient,
                    ReceiptPermission::Unknown,
                    true,
                    &empty,
                ),
                &TestVerifier
            ),
            Ok(OpenedReceiptStatus::UnavailableUnknown)
        );
        let mut revoked = BTreeMap::new();
        revoked.insert(consent.grant_id, consent.revocation_epoch);
        assert_eq!(
            opened_receipt_status(
                receipt_query(
                    ProductTier::Pro,
                    requester,
                    recipient,
                    ReceiptPermission::Allowed(&consent),
                    true,
                    &revoked,
                ),
                &TestVerifier
            ),
            Ok(OpenedReceiptStatus::UnavailableRevokedOrExpired)
        );
    }
}
