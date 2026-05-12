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
use std::time::{SystemTime, UNIX_EPOCH};
use store::{StoreError, StoredMessage};

// =====================================================================
// 7d-FIX1: persistence write-through helpers.
//
// Pre-FIX1 root cause: mutating commands (set_whitelist,
// unwhitelist_scope, apply_invitation_decision, toggle_scope_encryption,
// etc.) updated `AppState` in memory but NEVER wrote back to disk.
// peer_map.json / whitelist_state.json on disk only ever contained
// what the user hand-edited (or what bootstrap loaded at startup).
// This blocked encryption-at-rest from ever firing: with no write
// path, the `maybe_encrypt` retrofit in the write functions was
// never exercised.
//
// The helpers are best-effort: on disk-write failure we log and
// continue. The in-memory mutation already happened; surfacing a
// disk-write error to the caller would be confusing UX
// ("invitation accepted, but the action failed?") while doing
// nothing useful for the user.
// =====================================================================

/// 7d-FIX3b: ensure peer_map has a well-formed self-entry keyed
/// by the user's Discord snowflake, matching the loaded identity's
/// `user_id` and X25519 public key with `is_self = true`.
///
/// Called from bootstrap.rs after `load_peer_map`, and from
/// `cmd_osl_register_self_snowflake` after identity gets a new
/// snowflake. Idempotent — a no-op if the entry already matches.
///
/// Memory-only; the caller persists peer_map.json via the
/// `verify_and_persist_peer_map_self_entry` wrapper for production
/// paths. Splitting keeps tests hermetic (no
/// `keystore::osl_config_dir()` writes).
pub fn verify_peer_map_self_entry(state: &AppState) -> Result<(String, bool), String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let (osl_user_id, pubkey_b64, snowflake) = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let id = guard
            .as_ref()
            .ok_or_else(|| "identity_not_loaded".to_string())?;
        let snow = id
            .discord_snowflake
            .clone()
            .ok_or_else(|| "no_discord_snowflake".to_string())?;
        let pub_b64 = STANDARD.encode(id.x25519_public.as_bytes());
        (id.user_id.clone(), pub_b64, snow)
    };

    let needs_repair = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        match pm.get(&snowflake) {
            None => true,
            Some(entry) => {
                let user_id_ok = entry.osl_user_id.as_deref() == Some(osl_user_id.as_str());
                let pubkey_ok = entry.pubkey.as_deref() == Some(pubkey_b64.as_str());
                let is_self_ok = entry.is_self.unwrap_or(false);
                !(user_id_ok && pubkey_ok && is_self_ok)
            }
        }
    };

    if !needs_repair {
        return Ok((snowflake, false));
    }

    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(snowflake.clone()).or_default();
        entry.osl_user_id = Some(osl_user_id);
        entry.pubkey = Some(pubkey_b64);
        entry.discord_id = Some(snowflake.clone());
        entry.is_self = Some(true);
        // Leave outgoing_whitelists / incoming_decrypt_accepted /
        // burned_scopes alone — self-entry doesn't whitelist itself,
        // but if a prior bug populated those fields we don't want
        // to clobber unrelated state during repair.
    }
    Ok((snowflake, true))
}

/// 7d-FIX3b: production wrapper around `verify_peer_map_self_entry`
/// that persists peer_map.json if a repair happened. Tests use the
/// bare verify and inspect AppState directly.
pub fn verify_and_persist_peer_map_self_entry(state: &AppState) -> Result<(String, bool), String> {
    let result = verify_peer_map_self_entry(state)?;
    if result.1 {
        persist_peer_map_now(state);
    }
    Ok(result)
}

/// 7d-FIX3b: persist a Discord snowflake on the loaded identity and
/// repair the peer_map self-entry to match. Called from boot.js
/// the first time the runtime exposes the local user's snowflake.
///
/// Validates 17-20 digit format. Rejects mismatch against an
/// existing recorded snowflake (account-change refusal). Idempotent
/// for matching re-registrations (just runs verify).
pub fn cmd_osl_register_self_snowflake(state: &AppState, snowflake: String) -> Result<(), String> {
    if !snowflake.chars().all(|c| c.is_ascii_digit()) || !(17..=20).contains(&snowflake.len()) {
        return Err(format!(
            "OSL: register_self_snowflake: invalid format \
             (expected 17-20 digit numeric, got {} chars)",
            snowflake.len()
        ));
    }

    enum Step {
        Save(keystore::Identity),
        AlreadySet,
    }
    let step = {
        let mut guard = state.identity.lock().expect("identity mutex poisoned");
        let id = guard
            .as_mut()
            .ok_or_else(|| "OSL: register_self_snowflake: identity not loaded".to_string())?;
        if let Some(existing) = &id.discord_snowflake {
            if existing != &snowflake {
                return Err(format!(
                    "OSL: register_self_snowflake: snowflake mismatch \
                     (identity already bound to {}, refusing to retag to {}) — \
                     this could indicate a Discord account change or a \
                     state-corruption bug. Burn account + re-register if intentional.",
                    existing, snowflake
                ));
            }
            Step::AlreadySet
        } else {
            id.discord_snowflake = Some(snowflake.clone());
            let mut snapshot = keystore::Identity::from_bytes(
                id.user_id.clone(),
                *id.x25519_secret.as_bytes(),
                *id.x25519_public.as_bytes(),
                *id.ed25519_secret.as_bytes(),
                *id.ed25519_public.as_bytes(),
                *id.mlkem_secret_bytes(),
                id.mlkem_public_bytes,
            );
            snapshot.discord_snowflake = Some(snowflake.clone());
            Step::Save(snapshot)
        }
    };

    let to_save = match step {
        Step::AlreadySet => return run_verify(state),
        Step::Save(snapshot) => snapshot,
    };

    let dir = keystore::osl_config_dir()
        .map_err(|e| format!("OSL: register_self_snowflake: config dir: {e}"))?;
    let path = dir.join("identity.json");
    let sealer = keystore::select_best_sealer();
    if let Err(e) = keystore::save_identity(&path, &to_save, sealer.as_ref()) {
        // Roll back the in-memory change so callers can retry
        // without lying about the durable state.
        let mut guard = state.identity.lock().expect("identity mutex poisoned");
        if let Some(id) = guard.as_mut() {
            id.discord_snowflake = None;
        }
        return Err(format!(
            "OSL: register_self_snowflake: save_identity failed: {e}"
        ));
    }
    eprintln!("[OSL][bootstrap] self snowflake registered: {snowflake}");
    run_verify(state)
}

fn run_verify(state: &AppState) -> Result<(), String> {
    match verify_and_persist_peer_map_self_entry(state) {
        Ok((snowflake, repaired)) => {
            if repaired {
                eprintln!("[OSL][bootstrap] self-entry repaired for snowflake={snowflake}");
            } else {
                eprintln!("[OSL][bootstrap] self-entry verified");
            }
            Ok(())
        }
        Err(reason) if reason == "no_discord_snowflake" => {
            eprintln!(
                "[OSL][bootstrap] no discord snowflake on identity; \
                 deferring to boot.js"
            );
            Ok(())
        }
        Err(reason) if reason == "identity_not_loaded" => {
            Err("OSL: register_self_snowflake: identity not loaded".into())
        }
        Err(other) => Err(other),
    }
}

fn persist_peer_map_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(?e, "OSL: persist peer_map: cannot resolve config dir");
            return;
        }
    };
    let path = dir.join("peer_map.json");
    let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
    if let Err(e) = crate::peer_map::write_peer_map(&path, &pm) {
        tracing::warn!(?e, path = %path.display(), "OSL: persist peer_map.json failed");
    }
}

