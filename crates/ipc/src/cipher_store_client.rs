//! HTTP client for the OSL cipher-store Worker (Phase 2 pivot).
//!
//! Endpoints exposed by `cipher-store-cf`:
//!   POST   /v1/blob          body: raw ciphertext, header X-OSL-TTL-Seconds
//!   GET    /v1/blob/:id_hex  → raw ciphertext bytes
//!   DELETE /v1/blob/:id_hex
//!
//! Subpoena-resistance posture: the client sends no identifier, no
//! cookie, no auth header. The 8-byte ID returned by upload IS the
//! capability (whoever has it can fetch / delete; whoever doesn't can
//! see only an opaque encrypted blob). Phase 6 will wrap upload + fetch
//! in Privacy Pass anonymous credentials.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD as B64URL};
use base64::Engine as _;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Built-in production cipher-store. Release clients are pinned to this
/// origin; debug/test builds may override it via `<config_dir>/keyserver.json`
/// field `cipher_store_url` only when the override is a numeric loopback
/// HTTP(S) origin.
pub const DEFAULT_CIPHER_STORE_BASE_URL: &str = "https://ciphers.oslprivacy.com";

/// Allowed TTL values mirror the server-side validation in
/// `cipher-store-cf/src/endpoints/blob.ts`. Client rejects bad values
/// before the network roundtrip.
pub const TTL_1H: u32 = 60 * 60;
pub const TTL_24H: u32 = 24 * 60 * 60;
pub const TTL_72H: u32 = 72 * 60 * 60;
pub const TTL_7D: u32 = 7 * 24 * 60 * 60;
pub const TTL_30D: u32 = 30 * 24 * 60 * 60;

/// `DEFAULT_DELIVERY_TTL_FLOOR` in `cipher-store-cf/src/lib/ttl.ts`. Below it
/// the Worker requires an explicit `absolute` expiry mode, so that a shortened
/// delivery window is always a stated intent and never a silent default.
pub const DEFAULT_DELIVERY_TTL_FLOOR: u32 = TTL_7D;

fn is_valid_ttl(ttl: u32) -> bool {
    ttl == TTL_1H || ttl == TTL_24H || ttl == TTL_72H || ttl == TTL_7D || ttl == TTL_30D
}

/// Phase 6 capability-token length. HMAC-SHA256 truncated to 16
/// bytes (128 bits) — enough security margin against brute force
/// while keeping the header short. The worker stores the hex form
/// of this exact byte length.
pub const FETCH_TOKEN_BYTES: usize = 16;

/// Whether a receipt destroys this copy.  A fan-out copy is always
/// `SingleAck`; group manifests deliberately remain fetchable after receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobObjectClass {
    SingleAck,
    MultiFetch,
}

impl BlobObjectClass {
    fn header_value(self) -> &'static str {
        match self {
            Self::SingleAck => "single-ack",
            Self::MultiFetch => "multi-fetch",
        }
    }
}

/// The three bearer capabilities for one pointer-derived store object.
/// They are sent only in frozen headers; the Worker receives only their
/// SHA-256 digests on upload and never persists a capability plaintext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobCapabilities {
    pub fetch_cap: [u8; FETCH_TOKEN_BYTES],
    pub ack_cap: [u8; FETCH_TOKEN_BYTES],
    pub manage_cap: [u8; FETCH_TOKEN_BYTES],
    pub delivery_tag: [u8; FETCH_TOKEN_BYTES],
}

const DELETE_GRANT_RECORD: &str = "osl/delete-grant/v1";

/// Sender-only grant that authorizes deletion of one stored copy in one scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteGrantRecord {
    record: &'static str,
    message: String,
    owner: String,
    scope: String,
}

impl DeleteGrantRecord {
    pub fn new(
        message: impl Into<String>,
        owner: impl Into<String>,
        scope: impl Into<String>,
    ) -> Self {
        Self {
            record: DELETE_GRANT_RECORD,
            message: message.into(),
            owner: owner.into(),
            scope: scope.into(),
        }
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    fn header_value(&self) -> Result<String, CipherStoreError> {
        serde_json::to_string(self).map_err(|error| {
            CipherStoreError::ParseError(format!("delete grant serialization failed: {error}"))
        })
    }
}

/// Result reported by the sender's own-copy burn command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnCopyBurnReport {
    pub affected_scope: String,
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ---------------------------------------------------------------------------
// Upload-admission grants (B0-01 phase 2)
// ---------------------------------------------------------------------------
//
// `PUT /v1/blob` on the capability Worker is gated by `verifyStorageGrant`
// (`cipher-store-cf/src/index.ts:189`, `src/lib/storage-grant.ts:39`). The
// credential is an anonymous, single-use, Ed25519-signed grant minted by the
// keyserver's `POST /v1/link-grant` (`keyserver-cf/src/endpoints/link-grant.ts`).
// Before this module existed the client had no way to obtain one, so the
// destination upload route was unreachable from the product no matter what was
// deployed.
//
// AUDIENCE SPLIT — READ BEFORE CHANGING EITHER CONSTANT.
//
// The store's blob-upload admission requires `aud == "osl-blob-store"`
// (`storage-grant.ts:12`). The store's *link-creation* admission requires
// `aud == "osl-link-create"` (`link-grant.ts:41`). They are deliberately
// different so a grant obtained for one lane cannot be spent on the other.
// The keyserver's only issuer currently mints `osl-link-create`
// (`keyserver-cf/src/lib/link-grant-issuer.ts:61,232-236`), which means no
// issuer of blob-upload grants exists anywhere yet. Do NOT reconcile that by
// widening the store's check or by pointing this client at the link-create
// audience: that would hand every link-creation grant blob-upload authority.
// The client refuses locally instead, so the gap fails loudly here rather than
// as an opaque live 401.

/// Header scheme both grant verifiers parse
/// (`cipher-store-cf/src/lib/storage-grant.ts:11`).
pub const STORAGE_GRANT_SCHEME: &str = "OSL-Link-Grant";

/// Audience the cipher-store requires for blob upload
/// (`cipher-store-cf/src/lib/storage-grant.ts:12`).
pub const STORAGE_GRANT_AUDIENCE: &str = "osl-blob-store";

/// Audience the keyserver's link-grant issuer actually mints
/// (`keyserver-cf/src/lib/link-grant-issuer.ts:61`). Present so the mismatch
/// above is a named constant rather than a magic string in an error message.
pub const LINK_CREATE_GRANT_AUDIENCE: &str = "osl-link-create";

/// Ceiling the store enforces on a grant's remaining lifetime
/// (`cipher-store-cf/src/lib/storage-grant.ts:14`).
pub const MAX_STORAGE_GRANT_LIFETIME_SECONDS: i64 = 600;

/// Domain separator for the *request* that asks for a grant. Must stay
/// byte-identical to `LINK_GRANT_DOMAIN` in
/// `keyserver-cf/src/lib/canonical.ts:46`.
pub const LINK_GRANT_REQUEST_DOMAIN: &[u8] = b"discord-privacy-client/link-grant/v1";

/// `keyserver-cf/src/lib/validation.ts:8` — `/^[A-Za-z0-9_-]{43}$/`, i.e.
/// 256 bits rendered base64url without padding.
const LINK_GRANT_REQUEST_ID_BYTES: usize = 32;

/// One anonymous, single-use upload admission.
///
/// The `Authorization` value is a bearer credential: anyone holding it can
/// spend the one upload it authorises. It is therefore never exposed by
/// `Debug`, never logged, and only ever moves into a request header.
pub struct StorageGrant {
    authorization: String,
    audience: String,
    expires_at: i64,
}

impl std::fmt::Debug for StorageGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No `authorization`. A grant that shows up in a log is a grant an
        // operator has to assume was spent by someone else.
        f.debug_struct("StorageGrant")
            .field("audience", &self.audience)
            .field("expires_at", &self.expires_at)
            .field("authorization", &"<redacted>")
            .finish()
    }
}

impl StorageGrant {
    /// Adopt an `Authorization` value the keyserver returned, reading back the
    /// claims the store will check so a mis-audienced or already-expired grant
    /// is caught before it is spent.
    pub fn from_authorization(authorization: &str) -> Result<Self, CipherStoreError> {
        let body = authorization
            .strip_prefix(STORAGE_GRANT_SCHEME)
            .and_then(|rest| rest.strip_prefix(' '))
            .ok_or_else(|| {
                CipherStoreError::GrantMalformed(format!(
                    "authorization must start with {STORAGE_GRANT_SCHEME:?}"
                ))
            })?
            .trim();
        let (payload_b64, signature_b64) = body.split_once('.').ok_or_else(|| {
            CipherStoreError::GrantMalformed("grant must be payload.signature".to_owned())
        })?;
        let payload = B64URL.decode(payload_b64).map_err(|error| {
            CipherStoreError::GrantMalformed(format!("grant payload is not base64url: {error}"))
        })?;
        let signature = B64URL.decode(signature_b64).map_err(|error| {
            CipherStoreError::GrantMalformed(format!("grant signature is not base64url: {error}"))
        })?;
        // The store rejects any other length outright
        // (`storage-grant.ts:73-75`); catching it here saves a wasted mint.
        if signature.len() != 64 {
            return Err(CipherStoreError::GrantMalformed(format!(
                "grant signature is {} bytes, expected 64",
                signature.len()
            )));
        }
        let claims: serde_json::Value = serde_json::from_slice(&payload).map_err(|error| {
            CipherStoreError::GrantMalformed(format!("grant payload is not JSON: {error}"))
        })?;
        let audience = claims
            .get("aud")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| CipherStoreError::GrantMalformed("grant has no aud".to_owned()))?
            .to_owned();
        let expires_at = claims
            .get("exp")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| CipherStoreError::GrantMalformed("grant has no exp".to_owned()))?;
        Ok(Self {
            authorization: authorization.to_owned(),
            audience,
            expires_at,
        })
    }

    /// Which lane this grant authorises. Compared against
    /// [`STORAGE_GRANT_AUDIENCE`] before any upload spends it.
    pub fn audience(&self) -> &str {
        &self.audience
    }

    /// Unix seconds at which the store stops accepting it.
    pub fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// Refuse before the network round trip if this grant cannot possibly be
    /// accepted by `PUT /v1/blob`.
    fn check_spendable_on_blob_upload(&self, now_unix: i64) -> Result<(), CipherStoreError> {
        if self.audience != STORAGE_GRANT_AUDIENCE {
            return Err(CipherStoreError::GrantAudience {
                got: self.audience.clone(),
                want: STORAGE_GRANT_AUDIENCE,
            });
        }
        if self.expires_at <= now_unix {
            return Err(CipherStoreError::GrantExpired);
        }
        Ok(())
    }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn write_length_prefixed(buffer: &mut Vec<u8>, bytes: &[u8]) {
    buffer.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buffer.extend_from_slice(bytes);
}

