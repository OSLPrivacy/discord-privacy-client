//! Typed command surface bridging the webview to the Rust crates.
//!
//! Current production messaging posture:
//! - Identity lifecycle and key-server identity-bundle registration/fetch.
//! - Stateless wire-v3 recipient wrapping: X25519 + ML-KEM-768 feed a
//!   per-message hybrid key derivation, with the recipient identity key also
//!   occupying the signed-prekey role and no one-time prekey.
//! - The wire-v4 Double Ratchet and wire-v5 sender-key implementations remain
//!   in this crate, but production disables the v4 DM branch and defaults the
//!   v5 group branch off. They are implementation inventory, not current
//!   forward-secrecy or group sender-key product guarantees.
//! - The keystore prekey client is likewise not called by this production
//!   command path.
//!
//! ## Design
//!
//! - [`commands`] module exposes pure functions that take an
//!   explicit [`AppState`] and primitive types. Unit tests exercise
//!   these directly with no Tauri runtime.
//! - [`state::AppState`] holds the shared mutable state behind
//!   mutexes (loaded identity + key-server client).
//! - The actual `#[tauri::command]` wrappers live in
//!   `src-tauri/src/main.rs`. Keeping Tauri out of this crate avoids
//!   pulling Wry's gtk/webkit2gtk system-deps tree on Linux, so the
//!   crate's tests stay portable across dev environments.
//!
//! ## Errors
//!
//! [`IpcError`] is `Serialize` so Tauri can ship it back across the
//! bridge. Each variant carries an opaque human-readable message —
//! we deliberately do **not** expose typed crypto / keystore error
//! variants to JS, per the design's "no error oracle" stance. Future
//! work: collapse all rejection paths into a single
//! `IpcError::Rejected` once the protocol is stable.

pub mod app_preferences;
pub mod at_rest_boundary;
pub mod attachment_wire;
pub mod burned_scopes_file;
pub mod cipher_store_client;
pub mod commands;
pub mod control_inbox_dead_letter;
pub mod control_messages;
pub mod decoy_mp4;
pub mod fresh_start;
pub mod license_lifecycle;
pub mod log_id;
pub mod main_password;
pub mod mandatory_storage_key_policy;
pub mod membership;
pub mod migration;
pub mod peer_map;
pub mod prose_token;
pub mod recovery;
// Bilateral burn (wire 0x0A / 0x0B): sender sequencing, opaque commitments,
// the receiver replay ledger and the durable revocation outbox. Strictly
// additive; the legacy `MSG_TYPE_BURN` (0x01) path above is untouched except
// that an inbound legacy marker is now converted to a *bounded* revocation
// instead of a permanent scope flag.
pub mod revocation;
// 9-C1: `pending_invitations` module removed alongside the
// invitation handshake. Pre-C1 `pending_invitations.json` files are
// unconditionally deleted at bootstrap.
pub mod scope;
pub mod scope_blobs_file;
pub mod scope_ttl_file;
// Unit a45: encrypted UI-side storage contract (checklist A6). Defines the
// `SecureLocalStore` trait + `SealedStore` reference impl; does not migrate
// any caller yet (`apps/osl-hub-ui/src/main.ts` localStorage call sites and
// `main_password::maybe_encrypt` are separate, later units).
pub mod secure_local_store;
pub mod sender_key_state;
pub mod state;
pub mod state_reload;
pub mod tier_gate;
pub mod tofu;
pub mod whitelist;
pub mod whitelist_state;
pub mod wire_v2;
// OSL-RN (wire 0x10) integration: version selection with downgrade
// protection plus sealed per-peer ratchet session state. Strictly
// additive — the v=2/v=3/v=4/v=5 paths above are untouched.
pub mod wire_rn;

pub use at_rest_boundary::AtRestBoundary;
pub use commands::{
    AeadOpenRequest, AeadSealRequest, AeadSealResponse, FetchPubkeysResponse,
    GenerateIdentityResponse, RegisterResponse, StatusResponse, StegoDecodeResponse,
    StegoEncodeRequest, StegoEncodeResponse,
};
pub use state::AppState;

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum IpcError {
    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("stego error: {0}")]
    Stego(String),

    #[error("keystore error: {0}")]
    Keystore(String),

    #[error("base64 decode error: {0}")]
    Base64(String),

    #[error("identity not loaded — call generate_identity or load_identity first")]
    IdentityMissing,

    #[error("key-server client not initialised — call init_keyserver first")]
    KeyserverMissing,

    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

pub type IpcResult<T> = core::result::Result<T, IpcError>;

impl From<crypto::error::Error> for IpcError {
    fn from(e: crypto::error::Error) -> Self {
        IpcError::Crypto(e.to_string())
    }
}

impl From<stego::Error> for IpcError {
    fn from(e: stego::Error) -> Self {
        IpcError::Stego(e.to_string())
    }
}

impl From<keystore::Error> for IpcError {
    fn from(e: keystore::Error) -> Self {
        IpcError::Keystore(e.to_string())
    }
}

impl From<base64::DecodeError> for IpcError {
    fn from(e: base64::DecodeError) -> Self {
        IpcError::Base64(e.to_string())
    }
}
