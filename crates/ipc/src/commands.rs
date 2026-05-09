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
use crypto::{aead, x25519};
use keystore::{generate_identity, KeyServerClient};
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
    let id = keystore::load_identity(&PathBuf::from(path))?;
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
    keystore::save_identity(&PathBuf::from(path), id)?;
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
