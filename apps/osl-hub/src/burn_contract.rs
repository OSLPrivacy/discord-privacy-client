//! Authorization contract around the Hub's existing burn executors.
//!
//! `security::burn_scope`, `broker::burn_local_protected_context`, and
//! `identity_registry::burn_active_identity` own destructive mutations. This
//! module normalizes confirmation, consent, honesty, and replay requirements;
//! it does not duplicate those mutations or delete native-service history.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

const CONFIRM_DOMAIN: &[u8] = b"OSL/burn-confirm/v1";
const CONSENT_DOMAIN: &[u8] = b"OSL/remote-burn-consent/v1";
const NOTICE_DOMAIN: &[u8] = b"OSL/burn-notice/v1";
const REQUEST_DOMAIN: &[u8] = b"OSL/burn-request/v1";
const MAX_REMOTE_IDENTITIES: usize = 512;
const MAX_BURN_JOURNAL_ENTRIES: usize = 8_192;
const REVOCATION_DOMAIN: &[u8] = b"OSL/remote-burn-consent-revocation/v1";

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub enum BurnScopeLevel {
    CurrentChat,
    Service,
    Account,
}

impl BurnScopeLevel {
    pub(crate) fn tag(self) -> u8 {
        match self {
            Self::CurrentChat => 1,
            Self::Service => 2,
            Self::Account => 3,
        }
    }
}

/// Opaque local commitment: notices never contain service/account/chat names.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct BurnScopeCommitment {
    pub level: BurnScopeLevel,
    pub digest: [u8; 32],
}

