//! The one authorization boundary for burn instructions.
//!
//! Callers supply their authenticated local message metadata here before they
//! run any destructive effect.  A valid signature alone is not authority to
//! destroy an arbitrary message: the signed instruction must name the message
//! author, recipient, and conversation that this device already holds.

use std::collections::BTreeMap;

use crate::burn_contract::{
    plan_local_burn, plan_remote_friend_burn, BurnConfirmation, BurnContractError,
    BurnScopeCommitment, BurnScopeLevel, BurnSignatureVerifier, LocalBurnOptions, LocalBurnPlan,
    RemoteFriendBurnPlan, RemoteFriendBurnRequest,
};
use crate::control_contract::{ControlContractError, OpaqueControlKind, OpaqueSignedControl};

/// Bindings held by the trusted local conversation registry, never supplied by
/// an incoming control or renderer.
#[derive(Debug, Clone, Copy)]
pub struct BurnScopeBindings<'a> {
    pub level: BurnScopeLevel,
    pub identity: &'a [u8],
    pub service_account: Option<&'a [u8]>,
    pub conversation: Option<&'a [u8]>,
}

impl BurnScopeBindings<'_> {
    pub fn commitment(self) -> Result<BurnScopeCommitment, BurnAuthorizationError> {
        BurnScopeCommitment::derive(
            self.level,
            self.identity,
            self.service_account,
            self.conversation,
        )
        .map_err(BurnAuthorizationError::Contract)
    }
}

