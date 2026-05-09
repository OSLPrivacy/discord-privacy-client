//! Identity key generation, plain-file storage, and HTTP client glue
//! against the prototype key server.
//!
//! # ⚠️  INSECURE BY DESIGN — PROTOTYPE ONLY ⚠️
//!
//! v1 alpha prototype implementation — used for dev-to-dev testing
//! between two of the developer's own devices. **Not safe for any
//! production-like usage.** Specifically:
//!
//! - Identity keys are stored as **plain JSON on disk** with no
//!   passphrase wrapping, no Argon2id KDF, and no TPM sealing.
//! - HTTP traffic is **plain HTTP**, no TLS.
//! - The key-server registration request carries no Discord OAuth
//!   proof; the server trusts whatever `user_id` is sent.
//! - Re-registration rate limiting and signature verification on the
//!   identity-key bundle are deferred.
//!
//! v1 stable replaces every item above (TPM-sealed identity blobs via
//! Windows TBS, Argon2id passphrase wrapping, Discord OAuth gate, TLS,
//! signed re-registration) — see `docs/design/key-server-api.md`,
//! `docs/design/auth-flow.md`, and `docs/design/unlock-and-duress.md`.
//!
//! ## Modules
//!
//! - [`identity`] — generate and reconstruct ML-KEM-768 + X25519
//!   identity keypairs.
//! - [`storage`] — load and save the identity JSON blob.
//! - [`client`] — sync HTTP client (`ureq` 2) for the prototype key
//!   server endpoints used in this layer (`/v1/register`,
//!   `/v1/pubkeys/:user_id`).

pub mod client;
pub mod identity;
pub mod storage;

pub use client::{KeyServerClient, PubkeysResponse, RegisterResponse};
pub use identity::{generate_identity, Identity, IDENTITY_BLOB_VERSION};
pub use storage::{load_identity, save_identity};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("crypto error: {0}")]
    Crypto(#[from] crypto::error::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("on-disk identity blob version mismatch: got {got}, expected {expected}")]
    BlobVersionMismatch { got: u32, expected: u32 },

    #[error("on-disk identity blob field {field} has wrong length: got {got}, expected {expected}")]
    BlobFieldLength {
        field: &'static str,
        got: usize,
        expected: usize,
    },

    #[error("HTTP transport error: {0}")]
    Transport(String),

    #[error("HTTP server returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

pub type Result<T> = core::result::Result<T, Error>;