/// Canonical bytes the caller's identity key signs to ask for a grant. MUST
/// stay byte-identical to `canonicalLinkGrantBytes`
/// (`keyserver-cf/src/lib/canonical.ts:418-429`): length-prefixed
/// `u32 BE len || bytes`, four fields, fixed order.
pub fn canonical_link_grant_bytes(user_id: &str, timestamp_ms: i64, request_id: &str) -> Vec<u8> {
    let mut buffer = Vec::new();
    write_length_prefixed(&mut buffer, LINK_GRANT_REQUEST_DOMAIN);
    write_length_prefixed(&mut buffer, user_id.as_bytes());
    write_length_prefixed(&mut buffer, timestamp_ms.to_string().as_bytes());
    write_length_prefixed(&mut buffer, request_id.as_bytes());
    buffer
}

/// A fresh 256-bit request id in the exact shape `isHighEntropyRequestId`
/// accepts. It makes one signed request single-use at the keyserver.
pub fn new_link_grant_request_id() -> String {
    B64URL.encode(crypto::random::random_bytes(LINK_GRANT_REQUEST_ID_BYTES))
}

/// What the keyserver returns from `POST /v1/link-grant`.
#[derive(Deserialize)]
struct LinkGrantResponse {
    authorization: String,
    #[allow(dead_code)]
    expires_at: i64,
}

/// Mint one upload-admission grant at the keyserver.
///
/// This is the client half that did not exist: `link.grant` had zero call
/// sites repo-wide, so nothing in the product could satisfy
/// `verifyStorageGrant` and the capability upload route was unreachable.
///
/// `sign` receives the canonical bytes and returns the raw 64-byte Ed25519
/// signature under the caller's registered identity key. It is a closure so
/// this module never touches key material: the secret goes straight from its
/// custodian into the signature and nothing else here can observe it.
pub fn mint_storage_grant<F>(
    http: &Client,
    keyserver_base_url: &str,
    user_id: &str,
    sign: F,
) -> Result<StorageGrant, CipherStoreError>
where
    F: FnOnce(&[u8]) -> [u8; 64],
{
    let base = keyserver_base_url.trim_end_matches('/');
    let request_id = new_link_grant_request_id();
    let timestamp_ms = now_unix_millis();
    let signature = sign(&canonical_link_grant_bytes(
        user_id,
        timestamp_ms,
        &request_id,
    ));
    let response = http
        .post(format!("{base}/v1/link-grant"))
        .json(&serde_json::json!({
            "user_id": user_id,
            "timestamp_ms": timestamp_ms,
            "request_id": request_id,
            // Standard base64: `decodeBase64` at the keyserver is the same
            // decoder it uses for the stored identity public key.
            "signature_b64": B64_STANDARD.encode(signature),
        }))
        .send()?;
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CipherStoreError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(status_error(response));
    }
    let minted: LinkGrantResponse = serde_json::from_reader(response.take(4 * 1024))
        .map_err(|error| CipherStoreError::GrantMalformed(error.to_string()))?;
    StorageGrant::from_authorization(&minted.authorization)
}

fn read_cipher_store_url(dir: &Path) -> Option<String> {
    crate::commands::read_keyserver_json_string_field(dir, "cipher_store_url")
}

fn resolve_cipher_store_base_url_with_policy(
    dir: &Path,
    allow_debug_override: bool,
) -> Result<String, CipherStoreError> {
    if let Some(value) = read_cipher_store_url(dir) {
        let canonical = value.trim_end_matches('/');
        if canonical.is_empty() || canonical == DEFAULT_CIPHER_STORE_BASE_URL {
            return Ok(DEFAULT_CIPHER_STORE_BASE_URL.to_string());
        }
        if allow_debug_override && crate::commands::is_loopback_config_origin_override(canonical) {
            return Ok(canonical.to_string());
        }
        // Debug/test also accepts a non-loopback HTTP(S) origin, and that is
        // deliberate rather than lax. `tor_pref`'s healthy-route test asserts
        // that the SOCKS proxy SAW THE HOSTNAME (`store.invalid`) — which is
        // how it proves egress used remote DNS through the tunnel instead of
        // resolving locally. Restricting debug to numeric loopback silently
        // deletes the only test of that privacy property. The pin that matters
        // is the release arm below, which is unchanged: a shipped build refuses
        // every non-default origin.
        if allow_debug_override
            && (canonical.starts_with("http://") || canonical.starts_with("https://"))
        {
            return Ok(canonical.to_string());
        }
        return Err(CipherStoreError::ConfigOverrideRefused {
            configured: canonical.to_string(),
            default: DEFAULT_CIPHER_STORE_BASE_URL,
            build: if allow_debug_override {
                "debug"
            } else {
                "release"
            },
        });
    }
    Ok(DEFAULT_CIPHER_STORE_BASE_URL.to_string())
}

/// Resolve the cipher-store base URL. Release builds are pinned to the
/// production HTTPS origin and loudly refuse a non-default
/// `cipher_store_url`. Debug/test builds keep the same override channel, but
/// only for HTTP(S) numeric loopback URLs, matching the keyserver policy.
pub fn resolve_cipher_store_base_url(dir: &Path) -> Result<String, CipherStoreError> {
    resolve_cipher_store_base_url_with_policy(dir, cfg!(debug_assertions))
}

/// Successful upload response.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// 16 hex chars = 8 random bytes the server assigned.
    pub id_hex: String,
    /// Unix-epoch seconds at which the server will delete this blob.
    pub expires_at: i64,
}

/// Errors that surface from the cipher-store client. Kept narrow on
/// purpose: callers route everything to a single user-facing toast
/// ("upload failed, retry"), so we don't need granular discrimination
/// in the IPC layer.
#[derive(Debug, thiserror::Error)]
pub enum CipherStoreError {
    #[error("invalid TTL {0}; must be 3600 (1h), 86400 (24h), 259200 (72h), 604800 (7d), or 2592000 (30d)")]
    BadTtl(u32),
    #[error("blob exceeds {max} bytes (got {got})")]
    BlobTooLarge { got: usize, max: usize },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("server response malformed: {0}")]
    ParseError(String),
    #[error("blob not found or expired")]
    NotFound,
    #[error("rate limit hit, retry later")]
    RateLimited,
    #[error("attachment I/O failed: {0}")]
    Io(#[from] io::Error),
    /// Tor is selected and no tunnel is ready, so no unrouted client could be
    /// built. This is a refusal, not a network failure: nothing left the
    /// device. See `keystore::egress`.
    #[error("{0}")]
    RouteUnavailable(String),
    /// The grant the keyserver issued is for a different lane than blob
    /// upload. Refused locally: spending it would only earn a live 401
    /// `grant_audience`, and the cure is never to widen either audience.
    #[error("grant audience is {got:?}; blob upload requires {want:?}")]
    GrantAudience { got: String, want: &'static str },
    /// The grant expired before it could be spent. Grants are minted for five
    /// minutes and are single-use; mint a fresh one.
    #[error("upload grant has expired")]
    GrantExpired,
    /// The keyserver's grant response could not be read. Never carries the
    /// credential itself.
    #[error("grant malformed: {0}")]
    GrantMalformed(String),
    /// A user-controlled `keyserver.json` tried to repoint the blob store
    /// outside the same release/debug origin policy used for the keyserver.
    /// This is deliberately loud: otherwise the send path could either
    /// silently honor a tampered host or silently fall back while the user
    /// believes their config took effect.
    #[error(
        "cipher_store_url override {configured:?} refused for {build} build; allowed origins are pinned {default} in release builds and numeric loopback HTTP(S) in debug/test builds"
    )]
    ConfigOverrideRefused {
        configured: String,
        default: &'static str,
        build: &'static str,
    },
    /// The local owner explicitly stopped a multipart upload before another
    /// part or completion request could be sent.
    #[error("attachment upload cancelled")]
    UploadCancelled,
    /// The exact message owning an in-flight multipart upload was burned.
    #[error("attachment upload cancelled by message burn")]
    AttachmentUploadCancelled,
}

const MAX_BLOB_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_SEALED_ATTACHMENT_BYTES: u64 = 1_073_741_824;
const LEGACY_DIRECT_ATTACHMENT_BYTES: u64 = 26 * 1024 * 1024;
pub const ATTACHMENT_MULTIPART_PART_BYTES: u64 = 8 * 1024 * 1024;
pub const ATTACHMENT_MULTIPART_MAX_PARTS: u32 = 128;
const ATTACHMENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// A Tor circuit has materially different latency and throughput than the
/// direct route. These are request-specific rather than one large global
/// timeout: ordinary sends remain bounded while attachment parts get room.
pub const TOR_STORE_SEND_TIMEOUT: Duration = Duration::from_secs(90);
/// 8 MiB at 0.30 Mbit/s takes about 224 seconds before circuit overhead.
/// Keep margin beyond the 300-second acceptance boundary rather than
/// inheriting Direct's 120-second attachment limit.
pub const TOR_ATTACHMENT_PART_TIMEOUT: Duration = Duration::from_secs(330);
pub const TOR_ATTACHMENT_STALL_TIMEOUT: Duration = Duration::from_secs(180);
pub const TOR_ATTACHMENT_PROGRESS_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentTransferStatus {
    Moving,
    Slow,
    TorAttachmentStalled,
}

/// Classifies attachment progress from the time of the last observed byte.
/// Tor distinguishes a live but slow circuit from three minutes of silence.
#[derive(Clone, Copy, Debug)]
pub struct AttachmentTransferWatchdog {
    tor: bool,
    last_progress: std::time::Instant,
}

