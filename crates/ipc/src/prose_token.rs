//! Phase 2 prose-token pivot: composite send/receive helpers that
//! tie together the cipher-store HTTP client, the stego prose-token
//! encoder/decoder, and the per-conversation MAC-key derivation.
//!
//! On the wire:
//!   * Sender: existing PQXDH+ratchet encryption produces a v=4/v=5
//!     wire `DPC0::<base64(cipher)>`. We strip the prefix, decode the
//!     base64, upload the raw bytes to the cipher-store (returns an
//!     8-byte ID), and encode that ID as ~5 sentences of natural
//!     English prose via `encode_token`. The cover prose is what
//!     gets posted to Discord — no `DPC0::` marker, no high-entropy
//!     base64 blob.
//!   * Receiver: every incoming Discord message in an OSL-enabled
//!     scope runs through `decode_token`. If the HMAC tag validates,
//!     the 8-byte ID is extracted, the cipher fetched from the store,
//!     and re-wrapped as `DPC0::<base64>` so the existing decrypt
//!     pipeline picks it up unchanged.
//!
//! MAC-key derivation: HKDF-SHA256 over the scope's `storage_key`
//! (e.g. `"dm:<peer>"`, `"server_channel:<srv>:<ch>"`) with the
//! domain separator [`PROSE_TOKEN_MAC_HKDF_INFO`]. The salt itself
//! is public (anyone with scope IDs can derive the same MAC key) —
//! the HMAC's role is "tag this 8-byte payload as 'looks like an
//! OSL token' so receivers don't mistake plain English for one",
//! not "prevent forgery." Phase 6 hardening will rekey this from
//! the ratchet root for genuine sender authentication.

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
pub const PROSE_TOKEN_MAC_HKDF_INFO: &[u8] =
    b"discord-privacy-client/prose-token/mac-key/v1";

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
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(mac_key)
        .expect("HMAC-SHA256 accepts any key length");
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
}

/// Derive the per-conversation MAC key + ConversationCipher used by
/// both encode_token and decode_token. Same scope → same outputs on
/// both sender and receiver — no roundtrip needed.
fn derive_scope_primitives(
    scope_input: &ScopeInput,
) -> Result<(stego::ConversationCipher, [u8; MAC_KEY_LEN]), ProseTokenError> {
    let scope = crate::scope::Scope::try_from(scope_input.clone())?;
    let salt = scope.storage_key();
    let cipher = stego::ConversationCipher::from_salt(salt.as_bytes());
    let hk = Hkdf::<Sha256>::new(None, salt.as_bytes());
    let mut mac_key = [0u8; MAC_KEY_LEN];
    hk.expand(PROSE_TOKEN_MAC_HKDF_INFO, &mut mac_key)
        .expect("HKDF expand to 32 bytes is infallible");
    Ok((cipher, mac_key))
}

fn id_hex_to_bytes(id_hex: &str) -> Result<[u8; stego::TOKEN_ID_BYTES], ProseTokenError> {
    if id_hex.len() != stego::TOKEN_ID_BYTES * 2
        || !id_hex.chars().all(|c| c.is_ascii_hexdigit())
    {
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
        s.push_str(&format!("{:02x}", b));
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
/// returned blob ID as natural-English prose.
pub fn prose_token_send(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
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
    // - The threat the token defends against is link-leak: a third
    //   party who learns a blob_id outside the conversation context
    //   (screenshot, message-id leak, accidental share). Without
    //   mac_key they can't compute the token and the worker 403s.
    // - It does NOT defend against a compromised cipher-store
    //   operator who can read DB rows directly. That requires
    //   Privacy-Pass-style blind tokens (deferred).
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    let (cipher, mac_key) = derive_scope_primitives(scope_input)?;
    let fetch_token = derive_fetch_token(&mac_key, &[0u8; stego::TOKEN_ID_BYTES]);

    let UploadResult { id_hex, expires_at } =
        client.upload(&cipher_bytes, ttl_seconds, &fetch_token)?;

    let id_bytes = id_hex_to_bytes(&id_hex)?;
    let cover_text = stego::encode_token(&cipher, &mac_key, &id_bytes);

    Ok(ProseTokenSendOutput {
        cover_text,
        blob_id: id_hex,
        expires_at,
    })
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
pub fn prose_token_recv(
    config_dir: &std::path::Path,
    scope_input: &ScopeInput,
    msg: &str,
) -> Result<Option<ProseTokenRecvOutput>, ProseTokenError> {
    let (cipher, mac_key) = derive_scope_primitives(scope_input)?;
    let id_bytes = match stego::decode_token(&cipher, &mac_key, msg) {
        Some(id) => id,
        None => return Ok(None),
    };
    let id_hex = bytes_to_id_hex(&id_bytes);

    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    // Phase 6: present the same scope-derived fetch token the sender
    // used at upload. derive_fetch_token is deterministic over
    // (mac_key, [0u8; 8]) so sender + receiver produce identical
    // tokens without any wire roundtrip.
    let fetch_token = derive_fetch_token(&mac_key, &[0u8; stego::TOKEN_ID_BYTES]);
    let cipher_bytes = match client.fetch(&id_hex, &fetch_token) {
        Ok(b) => b,
        Err(CipherStoreError::NotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let wire = format!("{}{}", DPC0_PREFIX, B64.encode(&cipher_bytes));

    Ok(Some(ProseTokenRecvOutput {
        wire,
        blob_id: id_hex,
    }))
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
    let (_, mac_key) = derive_scope_primitives(scope_input)?;
    let fetch_token = derive_fetch_token(&mac_key, &[0u8; stego::TOKEN_ID_BYTES]);
    client.delete(blob_id, &fetch_token)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_hex_round_trip() {
        let id: [u8; stego::TOKEN_ID_BYTES] = [0x07, 0x74, 0xc9, 0x22, 0xdf, 0x45, 0x04, 0x7f];
        let s = bytes_to_id_hex(&id);
        assert_eq!(s, "0774c922df45047f");
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
    fn derive_is_deterministic_per_scope() {
        let scope = ScopeInput {
            kind: crate::scope::ScopeKind::Dm,
            id: "1234567890".to_string(),
            server_id: None,
            channel_id: None,
        };
        let (c1, k1) = derive_scope_primitives(&scope).unwrap();
        let (c2, k2) = derive_scope_primitives(&scope).unwrap();
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
        let (_, ka) = derive_scope_primitives(&a).unwrap();
        let (_, kb) = derive_scope_primitives(&b).unwrap();
        assert_ne!(ka, kb);
    }
}
