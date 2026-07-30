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
use crate::secure_local_store::{RecordId, SecureLocalStore, SecureLocalStoreError};
use crate::tofu::KeyBundle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

const FRIEND_AUTHORITY_DOMAIN: &[u8] = b"OSL-FRIEND-AUTHORITY-v1";
const FRIEND_REQUEST_STATE_VERSION: u32 = 1;
const FRIEND_REQUEST_STATE_NAMESPACE: &str = "friend-request-state";
const FRIEND_REQUEST_STATE_KEY: &str = "main-password-v1";

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

/// Durable friend-request state stored only through the main-password-backed
/// secure local store.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendRequestState {
    pending: Vec<FriendRequest>,
}

impl FriendRequestState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pending(pending: Vec<FriendRequest>) -> Self {
        Self { pending }
    }

    pub fn pending_requests(&self) -> &[FriendRequest] {
        &self.pending
    }
}

#[derive(Serialize, Deserialize)]
struct StoredFriendRequestState {
    version: u32,
    pending: Vec<StoredFriendRequest>,
}

#[derive(Serialize, Deserialize)]
struct StoredFriendRequest {
    requester_fingerprint: [u8; 32],
    target_fingerprint: [u8; 32],
    scope_key: String,
}

impl From<&FriendRequestState> for StoredFriendRequestState {
    fn from(state: &FriendRequestState) -> Self {
        Self {
            version: FRIEND_REQUEST_STATE_VERSION,
            pending: state
                .pending
                .iter()
                .map(StoredFriendRequest::from)
                .collect(),
        }
    }
}

impl TryFrom<StoredFriendRequestState> for FriendRequestState {
    type Error = FriendRequestError;

    fn try_from(value: StoredFriendRequestState) -> Result<Self, Self::Error> {
        if value.version != FRIEND_REQUEST_STATE_VERSION {
            return Err(FriendRequestError::InvalidRequest);
        }
        let pending = value
            .pending
            .into_iter()
            .map(FriendRequest::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { pending })
    }
}

impl From<&FriendRequest> for StoredFriendRequest {
    fn from(request: &FriendRequest) -> Self {
        Self {
            requester_fingerprint: request.requester.authority.fingerprint.0,
            target_fingerprint: request.target.authority.fingerprint.0,
            scope_key: request.scope_grant.scope.storage_key(),
        }
    }
}

impl TryFrom<StoredFriendRequest> for FriendRequest {
    type Error = FriendRequestError;

    fn try_from(value: StoredFriendRequest) -> Result<Self, Self::Error> {
        let scope = Scope::parse(&value.scope_key).ok_or(FriendRequestError::InvalidRequest)?;
        let requester_authority = VerifiedFriendAuthority {
            fingerprint: FriendAuthorityFingerprint(value.requester_fingerprint),
        };
        let target_authority = VerifiedFriendAuthority {
            fingerprint: FriendAuthorityFingerprint(value.target_fingerprint),
        };
        let grant = FriendScopeGrant::new(&requester_authority, &target_authority, scope);
        FriendRequest::new(
            FriendPeer::from_authority(requester_authority),
            FriendPeer::from_authority(target_authority),
            Some(grant),
        )
    }
}

fn friend_request_state_record_id() -> RecordId {
    RecordId::new(FRIEND_REQUEST_STATE_NAMESPACE, FRIEND_REQUEST_STATE_KEY)
}

fn storage_error_to_friend_request(error: SecureLocalStoreError) -> FriendRequestError {
    match error {
        SecureLocalStoreError::NotFound => FriendRequestError::RequestNotPending,
        SecureLocalStoreError::NoKey
        | SecureLocalStoreError::AuthenticationFailed
        | SecureLocalStoreError::Malformed(_)
        | SecureLocalStoreError::Backend(_) => FriendRequestError::StorageUnavailable,
    }
}

pub fn save_friend_request_state(
    store: &dyn SecureLocalStore,
    state: &FriendRequestState,
) -> Result<(), FriendRequestError> {
    let stored = StoredFriendRequestState::from(state);
    let bytes = serde_json::to_vec(&stored).map_err(|_| FriendRequestError::InvalidRequest)?;
    store
        .put(&friend_request_state_record_id(), &bytes)
        .map_err(storage_error_to_friend_request)
}

