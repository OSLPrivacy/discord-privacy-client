//! Pure functions backing the Tauri command surface.
//!
//! These take an explicit [`AppState`] reference plus primitive
//! arguments, return [`IpcResult`], and contain no Tauri-specific
//! glue. Unit tests exercise them directly.
//!
//! Tauri-attribute wrappers live in [`crate::tauri_glue`].

use crate::state::AppState;
use crate::{IpcError, IpcResult};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{aead, hkdf, random, x25519};
use keystore::{generate_identity, select_best_sealer, KeyServerClient};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---- DTOs ----

#[derive(Debug, Serialize)]
pub struct GenerateIdentityResponse {
    pub user_id: String,
    pub ik_x25519_pub_b64: String,
    pub ik_mlkem768_pub_b64: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub initial_registration: bool,
    pub registered_at: Option<String>,
    pub last_rotated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FetchPubkeysResponse {
    pub user_id: String,
    pub ik_x25519_pub_b64: String,
    pub ik_mlkem768_pub_b64: String,
    pub registered_at: String,
    pub last_rotated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AeadSealRequest {
    pub key_b64: String,
    pub nonce_b64: String,
    pub ad_b64: Option<String>,
    pub plaintext_b64: String,
}

#[derive(Debug, Serialize)]
pub struct AeadSealResponse {
    pub ciphertext_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct AeadOpenRequest {
    pub key_b64: String,
    pub nonce_b64: String,
    pub ad_b64: Option<String>,
    pub ciphertext_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct StegoEncodeRequest {
    pub ciphertext_b64: String,
}

#[derive(Debug, Serialize)]
pub struct StegoEncodeResponse {
    pub stego_message: String,
}

#[derive(Debug, Serialize)]
pub struct StegoDecodeResponse {
    pub ciphertext_b64: String,
}

// ---- helpers ----

fn b64_to_array<const N: usize>(field: &str, b: &str) -> IpcResult<[u8; N]> {
    let v = STANDARD.decode(b)?;
    if v.len() != N {
        return Err(IpcError::InvalidArgument(format!(
            "{field}: expected {N} bytes, got {}",
            v.len()
        )));
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&v);
    Ok(out)
}

fn b64_to_vec(b: &str) -> IpcResult<Vec<u8>> {
    Ok(STANDARD.decode(b)?)
}

// ---- identity ----

/// Generate a fresh [`keystore::Identity`] in [`AppState`] and return
/// its public bytes. Overwrites any previously-loaded identity.
pub fn cmd_generate_identity(
    state: &AppState,
    user_id: String,
) -> IpcResult<GenerateIdentityResponse> {
    if user_id.trim().is_empty() {
        return Err(IpcError::InvalidArgument("user_id must be non-empty".into()));
    }
    let identity = generate_identity(user_id.clone());
    let resp = GenerateIdentityResponse {
        user_id: identity.user_id.clone(),
        ik_x25519_pub_b64: STANDARD.encode(identity.x25519_public.as_bytes()),
        ik_mlkem768_pub_b64: STANDARD.encode(identity.mlkem_public_bytes),
    };
    *state.identity.lock().expect("identity mutex poisoned") = Some(identity);
    Ok(resp)
}

pub fn cmd_load_identity(state: &AppState, path: String) -> IpcResult<GenerateIdentityResponse> {
    let sealer = select_best_sealer();
    let id = keystore::load_identity(&PathBuf::from(path), sealer.as_ref())?;
    let resp = GenerateIdentityResponse {
        user_id: id.user_id.clone(),
        ik_x25519_pub_b64: STANDARD.encode(id.x25519_public.as_bytes()),
        ik_mlkem768_pub_b64: STANDARD.encode(id.mlkem_public_bytes),
    };
    *state.identity.lock().expect("identity mutex poisoned") = Some(id);
    Ok(resp)
}

pub fn cmd_save_identity(state: &AppState, path: String) -> IpcResult<()> {
    let guard = state.identity.lock().expect("identity mutex poisoned");
    let id = guard.as_ref().ok_or(IpcError::IdentityMissing)?;
    let sealer = select_best_sealer();
    keystore::save_identity(&PathBuf::from(path), id, sealer.as_ref())?;
    Ok(())
}

// ---- key server ----

pub fn cmd_init_keyserver(state: &AppState, base_url: String) -> IpcResult<()> {
    let client = KeyServerClient::new(base_url)?;
    *state.keyserver.lock().expect("keyserver mutex poisoned") = Some(client);
    Ok(())
}

pub fn cmd_register(state: &AppState) -> IpcResult<RegisterResponse> {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard.as_ref().ok_or(IpcError::IdentityMissing)?;
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard.as_ref().ok_or(IpcError::KeyserverMissing)?;
    let resp = client.register(identity)?;
    Ok(RegisterResponse {
        user_id: resp.user_id,
        initial_registration: resp.registered_at.is_some(),
        registered_at: resp.registered_at,
        last_rotated_at: resp.last_rotated_at,
    })
}

pub fn cmd_fetch_pubkeys(state: &AppState, user_id: String) -> IpcResult<FetchPubkeysResponse> {
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard.as_ref().ok_or(IpcError::KeyserverMissing)?;
    let resp = client.fetch_pubkeys(&user_id)?;
    Ok(FetchPubkeysResponse {
        user_id: resp.user_id,
        ik_x25519_pub_b64: resp.ik_x25519_pub,
        ik_mlkem768_pub_b64: resp.ik_mlkem768_pub,
        registered_at: resp.registered_at,
        last_rotated_at: resp.last_rotated_at,
    })
}

// ---- AEAD primitive ----

pub fn cmd_aead_seal(req: AeadSealRequest) -> IpcResult<AeadSealResponse> {
    let key = aead::Key::from_bytes(b64_to_array::<{ aead::KEY_SIZE }>("key_b64", &req.key_b64)?);
    let nonce =
        aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>("nonce_b64", &req.nonce_b64)?);
    let ad = match req.ad_b64.as_deref() {
        Some(a) => b64_to_vec(a)?,
        None => Vec::new(),
    };
    let plaintext = b64_to_vec(&req.plaintext_b64)?;
    let ct = aead::seal(&key, &nonce, &ad, &plaintext)?;
    Ok(AeadSealResponse {
        ciphertext_b64: STANDARD.encode(&ct),
    })
}

pub fn cmd_aead_open(req: AeadOpenRequest) -> IpcResult<AeadSealResponse> {
    let key = aead::Key::from_bytes(b64_to_array::<{ aead::KEY_SIZE }>("key_b64", &req.key_b64)?);
    let nonce =
        aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>("nonce_b64", &req.nonce_b64)?);
    let ad = match req.ad_b64.as_deref() {
        Some(a) => b64_to_vec(a)?,
        None => Vec::new(),
    };
    let ciphertext = b64_to_vec(&req.ciphertext_b64)?;
    let pt = aead::open(&key, &nonce, &ad, &ciphertext)?;
    // Reuse the seal-response shape — it's just `{ ciphertext_b64 }` —
    // but the field name is generic enough to carry recovered
    // plaintext too. JS callers should treat this as opaque bytes.
    Ok(AeadSealResponse {
        ciphertext_b64: STANDARD.encode(&pt),
    })
}

// ---- stego ----

pub fn cmd_stego_encode(req: StegoEncodeRequest) -> IpcResult<StegoEncodeResponse> {
    let ciphertext = b64_to_vec(&req.ciphertext_b64)?;
    let s = stego::encode_mode0(&ciphertext)?;
    Ok(StegoEncodeResponse { stego_message: s })
}

pub fn cmd_stego_decode(stego_message: String) -> IpcResult<StegoDecodeResponse> {
    let bytes = stego::decode_mode0(&stego_message)?;
    Ok(StegoDecodeResponse {
        ciphertext_b64: STANDARD.encode(&bytes),
    })
}

// ---- introspection ----

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub identity_loaded: bool,
    pub keyserver_initialised: bool,
    pub user_id: Option<String>,
    pub x25519_public_b64: Option<String>,
}

pub fn cmd_status(state: &AppState) -> StatusResponse {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let id_ref = id_guard.as_ref();
    StatusResponse {
        identity_loaded: id_ref.is_some(),
        keyserver_initialised: state.has_keyserver(),
        user_id: id_ref.map(|i| i.user_id.clone()),
        x25519_public_b64: id_ref.map(|i| STANDARD.encode(i.x25519_public.as_bytes())),
    }
}

// X25519 helper (used by tests + for the eventual ratchet-handshake
// command surface). Kept here so the IPC tests can verify the X25519
// glue end-to-end without re-importing the whole crypto crate API.
pub fn cmd_x25519_diffie_hellman(
    secret_b64: String,
    peer_public_b64: String,
) -> IpcResult<String> {
    let secret = x25519::SecretKey::from_bytes(b64_to_array::<{ x25519::SECRET_KEY_SIZE }>(
        "secret_b64",
        &secret_b64,
    )?);
    let peer = x25519::PublicKey::from_bytes(b64_to_array::<{ x25519::PUBLIC_KEY_SIZE }>(
        "peer_public_b64",
        &peer_public_b64,
    )?);
    let shared = x25519::diffie_hellman(&secret, &peer)?;
    Ok(STANDARD.encode(shared.as_bytes()))
}

// ---- Layer 10 / Phase 4: osl_encrypt_message pipeline ----
//
// Wire format (post-stego-decode bytes; the outer `DPC0::` prefix is
// applied by `stego::encode_mode0`):
//
// ```text
// [
//   version:    u8 = 0x01     // hard-coded; future formats bump this
//   N:          u8            // recipient count, 1..=255
//   per-recipient (N times, sender-sorted by user_id ASCII order to
//                  give the receiver a stable index for `pub_hint`
//                  collisions):
//     pub_hint: u8            // low byte of recipient's IK_X25519
//                             // public key — receiver scans for the
//                             // slot whose pub_hint matches their
//                             // own and tries decrypt
//     nonce_k:  [u8; 24]      // XChaCha20-Poly1305 nonce for the
//                             // session-key wrap
//     wrap_k:   [u8; 48]      // 32-byte session key + 16-byte tag
//   nonce_msg:  [u8; 24]      // nonce for the bulk message AEAD
//   ct_msg:     [u8; pt_len + 16]  // ciphertext + tag
// ]
// ```
//
// Why session-key wrap (KEM-then-DEM) over per-recipient AEAD of the
// full plaintext: a 1400-byte Mode-0 budget with N recipients gives
// us roughly `1400 - N*(plaintext_len + 40)` of working room with
// per-recipient AEAD; the wrap scheme drops the per-recipient cost
// to a constant 73 bytes (1 + 24 + 48), so a 1000-byte plaintext
// fits up to N=5 instead of N=1 only. See `docs/design/
// layer-10-discord-internals.md` §13 for the working math.
//
// AEAD associated-data strings are static domain separators — the
// inner JSON-shaped Discord context (reply IDs, attachments, etc.)
// is not bound here because Phase 4 does not yet have a receive-side
// decoder that would re-validate it. Phase 5 binds AD to the full
// PQXDH transcript.
//
// Phase 5+ replaces this entirely with the PQXDH handshake +
// Double Ratchet header keys. The wire shape changes (version byte
// bumps to 0x02+); the IPC contract `(channel_id, plaintext, options)
// -> Result<String, String>` does not.

/// Wire-format version of the Phase 4 OSL framing inside Mode 0
/// payloads. Bump in lockstep with any field-shape change. Phase 5
/// (PQXDH + Double Ratchet) will introduce 0x02+.
pub const OSL_PHASE4_WIRE_VERSION: u8 = 0x01;

/// Maximum plaintext byte length accepted by [`cmd_osl_encrypt_message`].
/// Chosen as a soft UX cap (single chat-input bubble) — the hard
/// cap from Mode 0's 1400-byte budget is computed dynamically per
/// recipient count and may be tighter; the smaller of the two
/// applies.
pub const OSL_PHASE4_PLAINTEXT_BYTE_CAP: usize = 1000;

/// Per-recipient framing cost inside the wire payload:
/// `pub_hint(1) + nonce_k(24) + wrap_k(32 session key + 16 tag)`.
pub const OSL_PHASE4_PER_RECIPIENT_BYTES: usize =
    1 + aead::NONCE_SIZE + aead::KEY_SIZE + aead::TAG_SIZE;

/// Fixed framing cost: `version(1) + N(1) + nonce_msg(24) + tag_msg(16)`.
pub const OSL_PHASE4_FIXED_FRAMING_BYTES: usize =
    1 + 1 + aead::NONCE_SIZE + aead::TAG_SIZE;

/// AEAD associated-data: static domain separator for the bulk
/// message ciphertext leg. Static (no transcript binding) is
/// deliberate for Phase 4 — Phase 5 will bind to the PQXDH
/// transcript.
pub const OSL_PHASE4_AD_MSG: &[u8] = b"OSL/P4/msg/v1";

/// AEAD associated-data: static domain separator for the
/// per-recipient session-key wrap leg.
pub const OSL_PHASE4_AD_WRAP: &[u8] = b"OSL/P4/wrap/v1";

/// HKDF info string for deriving the per-recipient wrap key from
/// the X25519 ECDH shared secret. Empty salt — the IKM is already
/// 32 bytes of high-entropy DH output and the AEAD nonces provide
/// per-message uniqueness.
pub const OSL_PHASE4_HKDF_INFO_WRAP: &[u8] = b"OSL/P4/wrap-key/v1";

/// Pure encoder for the Phase 4 wire format. Takes pre-resolved
/// recipient pubkeys and returns the Mode 0 cover string.
///
/// "Pure" in the I/O sense: no [`AppState`] access, no network
/// calls, no filesystem reads. Random bytes (session key + nonces)
/// come from `crypto::random`, so successive calls with identical
/// inputs produce different outputs — this isn't a hash. Tests
/// that need deterministic output should mock or fix the random
/// source separately.
///
/// # Auto-include sender as recipient
///
/// The sender's own X25519 public key is always added to the
/// recipient slot list (deduped against the explicit
/// `recipient_pubkeys`). Two reasons:
///
/// - **Optimistic-render UX.** When the sender hits Enter, the
///   server bounces the encrypted message back as a
///   `MESSAGE_CREATE` event; without a sender slot, the sender's
///   own client can't decrypt their own message and would render
///   the `DPC0::` cover instead of plaintext. The auto-slot fixes
///   this for the common case.
/// - **Search consistency.** Discord's Cmd-F search runs against
///   the rendered message text (post-decrypt). Without a sender
///   slot, the sender can't search their own past messages.
///
/// Cost: one extra slot per message (73 bytes inside the Mode 0
/// payload). Worth it.
///
/// `recipient_pubkeys` order determines the slot order in the
/// wire format up to the sender slot, which is appended last.
/// Callers should pre-sort the input to whatever ordering the
/// receive-side decoder expects (the IPC wrapper
/// [`cmd_osl_encrypt_message`] sorts by recipient `user_id` ASCII
/// before reaching this function).
///
/// Caps enforced here:
/// - Empty plaintext rejected.
/// - `plaintext.len() > OSL_PHASE4_PLAINTEXT_BYTE_CAP`.
/// - Effective recipient count (input + sender, post-dedup) `0`
///   or `> 255`.
/// - Total wire length `> stego::MODE0_MAX_RAW_LEN`.
/// - Stego output length `> 2000` chars (Discord message cap).
///
/// The IPC wrapper rechecks the recipient-count cap defensively;
/// it's the same check, so duplication is fine.
pub fn encrypt_osl_phase4_to_pubkeys(
    sender_secret: &x25519::SecretKey,
    recipient_pubkeys: &[x25519::PublicKey],
    plaintext: &str,
) -> Result<String, String> {
    let plaintext_bytes = plaintext.as_bytes();
    if plaintext_bytes.is_empty() {
        return Err("OSL: refusing to encrypt empty plaintext".to_string());
    }
    if plaintext_bytes.len() > OSL_PHASE4_PLAINTEXT_BYTE_CAP {
        return Err(format!(
            "OSL: plaintext is {} bytes, exceeds soft cap of {}",
            plaintext_bytes.len(),
            OSL_PHASE4_PLAINTEXT_BYTE_CAP
        ));
    }

    // Build the effective slot list: input recipients plus the
    // sender's own pubkey (auto-included). Dedup by raw pubkey
    // bytes so callers passing the sender as an explicit
    // recipient (e.g. tests, or future channels.json that lists
    // self) don't double up.
    let sender_pub = x25519::derive_public(sender_secret);
    let mut effective: Vec<x25519::PublicKey> =
        Vec::with_capacity(recipient_pubkeys.len() + 1);
    let mut seen_keys: Vec<[u8; x25519::PUBLIC_KEY_SIZE]> = Vec::new();
    for pk in recipient_pubkeys.iter() {
        let bytes = *pk.as_bytes();
        if !seen_keys.iter().any(|b| b == &bytes) {
            seen_keys.push(bytes);
            effective.push(pk.clone());
        }
    }
    let sender_bytes = *sender_pub.as_bytes();
    if !seen_keys.iter().any(|b| b == &sender_bytes) {
        seen_keys.push(sender_bytes);
        effective.push(sender_pub);
    }

    let n = effective.len();
    if n == 0 {
        return Err("OSL: zero recipients after lookup".to_string());
    }
    if n > 255 {
        return Err(format!(
            "OSL: recipient count {n} exceeds wire-format max of 255"
        ));
    }

    let total_wire_len = OSL_PHASE4_FIXED_FRAMING_BYTES
        + n * OSL_PHASE4_PER_RECIPIENT_BYTES
        + plaintext_bytes.len();
    if total_wire_len > stego::MODE0_MAX_RAW_LEN {
        let max_plaintext_for_n = stego::MODE0_MAX_RAW_LEN
            .saturating_sub(OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES);
        return Err(format!(
            "OSL: payload {} bytes exceeds Mode 0 cap {} ({} recipients; \
             max plaintext for this recipient count is {} bytes)",
            total_wire_len,
            stego::MODE0_MAX_RAW_LEN,
            n,
            max_plaintext_for_n
        ));
    }

    let session_key = random::random_aead_key();
    let nonce_msg = random::random_nonce();
    let ct_msg = aead::seal(&session_key, &nonce_msg, OSL_PHASE4_AD_MSG, plaintext_bytes)
        .map_err(|e| format!("OSL: AEAD seal (msg) failed: {e}"))?;

    let mut wire: Vec<u8> = Vec::with_capacity(total_wire_len);
    wire.push(OSL_PHASE4_WIRE_VERSION);
    wire.push(n as u8);

    for (slot_ix, peer_pub) in effective.iter().enumerate() {
        let shared = x25519::diffie_hellman(sender_secret, peer_pub)
            .map_err(|e| format!("OSL: ECDH (slot {slot_ix}): {e}"))?;
        let wrap_key_bytes =
            hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
                .map_err(|e| format!("OSL: HKDF wrap-key (slot {slot_ix}): {e}"))?;
        let wrap_key = aead::Key::from_bytes(wrap_key_bytes);

        let nonce_k = random::random_nonce();
        let wrap_ct = aead::seal(
            &wrap_key,
            &nonce_k,
            OSL_PHASE4_AD_WRAP,
            session_key.as_bytes(),
        )
        .map_err(|e| format!("OSL: AEAD seal (wrap) (slot {slot_ix}): {e}"))?;
        if wrap_ct.len() != aead::KEY_SIZE + aead::TAG_SIZE {
            return Err(format!(
                "OSL: wrap ciphertext unexpected length: got {}, want {}",
                wrap_ct.len(),
                aead::KEY_SIZE + aead::TAG_SIZE
            ));
        }

        let peer_pub_bytes = peer_pub.as_bytes();
        wire.push(peer_pub_bytes[0]);
        wire.extend_from_slice(nonce_k.as_bytes());
        wire.extend_from_slice(&wrap_ct);
    }

    wire.extend_from_slice(nonce_msg.as_bytes());
    wire.extend_from_slice(&ct_msg);

    if wire.len() != total_wire_len {
        return Err(format!(
            "OSL: internal wire-length mismatch: built {}, expected {}",
            wire.len(),
            total_wire_len
        ));
    }

    let stego_msg = stego::encode_mode0(&wire)
        .map_err(|e| format!("OSL: stego encode: {e}"))?;
    if stego_msg.len() > 2000 {
        return Err(format!(
            "OSL: stego output {} chars exceeds Discord 2000-char message cap",
            stego_msg.len()
        ));
    }
    Ok(stego_msg)
}

/// Layer 10 / Phase 4 IPC entry point: encrypt `plaintext` for
/// the configured recipients of `channel_id` and return a Mode 0
/// stego cover string suitable for direct insertion as the
/// outbound Discord message body.
///
/// Orchestrates IO around the pure
/// [`encrypt_osl_phase4_to_pubkeys`]:
/// 1. Resolve `channel_id` → list of recipient `user_id`s via
///    [`keystore::get_recipients`].
/// 2. Lock [`AppState`] for the loaded identity + keyserver
///    client.
/// 3. Sort recipient `user_id`s ASCII for stable wire-slot order.
/// 4. Per-recipient: `KeyServerClient::fetch_pubkeys` →
///    decode IK_X25519 base64 → typed `PublicKey`.
/// 5. Hand the resolved pubkey vector to the pure encoder.
///
/// Returns `Result<String, String>` (not [`IpcResult`]) — see
/// `osl_encrypt_message` in `src-tauri/src/main.rs` for the
/// rationale (flat string error across the JS-bootloader
/// fail-closed boundary).
///
/// Failure modes (all fail-closed):
///
/// - missing identity / keyserver in [`AppState`]
/// - unconfigured / empty channel in `channels.json`
/// - per-recipient pubkey fetch / decode failure
/// - any error surfaced by [`encrypt_osl_phase4_to_pubkeys`]
pub fn cmd_osl_encrypt_message(
    state: &AppState,
    channel_id: String,
    plaintext: String,
    _options: serde_json::Value,
) -> Result<String, String> {
    let recipients = keystore::get_recipients(&channel_id)
        .map_err(|e| format!("OSL: recipient lookup: {e}"))?;

    // Stable order: sort recipient ids ASCII so that the receiver
    // can scan slots deterministically.
    let mut sorted = recipients;
    sorted.sort();

    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard
        .as_ref()
        .ok_or_else(|| "OSL: key-server not initialised".to_string())?;

    let mut peer_pubkeys: Vec<x25519::PublicKey> = Vec::with_capacity(sorted.len());
    for user_id in &sorted {
        let resp = client
            .fetch_pubkeys(user_id)
            .map_err(|e| format!("OSL: fetch_pubkeys({user_id}): {e}"))?;
        let peer_pub_vec = STANDARD
            .decode(&resp.ik_x25519_pub)
            .map_err(|e| format!("OSL: decode peer pubkey ({user_id}): {e}"))?;
        if peer_pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: peer pubkey wrong length ({user_id}): got {}, want {}",
                peer_pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE
            ));
        }
        let mut peer_pub_bytes = [0u8; x25519::PUBLIC_KEY_SIZE];
        peer_pub_bytes.copy_from_slice(&peer_pub_vec);
        peer_pubkeys.push(x25519::PublicKey::from_bytes(peer_pub_bytes));
    }

    encrypt_osl_phase4_to_pubkeys(&identity.x25519_secret, &peer_pubkeys, &plaintext)
}