pub fn persist_whitelist_state_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                ?e,
                "OSL: persist whitelist_state: cannot resolve config dir"
            );
            return;
        }
    };
    let path = dir.join("whitelist_state.json");
    let ws = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    if let Err(e) = crate::whitelist_state::write_whitelist_state(&path, &ws) {
        tracing::warn!(?e, path = %path.display(), "OSL: persist whitelist_state.json failed");
    }
}

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
        return Err(IpcError::InvalidArgument(
            "user_id must be non-empty".into(),
        ));
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
    let nonce = aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>(
        "nonce_b64",
        &req.nonce_b64,
    )?);
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
    let nonce = aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>(
        "nonce_b64",
        &req.nonce_b64,
    )?);
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
pub fn cmd_x25519_diffie_hellman(secret_b64: String, peer_public_b64: String) -> IpcResult<String> {
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
pub const OSL_PHASE4_FIXED_FRAMING_BYTES: usize = 1 + 1 + aead::NONCE_SIZE + aead::TAG_SIZE;

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
    let mut effective: Vec<x25519::PublicKey> = Vec::with_capacity(recipient_pubkeys.len() + 1);
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

    let total_wire_len =
        OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES + plaintext_bytes.len();
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
        let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
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

    let stego_msg = stego::encode_mode0(&wire).map_err(|e| format!("OSL: stego encode: {e}"))?;
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
    let recipients =
        keystore::get_recipients(&channel_id).map_err(|e| format!("OSL: recipient lookup: {e}"))?;

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
        // Send-side diagnostic, mirroring the receive-side
        // `our_hint=…` / `hints=[…]` block. In dev builds, surface
        // each recipient's pubkey first byte AS FETCHED from the
        // keyserver (no client-side cache) so the user can sanity-
        // check against the keyserver's `ik_x25519_pub` field for
        // the same user_id. If these diverge across consecutive
        // sends, something is rotating mid-session.
        #[cfg(debug_assertions)]
        eprintln!(
            "[OSL] encrypt slot recipient={} pubkey_first_byte=0x{:02x}",
            user_id, peer_pub_bytes[0]
        );
        peer_pubkeys.push(x25519::PublicKey::from_bytes(peer_pub_bytes));
    }

    // Sender-side derived pub. Compare against keyserver-published
    // pub for our own user_id: divergence here means
    // `identity.x25519_public` (uploaded at register) drifted from
    // `derive_public(secret)`. `load_identity` self-heals at load
    // by re-deriving, but a session that started before the heal
    // landed could still surface this.
    #[cfg(debug_assertions)]
    {
        let derived_pub = x25519::derive_public(&identity.x25519_secret);
        let stored_first = identity.x25519_public.as_bytes()[0];
        let derived_first = derived_pub.as_bytes()[0];
        eprintln!(
            "[OSL] encrypt sender_user_id={} derived_first_byte=0x{:02x} \
             stored_first_byte=0x{:02x}{}",
            identity.user_id,
            derived_first,
            stored_first,
            if derived_first != stored_first {
                " DRIFT — register() uploaded stored, encrypt/decrypt use derived"
            } else {
                ""
            }
        );
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

    /// The sender's Discord user_id is not present in
    /// `peer_map.json`, so we can't translate it to an OSL
    /// user_id and the keyserver lookup would 404. Phase 5 v1's
    /// pre-decode resolution failure mode — distinct from
    /// `NoMatchingSlot` (where we ARE configured to talk to the
    /// sender but the message isn't addressed to us).
    ///
    /// JS hook treats this as "skip silently, leave cover in
    /// place" — same UX as `NoMatchingSlot` and `BadPrefix`. The
    /// `discord_id` is included so the hook can dedupe its
    /// onboarding-hint log to one line per unmapped sender
    /// rather than per message.
    #[error("no peer mapping for discord_id={discord_id} (add to peer_map.json)")]
    UnknownSender { discord_id: String },
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

        if let Ok(plaintext_bytes) = aead::open(&wrap_key, &nonce, OSL_PHASE4_AD_WRAP, wrap_ct) {
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
    let body = cover.strip_prefix("DPC0::").ok_or(DecodeError::BadPrefix)?;
    let wire = STANDARD
        .decode(body)
        .map_err(|e| DecodeError::Base64(e.to_string()))?;
    decrypt_osl_phase4_from_wire(recipient_secret, sender_pub, &wire)
}

/// Layer 10 / Phase 5 IPC entry point: decrypt an incoming
/// Discord message back to plaintext.
///
/// Takes `sender_discord_id` — the raw Discord snowflake the
/// boot.js receive observer pulled out of the message DOM
/// (`data-author-id`, avatar URL, etc.). Discord IDs aren't
/// keyserver identifiers, so we resolve to OSL `user_id` via
/// `AppState::peer_map` (loaded at bootstrap from
/// `<osl_config_dir>/peer_map.json`) before any keyserver call.
///
/// Caller (the JS hook on `MESSAGE_CREATE`) is expected to:
/// 1. Pre-filter on the `DPC0::` prefix (so this command isn't
///    invoked for every message — the prefix scan in JS is far
///    cheaper than crossing the IPC bridge).
/// 2. Pass the Discord `message.author.id` as `sender_discord_id`.
/// 3. Render the returned plaintext in place of the cover when
///    `Ok(_)` is returned.
/// 4. Leave the cover visible when `Err(_)` returns. Failure
///    branches: peer not in map (`UnknownSender`), not a recipient
///    (`NoMatchingSlot`), key rotation we haven't refetched yet
///    (`MessageAeadFailed`), wire corruption (`TooShort` /
///    `BadPrefix` / etc.).
///
/// Returns `Result<String, String>` (matching encrypt's wire
/// shape). On success, the plaintext is interpreted as UTF-8 and
/// returned verbatim. Non-UTF-8 plaintext returns a
/// `OSL: invalid UTF-8` error — the encoder accepts only UTF-8
/// input, so this should never trigger absent corruption.
///
/// Sender pubkey resolution is cached per `AppState`'s
/// [`crate::state::SenderPubkeyCache`] (30-minute TTL); first hit
/// per sender per window pays a keyserver round-trip, subsequent
/// hits are local. Cache is keyed by **OSL user_id** (post peer-
/// map resolution), not Discord id, so re-mapping a discord_id to
/// a different OSL identity in `peer_map.json` doesn't pollute
/// the cache.
///
/// `_channel_id` is currently unused — the recipient mapping is
/// not channel-keyed on the receive side (any message we can
/// decrypt belongs to us regardless of which channel it landed
/// in). Carried in the IPC signature for symmetry with encrypt
/// and so future per-channel ratchet state can plug in without a
/// wire change.
pub fn cmd_osl_decrypt_message(
    state: &AppState,
    channel_id: String,
    sender_discord_id: String,
    content: String,
) -> Result<String, String> {
    cmd_osl_decrypt_message_with_id(state, None, channel_id, sender_discord_id, content)
}

/// Same as [`cmd_osl_decrypt_message`] but accepts an optional
/// `discord_message_id`. When `Some`, the decrypted plaintext is
/// persisted to [`crate::state::AppState::message_store`] (Phase
/// 5b2). When `None`, the decrypt path runs unchanged with no
/// persistence side-effect (Phase 5b3 will wire boot.js to send
/// the id, at which point persistence becomes the default).
///
/// Persistence failures are logged and swallowed: they never
/// turn a successful decrypt into a user-visible error. The
/// receive-side rendering path is the source of truth for "did
/// it work?"; the store is a best-effort durability layer.
pub fn cmd_osl_decrypt_message_with_id(
    state: &AppState,
    discord_message_id: Option<String>,
    channel_id: String,
    sender_discord_id: String,
    content: String,
) -> Result<String, String> {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;

    // Discord-id → OSL-user-id translation. Missing mapping is
    // common (every non-peer in a channel triggers it) and is
    // handled silently by the JS hook — surface a typed
    // UnknownSender so the hook can dedupe its log.
    let osl_user_id = {
        let map_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        match map_guard
            .get(&sender_discord_id)
            .and_then(|e| e.osl_user_id.clone())
        {
            Some(v) => v,
            None => {
                return Err(format!(
                    "OSL: {}",
                    DecodeError::UnknownSender {
                        discord_id: sender_discord_id,
                    }
                ));
            }
        }
    };

    // Pubkey lookup: cache → keyserver → cache-insert. Keyed by
    // OSL user_id (post-resolution) so the cache is stable across
    // peer_map re-edits.
    let sender_pub = if let Some(cached) = state.sender_pubkey_cache.get(&osl_user_id) {
        cached
    } else {
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        let resp = client
            .fetch_pubkeys(&osl_user_id)
            .map_err(|e| format!("OSL: fetch_pubkeys({osl_user_id}): {e}"))?;
        let pub_vec = STANDARD
            .decode(&resp.ik_x25519_pub)
            .map_err(|e| format!("OSL: decode sender pubkey ({osl_user_id}): {e}"))?;
        if pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: sender pubkey wrong length ({osl_user_id}): got {}, want {}",
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
            .insert(osl_user_id.clone(), pub_key.clone());
        pub_key
    };

    let plaintext_bytes =
        match decrypt_osl_phase4_cover(&identity.x25519_secret, &sender_pub, &content) {
            Ok(bytes) => bytes,
            Err(DecodeError::NoMatchingSlot) => {
                // Diagnostic: when NoMatchingSlot fires, surface the
                // wire's slot hints alongside our recipient hint so a
                // post-mortem can tell hint-mismatch (we're really
                // not a recipient) apart from
                // hint-match-but-AEAD-failed (key disagreement —
                // which static-static ECDH should never produce
                // intermittently). Falls back gracefully if the cover
                // is ill-formed.
                let recipient_pub = x25519::derive_public(&identity.x25519_secret);
                let our_hint = recipient_pub.as_bytes()[0];
                let diag = decode_slot_diagnostic(&content);
                return Err(format!(
                    "OSL: not a recipient of this message \
                 [diag: our_hint=0x{our_hint:02x} {diag} osl_user_id={osl_user_id}]"
                ));
            }
            Err(e) => return Err(format!("OSL: {e}")),
        };
    let plaintext = String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: decrypted plaintext is not valid UTF-8".to_string())?;

    // Drop the identity guard before touching the store mutex so
    // the two locks never overlap — keeps the lock graph trivially
    // free of cycles even when future callers hold both.
    drop(id_guard);

    if let Some(message_id) = discord_message_id {
        persist_decrypted(
            state,
            message_id,
            channel_id,
            sender_discord_id,
            osl_user_id,
            &plaintext,
        );
    }

    Ok(plaintext)
}

/// Best-effort persistence of a freshly decrypted message into
/// [`crate::state::AppState::message_store`]. Logs and swallows
/// every failure so a store outage cannot regress decrypt UX.
///
/// Skipped silently when the store is `None` (open failed at
/// bootstrap, or the user is running with persistence disabled).
fn persist_decrypted(
    state: &AppState,
    discord_message_id: String,
    channel_id: String,
    sender_discord_id: String,
    sender_osl_user_id: String,
    plaintext: &str,
) {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        tracing::debug!(
            discord_message_id = %discord_message_id,
            "OSL: message_store disabled; skipping persistence"
        );
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let msg = StoredMessage {
        discord_message_id: discord_message_id.clone(),
        channel_id,
        sender_discord_id,
        sender_osl_user_id,
        plaintext: plaintext.to_string(),
        decrypted_at: now,
        burned: false,
    };
    if let Err(e) = store.put(&msg) {
        tracing::warn!(
            discord_message_id = %discord_message_id,
            error = %e,
            "OSL: message_store.put failed; decrypt UX unaffected"
        );
    }
}

/// JS-facing DTO mirror of [`store::StoredMessage`]. The store
/// crate intentionally does not depend on `serde` (it's a pure
/// at-rest layer); this DTO crosses the IPC boundary and is the
/// shape boot.js sees on `osl_load_channel_history`.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct StoredMessageDto {
    pub discord_message_id: String,
    pub channel_id: String,
    pub sender_discord_id: String,
    pub sender_osl_user_id: String,
    pub plaintext: String,
    pub decrypted_at: i64,
    pub burned: bool,
}

impl From<StoredMessage> for StoredMessageDto {
    fn from(m: StoredMessage) -> Self {
        StoredMessageDto {
            discord_message_id: m.discord_message_id,
            channel_id: m.channel_id,
            sender_discord_id: m.sender_discord_id,
            sender_osl_user_id: m.sender_osl_user_id,
            plaintext: m.plaintext,
            decrypted_at: m.decrypted_at,
            burned: m.burned,
        }
    }
}

/// Default cap for [`cmd_osl_load_channel_history`] when the
/// caller passes `None`. Sized for a typical Discord channel
/// scrollback view (~one screen of messages).
pub const OSL_LOAD_HISTORY_DEFAULT_LIMIT: u32 = 100;

/// Layer 10 / Phase 5b2 IPC entry point: list previously
/// decrypted messages for a channel from the persistent store,
/// newest-first.
///
/// Returns an empty vector (not an error) when the store is
/// `None` — boot.js treats that as "no history to render" and
/// proceeds normally. Any other store error surfaces to the
/// caller as `Err(_)`.
///
/// `limit` defaults to [`OSL_LOAD_HISTORY_DEFAULT_LIMIT`] when
/// `None`. Callers may pass a higher cap if they need bulk
/// scrollback rehydration; the store's `list_by_channel`
/// streams the rows, so memory pressure scales with the cap.
pub fn cmd_osl_load_channel_history(
    state: &AppState,
    channel_id: String,
    limit: Option<u32>,
) -> Result<Vec<StoredMessageDto>, String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    let cap = limit.unwrap_or(OSL_LOAD_HISTORY_DEFAULT_LIMIT);
    let rows = store
        .list_by_channel(&channel_id, cap)
        .map_err(|e| format!("OSL: list_by_channel: {e}"))?;
    Ok(rows.into_iter().map(StoredMessageDto::from).collect())
}

/// Layer 10 / Phase 6a IPC entry point: re-persist a stored
/// message under a new plaintext after the user edited it
/// through Discord's edit flow.
///
/// Boot.js calls this from the PATCH-response load listener
/// once the edit's outbound network leg succeeds. The flow is:
///
/// 1. User edits a `DPC0::` message in Discord.
/// 2. Boot.js intercepts the PATCH, swaps `content` for a
///    fresh `DPC0::<base64>` cover, lets the request continue.
/// 3. Discord's response acknowledges the edit (200/204).
/// 4. Load listener calls this IPC with the *plaintext the
///    user typed* and the message_id from the URL.
///
/// On a known id: looks up the existing row to preserve
/// channel_id + sender_discord_id + sender_osl_user_id, then
/// upserts with `new_plaintext` and a fresh `decrypted_at`
/// (treating the edit time as the new "decrypted at" since
/// that's the moment the local store learned this plaintext).
/// `burned` is preserved as `false` — burned rows are filtered
/// from `store.get` so we'd already be on the unknown-id path
/// for those.
///
/// On an unknown id: idempotent no-op returning `Ok(())`. The
/// 2-arg signature can't construct a complete row without
/// channel/sender metadata, and the receive observer's normal
/// decrypt-and-persist path handles edit-before-decrypt
/// (which is exotic anyway — we'd have to have edited a
/// message we never saw bounce back, or one whose row was
/// burned). Surfacing an error here would only confuse the
/// boot.js caller, since the receive observer is also racing
/// to persist the same edit through the regular path.
///
/// Persistence is disabled when `state.message_store` is
/// `None`; we return `Ok(())` for the same reason
/// `cmd_osl_burn_message` does.
pub fn cmd_osl_persist_edit(
    state: &AppState,
    discord_message_id: String,
    new_plaintext: String,
) -> Result<(), String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    let existing = store
        .get(&discord_message_id)
        .map_err(|e| format!("OSL: persist_edit get: {e}"))?;
    let Some(prior) = existing else {
        // Edit-before-decrypt path — see fn-doc.
        return Ok(());
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let updated = StoredMessage {
        discord_message_id: prior.discord_message_id,
        channel_id: prior.channel_id,
        sender_discord_id: prior.sender_discord_id,
        sender_osl_user_id: prior.sender_osl_user_id,
        plaintext: new_plaintext,
        decrypted_at: now,
        burned: false,
    };
    store
        .put(&updated)
        .map_err(|e| format!("OSL: persist_edit put: {e}"))?;
    Ok(())
}

