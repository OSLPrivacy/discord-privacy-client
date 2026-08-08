//! Phase 2 prose-token pivot: composite send/receive helpers that
//! tie together the cipher-store HTTP client, the stego prose-token
//! encoder/decoder, and the per-conversation MAC-key derivation.
//!
//! On the wire:
//!   * Sender: existing PQXDH encryption produces a `DPC0::<base64(cipher)>`
//!     wire. We strip the prefix, decode the base64, frame and Padmé-pad the
//!     ciphertext into a transport object, mint a fresh 160-bit pointer `P`,
//!     derive this object's cipher-store authority from `P` and the sending
//!     layer's keys, upload under the client-derived blob id, and encode `P`
//!     itself as compact chat-like cover text via `encode_token`. The cover
//!     text is what gets posted to chat and OSL Mail bodies — no `DPC0::`
//!     marker, no high-entropy base64 blob, and no server-assigned identifier.
//!   * Receiver: every incoming text message in an OSL-enabled
//!     scope runs through `decode_token`. If the HMAC tag validates,
//!     `P` is extracted, the blob id and fetch capability are derived from
//!     `P` alone, the object is fetched and unframed, and the ciphertext is
//!     re-wrapped as `DPC0::<base64>` so the existing decrypt pipeline picks
//!     it up unchanged.
//!
//! Authority model (T1-30/T1-32/T6-R3): the pointer is the read capability and
//! nothing else. `blob_id` and `fetch_cap` come from `P`, so a recipient can
//! address and read exactly the object it was pointed at. `manage_cap` is
//! rooted in the sender's own send key, so only the sender can burn, and
//! `ack_cap` is rooted in the message key. None of the three is recoverable
//! from the public carrier. This replaces the retired scope-derived fetch
//! token, which any party that merely knew the public scope could compute.
//!
//! Detection-key derivation: the shipping caller supplies secret conversation
//! material and this module expands it under [`DETECT_KEY_HKDF_INFO`]. Scope
//! labels must never decide whether public cover text carries an OSL pointer.
//!
//! Scope binding (D-231): the scope label is not, on its own, allowed to decide
//! detectability — but it must still *constrain* it, or a carrier minted in one
//! conversation is readable in every other conversation that shares a detector.
//! Both properties hold at once by mixing the public scope label into a key that
//! is already secret: [`scope_bound_detection_key`] expands the caller's secret
//! detector under the scope's own salt, and it is that bound value — never the
//! caller's — that reaches the stego layer. A party holding only the public
//! scope still derives nothing (T1-31 unchanged), and a party holding the
//! detector for the wrong conversation now recovers no carrier at all, so no
//! cipher-store request is made either.

use crate::cipher_store_client::{CipherStoreClient, CipherStoreError, FETCH_TOKEN_BYTES};
use crate::scope::{ScopeInput, ScopeKind};
use crate::transport_padding::{frame_padded_transport_object, unframe_padded_transport_object};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use crypto::pointer::{derive_manage_capability, CAPABILITY_BYTES, POINTER_BYTES};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const DPC0_PREFIX: &str = "DPC0::";
const MAC_KEY_LEN: usize = 32;
pub const PROSE_TOKEN_MAC_HKDF_INFO: &[u8] = b"discord-privacy-client/prose-token/mac-key/v1";
/// Domain separator for the private pointer-detection key in transport §6b.
pub const DETECT_KEY_HKDF_INFO: &[u8] = b"osl/detect/v1";

/// The carrier transports the pointer and only the pointer. T1-32 widened the
/// prose carrier to the 160-bit seed for exactly this reason, so a drift
/// between the two constants must not compile.
const _: () = assert!(stego::TOKEN_ID_BYTES == POINTER_BYTES);

// ---------------------------------------------------------------------------
// BRIDGE (B0-01) — a temporary carrier layout. NOT the destination protocol.
// ---------------------------------------------------------------------------
//
// The Worker deployed at ciphers.oslprivacy.com still speaks the
// pre-capability protocol: the *server* assigns the blob id, and a single
// `x-osl-fetch-token` gates both GET and DELETE. The capability Worker in
// `cipher-store-cf/` — client-derived id, three separate capability digests,
// receipts — has never been deployed. Verified live on 2026-08-03:
// `PUT /v1/blob` (the repo's upload route, `cipher-store-cf/src/index.ts:186`)
// answers 404, and that route additionally gates on `verifyStorageGrant`
// (`src/index.ts:189`), which returns 503 for as long as
// `LINK_GRANT_PUBKEY_B64` is unset (`src/lib/storage-grant.ts:43-52`).
//
// So the send path speaks what production implements, using this layout:
//
//     carrier[20] = server_blob_id[8] ‖ seed[12]
//     fetch_token = HKDF-SHA256(ikm = seed, info = BRIDGE_FETCH_INFO)[..16]
//
// The sender picks `seed` *before* uploading, because the token has to ride in
// the upload request's own header, and learns `server_blob_id` only from the
// 201 response. That ordering is the whole reason the id cannot be derived
// from the pointer the way `derive_fetch_authority` derives it — under this
// protocol the client does not choose the id.
//
// WHAT THE BRIDGE GIVES UP, AND WHAT PHASE 2 RESTORES:
//
//   * Capability separation. The deployed Worker's DELETE accepts the same
//     `x-osl-fetch-token` as GET, so on this path a recipient who can read can
//     also destroy. The destination splits `fetch_cap` (derived from the
//     pointer, so the recipient has it) from `manage_cap` (derived from the
//     sender's send key, so the recipient cannot have it) precisely to make
//     recipient-side deletion impossible. That property is the point of the
//     capability design and it MUST come back — see Phase 2 in
//     `plan-test/tasklogs/B0-01.md`.
//   * Receipts. `POST /v1/blob/:id/ack` does not exist in production (404).
//   * Burn. `prose_token_burn_id` presents `x-osl-manage-cap`, which the
//     deployed Worker rejects with 401. Burn is already broken against
//     production today; the bridge neither regresses nor repairs it.
//
// Nothing in this section should outlive the capability Worker's deployment.

/// Domain separator for the bridge's carrier-derived fetch token. Distinct
/// from every pointer-capability separator so a bridge token can never be
/// mistaken for, or collide with, a real capability.
const BRIDGE_FETCH_INFO: &[u8] = b"osl/bridge/b0-01/fetch-token/v1";

/// Width of the id the deployed Worker assigns: 8 bytes, rendered as the
/// 16 hex chars `CipherStoreClient::upload` validates.
///
/// Public because consumers of a recovered pointer must validate the width the
/// send path actually produces rather than restating one of their own. D-232
/// was exactly that drift: `apps/osl-hub/src/broker.rs` hard-coded the
/// destination Worker's 32-hex width and silently discarded every row this
/// bridge produced.
pub const BRIDGE_ID_BYTES: usize = 8;

/// Whatever the id leaves over in the carrier becomes secret seed material.
/// 96 bits, freshly drawn per message, and it never crosses the network —
/// only its HKDF output does.
const BRIDGE_SEED_BYTES: usize = stego::TOKEN_ID_BYTES - BRIDGE_ID_BYTES;

/// The bridge's read capability, derived from carrier material alone so the
/// receiver needs no key material and no roundtrip — the same property the
/// pointer path has, at 96 rather than 160 bits of input entropy.
fn bridge_fetch_token(seed: &[u8; BRIDGE_SEED_BYTES]) -> [u8; FETCH_TOKEN_BYTES] {
    let hk = Hkdf::<Sha256>::new(None, seed);
    let mut out = [0u8; FETCH_TOKEN_BYTES];
    hk.expand(BRIDGE_FETCH_INFO, &mut out)
        .expect("HKDF expand to 16 bytes is infallible");
    out
}