// ---- Layer 10 / Phase 5: receive-side decoder + IPC command ----

/// Errors returned by the Phase 4 wire-format decoder.
///
/// `Display` strings are user-visible (the IPC bridge maps them
/// straight into the JS hook's reject path), so they're worded as
/// brief diagnostic phrases rather than internal-state dumps.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The cover string didn't carry the `DPC0::` magic prefix —
    /// not an OSL message at all. The JS hook treats this as
    /// "leave content alone" and never surfaces it as a user
    /// error.
    #[error("cover string missing DPC0:: prefix")]
    BadPrefix,

    /// The base64 body of the cover string failed to decode.
    /// Fragmented send, truncation, or someone manually editing
    /// the cover. Renders as "Failed to decode" in the UI.
    #[error("base64 decode of cover body failed: {0}")]
    Base64(String),

    /// Wire bytes shorter than the minimum framing requires for
    /// the declared recipient count. Always corruption.
    #[error("wire too short: {got} bytes, expected at least {expected}")]
    TooShort { got: usize, expected: usize },

    /// Wire version byte didn't match Phase 4's `0x01`. Indicates
    /// either a future version (Phase 5+ ratchet wire) we can't
    /// yet decode, or junk.
    #[error("unsupported wire version 0x{got:02x} (this client only decodes 0x{expected:02x})")]
    UnsupportedVersion { got: u8, expected: u8 },

    /// Recipient count byte `N` was zero. A well-formed encoder
    /// rejects this; if we see it, the wire is junk.
    #[error("recipient count is zero in wire header")]
    ZeroRecipients,

    /// We are not a recipient of this message — no slot's wrap
    /// AEAD opened under our identity key. The JS hook treats
    /// this as "leave content alone" so non-recipients in a
    /// channel still see the cover string normally.
    #[error("not a recipient of this message")]
    NoMatchingSlot,

    /// A wrap slot opened (revealing a session key candidate),
    /// but the bulk message AEAD failed under that key. Indicates
    /// either a corrupted wire or a deliberate splice. Distinct
    /// from `NoMatchingSlot` for diagnostics.
    #[error("wrap slot opened but message AEAD failed: {0}")]
    MessageAeadFailed(String),

    /// Underlying X25519 / HKDF / AEAD primitive returned an
    /// error not otherwise classified. Surfaces the inner
    /// message verbatim — useful when debugging primitives, never
    /// triggers in normal operation.
    #[error("crypto primitive error: {0}")]
    Crypto(String),
}

