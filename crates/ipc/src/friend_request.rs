//! Type-only friend-request trust model.
//!
//! Friend requests fail closed: a missing consent decision, verified
//! relationship, request authority, or exact scope grant is a refusal, never an
//! implicit permission. A scope grant is a capability value, not a renderer
//! claim. It can only be minted from a complete TOFU-trusted key bundle.
//! Legacy friend-code payloads, `osl_` routing labels, unsigned metadata,
//! renderer DTOs and `safety_number_verified` booleans have no constructor path
//! into [`FriendScopeGrant`].

use crate::scope::Scope;
use crate::tofu::KeyBundle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::Path;

const FRIEND_AUTHORITY_DOMAIN: &[u8] = b"OSL-FRIEND-AUTHORITY-v1";
pub const FRIEND_REQUEST_STATE_FILE: &str = "friend_request_state.json";
const FRIEND_REQUEST_STATE_SCHEMA_VERSION: u32 = 1;

/// Errors that can reject a friend-request operation.
///
/// Variants are payload-free so formatting can never expose secrets, account
/// identifiers, handles, credentials, or local file paths. Callers that need
/// diagnostics should log their own bounded context separately from the value
/// returned to UI/IPC boundaries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendRequestError {
    /// The user has not explicitly approved the operation.
    ConsentMissing,
    /// The request is not tied to a verified relationship on this device.
    BindingMissing,
    /// The request is not backed by trusted local authority.
    AuthorityMissing,
    /// The requested conversation is not covered by an exact grant.
    ScopeNotGranted,
    /// A typed request was attempted without an authenticated scope grant.
    GrantAbsent,
    /// The request authority is not authenticated by trusted local state.
    UnauthenticatedAuthority,
    /// The grant does not bind the same requester and target as the request.
    GrantPartyMismatch,
    /// The trusted object is missing required key material.
    IncompleteTrustObject,
    /// The request is not in the pending set.
    RequestNotPending,
    /// The request's validity window has passed.
    RequestExpired,
    /// The request was previously revoked or declined.
    RequestRevoked,
    /// An equivalent pending request already exists.
    DuplicateRequest,
    /// The request shape or state transition is invalid.
    InvalidRequest,
    /// Durable friend-request state could not be read or written.
    StorageUnavailable,
}

impl FriendRequestError {
    pub const ALL: [Self; 14] = [
        Self::ConsentMissing,
        Self::BindingMissing,
        Self::AuthorityMissing,
        Self::ScopeNotGranted,
        Self::GrantAbsent,
        Self::UnauthenticatedAuthority,
        Self::GrantPartyMismatch,
        Self::IncompleteTrustObject,
        Self::RequestNotPending,
        Self::RequestExpired,
        Self::RequestRevoked,
        Self::DuplicateRequest,
        Self::InvalidRequest,
        Self::StorageUnavailable,
    ];

    /// Stable machine-readable code for IPC or persisted refusal records.
    pub const fn code(self) -> &'static str {
        match self {
            Self::ConsentMissing => "consent_missing",
            Self::BindingMissing => "binding_missing",
            Self::AuthorityMissing => "authority_missing",
            Self::ScopeNotGranted => "scope_not_granted",
            Self::GrantAbsent => "grant_absent",
            Self::UnauthenticatedAuthority => "unauthenticated_authority",
            Self::GrantPartyMismatch => "grant_party_mismatch",
            Self::IncompleteTrustObject => "incomplete_trust_object",
            Self::RequestNotPending => "request_not_pending",
            Self::RequestExpired => "request_expired",
            Self::RequestRevoked => "request_revoked",
            Self::DuplicateRequest => "duplicate_request",
            Self::InvalidRequest => "invalid_request",
            Self::StorageUnavailable => "storage_unavailable",
        }
    }

    /// Friend-request errors are fail-closed; none authorizes a scope grant.
    pub const fn grants_scope(self) -> bool {
        false
    }

    pub const fn is_missing_authority(self) -> bool {
        matches!(
            self,
            Self::ConsentMissing
                | Self::BindingMissing
                | Self::AuthorityMissing
                | Self::ScopeNotGranted
                | Self::GrantAbsent
                | Self::UnauthenticatedAuthority
                | Self::GrantPartyMismatch
                | Self::IncompleteTrustObject
        )
    }
}