/// Pack the server's id and our seed into the fixed-width carrier payload.
fn bridge_pack(
    id: &[u8; BRIDGE_ID_BYTES],
    seed: &[u8; BRIDGE_SEED_BYTES],
) -> [u8; stego::TOKEN_ID_BYTES] {
    let mut carrier = [0u8; stego::TOKEN_ID_BYTES];
    carrier[..BRIDGE_ID_BYTES].copy_from_slice(id);
    carrier[BRIDGE_ID_BYTES..].copy_from_slice(seed);
    carrier
}

/// Inverse of [`bridge_pack`]. Total width is pinned by the carrier, so this
/// cannot fail — a carrier that decoded at all is exactly this wide.
fn bridge_unpack(
    carrier: &[u8; stego::TOKEN_ID_BYTES],
) -> ([u8; BRIDGE_ID_BYTES], [u8; BRIDGE_SEED_BYTES]) {
    let mut id = [0u8; BRIDGE_ID_BYTES];
    let mut seed = [0u8; BRIDGE_SEED_BYTES];
    id.copy_from_slice(&carrier[..BRIDGE_ID_BYTES]);
    seed.copy_from_slice(&carrier[BRIDGE_ID_BYTES..]);
    (id, seed)
}

/// Parse the 16-hex id the deployed Worker returns.
fn bridge_id_from_hex(id_hex: &str) -> Result<[u8; BRIDGE_ID_BYTES], ProseTokenError> {
    if id_hex.len() != BRIDGE_ID_BYTES * 2 || !id_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProseTokenError::BadIdHex(id_hex.to_string()));
    }
    let mut out = [0u8; BRIDGE_ID_BYTES];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&id_hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| ProseTokenError::BadIdHex(id_hex.to_string()))?;
    }
    Ok(out)
}

/// Key material the sending layer holds, threaded to the pointer derivation.
///
/// These are separate fields rather than one conversation secret because the
/// three authorities they root are deliberately not interchangeable: whoever
/// holds `send_key` can burn the object, and the recipient must not.
#[derive(Clone, Copy)]
pub struct ProseTokenSendKeys<'a> {
    /// Roots the per-object receipt authority. Both ends of the conversation
    /// derive it; the object id is bound into the derivation, so the resulting
    /// capability is still per-object.
    pub message_key: &'a [u8],
    /// Roots the burn authority. Sender-private: a recipient that could derive
    /// this could destroy the sender's copies.
    pub send_key: &'a [u8],
    /// Roots the delivery tag the store groups undelivered objects by.
    pub conversation_key: &'a [u8],
}

/// Errors that surface from the composite send/recv paths.
#[derive(Debug, thiserror::Error)]
pub enum ProseTokenError {
    #[error("scope: {0}")]
    Scope(#[from] crate::scope::ScopeError),
    #[error("expected DPC0:: wire prefix")]
    NotDpc0Wire,
    #[error("wire body base64 decode failed: {0}")]
    BadBase64(String),
    #[error("cipher-store: {0}")]
    CipherStore(#[from] CipherStoreError),
    #[error("blob id was not 32 hex chars: {0}")]
    BadIdHex(String),
    /// D-232. The object was uploaded under the bridge protocol, whose only
    /// delete authority is the fetch token chosen before upload, and no such
    /// token was recorded beside the id. The object CANNOT be destroyed by this
    /// client. Distinct from every transport error on purpose: a caller must be
    /// able to tell "the network refused" from "we never kept the key to this",
    /// and must never be able to mistake either for success.
    #[error("no burn capability was recorded for blob {0}, so it cannot be destroyed")]
    MissingBurnCapability(String),
    #[error("recorded burn capability was malformed")]
    BadBurnCapability,
    #[error("a secret conversation detection key is required")]
    MissingDetectionKey,
    #[error("pointer capability derivation failed")]
    Capability,
    /// The store echoed an id other than the one this client derived, so the
    /// object it stored is not the object the carrier points at and the burn
    /// ledger would record an id nobody can reach.
    #[error("cipher-store acknowledged a different blob id")]
    BlobIdMismatch,
    #[error("transport object could not be framed for upload")]
    ObjectTooLarge,
    #[error("transport object framing was malformed")]
    MalformedObject,
}

impl From<crypto::Error> for ProseTokenError {
    fn from(_: crypto::Error) -> Self {
        Self::Capability
    }
}

/// Derive the per-conversation MAC key + ConversationCipher used by
/// both encode_token and decode_token. Same scope → same outputs on
/// both sender and receiver — no roundtrip needed.
///
/// Symmetric salt rules (matter for prose-token only; other crypto
/// like the ratchet keeps using `scope.storage_key()`):
///
///   - `dm`: use `dm:<channel_id>` if `channel_id` is set in the
///     ScopeInput, else fall back to `scope.storage_key()` which is
///     `dm:<peer_id>`. The Discord DM channel id is symmetric across
///     both peers; the per-peer storage_key would give EACH side a
///     different salt -> different mac_key -> recipient HMAC fails
///     -> silent decode None. That's the bug that broke cross-user
///     DM decrypt since the Phase 2 prose-token cutover.
///   - `gc` / `server_channel` / `server_full`: scope.storage_key()
///     is already symmetric across peers (uses channel_id /
///     server_id) so no change is needed.
fn derive_scope_cipher(
    scope_input: &ScopeInput,
) -> Result<stego::ConversationCipher, ProseTokenError> {
    let scope = crate::scope::Scope::try_from(scope_input.clone())?;
    let salt = prose_token_salt(&scope);
    Ok(stego::ConversationCipher::from_salt(salt.as_bytes()))
}

/// Derive `K_detect` from shared secret conversation material. It is separate
/// from the `osl/tag/v1` delivery-tag derivation, preserving D-SEP.
pub fn derive_detection_key(
    conversation_epoch_secret: &[u8],
) -> Result<[u8; MAC_KEY_LEN], ProseTokenError> {
    if conversation_epoch_secret.is_empty() {
        return Err(ProseTokenError::MissingDetectionKey);
    }
    let hk = Hkdf::<Sha256>::new(None, conversation_epoch_secret);
    let mut key = [0u8; MAC_KEY_LEN];
    hk.expand(DETECT_KEY_HKDF_INFO, &mut key)
        .expect("HKDF expand to 32 bytes is infallible");
    Ok(key)
}

/// Domain separator that binds one conversation detector to one scope.
pub const DETECT_SCOPE_BIND_HKDF_INFO: &[u8] = b"osl/detect/scope-bind/v1";

/// Bind a conversation's detector to one scope, cryptographically.
///
/// D-231. Before this existed, nothing on the carrier path constrained decode
/// to a scope. `derive_scope_cipher` looks like it does and does not: since the
/// B0-06 bigram pivot `stego::encode_token` ignores its `ConversationCipher`
/// argument outright (`crates/stego/src/mode1.rs:296-320`, the parameter is
/// literally `_cipher`), and `decode_token_bigram` never receives one. The only
/// value keying the carrier was the detector, and the shipping detector
/// (`apps/osl-hub/src/broker.rs:106-110`) is a raw peer-pair X25519 shared
/// secret with no conversation input at all — so one detector covered every
/// conversation, every channel and every service that peer appeared in.
///
/// The fix is a derivation, not a comparison. A comparison would have to run
/// *after* a successful decode, which means after the carrier is already in
/// hand and, on this path, after the cipher-store fetch it authorizes. Binding
/// the key means a foreign-scope cover fails the 32-bit detect tag and produces
/// no carrier, no blob id and no network request.
///
/// The salt is [`prose_token_salt`] — the same value `derive_scope_cipher`
/// uses, so the symmetry rules documented there apply unchanged and both peers
/// of a DM derive the identical bound key. The domain separator is a fixed
/// prefix and the variable salt is the tail, so no two (prefix, salt) pairs can
/// alias.
fn scope_bound_detection_key(
    detection_key: &[u8; MAC_KEY_LEN],
    scope_input: &ScopeInput,
) -> Result<[u8; MAC_KEY_LEN], ProseTokenError> {
    let scope = crate::scope::Scope::try_from(scope_input.clone())?;
    let salt = prose_token_salt(&scope);
    let mut info = Vec::with_capacity(DETECT_SCOPE_BIND_HKDF_INFO.len() + 1 + salt.len());
    info.extend_from_slice(DETECT_SCOPE_BIND_HKDF_INFO);
    info.push(0x00);
    info.extend_from_slice(salt.as_bytes());
    let hk = Hkdf::<Sha256>::new(None, detection_key);
    let mut key = [0u8; MAC_KEY_LEN];
    hk.expand(&info, &mut key)
        .expect("HKDF expand to 32 bytes is infallible");
    Ok(key)
}

/// Domain separator for the sender-private root of the burn authority.
pub const SEND_KEY_HKDF_INFO: &[u8] = b"osl/prose/send-key/v1";

/// Derive the sending device's private `send_key` from its own long-term
/// secret.
///
/// This root must satisfy two things at once. It has to be unavailable to the
/// recipient, or the recipient could burn the sender's objects; and it has to
/// be recomputable at burn time, which happens long after the send, with only
/// the account's own state -- a scope burn walks recorded ids and may have no
/// peer left to resolve. A key derived from the device's identity secret is
/// the only material at hand that is both.
pub fn derive_send_key(identity_secret: &[u8]) -> Result<[u8; MAC_KEY_LEN], ProseTokenError> {
    if identity_secret.is_empty() {
        return Err(ProseTokenError::MissingDetectionKey);
    }
    let hk = Hkdf::<Sha256>::new(None, identity_secret);
    let mut key = [0u8; MAC_KEY_LEN];
    hk.expand(SEND_KEY_HKDF_INFO, &mut key)
        .expect("HKDF expand to 32 bytes is infallible");
    Ok(key)
}

/// Prose-token-specific salt. See [`derive_scope_primitives`] for the
/// rationale — TL;DR: DMs need a symmetric value across peers, and
/// the per-peer `dm:<peer_id>` form `Scope::storage_key()` produces
/// isn't symmetric.
fn prose_token_salt(scope: &crate::scope::Scope) -> String {
    use crate::scope::ScopeKind;
    match scope.kind {
        ScopeKind::Dm => match &scope.channel_id {
            // Heuristic for "real channel_id" vs "TryFrom fallback to
            // peer_id" (line ~207 in scope.rs):
            //   - present, non-empty, AND different from scope.id
            //     (peer_id) -> real DM channel id, symmetric.
            //   - equal to scope.id -> the TryFrom fallback fired
            //     because the JS caller didn't pass channel_id, so
            //     using it would still be asymmetric. Fall back to
            //     storage_key() to preserve pre-fix behaviour for
            //     those legacy callsites (mostly profile-action and
            //     recovery paths that don't drive content encrypt).
            Some(ch) if !ch.is_empty() && ch != &scope.id => format!("dm:{ch}"),
            _ => scope.storage_key(),
        },
        ScopeKind::Gc | ScopeKind::ServerChannel | ScopeKind::ServerFull => scope.storage_key(),
    }
}

/// Parse a canonical lowercase cipher-store blob id.
///
/// The id is the 128-bit value the client derives from `P`, never a
/// server-assigned one, so its exact shape is a client-side invariant.
fn blob_id_hex_to_bytes(id_hex: &str) -> Result<[u8; CAPABILITY_BYTES], ProseTokenError> {
    if id_hex.len() != CAPABILITY_BYTES * 2
        || !id_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProseTokenError::BadIdHex(id_hex.to_string()));
    }
    let mut bytes = [0u8; CAPABILITY_BYTES];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&id_hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| ProseTokenError::BadIdHex(id_hex.to_string()))?;
    }
    Ok(bytes)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Successful send result.