/// Decode the Phase 4 wire-format raw bytes (post-`DPC0::`-strip,
/// post-base64-decode) into the recovered plaintext bytes.
///
/// Pure: takes pre-resolved sender + recipient keys, no `AppState`,
/// no IO. Tests exercise this directly with hand-built keypairs.
///
/// # Constant-time-ish slot iteration
///
/// The loop runs **all** slots that match our `pub_hint` —
/// it does not break on first successful unwrap. Two slots
/// could share a `pub_hint` byte (1/256 probability per
/// collision), and breaking early would let a timing-aware
/// observer narrow down which slot is ours. The cost is one
/// extra AEAD attempt per legitimate hint collision; usually
/// zero such collisions in practice.
///
/// We do still skip slots whose `pub_hint` doesn't match ours.
/// The `pub_hint` is public information (sender writes the
/// recipient's public-key low byte into the wire), so iterating
/// over non-matching slots is wasted work, not a leak.
///
/// We do not (and cannot reasonably) make "are we a recipient at
/// all?" constant-time relative to "we are a recipient" — those
/// states are externally observable via whether we re-dispatch a
/// `MESSAGE_UPDATE` afterwards.
pub fn decrypt_osl_phase4_from_wire(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    wire: &[u8],
) -> Result<Vec<u8>, DecodeError> {
    if wire.len() < OSL_PHASE4_FIXED_FRAMING_BYTES {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: OSL_PHASE4_FIXED_FRAMING_BYTES,
        });
    }
    let version = wire[0];
    if version != OSL_PHASE4_WIRE_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            got: version,
            expected: OSL_PHASE4_WIRE_VERSION,
        });
    }
    let n = wire[1] as usize;
    if n == 0 {
        return Err(DecodeError::ZeroRecipients);
    }
    let expected_min = OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES;
    if wire.len() < expected_min {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: expected_min,
        });
    }

    // Compute receiver's own pub_hint to find candidate slots.
    let recipient_pub = x25519::derive_public(recipient_secret);
    let our_hint = recipient_pub.as_bytes()[0];

    // Recover the shared secret + wrap key once — every slot
    // belonging to us derives from the same `(recipient_sk,
    // sender_pk)` pair.
    let shared = x25519::diffie_hellman(recipient_secret, sender_pub)
        .map_err(|e| DecodeError::Crypto(format!("ECDH: {e}")))?;
    let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
        .map_err(|e| DecodeError::Crypto(format!("HKDF wrap-key: {e}")))?;
    let wrap_key = aead::Key::from_bytes(wrap_key_bytes);

    // Walk all slots; for any with a matching `pub_hint`, attempt
    // wrap-decrypt. Don't break on first success — see the
    // "constant-time-ish" note in the docstring above.
    let slot_size = OSL_PHASE4_PER_RECIPIENT_BYTES;
    let mut session_key: Option<aead::Key> = None;
    for slot_ix in 0..n {
        let base = 2 + slot_ix * slot_size;
        let hint = wire[base];
        if hint != our_hint {
            continue;
        }
        let nonce_start = base + 1;
        let nonce_end = nonce_start + aead::NONCE_SIZE;
        let wrap_start = nonce_end;
        let wrap_end = wrap_start + aead::KEY_SIZE + aead::TAG_SIZE;
        let mut nonce_bytes = [0u8; aead::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&wire[nonce_start..nonce_end]);
        let nonce = aead::Nonce::from_bytes(nonce_bytes);
        let wrap_ct = &wire[wrap_start..wrap_end];

        if let Ok(plaintext_bytes) =
            aead::open(&wrap_key, &nonce, OSL_PHASE4_AD_WRAP, wrap_ct)
        {
            if plaintext_bytes.len() == aead::KEY_SIZE && session_key.is_none() {
                let mut sk = [0u8; aead::KEY_SIZE];
                sk.copy_from_slice(&plaintext_bytes);
                session_key = Some(aead::Key::from_bytes(sk));
                // Deliberately no `break` — see docstring.
            }
            // Wrong-length plaintext from a "successful" open is
            // pathological (AEAD tag matched against a corrupted
            // body). Treat as not-our-slot and keep going.
        }
        // Failed AEAD: not our slot under this hint collision; keep going.
    }
    let session_key = session_key.ok_or(DecodeError::NoMatchingSlot)?;

    // Bulk message decrypt. Position is fixed: nonce at
    // `2 + n * slot_size`, ciphertext to end of wire.
    let msg_nonce_start = 2 + n * slot_size;
    let msg_nonce_end = msg_nonce_start + aead::NONCE_SIZE;
    if wire.len() < msg_nonce_end + aead::TAG_SIZE {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: msg_nonce_end + aead::TAG_SIZE,
        });
    }
    let mut msg_nonce_bytes = [0u8; aead::NONCE_SIZE];
    msg_nonce_bytes.copy_from_slice(&wire[msg_nonce_start..msg_nonce_end]);
    let msg_nonce = aead::Nonce::from_bytes(msg_nonce_bytes);
    let ct_msg = &wire[msg_nonce_end..];
    aead::open(&session_key, &msg_nonce, OSL_PHASE4_AD_MSG, ct_msg)
        .map_err(|e| DecodeError::MessageAeadFailed(e.to_string()))
}

