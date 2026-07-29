//! Type-only friend-request trust model.
//!
//! A scope grant is a capability value, not a renderer claim. It can only be
//! minted from a complete TOFU-trusted key bundle, and a request without that
//! grant is a refusal. Legacy friend-code payloads, `osl_` routing labels,
//! unsigned metadata, renderer DTOs and `safety_number_verified` booleans have
//! no constructor path into [`FriendScopeGrant`].

use crate::scope::Scope;
use crate::tofu::KeyBundle;
use sha2::{Digest, Sha256};
use std::fmt;

const FRIEND_AUTHORITY_DOMAIN: &[u8] = b"OSL-FRIEND-AUTHORITY-v1";

/// Errors for constructing or admitting a typed friend request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FriendRequestError {
    #[error("friend request refused")]
    GrantAbsent,
    #[error("friend request authority is not authenticated")]
    UnauthenticatedAuthority,
    #[error("friend request scope grant does not match its parties")]
    GrantPartyMismatch,
    #[error("friend request trust object is incomplete")]
    IncompleteTrustObject,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct FriendAuthorityFingerprint([u8; 32]);

impl fmt::Debug for FriendAuthorityFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FriendAuthorityFingerprint(<redacted>)")
    }
}

/// Authority that has already passed the complete-bundle TOFU ceremony.
///
/// This type intentionally does not accept legacy routing labels, friend-code
/// key fragments, unsigned metadata, renderer fields or boolean verification
/// flags. IPC-internal callers must first obtain the full trusted
/// [`KeyBundle`] from peer state.
#[derive(Clone, PartialEq, Eq)]
pub struct VerifiedFriendAuthority {
    fingerprint: FriendAuthorityFingerprint,
}

impl VerifiedFriendAuthority {
    pub(crate) fn from_tofu_trusted_key_bundle(
        bundle: &KeyBundle,
    ) -> Result<Self, FriendRequestError> {
        if bundle.ed25519_pub.is_empty()
            || bundle.x25519_pub.is_empty()
            || bundle.mlkem768_pub.is_empty()
        {
            return Err(FriendRequestError::IncompleteTrustObject);
        }
        Ok(Self {
            fingerprint: fingerprint_bundle(bundle),
        })
    }
}

impl fmt::Debug for VerifiedFriendAuthority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedFriendAuthority")
            .field("fingerprint", &self.fingerprint)
            .finish()
    }
}

/// One verified side of a friend request.
#[derive(Clone, PartialEq, Eq)]
pub struct FriendPeer {
    authority: VerifiedFriendAuthority,
}

impl FriendPeer {
    pub(crate) fn from_authority(authority: VerifiedFriendAuthority) -> Self {
        Self { authority }
    }
}

impl fmt::Debug for FriendPeer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FriendPeer")
            .field("authority", &self.authority)
            .finish()
    }
}

/// Explicit scope grant carried by an authenticated friend request.
#[derive(Clone, PartialEq, Eq)]
pub struct FriendScopeGrant {
    scope: Scope,
    requester: FriendAuthorityFingerprint,
    target: FriendAuthorityFingerprint,
}

impl FriendScopeGrant {
    pub(crate) fn new(
        requester: &VerifiedFriendAuthority,
        target: &VerifiedFriendAuthority,
        scope: Scope,
    ) -> Self {
        Self {
            scope,
            requester: requester.fingerprint,
            target: target.fingerprint,
        }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn grants_scope(&self, scope: &Scope) -> bool {
        &self.scope == scope
    }
}

impl fmt::Debug for FriendScopeGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FriendScopeGrant")
            .field("scope_kind", &self.scope.kind)
            .field("requester", &self.requester)
            .field("target", &self.target)
            .finish()
    }
}

/// Canonical friend request admitted by the type layer.
///
/// `scope_grant` is deliberately non-optional: callers that have no grant must
/// receive [`FriendRequestError::GrantAbsent`] instead of an allow-by-default
/// request object.
#[derive(Clone, PartialEq, Eq)]
pub struct FriendRequest {
    pub requester: FriendPeer,
    pub target: FriendPeer,
    pub scope_grant: FriendScopeGrant,
}

