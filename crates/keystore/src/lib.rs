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

pub mod burn;
pub mod burn_alert;
pub mod client;
pub mod duress;
pub mod identity;
pub mod license_cache;
pub mod password;
pub mod pending_rotation;
pub mod prekeys;
pub mod recipients;
pub mod sealer;
pub mod storage;
pub mod unregister;

pub use burn::{canonical_burn_bytes, sign_burn, BurnScope, BURN_DOMAIN};
pub use unregister::{canonical_unregister_bytes, sign_unregister, UNREGISTER_DOMAIN};
pub use burn_alert::{sign_burn_alert, verify_burn_alert, BurnAlertPayload, BURN_ALERT_DOMAIN};
pub use client::{
    BurnResponse, KeyServerClient, LicenseValidateResponse, PrekeyBundleOpk, PrekeyBundleResponse,
    PubkeysResponse, RegisterResponse, ReplenishResponse,
};
pub use duress::{
    DuressEngine, DuressError, DuressHandlers, DuressJournal, DuressPaths, DuressReport,
    StepOutcome, WipeFn, WipeStep,
};
pub use identity::{generate_identity, Identity, IDENTITY_BLOB_VERSION};
pub use license_cache::{
    classify_state, load_license_cache, save_license_cache, LicenseCacheInner, LicenseCacheOnDisk,
    LicenseState, LicenseStateDto,
};
pub use password::{
    load_password_record, save_password_record, validate_password, validate_setup_pair,
    verify_against_record, Argon2Params, InactivityTimer, PasswordError, PasswordHash,
    PasswordRecord, VerifyOutcome, DEFAULT_FAILED_ATTEMPT_THRESHOLD, DEFAULT_INACTIVITY_SECONDS,
    MIN_PASSWORD_LENGTH,
};
pub use pending_rotation::{
    delete_pending_rotation, load_pending_rotation, pending_rotation_from, save_pending_rotation,
    PendingRotation,
};
pub use prekeys::{
    canonical_replenish_bytes, iso_8601_from_unix_seconds, load_prekey_state, save_prekey_state,
    sign_replenish_batch, OpkEntry, PrekeyConfig, PrekeyState, ReplenishOpk, ReplenishSpk,
    SpkEntry, REPLENISH_DOMAIN, SPK_ROTATION_INTERVAL_SECONDS,
};
pub use recipients::{get_recipients, get_recipients_from_path, osl_config_dir, RecipientError};
pub use sealer::{
    evict_tpm_key, select_best_sealer, KeyringSealer, MemorySealer, NoOpSealer, Sealer,
    SealerError, TpmSealer, METHOD_KEYRING, METHOD_MEMORY, METHOD_NOOP, METHOD_TPM,
};
pub use storage::{load_identity, save_identity, IdentityOnDisk};

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

    #[error("sealing error: {0}")]
    Sealer(#[from] sealer::SealerError),

    #[error("on-disk identity blob version mismatch: got {got}, expected {expected}")]
    BlobVersionMismatch { got: u32, expected: u32 },

    #[error(
        "on-disk identity blob field {field} has wrong length: got {got}, expected {expected}"
    )]
    BlobFieldLength {
        field: &'static str,
        got: usize,
        expected: usize,
    },

    #[error("on-disk identity blob method tag {got:?} disagrees with active sealer {expected:?}")]
    BlobMethodMismatch { got: String, expected: String },

    #[error("HTTP transport error: {0}")]
    Transport(String),

    #[error("HTTP server returned status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

pub type Result<T> = core::result::Result<T, Error>;