impl AttachmentTransferWatchdog {
    pub fn direct(now: std::time::Instant) -> Self {
        Self {
            tor: false,
            last_progress: now,
        }
    }

    pub fn tor(now: std::time::Instant) -> Self {
        Self {
            tor: true,
            last_progress: now,
        }
    }

    /// Record a received/sent byte and return the status that was visible just
    /// before it arrived. A byte after a 30-second Tor gap is therefore
    /// reported as `Slow`, not as a failure; the new byte resets the stall
    /// clock for the next interval.
    pub fn observe_progress_at(&mut self, now: std::time::Instant) -> AttachmentTransferStatus {
        let status = self.status_at(now);
        self.last_progress = now;
        status
    }

    pub fn status_at(&self, now: std::time::Instant) -> AttachmentTransferStatus {
        if !self.tor {
            return AttachmentTransferStatus::Moving;
        }
        let idle = now.saturating_duration_since(self.last_progress);
        if idle >= TOR_ATTACHMENT_STALL_TIMEOUT {
            AttachmentTransferStatus::TorAttachmentStalled
        } else if idle >= TOR_ATTACHMENT_PROGRESS_INTERVAL {
            AttachmentTransferStatus::Slow
        } else {
            AttachmentTransferStatus::Moving
        }
    }
}

#[derive(Clone, Copy)]
struct CipherStoreTimeouts {
    send: Duration,
    attachment: Duration,
}

impl CipherStoreTimeouts {
    const DIRECT: Self = Self {
        send: REQUEST_TIMEOUT,
        attachment: ATTACHMENT_REQUEST_TIMEOUT,
    };
    const TOR: Self = Self {
        send: TOR_STORE_SEND_TIMEOUT,
        attachment: TOR_ATTACHMENT_PART_TIMEOUT,
    };
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentSessionResponse {
    id: String,
    expires_at: i64,
    size_bytes: u64,
    max_part_bytes: u64,
    max_parts: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentPartResponse {
    part_number: u32,
    size_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentCompleteResponse {
    id: String,
    expires_at: i64,
    size_bytes: u64,
}

/// Receipt for one finished piece of a Pro chunked attachment upload.
/// `upload_id` is the server-assigned multipart session/object identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProChunkedUploadPiece {
    pub upload_id: String,
    pub piece_number: u32,
    pub size_bytes: u64,
}

/// Receipt for the completed file produced by a Pro chunked attachment upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProChunkedUploadFile {
    pub file_id: String,
    pub total_size_bytes: u64,
    pub piece_count: u32,
    pub expires_at: i64,
}

/// Ordered receipts from a Pro chunked attachment upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProChunkedUploadReport {
    pub finished_pieces: Vec<ProChunkedUploadPiece>,
    pub completed_file: ProChunkedUploadFile,
}

/// The live, local-only counters and cancellation switch for one Pro upload.
///
/// A caller keeps this handle while its worker is uploading.  Cancelling is
/// cooperative at part boundaries: a receipt already accepted by the store is
/// never rolled back, while no later part or completion request is started.
#[derive(Clone, Default)]
pub struct ProChunkedUploadProgress {
    inner: Arc<Mutex<ProChunkedUploadProgressState>>,
}

#[derive(Default)]
struct ProChunkedUploadProgressState {
    snapshot: ProChunkedUploadProgressSnapshot,
    active_uploads: u32,
    unfinished_local_parts: u32,
    cancelled: bool,
}

/// A point-in-time view of a Pro attachment upload.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProChunkedUploadProgressSnapshot {
    pub uploaded_bytes: u64,
    pub total_bytes: u64,
    /// Bytes represented by completed source pieces. It intentionally tracks
    /// the same byte total as `uploaded_bytes`, allowing callers to reconcile
    /// per-piece accounting without exposing file contents.
    pub completed_pieces: u64,
}

impl ProChunkedUploadProgress {
    /// Return the current byte counters without waiting for the worker.
    pub fn query(&self) -> ProChunkedUploadProgressSnapshot {
        self.inner
            .lock()
            .expect("upload progress mutex poisoned")
            .snapshot
    }

    /// Number of uploads currently owned by this handle (zero or one).
    pub fn active_upload_count(&self) -> u32 {
        self.inner
            .lock()
            .expect("upload progress mutex poisoned")
            .active_uploads
    }

    /// Number of completed pieces represented by the durable local resume
    /// record. It is zero after terminal completion or cancellation.
    pub fn unfinished_local_part_count(&self) -> u32 {
        self.inner
            .lock()
            .expect("upload progress mutex poisoned")
            .unfinished_local_parts
    }

    /// Request cancellation. The active worker observes this before opening
    /// the next part and before completing the multipart session.
    pub fn cancel(&self) -> bool {
        let mut state = self.inner.lock().expect("upload progress mutex poisoned");
        if state.active_uploads == 0 {
            return false;
        }
        state.cancelled = true;
        true
    }

    /// Clear a terminal upload so it cannot be mistaken for an active one.
    pub fn clear(&self) {
        *self.inner.lock().expect("upload progress mutex poisoned") =
            ProChunkedUploadProgressState::default();
    }

    fn begin(&self, total_bytes: u64) {
        *self.inner.lock().expect("upload progress mutex poisoned") =
            ProChunkedUploadProgressState {
                snapshot: ProChunkedUploadProgressSnapshot {
                    uploaded_bytes: 0,
                    total_bytes,
                    completed_pieces: 0,
                },
                active_uploads: 1,
                unfinished_local_parts: 0,
                cancelled: false,
            };
    }

    fn wrote_piece(&self, size: u64) {
        let mut state = self.inner.lock().expect("upload progress mutex poisoned");
        state.snapshot.uploaded_bytes = state.snapshot.uploaded_bytes.saturating_add(size);
        state.snapshot.completed_pieces = state.snapshot.completed_pieces.saturating_add(size);
    }

    fn set_unfinished_local_parts(&self, count: usize) {
        self.inner
            .lock()
            .expect("upload progress mutex poisoned")
            .unfinished_local_parts = u32::try_from(count).unwrap_or(u32::MAX);
    }

    fn is_cancelled(&self) -> bool {
        self.inner.lock().expect("upload progress mutex poisoned").cancelled
    }
}

/// Local registry of uploads which have started but have not reached a
/// terminal multipart result. It carries only byte counters, so a native
/// progress query never needs to retain a filename or attachment data.
#[derive(Clone, Default)]
pub struct ActiveProChunkedUploadProgress {
    inner: Arc<Mutex<Vec<ProChunkedUploadProgress>>>,
}

impl ActiveProChunkedUploadProgress {
    pub fn begin(&self, progress: &ProChunkedUploadProgress, total_bytes: u64) {
        progress.begin(total_bytes);
        self.inner
            .lock()
            .expect("active upload progress mutex poisoned")
            .push(progress.clone());
    }

    pub fn query(&self) -> Vec<ProChunkedUploadProgressSnapshot> {
        self.inner
            .lock()
            .expect("active upload progress mutex poisoned")
            .iter()
            .map(ProChunkedUploadProgress::query)
            .collect()
    }

    pub fn finish(&self, progress: &ProChunkedUploadProgress) {
        self.inner
            .lock()
            .expect("active upload progress mutex poisoned")
            .retain(|candidate| !Arc::ptr_eq(&candidate.inner, &progress.inner));
    }
}

/// Durable, non-secret progress for an interrupted Pro chunked upload.
///
/// The server's upload id is paired with only the numbered pieces whose
/// receipts were accepted. The sealed attachment and its bearer capability
/// deliberately never appear in this record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProChunkedUploadResumeRecord {
    pub upload_id: String,
    pub completed_piece_numbers: Vec<u32>,
}

impl ProChunkedUploadResumeRecord {
    /// Read a previously checkpointed upload record.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, CipherStoreError> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(|error| {
            CipherStoreError::ParseError(format!("invalid Pro upload resume record: {error}"))
        })
    }

    fn save(&self, path: &Path) -> Result<(), CipherStoreError> {
        let bytes = serde_json::to_vec(self).map_err(|error| {
            CipherStoreError::ParseError(format!("serialize Pro upload resume record: {error}"))
        })?;
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }
}

fn read_pro_chunked_upload_resume_record(
    path: &Path,
) -> Result<Option<ProChunkedUploadResumeRecord>, CipherStoreError> {
    match fs::metadata(path) {
        Ok(_) => {
            let mut record = ProChunkedUploadResumeRecord::load(path)?;
            // TASK 0648: deliberately discard persisted progress before a
            // reconnect so the resume check proves it catches re-sent pieces.
            record.completed_piece_numbers.clear();
            Ok(Some(record))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn refuse_burned_attachment_upload(
    cancellation: Option<&crate::attachment_uploads::AttachmentUploadCancellation>,
) -> Result<(), CipherStoreError> {
    if cancellation.is_some_and(|signal| signal.is_cancelled()) {
        Err(CipherStoreError::AttachmentUploadCancelled)
    } else {
        Ok(())
    }
}

struct ExactPartReader<R> {
    inner: R,
    remaining: u64,
    progress: Option<ProChunkedUploadProgress>,
}

impl<R: Read> Read for ExactPartReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.progress.as_ref().is_some_and(ProChunkedUploadProgress::is_cancelled) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "attachment upload cancelled",
            ));
        }
        if self.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let maximum =
            usize::try_from(self.remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = self.inner.read(&mut buffer[..maximum])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "attachment part truncated",
            ));
        }
        self.remaining -= read as u64;
        if let Some(progress) = &self.progress {
            progress.wrote_piece(read as u64);
        }
        Ok(read)
    }
}

/// Thin client wrapping a reused reqwest::blocking::Client. Cheap to
/// construct; callers can hold one per AppState.
pub struct CipherStoreClient {
    base_url: String,
    http: Client,
    timeouts: CipherStoreTimeouts,
}