impl FriendRequest {
    pub fn new(
        requester: FriendPeer,
        target: FriendPeer,
        scope_grant: Option<FriendScopeGrant>,
    ) -> Result<Self, FriendRequestError> {
        let scope_grant = scope_grant.ok_or(FriendRequestError::GrantAbsent)?;
        if scope_grant.requester != requester.authority.fingerprint
            || scope_grant.target != target.authority.fingerprint
        {
            return Err(FriendRequestError::GrantPartyMismatch);
        }
        Ok(Self {
            requester,
            target,
            scope_grant,
        })
    }

    pub fn grants_scope(&self, scope: &Scope) -> bool {
        self.scope_grant.grants_scope(scope)
    }
}

impl fmt::Debug for FriendRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FriendRequest")
            .field("requester", &self.requester)
            .field("target", &self.target)
            .field("scope_grant", &self.scope_grant)
            .finish()
    }
}

/// Source category for untrusted friend-request inputs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UntrustedFriendRequestSource {
    UnsignedMetadata,
    RendererSupplied,
}

impl fmt::Debug for UntrustedFriendRequestSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsignedMetadata => f.write_str("UnsignedMetadata"),
            Self::RendererSupplied => f.write_str("RendererSupplied"),
        }
    }
}

/// A shape that may be convenient for callers to stage, but is never authority.
///
/// Even with `safety_number_verified = true`, this type cannot mint
/// [`FriendScopeGrant`]. It exists so runtime code and tests can route
/// renderer or unsigned input to an explicit refusal instead of accidentally
/// treating the claimed scope as a grant.
#[derive(Clone, PartialEq, Eq)]
pub struct UntrustedFriendRequest {
    requester_hint: String,
    target_hint: String,
    requested_scope: Scope,
    source: UntrustedFriendRequestSource,
    safety_number_verified: bool,
}

impl UntrustedFriendRequest {
    pub fn new(
        requester_hint: impl Into<String>,
        target_hint: impl Into<String>,
        requested_scope: Scope,
        source: UntrustedFriendRequestSource,
        safety_number_verified: bool,
    ) -> Self {
        Self {
            requester_hint: requester_hint.into(),
            target_hint: target_hint.into(),
            requested_scope,
            source,
            safety_number_verified,
        }
    }
}

impl TryFrom<UntrustedFriendRequest> for FriendRequest {
    type Error = FriendRequestError;

    fn try_from(_value: UntrustedFriendRequest) -> Result<Self, Self::Error> {
        Err(FriendRequestError::UnauthenticatedAuthority)
    }
}

impl fmt::Debug for UntrustedFriendRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = (&self.requester_hint, &self.target_hint);
        f.debug_struct("UntrustedFriendRequest")
            .field("requester_hint", &"<redacted>")
            .field("target_hint", &"<redacted>")
            .field("requested_scope_kind", &self.requested_scope.kind)
            .field("source", &self.source)
            .field("safety_number_verified", &self.safety_number_verified)
            .finish()
    }
}

fn fingerprint_bundle(bundle: &KeyBundle) -> FriendAuthorityFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(FRIEND_AUTHORITY_DOMAIN);
    update_component(&mut hasher, &bundle.ed25519_pub);
    update_component(&mut hasher, &bundle.x25519_pub);
    update_component(&mut hasher, &bundle.mlkem768_pub);
    match bundle.ratchet_initial_pub.as_deref() {
        Some(value) => update_component(&mut hasher, value),
        None => hasher.update(0u32.to_be_bytes()),
    }
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    FriendAuthorityFingerprint(out)
}