/// Layer 10 / Phase 5b2 IPC entry point: mark a message burned
/// in the persistent store. Subsequent `osl_load_channel_history`
/// calls will not return it, and `get`-style lookups skip it.
///
/// Idempotent: a burn against a non-existent
/// `discord_message_id` returns `Ok(())` (the row is gone or
/// was never persisted; either way the caller's intent — "this
/// message must not surface from the store" — is satisfied).
/// All other store errors surface as `Err(_)`.
///
/// Returns `Ok(())` (no-op) when the store is `None` so a UI
/// burn button doesn't error against a persistence-disabled
/// session.
pub fn cmd_osl_burn_message(state: &AppState, discord_message_id: String) -> Result<(), String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    match store.mark_burned(&discord_message_id) {
        Ok(()) => Ok(()),
        Err(StoreError::NotFound(_)) => Ok(()),
        Err(e) => Err(format!("OSL: mark_burned: {e}")),
    }
}

/// Pull diagnostic facts out of a Phase 4 cover string for the
/// NoMatchingSlot error path. Returns a single-line summary like
/// `version=0x01 N=2 hints=[0xab,0xcd]`, OR a fallback string
/// describing why the wire couldn't be inspected. Never fails —
/// designed to be safe to call on attacker-controlled covers.
///
/// **Information leak posture.** Slot hints are public (the
/// sender writes them in the clear) and our recipient hint is a
/// derived byte of our public identity key. Both are already
/// observable to anyone watching the channel, so surfacing them
/// in our own logs costs nothing.
fn decode_slot_diagnostic(cover: &str) -> String {
    let body = match cover.strip_prefix("DPC0::") {
        Some(b) => b,
        None => return "wire=<no DPC0:: prefix>".to_string(),
    };
    let raw = match STANDARD.decode(body) {
        Ok(r) => r,
        Err(e) => return format!("wire=<base64 error: {e}>"),
    };
    if raw.len() < 2 {
        return format!("wire=<too short: {} bytes>", raw.len());
    }
    let version = raw[0];
    let n = raw[1] as usize;
    let slot_size = OSL_PHASE4_PER_RECIPIENT_BYTES;
    let needed = OSL_PHASE4_FIXED_FRAMING_BYTES + n * slot_size;
    if raw.len() < needed {
        return format!(
            "wire=<truncated: have {} bytes, need {} for N={}>",
            raw.len(),
            needed,
            n
        );
    }
    let mut hints = String::with_capacity(2 + n * 5);
    hints.push('[');
    for slot_ix in 0..n {
        if slot_ix > 0 {
            hints.push(',');
        }
        let base = 2 + slot_ix * slot_size;
        hints.push_str(&format!("0x{:02x}", raw[base]));
    }
    hints.push(']');
    format!("version=0x{version:02x} N={n} hints={hints}")
}

// ---- Phase 7b: wire v=2 send-path commands ----
//
// All five send-path commands share the same shape: take an
// AppState reference + the caller's intent, construct a v=2 wire
// blob, and return it as a string for boot.js to ship through
// Discord's API. Persistence (writing the on-disk
// peer_map/whitelist_state/pending_invitations side-effects) is
// handled by separate "apply" commands; the send-side stays
// stateless beyond reading current state.

/// Helper: resolve the on-disk pubkey for a single Discord id, or
/// surface a stable string error suitable for boot.js logging.
fn lookup_peer_pubkey(
    peer_map: &crate::peer_map::PeerMap,
    discord_id: &str,
) -> Result<crypto::x25519::PublicKey, String> {
    let entry = peer_map
        .get(discord_id)
        .ok_or_else(|| format!("OSL: no peer entry for discord_id={discord_id}"))?;
    let b64 = entry
        .pubkey
        .as_deref()
        .ok_or_else(|| format!("OSL: no pubkey for discord_id={discord_id}"))?;
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| format!("OSL: peer pubkey base64 decode failed: {e}"))?;
    if bytes.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: peer pubkey length {} != {}",
            bytes.len(),
            crypto::x25519::PUBLIC_KEY_SIZE
        ));
    }
    let mut arr = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    arr.copy_from_slice(&bytes);
    Ok(crypto::x25519::PublicKey::from_bytes(arr))
}