impl BurnScopeCommitment {
    pub fn derive(
        level: BurnScopeLevel,
        identity_binding: &[u8],
        service_account_binding: Option<&[u8]>,
        conversation_binding: Option<&[u8]>,
    ) -> Result<Self, BurnContractError> {
        let binding_ok = |value: &[u8]| !value.is_empty() && value.len() <= 256;
        if !binding_ok(identity_binding) {
            return Err(BurnContractError::InvalidScope);
        }
        let shape_ok = match level {
            BurnScopeLevel::CurrentChat => {
                service_account_binding.is_some() && conversation_binding.is_some()
            }
            BurnScopeLevel::Service => {
                service_account_binding.is_some() && conversation_binding.is_none()
            }
            BurnScopeLevel::Account => {
                service_account_binding.is_none() && conversation_binding.is_none()
            }
        };
        if !shape_ok
            || service_account_binding.is_some_and(|value| !binding_ok(value))
            || conversation_binding.is_some_and(|value| !binding_ok(value))
        {
            return Err(BurnContractError::InvalidScope);
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"OSL/burn-scope/v1");
        bytes.push(level.tag());
        write_lp(&mut bytes, identity_binding)?;
        write_optional_lp(&mut bytes, service_account_binding)?;
        write_optional_lp(&mut bytes, conversation_binding)?;
        Ok(Self {
            level,
            digest: Sha256::digest(bytes).into(),
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ProductTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalBurnOptions {
    /// Also forget locally cached incoming/member messages in this scope.
    pub forget_incoming_and_member_messages: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct BurnConfirmation {
    pub scope: BurnScopeCommitment,
    pub effects_digest: [u8; 32],
    pub confirmed_at_ms: u64,
    pub nonce: [u8; 24],
    pub confirming_identity_commitment: [u8; 32],
    pub signature: [u8; 64],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NativeCarrierHistoryEffect {
    Unchanged,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExternalCopiesEffect {
    /// Screenshots, exports, backups, provider and recipient copies cannot be erased.
    NotControllable,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalBurnPlan {
    pub scope: BurnScopeCommitment,
    pub destroy_local_decrypt_capability: bool,
    pub destroy_local_key_mappings: bool,
    pub clear_local_caches: bool,
    pub forget_incoming_and_member_messages: bool,
    pub native_carrier_history: NativeCarrierHistoryEffect,
    pub screenshots_exports_and_external_copies: ExternalCopiesEffect,
    /// Uninstall is separate, post-burn, and available only after account burn.
    pub may_offer_separate_uninstall_after_completion: bool,
}

pub trait BurnSignatureVerifier {
    fn verify(
        &self,
        signer_identity_commitment: &[u8; 32],
        canonical_message: &[u8],
        signature: &[u8; 64],
    ) -> bool;
}

pub fn local_effects_digest(scope: BurnScopeCommitment, options: LocalBurnOptions) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"OSL/local-burn-effects/v1");
    bytes.push(scope.level.tag());
    bytes.extend_from_slice(&scope.digest);
    bytes.push(u8::from(options.forget_incoming_and_member_messages));
    bytes.extend_from_slice(b"local-keys=destroy;cache=clear;native=unchanged;copies=unknown");
    Sha256::digest(bytes).into()
}

pub fn plan_local_burn(
    scope: BurnScopeCommitment,
    options: LocalBurnOptions,
    confirmation: &BurnConfirmation,
    verifier: &impl BurnSignatureVerifier,
) -> Result<LocalBurnPlan, BurnContractError> {
    if confirmation.scope != scope
        || confirmation.confirmed_at_ms == 0
        || all_zero(&confirmation.nonce)
        || all_zero(&confirmation.confirming_identity_commitment)
    {
        return Err(BurnContractError::ConfirmationRequired);
    }
    if confirmation.effects_digest != local_effects_digest(scope, options) {
        return Err(BurnContractError::ConfirmationDoesNotMatch);
    }
    if !verifier.verify(
        &confirmation.confirming_identity_commitment,
        &canonical_confirmation(confirmation),
        &confirmation.signature,
    ) {
        return Err(BurnContractError::InvalidSignature);
    }
    Ok(LocalBurnPlan {
        scope,
        destroy_local_decrypt_capability: true,
        destroy_local_key_mappings: true,
        clear_local_caches: true,
        forget_incoming_and_member_messages: options.forget_incoming_and_member_messages,
        native_carrier_history: NativeCarrierHistoryEffect::Unchanged,
        screenshots_exports_and_external_copies: ExternalCopiesEffect::NotControllable,
        may_offer_separate_uninstall_after_completion: scope.level == BurnScopeLevel::Account,
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RemoteBurnConsent {
    pub affected_identity_commitment: [u8; 32],
    pub scope: BurnScopeCommitment,
    pub grant_id: [u8; 16],
    pub revocation_epoch: u64,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: [u8; 64],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RemoteConsentRevocation {
    pub affected_identity_commitment: [u8; 32],
    pub grant_id: [u8; 16],
    pub revocation_epoch: u64,
    pub issued_at_ms: u64,
    pub nonce: [u8; 24],
    pub signature: [u8; 64],
}

pub fn apply_remote_consent_revocation(
    revoked_grants: &mut BTreeMap<[u8; 16], u64>,
    revocation: &RemoteConsentRevocation,
    verifier: &impl BurnSignatureVerifier,
) -> Result<bool, BurnContractError> {
    if all_zero(&revocation.affected_identity_commitment)
        || all_zero(&revocation.grant_id)
        || all_zero(&revocation.nonce)
        || revocation.revocation_epoch == 0
        || revocation.issued_at_ms == 0
        || !verifier.verify(
            &revocation.affected_identity_commitment,
            &canonical_revocation(revocation),
            &revocation.signature,
        )
    {
        return Err(BurnContractError::InvalidSignature);
    }
    match revoked_grants.get(&revocation.grant_id) {
        Some(epoch) if *epoch > revocation.revocation_epoch => {
            Err(BurnContractError::ReplayRejected)
        }
        Some(epoch) if *epoch == revocation.revocation_epoch => Ok(false),
        _ => {
            revoked_grants.insert(revocation.grant_id, revocation.revocation_epoch);
            Ok(true)
        }
    }
}

/// Authenticated opaque metadata only; it deliberately has no text/service fields.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AuthenticatedBurnNotice {
    pub version: u8,
    pub burn_id: [u8; 32],
    pub scope: BurnScopeCommitment,
    pub issuer_identity_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
    pub issued_at_ms: u64,
    pub nonce: [u8; 24],
    pub consent_grant_id: [u8; 16],
    pub consent_revocation_epoch: u64,
    pub signature: [u8; 64],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoteFriendBurnRequest {
    pub tier: ProductTier,
    pub explicitly_enabled: bool,
    pub scope: BurnScopeCommitment,
    pub issuer_identity_commitment: [u8; 32],
    pub request_id: [u8; 24],
    pub requested_at_ms: u64,
    pub confirmation: BurnConfirmation,
    pub affected_identity_commitments: Vec<[u8; 32]>,
    pub consent: Vec<RemoteBurnConsent>,
    pub notices: Vec<AuthenticatedBurnNotice>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RemoteFriendBurnPlan {
    pub burn_id: [u8; 32],
    pub notices: Vec<AuthenticatedBurnNotice>,
}

pub fn plan_remote_friend_burn(
    request: &RemoteFriendBurnRequest,
    revoked_grants: &BTreeMap<[u8; 16], u64>,
    verifier: &impl BurnSignatureVerifier,
) -> Result<RemoteFriendBurnPlan, BurnContractError> {
    if request.tier != ProductTier::Pro {
        return Err(BurnContractError::ProRequired);
    }
    if !request.explicitly_enabled {
        return Err(BurnContractError::RemoteBurnNotEnabled);
    }
    if request.requested_at_ms == 0
        || all_zero(&request.request_id)
        || all_zero(&request.issuer_identity_commitment)
        || request.affected_identity_commitments.is_empty()
        || request.affected_identity_commitments.len() > MAX_REMOTE_IDENTITIES
    {
        return Err(BurnContractError::InvalidRequest);
    }
    let affected: BTreeSet<_> = request
        .affected_identity_commitments
        .iter()
        .copied()
        .collect();
    if affected.len() != request.affected_identity_commitments.len()
        || affected.iter().any(all_zero)
    {
        return Err(BurnContractError::InvalidRequest);
    }
    plan_local_burn(
        request.scope,
        LocalBurnOptions {
            forget_incoming_and_member_messages: false,
        },
        &request.confirmation,
        verifier,
    )?;
    if request.confirmation.confirming_identity_commitment != request.issuer_identity_commitment {
        return Err(BurnContractError::ConfirmationDoesNotMatch);
    }
    if request.consent.len() != affected.len() || request.notices.len() != affected.len() {
        return Err(BurnContractError::ConsentRequired);
    }
    let grants: BTreeMap<_, _> = request
        .consent
        .iter()
        .map(|grant| (grant.affected_identity_commitment, grant))
        .collect();
    let notices: BTreeMap<_, _> = request
        .notices
        .iter()
        .map(|notice| (notice.recipient_identity_commitment, notice))
        .collect();
    if grants.len() != request.consent.len() || notices.len() != request.notices.len() {
        return Err(BurnContractError::ConsentRequired);
    }
    let burn_id = remote_burn_id(request);
    for identity in affected {
        let grant = grants
            .get(&identity)
            .ok_or(BurnContractError::ConsentRequired)?;
        if grant.scope != request.scope
            || grant.issued_at_ms == 0
            || grant.issued_at_ms > request.requested_at_ms
            || grant.expires_at_ms < request.requested_at_ms
            || all_zero(&grant.grant_id)
            || revoked_grants
                .get(&grant.grant_id)
                .is_some_and(|epoch| *epoch >= grant.revocation_epoch)
        {
            return Err(BurnContractError::ConsentExpiredOrRevoked);
        }
        if !verifier.verify(&identity, &canonical_consent(grant), &grant.signature) {
            return Err(BurnContractError::InvalidSignature);
        }
        let notice = notices
            .get(&identity)
            .ok_or(BurnContractError::InvalidNotice)?;
        if notice.version != 1
            || notice.burn_id != burn_id
            || notice.scope != request.scope
            || notice.issuer_identity_commitment != request.issuer_identity_commitment
            || notice.recipient_identity_commitment != identity
            || notice.issued_at_ms != request.requested_at_ms
            || notice.consent_grant_id != grant.grant_id
            || notice.consent_revocation_epoch != grant.revocation_epoch
            || all_zero(&notice.nonce)
        {
            return Err(BurnContractError::InvalidNotice);
        }
        if !verifier.verify(
            &request.issuer_identity_commitment,
            &canonical_notice(notice),
            &notice.signature,
        ) {
            return Err(BurnContractError::InvalidSignature);
        }
    }
    Ok(RemoteFriendBurnPlan {
        burn_id,
        notices: request.notices.clone(),
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BurnJournalDisposition {
    Applied,
    AlreadyApplied,
}

#[derive(Debug, Default)]
pub struct BurnReplayJournal {
    accepted: BTreeMap<[u8; 32], [u8; 24]>,
    nonces: BTreeMap<[u8; 24], [u8; 32]>,
}

impl BurnReplayJournal {
    pub fn accept(
        &mut self,
        burn_id: [u8; 32],
        nonce: [u8; 24],
    ) -> Result<BurnJournalDisposition, BurnContractError> {
        if all_zero(&burn_id) || all_zero(&nonce) {
            return Err(BurnContractError::InvalidRequest);
        }
        if self.accepted.get(&burn_id) == Some(&nonce) {
            return Ok(BurnJournalDisposition::AlreadyApplied);
        }
        if self.accepted.contains_key(&burn_id) || self.nonces.contains_key(&nonce) {
            return Err(BurnContractError::ReplayRejected);
        }
        if self.accepted.len() >= MAX_BURN_JOURNAL_ENTRIES {
            return Err(BurnContractError::JournalFull);
        }
        self.accepted.insert(burn_id, nonce);
        self.nonces.insert(nonce, burn_id);
        Ok(BurnJournalDisposition::Applied)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BurnContractError {
    InvalidScope,
    ConfirmationRequired,
    ConfirmationDoesNotMatch,
    InvalidSignature,
    ProRequired,
    RemoteBurnNotEnabled,
    InvalidRequest,
    ConsentRequired,
    ConsentExpiredOrRevoked,
    InvalidNotice,
    ReplayRejected,
    JournalFull,
}

fn canonical_confirmation(value: &BurnConfirmation) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(CONFIRM_DOMAIN);
    bytes.push(value.scope.level.tag());
    bytes.extend_from_slice(&value.scope.digest);
    bytes.extend_from_slice(&value.effects_digest);
    bytes.extend_from_slice(&value.confirmed_at_ms.to_be_bytes());
    bytes.extend_from_slice(&value.nonce);
    bytes.extend_from_slice(&value.confirming_identity_commitment);
    bytes
}

fn canonical_consent(value: &RemoteBurnConsent) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(CONSENT_DOMAIN);
    bytes.extend_from_slice(&value.affected_identity_commitment);
    bytes.push(value.scope.level.tag());
    bytes.extend_from_slice(&value.scope.digest);
    bytes.extend_from_slice(&value.grant_id);
    bytes.extend_from_slice(&value.revocation_epoch.to_be_bytes());
    bytes.extend_from_slice(&value.issued_at_ms.to_be_bytes());
    bytes.extend_from_slice(&value.expires_at_ms.to_be_bytes());
    bytes
}

fn canonical_revocation(value: &RemoteConsentRevocation) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(REVOCATION_DOMAIN);
    bytes.extend_from_slice(&value.affected_identity_commitment);
    bytes.extend_from_slice(&value.grant_id);
    bytes.extend_from_slice(&value.revocation_epoch.to_be_bytes());
    bytes.extend_from_slice(&value.issued_at_ms.to_be_bytes());
    bytes.extend_from_slice(&value.nonce);
    bytes
}

fn canonical_notice(value: &AuthenticatedBurnNotice) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(NOTICE_DOMAIN);
    bytes.push(value.version);
    bytes.extend_from_slice(&value.burn_id);
    bytes.push(value.scope.level.tag());
    bytes.extend_from_slice(&value.scope.digest);
    bytes.extend_from_slice(&value.issuer_identity_commitment);
    bytes.extend_from_slice(&value.recipient_identity_commitment);
    bytes.extend_from_slice(&value.issued_at_ms.to_be_bytes());
    bytes.extend_from_slice(&value.nonce);
    bytes.extend_from_slice(&value.consent_grant_id);
    bytes.extend_from_slice(&value.consent_revocation_epoch.to_be_bytes());
    bytes
}

fn remote_burn_id(request: &RemoteFriendBurnRequest) -> [u8; 32] {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(REQUEST_DOMAIN);
    bytes.push(request.scope.level.tag());
    bytes.extend_from_slice(&request.scope.digest);
    bytes.extend_from_slice(&request.issuer_identity_commitment);
    bytes.extend_from_slice(&request.request_id);
    bytes.extend_from_slice(&request.requested_at_ms.to_be_bytes());
    for identity in &request.affected_identity_commitments {
        bytes.extend_from_slice(identity);
    }
    Sha256::digest(bytes).into()
}

fn write_optional_lp(target: &mut Vec<u8>, value: Option<&[u8]>) -> Result<(), BurnContractError> {
    match value {
        Some(value) => {
            target.push(1);
            write_lp(target, value)
        }
        None => {
            target.push(0);
            Ok(())
        }
    }
}

fn write_lp(target: &mut Vec<u8>, value: &[u8]) -> Result<(), BurnContractError> {
    let length = u32::try_from(value.len()).map_err(|_| BurnContractError::InvalidScope)?;
    target.extend_from_slice(&length.to_be_bytes());
    target.extend_from_slice(value);
    Ok(())
}

fn all_zero<const N: usize>(value: &[u8; N]) -> bool {
    value.iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestVerifier;

    impl TestVerifier {
        fn signature(identity: &[u8; 32], message: &[u8]) -> [u8; 64] {
            let first = Sha256::new()
                .chain_update(identity)
                .chain_update(message)
                .finalize();
            let second = Sha256::new()
                .chain_update(message)
                .chain_update(identity)
                .finalize();
            let mut signature = [0; 64];
            signature[..32].copy_from_slice(&first);
            signature[32..].copy_from_slice(&second);
            signature
        }
    }

    impl BurnSignatureVerifier for TestVerifier {
        fn verify(&self, id: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
            *signature == Self::signature(id, message)
        }
    }

    fn scope(level: BurnScopeLevel) -> BurnScopeCommitment {
        match level {
            BurnScopeLevel::CurrentChat => BurnScopeCommitment::derive(
                level,
                b"identity-test-1",
                Some(b"service-account-test-1"),
                Some(b"chat-test-1"),
            ),
            BurnScopeLevel::Service => BurnScopeCommitment::derive(
                level,
                b"identity-test-1",
                Some(b"service-account-test-1"),
                None,
            ),
            BurnScopeLevel::Account => {
                BurnScopeCommitment::derive(level, b"identity-test-1", None, None)
            }
        }
        .unwrap()
    }

    fn confirmation(scope: BurnScopeCommitment, issuer: [u8; 32]) -> BurnConfirmation {
        let mut value = BurnConfirmation {
            scope,
            effects_digest: local_effects_digest(
                scope,
                LocalBurnOptions {
                    forget_incoming_and_member_messages: false,
                },
            ),
            confirmed_at_ms: 100,
            nonce: [5; 24],
            confirming_identity_commitment: issuer,
            signature: [0; 64],
        };
        value.signature = TestVerifier::signature(&issuer, &canonical_confirmation(&value));
        value
    }

    #[test]
    fn three_scopes_have_distinct_strict_shapes() {
        assert_ne!(
            scope(BurnScopeLevel::CurrentChat).digest,
            scope(BurnScopeLevel::Service).digest
        );
        assert_ne!(
            scope(BurnScopeLevel::Service).digest,
            scope(BurnScopeLevel::Account).digest
        );
        assert!(BurnScopeCommitment::derive(
            BurnScopeLevel::CurrentChat,
            b"identity-test-1",
            Some(b"service-account-test-1"),
            None
        )
        .is_err());
    }

    #[test]
    fn local_burn_is_truthful_and_uninstall_is_account_only() {
        for level in [
            BurnScopeLevel::CurrentChat,
            BurnScopeLevel::Service,
            BurnScopeLevel::Account,
        ] {
            let scope = scope(level);
            let plan = plan_local_burn(
                scope,
                LocalBurnOptions {
                    forget_incoming_and_member_messages: false,
                },
                &confirmation(scope, [1; 32]),
                &TestVerifier,
            )
            .unwrap();
            assert!(
                plan.destroy_local_decrypt_capability
                    && plan.destroy_local_key_mappings
                    && plan.clear_local_caches
            );
            assert_eq!(
                plan.native_carrier_history,
                NativeCarrierHistoryEffect::Unchanged
            );
            assert_eq!(
                plan.screenshots_exports_and_external_copies,
                ExternalCopiesEffect::NotControllable
            );
            assert_eq!(
                plan.may_offer_separate_uninstall_after_completion,
                level == BurnScopeLevel::Account
            );
        }
    }

    #[test]
    fn changed_options_invalidate_confirmation() {
        let scope = scope(BurnScopeLevel::CurrentChat);
        assert_eq!(
            plan_local_burn(
                scope,
                LocalBurnOptions {
                    forget_incoming_and_member_messages: true
                },
                &confirmation(scope, [1; 32]),
                &TestVerifier
            ),
            Err(BurnContractError::ConfirmationDoesNotMatch)
        );
    }

    fn remote_request() -> RemoteFriendBurnRequest {
        let scope = scope(BurnScopeLevel::CurrentChat);
        let issuer = [1; 32];
        let recipient = [2; 32];
        let mut grant = RemoteBurnConsent {
            affected_identity_commitment: recipient,
            scope,
            grant_id: [8; 16],
            revocation_epoch: 4,
            issued_at_ms: 10,
            expires_at_ms: 1_000,
            signature: [0; 64],
        };
        grant.signature = TestVerifier::signature(&recipient, &canonical_consent(&grant));
        let mut request = RemoteFriendBurnRequest {
            tier: ProductTier::Pro,
            explicitly_enabled: true,
            scope,
            issuer_identity_commitment: issuer,
            request_id: [9; 24],
            requested_at_ms: 100,
            confirmation: confirmation(scope, issuer),
            affected_identity_commitments: vec![recipient],
            consent: vec![grant],
            notices: Vec::new(),
        };
        let mut notice = AuthenticatedBurnNotice {
            version: 1,
            burn_id: remote_burn_id(&request),
            scope,
            issuer_identity_commitment: issuer,
            recipient_identity_commitment: recipient,
            issued_at_ms: 100,
            nonce: [7; 24],
            consent_grant_id: grant.grant_id,
            consent_revocation_epoch: grant.revocation_epoch,
            signature: [0; 64],
        };
        notice.signature = TestVerifier::signature(&issuer, &canonical_notice(&notice));
        request.notices.push(notice);
        request
    }

    #[test]
    fn remote_burn_requires_pro_opt_in_and_every_consent() {
        let request = remote_request();
        assert_eq!(
            plan_remote_friend_burn(&request, &BTreeMap::new(), &TestVerifier)
                .unwrap()
                .notices
                .len(),
            1
        );
        let mut free = request.clone();
        free.tier = ProductTier::Free;
        assert_eq!(
            plan_remote_friend_burn(&free, &BTreeMap::new(), &TestVerifier),
            Err(BurnContractError::ProRequired)
        );
        let mut missing = request;
        missing.consent.clear();
        assert_eq!(
            plan_remote_friend_burn(&missing, &BTreeMap::new(), &TestVerifier),
            Err(BurnContractError::ConsentRequired)
        );
    }

    #[test]
    fn revoked_consent_fails_closed() {
        let request = remote_request();
        let mut revoked = BTreeMap::new();
        revoked.insert(
            request.consent[0].grant_id,
            request.consent[0].revocation_epoch,
        );
        assert_eq!(
            plan_remote_friend_burn(&request, &revoked, &TestVerifier),
            Err(BurnContractError::ConsentExpiredOrRevoked)
        );
    }

    #[test]
    fn signed_revocation_is_idempotent_and_rejects_rollback_or_tamper() {
        let identity = [2; 32];
        let mut revocation = RemoteConsentRevocation {
            affected_identity_commitment: identity,
            grant_id: [8; 16],
            revocation_epoch: 4,
            issued_at_ms: 100,
            nonce: [6; 24],
            signature: [0; 64],
        };
        revocation.signature =
            TestVerifier::signature(&identity, &canonical_revocation(&revocation));
        let mut journal = BTreeMap::new();
        assert_eq!(
            apply_remote_consent_revocation(&mut journal, &revocation, &TestVerifier),
            Ok(true)
        );
        assert_eq!(
            apply_remote_consent_revocation(&mut journal, &revocation, &TestVerifier),
            Ok(false)
        );
        let mut rollback = revocation;
        rollback.revocation_epoch = 3;
        rollback.signature = TestVerifier::signature(&identity, &canonical_revocation(&rollback));
        assert_eq!(
            apply_remote_consent_revocation(&mut journal, &rollback, &TestVerifier),
            Err(BurnContractError::ReplayRejected)
        );
        let mut tampered = revocation;
        tampered.signature[0] ^= 1;
        assert_eq!(
            apply_remote_consent_revocation(&mut journal, &tampered, &TestVerifier),
            Err(BurnContractError::InvalidSignature)
        );
    }

    #[test]
    fn notice_is_opaque_and_tamper_authenticated() {
        let request = remote_request();
        let notice_bytes = canonical_notice(&request.notices[0]);
        let rendered = String::from_utf8_lossy(&notice_bytes);
        assert!(
            !rendered.contains("discord")
                && !rendered.contains("service-account")
                && !rendered.contains("chat-test")
        );
        let mut tampered = request;
        tampered.notices[0].signature[0] ^= 1;
        assert_eq!(
            plan_remote_friend_burn(&tampered, &BTreeMap::new(), &TestVerifier),
            Err(BurnContractError::InvalidSignature)
        );
    }

    #[test]
    fn replay_journal_is_idempotent_and_nonce_safe() {
        let mut journal = BurnReplayJournal::default();
        assert_eq!(
            journal.accept([1; 32], [2; 24]),
            Ok(BurnJournalDisposition::Applied)
        );
        assert_eq!(
            journal.accept([1; 32], [2; 24]),
            Ok(BurnJournalDisposition::AlreadyApplied)
        );
        assert_eq!(
            journal.accept([3; 32], [2; 24]),
            Err(BurnContractError::ReplayRejected)
        );
        assert_eq!(
            journal.accept([1; 32], [4; 24]),
            Err(BurnContractError::ReplayRejected)
        );
    }
}