fn update_component(hasher: &mut Sha256, value: &str) {
    let bytes = value.as_bytes();
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    hasher.update(len.to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(label: &str) -> KeyBundle {
        KeyBundle {
            ed25519_pub: format!("{label}-ed25519"),
            x25519_pub: format!("{label}-x25519"),
            mlkem768_pub: format!("{label}-mlkem768"),
            ratchet_initial_pub: Some(format!("{label}-ratchet")),
        }
    }

    fn authority(label: &str) -> VerifiedFriendAuthority {
        VerifiedFriendAuthority::from_tofu_trusted_key_bundle(&bundle(label)).unwrap()
    }

    fn peer(authority: VerifiedFriendAuthority) -> FriendPeer {
        FriendPeer::from_authority(authority)
    }

    #[test]
    fn absence_of_scope_grant_refuses() {
        let requester = peer(authority("requester"));
        let target = peer(authority("target"));

        assert!(matches!(
            FriendRequest::new(requester, target, None),
            Err(FriendRequestError::GrantAbsent)
        ));
    }

    #[test]
    fn unsigned_or_renderer_supplied_request_cannot_grant_scope() {
        for source in [
            UntrustedFriendRequestSource::UnsignedMetadata,
            UntrustedFriendRequestSource::RendererSupplied,
        ] {
            let untrusted = UntrustedFriendRequest::new(
                "legacy-friend-code-or-osl-id",
                "renderer-target-account",
                Scope::gc("scope-a"),
                source,
                true,
            );

            assert!(matches!(
                FriendRequest::try_from(untrusted),
                Err(FriendRequestError::UnauthenticatedAuthority)
            ));
        }
    }

    #[test]
    fn request_for_scope_a_does_not_grant_scope_b() {
        let requester_authority = authority("requester");
        let target_authority = authority("target");
        let requester = peer(requester_authority.clone());
        let target = peer(target_authority.clone());
        let scope_a = Scope::server_channel("server-a", "channel-a");
        let scope_b = Scope::server_channel("server-a", "channel-b");
        let grant =
            FriendScopeGrant::new(&requester_authority, &target_authority, scope_a.clone());

        let request = FriendRequest::new(requester, target, Some(grant)).unwrap();

        assert!(request.grants_scope(&scope_a));
        assert!(!request.grants_scope(&scope_b));
    }

    #[test]
    fn grant_must_match_request_parties() {
        let requester_authority = authority("requester");
        let target_authority = authority("target");
        let other_authority = authority("other");
        let requester = peer(requester_authority.clone());
        let target = peer(target_authority.clone());
        let grant = FriendScopeGrant::new(
            &requester_authority,
            &other_authority,
            Scope::dm("target-dm"),
        );

        assert!(matches!(
            FriendRequest::new(requester, target, Some(grant)),
            Err(FriendRequestError::GrantPartyMismatch)
        ));
    }

    #[test]
    fn incomplete_trust_object_cannot_be_authority() {
        let mut incomplete = bundle("peer");
        incomplete.mlkem768_pub.clear();

        assert!(matches!(
            VerifiedFriendAuthority::from_tofu_trusted_key_bundle(&incomplete),
            Err(FriendRequestError::IncompleteTrustObject)
        ));
    }

    #[test]
    fn debug_output_redacts_account_identifiers_and_scope_ids() {
        let untrusted = UntrustedFriendRequest::new(
            "osl_0123456789abcdef",
            "900000000000000003",
            Scope::server_channel("server-secret", "channel-secret"),
            UntrustedFriendRequestSource::RendererSupplied,
            true,
        );
        let rendered = format!("{untrusted:?}");

        assert!(!rendered.contains("osl_0123456789abcdef"));
        assert!(!rendered.contains("900000000000000003"));
        assert!(!rendered.contains("server-secret"));
        assert!(!rendered.contains("channel-secret"));
        assert!(rendered.contains("ServerChannel"));
    }

    #[test]
    fn debug_output_redacts_verified_request_material() {
        let requester_authority = authority("requester-secret");
        let target_authority = authority("target-secret");
        let grant =
            FriendScopeGrant::new(&requester_authority, &target_authority, Scope::gc("scope-a"));
        let request = FriendRequest::new(
            peer(requester_authority),
            peer(target_authority),
            Some(grant),
        )
        .unwrap();
        let rendered = format!("{request:?}");

        assert!(!rendered.contains("requester-secret"));
        assert!(!rendered.contains("target-secret"));
        assert!(!rendered.contains("scope-a"));
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("Gc"));
    }

    #[test]
    fn error_display_contains_no_identifiers() {
        assert_eq!(
            FriendRequestError::UnauthenticatedAuthority.to_string(),
            "friend request authority is not authenticated"
        );
        assert_eq!(
            FriendRequestError::GrantAbsent.to_string(),
            "friend request refused"
        );
    }
}