/// Current unix-seconds timestamp, falling back to 0 if the clock
/// is somehow before the epoch.
fn now_unix_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Layer 10 / Phase 7b IPC entry point: encrypt a v=2 content
/// message for the whitelist-resolved recipients in `scope`.
///
/// Reads:
/// - `state.identity` for our x25519 (secret + public).
/// - `state.whitelist_state` + `state.peer_map` for scope
///   resolution.
///
/// Behaviour:
/// 1. Resolve recipients via
///    [`crate::whitelist::recipients_for_scope`]. Always includes
///    self.
/// 2. If the only recipient is self (no one to actually encrypt
///    *to*), return `"no_whitelisted_recipients"`. The
///    auto-self-include means the recipient list is never empty,
///    so we test against `len == 1` for the "no peers" case.
/// 3. Call [`crate::wire_v2::encrypt_v2`] with
///    `msg_type = MSG_TYPE_CONTENT (0x00)`.
/// 4. Return the `DPC0::<base64>` wire string.
///
/// Persistence of the local message row is deferred to the recv
/// path (when our own bounced message arrives) so the v=2
/// send-side stays uniform with v=1's existing flow.
pub fn cmd_osl_encrypt_message_v2(
    state: &AppState,
    plaintext: String,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);

    let recipients = {
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        crate::whitelist::recipients_for_scope(
            &ws_guard,
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    // 7d-PIVOT: encrypt_toggle is no longer coupled to having a
    // peer whitelist. `recipients_for_scope` always returns at
    // least self_pk (len >= 1); encrypt-to-self is a valid send
    // result (you alone will be able to decrypt). The caller
    // (boot.js v=2 send gate) decides whether to call this based
    // on the per-scope encrypt_toggle, independent of whitelist
    // size.

    crate::wire_v2::encrypt_v2(
        plaintext.as_bytes(),
        &recipients,
        crate::wire_v2::MSG_TYPE_CONTENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2: {e}"))
}

/// 7d-PIVOT-FIX2 Bug F: re-engaging a previously-burned scope by
/// sending a fresh encrypted message un-burns it. Idempotent — if
/// the scope isn't currently burned, returns `Ok(false)` and the
/// caller skips the cross-window event emit. The Tauri wrapper
/// (`osl_encrypt_message_v2`) calls this after a successful
/// encrypt and emits `osl:scope_unburned` when this returns true.
///
/// Scope-kind mapping matches `cmd_osl_set_whitelist`'s existing
/// auto-unburn-on-re-whitelist path.
pub fn cmd_osl_unburn_scope_after_encrypt(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> bool {
    // 7d-PIVOT-FIX3 Bug F: match the JS-style kind strings used by
    // `cmd_osl_mark_scope_burned` (the only writer of burned_scopes
    // entries). PIVOT-FIX2's "gc_full"/"server_channel_full" mapping
    // never matched anything in the ledger, so this helper silently
    // no-op'd and `osl:scope_unburned` was never emitted — which is
    // why FIX2's cross-window unburn never actually fired.
    let scope_kind_str = match scope_input.kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc",
        crate::scope::ScopeKind::ServerChannel => "server_channel",
        crate::scope::ScopeKind::ServerFull => "server_full",
    };
    cmd_osl_unburn_scope(state, scope_kind_str.to_string(), scope_input.id).unwrap_or(false)
}

/// Phase 8b: structured per-attachment input for
/// [`cmd_osl_encrypt_attachment_envelope`]. JS builds one of these
/// per file picked, then passes the whole list so the cover
/// references every attachment in the Discord message.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEnvelopeInput {
    pub att_key_b64: String,
    pub original_filename: String,
    pub random_filename: String,
    pub mime_type: String,
}

/// Phase 8 / 8b: encrypt an [`AttachmentEnvelope`] (list of per-
/// attachment entries) as a v=2 `MSG_TYPE_ATTACHMENT` message,
/// distributing every per-attachment AEAD key to every scope-
/// whitelisted recipient. The cover string returned is the
/// message-text payload boot.js drops into the `/messages` POST
/// body (replacing the user's typed plaintext on attachment sends).
/// Discord allows up to 10 attachments per message; the cover
/// covers all of them in a single CBOR list.
pub fn cmd_osl_encrypt_attachment_envelope(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    attachments: Vec<AttachmentEnvelopeInput>,
) -> Result<String, String> {
    if attachments.is_empty() {
        return Err("OSL: attachment envelope has no entries".to_string());
    }
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;

    let mut entries: Vec<crate::control_messages::AttachmentEnvelopeEntry> =
        Vec::with_capacity(attachments.len());
    for input in attachments {
        let key_bytes = STANDARD
            .decode(&input.att_key_b64)
            .map_err(|e| format!("OSL: att_key b64 decode: {e}"))?;
        if key_bytes.len() != 32 {
            return Err(format!(
                "OSL: att_key must be 32 bytes, got {}",
                key_bytes.len()
            ));
        }
        let mut att_key = [0u8; 32];
        att_key.copy_from_slice(&key_bytes);
        entries.push(crate::control_messages::AttachmentEnvelopeEntry {
            att_key,
            original_filename: input.original_filename,
            random_filename: input.random_filename,
            mime_type: input.mime_type,
        });
    }

    let env = crate::control_messages::AttachmentEnvelope {
        attachments: entries,
    };
    let env_bytes = crate::control_messages::serialize_attachment_envelope(&env)
        .map_err(|e| format!("OSL: serialize attachment envelope: {e}"))?;

    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);

    let recipients = {
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        crate::whitelist::recipients_for_scope(
            &ws_guard,
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    crate::wire_v2::encrypt_v2(
        &env_bytes,
        &recipients,
        crate::wire_v2::MSG_TYPE_ATTACHMENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 (attachment envelope): {e}"))
}

/// Phase 8d output: one-shot seal that returns everything JS needs
/// to upload + reference the file. The cover envelope lives INSIDE
/// `sealed_b64`; no separate cover-on-the-wire is needed.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedAttachmentV2 {
    pub sealed_b64: String,
    pub random_filename: String,
    pub mime_type: String,
}

/// Phase 8d: one-shot seal. Generates a fresh per-attachment AEAD
/// key, builds the v=2 cover (multi-recipient envelope carrying
/// that AEAD key + filenames + MIME) using the existing
/// MSG_TYPE_ATTACHMENT path, seals the file payload with the AEAD
/// key, and assembles the V2 wire bundle. The JS caller never sees
/// the AEAD key — it lives only inside the embedded cover, which
/// only whitelisted recipients can decrypt.
#[allow(clippy::too_many_arguments)]
pub fn cmd_osl_seal_attachment_with_cover_v2(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    original_bytes_b64: String,
    original_filename: String,
    random_filename: String,
) -> Result<SealedAttachmentV2, String> {
    let mime = crate::attachment_wire::mime_for_filename(&original_filename)
        .ok_or_else(|| "OSL: unsupported file extension".to_string())?;
    let original_bytes = STANDARD
        .decode(&original_bytes_b64)
        .map_err(|e| format!("OSL: original_bytes b64 decode: {e}"))?;

    // Fresh attachment AEAD key. Lives only here + inside the
    // ciphered cover; never returned to JS.
    let key_bytes = random::random_bytes(32);
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key_bytes);
    let att_key = crypto::aead::Key::from_bytes(key_arr);

    // Build the CBOR-encoded envelope (single-attachment list).
    let env = crate::control_messages::AttachmentEnvelope {
        attachments: vec![crate::control_messages::AttachmentEnvelopeEntry {
            att_key: key_arr,
            original_filename: original_filename.clone(),
            random_filename: random_filename.clone(),
            mime_type: mime.to_string(),
        }],
    };
    let env_bytes = crate::control_messages::serialize_attachment_envelope(&env)
        .map_err(|e| format!("OSL: serialize attachment envelope: {e}"))?;

    // Resolve identity + recipients for the v=2 multi-recipient
    // wrap of the cover.
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);
    let recipients = {
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        crate::whitelist::recipients_for_scope(
            &ws_guard,
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    // Cover wire — the same shape MSG_TYPE_ATTACHMENT would have
    // had on the message text; here we embed the raw v=2 wire
    // bytes (no DPC0:: base64-string framing) inside the file.
    let cover_wire_str = crate::wire_v2::encrypt_v2(
        &env_bytes,
        &recipients,
        crate::wire_v2::MSG_TYPE_ATTACHMENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 cover: {e}"))?;
    let cover_bytes = STANDARD
        .decode(
            cover_wire_str
                .strip_prefix("DPC0::")
                .unwrap_or(&cover_wire_str),
        )
        .map_err(|e| format!("OSL: cover wire b64 decode: {e}"))?;

    // Seal the file with the AEAD key + embed the cover.
    let sealed_bytes = crate::attachment_wire::seal_attachment_v2(
        att_key,
        &original_bytes,
        &original_filename,
        &cover_bytes,
    )
    .map_err(|e| format!("OSL: seal_attachment_v2: {e}"))?;

    Ok(SealedAttachmentV2 {
        sealed_b64: STANDARD.encode(&sealed_bytes),
        random_filename,
        mime_type: mime.to_string(),
    })
}

/// Phase 8d: one-shot open. Splits the file into (cover, filename,
/// payload), decrypts the cover via the existing v=2 path, recovers
/// the per-attachment AEAD key from the envelope, then decrypts the
/// payload. Backwards-compatible with V1 files (signaled by the
/// empty cover from `open_attachment_v2_split`) — falls back to the
/// caller-supplied legacy `att_key_b64` argument for V1 only.
pub fn cmd_osl_open_attachment_v2(
    state: &AppState,
    sender_discord_id: String,
    scope_input: Option<crate::scope::ScopeInput>,
    file_bytes_b64: String,
    legacy_att_key_b64: Option<String>,
) -> Result<crate::attachment_wire::OpenedAttachment, String> {
    let file_bytes = STANDARD
        .decode(&file_bytes_b64)
        .map_err(|e| format!("OSL: file_bytes b64 decode: {e}"))?;
    let (cover_bytes, filename, payload_bytes) =
        crate::attachment_wire::open_attachment_v2_split(&file_bytes)
            .map_err(|e| format!("OSL: open_attachment_v2_split: {e}"))?;

    // Recover the attachment AEAD key. V2 path: decrypt the
    // embedded cover via v=2 (uses our SK + sender PK + scope
    // whitelist gate). V1 path: trust the caller-supplied
    // att_key_b64 (legacy Phase 8/8c flow).
    let att_key_arr: [u8; 32] = if !cover_bytes.is_empty() {
        // Build the DPC0:: string the v=2 decoder expects.
        let cover_wire = format!("DPC0::{}", STANDARD.encode(&cover_bytes));
        // V2 cover MUST be MSG_TYPE_ATTACHMENT — dispatch through
        // cmd_osl_decrypt_message_v2 to honour the scope gate.
        let recovered = cmd_osl_decrypt_message_v2(
            state,
            None,
            // channel_id is unused for MSG_TYPE_ATTACHMENT processing.
            String::new(),
            sender_discord_id,
            cover_wire,
            scope_input,
            None,
        )?;
        if !recovered.starts_with(OSL_RESULT_ATTACHMENT_PREFIX) {
            return Err(format!(
                "OSL: V2 cover did not decode to attachment sentinel: {recovered}"
            ));
        }
        let json_part = recovered.trim_start_matches(OSL_RESULT_ATTACHMENT_PREFIX);
        let v: serde_json::Value = serde_json::from_str(json_part)
            .map_err(|e| format!("OSL: V2 cover sentinel JSON: {e}"))?;
        let arr = v["attachments"]
            .as_array()
            .ok_or_else(|| "OSL: V2 cover missing attachments[]".to_string())?;
        if arr.is_empty() {
            return Err("OSL: V2 cover attachments[] is empty".to_string());
        }
        // V2 currently has one entry per file (multi-file
        // messages get one cover per file, embedded in each
        // file's wire).
        let entry = &arr[0];
        let key_b64 = entry["attKey"]
            .as_str()
            .ok_or_else(|| "OSL: V2 cover missing attKey".to_string())?;
        let key_bytes = STANDARD
            .decode(key_b64)
            .map_err(|e| format!("OSL: V2 cover attKey b64: {e}"))?;
        if key_bytes.len() != 32 {
            return Err(format!(
                "OSL: V2 cover attKey length {} != 32",
                key_bytes.len()
            ));
        }
        let mut k = [0u8; 32];
        k.copy_from_slice(&key_bytes);
        k
    } else {
        // V1 path: caller must supply the AEAD key.
        let b64 = legacy_att_key_b64
            .ok_or_else(|| "OSL: V1 file with no legacy att_key supplied".to_string())?;
        let key_bytes = STANDARD
            .decode(&b64)
            .map_err(|e| format!("OSL: legacy att_key b64: {e}"))?;
        if key_bytes.len() != 32 {
            return Err(format!(
                "OSL: legacy att_key length {} != 32",
                key_bytes.len()
            ));
        }
        let mut k = [0u8; 32];
        k.copy_from_slice(&key_bytes);
        k
    };
    let file_key = crypto::aead::Key::from_bytes(att_key_arr);
    let plaintext = crypto::attachment::decrypt_attachment(file_key, &payload_bytes)
        .map_err(|e| format!("OSL: decrypt_attachment: {e:?}"))?;
    let mime = crate::attachment_wire::mime_for_filename(&filename)
        .ok_or_else(|| "OSL: unsupported file extension on decrypted name".to_string())?;
    Ok(crate::attachment_wire::OpenedAttachment {
        plaintext_b64: STANDARD.encode(&plaintext),
        original_filename: filename,
        mime_type: mime.to_string(),
    })
}

/// Layer 10 / Phase 7b: send a burn marker for `scope` to the
/// channel members who'd have been able to decrypt content in it
/// (so they wipe their decryption capability).
///
/// Recipient set is computed via `recipients_for_scope` **before**
/// the local burn-state mutation lands — callers must call this
/// before `cmd_osl_apply_burn` updates `peer_map.burned_scopes`,
/// otherwise the burned recipients would be filtered out of the
/// recipient list and never receive the burn notice. The Tauri
/// wrapper in `cmd_osl_unwhitelist_scope` enforces this ordering.
pub fn cmd_osl_send_burn_marker(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);

    let recipients = {
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        crate::whitelist::recipients_for_scope(
            &ws_guard,
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };
    if recipients.len() <= 1 {
        return Err("no_whitelisted_recipients".to_string());
    }

    let marker = crate::control_messages::BurnMarker {
        scope,
        burned_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_burn_marker(&marker)
        .map_err(|e| format!("OSL: serialize burn_marker: {e}"))?;
    crate::wire_v2::encrypt_v2(
        &body,
        &recipients,
        crate::wire_v2::MSG_TYPE_BURN,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 burn_marker: {e}"))
}

/// Layer 10 / Phase 7b: build + encrypt a whitelist invitation
/// for `to_discord_id` covering `scope`. The receiver's UI
/// surfaces this as a banner; on accept they'll set
/// `peer_map[<sender>].incoming_decrypt_accepted[scope_key]
/// = true` and our subsequent v=2 messages in `scope` decrypt
/// for them.
///
/// `from_discord_id` is supplied by the caller (boot.js, which
/// extracts our Discord user_id from the page state). The
/// [`Identity`] only carries the keyserver `user_id`, not the
/// Discord snowflake.
pub fn cmd_osl_send_whitelist_invitation(
    state: &AppState,
    to_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    from_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let from_pubkey = identity.x25519_public;
    drop(id_guard);

    let to_pubkey = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        lookup_peer_pubkey(&pm_guard, &to_discord_id)?
    };

    let invitation = crate::control_messages::WhitelistInvitation {
        from_discord_id,
        from_pubkey,
        scope,
        sent_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_whitelist_invitation(&invitation)
        .map_err(|e| format!("OSL: serialize whitelist_invitation: {e}"))?;
    crate::wire_v2::encrypt_v2(
        &body,
        &[to_pubkey],
        crate::wire_v2::MSG_TYPE_WHITELIST_INVITATION,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 whitelist_invitation: {e}"))
}

/// Layer 10 / Phase 7b: build + encrypt a whitelist response
/// (accept or decline) to `to_discord_id` for `scope`. The
/// original inviter's UI consumes this to mark the
/// outgoing-whitelist entry as accepted/declined.
pub fn cmd_osl_send_whitelist_response(
    state: &AppState,
    to_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    accepted: bool,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    drop(id_guard);

    let to_pubkey = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        lookup_peer_pubkey(&pm_guard, &to_discord_id)?
    };

    let response = crate::control_messages::WhitelistResponse {
        scope,
        accepted,
        responded_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_whitelist_response(&response)
        .map_err(|e| format!("OSL: serialize whitelist_response: {e}"))?;
    crate::wire_v2::encrypt_v2(
        &body,
        &[to_pubkey],
        crate::wire_v2::MSG_TYPE_WHITELIST_RESPONSE,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 whitelist_response: {e}"))
}

// ---- Phase 7b: recv-path branching + helper commands ----
//
// Sentinel return strings for `cmd_osl_decrypt_message_v2` when
// the body is a control message rather than user-visible content.
// boot.js dispatches on these prefixes via `oslHandleDecryptResult`.

/// Returned by the recv path when a v=2 burn marker was
/// processed (peer_map + sqlite mutated). Boot.js re-renders the
/// message as ciphertext when it sees this.
pub const OSL_RESULT_BURN_APPLIED: &str = "__OSL_CONTROL_BURN_APPLIED__";

/// Returned when a v=2 invitation was queued in
/// `pending_invitations`.
pub const OSL_RESULT_INVITATION_RECEIVED: &str = "__OSL_CONTROL_INVITATION_RECEIVED__";

/// Returned when a v=2 response updated
/// `peer_map.outgoing_whitelist_responses`.
pub const OSL_RESULT_RESPONSE_RECEIVED: &str = "__OSL_CONTROL_RESPONSE_RECEIVED__";

/// Phase 8 attachment-envelope sentinel prefix. The recv path returns
/// `__OSL_CONTROL_ATTACHMENT__|<json-envelope>` when a v=2
/// `MSG_TYPE_ATTACHMENT` message is decrypted; boot.js splits on the
/// `|` and uses the JSON to call `osl_open_attachment` against the
/// CDN-fetched blob.
pub const OSL_RESULT_ATTACHMENT_PREFIX: &str = "__OSL_CONTROL_ATTACHMENT__|";

/// Phase 7b recv-path entry point covering both wire versions.
/// Peeks the wire's version byte after base64 decode and routes
/// to the appropriate path:
///
/// - v=1 → delegate to [`cmd_osl_decrypt_message_with_id`] (the
///   Phase 5/6 path, untouched).
/// - v=2 → run the v=2 decode + dispatch on msg_type:
///   - 0x00 content: gate via [`crate::whitelist::should_decrypt_from`]
///     when `scope_input` is `Some`, return plaintext as String.
///   - 0x01 burn marker: apply the burn locally, return
///     [`OSL_RESULT_BURN_APPLIED`].
///   - 0x02 invitation: enqueue in pending_invitations, return
///     [`OSL_RESULT_INVITATION_RECEIVED`].
///   - 0x03 response: update peer_map.outgoing_whitelist_responses,
///     return [`OSL_RESULT_RESPONSE_RECEIVED`].
///
/// `scope_input` is **optional**: callers that don't yet know
/// the scope (legacy Phase 5/6 boot.js) pass `None` and the v=2
/// gate is skipped. Phase 7c boot.js will populate it.
///
/// `config_dir` is required for the invitation-enqueue side
/// effect (writes pending_invitations.json). Passed by the
/// Tauri wrapper as `keystore::osl_config_dir()`.
#[allow(clippy::too_many_arguments)]
pub fn cmd_osl_decrypt_message_v2(
    state: &AppState,
    discord_message_id: Option<String>,
    channel_id: String,
    sender_discord_id: String,
    content: String,
    scope_input: Option<crate::scope::ScopeInput>,
    config_dir: Option<std::path::PathBuf>,
) -> Result<String, String> {
    // Peek the wire version.
    let version = peek_wire_version(&content);
    if version != Some(crate::wire_v2::WIRE_VERSION_V2) {
        // v=1 (or unknown): preserve the existing Phase 5 path.
        return cmd_osl_decrypt_message_with_id(
            state,
            discord_message_id,
            channel_id,
            sender_discord_id,
            content,
        );
    }

    // v=2 path.
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let our_sk = identity.x25519_secret.clone();
    drop(id_guard);

    // Resolve sender pubkey from peer_map (v=2 carries it via
    // invitations, so by the time we have content from a peer
    // we should have their pubkey). Fall back to the keyserver
    // round-trip path used by v=1 if it's not on file yet.
    let sender_pub = resolve_sender_pubkey(state, &sender_discord_id)?;

    let scope_opt: Option<crate::scope::Scope> = match scope_input {
        Some(input) => Some(
            input
                .try_into()
                .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?,
        ),
        None => None,
    };

    let recovered = crate::wire_v2::decrypt_v2(&content, &our_sk, &sender_pub)
        .map_err(|e| format!("OSL: {e}"))?;

    match recovered.msg_type {
        crate::wire_v2::MSG_TYPE_CONTENT => {
            if let Some(scope) = scope_opt.as_ref() {
                let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
                if !crate::whitelist::should_decrypt_from(&pm_guard, scope, &sender_discord_id) {
                    return Err("sender_not_accepted_in_scope".to_string());
                }
            }
            String::from_utf8(recovered.plaintext)
                .map_err(|_| "OSL: decrypted plaintext is not valid UTF-8".to_string())
        }
        crate::wire_v2::MSG_TYPE_BURN => {
            let marker = crate::control_messages::deserialize_burn_marker(&recovered.plaintext)
                .map_err(|e| format!("OSL: deserialize burn_marker: {e}"))?;
            apply_burn_recv(state, &sender_discord_id, &marker)?;
            Ok(OSL_RESULT_BURN_APPLIED.to_string())
        }
        crate::wire_v2::MSG_TYPE_WHITELIST_INVITATION => {
            let invitation =
                crate::control_messages::deserialize_whitelist_invitation(&recovered.plaintext)
                    .map_err(|e| format!("OSL: deserialize invitation: {e}"))?;
            enqueue_invitation_recv(
                state,
                &sender_discord_id,
                &invitation,
                config_dir.as_deref(),
            )?;
            Ok(OSL_RESULT_INVITATION_RECEIVED.to_string())
        }
        crate::wire_v2::MSG_TYPE_WHITELIST_RESPONSE => {
            let response =
                crate::control_messages::deserialize_whitelist_response(&recovered.plaintext)
                    .map_err(|e| format!("OSL: deserialize response: {e}"))?;
            apply_response_recv(state, &sender_discord_id, &response)?;
            Ok(OSL_RESULT_RESPONSE_RECEIVED.to_string())
        }
        crate::wire_v2::MSG_TYPE_ATTACHMENT => {
            // Phase 8: enforce the same per-scope acceptance gate
            // that MSG_TYPE_CONTENT does. Anyone whitelisted in the
            // scope can decrypt the attachment; anyone NOT
            // whitelisted should see the decoy + ciphertext, just
            // like an unwhitelisted text message.
            if let Some(scope) = scope_opt.as_ref() {
                let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
                if !crate::whitelist::should_decrypt_from(&pm_guard, scope, &sender_discord_id) {
                    return Err("sender_not_accepted_in_scope".to_string());
                }
            }
            let env =
                crate::control_messages::deserialize_attachment_envelope(&recovered.plaintext)
                    .map_err(|e| format!("OSL: deserialize attachment envelope: {e}"))?;
            // 8b: serialize the full attachments list for JS — JS
            // dispatches on the sentinel prefix and iterates the
            // `attachments` array, feeding each entry into
            // `osl_open_attachment` along with the matching CDN-
            // fetched file bytes.
            let attachments_json: Vec<serde_json::Value> = env
                .attachments
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "attKey": STANDARD.encode(e.att_key),
                        "originalFilename": e.original_filename,
                        "randomFilename": e.random_filename,
                        "mimeType": e.mime_type,
                    })
                })
                .collect();
            let json = serde_json::json!({ "attachments": attachments_json });
            Ok(format!("{}{}", OSL_RESULT_ATTACHMENT_PREFIX, json))
        }
        other => Err(format!(
            "OSL: v=2 msg_type 0x{other:02x} not supported by this client"
        )),
    }
}

/// Peek the wire version byte. Returns `None` for non-DPC0::
/// content or malformed base64.
fn peek_wire_version(cover: &str) -> Option<u8> {
    let body = cover.strip_prefix("DPC0::")?;
    let bytes = STANDARD.decode(body).ok()?;
    bytes.first().copied()
}

/// Resolve sender pubkey. Prefers `peer_map[sender].pubkey` (v=2
/// invitations carry pubkeys, so by content-message time we
/// expect this populated); falls back to the legacy
/// keyserver round-trip via `peer_map[sender].osl_user_id`.
fn resolve_sender_pubkey(
    state: &AppState,
    sender_discord_id: &str,
) -> Result<crypto::x25519::PublicKey, String> {
    let osl_user_id_for_lookup = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        if let Ok(pk) = lookup_peer_pubkey(&pm_guard, sender_discord_id) {
            return Ok(pk);
        }
        // Fall back to keyserver path: need the osl_user_id.
        pm_guard
            .get(sender_discord_id)
            .and_then(|e| e.osl_user_id.clone())
    };
    let osl_user_id = osl_user_id_for_lookup.ok_or_else(|| {
        format!(
            "OSL: {}",
            DecodeError::UnknownSender {
                discord_id: sender_discord_id.to_string(),
            }
        )
    })?;
    if let Some(cached) = state.sender_pubkey_cache.get(&osl_user_id) {
        return Ok(cached);
    }
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard
        .as_ref()
        .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
    let resp = client
        .fetch_pubkeys(&osl_user_id)
        .map_err(|e| format!("OSL: fetch_pubkeys({osl_user_id}): {e}"))?;
    let pub_vec = STANDARD
        .decode(&resp.ik_x25519_pub)
        .map_err(|e| format!("OSL: decode sender pubkey ({osl_user_id}): {e}"))?;
    if pub_vec.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: sender pubkey wrong length ({osl_user_id}): got {}",
            pub_vec.len()
        ));
    }
    let mut bytes = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    bytes.copy_from_slice(&pub_vec);
    let pub_key = crypto::x25519::PublicKey::from_bytes(bytes);
    drop(ks_guard);
    state.sender_pubkey_cache.insert(osl_user_id, pub_key);
    Ok(pub_key)
}

