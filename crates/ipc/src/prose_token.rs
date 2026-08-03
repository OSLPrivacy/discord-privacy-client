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
//!     text is what gets posted to Discord — no `DPC0::` marker, no
//!     high-entropy base64 blob, and no server-assigned identifier.
//!   * Receiver: every incoming Discord message in an OSL-enabled
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

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use crypto::pointer::{
    derive_capabilities, derive_fetch_authority, derive_manage_capability, Pointer,
    CAPABILITY_BYTES, POINTER_BYTES,
};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::cipher_store_client::{
    BlobCapabilities, BlobObjectClass, CipherStoreClient, CipherStoreError,
};
use crate::scope::ScopeInput;
use crate::transport;
use crate::transport_padding::{frame_padded_transport_object, unframe_padded_transport_object};

const DPC0_PREFIX: &str = "DPC0::";
const MAC_KEY_LEN: usize = 32;
pub const PROSE_TOKEN_MAC_HKDF_INFO: &[u8] = b"discord-privacy-client/prose-token/mac-key/v1";
/// Domain separator for the private pointer-detection key in transport §6b.
pub const DETECT_KEY_HKDF_INFO: &[u8] = b"osl/detect/v1";

/// The carrier transports the pointer and only the pointer. T1-32 widened the
/// prose carrier to the 160-bit seed for exactly this reason, so a drift
/// between the two constants must not compile.
const _: () = assert!(stego::TOKEN_ID_BYTES == POINTER_BYTES);

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
    /// 32-hex-char client-derived blob ID. Caller stashes for burn / lookup.
    /// It is not the pointer: the pointer never leaves the carrier.
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
    /// 32-hex-char blob ID derived from the pointer in the prose. Useful for
    /// matching against a burn-tracking ledger.
    pub blob_id: String,
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
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
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

    // A fresh 160-bit seed per message. Two messages in one conversation share
    // no store-visible value except the delivery tag, so the store cannot link
    // them by id, capability digest or object key.
    let pointer = transport::fresh_pointer();
    let capabilities = derive_capabilities(
        &pointer,
        keys.message_key,
        keys.send_key,
        keys.conversation_key,
    )?;
    let blob_id_hex = hex_lower(&capabilities.blob_id);

    let uploaded = client.upload_pointer(
        &object,
        ttl_seconds,
        &capabilities.blob_id,
        BlobCapabilities {
            fetch_cap: capabilities.fetch_cap,
            ack_cap: capabilities.ack_cap,
            manage_cap: capabilities.manage_cap,
            delivery_tag: capabilities.delivery_tag,
        },
        // One fan-out copy per recipient: a receipt destroys this copy.
        BlobObjectClass::SingleAck,
    )?;
    // The id is ours, not the server's. If the store answers with a different
    // one, the object it kept is not the object the carrier points at, and the
    // burn ledger would record an id no manage capability matches.
    if uploaded.id_hex != blob_id_hex {
        return Err(ProseTokenError::BlobIdMismatch);
    }

    let cipher = derive_scope_cipher(scope_input)?;
    let cover_text = stego::encode_token(&cipher, detection_key, pointer.as_bytes());

    Ok(ProseTokenSendOutput {
        cover_text,
        blob_id: blob_id_hex,
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
    let pointer = match stego::decode_token(&cipher, detection_key, msg) {
        Some(seed) => Pointer::from_bytes(seed),
        None => return Ok(ProseTokenRecv::Missed(ProseTokenMiss::NoToken)),
    };
    // The pointer is the whole of the receiver's authority: the id it must ask
    // for and the capability that reads it, both from `P` and nothing else. No
    // roundtrip, and no scope metadata anywhere in the derivation.
    let authority = derive_fetch_authority(&pointer)?;
    let id_hex = hex_lower(&authority.blob_id);

    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    let object = match client.fetch(&id_hex, &authority.fetch_cap) {
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
    let blob_id_bytes = blob_id_hex_to_bytes(blob_id)?;
    let manage_cap = derive_manage_capability(send_key, &blob_id_bytes)?;
    let base_url = crate::cipher_store_client::resolve_cipher_store_base_url(config_dir);
    let client = CipherStoreClient::new(base_url)?;
    client.burn(blob_id, &manage_cap)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired scope-derived capability key. It is no longer part of any
    /// send or receive path -- that is the point of this migration -- but the
    /// tests below still need a key a party who knows only the *public* scope
    /// could compute, in order to assert what such a party cannot do.
    fn public_scope_derived_key(
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