#[derive(Debug, Clone)]
pub struct ProseTokenSendOutput {
    /// Natural-English cover text to post to Discord.
    pub cover_text: String,
    /// The blob ID the store answers to. Caller stashes for burn / lookup.
    /// It is not the pointer: the pointer never leaves the carrier.
    ///
    /// Its width names the protocol that produced it: [`BRIDGE_ID_BYTES`] * 2
    /// hex under the deployed bridge, where the *server* assigns it, and
    /// `CAPABILITY_BYTES` * 2 under the destination capability Worker, where
    /// the client derives it from `P`. Consumers must accept the width their
    /// own send path produces rather than restating one — see D-232.
    pub blob_id: String,
    /// The credential this object can later be **destroyed** with, hex-encoded,
    /// or `None` when the burn authority is recomputable without it.
    ///
    /// D-232. Under the bridge the delete authority is the fetch token, and it
    /// is derived from a `seed` drawn freshly per message and never stored
    /// anywhere else — so a sender that drops this value keeps an id it cannot
    /// authenticate for, and its own burn can never delete. Verified live
    /// against the deployed Worker on 2026-08-04:
    ///
    /// ```text
    /// DELETE /v1/blob/<id>  x-osl-manage-cap: <cap>   -> 401 fetch_token_required
    /// DELETE /v1/blob/<id>  x-osl-fetch-token: <tok>  -> 204
    /// DELETE /v1/blob/<id>  x-osl-fetch-token: <bad>  -> 403 fetch_token_mismatch
    /// ```
    ///
    /// So burn is NOT blocked on deploying the capability Worker; it is blocked
    /// on the sender retaining this. The caller MUST persist it beside the id.
    ///
    /// Under the destination protocol this is `None`: the manage capability is
    /// derived from the sender-private `send_key` and the id, so nothing has to
    /// be kept. That asymmetry is the point — it is why this is an `Option` and
    /// not a required field that Phase 2 would have to fill with a dummy.
    pub burn_capability: Option<String>,
    /// Unix-epoch seconds when the server will delete the blob.
    pub expires_at: i64,
}

/// Successful receive result.
#[derive(Debug, Clone)]
pub struct ProseTokenRecvOutput {
    /// `DPC0::<base64>` wire reconstructed from the fetched cipher.
    /// Caller feeds this into the existing decrypt pipeline.
    pub wire: String,
    /// 32-hex-char blob ID derived from the pointer in the prose. Useful for
    /// matching against a burn-tracking ledger.
    pub blob_id: String,
}

/// Pointer material recovered from a prepared prose carrier without fetching
/// the ciphertext it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseTokenPointer {
    /// The cipher-store id encoded into the carrier under the active bridge
    /// protocol. This is enough to prove the prepared copy still names a
    /// retrievable object; no fetch token or wire bytes are exposed.
    pub blob_id: String,
}

/// Put an already prepared prose-token cover into the visible body of an OSL
/// Mail message.
///
/// OSL Mail deliberately uses the same visible carrier as the chat surfaces:
/// the body is the marker-free cover text itself. Wrapping it with extra prose
/// would change the word stream and make the canonical reader reject it.
pub fn prose_token_mail_body_from_cover(cover_text: &str) -> String {
    cover_text.trim().to_owned()
}

/// Encrypt-and-upload: takes a `DPC0::<base64>` wire string produced by the
/// existing encrypt pipeline, uploads the underlying cipher bytes to the
/// cipher-store under a client-derived id with the chosen TTL, and encodes the
/// pointer that names it as marker-free, chat-like cover text.
///
/// The upload carries only SHA-256 digests of the three capabilities. The
/// store therefore learns which authorities exist without ever holding one,
/// and the pointer that produces the fetch capability never crosses the
/// network at all — it travels in the public prose, readable only by a party
/// that already holds this conversation's detection key.
pub fn prose_token_send(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    keys: ProseTokenSendKeys<'_>,
    dpc0_wire: &str,
    ttl_seconds: u32,
) -> Result<ProseTokenSendOutput, ProseTokenError> {
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir)?;
    let client = CipherStoreClient::new(base_url)?;
    prose_token_send_with_client(
        &client,
        scope_input,
        detection_key,
        keys,
        dpc0_wire,
        ttl_seconds,
    )
}