/// Apply a received burn marker:
/// - Append a [`crate::peer_map::BurnedScope`] entry to
///   `peer_map[sender].burned_scopes` (idempotent — duplicates
///   skipped).
/// - Wipe `wrapped_key` on matching rows in `messages.sqlite`
///   (best-effort; failures logged, not propagated).
fn apply_burn_recv(
    state: &AppState,
    sender_discord_id: &str,
    marker: &crate::control_messages::BurnMarker,
) -> Result<(), String> {
    use crate::peer_map::BurnedScope as B;
    let burned_at_iso = format_iso8601_secs(marker.burned_at).unwrap_or_else(|| "?".to_string());
    let entry = match marker.scope.kind {
        crate::scope::ScopeKind::Dm => B::Dm {
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::Gc => B::Gc {
            id: marker.scope.id.clone(),
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::ServerChannel => B::ServerChannel {
            server_id: marker.scope.server_id.clone().unwrap_or_default(),
            channel_id: marker.scope.channel_id.clone().unwrap_or_default(),
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::ServerFull => B::ServerFull {
            server_id: marker.scope.server_id.clone().unwrap_or_default(),
            burned_at: burned_at_iso,
        },
    };
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(sender_discord_id.to_string()).or_default();
        if !pe.burned_scopes.iter().any(|b| same_burn(b, &entry)) {
            pe.burned_scopes.push(entry);
        }
    }
    // Wipe sqlite wrapped_keys for the scope (best-effort).
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        let (scope_type, scope_id) = scope_storage_pair(&marker.scope);
        if let Err(e) = store.wipe_wrapped_keys_in_scope(&scope_type, &scope_id) {
            tracing::warn!(error = %e, "OSL: wipe wrapped_keys failed; burn proceeded in peer_map only");
        }
    }
    // 7d-FIX1: persist peer_map (the burned-scope entry is new state).
    persist_peer_map_now(state);
    Ok(())
}

/// Enqueue a whitelist invitation in pending_invitations. ID
/// shape: `from_<sender_discord_id>_<scope_storage_key>` for
/// idempotency — re-receiving the same invitation is a no-op.
fn enqueue_invitation_recv(
    state: &AppState,
    sender_discord_id: &str,
    invitation: &crate::control_messages::WhitelistInvitation,
    config_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    use crate::pending_invitations::{InvitationStatus, PendingInvitation};
    let invitation_id = format!(
        "from_{}_{}",
        sender_discord_id,
        invitation.scope.storage_key()
    );
    let entry = PendingInvitation {
        from: sender_discord_id.to_string(),
        scope: format!("{:?}", invitation.scope.kind).to_ascii_lowercase(),
        scope_id: Some(invitation.scope.storage_key()),
        received_at: format_iso8601_secs(invitation.sent_at).unwrap_or_else(|| "?".to_string()),
        status: InvitationStatus::Pending,
    };
    {
        let mut pi_guard = state
            .pending_invitations
            .lock()
            .expect("pending_invitations mutex poisoned");
        pi_guard.entry(invitation_id).or_insert(entry);
    }
    // Persist the sender's pubkey eagerly — the invitation
    // carries it.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(sender_discord_id.to_string()).or_default();
        pe.pubkey = Some(STANDARD.encode(invitation.from_pubkey.as_bytes()));
        pe.discord_id
            .get_or_insert_with(|| sender_discord_id.to_string());
    }
    // 7d-FIX1: persist peer_map (the new pubkey is new state).
    persist_peer_map_now(state);
    // Write-through to disk if we have a config_dir.
    if let Some(dir) = config_dir {
        let pi_path = dir.join("pending_invitations.json");
        let pi_guard = state
            .pending_invitations
            .lock()
            .expect("pending_invitations mutex poisoned");
        if let Err(e) = crate::pending_invitations::write_pending_invitations(&pi_path, &pi_guard) {
            tracing::warn!(error = %e, "OSL: write pending_invitations failed");
        }
    }
    Ok(())
}

/// Apply a received whitelist response: update
/// `peer_map[sender].outgoing_whitelist_responses[scope]`.
fn apply_response_recv(
    state: &AppState,
    sender_discord_id: &str,
    response: &crate::control_messages::WhitelistResponse,
) -> Result<(), String> {
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(sender_discord_id.to_string()).or_default();
        pe.outgoing_whitelist_responses
            .insert(response.scope.storage_key(), response.accepted);
    }
    // 7d-FIX1: persist peer_map.
    persist_peer_map_now(state);
    Ok(())
}

fn same_burn(a: &crate::peer_map::BurnedScope, b: &crate::peer_map::BurnedScope) -> bool {
    use crate::peer_map::BurnedScope as B;
    match (a, b) {
        (B::Dm { .. }, B::Dm { .. }) => true,
        (B::Gc { id: a, .. }, B::Gc { id: bid, .. }) => a == bid,
        (
            B::ServerChannel {
                server_id: sa,
                channel_id: ca,
                ..
            },
            B::ServerChannel {
                server_id: sb,
                channel_id: cb,
                ..
            },
        ) => sa == sb && ca == cb,
        (B::ServerFull { server_id: a, .. }, B::ServerFull { server_id: b, .. }) => a == b,
        _ => false,
    }
}

fn scope_storage_pair(s: &crate::scope::Scope) -> (String, String) {
    use crate::scope::ScopeKind as K;
    let kind = match s.kind {
        K::Dm => "dm",
        K::Gc => "gc",
        K::ServerChannel => "server_channel",
        K::ServerFull => "server_full",
    };
    (kind.to_string(), s.id.clone())
}