pub fn load_friend_request_state(
    store: &dyn SecureLocalStore,
) -> Result<Option<FriendRequestState>, FriendRequestError> {
    let bytes = match store.get(&friend_request_state_record_id()) {
        Ok(bytes) => bytes,
        Err(SecureLocalStoreError::NotFound) => return Ok(None),
        Err(error) => return Err(storage_error_to_friend_request(error)),
    };
    let stored: StoredFriendRequestState =
        serde_json::from_slice(&bytes).map_err(|_| FriendRequestError::InvalidRequest)?;
    FriendRequestState::try_from(stored).map(Some)
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
    use crate::secure_local_store::{RawBackend, SealedStore};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    #[derive(Clone)]
    struct DiskBackend {
        root: PathBuf,
    }

    impl DiskBackend {
        fn new(root: &Path) -> Self {
            std::fs::create_dir_all(root).unwrap();
            Self {
                root: root.to_path_buf(),
            }
        }

        fn path_for(&self, storage_key: &str) -> PathBuf {
            let mut hasher = Sha256::new();
            hasher.update(storage_key.as_bytes());
            let digest = hasher.finalize();
            let mut name = String::with_capacity(digest.len() * 2);
            for byte in digest {
                use std::fmt::Write as _;
                let _ = write!(&mut name, "{byte:02x}");
            }
            self.root.join(name)
        }
    }

    impl RawBackend for DiskBackend {
        fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            std::fs::write(self.path_for(storage_key), blob)
                .map_err(|error| SecureLocalStoreError::Backend(error.to_string()))
        }

        fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            let path = self.path_for(storage_key);
            match std::fs::read(path) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(SecureLocalStoreError::Backend(error.to_string())),
            }
        }

        fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
            let path = self.path_for(storage_key);
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(SecureLocalStoreError::Backend(error.to_string())),
            }
        }
    }

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
    fn friend_request_state_round_trips_through_main_password_storage() {
        let dir = TempDir::new().expect("tempdir");
        let store = SealedStore::new([0x31; 32], DiskBackend::new(dir.path()));
        let requester_authority = authority("requester");
        let target_authority = authority("target");
        let scope_a = Scope::server_channel("server-a", "channel-a");
        let scope_b = Scope::server_channel("server-a", "channel-b");
        let request = FriendRequest::new(
            peer(requester_authority.clone()),
            peer(target_authority.clone()),
            Some(FriendScopeGrant::new(
                &requester_authority,
                &target_authority,
                scope_a.clone(),
            )),
        )
        .unwrap();
        let state = FriendRequestState::from_pending(vec![request]);

        save_friend_request_state(&store, &state).expect("save keyed friend request state");

        let persisted_files = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(persisted_files.len(), 1);
        let persisted = std::fs::read(&persisted_files[0]).unwrap();
        assert!(
            !persisted
                .windows(b"server_channel:server-a:channel-a".len())
                .any(|window| window == b"server_channel:server-a:channel-a"),
            "main-password storage must not write friend request state as plaintext"
        );

        let loaded = load_friend_request_state(&store)
            .expect("load keyed friend request state")
            .expect("state exists");
        assert_eq!(loaded.pending_requests().len(), 1);
        let loaded_request = &loaded.pending_requests()[0];
        assert!(loaded_request.grants_scope(&scope_a));
        assert!(
            !loaded_request.grants_scope(&scope_b),
            "round-tripped request must still refuse scopes outside the persisted grant"
        );

        let no_key_dir = TempDir::new().expect("no-key tempdir");
        let no_key_store = SealedStore::without_key(DiskBackend::new(no_key_dir.path()));
        assert!(matches!(
            save_friend_request_state(&no_key_store, &state),
            Err(FriendRequestError::StorageUnavailable)
        ));
        assert!(
            std::fs::read_dir(no_key_dir.path())
                .unwrap()
                .next()
                .is_none(),
            "without the main-password key, saving must refuse before writing"
        );
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
        let forbidden = [
            "123456789012345678",
            "alice@example.com",
            "@alice",
            "osl_",
            "server-secret",
            "channel-secret",
            "password",
            "token",
            "credential",
        ];

        for error in FriendRequestError::ALL {
            let display = error.to_string();
            assert!(!display.is_empty());
            for term in forbidden {
                assert!(!display.contains(term), "display leaked {term}: {display}");
            }
        }
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
}