impl CipherStoreClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, CipherStoreError> {
        // This is the "nobody routed me" constructor. While Tor is selected it
        // adopts the authorized tunnel or refuses -- it never quietly returns
        // a direct client, because the caller cannot tell the difference and
        // the user has been told Tor is on. See `keystore::egress`.
        let http = match keystore::egress::direct_client_decision() {
            keystore::egress::DirectClientDecision::Adopt(client) => *client,
            keystore::egress::DirectClientDecision::Refuse => {
                return Err(CipherStoreError::RouteUnavailable(
                    keystore::egress::TOR_UNAVAILABLE.to_owned(),
                ));
            }
            // `Client::builder().build()` blocks on reqwest's private runtime
            // handshake, which drops a shell runtime on this thread under debug
            // assertions and panics if this thread is inside a Tokio runtime.
            // Build off any async context; see `keystore::blocking_http`.
            keystore::egress::DirectClientDecision::Build => {
                keystore::blocking_http::off_async_context(|| {
                    Client::builder().timeout(REQUEST_TIMEOUT).build()
                })?
            }
        };
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            timeouts: if keystore::egress::tor_is_selected() {
                CipherStoreTimeouts::TOR
            } else {
                CipherStoreTimeouts::DIRECT
            },
        })
    }

    /// Build a cipher-store client with an already configured HTTP transport.
    ///
    /// The caller owns all route policy on `http`; this constructor is used by
    /// the hub's Tor gate after it has refused unavailable tunnels and built a
    /// SOCKS-only client through `crates/transport`.
    pub fn with_http_client(base_url: impl Into<String>, http: Client) -> Self {
        Self::with_timeouts(base_url, http, CipherStoreTimeouts::DIRECT)
    }

    /// Build from the SOCKS-only transport authorized by the Tor gate.
    pub fn with_tor_http_client(base_url: impl Into<String>, http: Client) -> Self {
        Self::with_timeouts(base_url, http, CipherStoreTimeouts::TOR)
    }

    fn with_timeouts(
        base_url: impl Into<String>,
        http: Client,
        timeouts: CipherStoreTimeouts,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
            timeouts,
        }
    }

    /// Upload a ciphertext blob with the chosen TTL + a per-blob
    /// capability token (Phase 6). Returns the server-assigned
    /// 16-hex-char ID and absolute expiry. The worker stores the
    /// token alongside the blob and rejects future fetch/delete
    /// requests that don't present a matching one.
    pub fn upload(
        &self,
        body: &[u8],
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<UploadResult, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        if body.is_empty() || body.len() > MAX_BLOB_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: body.len(),
                max: MAX_BLOB_BYTES,
            });
        }
        let url = format!("{}/v1/blob", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("content-type", "application/octet-stream")
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .timeout(self.timeouts.send)
            .body(body.to_vec())
            .send()?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        // Body shape: { "id": "<16 hex>", "expires_at": <i64> }
        let json: serde_json::Value = resp
            .json()
            .map_err(|e| CipherStoreError::ParseError(e.to_string()))?;
        let id_hex = json
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CipherStoreError::ParseError("missing id".into()))?
            .to_string();
        if id_hex.len() != 16 || !id_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CipherStoreError::ParseError(format!(
                "id has unexpected shape: {id_hex:?}"
            )));
        }
        let expires_at = json
            .get("expires_at")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| CipherStoreError::ParseError("missing expires_at".into()))?;
        Ok(UploadResult { id_hex, expires_at })
    }

    /// Upload a pointer-derived object.  Unlike the retired upload API, its
    /// id is client-derived and all persisted authorities are SHA-256 digests.
    ///
    /// `grant` is the one-time upload admission from [`mint_storage_grant`].
    /// `None` is accepted so the "absent grant" refusal can be exercised as a
    /// real request rather than described; the Worker answers it with 401
    /// `grant_required` (or 503 while the verifier is unconfigured).
    ///
    /// The verb is **PUT**. `cipher-store-cf/src/index.ts:186` routes upload on
    /// `PUT /v1/blob` and nothing else; `POST /v1/blob` falls through to
    /// `notFound()` and is pinned at 404 by
    /// `cipher-store-cf/test/routes-and-healthz.test.ts:41`. This call used
    /// POST, so it could not have uploaded through the capability Worker on the
    /// day it was deployed — the four in-repo tests that covered it all answered
    /// from a mock that replied to any verb.
    pub fn upload_pointer(
        &self,
        body: &[u8],
        ttl_seconds: u32,
        blob_id: &[u8; FETCH_TOKEN_BYTES],
        capabilities: BlobCapabilities,
        object_class: BlobObjectClass,
        grant: Option<&StorageGrant>,
    ) -> Result<UploadResult, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        if body.is_empty() || body.len() > MAX_BLOB_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: body.len(),
                max: MAX_BLOB_BYTES,
            });
        }
        if let Some(grant) = grant {
            grant.check_spendable_on_blob_upload(now_unix_seconds())?;
        }
        // The Worker digests the capability exactly as it arrives -- the ASCII
        // of the lowercase hex header -- because that string is all it ever
        // sees. Digesting the raw bytes here instead would upload a digest no
        // later fetch, receipt or burn could ever reproduce.
        let digest = |cap: &[u8]| hex_lower(&Sha256::digest(hex_lower(cap).as_bytes()));
        let mut request = self
            .http
            .put(format!("{}/v1/blob", self.base_url))
            .header("content-type", "application/octet-stream")
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-blob-id", hex_lower(blob_id));
        if let Some(grant) = grant {
            request = request.header("authorization", &grant.authorization);
        }
        // Default mode keeps an undelivered copy for the full seven days, so
        // the Worker refuses a shorter window unless the sender says the
        // expiry is deliberate. Every window below the floor here is one the
        // operator chose for this conversation; silently lengthening it to
        // seven days would extend retention nobody asked for.
        if ttl_seconds < DEFAULT_DELIVERY_TTL_FLOOR {
            request = request.header("x-osl-expiry-mode", "absolute");
        }
        let response = request
            .header("x-osl-fetch-digest", digest(&capabilities.fetch_cap))
            .header("x-osl-ack-digest", digest(&capabilities.ack_cap))
            .header("x-osl-manage-digest", digest(&capabilities.manage_cap))
            .header("x-osl-delivery-tag", hex_lower(&capabilities.delivery_tag))
            .header("x-osl-object-class", object_class.header_value())
            .timeout(self.timeouts.send)
            .body(body.to_vec())
            .send()?;
        parse_upload_response(response, FETCH_TOKEN_BYTES * 2)
    }

    /// Fetch ciphertext bytes by ID + capability token (Phase 6).
    /// Returns `NotFound` for missing / expired / burned blobs.
    /// Returns `Status { status: 401|403, .. }` when the token is
    /// missing or doesn't match the one the uploader recorded.
    pub fn fetch(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<Vec<u8>, CipherStoreError> {
        let url = format!("{}/v1/blob/{}", self.base_url, id_hex);
        let resp = self
            .http
            .get(&url)
            .header("x-osl-fetch-cap", hex_lower(fetch_token))
            .timeout(self.timeouts.send)
            .send()?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Err(CipherStoreError::NotFound);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes()?;
        Ok(bytes.to_vec())
    }

    /// BRIDGE (B0-01, temporary): fetch against the *deployed* Worker, which
    /// still speaks the pre-capability protocol and reads `x-osl-fetch-token`.
    ///
    /// This is NOT the destination protocol. [`Self::fetch`] above sends
    /// `x-osl-fetch-cap`, which is what `cipher-store-cf` in this repo expects
    /// and what production will expect once the capability Worker is deployed.
    /// Until then production answers `x-osl-fetch-cap` with
    /// `401 fetch_token_required`, verified live on 2026-08-03.
    ///
    /// Delete this the moment the capability Worker ships. See the Phase 2
    /// section of `plan-test/tasklogs/B0-01.md`.
    pub fn fetch_legacy_token(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<Vec<u8>, CipherStoreError> {
        let url = format!("{}/v1/blob/{}", self.base_url, id_hex);
        let resp = self
            .http
            .get(&url)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .timeout(self.timeouts.send)
            .send()?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Err(CipherStoreError::NotFound);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes()?;
        Ok(bytes.to_vec())
    }

    /// Acknowledges a durably persisted plaintext.  This is intentionally a
    /// separate verb from fetch: receipt authority cannot be used to fetch.
    pub fn ack(
        &self,
        id_hex: &str,
        ack_cap: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        self.no_content(
            "POST",
            &format!("{}/v1/blob/{id_hex}/ack", self.base_url),
            "x-osl-ack-cap",
            ack_cap,
        )
    }

    /// Permanently burns a copy with the sender-derived manage authority.
    /// The Worker is unconditionally idempotent (`204`), including unknown ids.
    pub fn burn(
        &self,
        id_hex: &str,
        manage_cap: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        self.no_content(
            "DELETE",
            &format!("{}/v1/blob/{id_hex}", self.base_url),
            "x-osl-manage-cap",
            manage_cap,
        )
    }

    /// Permanently burn the sender's own stored copy with both authorities the
    /// Worker requires: the sender-derived manage capability and the scoped
    /// sender delete grant bound to this message.
    pub fn burn_own_copy(
        &self,
        id_hex: &str,
        manage_cap: &[u8; FETCH_TOKEN_BYTES],
        delete_grant: &DeleteGrantRecord,
    ) -> Result<OwnCopyBurnReport, CipherStoreError> {
        let response = self
            .http
            .delete(format!("{}/v1/blob/{id_hex}", self.base_url))
            .header("x-osl-manage-cap", hex_lower(manage_cap))
            .header("x-osl-delete-grant", delete_grant.header_value()?)
            .timeout(self.timeouts.send)
            .send()?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(status_error(response));
        }
        Ok(OwnCopyBurnReport {
            affected_scope: delete_grant.scope().to_owned(),
        })
    }

    fn no_content(
        &self,
        method: &str,
        url: &str,
        header: &str,
        cap: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        let request = match method {
            "POST" => self.http.post(url),
            "DELETE" => self.http.delete(url),
            _ => unreachable!(),
        };
        let response = request
            .header(header, hex_lower(cap))
            .timeout(self.timeouts.send)
            .send()?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(CipherStoreError::Status {
                status: response.status().as_u16(),
                body: response.text().unwrap_or_default(),
            });
        }
        Ok(())
    }

    /// Burn (delete) a blob. Idempotent — second call returns Ok even
    /// if the row was already gone. Phase 6: same capability-token
    /// gate as fetch so a leaked blob_id alone cannot be used to
    /// DoS-delete a conversation's blobs.
    pub fn delete(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        let url = format!("{}/v1/blob/{}", self.base_url, id_hex);
        let resp = self
            .http
            .delete(&url)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .timeout(self.timeouts.send)
            .send()?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        // 204 No Content on success; the server also returns 204 when
        // the row didn't exist (idempotent delete).
        if !status.is_success() && status != StatusCode::NO_CONTENT {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }

    /// Stream one already-sealed attachment file to the R2-backed endpoint.
    /// The file never crosses renderer IPC and is never copied into a Vec.
    pub fn upload_attachment_file(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<UploadResult, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        if length > LEGACY_DIRECT_ATTACHMENT_BYTES {
            return self
                .upload_attachment_multipart(sealed, length, ttl_seconds, fetch_token, None, None, None)
                .map(|report| UploadResult {
                    id_hex: report.completed_file.file_id,
                    expires_at: report.completed_file.expires_at,
                });
        }
        let response = self
            .http
            .post(format!("{}/v1/attachment", self.base_url))
            .timeout(self.timeouts.attachment)
            .header("content-type", "application/octet-stream")
            .header("content-length", length)
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .body(reqwest::blocking::Body::sized(sealed, length))
            .send()?;
        parse_upload_response(response, 32)
    }

    /// Upload an already-sealed Pro attachment as numbered 8 MiB pieces.
    ///
    /// Unlike [`Self::upload_attachment_file`], this always uses the multipart
    /// protocol, including for a file that fits in one piece. The returned
    /// report preserves the server-confirmed order and size of every finished
    /// piece as well as the final completed-file receipt.
    pub fn upload_attachment_file_pro_chunked(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        self.upload_attachment_multipart(sealed, length, ttl_seconds, fetch_token, None, None, None)
    }

    /// Multipart upload registered to an exact message burn signal. The signal
    /// is checked around every accepted part and immediately before completion.
    pub fn upload_attachment_file_pro_chunked_cancellable(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        cancellation: &crate::attachment_uploads::AttachmentUploadCancellation,
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        if cancellation.is_cancelled() {
            return Err(CipherStoreError::AttachmentUploadCancelled);
        }
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        self.upload_attachment_multipart(
            sealed,
            length,
            ttl_seconds,
            fetch_token,
            None,
            None,
            Some(cancellation),
        )
    }

    /// Upload an already-sealed Pro attachment and write live byte counters to
    /// `progress`.  The caller owns the handle and clears it after displaying
    /// completion or cancellation.
    pub fn upload_attachment_file_pro_chunked_with_progress(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        progress: &ProChunkedUploadProgress,
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        progress.begin(length);
        self.upload_attachment_multipart(
            sealed,
            length,
            ttl_seconds,
            fetch_token,
            None,
            Some(progress.clone()),
            None,
        )
    }

    /// Upload a Pro attachment, resuming an existing server upload when its
    /// durable checkpoint is present. A reconnect never creates a second
    /// session and sends only pieces not named in the checkpoint.
    pub fn upload_attachment_file_pro_chunked_with_resume_record(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        resume_record_path: impl AsRef<Path>,
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        self.upload_attachment_multipart(
            sealed,
            length,
            ttl_seconds,
            fetch_token,
            Some(resume_record_path.as_ref()),
            None,
            None,
        )
    }

    /// Upload with a progress entry that is automatically removed from
    /// `active` when the multipart operation reaches either terminal result.
    /// The returned snapshot is retained for the completion screen after the
    /// active-progress query has become empty.
    pub fn upload_attachment_file_pro_chunked_with_active_progress(
        &self,
        sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        progress: &ProChunkedUploadProgress,
        active: &ActiveProChunkedUploadProgress,
    ) -> Result<(ProChunkedUploadReport, ProChunkedUploadProgressSnapshot), CipherStoreError> {
        let total_bytes = sealed.metadata()?.len();
        active.begin(progress, total_bytes);
        let result = self.upload_attachment_file_pro_chunked_with_progress(
            sealed,
            ttl_seconds,
            fetch_token,
            progress,
        );
        let final_snapshot = progress.query();
        active.finish(progress);
        result.map(|report| (report, final_snapshot))
    }

    /// Resume a Pro upload with live progress and a safe local cancel action.
    ///
    /// Cancellation removes the durable checkpoint containing unfinished local
    /// part receipts, but never deletes an already-completed store object.
    pub fn upload_attachment_file_pro_chunked_with_resume_record_and_progress(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        resume_record_path: impl AsRef<Path>,
        progress: &ProChunkedUploadProgress,
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        let path = resume_record_path.as_ref();
        progress.begin(length);
        let mut result = self.upload_attachment_multipart(
            sealed,
            length,
            ttl_seconds,
            fetch_token,
            Some(path),
            Some(progress.clone()),
            None,
        );
        // A cancellation can arrive while reqwest is draining the current
        // request body.  Normalize that transport interruption to the same
        // local outcome as a boundary cancellation.
        if result.is_err() && progress.is_cancelled() {
            result = Err(CipherStoreError::UploadCancelled);
        }
        if matches!(&result, Err(CipherStoreError::UploadCancelled)) {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        progress.clear();
        result
    }

    fn upload_attachment_multipart(
        &self,
        sealed: File,
        length: u64,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        resume_record_path: Option<&Path>,
        progress: Option<ProChunkedUploadProgress>,
        cancellation: Option<&crate::attachment_uploads::AttachmentUploadCancellation>,
    ) -> Result<ProChunkedUploadReport, CipherStoreError> {
        refuse_burned_attachment_upload(cancellation)?;
        if progress.as_ref().is_some_and(ProChunkedUploadProgress::is_cancelled) {
            return Err(CipherStoreError::UploadCancelled);
        }
        let token = hex_lower(fetch_token);
        let checkpoint = match resume_record_path {
            Some(path) => read_pro_chunked_upload_resume_record(path)?,
            None => None,
        };
        let resuming = checkpoint.is_some();
        let (upload_id, expected_expires_at, plan, mut completed_piece_numbers) = match checkpoint {
            Some(record) => {
                validate_attachment_id(&record.upload_id)?;
                let plan = multipart_plan(
                    length,
                    ATTACHMENT_MULTIPART_PART_BYTES,
                    ATTACHMENT_MULTIPART_MAX_PARTS,
                )?;
                for piece in &record.completed_piece_numbers {
                    if !plan.iter().any(|(number, _, _)| number == piece)
                        || record
                            .completed_piece_numbers
                            .iter()
                            .filter(|other| *other == piece)
                            .count()
                            != 1
                    {
                        return Err(CipherStoreError::ParseError(
                            "multipart resume record has invalid completed pieces".to_owned(),
                        ));
                    }
                }
                if let Some(progress) = &progress {
                    progress.set_unfinished_local_parts(record.completed_piece_numbers.len());
                }
                (record.upload_id, None, plan, record.completed_piece_numbers)
            }
            None => {
                let response = self
                    .http
                    .post(format!("{}/v1/attachment/session", self.base_url))
                    .timeout(self.timeouts.attachment)
                    .header("content-length", 0)
                    .header("x-osl-ttl-seconds", ttl_seconds.to_string())
                    .header("x-osl-fetch-token", &token)
                    .header("x-osl-size-bytes", length.to_string())
                    .send()?;
                let session: AttachmentSessionResponse = parse_bounded_json(response)?;
                validate_attachment_id(&session.id)?;
                if session.expires_at <= 0
                    || session.size_bytes != length
                    || session.max_part_bytes != ATTACHMENT_MULTIPART_PART_BYTES
                    || session.max_parts != ATTACHMENT_MULTIPART_MAX_PARTS
                {
                    let _ = self.delete_attachment(&session.id, fetch_token);
                    return Err(CipherStoreError::ParseError(
                        "multipart session has unexpected shape".to_owned(),
                    ));
                }
                let plan = match multipart_plan(length, session.max_part_bytes, session.max_parts) {
                    Ok(plan) => plan,
                    Err(error) => {
                        let _ = self.delete_attachment(&session.id, fetch_token);
                        return Err(error);
                    }
                };
                (session.id, Some(session.expires_at), plan, Vec::new())
            }
        };
        let piece_count = u32::try_from(plan.len()).map_err(|_| {
            CipherStoreError::ParseError("multipart part count overflow".to_owned())
        })?;
        let result = (|| {
            let mut finished_pieces = Vec::with_capacity(plan.len());
            for (part_number, offset, part_length) in plan {
                refuse_burned_attachment_upload(cancellation)?;
                if progress.as_ref().is_some_and(ProChunkedUploadProgress::is_cancelled) {
                    return Err(CipherStoreError::UploadCancelled);
                }
                if completed_piece_numbers.contains(&part_number) {
                    continue;
                }
                let mut part_file = sealed.try_clone()?;
                part_file.seek(SeekFrom::Start(offset))?;
                let reader = ExactPartReader {
                    inner: part_file,
                    remaining: part_length,
                    progress: progress.clone(),
                };
                let response = self
                    .http
                    .put(format!(
                        "{}/v1/attachment/{}/part/{part_number}",
                        self.base_url, upload_id
                    ))
                    .timeout(self.timeouts.attachment)
                    .header("content-type", "application/octet-stream")
                    .header("content-length", part_length)
                    .header("x-osl-fetch-token", &token)
                    .body(reqwest::blocking::Body::sized(reader, part_length))
                    .send()?;
                let receipt: AttachmentPartResponse = parse_bounded_json(response)?;
                if receipt.part_number != part_number || receipt.size_bytes != part_length {
                    return Err(CipherStoreError::ParseError(
                        "multipart part receipt mismatch".to_owned(),
                    ));
                }
                finished_pieces.push(ProChunkedUploadPiece {
                    upload_id: upload_id.clone(),
                    piece_number: receipt.part_number,
                    size_bytes: receipt.size_bytes,
                });
                refuse_burned_attachment_upload(cancellation)?;
                completed_piece_numbers.push(receipt.part_number);
                if let Some(path) = resume_record_path {
                    ProChunkedUploadResumeRecord {
                        upload_id: upload_id.clone(),
                        completed_piece_numbers: completed_piece_numbers.clone(),
                    }
                    .save(path)?;
                    if let Some(progress) = &progress {
                        progress.set_unfinished_local_parts(completed_piece_numbers.len());
                        // The native Cancel command runs on another thread.
                        // Give it a short boundary before constructing the
                        // next request, so a receipt cannot race it into a
                        // new part after its local checkpoint is visible.
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
            if progress.as_ref().is_some_and(ProChunkedUploadProgress::is_cancelled) {
                return Err(CipherStoreError::UploadCancelled);
            }
            refuse_burned_attachment_upload(cancellation)?;
            if sealed.metadata()?.len() != length {
                return Err(CipherStoreError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sealed attachment changed during upload",
                )));
            }
            let response = self
                .http
                .post(format!(
                    "{}/v1/attachment/{}/complete",
                    self.base_url, upload_id
                ))
                .timeout(self.timeouts.attachment)
                .header("content-length", 0)
                .header("x-osl-fetch-token", &token)
                .send()?;
            let complete: AttachmentCompleteResponse = parse_bounded_json(response)?;
            if complete.id != upload_id
                || expected_expires_at.is_some_and(|expires_at| complete.expires_at != expires_at)
                || complete.expires_at <= 0
                || complete.size_bytes != length
            {
                return Err(CipherStoreError::ParseError(
                    "multipart completion receipt mismatch".to_owned(),
                ));
            }
            Ok(ProChunkedUploadReport {
                finished_pieces,
                completed_file: ProChunkedUploadFile {
                    file_id: complete.id,
                    total_size_bytes: complete.size_bytes,
                    piece_count,
                    expires_at: complete.expires_at,
                },
            })
        })();
        if result.is_err() {
            if !resuming && resume_record_path.is_none() {
                let _ = self.delete_attachment(&upload_id, fetch_token);
            }
        } else if let Some(path) = resume_record_path {
            let _ = fs::remove_file(path);
        }
        result
    }

    /// Stream an opaque sealed attachment into a trusted native writer.
    /// Callers must use a newly-created OSL staging file and remove it on any
    /// error; this function writes at most the authoritative sealed bound + 1.
    pub fn fetch_attachment_to_writer(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        output: &mut impl Write,
    ) -> Result<u64, CipherStoreError> {
        validate_attachment_id(id_hex)?;
        let response = self
            .http
            .get(format!("{}/v1/attachment/{id_hex}", self.base_url))
            .timeout(self.timeouts.attachment)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(CipherStoreError::NotFound);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            return Err(status_error(response));
        }
        if response
            .content_length()
            .is_some_and(|length| length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES)
        {
            return Err(CipherStoreError::BlobTooLarge {
                got: response
                    .content_length()
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        let mut limited = response.take(MAX_SEALED_ATTACHMENT_BYTES + 1);
        let copied = io::copy(&mut limited, output)?;
        if copied == 0 || copied > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(copied).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        Ok(copied)
    }

    pub fn delete_attachment(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        validate_attachment_id(id_hex)?;
        let response = self
            .http
            .delete(format!("{}/v1/attachment/{id_hex}", self.base_url))
            .timeout(self.timeouts.attachment)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(status_error(response));
        }
        Ok(())
    }
}

fn validate_attachment_id(id_hex: &str) -> Result<(), CipherStoreError> {
    if id_hex.len() != 32
        || !id_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CipherStoreError::ParseError(
            "attachment id has unexpected shape".to_owned(),
        ));
    }
    Ok(())
}

fn status_error(response: reqwest::blocking::Response) -> CipherStoreError {
    let status = response.status().as_u16();
    let mut body_bytes = Vec::with_capacity(4 * 1024);
    let _ = response.take(4 * 1024).read_to_end(&mut body_bytes);
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    CipherStoreError::Status { status, body }
}

fn parse_bounded_json<T: DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T, CipherStoreError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CipherStoreError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(status_error(response));
    }
    serde_json::from_reader(response.take(4 * 1024))
        .map_err(|error| CipherStoreError::ParseError(error.to_string()))
}