/// Same send, performed over a caller-supplied cipher-store client.
///
/// The hub's Tor gate uses this after it has resolved a route: when Tor is
/// selected and healthy the client it hands in is SOCKS-only, and when Tor is
/// selected and unhealthy the gate refuses before ever reaching here. Route
/// policy is entirely the caller's; the pointer, capability derivation and
/// client-chosen blob id below are identical on every route.
pub fn prose_token_send_with_client(
    client: &CipherStoreClient,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    keys: ProseTokenSendKeys<'_>,
    dpc0_wire: &str,
    ttl_seconds: u32,
) -> Result<ProseTokenSendOutput, ProseTokenError> {
    let body = dpc0_wire
        .strip_prefix(DPC0_PREFIX)
        .ok_or(ProseTokenError::NotDpc0Wire)?;
    let cipher_bytes = B64
        .decode(body)
        .map_err(|e| ProseTokenError::BadBase64(e.to_string()))?;
    // The store accepts only Padmé lengths, and padding a bare ciphertext
    // would move its AEAD tag, so the object carries its own length.
    let object =
        frame_padded_transport_object(&cipher_bytes).ok_or(ProseTokenError::ObjectTooLarge)?;

    // BRIDGE (B0-01). The destination code this replaced minted a 160-bit
    // pointer, derived the fetch/ack/manage/delivery tuple from it with
    // `derive_capabilities`, and uploaded under a client-chosen id via
    // `upload_pointer`. Production implements none of that: `PUT /v1/blob`
    // 404s and the capability headers are unread. Restoring that call is
    // Phase 2's job and is a deploy plus a grant-minting client, not an edit
    // here. Read the BRIDGE section above before changing anything below.
    //
    // `keys` stays in the signature because Phase 2 needs all three roots
    // back; the bridge protocol simply has nowhere to put them, since the
    // deployed Worker persists one token and no digests.
    let _ = keys;

    // Fresh per message, so two messages in one conversation share no
    // store-visible value and the store cannot link them by id or token.
    let seed_bytes = crypto::random::random_bytes(BRIDGE_SEED_BYTES);
    let seed: [u8; BRIDGE_SEED_BYTES] = seed_bytes
        .try_into()
        .expect("random seed length is fixed by BRIDGE_SEED_BYTES");
    let fetch_token = bridge_fetch_token(&seed);

    // The token must be chosen before the upload — it rides in the upload's
    // own header — and the id is only knowable after it.
    let uploaded = client.upload(&object, ttl_seconds, &fetch_token)?;
    let id = bridge_id_from_hex(&uploaded.id_hex)?;

    let cipher = derive_scope_cipher(scope_input)?;
    // D-231: the scope-bound detector, never the caller's. See
    // `scope_bound_detection_key`.
    let scoped_detector = scope_bound_detection_key(detection_key, scope_input)?;
    let cover_text = stego::encode_token(&cipher, &scoped_detector, &bridge_pack(&id, &seed));

    Ok(ProseTokenSendOutput {
        cover_text,
        blob_id: uploaded.id_hex,
        // D-232. The bridge's delete authority IS this token, and `seed` is
        // gone the moment this function returns. Handing it back is what makes
        // the sender's own burn possible at all.
        burn_capability: Some(hex_lower(&fetch_token)),
        expires_at: uploaded.expires_at,
    })
}

/// Why one cover produced no wire.
///
/// These two used to be the same `Ok(None)`, and that fusion made the eye
/// undiagnosable: a conversation full of retired placeholders (which carry no
/// pointer at all) and a conversation whose blobs had all expired reported
/// exactly the same thing, so "OSL cannot decode this" could never be told apart
/// from "OSL decoded this and the ciphertext is gone".
///
/// PRIVACY: a verdict, never a value. Neither variant carries the cover, the
/// blob id or any part of the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProseTokenMiss {
    /// The cover carried no prose token for this scope. Ordinary chat looks
    /// exactly like this, and so does a retired placeholder. The check is local
    /// and permanent: no fetch was attempted and retrying cannot change it.
    NoToken,
    /// A token decoded, and the cipher store answered a clean 404 for it. The
    /// pointer was real; the ciphertext it named is gone -- burned, or expired
    /// past its TTL. Permanent for this blob, and nothing to retry.
    BlobGone,
}

/// What one `prose_token_recv_classified` produced.
#[derive(Debug)]
pub enum ProseTokenRecv {
    Recovered(ProseTokenRecvOutput),
    Missed(ProseTokenMiss),
}

fn prose_token_decode_carrier(
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    msg: &str,
) -> Result<Option<[u8; stego::TOKEN_ID_BYTES]>, ProseTokenError> {
    let cipher = derive_scope_cipher(scope_input)?;
    // D-231: the scope-bound detector, never the caller's. A cover minted for a
    // different conversation fails the detect tag here and returns `None`, so
    // callers can check prepared text without opening any cipher-store route.
    let scoped_detector = scope_bound_detection_key(detection_key, scope_input)?;
    Ok(stego::decode_token(&cipher, &scoped_detector, msg))
}

/// Recover only the pointer/id encoded in prepared cover text.
///
/// This is the local read-back check for a post copy that OSL is about to hand
/// to a native surface. It deliberately stops before `CipherStoreClient` is
/// constructed: failure here means the prepared visible text no longer carries
/// a recoverable OSL pointer, and success says only that the pointer can be
/// decoded under this scope and detector.
pub fn prose_token_recover_pointer(
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    msg: &str,
) -> Result<Option<ProseTokenPointer>, ProseTokenError> {
    let Some(carrier) = prose_token_decode_carrier(scope_input, detection_key, msg)? else {
        return Ok(None);
    };
    // BRIDGE (B0-01). Under the deployed bridge, the carrier holds the
    // server-assigned id plus the seed that derives the fetch token. The
    // quality check reports only the public id; the fetch token remains local
    // to the receive path that actually opens the ciphertext.
    let (id, _seed) = bridge_unpack(&carrier);
    Ok(Some(ProseTokenPointer {
        blob_id: hex_lower(&id),
    }))
}

/// Try to decode a Discord message as an OSL prose-token, keeping the two
/// distinct reasons a cover can produce nothing apart.
///
/// The decode order is unchanged and so is every verdict: match the cover
/// locally, and only then fetch. What is new is that the caller can tell which
/// step declined. Callers that genuinely do not care use `prose_token_recv`,
/// which folds both misses back into `Ok(None)` exactly as before.
pub fn prose_token_recv_classified(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    msg: &str,
) -> Result<ProseTokenRecv, ProseTokenError> {
    let carrier = match prose_token_decode_carrier(scope_input, detection_key, msg)? {
        Some(bytes) => bytes,
        None => return Ok(ProseTokenRecv::Missed(ProseTokenMiss::NoToken)),
    };
    // BRIDGE (B0-01). The destination read the carrier as a pointer `P` and
    // called `derive_fetch_authority` for both the id and the capability. The
    // deployed Worker assigns ids itself, so the id has to be carried and the
    // capability comes from the rest of the carrier. Still no roundtrip and
    // still no scope metadata in the derivation — see the BRIDGE section above.
    let (id, seed) = bridge_unpack(&carrier);
    let id_hex = hex_lower(&id);
    let fetch_token = bridge_fetch_token(&seed);

    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir)?;
    let client = CipherStoreClient::new(base_url)?;
    let object = match client.fetch_legacy_token(&id_hex, &fetch_token) {
        Ok(b) => b,
        // The one line this split exists for. A clean 404 means the pointer was
        // real and its ciphertext is gone; it is NOT "this was never a token".
        Err(CipherStoreError::NotFound) => {
            return Ok(ProseTokenRecv::Missed(ProseTokenMiss::BlobGone))
        }
        Err(e) => return Err(e.into()),
    };
    let cipher_bytes =
        unframe_padded_transport_object(&object).ok_or(ProseTokenError::MalformedObject)?;
    let wire = format!("{}{}", DPC0_PREFIX, B64.encode(cipher_bytes));

    Ok(ProseTokenRecv::Recovered(ProseTokenRecvOutput {
        wire,
        blob_id: id_hex,
    }))
}

