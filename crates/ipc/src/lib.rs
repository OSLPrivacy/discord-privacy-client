//! Typed command surface bridging the webview to the Rust crates.
//!
//! v1 alpha prototype scope:
//! - Identity lifecycle: generate, load, save (plain-file via
//!   [`keystore`]).
//! - Key-server interactions: init, register, fetch_pubkeys.
//! - AEAD primitive operations: seal / open (direct
//!   [`crypto::aead`] wrapper for end-to-end smoke testing).
//! - Stego Mode 0 encode / decode.
//!
//! Out of scope this layer (per `docs/design/build-order.md`):
//! - Full PQXDH handshake + Double Ratchet session start.
//! - Group sender-keys session distribution.
//! - Wrapped-key burn / re-validation flow.
//! - View-once UI plumbing (short-lived blob URLs).
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

pub mod commands;
pub mod fresh_start;
pub mod peer_map;
pub mod pending_invitations;
pub mod state;
pub mod whitelist_state;
pub mod wire_v2;

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