/// Higher-level decoder: takes the on-the-wire `DPC0::<base64>`
/// cover string, strips the prefix, base64-decodes the body, then
/// hands off to [`decrypt_osl_phase4_from_wire`].
///
/// Returns `BadPrefix` for non-OSL content so the JS hook can
/// trivially distinguish "this isn't ours, leave it alone" from
/// "this is ours but we're not a recipient." Same effective UX
/// (cover stays visible) but useful for log-grep separation.
pub fn decrypt_osl_phase4_cover(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    cover: &str,
) -> Result<Vec<u8>, DecodeError> {
    let body = cover
        .strip_prefix("DPC0::")
        .ok_or(DecodeError::BadPrefix)?;
    let wire = STANDARD
        .decode(body)
        .map_err(|e| DecodeError::Base64(e.to_string()))?;
    decrypt_osl_phase4_from_wire(recipient_secret, sender_pub, &wire)
}

/// Layer 10 / Phase 5 IPC entry point: decrypt an incoming
/// Discord message back to plaintext, given the sender's user_id
/// (so the keyserver can resolve their X25519 public key).
///
/// Caller (the JS hook on `MESSAGE_CREATE`) is expected to:
/// 1. Pre-filter on the `DPC0::` prefix (so this command isn't
///    invoked for every message — the prefix scan in JS is far
///    cheaper than crossing the IPC bridge).
/// 2. Pass the Discord `message.author.id` as `sender_user_id`.
/// 3. Render the returned plaintext in place of the cover when
///    `Ok(_)` is returned.
/// 4. Leave the cover visible when `Err(_)` returns. Failure
///    branches: not a recipient (`NoMatchingSlot`), key rotation
///    we haven't refetched yet (`MessageAeadFailed`), wire
///    corruption (`TooShort` / `BadPrefix` / etc.).
///
/// Returns `Result<String, String>` (matching encrypt's wire
/// shape). On success, the plaintext is interpreted as UTF-8 and
/// returned verbatim. Non-UTF-8 plaintext returns a
/// `OSL: invalid UTF-8` error — the encoder accepts only UTF-8
/// input, so this should never trigger absent corruption.
///
/// Sender pubkey resolution is cached per `AppState`'s
/// [`crate::state::SenderPubkeyCache`] (5-minute TTL); first hit
/// per sender per 5-minute window pays a keyserver round-trip,
/// subsequent hits are local.
///
/// `_channel_id` is currently unused — the recipient mapping is
/// not channel-keyed on the receive side (any message we can
/// decrypt belongs to us regardless of which channel it landed
/// in). Carried in the IPC signature for symmetry with encrypt
/// and so future per-channel ratchet state can plug in without a
/// wire change.
pub fn cmd_osl_decrypt_message(
    state: &AppState,
    _channel_id: String,
    sender_user_id: String,
    content: String,
) -> Result<String, String> {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;

    // Pubkey lookup: cache → keyserver → cache-insert.
    let sender_pub = if let Some(cached) = state.sender_pubkey_cache.get(&sender_user_id) {
        cached
    } else {
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        let resp = client
            .fetch_pubkeys(&sender_user_id)
            .map_err(|e| format!("OSL: fetch_pubkeys({sender_user_id}): {e}"))?;
        let pub_vec = STANDARD
            .decode(&resp.ik_x25519_pub)
            .map_err(|e| format!("OSL: decode sender pubkey ({sender_user_id}): {e}"))?;
        if pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: sender pubkey wrong length ({sender_user_id}): got {}, want {}",
                pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE
            ));
        }
        let mut bytes = [0u8; x25519::PUBLIC_KEY_SIZE];
        bytes.copy_from_slice(&pub_vec);
        let pub_key = x25519::PublicKey::from_bytes(bytes);
        // Drop the keyserver lock before inserting into the cache
        // (the cache has its own mutex).
        drop(ks_guard);
        state
            .sender_pubkey_cache
            .insert(sender_user_id.clone(), pub_key.clone());
        pub_key
    };

    let plaintext_bytes =
        decrypt_osl_phase4_cover(&identity.x25519_secret, &sender_pub, &content)
            .map_err(|e| format!("OSL: {e}"))?;
    String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: decrypted plaintext is not valid UTF-8".to_string())
}