/// Try to decode a Discord message as an OSL prose-token. Returns
/// `Ok(None)` for normal chat (no HMAC match — safe + cheap, can be
/// called on every incoming message); `Ok(Some(...))` for a real
/// token whose cipher was fetched successfully; `Err(...)` for an
/// actual error (network failure, server returned !200 !404).
///
/// Specifically: NotFound from the server is folded into Ok(None)
/// because from the user's perspective there's nothing to render,
/// and the placeholder UX is the caller's concern (Phase 4).
///
/// Callers that need to tell those two apart -- the eye's transcript
/// rehydration does -- use `prose_token_recv_classified` instead. This wrapper
/// exists so every caller that does not is completely unchanged.
pub fn prose_token_recv(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    msg: &str,
) -> Result<Option<ProseTokenRecvOutput>, ProseTokenError> {
    Ok(
        match prose_token_recv_classified(config_dir, scope_input, detection_key, msg)? {
            ProseTokenRecv::Recovered(output) => Some(output),
            ProseTokenRecv::Missed(_) => None,
        },
    )
}

/// Best-effort burn of a single blob ID. Idempotent on the server
/// side — second call is a no-op. Errors surface to the caller so
/// the UI can decide whether to retry / toast.
///
/// The burn presents the sender's manage capability, derived from `send_key`
/// and the recorded id. The pointer is not needed and is usually long gone by
/// the time a scope is burned. Because the authority is rooted in a key only
/// the sender holds, neither a leaked id nor the recipient's copy of the
/// pointer can destroy a conversation's objects.
pub fn prose_token_burn_id(
    config_dir: &std::path::Path,
    send_key: &[u8],
    blob_id: &str,
) -> Result<(), ProseTokenError> {
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir)?;
    let client = CipherStoreClient::new(base_url)?;
    prose_token_burn_id_with_client(&client, send_key, blob_id)
}

/// Same burn, performed over a caller-supplied cipher-store client.
///
/// A burn must take the same route as the upload it destroys. A blob that was
/// uploaded over Tor and then deleted over clearnet correlates the Tor upload
/// with the sender's real address, which is worse than never having used Tor
/// at all -- so the rollback path in the hub's broker hands in the very client
/// its upload used rather than building a fresh direct one.
pub fn prose_token_burn_id_with_client(
    client: &CipherStoreClient,
    send_key: &[u8],
    blob_id: &str,
) -> Result<(), ProseTokenError> {
    let blob_id_bytes = blob_id_hex_to_bytes(blob_id)?;
    let manage_cap = derive_manage_capability(send_key, &blob_id_bytes)?;
    client.burn(blob_id, &manage_cap)?;
    Ok(())
}

/// Parse a recorded bridge burn capability: the hex of a [`FETCH_TOKEN_BYTES`]
/// token. Deliberately strict and deliberately separate from
/// [`blob_id_hex_to_bytes`] — an id is not a credential and the two must never
/// be interchangeable by accident.
fn burn_capability_hex_to_bytes(
    capability_hex: &str,
) -> Result<[u8; FETCH_TOKEN_BYTES], ProseTokenError> {
    if capability_hex.len() != FETCH_TOKEN_BYTES * 2
        || !capability_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProseTokenError::BadBurnCapability);
    }
    let mut bytes = [0u8; FETCH_TOKEN_BYTES];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&capability_hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| ProseTokenError::BadBurnCapability)?;
    }
    Ok(bytes)
}

/// Burn one blob the way the send path that created it can actually be
/// authenticated for.
///
/// **D-232.** This exists because burn was speaking a different protocol from
/// send. `prose_token_burn_id` presents a manage capability derived from the
/// sender's `send_key` and a 32-hex client-derived id — the *destination*
/// capability protocol. The deployed Worker implements the *bridge* protocol:
/// it assigns its own 8-byte id and its DELETE accepts only the fetch token
/// (`401 fetch_token_required` for a manage cap, `204` for the right token —
/// measured live 2026-08-04). So every id the shipping send path recorded was
/// refused locally by the 32-hex parser before a packet was sent, and the
/// hub's panic burn and scope burn destroyed nothing while counting zero.
///
/// The dispatch is on the recorded object's own protocol, never on a width
/// allowlist: a destination-width id burns with the derived manage capability
/// exactly as before, and a bridge-width id burns with the capability the
/// sender persisted at send time.
///
/// **This never silently succeeds.** A recorded object with no usable
/// credential returns [`ProseTokenError::MissingBurnCapability`] rather than
/// `Ok`, so a burn that cannot delete is counted as a failure and reported as
/// one. That is the whole point: the defect was not that burn failed, it was
/// that it failed quietly while the walk reported completion.
pub fn prose_token_burn_recorded_with_client(
    client: &CipherStoreClient,
    send_key: &[u8],
    blob_id: &str,
    burn_capability: Option<&str>,
) -> Result<(), ProseTokenError> {
    // Destination protocol: the id is client-derived, so the manage capability
    // is recomputable and nothing needed to be kept.
    if blob_id.len() == CAPABILITY_BYTES * 2 {
        return prose_token_burn_id_with_client(client, send_key, blob_id);
    }
    // Bridge protocol: the store assigned the id, and the only credential its
    // DELETE honours is the fetch token the sender chose before uploading.
    if blob_id.len() == BRIDGE_ID_BYTES * 2 {
        let _ = bridge_id_from_hex(blob_id)?;
        let capability = burn_capability
            .ok_or_else(|| ProseTokenError::MissingBurnCapability(blob_id.to_string()))?;
        let token = burn_capability_hex_to_bytes(capability)?;
        client.delete(blob_id, &token)?;
        return Ok(());
    }
    Err(ProseTokenError::BadIdHex(blob_id.to_string()))
}

/// Same dispatch, resolving the store client from `config_dir`.
pub fn prose_token_burn_recorded(
    config_dir: &std::path::Path,
    send_key: &[u8],
    blob_id: &str,
    burn_capability: Option<&str>,
) -> Result<(), ProseTokenError> {
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir)?;
    let client = CipherStoreClient::new(base_url)?;
    prose_token_burn_recorded_with_client(&client, send_key, blob_id, burn_capability)
}

#[cfg(test)]
mod tests {
    // BRIDGE (B0-01): the pointer-capability tests below still pin the
    // destination's authority split, which the bridge send path does not yet
    // exercise. They are the standing proof that Phase 2 has something to
    // return to, so they must keep passing.
    use super::*;
    use crate::transport;
    use crypto::pointer::{derive_capabilities, derive_fetch_authority, Pointer};

    /// The retired scope-derived capability key. It is no longer part of any
    /// send or receive path -- that is the point of this migration -- but the
    /// tests below still need a key a party who knows only the *public* scope
    /// could compute, in order to assert what such a party cannot do.
    pub(super) fn public_scope_derived_key(
        scope_input: &ScopeInput,
    ) -> Result<[u8; MAC_KEY_LEN], ProseTokenError> {
        let scope = crate::scope::Scope::try_from(scope_input.clone())?;
        let salt = prose_token_salt(&scope);
        let hk = Hkdf::<Sha256>::new(None, salt.as_bytes());
        let mut key = [0u8; MAC_KEY_LEN];
        hk.expand(PROSE_TOKEN_MAC_HKDF_INFO, &mut key)
            .expect("HKDF expand to 32 bytes is infallible");
        Ok(key)
    }