/// Stringify a unix-seconds timestamp for storage in the
/// `burned_at` / `enabled_at` / `received_at` ISO-style fields on
/// `peer_map` / `pending_invitations`. The design doc shows
/// ISO-8601, but Phase 7b stores the raw unix-seconds string
/// (e.g. `"1700000000"`) to avoid pulling a date-formatting
/// dep — every consumer in the v=7 codepaths treats these fields
/// as opaque strings already. Phase 7c can swap to true
/// ISO-8601 when the UI needs it (and the JS layer renders
/// formatted dates from a fresh `Date.now()` anyway).
fn format_iso8601_secs(unix_secs: i64) -> Option<String> {
    if unix_secs < 0 {
        return None;
    }
    Some(unix_secs.to_string())
}

// ---- Phase 7b: helper Tauri-callable commands ----

/// Apply a local burn for `scope`. Updates self-side state only —
/// the wire burn marker is sent by `cmd_osl_unwhitelist_scope` /
/// `cmd_osl_send_burn_marker` before this is called, so by the
/// time we're here the recipients have already been notified.
///
/// Effects:
/// - Wipes `wrapped_key` on matching rows in messages.sqlite
///   (so we can't re-decrypt our outgoing history in `scope`).
/// - **Does not** mutate any peer_map entry. Peer-side burn
///   tracking lives in their burned_scopes; ours is implicit
///   via the wiped wrapped_keys + the scope no longer being in
///   whitelist_state.
pub fn cmd_osl_apply_burn(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let (scope_type, scope_id) = scope_storage_pair(&scope);
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        store
            .wipe_wrapped_keys_in_scope(&scope_type, &scope_id)
            .map_err(|e| format!("OSL: wipe wrapped_keys: {e}"))?;
    }
    Ok(())
}

/// Accept a pending whitelist invitation. Side effects:
/// - Set `peer_map[from].incoming_decrypt_accepted[scope] = true`
///   so subsequent v=2 content from that sender + scope decrypts.
/// - Remove the entry from pending_invitations (Phase 7c could
///   alternatively mark it `Accepted` for a brief confirmation
///   banner before removal; 7b removes outright).
pub fn cmd_osl_accept_invitation(state: &AppState, invitation_id: String) -> Result<(), String> {
    apply_invitation_decision(state, &invitation_id, true)
}

/// Decline a pending whitelist invitation. Side effects: same
/// shape as `accept` but stores `false`.
pub fn cmd_osl_decline_invitation(state: &AppState, invitation_id: String) -> Result<(), String> {
    apply_invitation_decision(state, &invitation_id, false)
}

fn apply_invitation_decision(
    state: &AppState,
    invitation_id: &str,
    accepted: bool,
) -> Result<(), String> {
    // Pull the invitation out of pending.
    let invitation = {
        let mut pi_guard = state
            .pending_invitations
            .lock()
            .expect("pending_invitations mutex poisoned");
        pi_guard
            .remove(invitation_id)
            .ok_or_else(|| format!("OSL: invitation '{invitation_id}' not pending"))?
    };
    let scope_key = invitation
        .scope_id
        .ok_or_else(|| "OSL: invitation missing scope_id".to_string())?;
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(invitation.from.clone()).or_default();
        pe.incoming_decrypt_accepted.insert(scope_key, accepted);
    }
    // 7d-FIX1: persist peer_map. Also persist pending_invitations
    // since we mutated it above (remove `invitation_id`).
    persist_peer_map_now(state);
    if let Ok(dir) = keystore::osl_config_dir() {
        let path = dir.join("pending_invitations.json");
        let pi = state
            .pending_invitations
            .lock()
            .expect("pending_invitations mutex poisoned");
        let _ = crate::pending_invitations::write_pending_invitations(&path, &pi);
    }
    Ok(())
}

/// Remove a whitelist entry for `peer` in `scope`. Returns the
/// wire-format burn marker the caller must send through Discord's
/// API so the peer's client wipes its decrypt capability.
///
/// Behaviour:
/// - Compute burn marker recipients via `recipients_for_scope`
///   **before** mutating state (so the burned peer is still in
///   the recipient list).
/// - Encrypt as v=2 type=0x01.
/// - Mutate local state:
///   - Remove the `WhitelistEntry` from
///     `peer_map[peer].outgoing_whitelists` that matches `scope`.
///   - Append a `BurnedScope` to the same peer's `burned_scopes`.
///   - If `scope.kind == Dm` and `revoke_broadened`: clear the
///     `broadened` flag on any DM whitelist entry for this peer.
///     Per §3.4 this revokes their cross-scope grant in shared
///     GCs/servers without burning those scopes individually.
///   - Drop the scope's entry from `whitelist_state`.
/// - Call `cmd_osl_apply_burn` to wipe wrapped_keys.
///
/// Returns the wire-format burn marker string.
pub fn cmd_osl_unwhitelist_scope(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    revoke_broadened: bool,
) -> Result<String, String> {
    // 1. Build the burn marker wire BEFORE mutating state.
    let wire =
        cmd_osl_send_burn_marker(state, scope_input.clone(), channel_members, self_discord_id)?;
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;

    // 2. Mutate peer_map for the named peer.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(peer_discord_id.clone()).or_default();
        pe.outgoing_whitelists
            .retain(|w| !whitelist_entry_matches(w, &scope));
        let burned_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
        let burn = match scope.kind {
            crate::scope::ScopeKind::Dm => crate::peer_map::BurnedScope::Dm {
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::Gc => crate::peer_map::BurnedScope::Gc {
                id: scope.id.clone(),
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::ServerChannel => crate::peer_map::BurnedScope::ServerChannel {
                server_id: scope.server_id.clone().unwrap_or_default(),
                channel_id: scope.channel_id.clone().unwrap_or_default(),
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::ServerFull => crate::peer_map::BurnedScope::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                burned_at: burned_at_iso,
            },
        };
        if !pe.burned_scopes.iter().any(|b| same_burn(b, &burn)) {
            pe.burned_scopes.push(burn);
        }
        if revoke_broadened && scope.kind == crate::scope::ScopeKind::Dm {
            for w in pe.outgoing_whitelists.iter_mut() {
                if let crate::peer_map::WhitelistEntry::Dm { broadened, .. } = w {
                    *broadened = false;
                }
            }
        }
    }

    // 3. Remove the named peer from the scope's whitelist
    //    entry, but KEEP `encrypt_toggle` intact (7d-PIVOT
    //    decision Q3:B — scope burn destroys data + removes
    //    whitelist for this peer, but the user's per-scope
    //    encrypt preference survives). The scope entry stays
    //    in whitelist_state as long as encrypt_toggle is set
    //    or any other peers remain whitelisted; otherwise we
    //    drop the empty entry to keep the file compact.
    {
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let key = scope.storage_key();
        let drop_entry = if let Some(entry) = ws_guard.get_mut(&key) {
            entry.members.retain(|m| m != &peer_discord_id);
            entry.whitelisted_users.retain(|u| u != &peer_discord_id);
            // Drop only if there's nothing left worth persisting.
            !entry.encrypt_toggle
                && entry.members.is_empty()
                && entry.whitelisted_users.is_empty()
                && !entry.full_whitelist
        } else {
            false
        };
        if drop_entry {
            ws_guard.remove(&key);
        }
    }

    // 4. Wipe wrapped_keys.
    let _ = cmd_osl_apply_burn(state, (&scope).into());

    // 7d-FIX1: persist peer_map + whitelist_state.
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    Ok(wire)
}

fn whitelist_entry_matches(w: &crate::peer_map::WhitelistEntry, s: &crate::scope::Scope) -> bool {
    use crate::peer_map::WhitelistEntry as W;
    use crate::scope::ScopeKind as K;
    match (w, &s.kind) {
        (W::Dm { .. }, K::Dm) => true,
        (W::Gc { id, .. }, K::Gc) => id == &s.id,
        (
            W::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            K::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (W::ServerFull { server_id, .. }, K::ServerFull) => Some(server_id) == s.server_id.as_ref(),
        _ => false,
    }
}

/// Set a whitelist for `peer_discord_id` in `scope`. Returns the
/// wire-format invitation the caller must send through Discord's
/// API so the peer can accept/decline.
///
/// Behaviour:
/// - Mutate peer_map: append/replace `WhitelistEntry` for the
///   scope. For DM scope, the `broadened` flag carries through.
/// - Mutate whitelist_state: ensure the scope has a `ScopeState`
///   entry with `encrypt_toggle = true; auto_enabled = true`
///   (§2.3 auto-enable). For Dm, no member list needed; for
///   GC/server-channel/server-full, add the peer to
///   `whitelisted_users` and set `full_whitelist = false` (the
///   default for newly-created per-user whitelists; the UI can
///   promote later).
/// - Call `cmd_osl_send_whitelist_invitation` to build the wire.
pub fn cmd_osl_set_whitelist(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    broadened: bool,
    from_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .clone()
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let enabled_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());

    // 1. peer_map.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(peer_discord_id.clone()).or_default();
        // De-dupe: remove any prior entry for the same scope
        // shape before appending the new one.
        pe.outgoing_whitelists
            .retain(|w| !whitelist_entry_matches(w, &scope));
        let new_entry = match scope.kind {
            crate::scope::ScopeKind::Dm => crate::peer_map::WhitelistEntry::Dm {
                broadened,
                enabled_at: Some(enabled_at_iso),
            },
            crate::scope::ScopeKind::Gc => crate::peer_map::WhitelistEntry::Gc {
                id: scope.id.clone(),
                user_specific: true,
            },
            crate::scope::ScopeKind::ServerChannel => {
                crate::peer_map::WhitelistEntry::ServerChannel {
                    server_id: scope.server_id.clone().unwrap_or_default(),
                    channel_id: scope.channel_id.clone().unwrap_or_default(),
                    user_specific: true,
                }
            }
            crate::scope::ScopeKind::ServerFull => crate::peer_map::WhitelistEntry::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                user_specific: true,
            },
        };
        pe.outgoing_whitelists.push(new_entry);
        // Also evict any prior burned-scope entry for the same
        // scope shape — re-whitelisting after a burn is allowed
        // and the §3.5 semantics say "fresh keys → new messages
        // encrypt and decrypt normally."
        pe.burned_scopes.retain(|b| !burn_matches_scope(b, &scope));
    }

    // 2. whitelist_state.
    {
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let ws = ws_guard
            .entry(scope.storage_key())
            .or_insert_with(crate::whitelist_state::ScopeState::default);
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
        if matches!(scope.kind, crate::scope::ScopeKind::Dm) {
            // No list semantics for DM (the scope id IS the peer).
        } else if !ws.whitelisted_users.iter().any(|u| u == &peer_discord_id) {
            ws.whitelisted_users.push(peer_discord_id.clone());
        }
    }

    // 7d-FIX1: persist BOTH files. Encryption-at-rest is applied
    // transparently by write_peer_map / write_whitelist_state via
    // `maybe_encrypt` when a main password is set.
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    // 7d-FIX1 decision-B: re-whitelisting a scope removes it from
    // the global burned-scopes ledger so the receive observer
    // stops skipping its messages. Old burned ciphertext stays
    // unreadable (wrapped_keys gone), but NEW messages decrypt
    // normally.
    let scope_kind_str = match scope.kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc_full",
        crate::scope::ScopeKind::ServerChannel => "server_channel_full",
        crate::scope::ScopeKind::ServerFull => "server_full",
    };
    let _ = cmd_osl_unburn_scope(state, scope_kind_str.to_string(), scope.id.clone());

    // 3. Build the wire invitation.
    cmd_osl_send_whitelist_invitation(state, peer_discord_id, scope_input, from_discord_id)
}