/// Immutable metadata recorded when a message is accepted into this device's
/// authenticated local store.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct StoredMessageAuthorization {
    pub scope: BurnScopeCommitment,
    pub message_commitment: [u8; 32],
    pub author_identity_commitment: [u8; 32],
    pub recipient_identity_commitment: [u8; 32],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AuthorizedIncomingBurn {
    pub scope: BurnScopeCommitment,
    pub message_commitment: [u8; 32],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BurnAuthorizationError {
    Contract(BurnContractError),
    InvalidControl(ControlContractError),
    NotBurnInstruction,
    IssuerIsNotMessageAuthor,
    RecipientDoesNotMatch,
    ConversationDoesNotMatch,
    MessageDoesNotMatch,
    RequestIssuerDoesNotMatchLocalIdentity,
}

/// Plan a local burn only for the scope derived from trusted local bindings.
pub fn authorize_local_burn(
    bindings: BurnScopeBindings<'_>,
    options: LocalBurnOptions,
    confirmation: &BurnConfirmation,
    verifier: &impl BurnSignatureVerifier,
) -> Result<LocalBurnPlan, BurnAuthorizationError> {
    let scope = bindings.commitment()?;
    plan_local_burn(scope, options, confirmation, verifier)
        .map_err(BurnAuthorizationError::Contract)
}

/// Plan peer notifications only when the request is for this device's own
/// identity and its exact locally-derived conversation scope.
pub fn authorize_remote_friend_burn(
    bindings: BurnScopeBindings<'_>,
    local_identity_commitment: [u8; 32],
    request: &RemoteFriendBurnRequest,
    revoked_grants: &BTreeMap<[u8; 16], u64>,
    verifier: &impl BurnSignatureVerifier,
) -> Result<RemoteFriendBurnPlan, BurnAuthorizationError> {
    if request.issuer_identity_commitment != local_identity_commitment {
        return Err(BurnAuthorizationError::RequestIssuerDoesNotMatchLocalIdentity);
    }
    if request.scope != bindings.commitment()? {
        return Err(BurnAuthorizationError::ConversationDoesNotMatch);
    }
    plan_remote_friend_burn(request, revoked_grants, verifier)
        .map_err(BurnAuthorizationError::Contract)
}

/// Authorize an authenticated inbound destruct instruction before a caller
/// applies local destruction.  This is N11: an issuer may burn only a message
/// it authored in its own conversation with this recipient.
pub fn authorize_incoming_message_burn(
    control: &OpaqueSignedControl,
    stored: StoredMessageAuthorization,
    now_ms: u64,
    verifier: &impl BurnSignatureVerifier,
) -> Result<AuthorizedIncomingBurn, BurnAuthorizationError> {
    control
        .validate(now_ms, verifier)
        .map_err(BurnAuthorizationError::InvalidControl)?;
    if control.kind != OpaqueControlKind::Burn {
        return Err(BurnAuthorizationError::NotBurnInstruction);
    }
    if control.issuer_identity_commitment != stored.author_identity_commitment {
        return Err(BurnAuthorizationError::IssuerIsNotMessageAuthor);
    }
    if control.recipient_identity_commitment != stored.recipient_identity_commitment {
        return Err(BurnAuthorizationError::RecipientDoesNotMatch);
    }
    if control.scope != stored.scope {
        return Err(BurnAuthorizationError::ConversationDoesNotMatch);
    }
    if control.message_commitment != stored.message_commitment {
        return Err(BurnAuthorizationError::MessageDoesNotMatch);
    }
    Ok(AuthorizedIncomingBurn {
        scope: stored.scope,
        message_commitment: stored.message_commitment,
    })
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    struct AcceptingVerifier;

    impl BurnSignatureVerifier for AcceptingVerifier {
        fn verify(&self, _: &[u8; 32], _: &[u8], _: &[u8; 64]) -> bool {
            true
        }
    }

    fn bindings<'a>(conversation: &'a [u8]) -> BurnScopeBindings<'a> {
        BurnScopeBindings {
            level: BurnScopeLevel::CurrentChat,
            identity: b"local-identity",
            service_account: Some(b"service-account"),
            conversation: Some(conversation),
        }
    }

    fn control(scope: BurnScopeCommitment) -> OpaqueSignedControl {
        let mut control = OpaqueSignedControl {
            version: 1,
            kind: OpaqueControlKind::Burn,
            control_id: [0; 32],
            scope,
            issuer_identity_commitment: [1; 32],
            recipient_identity_commitment: [2; 32],
            message_commitment: [3; 32],
            action_commitment: [4; 32],
            issued_at_ms: 100,
            expires_at_ms: 200,
            sequence: 1,
            nonce: [5; 24],
            signature: [6; 64],
        };
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"OSL/opaque-control/v1");
        bytes.push(1);
        bytes.push(1);
        bytes.extend_from_slice(&[0; 32]);
        bytes.push(1);
        bytes.extend_from_slice(&scope.digest);
        bytes.extend_from_slice(&control.issuer_identity_commitment);
        bytes.extend_from_slice(&control.recipient_identity_commitment);
        bytes.extend_from_slice(&control.message_commitment);
        bytes.extend_from_slice(&control.action_commitment);
        bytes.extend_from_slice(&control.issued_at_ms.to_be_bytes());
        bytes.extend_from_slice(&control.expires_at_ms.to_be_bytes());
        bytes.extend_from_slice(&control.sequence.to_be_bytes());
        bytes.extend_from_slice(&control.nonce);
        control.control_id = Sha256::digest(bytes).into();
        control
    }

    #[test]
    fn tf_20_cross_conversation_instruction_is_refused() {
        let own_scope = bindings(b"conversation-a").commitment().unwrap();
        let foreign_scope = bindings(b"conversation-b").commitment().unwrap();
        let stored = StoredMessageAuthorization {
            scope: own_scope,
            message_commitment: [3; 32],
            author_identity_commitment: [1; 32],
            recipient_identity_commitment: [2; 32],
        };

        assert_eq!(
            authorize_incoming_message_burn(
                &control(foreign_scope),
                stored,
                150,
                &AcceptingVerifier
            ),
            Err(BurnAuthorizationError::ConversationDoesNotMatch)
        );
        assert_eq!(
            authorize_incoming_message_burn(&control(own_scope), stored, 150, &AcceptingVerifier),
            Ok(AuthorizedIncomingBurn {
                scope: own_scope,
                message_commitment: [3; 32],
            })
        );
    }
}