    #[test]
    fn prepared_post_copy_pointer_recovery_accepts_shaped_copy_and_refuses_plaintext() {
        let scope = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "task-0664-peer".to_owned(),
            server_id: None,
            channel_id: Some("task-0664-dm-channel".to_owned()),
        };
        let detection_key = [0x66u8; MAC_KEY_LEN];
        let cipher = derive_scope_cipher(&scope).expect("scope cipher");
        let scoped_detector =
            scope_bound_detection_key(&detection_key, &scope).expect("scope-bound detector");
        let mut carrier = [0u8; stego::TOKEN_ID_BYTES];
        carrier[..BRIDGE_ID_BYTES].copy_from_slice(&[0x42u8; BRIDGE_ID_BYTES]);
        carrier[BRIDGE_ID_BYTES..].copy_from_slice(&[0x24u8; BRIDGE_SEED_BYTES]);
        let prepared = stego::encode_token(&cipher, &scoped_detector, &carrier);
        let shaped = prepared.replacen(' ', "\n", 1);

        let recovered = prose_token_recover_pointer(&scope, &detection_key, &shaped)
            .expect("local pointer recovery")
            .expect("prepared post copy recovers a pointer");
        println!("TASK0664_RECOVERABLE_POINTER_BLOB_ID={}", recovered.blob_id);
        assert_eq!(
            recovered.blob_id,
            "42".repeat(BRIDGE_ID_BYTES),
            "TASK0664_RECOVERABLE_POINTER_BLOB_ID={}",
            recovered.blob_id
        );
        assert!(prose_token_recover_pointer(
            &scope,
            &detection_key,
            "ordinary prose with no recoverable pointer",
        )
        .expect("ordinary prose is a local miss")
        .is_none());
        println!("TASK0664_UNRECOVERABLE_POINTER_RECOVERED=false");
    }

    /// D-232. The burn dispatch must refuse, loudly and by a distinguishable
    /// error, every recorded object it has no way to destroy. All three cases
    /// below decide before any request is made, which is why this test needs no
    /// network: the point is that they decide at all rather than returning
    /// `Ok`. The deletion that *does* happen is proven live in
    /// `tests/d232_burn_walk_live.rs` — a local test could only ever prove the
    /// refusals.
    #[test]
    fn a_recorded_object_that_cannot_be_destroyed_is_never_reported_destroyed() {
        // Never contacted: every assertion below fails before any request.
        let client = CipherStoreClient::new("http://127.0.0.1:1").expect("unrouted test client");
        let send_key = [0x22u8; 32];
        let bridge_id = "c882e13e918656da";
        let capability = "e120fe7bb3c76b186459a18d115a8885";
        assert_eq!(bridge_id.len(), BRIDGE_ID_BYTES * 2);
        assert_eq!(capability.len(), FETCH_TOKEN_BYTES * 2);

        // The exact pre-D-232 row: a real uploaded object whose delete
        // credential was thrown away. It cannot be destroyed, and saying so is
        // the whole contract.
        match prose_token_burn_recorded_with_client(&client, &send_key, bridge_id, None) {
            Err(ProseTokenError::MissingBurnCapability(id)) => assert_eq!(id, bridge_id),
            other => {
                panic!("a credential-less bridge row must report that it cannot burn: {other:?}")
            }
        }

        // A malformed credential is not a credential.
        for bad in [
            "",
            "abc",
            "E120FE7BB3C76B186459A18D115A8885",
            "zz20fe7bb3c76b186459a18d115a888",
        ] {
            assert!(
                matches!(
                    prose_token_burn_recorded_with_client(&client, &send_key, bridge_id, Some(bad)),
                    Err(ProseTokenError::BadBurnCapability)
                ),
                "a malformed burn capability must be refused: {bad:?}"
            );
        }

        // An id of neither protocol's width is still a hard refusal, so
        // dispatching on width never became "try something and hope".
        for bad in [
            "",
            "c882e13e918656d",
            "c882e13e918656dab",
            "0707070707070707070707070707070",
        ] {
            assert!(
                matches!(
                    prose_token_burn_recorded_with_client(
                        &client,
                        &send_key,
                        bad,
                        Some(capability)
                    ),
                    Err(ProseTokenError::BadIdHex(_))
                ),
                "an id of no known store protocol must be refused: {bad:?}"
            );
        }
    }

    /// The bridge send must hand back the credential its own protocol needs to
    /// delete, because `seed` is unrecoverable once the send returns. This is
    /// a structural pin on the send output, not on the network.
    #[test]
    fn the_bridge_send_yields_a_usable_burn_capability_for_the_id_it_returns() {
        let seed = [0x5au8; BRIDGE_SEED_BYTES];
        let token = bridge_fetch_token(&seed);
        let capability = hex_lower(&token);
        assert_eq!(capability.len(), FETCH_TOKEN_BYTES * 2);
        // It must survive the round trip the ledger puts it through.
        assert_eq!(
            burn_capability_hex_to_bytes(&capability).expect("recorded capability parses"),
            token
        );
        // And it must be the seed's own derivation, not a constant: two seeds,
        // two credentials.
        let other = bridge_fetch_token(&[0x5bu8; BRIDGE_SEED_BYTES]);
        assert_ne!(token, other);
    }

    #[test]
    fn blob_id_hex_round_trip() {
        let id = [0x07; CAPABILITY_BYTES];
        let s = hex_lower(&id);
        assert_eq!(s, "07070707070707070707070707070707");
        assert_eq!(blob_id_hex_to_bytes(&s).unwrap(), id);
    }

    #[test]
    fn blob_id_hex_rejects_bad_input() {
        assert!(blob_id_hex_to_bytes("").is_err());
        assert!(blob_id_hex_to_bytes("zzzz").is_err());
        // The retired server-assigned 16-hex id, which no manage capability
        // matches and which the shipping worker would refuse outright.
        assert!(blob_id_hex_to_bytes("0774c922df450477").is_err());
        assert!(blob_id_hex_to_bytes("0707070707070707070707070707070").is_err()); // 31
        assert!(blob_id_hex_to_bytes("070707070707070707070707070707070").is_err()); // 33
                                                                                     // Uppercase is not the canonical form the store indexes on.
        assert!(blob_id_hex_to_bytes("0707070707070707070707070707070A").is_err());
    }

    /// The carrier must transport the pointer itself, and the store must be
    /// addressed by a value derived from it. Encoding the blob id into the
    /// prose instead would hand the store's own identifier to every observer
    /// holding the detection key, and would make the fetch capability
    /// underivable on the receiving side.
    #[test]
    fn the_carrier_transports_the_pointer_and_not_the_store_identifier() {
        let scope = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "1234567890".to_string(),
            server_id: None,
            channel_id: Some("9999999999999999".to_string()),
        };
        let cipher = derive_scope_cipher(&scope).unwrap();
        let detector = derive_detection_key(&[0x42; 32]).unwrap();
        let pointer = transport::fresh_pointer();
        let authority = derive_fetch_authority(&pointer).unwrap();

        let cover = stego::encode_token(&cipher, &detector, pointer.as_bytes());
        let decoded = stego::decode_token(&cipher, &detector, &cover).expect("carrier decodes");
        assert_eq!(&decoded, pointer.as_bytes());
        assert_eq!(
            derive_fetch_authority(&Pointer::from_bytes(decoded))
                .unwrap()
                .blob_id,
            authority.blob_id,
            "the receiver reaches the sender's object from the carrier alone"
        );
        assert!(
            !cover.contains(&hex_lower(&authority.blob_id)),
            "the public carrier must not spell out the store identifier"
        );
    }

    /// A recipient holds the pointer, so if burn authority followed from it the
    /// recipient could destroy the sender's copies. It must follow from the
    /// send key instead.
    #[test]
    fn burn_authority_does_not_follow_from_the_pointer() {
        let pointer = transport::fresh_pointer();
        let sender = derive_capabilities(&pointer, &[0x11; 32], &[0x22; 32], &[0x33; 32]).unwrap();
        let recipient = derive_fetch_authority(&pointer).unwrap();

        assert_eq!(recipient.blob_id, sender.blob_id);
        assert_eq!(recipient.fetch_cap, sender.fetch_cap);
        assert_ne!(recipient.fetch_cap, sender.manage_cap);
        // What a recipient could try: its own key material over the id it can
        // derive. It does not reproduce the sender's manage capability.
        assert_ne!(
            derive_manage_capability(&recipient.fetch_cap, &recipient.blob_id).unwrap(),
            sender.manage_cap
        );
        assert_eq!(
            derive_manage_capability(&[0x22; 32], &sender.blob_id).unwrap(),
            sender.manage_cap,
            "the sender recovers burn authority from its send key and the recorded id"
        );
    }

    #[test]
    fn derive_is_deterministic_for_shared_secret() {
        let scope = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "1234567890".to_string(),
            server_id: None,
            channel_id: None,
        };
        let c1 = derive_scope_cipher(&scope).unwrap();
        let c2 = derive_scope_cipher(&scope).unwrap();
        let k1 = derive_detection_key(&[0x42; 32]).unwrap();
        let k2 = derive_detection_key(&[0x42; 32]).unwrap();
        assert_eq!(k1, k2);
        // ConversationCipher comparison via encoding the same payload.
        let id = [0u8; stego::TOKEN_ID_BYTES];
        let s1 = stego::encode_token(&c1, &k1, &id);
        let s2 = stego::encode_token(&c2, &k2, &id);
        assert_eq!(s1, s2);
    }

    #[test]
    fn distinct_scopes_yield_distinct_mac_keys() {
        let a = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "111".to_string(),
            server_id: None,
            channel_id: None,
        };
        let b = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "222".to_string(),
            server_id: None,
            channel_id: None,
        };
        let ka = public_scope_derived_key(&a).unwrap();
        let kb = public_scope_derived_key(&b).unwrap();
        assert_ne!(ka, kb);
    }

    /// Cross-peer DM symmetry: each side sees the OTHER user as `id`
    /// but both share the same Discord DM `channel_id`. Phase 6.1
    /// (this commit): mac_keys MUST be identical so the prose-token
    /// cover decodes on the recipient.
    #[test]
    fn dm_with_channel_id_yields_symmetric_mac_keys() {
        // Desktop's view: peer = LAPTOP_ID
        let desktop_view = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "900000000000000001".to_string(), // synthetic device A id
            server_id: None,
            channel_id: Some("9999999999999999".to_string()), // DM channel
        };
        // Laptop's view: peer = DESKTOP_ID
        let laptop_view = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "900000000000000002".to_string(), // synthetic device B id
            server_id: None,
            channel_id: Some("9999999999999999".to_string()), // DM channel
        };
        let ka = public_scope_derived_key(&desktop_view).unwrap();
        let kb = public_scope_derived_key(&laptop_view).unwrap();
        assert_eq!(
            ka, kb,
            "DM mac_keys must be symmetric across peers when channel_id is set"
        );
    }

    /// Pre-fix behaviour preserved: when channel_id is missing /
    /// equals scope.id (TryFrom fallback case), we use storage_key().
    /// Cross-peer derivation in this case is ASYMMETRIC (the pre-fix
    /// bug) — verifying that fallback path so we don't regress legacy
    /// callsites that never passed a real DM channel_id.
    #[test]
    fn dm_without_channel_id_falls_back_to_storage_key() {
        let no_ch = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "111".to_string(),
            server_id: None,
            channel_id: None,
        };
        let ch_eq_id = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "111".to_string(),
            server_id: None,
            channel_id: Some("111".to_string()), // fallback equals id
        };
        let k1 = public_scope_derived_key(&no_ch).unwrap();
        let k2 = public_scope_derived_key(&ch_eq_id).unwrap();
        // Both should produce the same key (both use storage_key()).
        assert_eq!(k1, k2);
    }

    /// GC scopes are already symmetric (both peers share the same
    /// gc channel_id as `scope.id`). Verify channel_id presence
    /// doesn't change anything.
    #[test]
    fn gc_mac_key_unchanged_by_channel_id_presence() {
        let with_ch = ScopeInput {
            kind: crate::scope::ScopeKind::Gc,
            id: "111".to_string(),
            server_id: None,
            channel_id: Some("111".to_string()),
        };
        let without_ch = ScopeInput {
            kind: crate::scope::ScopeKind::Gc,
            id: "111".to_string(),
            server_id: None,
            channel_id: None,
        };
        let k1 = public_scope_derived_key(&with_ch).unwrap();
        let k2 = public_scope_derived_key(&without_ch).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn t1_t31_public_scope_cannot_detect_a_secret_keyed_pointer() {
        let scope = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "public-channel-context".to_owned(),
            server_id: None,
            channel_id: Some("public-channel-context".to_owned()),
        };
        let cipher = derive_scope_cipher(&scope).unwrap();
        let detector = derive_detection_key(&[0x5a; 32]).unwrap();
        let public_scope_key = public_scope_derived_key(&scope).unwrap();
        let pointer = [0x17; stego::TOKEN_ID_BYTES];
        let cover = stego::encode_token(&cipher, &detector, &pointer);

        assert_eq!(
            stego::decode_token(&cipher, &detector, &cover),
            Some(pointer)
        );
        assert_eq!(
            stego::decode_token(&cipher, &public_scope_key, &cover),
            None
        );

        let delivery_tag = Hkdf::<Sha256>::new(None, &[0x5a; 32]);
        let mut delivery = [0u8; MAC_KEY_LEN];
        delivery_tag
            .expand(b"osl/tag/v1", &mut delivery)
            .expect("fixed HKDF output is valid");
        assert_ne!(
            detector, delivery,
            "D-SEP requires independent HKDF outputs"
        );
    }
}