impl fmt::Debug for FriendRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FriendRequestError::")?;
        f.write_str(match self {
            Self::ConsentMissing => "ConsentMissing",
            Self::BindingMissing => "BindingMissing",
            Self::AuthorityMissing => "AuthorityMissing",
            Self::ScopeNotGranted => "ScopeNotGranted",
            Self::GrantAbsent => "GrantAbsent",
            Self::UnauthenticatedAuthority => "UnauthenticatedAuthority",
            Self::GrantPartyMismatch => "GrantPartyMismatch",
            Self::IncompleteTrustObject => "IncompleteTrustObject",
            Self::RequestNotPending => "RequestNotPending",
            Self::RequestExpired => "RequestExpired",
            Self::RequestRevoked => "RequestRevoked",
            Self::DuplicateRequest => "DuplicateRequest",
            Self::InvalidRequest => "InvalidRequest",
            Self::StorageUnavailable => "StorageUnavailable",
        })
    }
}

impl fmt::Display for FriendRequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ConsentMissing => "friend request consent is missing",
            Self::BindingMissing => "friend request verification is missing",
            Self::AuthorityMissing => "friend request authority is missing",
            Self::ScopeNotGranted => "friend request scope was not granted",
            Self::GrantAbsent => "friend request refused",
            Self::UnauthenticatedAuthority => "friend request authority is not authenticated",
            Self::GrantPartyMismatch => "friend request scope grant does not match its parties",
            Self::IncompleteTrustObject => "friend request trust object is incomplete",
            Self::RequestNotPending => "friend request is not pending",
            Self::RequestExpired => "friend request expired",
            Self::RequestRevoked => "friend request was revoked",
            Self::DuplicateRequest => "friend request already exists",
            Self::InvalidRequest => "friend request is invalid",
            Self::StorageUnavailable => "friend request storage is unavailable",
        })
    }
}