fn burn_matches_scope(b: &crate::peer_map::BurnedScope, s: &crate::scope::Scope) -> bool {
    use crate::peer_map::BurnedScope as B;
    use crate::scope::ScopeKind as K;
    match (b, &s.kind) {
        (B::Dm { .. }, K::Dm) => true,
        (B::Gc { id, .. }, K::Gc) => id == &s.id,
        (
            B::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            K::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (B::ServerFull { server_id, .. }, K::ServerFull) => Some(server_id) == s.server_id.as_ref(),
        _ => false,
    }
}

// ---- Phase 7c: UI-supporting read/write commands ----
//
// These thin wrappers expose pieces of whitelist_state +
// pending_invitations to boot.js so the channel-header encrypt
// toggle, burn button, and invitation banner can render their
// initial state without each having to walk the full schema.

/// Per-scope encryption posture for the channel-header lock icon.
///
/// - `encrypt_toggle`: the user's current ON/OFF state for
///   encryption in this scope. Drives the icon's "open lock vs
///   closed lock" visual.
/// - `has_whitelist`: whether **any** recipient is whitelisted in
///   this scope. Drives the icon's grayed-out state — without a
///   whitelist there's no one to encrypt to, so the toggle is
///   non-interactive.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ScopeEncryptionState {
    pub encrypt_toggle: bool,
    pub has_whitelist: bool,
}

/// Layer 10 / Phase 7c: read the encryption posture for a scope.
/// Boot.js calls this every channel-switch + after every
/// whitelist mutation so the header icon updates promptly.
pub fn cmd_osl_get_scope_encryption_state(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<ScopeEncryptionState, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let entry = ws_guard.get(&key);
    let encrypt_toggle = entry.map(|s| s.encrypt_toggle).unwrap_or(false);
    let has_whitelist = match entry {
        None => false,
        Some(s) => match scope.kind {
            // DM scope: whitelist existence == ScopeState entry
            // present (the scope id IS the peer; no member list
            // is tracked).
            crate::scope::ScopeKind::Dm => true,
            // Other scopes: either a full whitelist with a
            // member list, or a per-user whitelist with
            // whitelisted_users.
            _ => (s.full_whitelist && !s.members.is_empty()) || !s.whitelisted_users.is_empty(),
        },
    };
    Ok(ScopeEncryptionState {
        encrypt_toggle,
        has_whitelist,
    })
}

/// Layer 10 / Phase 7c: flip `encrypt_toggle` for a scope.
/// Returns the new value (post-flip) so boot.js doesn't need to
/// follow up with a read.
///
/// Refuses to enable the toggle when `has_whitelist == false` —
/// per design doc §2.4, the toggle is grayed-out / unavailable
/// in that state. boot.js gates the click handler on
/// `has_whitelist`, but we double-check here so a buggy caller
/// can't end up with encrypt-to-nobody enabled.
pub fn cmd_osl_toggle_scope_encryption(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<bool, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let mut ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let entry = ws_guard
        .entry(key)
        .or_insert_with(crate::whitelist_state::ScopeState::default);
    // 7d-PIVOT: encrypt_toggle is now independent of whitelist
    // existence. Toggling ON with no whitelist is the
    // "encrypt-to-self-only" mode — your messages encrypt and
    // only you can decrypt them. The previous
    // `encrypt_toggle_refused_no_whitelist` early-error has been
    // removed.
    entry.encrypt_toggle = !entry.encrypt_toggle;
    // Mark `auto_enabled = false` since this is a manual
    // user action — distinguishes the §2.3 auto-enable from a
    // later user toggle in the UI's tooltip.
    entry.auto_enabled = false;
    let new_toggle = entry.encrypt_toggle;
    drop(ws_guard);
    // 7d-FIX1: persist the new toggle state.
    persist_whitelist_state_now(state);
    Ok(new_toggle)
}

/// 7d-PIVOT: explicit set (not toggle) of a scope's encrypt state.
/// Used by the composer toggle UI which knows the desired end state
/// rather than just "flip whatever it was." Idempotent — no-op when
/// the requested state already matches. Persists on change.
pub fn cmd_osl_set_scope_encrypt(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    enabled: bool,
) -> Result<bool, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let mut ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let entry = ws_guard
        .entry(key)
        .or_insert_with(crate::whitelist_state::ScopeState::default);
    if entry.encrypt_toggle == enabled {
        return Ok(enabled);
    }
    entry.encrypt_toggle = enabled;
    entry.auto_enabled = false;
    drop(ws_guard);
    persist_whitelist_state_now(state);
    Ok(enabled)
}

/// JS-facing DTO for one pending invitation. Mirrors
/// [`crate::pending_invitations::PendingInvitation`] plus the
/// invitation id (the map key) so boot.js can pass it back to
/// accept/decline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingInvitationDto {
    pub id: String,
    pub from: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub received_at: String,
    pub status: String,
}

/// Layer 10 / Phase 7c: list every entry in
/// `pending_invitations` for the banner system. Returns an
/// empty vec when none — boot.js renders zero banners.
pub fn cmd_osl_list_pending_invitations(
    state: &AppState,
) -> Result<Vec<PendingInvitationDto>, String> {
    let guard = state
        .pending_invitations
        .lock()
        .expect("pending_invitations mutex poisoned");
    let mut out: Vec<PendingInvitationDto> = guard
        .iter()
        .map(|(id, inv)| PendingInvitationDto {
            id: id.clone(),
            from: inv.from.clone(),
            scope: inv.scope.clone(),
            scope_id: inv.scope_id.clone(),
            received_at: inv.received_at.clone(),
            status: match inv.status {
                crate::pending_invitations::InvitationStatus::Pending => "pending",
                crate::pending_invitations::InvitationStatus::Accepted => "accepted",
                crate::pending_invitations::InvitationStatus::Declined => "declined",
            }
            .to_string(),
        })
        .collect();
    // Stable order — oldest first, by received_at. Falls back to
    // id for ties (received_at strings are unix-second strings;
    // ties are unlikely but possible).
    out.sort_by(|a, b| {
        a.received_at
            .cmp(&b.received_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

/// Phase 7c bug-fix #1 (round 3): return the local user's
/// **Discord snowflake** by reverse-lookup against `peer_map`.
///
/// Naming hazard: `Identity::user_id` is the **OSL** user_id
/// (a logical username like "liam"), NOT a Discord snowflake.
/// The injection layer needs the Discord snowflake for send
/// pipelines (`self_discord_id` excludes self from channel-
/// member walks). The snowflake is configured in
/// `peer_map.json` keyed by snowflake with
/// `PeerEntry::osl_user_id` as the value — so we walk the map
/// and return the key whose entry matches our `Identity`.
///
/// Failure modes (all flat-string `Err`):
///   - `"OSL: identity not loaded"` — bootstrap hasn't run
///     or identity.json is missing.
///   - `"OSL: self not registered in peer_map.json (osl_user_id=<name>);
///     add an entry mapping your Discord snowflake to
///     {"osl_user_id":"<name>"}"` — identity loaded but no
///     peer_map row has a matching `osl_user_id`. JS toasts
///     this so the user can fix peer_map.json without grepping
///     logs.
pub fn cmd_osl_get_self_user_id(state: &AppState) -> Result<String, String> {
    let osl_user_id = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        identity.user_id.clone()
    };
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    for (discord_id, entry) in pm_guard.iter() {
        if entry.osl_user_id.as_deref() == Some(osl_user_id.as_str()) {
            return Ok(discord_id.clone());
        }
    }
    Err(format!(
        "OSL: self not registered in peer_map.json \
         (osl_user_id={osl_user_id}); add an entry mapping your Discord \
         snowflake to {{\"osl_user_id\":\"{osl_user_id}\"}}"
    ))
}

// =====================================================================
// Phase 7d-A: settings-menu data sources
// =====================================================================

/// 7d-A: payload backing the Identity page of the settings modal.
/// All fields are display-only — JS renders them in a read-only
/// monospace block. Missing data points (e.g. snowflake not yet
/// in peer_map, keyserver.json absent) come back as the literal
/// string `"Unknown"` rather than an `Err`, so the page always
/// renders even when bootstrap was partially successful.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdentityInfoDto {
    pub osl_user_id: String,
    pub discord_snowflake: String,
    pub pubkey: String,
    pub keyserver_url: String,
}

/// 7d-A: assemble the Identity page payload from `AppState` plus a
/// best-effort read of `keyserver.json` for the configured base
/// URL. The Tauri shell exposes this via `osl_get_identity_info`.
pub fn cmd_osl_get_identity_info(state: &AppState) -> Result<IdentityInfoDto, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let (osl_user_id, pubkey_b64) = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        (
            identity.user_id.clone(),
            STANDARD.encode(identity.x25519_public.as_bytes()),
        )
    };
    // Snowflake: reverse-lookup peer_map (same shape as
    // cmd_osl_get_self_user_id). Display-only — "Unknown" on miss.
    let snowflake = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let mut found = None;
        for (discord_id, entry) in pm_guard.iter() {
            if entry.osl_user_id.as_deref() == Some(osl_user_id.as_str()) {
                found = Some(discord_id.clone());
                break;
            }
        }
        found.unwrap_or_else(|| "Unknown".to_string())
    };
    // Keyserver URL: best-effort read of <config_dir>/keyserver.json.
    // Mirrors the bootstrap loader shape but tolerant of any error.
    let keyserver_url = match keystore::osl_config_dir() {
        Ok(dir) => {
            let path = dir.join("keyserver.json");
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                .and_then(|v| {
                    v.get("base_url")
                        .and_then(|b| b.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "Unknown".to_string())
        }
        Err(_) => "Unknown".to_string(),
    };
    Ok(IdentityInfoDto {
        osl_user_id,
        discord_snowflake: snowflake,
        pubkey: pubkey_b64,
        keyserver_url,
    })
}

/// 7d-A: one row in the Whitelist Manager's flat table. The
/// Tauri shell exposes a `Vec<WhitelistRowDto>` via
/// `osl_list_all_whitelists`.
///
/// Field shapes:
///   - `scope_kind`: one of "dm", "gc_full", "gc_per_user",
///     "server_channel_full", "server_channel_per_user",
///     "server_full". JS uses this to render the human-readable
///     scope label and to build the right `ScopeInput` when the
///     user clicks Remove / Burn.
///   - `scope_id`: the raw scope id (peer snowflake for DM,
///     channel id for GC, channel id for server_channel,
///     server id for server_full). NOT the storage key.
///   - `server_id` / `channel_id`: populated when the kind
///     carries them; null otherwise.
///   - `encrypt_toggle`: pulled from `whitelist_state` by
///     storage_key; false when the scope has no state entry.
///   - `broadened`: only meaningful for DM scope; always false
///     for other kinds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WhitelistRowDto {
    pub peer_discord_id: String,
    pub peer_username: String,
    pub scope_kind: String,
    pub scope_id: String,
    pub server_id: Option<String>,
    pub channel_id: Option<String>,
    pub encrypt_toggle: bool,
    pub broadened: bool,
}

/// 7d-A: flatten every peer's outgoing_whitelists into a single
/// list of DTOs for the settings-menu Whitelist Manager. Order
/// is stable: peers sorted by Discord snowflake (string), then
/// scopes in the order they were added (`Vec` preserves insert
/// order).
pub fn cmd_osl_list_all_whitelists(state: &AppState) -> Result<Vec<WhitelistRowDto>, String> {
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    let ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let mut peers: Vec<(&String, &crate::peer_map::PeerEntry)> = pm_guard.iter().collect();
    peers.sort_by(|a, b| a.0.cmp(b.0));
    let mut out: Vec<WhitelistRowDto> = Vec::new();
    for (discord_id, entry) in peers {
        let username = entry
            .osl_user_id
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        for w in &entry.outgoing_whitelists {
            let (scope_kind, scope_id, server_id, channel_id, broadened, storage_key) = match w {
                crate::peer_map::WhitelistEntry::Dm { broadened, .. } => (
                    "dm".to_string(),
                    discord_id.clone(),
                    None,
                    Some(discord_id.clone()),
                    *broadened,
                    format!("dm:{discord_id}"),
                ),
                crate::peer_map::WhitelistEntry::Gc { id, user_specific } => {
                    let kind = if *user_specific {
                        "gc_per_user".to_string()
                    } else {
                        "gc_full".to_string()
                    };
                    (
                        kind,
                        id.clone(),
                        None,
                        Some(id.clone()),
                        false,
                        format!("gc:{id}"),
                    )
                }
                crate::peer_map::WhitelistEntry::ServerChannel {
                    server_id,
                    channel_id,
                    user_specific,
                } => {
                    let kind = if *user_specific {
                        "server_channel_per_user".to_string()
                    } else {
                        "server_channel_full".to_string()
                    };
                    let combined = format!("{server_id}:{channel_id}");
                    (
                        kind,
                        combined.clone(),
                        Some(server_id.clone()),
                        Some(channel_id.clone()),
                        false,
                        format!("server_channel:{combined}"),
                    )
                }
                crate::peer_map::WhitelistEntry::ServerFull {
                    server_id,
                    user_specific,
                } => {
                    let kind = if *user_specific {
                        "server_full_per_user".to_string()
                    } else {
                        "server_full".to_string()
                    };
                    (
                        kind,
                        server_id.clone(),
                        Some(server_id.clone()),
                        None,
                        false,
                        format!("server_full:{server_id}"),
                    )
                }
            };
            let encrypt_toggle = ws_guard
                .get(&storage_key)
                .map(|s| s.encrypt_toggle)
                .unwrap_or(false);
            out.push(WhitelistRowDto {
                peer_discord_id: discord_id.clone(),
                peer_username: username.clone(),
                scope_kind,
                scope_id,
                server_id,
                channel_id,
                encrypt_toggle,
                broadened,
            });
        }
    }
    Ok(out)
}