#[cfg(test)]
mod b0_01_scope_isolation {
    //! B0-01 finding: what actually isolates one conversation's cover text
    //! from another's is the *detection key*, not the scope cipher.
    //!
    //! `crates/ipc/tests/prose_token_live.rs::cross_scope_does_not_decode`
    //! asserts the opposite -- "different cipher permutation + different MAC
    //! key" -- while holding the detection key constant across both scopes. It
    //! passed only because the blob id it derived from the recovered carrier
    //! had never been uploaded, so the store answered 404 and the client
    //! folded that to `None`. The assertion was reading a storage miss as a
    //! cryptographic refusal. These two tests pin the real behaviour so the
    //! next reader does not have to rediscover it.
    //!
    //! D-231 UPDATE. Once the bridge made the send path actually upload, the
    //! storage miss stopped happening and `cross_scope_does_not_decode` went
    //! red — correctly. Both findings above still hold verbatim: the scope
    //! cipher still isolates nothing, and the detector is still the only value
    //! that does. What changed is that the detector is now *derived per scope*
    //! (`scope_bound_detection_key`), so the live assertion is satisfied by a
    //! cryptographic refusal for the first time rather than by a 404. The two
    //! tests below are unchanged and still pass, because they exercise the
    //! `stego` layer directly and that layer's behaviour did not change.

    use super::*;
    use crate::scope::{ScopeInput, ScopeKind};

    fn scope(id: &str) -> ScopeInput {
        ScopeInput {
            kind: ScopeKind::Dm,
            id: id.to_string(),
            server_id: None,
            channel_id: None,
        }
    }