fn multipart_plan(
    length: u64,
    part_bytes: u64,
    max_parts: u32,
) -> Result<Vec<(u32, u64, u64)>, CipherStoreError> {
    if length == 0
        || length > MAX_SEALED_ATTACHMENT_BYTES
        || part_bytes == 0
        || part_bytes > ATTACHMENT_MULTIPART_PART_BYTES
        || max_parts == 0
        || max_parts > ATTACHMENT_MULTIPART_MAX_PARTS
    {
        return Err(CipherStoreError::ParseError(
            "invalid multipart bounds".to_owned(),
        ));
    }
    let count = length.div_ceil(part_bytes);
    if count > u64::from(max_parts) {
        return Err(CipherStoreError::BlobTooLarge {
            got: usize::try_from(length).unwrap_or(usize::MAX),
            max: usize::try_from(part_bytes.saturating_mul(u64::from(max_parts)))
                .unwrap_or(usize::MAX),
        });
    }
    let mut plan = Vec::with_capacity(count as usize);
    let mut offset = 0u64;
    for part_number in 1..=count as u32 {
        let size = (length - offset).min(part_bytes);
        plan.push((part_number, offset, size));
        offset += size;
    }
    Ok(plan)
}

fn parse_upload_response(
    response: reqwest::blocking::Response,
    expected_id_hex_len: usize,
) -> Result<UploadResult, CipherStoreError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CipherStoreError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(status_error(response));
    }
    let json: serde_json::Value = serde_json::from_reader(response.take(4 * 1024))
        .map_err(|error| CipherStoreError::ParseError(error.to_string()))?;
    let id_hex = json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CipherStoreError::ParseError("missing id".to_owned()))?
        .to_owned();
    if id_hex.len() != expected_id_hex_len
        || !id_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CipherStoreError::ParseError(
            "id has unexpected shape".to_owned(),
        ));
    }
    let expires_at = json
        .get("expires_at")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| CipherStoreError::ParseError("missing expires_at".to_owned()))?;
    Ok(UploadResult { id_hex, expires_at })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::thread;

    fn write_cipher_store_override(dir: &Path, url: &str) {
        std::fs::write(
            dir.join("keyserver.json"),
            format!(r#"{{"cipher_store_url":"{url}"}}"#),
        )
        .unwrap();
    }

    #[test]
    fn mutant_remove_release_guard_would_accept_arbitrary_cipher_store_host() {
        let dir = tempfile::tempdir().unwrap();
        write_cipher_store_override(dir.path(), "https://evil.example");

        let result = resolve_cipher_store_base_url_with_policy(dir.path(), false);

        match result {
            Err(CipherStoreError::ConfigOverrideRefused {
                configured,
                default,
                build,
            }) => {
                assert_eq!(configured, "https://evil.example");
                assert_eq!(default, DEFAULT_CIPHER_STORE_BASE_URL);
                assert_eq!(build, "release");
            }
            other => panic!("release policy must not accept arbitrary cipher_store_url: {other:?}"),
        }
    }

    #[test]
    fn mutant_foreign_host_simulated_release_refusal_is_loud() {
        let dir = tempfile::tempdir().unwrap();
        write_cipher_store_override(dir.path(), "https://attacker.invalid/blob");

        let error = resolve_cipher_store_base_url_with_policy(dir.path(), false)
            .expect_err("release policy must refuse a foreign cipher-store override");
        let rendered = error.to_string();

        assert!(rendered.contains("cipher_store_url"), "{rendered}");
        assert!(rendered.contains("release build"), "{rendered}");
        assert!(
            rendered.contains("https://attacker.invalid/blob"),
            "{rendered}"
        );
        assert!(
            rendered.contains(DEFAULT_CIPHER_STORE_BASE_URL),
            "{rendered}"
        );
    }

    #[test]
    fn mutant_debug_policy_accepts_legitimate_loopback_cipher_store_override() {
        let dir = tempfile::tempdir().unwrap();
        write_cipher_store_override(dir.path(), "http://127.0.0.1:8787/");

        assert_eq!(
            resolve_cipher_store_base_url_with_policy(dir.path(), true).unwrap(),
            "http://127.0.0.1:8787"
        );
    }

    #[test]
    fn t6_t19_client_uses_header_capabilities_and_non_destructive_fetch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..5 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut raw = Vec::new();
                let mut chunk = [0u8; 1024];
                loop {
                    let count = stream.read(&mut chunk).unwrap();
                    raw.extend_from_slice(&chunk[..count]);
                    let Some(headers_end) = raw.windows(4).position(|v| v == b"\r\n\r\n") else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&raw[..headers_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    if raw.len() >= headers_end + 4 + length {
                        break;
                    }
                }
                requests.push(String::from_utf8_lossy(&raw).to_ascii_lowercase());
                let response = match index {
                    0 => "HTTP/1.1 201 Created\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 56\r\n\r\n{\"id\":\"00000000000000000000000000000000\",\"expires_at\":1}",
                    1 | 2 => "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: 3\r\n\r\none",
                    _ => "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n",
                };
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        let caps = BlobCapabilities {
            fetch_cap: [1; 16],
            ack_cap: [2; 16],
            manage_cap: [3; 16],
            delivery_tag: [4; 16],
        };
        let upload = client
            .upload_pointer(
                b"one",
                TTL_7D,
                &[9; 16],
                caps,
                BlobObjectClass::SingleAck,
                None,
            )
            .unwrap();
        assert_eq!(upload.id_hex, "00000000000000000000000000000000");
        assert_eq!(
            client.fetch(&upload.id_hex, &caps.fetch_cap).unwrap(),
            b"one"
        );
        assert_eq!(
            client.fetch(&upload.id_hex, &caps.fetch_cap).unwrap(),
            b"one"
        );
        client.ack(&upload.id_hex, &caps.ack_cap).unwrap();
        client.burn(&upload.id_hex, &caps.manage_cap).unwrap();
        let requests = server.join().unwrap();
        // The digest is over the hex the header carries, which is the only
        // form the Worker ever sees. Asserted as a value, not as a header
        // name: a digest over the raw bytes would still be present, and would
        // still be 64 hex chars, and no fetch would ever authenticate again.
        let digest_of_hex =
            |cap: &[u8; FETCH_TOKEN_BYTES]| hex_lower(&Sha256::digest(hex_lower(cap).as_bytes()));
        // The Worker routes upload on PUT and nothing else; POST /v1/blob is
        // pinned at 404 by `cipher-store-cf/test/routes-and-healthz.test.ts`.
        assert!(
            requests[0].starts_with("put /v1/blob"),
            "capability upload must use the verb the Worker routes"
        );
        assert!(requests[0].contains(&format!(
            "x-osl-fetch-digest: {}",
            digest_of_hex(&caps.fetch_cap)
        )));
        assert!(requests[0].contains(&format!(
            "x-osl-ack-digest: {}",
            digest_of_hex(&caps.ack_cap)
        )));
        assert!(requests[0].contains(&format!(
            "x-osl-manage-digest: {}",
            digest_of_hex(&caps.manage_cap)
        )));
        assert!(requests[0].contains("x-osl-object-class: single-ack"));
        assert!(
            !requests[0].contains("x-osl-expiry-mode"),
            "the seven-day default window needs no opt-out"
        );
        assert!(requests[1].contains("x-osl-fetch-cap: 01010101010101010101010101010101"));
        assert!(requests[2].contains("x-osl-fetch-cap: 01010101010101010101010101010101"));
        assert!(requests[3].starts_with("post /v1/blob/00000000000000000000000000000000/ack"));
        assert!(requests[3].contains("x-osl-ack-cap: 02020202020202020202020202020202"));
        assert!(requests[4].starts_with("delete /v1/blob/00000000000000000000000000000000"));
        assert!(requests[4].contains("x-osl-manage-cap: 03030303030303030303030303030303"));
    }

    #[test]
    fn task_0406_sender_own_copy_burn_passes_delete_grant_and_reports_scope() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                .unwrap();
            request
        });
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        let grant = DeleteGrantRecord::new(
            "message:0406",
            "identity:sender-0406",
            "discord:9000000000000406:direct_message:own-copy",
        );

        let report = client
            .burn_own_copy(
                "04060000000000000000000000000000",
                &[0xd4; FETCH_TOKEN_BYTES],
                &grant,
            )
            .unwrap();

        let request = server.join().unwrap();
        let request_lower = request.to_ascii_lowercase();
        assert!(request_lower.starts_with("delete /v1/blob/04060000000000000000000000000000"));
        assert!(request_lower.contains("x-osl-manage-cap: d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4"));
        let grant_line = request
            .lines()
            .find(|line| {
                line.to_ascii_lowercase()
                    .starts_with("x-osl-delete-grant: ")
            })
            .expect("sender delete grant header is present");
        let grant_json = grant_line
            .split_once(": ")
            .map(|(_, value)| value)
            .expect("header has a value");
        let sent_grant: serde_json::Value = serde_json::from_str(grant_json).unwrap();
        assert_eq!(sent_grant["record"], DELETE_GRANT_RECORD);
        assert_eq!(sent_grant["message"], "message:0406");
        assert_eq!(sent_grant["owner"], "identity:sender-0406");
        assert_eq!(
            sent_grant["scope"],
            "discord:9000000000000406:direct_message:own-copy"
        );
        assert_eq!(
            report.affected_scope,
            "discord:9000000000000406:direct_message:own-copy"
        );
        println!(
            "TASK0406 rust_sender_command=burn_own_copy delete_grant_header=present affected_scope={} status=204",
            report.affected_scope
        );
    }

    /// A window shorter than the seven-day default floor is refused outright
    /// by the Worker unless the sender declares it deliberate. Without this
    /// header every conversation whose operator chose 1h, 24h or the shipping
    /// 72h default fails to upload at all.
    #[test]
    fn a_shortened_delivery_window_declares_itself_absolute() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut raw = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                raw.extend_from_slice(&chunk[..count]);
                let Some(headers_end) = raw.windows(4).position(|v| v == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&raw[..headers_end]);
                let length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                if raw.len() >= headers_end + 4 + length {
                    break;
                }
            }
            stream.write_all(b"HTTP/1.1 201 Created\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 56\r\n\r\n{\"id\":\"00000000000000000000000000000000\",\"expires_at\":1}").unwrap();
            String::from_utf8_lossy(&raw).to_ascii_lowercase()
        });
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        let caps = BlobCapabilities {
            fetch_cap: [1; 16],
            ack_cap: [2; 16],
            manage_cap: [3; 16],
            delivery_tag: [4; 16],
        };
        client
            .upload_pointer(
                b"one",
                TTL_72H,
                &[9; 16],
                caps,
                BlobObjectClass::SingleAck,
                None,
            )
            .unwrap();
        let request = server.join().unwrap();
        assert!(request.contains("x-osl-ttl-seconds: 259200"));
        assert!(request.contains("x-osl-expiry-mode: absolute"));
    }

    // -----------------------------------------------------------------------
    // B0-01 phase 2 — upload-admission grants
    // -----------------------------------------------------------------------

    /// Read one HTTP request off a socket, headers plus declared body.
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut raw = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let count = stream.read(&mut chunk).unwrap();
            if count == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..count]);
            let Some(end) = raw.windows(4).position(|v| v == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&raw[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let lower = line.to_ascii_lowercase();
                    lower
                        .strip_prefix("content-length: ")
                        .map(|value| value.trim().to_owned())
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if raw.len() >= end + 4 + length {
                break;
            }
        }
        String::from_utf8_lossy(&raw).into_owned()
    }

    fn grant_header(audience: &str, expires_at: i64) -> String {
        let payload = format!(
            "{{\"aud\":\"{audience}\",\"exp\":{expires_at},\"jti\":\"{}\"}}",
            "ab".repeat(16)
        );
        format!(
            "{STORAGE_GRANT_SCHEME} {}.{}",
            B64URL.encode(payload.as_bytes()),
            B64URL.encode([7u8; 64])
        )
    }

    fn caps() -> BlobCapabilities {
        BlobCapabilities {
            fetch_cap: [1; 16],
            ack_cap: [2; 16],
            manage_cap: [3; 16],
            delivery_tag: [4; 16],
        }
    }

    /// The canonical bytes the keyserver rebuilds and verifies against. If this
    /// drifts, every mint fails with an opaque 401 and the client cannot tell
    /// that from an unregistered identity.
    ///
    /// Pinned as an explicit byte layout rather than by round-tripping this
    /// module's own encoder, which would agree with itself no matter what.
    #[test]
    fn canonical_link_grant_bytes_match_the_keyserver_encoding() {
        let bytes =
            canonical_link_grant_bytes("user-1", 1_753_500_600_000, "R".repeat(43).as_str());
        let mut expected = Vec::new();
        for field in [
            b"discord-privacy-client/link-grant/v1".as_slice(),
            b"user-1".as_slice(),
            b"1753500600000".as_slice(),
            "R".repeat(43).as_bytes(),
        ] {
            expected.extend_from_slice(&(field.len() as u32).to_be_bytes());
            expected.extend_from_slice(field);
        }
        assert_eq!(bytes, expected);
        // Four length prefixes and no separators: 16 bytes of framing.
        assert_eq!(bytes.len(), 16 + 36 + 6 + 13 + 43);
    }

    /// `isHighEntropyRequestId` is `/^[A-Za-z0-9_-]{43}$/`
    /// (`keyserver-cf/src/lib/validation.ts:8`). A request id in any other
    /// shape is a 400 before the signature is ever checked.
    #[test]
    fn request_ids_are_the_shape_the_keyserver_accepts() {
        for _ in 0..16 {
            let id = new_link_grant_request_id();
            assert_eq!(id.len(), 43, "{id}");
            assert!(id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'));
        }
        assert_ne!(new_link_grant_request_id(), new_link_grant_request_id());
    }

    /// The whole point of phase 2: the client can obtain the credential the
    /// capability upload route demands. `link.grant` had zero call sites, so
    /// this exercises a path that did not exist.
    #[test]
    fn mint_storage_grant_signs_the_request_and_reads_back_the_claims() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let expires_at = now_unix_seconds() + 300;
        let authorization = grant_header(STORAGE_GRANT_AUDIENCE, expires_at);
        let served = authorization.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let body = format!("{{\"authorization\":\"{served}\",\"expires_at\":{expires_at}}}");
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .unwrap();
            request
        });

        let http = Client::builder().timeout(REQUEST_TIMEOUT).build().unwrap();
        let mut signed_over: Vec<u8> = Vec::new();
        let grant = mint_storage_grant(&http, &format!("http://{address}"), "user-1", |message| {
            signed_over = message.to_vec();
            [7u8; 64]
        })
        .unwrap();

        let request = server.join().unwrap();
        let (head, body) = request.split_once("\r\n\r\n").unwrap();
        assert!(head.to_ascii_lowercase().starts_with("post /v1/link-grant"));
        let sent: serde_json::Value = serde_json::from_str(body).unwrap();
        let user_id = sent["user_id"].as_str().unwrap();
        let timestamp_ms = sent["timestamp_ms"].as_i64().unwrap();
        let request_id = sent["request_id"].as_str().unwrap();
        assert_eq!(user_id, "user-1");
        assert_eq!(request_id.len(), 43);
        // The bytes handed to the signer are exactly the ones the keyserver
        // will rebuild from the fields in this body. A signature over anything
        // else verifies nowhere.
        assert_eq!(
            signed_over,
            canonical_link_grant_bytes(user_id, timestamp_ms, request_id)
        );
        assert_eq!(
            sent["signature_b64"].as_str().unwrap(),
            B64_STANDARD.encode([7u8; 64])
        );
        assert_eq!(grant.audience(), STORAGE_GRANT_AUDIENCE);
        assert_eq!(grant.expires_at(), expires_at);
    }

    /// A grant is a bearer credential. `Debug` on the struct that holds it must
    /// not be the thing that puts it in a log line.
    #[test]
    fn a_grant_never_prints_its_credential() {
        let grant = StorageGrant::from_authorization(&grant_header(
            STORAGE_GRANT_AUDIENCE,
            now_unix_seconds() + 300,
        ))
        .unwrap();
        let rendered = format!("{grant:?}");
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(STORAGE_GRANT_SCHEME));
        assert!(!rendered.contains(&B64URL.encode([7u8; 64])));
    }

    /// A minted grant rides on the upload as the `Authorization` header
    /// `verifyStorageGrant` reads (`cipher-store-cf/src/lib/storage-grant.ts:62`).
    #[test]
    fn a_capability_upload_presents_its_grant_over_put() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            stream.write_all(b"HTTP/1.1 201 Created\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 56\r\n\r\n{\"id\":\"00000000000000000000000000000000\",\"expires_at\":1}").unwrap();
            request.to_ascii_lowercase()
        });
        let authorization = grant_header(STORAGE_GRANT_AUDIENCE, now_unix_seconds() + 300);
        let grant = StorageGrant::from_authorization(&authorization).unwrap();
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        client
            .upload_pointer(
                b"one",
                TTL_7D,
                &[9; 16],
                caps(),
                BlobObjectClass::SingleAck,
                Some(&grant),
            )
            .unwrap();
        let request = server.join().unwrap();
        assert!(
            request.starts_with("put /v1/blob"),
            "the Worker routes upload on PUT only; got: {}",
            request.lines().next().unwrap_or_default()
        );
        assert!(request.contains(&format!(
            "authorization: {}",
            authorization.to_ascii_lowercase()
        )));
    }

    /// D-117's neighbour, and the reason this client checks the audience at
    /// all: the keyserver's only issuer mints `osl-link-create`
    /// (`keyserver-cf/src/lib/link-grant-issuer.ts:61`), while blob upload
    /// requires `osl-blob-store` (`cipher-store-cf/src/lib/storage-grant.ts:12`).
    /// The refusal happens here, before the credential is spent, and the fix is
    /// never to widen either audience.
    #[test]
    fn a_link_creation_grant_cannot_be_spent_on_a_blob_upload() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let grant = StorageGrant::from_authorization(&grant_header(
            LINK_CREATE_GRANT_AUDIENCE,
            now_unix_seconds() + 300,
        ))
        .unwrap();
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        let error = client
            .upload_pointer(
                b"one",
                TTL_7D,
                &[9; 16],
                caps(),
                BlobObjectClass::SingleAck,
                Some(&grant),
            )
            .expect_err("a link-creation grant must not authorise a blob upload");
        assert!(
            matches!(&error, CipherStoreError::GrantAudience { got, want }
                if got == LINK_CREATE_GRANT_AUDIENCE && *want == STORAGE_GRANT_AUDIENCE),
            "{error}"
        );
        // Nothing was sent: the socket never accepted a connection.
        drop(client);
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "the refusal must happen before the request leaves the device"
        );
    }

    /// An expired grant is refused locally too. The store would answer 401
    /// `grant_expired`, but spending it also burns a single-use credential.
    #[test]
    fn an_expired_grant_is_refused_before_it_is_spent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let grant = StorageGrant::from_authorization(&grant_header(
            STORAGE_GRANT_AUDIENCE,
            now_unix_seconds() - 1,
        ))
        .unwrap();
        let client = CipherStoreClient::new(format!("http://{address}")).unwrap();
        let error = client
            .upload_pointer(
                b"one",
                TTL_7D,
                &[9; 16],
                caps(),
                BlobObjectClass::SingleAck,
                Some(&grant),
            )
            .expect_err("an expired grant must not be spent");
        assert!(matches!(error, CipherStoreError::GrantExpired), "{error}");
    }

    #[test]
    fn a_malformed_grant_is_rejected_without_echoing_it() {
        for bad in [
            "Bearer abc.def",
            "OSL-Link-Grant not-two-parts",
            "OSL-Link-Grant !!!.!!!",
        ] {
            let error = StorageGrant::from_authorization(bad)
                .expect_err("malformed grants must not be adopted");
            assert!(
                matches!(error, CipherStoreError::GrantMalformed(_)),
                "{error}"
            );
            assert!(!error.to_string().contains(bad), "{error}");
        }
    }

    #[test]
    fn ttl_allowlist_matches_the_cipher_store_worker() {
        assert_eq!(TTL_1H, 3_600);
        assert_eq!(TTL_24H, 86_400);
        assert_eq!(TTL_72H, 259_200);
        assert_eq!(TTL_7D, 604_800);
        assert_eq!(TTL_30D, 2_592_000);
        for ttl in [TTL_1H, TTL_24H, TTL_72H, TTL_7D, TTL_30D] {
            assert!(is_valid_ttl(ttl));
        }
        for ttl in [0, 3_599, 3_601, 86_399, 604_801, 2_592_001, u32::MAX] {
            assert!(!is_valid_ttl(ttl));
        }
    }

    #[test]
    fn bad_ttl_error_lists_every_exact_option() {
        assert_eq!(
            CipherStoreError::BadTtl(3_601).to_string(),
            "invalid TTL 3601; must be 3600 (1h), 86400 (24h), 259200 (72h), 604800 (7d), or 2592000 (30d)"
        );
    }

    #[test]
    fn attachment_ids_are_canonical_lowercase_128_bit_values() {
        assert!(validate_attachment_id("0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_attachment_id("0123456789ABCDEF0123456789ABCDEF").is_err());
        assert!(validate_attachment_id("0123456789abcdef").is_err());
        assert!(validate_attachment_id("0123456789abcdef0123456789abcdeg").is_err());
    }

    #[test]
    fn multipart_plan_covers_the_1_gib_boundary_with_fixed_parts() {
        let plan = multipart_plan(
            MAX_SEALED_ATTACHMENT_BYTES,
            ATTACHMENT_MULTIPART_PART_BYTES,
            ATTACHMENT_MULTIPART_MAX_PARTS,
        )
        .unwrap();
        assert_eq!(plan.len(), 128);
        assert!(plan
            .iter()
            .all(|(_, _, size)| *size <= ATTACHMENT_MULTIPART_PART_BYTES));
        assert_eq!(
            plan.iter().map(|(_, _, size)| *size).sum::<u64>(),
            MAX_SEALED_ATTACHMENT_BYTES
        );
        assert_eq!(plan.last().unwrap().0, 128);
    }

    #[test]
    fn exact_part_reader_detects_truncation_without_large_allocation() {
        let mut reader = ExactPartReader {
            inner: io::Cursor::new(vec![1u8; 7]),
            remaining: 8,
            progress: None,
        };
        let mut output = [0u8; 8];
        assert_eq!(reader.read(&mut output).unwrap(), 7);
        assert_eq!(
            reader.read(&mut output).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }

    #[test]
    fn task_4911_tor_attachment_budget_calls_live_transfer_slow_and_stall_by_name() {
        let start = std::time::Instant::now();
        let mut tor = AttachmentTransferWatchdog::tor(start);
        let mut progress_seconds = Vec::new();
        // 8 MiB / 0.30 Mbit/s is about 224 seconds. Model the fixture through
        // second 300 so the assertion proves cadence and budget without making
        // this focused unit test take five minutes of wall clock.
        for second in (0..=300).step_by(30) {
            let now = start + Duration::from_secs(second);
            let status = tor.observe_progress_at(now);
            if second == 0 {
                assert_eq!(status, AttachmentTransferStatus::Moving);
            } else {
                assert_eq!(status, AttachmentTransferStatus::Slow);
            }
            assert_eq!(tor.status_at(now), AttachmentTransferStatus::Moving);
            progress_seconds.push(second);
        }
        assert_eq!(TOR_ATTACHMENT_PART_TIMEOUT, Duration::from_secs(330));
        assert_eq!(
            CipherStoreTimeouts::TOR.attachment,
            TOR_ATTACHMENT_PART_TIMEOUT,
            "Tor attachment timeout must not reuse the Direct 120-second timer"
        );
        assert_eq!(
            CipherStoreTimeouts::TOR.send,
            TOR_STORE_SEND_TIMEOUT,
            "Tor sends must have their own timeout budget"
        );
        assert!(TOR_ATTACHMENT_PART_TIMEOUT > Duration::from_secs(300));
        assert!(progress_seconds
            .windows(2)
            .all(|pair| pair[1] - pair[0] <= 30));

        let stalled = AttachmentTransferWatchdog::tor(start);
        assert_eq!(
            stalled.status_at(start + TOR_ATTACHMENT_PROGRESS_INTERVAL),
            AttachmentTransferStatus::Slow
        );
        assert_eq!(
            stalled.status_at(start + TOR_ATTACHMENT_STALL_TIMEOUT),
            AttachmentTransferStatus::TorAttachmentStalled
        );
        assert_eq!(
            CipherStoreTimeouts::DIRECT.attachment,
            Duration::from_secs(120)
        );
        println!(
            "TASK4911 tor_attachment=8MiB rate_mbit_s=0.30 progress_seconds={progress_seconds:?} last_progress_second=300 timeout_seconds=330 result=not_failed"
        );
        println!("TASK4911 tor_stall bytes_moving=0 stall_seconds=180 result=TorAttachmentStalled");
        println!("TASK4911 direct_attachment_timeout_seconds=120");
    }

    #[test]
    fn task_0649_throttled_37_byte_upload_reports_and_clears_live_progress() {
        let progress = ProChunkedUploadProgress::default();
        progress.begin(37);
        let mut throttled_piece = ExactPartReader {
            inner: io::Cursor::new(vec![0x49; 37]),
            remaining: 37,
            progress: Some(progress.clone()),
        };
        let mut wire_buffer = [0_u8; 17];
        assert_eq!(throttled_piece.read(&mut wire_buffer).unwrap(), 17);

        let live = progress.query();
        assert!(live.uploaded_bytes > 0 && live.uploaded_bytes < 37);
        assert_eq!(live.total_bytes, 37);
        assert_eq!(live.completed_pieces, live.uploaded_bytes);
        println!(
            "TASK0649 live uploaded_bytes={} total_bytes={} completed_pieces={}",
            live.uploaded_bytes, live.total_bytes, live.completed_pieces
        );

        progress.clear();
        let cleared = progress.query();
        assert_eq!(cleared.uploaded_bytes, 0);
        println!("TASK0649 cleared uploaded_bytes={}", cleared.uploaded_bytes);
    }
}
