//! Phase 2 prose-token pivot: composite send/receive helpers that
//! tie together the cipher-store HTTP client, the stego prose-token
//! encoder/decoder, and the per-conversation MAC-key derivation.
//!
//! On the wire:
//!   * Sender: existing PQXDH+ratchet encryption produces a v=4/v=5
//!     wire `DPC0::<base64(cipher)>`. We strip the prefix, decode the
//!     base64, upload the raw bytes to the cipher-store (returns an
//!     8-byte ID), and encode that ID as compact chat-like cover text
//!     via `encode_token`. The cover text is what
//!     gets posted to Discord — no `DPC0::` marker, no high-entropy
//!     base64 blob.
//!   * Receiver: every incoming Discord message in an OSL-enabled
//!     scope runs through `decode_token`. If the HMAC tag validates,
//!     the 8-byte ID is extracted, the cipher fetched from the store,
//!     and re-wrapped as `DPC0::<base64>` so the existing decrypt
//!     pipeline picks it up unchanged.
//!
//! Detection-key derivation: the shipping caller supplies secret conversation
//! material and this module expands it under [`DETECT_KEY_HKDF_INFO`]. Scope
//! labels must never decide whether public cover text carries an OSL pointer.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::cipher_store_client::{
    CipherStoreClient, CipherStoreError, UploadResult, FETCH_TOKEN_BYTES,
};
use crate::scope::ScopeInput;

const DPC0_PREFIX: &str = "DPC0::";
const MAC_KEY_LEN: usize = 32;
pub const PROSE_TOKEN_MAC_HKDF_INFO: &[u8] = b"discord-privacy-client/prose-token/mac-key/v1";
/// Domain separator for the private pointer-detection key in transport §6b.
pub const DETECT_KEY_HKDF_INFO: &[u8] = b"osl/detect/v1";

/// Phase 6 capability-token domain separator. The token is
/// HMAC-SHA256(mac_key, FETCH_TOKEN_INFO || blob_id_bytes)[..16];
/// truncated to 128 bits, kept the same length on both ends.
const FETCH_TOKEN_INFO: &[u8] = b"discord-privacy-client/cipher-store/fetch-token/v1";

/// Derive the Phase 6 cipher-store fetch capability token from the
/// per-conversation MAC key + the blob's 8-byte ID. Same scope + same
/// blob_id on sender and receiver → same token, no roundtrip.
fn derive_fetch_token(
    mac_key: &[u8; MAC_KEY_LEN],
    blob_id: &[u8; stego::TOKEN_ID_BYTES],
) -> [u8; FETCH_TOKEN_BYTES] {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(mac_key).expect("HMAC-SHA256 accepts any key length");
    mac.update(FETCH_TOKEN_INFO);
    mac.update(blob_id);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; FETCH_TOKEN_BYTES];
    out.copy_from_slice(&result[..FETCH_TOKEN_BYTES]);
    out
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
    #[error("blob id was not 16 hex chars: {0}")]
    BadIdHex(String),
    #[error("a secret conversation detection key is required")]
    MissingDetectionKey,
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

// T6-R3 replaces this temporary legacy capability key with P-derived
// authority. It is deliberately separate from the private detector above.
fn legacy_scope_fetch_key(scope_input: &ScopeInput) -> Result<[u8; MAC_KEY_LEN], ProseTokenError> {
    let scope = crate::scope::Scope::try_from(scope_input.clone())?;
    let salt = prose_token_salt(&scope);
    let hk = Hkdf::<Sha256>::new(None, salt.as_bytes());
    let mut key = [0u8; MAC_KEY_LEN];
    hk.expand(PROSE_TOKEN_MAC_HKDF_INFO, &mut key)
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

fn id_hex_to_bytes(id_hex: &str) -> Result<[u8; stego::TOKEN_ID_BYTES], ProseTokenError> {
    if id_hex.len() != stego::TOKEN_ID_BYTES * 2 || !id_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ProseTokenError::BadIdHex(id_hex.to_string()));
    }
    let mut bytes = [0u8; stego::TOKEN_ID_BYTES];
    for i in 0..stego::TOKEN_ID_BYTES {
        bytes[i] = u8::from_str_radix(&id_hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| ProseTokenError::BadIdHex(id_hex.to_string()))?;
    }
    Ok(bytes)
}