    /// The scope cipher permutes word choice, not payload bits: a cover text
    /// encoded under one scope decodes to the identical carrier under another
    /// when the detection key is shared. Not a leak in shipping use -- the
    /// shipping caller derives the detection key from secret conversation
    /// material, so two conversations never share one -- but it does mean the
    /// scope label carries no isolation of its own, and nothing should be
    /// built on the assumption that it does.
    #[test]
    fn scope_cipher_alone_does_not_isolate_the_carrier() {
        let key = derive_detection_key(&[0x44; 32]).unwrap();
        let a = derive_scope_cipher(&scope("scope-a-id")).unwrap();
        let b = derive_scope_cipher(&scope("scope-b-id")).unwrap();
        let carrier = [0x5Au8; stego::TOKEN_ID_BYTES];
        let cover = stego::encode_token(&a, &key, &carrier);
        assert_eq!(stego::decode_token(&a, &key, &cover), Some(carrier));
        assert_eq!(
            stego::decode_token(&b, &key, &cover),
            Some(carrier),
            "the scope cipher does not key the payload; only the detector does"
        );
    }

    /// The property the live test meant to assert, stated against the value
    /// that actually carries it. A distinct conversation secret yields a
    /// distinct detector, and the cover text stops decoding entirely -- no
    /// carrier, no id, and therefore no request to the store at all.
    #[test]
    fn a_distinct_detection_key_is_what_isolates() {
        let mine = derive_detection_key(&[0x44; 32]).unwrap();
        let theirs = derive_detection_key(&[0x77; 32]).unwrap();
        let cipher = derive_scope_cipher(&scope("scope-a-id")).unwrap();
        let carrier = [0x5Au8; stego::TOKEN_ID_BYTES];
        let cover = stego::encode_token(&cipher, &mine, &carrier);
        assert_eq!(stego::decode_token(&cipher, &mine, &cover), Some(carrier));
        assert_eq!(
            stego::decode_token(&cipher, &theirs, &cover),
            None,
            "a foreign conversation must not recover the carrier"
        );
    }

    /// D-231. The property the live test names, proven without a network: one
    /// secret detector, two scopes, two different bound keys.
    #[test]
    fn the_bound_detector_differs_per_scope() {
        let key = derive_detection_key(&[0x44; 32]).unwrap();
        let a = scope_bound_detection_key(&key, &scope("scope-a-id")).unwrap();
        let b = scope_bound_detection_key(&key, &scope("scope-b-id")).unwrap();
        assert_ne!(a, b, "one detector must not cover two scopes");
        assert_ne!(a, key, "the caller's raw detector must not reach stego");
        assert_ne!(b, key);
    }

    /// The refusal is local and total: a cover minted under scope A yields no
    /// carrier at all under scope B, so no blob id is derived and no
    /// cipher-store request can follow. This is the whole reason the fix is a
    /// derivation and not a post-decode comparison.
    #[test]
    fn a_cover_minted_under_one_scope_yields_no_carrier_under_another() {
        let key = derive_detection_key(&[0x44; 32]).unwrap();
        let a = scope("scope-a-id");
        let b = scope("scope-b-id");
        let carrier = [0x5Au8; stego::TOKEN_ID_BYTES];
        let cover = stego::encode_token(
            &derive_scope_cipher(&a).unwrap(),
            &scope_bound_detection_key(&key, &a).unwrap(),
            &carrier,
        );
        assert_eq!(
            stego::decode_token(
                &derive_scope_cipher(&a).unwrap(),
                &scope_bound_detection_key(&key, &a).unwrap(),
                &cover
            ),
            Some(carrier),
            "the minting scope must still read its own carrier"
        );
        assert_eq!(
            stego::decode_token(
                &derive_scope_cipher(&b).unwrap(),
                &scope_bound_detection_key(&key, &b).unwrap(),
                &cover
            ),
            None,
            "a foreign scope must recover nothing, so it never reaches the store"
        );
    }

    /// The binding must not break the property `prose_token_salt` exists for.
    /// Alice and Bob see different `id`s for the same DM and must still derive
    /// the same bound detector, or the fix would silently kill every real DM.
    #[test]
    fn both_peers_of_one_dm_derive_the_same_bound_detector() {
        let key = derive_detection_key(&[0x44; 32]).unwrap();
        let alices_view = ScopeInput {
            kind: ScopeKind::Dm,
            id: "900000000000000001".to_string(),
            server_id: None,
            channel_id: Some("manual-dm-deadbeefdeadbeef".to_string()),
        };
        let bobs_view = ScopeInput {
            kind: ScopeKind::Dm,
            id: "900000000000000002".to_string(),
            server_id: None,
            channel_id: Some("manual-dm-deadbeefdeadbeef".to_string()),
        };
        assert_eq!(
            scope_bound_detection_key(&key, &alices_view).unwrap(),
            scope_bound_detection_key(&key, &bobs_view).unwrap(),
            "a symmetric DM channel binding must produce a symmetric detector"
        );
    }

    /// T1-31 must survive the fix. The bound key mixes a *public* label into a
    /// *secret*; a party holding only the public scope still derives nothing.
    #[test]
    fn binding_the_scope_does_not_make_the_scope_sufficient() {
        let a = scope("public-channel-context");
        let secret = derive_detection_key(&[0x5a; 32]).unwrap();
        let bound = scope_bound_detection_key(&secret, &a).unwrap();
        let carrier = [0x17u8; stego::TOKEN_ID_BYTES];
        let cover = stego::encode_token(&derive_scope_cipher(&a).unwrap(), &bound, &carrier);

        // What an observer who knows only the public scope label can build.
        let public_only = super::tests::public_scope_derived_key(&a).unwrap();
        let public_bound = scope_bound_detection_key(&public_only, &a).unwrap();
        assert_eq!(
            stego::decode_token(&derive_scope_cipher(&a).unwrap(), &public_bound, &cover),
            None,
            "the public scope label must remain insufficient to detect a token"
        );
        assert_eq!(
            stego::decode_token(&derive_scope_cipher(&a).unwrap(), &public_only, &cover),
            None
        );
    }
}

/// The deployed bridge pointer carried by prose-token covers.
///
/// This is deliberately not the destination capability shape. Production sends
/// an 8-byte server-assigned blob id plus a 12-byte seed; the fetch token is
/// derived from that seed at receive time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgePointer {
    pub server_blob_id: [u8; BRIDGE_ID_BYTES],
    pub seed: [u8; BRIDGE_SEED_BYTES],
}

impl BridgePointer {
    pub fn from_carrier(carrier: &[u8; stego::TOKEN_ID_BYTES]) -> Self {
        let (server_blob_id, seed) = bridge_unpack(carrier);
        Self {
            server_blob_id,
            seed,
        }
    }

    pub fn carrier(&self) -> [u8; stego::TOKEN_ID_BYTES] {
        bridge_pack(&self.server_blob_id, &self.seed)
    }

    pub fn blob_id_hex(&self) -> String {
        hex_lower(&self.server_blob_id)
    }

    pub fn fetch_token(&self) -> [u8; FETCH_TOKEN_BYTES] {
        bridge_fetch_token(&self.seed)
    }
}

pub fn prose_token_bridge_pointer(
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    msg: &str,
) -> Result<Option<BridgePointer>, ProseTokenError> {
    let Some(carrier) = prose_token_decode_carrier(scope_input, detection_key, msg)? else {
        return Ok(None);
    };
    Ok(Some(BridgePointer::from_carrier(&carrier)))
}

/// Rebuild the regular `DPC0::<base64>` receive wire from a fetched bridge
/// object. This is the single bridge object opener used after the cipher-store
/// bytes have already been retrieved.
pub fn prose_token_bridge_object_to_wire(object: &[u8]) -> Result<String, ProseTokenError> {
    let cipher_bytes =
        unframe_padded_transport_object(object).ok_or(ProseTokenError::MalformedObject)?;
    Ok(format!("{}{}", DPC0_PREFIX, B64.encode(cipher_bytes)))
}

/// Derive the token the deployed bridge Worker expects from the carrier seed.
pub fn bridge_fetch_token_from_seed(seed: &[u8; BRIDGE_SEED_BYTES]) -> [u8; FETCH_TOKEN_BYTES] {
    bridge_fetch_token(seed)
}