impl std::error::Error for FriendRequestError {}

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredFriendRequest {
    pub request_id: String,
    pub requester_id: String,
    pub target_id: String,
    pub scope_key: String,
    pub received_at_ms: u64,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FriendRequestState {
    pub schema_version: u32,
    pub pending: Vec<StoredFriendRequest>,
    pub accepted: Vec<StoredFriendRequest>,
    pub declined_or_revoked: Vec<StoredFriendRequest>,
}

impl FriendRequestState {
    pub fn new() -> Self {
        Self {
            schema_version: FRIEND_REQUEST_STATE_SCHEMA_VERSION,
            pending: Vec::new(),
            accepted: Vec::new(),
            declined_or_revoked: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), FriendRequestError> {
        if self.schema_version != FRIEND_REQUEST_STATE_SCHEMA_VERSION {
            return Err(FriendRequestError::InvalidRequest);
        }
        for request in self
            .pending
            .iter()
            .chain(self.accepted.iter())
            .chain(self.declined_or_revoked.iter())
        {
            validate_stored_request(request)?;
        }
        Ok(())
    }
}

impl Default for FriendRequestState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn save_friend_request_state(
    dir: &Path,
    state: &FriendRequestState,
) -> Result<(), FriendRequestError> {
    state.validate()?;
    fs::create_dir_all(dir).map_err(|_| FriendRequestError::StorageUnavailable)?;
    let body = serde_json::to_vec(state).map_err(|_| FriendRequestError::StorageUnavailable)?;
    let sealed = crate::main_password::maybe_encrypt(&body)
        .map_err(|_| FriendRequestError::StorageUnavailable)?;
    let path = dir.join(FRIEND_REQUEST_STATE_FILE);
    let tmp = dir.join(format!("{FRIEND_REQUEST_STATE_FILE}.tmp"));
    fs::write(&tmp, sealed).map_err(|_| FriendRequestError::StorageUnavailable)?;
    fs::rename(&tmp, &path).map_err(|_| FriendRequestError::StorageUnavailable)?;
    Ok(())
}

pub fn load_friend_request_state(dir: &Path) -> Result<FriendRequestState, FriendRequestError> {
    let path = dir.join(FRIEND_REQUEST_STATE_FILE);
    if !path.exists() {
        return Ok(FriendRequestState::new());
    }
    let blob = fs::read(path).map_err(|_| FriendRequestError::StorageUnavailable)?;
    let plain = crate::main_password::maybe_decrypt(&blob)
        .map_err(|_| FriendRequestError::StorageUnavailable)?;
    let state: FriendRequestState =
        serde_json::from_slice(&plain).map_err(|_| FriendRequestError::StorageUnavailable)?;
    state.validate()?;
    Ok(state)
}

fn validate_stored_request(request: &StoredFriendRequest) -> Result<(), FriendRequestError> {
    if request.request_id.is_empty()
        || request.requester_id.is_empty()
        || request.target_id.is_empty()
        || Scope::parse(&request.scope_key).is_none()
        || request.expires_at_ms <= request.received_at_ms
    {
        return Err(FriendRequestError::InvalidRequest);
    }
    Ok(())
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

    fn stored_request(label: &str, scope: Scope) -> StoredFriendRequest {
        StoredFriendRequest {
            request_id: format!("request-{label}"),
            requester_id: format!("requester-{label}"),
            target_id: format!("target-{label}"),
            scope_key: scope.storage_key(),
            received_at_ms: 1000,
            expires_at_ms: 2000,
        }
    }

    #[test]
    fn stable_codes_match_wire_names() {
        for error in FriendRequestError::ALL {
            let encoded = serde_json::to_string(&error).unwrap();
            assert_eq!(encoded, format!("\"{}\"", error.code()));
            assert_eq!(
                serde_json::from_str::<FriendRequestError>(&encoded).unwrap(),
                error
            );
        }
    }

    #[test]
    fn absence_of_consent_binding_authority_or_scope_is_refusal() {
        for error in [
            FriendRequestError::ConsentMissing,
            FriendRequestError::BindingMissing,
            FriendRequestError::AuthorityMissing,
            FriendRequestError::ScopeNotGranted,
        ] {
            assert!(error.is_missing_authority());
            assert!(!error.grants_scope());
        }

        for error in FriendRequestError::ALL {
            assert!(!error.grants_scope());
        }
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
        let grant = FriendScopeGrant::new(&requester_authority, &target_authority, scope_a.clone());

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
        let grant = FriendScopeGrant::new(
            &requester_authority,
            &target_authority,
            Scope::gc("scope-a"),
        );
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

    #[test]
    fn formatting_is_payload_free_and_hides_internal_machinery() {
        let forbidden = [
            "123456789012345678",
            "alice@example.com",
            "@alice",
            "password",
            "token",
            "secret",
            "credential",
            "keyserver",
            "ratchet",
            "receipt",
            "browser profile",
            "provider adapter",
        ];

        for error in FriendRequestError::ALL {
            let debug = format!("{error:?}");
            let display = error.to_string();
            assert!(debug.starts_with("FriendRequestError::"));
            assert!(!display.is_empty());
            for term in forbidden {
                assert!(!debug.contains(term), "debug leaked {term}: {debug}");
                assert!(!display.contains(term), "display leaked {term}: {display}");
            }
        }
    }

    #[test]
    fn friend_request_state_round_trips_through_main_password_storage() {
        crate::main_password::set_file_storage_key(None);
        let dir = tempfile::tempdir().unwrap();
        let state = FriendRequestState {
            schema_version: FRIEND_REQUEST_STATE_SCHEMA_VERSION,
            pending: vec![stored_request(
                "pending",
                Scope::server_channel("server-secret-a113", "channel-secret-a113"),
            )],
            accepted: vec![stored_request("accepted", Scope::dm("peer-secret-a113"))],
            declined_or_revoked: vec![stored_request("revoked", Scope::gc("gc-secret-a113"))],
        };

        crate::main_password::set_file_storage_key(Some([0xA1; 32]));
        save_friend_request_state(dir.path(), &state).unwrap();
        let raw = fs::read(dir.path().join(FRIEND_REQUEST_STATE_FILE)).unwrap();
        assert!(crate::main_password::has_enc_magic(&raw));
        for needle in [
            b"server-secret-a113".as_slice(),
            b"channel-secret-a113".as_slice(),
            b"peer-secret-a113".as_slice(),
            b"gc-secret-a113".as_slice(),
        ] {
            assert!(
                !raw.windows(needle.len()).any(|window| window == needle),
                "friend-request state leaked a plaintext identifier to disk"
            );
        }

        crate::main_password::set_file_storage_key(None);
        assert!(matches!(
            load_friend_request_state(dir.path()),
            Err(FriendRequestError::StorageUnavailable)
        ));

        crate::main_password::set_file_storage_key(Some([0xA1; 32]));
        let loaded = load_friend_request_state(dir.path()).unwrap();
        assert_eq!(loaded, state);

        let mut tampered = state.clone();
        tampered.pending[0].scope_key = "server_channel:server-only".to_owned();
        assert!(matches!(
            save_friend_request_state(dir.path(), &tampered),
            Err(FriendRequestError::InvalidRequest)
        ));
        crate::main_password::set_file_storage_key(None);
    }
}
