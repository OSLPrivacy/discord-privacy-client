//! Identity key generation, plain-file storage, and HTTP client glue
//! against the prototype key server.
//!
//! Identity registration is deliberately service-neutral: a Discord
//! snowflake is local carrier metadata, never a keyserver lookup name.
//! Every public bundle returned by `/v1/pubkeys` must carry the
//! identity's Ed25519 registration signature, which this crate verifies
//! before returning it to callers. Secret-key at-rest protection is
//! supplied by the selected [`sealer`] implementation.
//!
//! ## Modules
//!
//! - [`identity`] — generate and reconstruct ML-KEM-768 + X25519
//!   identity keypairs.
//! - [`storage`] — load and save the identity JSON blob.
//! - [`client`] — sync HTTP client (`ureq` 2) for the prototype key
//!   server endpoints used in this layer (`/v1/register`,
//!   `/v1/pubkeys/:user_id`).

pub mod account_ownership_error;
pub mod account_ownership_proof;
pub mod burn;
pub mod burn_alert;
pub mod client;
pub mod control_inbox;
pub mod duress;
pub mod identity;
pub mod identity_bundle;
pub mod keystore_anchor;
pub mod license_cache;
pub mod password;
pub mod pending_rotation;
pub mod prekeys;
pub mod proof_challenge;
pub mod recipients;
pub mod sealer;
mod sender_filter_rollout;
pub mod sensitive_memory;
pub mod signed_get;
pub mod storage;
pub mod unregister;
pub mod wrapped_key;

// A8: `Sealer::unseal` returns `Zeroizing<Vec<u8>>`, which makes that type part
// of this crate's public API. Without this re-export no caller outside the crate
// can name the return type or implement the trait, which is exactly what broke
// `tests/sealer_test.rs` when the wrapper was introduced.
pub use zeroize::Zeroizing;

pub use account_ownership_error::AccountOwnershipError;
pub use account_ownership_proof::{
    canonical_account_ownership_proof_bytes, AccountOwnershipEvidence, AccountOwnershipProof,
    ACCOUNT_OWNERSHIP_PROOF_DOMAIN, ACCOUNT_OWNERSHIP_PROOF_TYPE_ED25519_CHALLENGE_V1,
};
pub use burn::{canonical_burn_bytes, sign_burn, BurnScope, BURN_DOMAIN};
pub use burn_alert::{sign_burn_alert, verify_burn_alert, BurnAlertPayload, BURN_ALERT_DOMAIN};
pub use client::{
    validate_peer_bundle, BurnResponse, ControlInboxItem, ControlInboxPostResponse,
    IdentityBundleError, KeyServerClient, LicenseValidateResponse, PrekeyBundleOpk,
    PrekeyBundleResponse, PubkeysResponse, RegisterResponse, ReplenishResponse,
    WrappedKeyPostResponse, WrappedKeyResponse,
};
pub use duress::{
    DuressEngine, DuressError, DuressHandlers, DuressJournal, DuressPaths, DuressReport,
    ProductionDuressHandlers, StepOutcome, WipeFn, WipeStep,
};
pub use identity::{
    generate_identity, generate_native_identity, identity_from_entropy,
    native_identity_from_entropy, native_user_id, Identity, IDENTITY_BLOB_VERSION,
};
pub use keystore_anchor::KeystoreBackedAnchor;
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
pub use proof_challenge::{ProofChallenge, PROOF_CHALLENGE_NONCE_BYTES};
pub use recipients::{
    account_dir, active_account_dir, get_recipients, get_recipients_from_path, osl_base_dir,
    osl_config_dir, set_active_account_dir, set_base_dir_override, RecipientError,
};
pub use sealer::{
    evict_tpm_key, select_best_sealer, verify_sealer_round_trip, KeyringSealer, MemorySealer,
    NoOpSealer, Sealer, SealerError, TpmSealer, METHOD_EPHEMERAL, METHOD_KEYRING, METHOD_MEMORY,
    METHOD_NOOP, METHOD_TPM,
};
pub use signed_get::{
    canonical_prekey_bundle_get_bytes, canonical_wrapped_key_get_bytes, sign_prekey_bundle_get,
    sign_wrapped_key_get, PREKEY_BUNDLE_GET_DOMAIN, WRAPPED_KEY_GET_DOMAIN,
};
pub use storage::{load_identity, save_identity, IdentityOnDisk};
pub use unregister::{canonical_unregister_bytes, sign_unregister, UNREGISTER_DOMAIN};
pub use wrapped_key::{
    canonical_wrapped_key_post_bytes, sign_wrapped_key_post, WrappedKeyUpload,
    WRAPPED_KEY_POST_DOMAIN,
};

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

    #[error("OSL: keyserver bundle proof invalid")]
    PeerBundleProofInvalid,

    #[error("required handshake key is absent")]
    PrekeyMissing,
}

pub type Result<T> = core::result::Result<T, Error>;