fn bytes_to_id_hex(bytes: &[u8; stego::TOKEN_ID_BYTES]) -> String {
    let mut s = String::with_capacity(16);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Successful send result.
#[derive(Debug, Clone)]
pub struct ProseTokenSendOutput {
    /// Natural-English cover text to post to Discord.
    pub cover_text: String,
    /// 16-hex-char blob ID. Caller stashes for burn / lookup.
    pub blob_id: String,
    /// Unix-epoch seconds when the server will delete the blob.
    pub expires_at: i64,
}

/// Successful receive result.
#[derive(Debug, Clone)]
pub struct ProseTokenRecvOutput {
    /// `DPC0::<base64>` wire reconstructed from the fetched cipher.
    /// Caller feeds this into the existing decrypt pipeline.
    pub wire: String,
    /// 16-hex-char blob ID extracted from the prose. Useful for
    /// matching against a burn-tracking ledger.
    pub blob_id: String,
}

/// Encrypt-and-upload: takes a `DPC0::<base64>` wire string produced
/// by the existing encrypt pipeline, uploads the underlying cipher
/// bytes to the cipher-store with the chosen TTL, and encodes the
/// returned blob ID as marker-free, chat-like cover text.
pub fn prose_token_send(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    dpc0_wire: &str,
    ttl_seconds: u32,
) -> Result<ProseTokenSendOutput, ProseTokenError> {
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    prose_token_send_with_client(&client, scope_input, detection_key, dpc0_wire, ttl_seconds)
}

pub fn prose_token_send_with_client(
    client: &CipherStoreClient,
    scope_input: &ScopeInput,
    detection_key: &[u8; MAC_KEY_LEN],
    dpc0_wire: &str,
    ttl_seconds: u32,
) -> Result<ProseTokenSendOutput, ProseTokenError> {
    let body = dpc0_wire
        .strip_prefix(DPC0_PREFIX)
        .ok_or(ProseTokenError::NotDpc0Wire)?;
    let cipher_bytes = B64
        .decode(body)
        .map_err(|e| ProseTokenError::BadBase64(e.to_string()))?;

    // Phase 6: derive a per-conversation capability token and pass it
    // to the worker on upload. Since the blob_id is server-assigned
    // and the token has to be in the upload POST itself, the token's
    // input is fixed to (mac_key, FETCH_TOKEN_INFO || zeros) — the
    // "blob_id" position holds a constant placeholder, making the
    // token effectively per-scope rather than per-blob.
    //
    // This is the intended design, not a workaround:
    // - Every recipient in the conversation has mac_key (it's
    //   HKDF-derived from the public scope id, the same way the cover
    //   HMAC tag is derived). So per-blob granularity wouldn't add
    //   any access-control distinction inside the scope — anyone with
    //   read access to one blob has read access to all blobs.
    // - The token blocks fetch/delete by a bare leaked blob_id. It is
    //   not an identity or membership credential: mac_key is derived
    //   from public scope metadata, so a party that also knows the scope
    //   can compute it. The worker returns 403 only when the token is
    //   absent or different.
    // - It does NOT defend against a compromised cipher-store
    //   operator who can read DB rows directly. That requires
    //   Privacy-Pass-style blind tokens (deferred).
    let cipher = derive_scope_cipher(scope_input)?;
    let fetch_key = legacy_scope_fetch_key(scope_input)?;
    let fetch_token = derive_fetch_token(&fetch_key, &[0u8; stego::TOKEN_ID_BYTES]);

    let UploadResult { id_hex, expires_at } =
        client.upload(&cipher_bytes, ttl_seconds, &fetch_token)?;

    let id_bytes = id_hex_to_bytes(&id_hex)?;
    let cover_text = stego::encode_token(&cipher, detection_key, &id_bytes);

    Ok(ProseTokenSendOutput {
        cover_text,
        blob_id: id_hex,
        expires_at,
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
pub enum ProseTokenRecv {
    Recovered(ProseTokenRecvOutput),
    Missed(ProseTokenMiss),
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
    let cipher = derive_scope_cipher(scope_input)?;
    let id_bytes = match stego::decode_token(&cipher, detection_key, msg) {
        Some(id) => id,
        None => return Ok(ProseTokenRecv::Missed(ProseTokenMiss::NoToken)),
    };
    let id_hex = bytes_to_id_hex(&id_bytes);

    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    // Phase 6: present the same scope-derived fetch token the sender
    // used at upload. derive_fetch_token is deterministic over
    // (mac_key, [0u8; 8]) so sender + receiver produce identical
    // tokens without any wire roundtrip.
    let fetch_key = legacy_scope_fetch_key(scope_input)?;
    let fetch_token = derive_fetch_token(&fetch_key, &[0u8; stego::TOKEN_ID_BYTES]);
    let cipher_bytes = match client.fetch(&id_hex, &fetch_token) {
        Ok(b) => b,
        // The one line this split exists for. A clean 404 means the pointer was
        // real and its ciphertext is gone; it is NOT "this was never a token".
        Err(CipherStoreError::NotFound) => {
            return Ok(ProseTokenRecv::Missed(ProseTokenMiss::BlobGone))
        }
        Err(e) => return Err(e.into()),
    };
    let wire = format!("{}{}", DPC0_PREFIX, B64.encode(&cipher_bytes));

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
/// Phase 6: needs `scope_input` to derive the capability token the
/// worker requires for DELETE. Without the token, the worker 401s
/// (which protects against blob_id-leak DoS where an outsider with a
/// blob_id could otherwise nuke a conversation's covers).
pub fn prose_token_burn_id(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    blob_id: &str,
) -> Result<(), ProseTokenError> {
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    let fetch_key = legacy_scope_fetch_key(scope_input)?;
    let fetch_token = derive_fetch_token(&fetch_key, &[0u8; stego::TOKEN_ID_BYTES]);
    client.delete(blob_id, &fetch_token)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_hex_round_trip() {
        let id: [u8; stego::TOKEN_ID_BYTES] = [0x07; stego::TOKEN_ID_BYTES];
        let s = bytes_to_id_hex(&id);
        assert_eq!(s, "0707070707070707070707070707070707070707");
        assert_eq!(id_hex_to_bytes(&s).unwrap(), id);
    }

    #[test]
    fn id_hex_rejects_bad_input() {
        assert!(id_hex_to_bytes("").is_err());
        assert!(id_hex_to_bytes("zzzz").is_err());
        assert!(id_hex_to_bytes("0774c922df45047").is_err()); // 15 chars
        assert!(id_hex_to_bytes("0774c922df45047fab").is_err()); // 18 chars
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
        let ka = legacy_scope_fetch_key(&a).unwrap();
        let kb = legacy_scope_fetch_key(&b).unwrap();
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
        let ka = legacy_scope_fetch_key(&desktop_view).unwrap();
        let kb = legacy_scope_fetch_key(&laptop_view).unwrap();
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
        let k1 = legacy_scope_fetch_key(&no_ch).unwrap();
        let k2 = legacy_scope_fetch_key(&ch_eq_id).unwrap();
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
        let k1 = legacy_scope_fetch_key(&with_ch).unwrap();
        let k2 = legacy_scope_fetch_key(&without_ch).unwrap();
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
        let public_scope_key = legacy_scope_fetch_key(&scope).unwrap();
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