// =====================================================================
// Phase 7d-B1: main-password gate commands. Each delegates to the
// `crate::main_password` module which holds the marker/lockout file
// layout, argon2id derivation, AES-GCM phrase blob, and BIP39
// phrase generation. Tauri wrappers in `src-tauri/src/main.rs`.
// =====================================================================

pub use crate::main_password::{LockoutStatusDto, PasswordStatusDto};

pub fn cmd_osl_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(crate::main_password::password_status(&dir))
}

pub fn cmd_osl_set_main_password(password: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_main_password(&dir, &password)
}

pub fn cmd_osl_change_main_password(current: String, new: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::change_main_password(&dir, &current, &new)
}

pub fn cmd_osl_remove_main_password(current: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_main_password(&dir, &current)
}

pub fn cmd_osl_view_recovery_phrase(current: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::view_recovery_phrase(&dir, &current)
}

pub fn cmd_osl_verify_main_password(password: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::verify_main_password(&dir, &password)
}

pub fn cmd_osl_verify_recovery_phrase(state: &AppState, phrase: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::verify_recovery_phrase(state, &dir, &phrase)
}

pub fn cmd_osl_set_main_password_after_recovery(
    state: &AppState,
    new_password: String,
    token: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_main_password_after_recovery(state, &dir, &new_password, &token)
}

pub fn cmd_osl_lockout_status() -> Result<LockoutStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(crate::main_password::lockout_status(&dir))
}

// =====================================================================
// Phase 7d-B2: stealth password operations.
// =====================================================================

pub fn cmd_osl_set_stealth_password(
    current_main: String,
    new_stealth: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_stealth_password(&dir, &current_main, &new_stealth)
}

pub fn cmd_osl_remove_stealth_password(current_main: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_stealth_password(&dir, &current_main)
}

pub fn cmd_osl_stealth_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(PasswordStatusDto {
        is_set: crate::main_password::stealth_password_status(&dir),
    })
}

// =====================================================================
// Phase 7d-B3: burn password operations.
// =====================================================================

pub fn cmd_osl_set_burn_password(current_main: String, new_burn: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_burn_password(&dir, &current_main, &new_burn)
}

pub fn cmd_osl_remove_burn_password(current_main: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_burn_password(&dir, &current_main)
}

pub fn cmd_osl_burn_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(PasswordStatusDto {
        is_set: crate::main_password::burn_password_status(&dir),
    })
}

// =====================================================================
// Phase 7d-B2/B3: gate-side single-call password verify across the
// three roles. Returns one of "main" | "stealth" | "burn" | "wrong"
// + the same lockout fields as `verify_main_password`. All three
// successful entries reset the shared counter (so an attacker
// observing repeated entries can't distinguish "main" from
// "stealth"/"burn" via counter dynamics).
// =====================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct GateVerifyDto {
    pub result: String,
    pub lockout_seconds_remaining: i64,
    pub attempts_used: u32,
}

pub fn cmd_osl_verify_gate_password(
    _state: &AppState,
    password: String,
) -> Result<GateVerifyDto, String> {
    use crate::main_password::GateMatch;
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    // Lockout-window check first (same as verify_main_password).
    let mut lock = crate::main_password::read_lockout_pub(&dir);
    let now = crate::main_password::now_unix_secs_pub();
    if let Some(until) = lock.password_locked_until {
        if now < until {
            return Ok(GateVerifyDto {
                result: "wrong".to_string(),
                lockout_seconds_remaining: until - now,
                attempts_used: lock.password_failed_attempts,
            });
        }
    }
    let marker = crate::main_password::read_marker_pub(&dir)?;
    let outcome = crate::main_password::verify_gate_password_with_marker(&marker, &password)?;
    match outcome {
        GateMatch::Main(file_key) => {
            crate::main_password::set_file_storage_key(Some(file_key));
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "main".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Stealth => {
            // Shared counter reset on any successful entry — see
            // security rationale in the spec (prevents attacker
            // distinguishing main from stealth via counter dynamics).
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "stealth".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Burn => {
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "burn".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Wrong => {
            lock.password_failed_attempts = lock.password_failed_attempts.saturating_add(1);
            let secs =
                crate::main_password::password_lockout_secs_pub(lock.password_failed_attempts);
            lock.password_locked_until = if secs > 0 { Some(now + secs) } else { None };
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "wrong".to_string(),
                lockout_seconds_remaining: secs,
                attempts_used: lock.password_failed_attempts,
            })
        }
    }
}

/// 7d-B2: hide the OSL config dir + record stealth-active for the
/// session so initialization_script can suppress boot.js injection.
pub fn cmd_osl_stealth_mode_engage(state: &AppState) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    let _ = crate::main_password::stealth_hide_dir(&dir);
    *state
        .stealth_active
        .lock()
        .expect("stealth_active mutex poisoned") = true;
    Ok(())
}

// =====================================================================
// Phase 7d-FIX1: scope burn data destruction + burned-scope ledger.
// =====================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct BurnScopeDataDto {
    pub rows_destroyed: usize,
    pub channel_id: String,
}

/// Destroy local message rows for the channel(s) covered by
/// `scope`. Per spec 7d-FIX1 Task 3a + 7d-D Task 2:
///   - DM and server_channel_full scopes resolve to a single
///     channel_id and `DELETE FROM messages WHERE channel_id = ?`.
///   - gc_full (7d-D): scope_id IS the GC channel_id — same
///     single-channel DELETE as DM.
///   - gc_per_user and server_full / server_full_per_user remain
///     NOT implemented in this phase (they'd require either
///     per-sender row filtering or enumerating multiple
///     channel_ids); we return a not-implemented error string
///     so the JS caller can surface it but the rest of the burn
///     flow keeps going.
pub fn cmd_osl_burn_scope_data(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
) -> Result<BurnScopeDataDto, String> {
    let channel_id = match scope_kind.as_str() {
        "dm" => scope_id.clone(),
        "server_channel_full" | "server_channel_per_user" | "server_channel" => {
            if let Some((_, ch)) = scope_id.split_once(':') {
                ch.to_string()
            } else {
                scope_id.clone()
            }
        }
        // 7d-D Task 2: gc_full's scope_id is the GC channel_id.
        // Same single-channel destroy path as DM.
        "gc_full" | "gc" => scope_id.clone(),
        "gc_per_user" => {
            return Err(format!(
                "OSL: burn_scope_data: gc_per_user burn not yet implemented (scope_id={scope_id}) — \
                 deferred to a later cleanup pass, see 7d-D spec"
            ));
        }
        "server_full" | "server_full_per_user" => {
            return Err(format!(
                "OSL: burn_scope_data: server_full burn not yet implemented (scope_id={scope_id}) — \
                 deferred, see 7d-D spec"
            ));
        }
        other => {
            return Err(format!("OSL: burn_scope_data: unknown scope_kind={other}"));
        }
    };
    let rows = if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        store
            .delete_messages_in_channel(&channel_id)
            .map_err(|e| format!("OSL: delete_messages_in_channel: {e}"))?
    } else {
        0
    };
    eprintln!("[OSL][burn] destroyed {rows} rows for channel {channel_id}");
    let _ = server_id;
    Ok(BurnScopeDataDto {
        rows_destroyed: rows,
        channel_id,
    })
}

pub fn cmd_osl_mark_scope_burned(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
    channel_id: Option<String>,
) -> Result<(), String> {
    use crate::burned_scopes_file::BurnedScopeEntry;
    let now = now_unix_secs();
    let entry = BurnedScopeEntry {
        scope_kind: scope_kind.clone(),
        scope_id: scope_id.clone(),
        server_id,
        channel_id,
        burned_at: now as i64,
    };
    {
        let mut g = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        if !g
            .scopes
            .iter()
            .any(|e| e.scope_kind == scope_kind && e.scope_id == scope_id)
        {
            g.scopes.push(entry);
        }
        g.version = 1;
    }
    persist_burned_scopes_now(state);
    Ok(())
}

/// Returns `Ok(true)` if a burned-scopes entry was removed,
/// `Ok(false)` if there was nothing to remove (idempotent no-op).
/// 7d-PIVOT-FIX2: callers use the boolean to emit the
/// `osl:scope_unburned` cross-window event so the JS-side
/// `__oslBurnedScopes` cache stays in sync.
pub fn cmd_osl_unburn_scope(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
) -> Result<bool, String> {
    let removed = {
        let mut g = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        let before = g.scopes.len();
        g.scopes
            .retain(|e| !(e.scope_kind == scope_kind && e.scope_id == scope_id));
        let after = g.scopes.len();
        if after < before {
            g.version = 1;
            true
        } else {
            false
        }
    };
    if removed {
        persist_burned_scopes_now(state);
    }
    Ok(removed)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BurnedScopeDto {
    pub scope_kind: String,
    pub scope_id: String,
    pub server_id: Option<String>,
    pub channel_id: Option<String>,
    pub burned_at: i64,
}

pub fn cmd_osl_list_burned_scopes(state: &AppState) -> Result<Vec<BurnedScopeDto>, String> {
    let g = state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned");
    Ok(g.scopes
        .iter()
        .map(|e| BurnedScopeDto {
            scope_kind: e.scope_kind.clone(),
            scope_id: e.scope_id.clone(),
            server_id: e.server_id.clone(),
            channel_id: e.channel_id.clone(),
            burned_at: e.burned_at,
        })
        .collect())
}

fn persist_burned_scopes_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(?e, "OSL: persist burned_scopes: cannot resolve config dir");
            return;
        }
    };
    let path = dir.join("burned_scopes.json");
    let g = state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned");
    if let Err(e) = crate::burned_scopes_file::write_burned_scopes(&path, &g) {
        tracing::warn!(?e, "OSL: persist burned_scopes.json failed");
    }
}

/// 7d-B3: wipe every OSL file. Also clears in-memory AppState so the
/// current session doesn't surface previously-decrypted state.
pub fn cmd_osl_burn_engage(state: &AppState) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::burn_wipe_all(&dir)?;
    crate::main_password::set_file_storage_key(None);
    // Drop in-memory state. Identity goes to None, every state
    // mutex is cleared. The webview will navigate away after this
    // returns, but if anything still queries state in the
    // intervening millisecond we want it empty.
    *state.identity.lock().expect("identity mutex poisoned") = None;
    state
        .peer_map
        .lock()
        .expect("peer_map mutex poisoned")
        .clear();
    state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned")
        .clear();
    state
        .pending_invitations
        .lock()
        .expect("pending_invitations mutex poisoned")
        .clear();
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") = None;
    // 7d-FIX1: also clear burned-scopes ledger.
    *state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned") =
        crate::burned_scopes_file::BurnedScopesFile::default();
    Ok(())
}
