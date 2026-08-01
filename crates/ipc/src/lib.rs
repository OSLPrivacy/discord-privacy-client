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
//! - Identity registration publishes the initial prekey batch. The messaging
//!   send path still does not fetch peer prekey bundles or consume local OPKs.
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
pub mod friend_request;
pub mod license_lifecycle;
pub mod log_id;
pub mod main_password;
pub mod mandatory_storage_key_policy;
pub mod membership;
pub mod migration;
pub mod peer_map;
pub mod prose_token;
pub mod recovery;
// OSL-RN ciphertexts are single-use.  This sealed cache lets transcript
// rendering reuse an already-decrypted payload without advancing the ratchet.
pub mod rn_plaintext_cache;
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
pub mod space_roster;
// Unit a45: encrypted UI-side storage contract (checklist A6). Defines the
// `SecureLocalStore` trait + `SealedStore` reference impl; does not migrate
// any caller yet (`apps/osl-hub-ui/src/main.ts` localStorage call sites and
// `main_password::maybe_encrypt` are separate, later units).
pub mod secure_local_store;
pub mod sender_attribution_proof;
pub mod sender_key_state;
// A7: the session lock that actually locks. Supersedes
// `main_password::lock_main_password_session` (which had zero non-test
// callers) and the file-key-only inactivity timer (which left the identity,
// peer map, whitelist, sender chains and MessageStore live and decrypting).
pub mod session_lock;
pub mod state;
pub mod state_reload;
pub mod tier_gate;
pub mod tombstone_file;
pub mod tofu;
pub mod whitelist;
pub mod whitelist_state;
pub mod wire_v2;
// OSL-RN (wire 0x10) integration: version selection with downgrade
// protection plus sealed per-peer ratchet session state. Strictly
// additive — the v=2/v=3/v=4/v=5 paths above are untouched.
pub mod wire_rn;
// OSL-RN per-peer health state. Kept separate from ratchet sessions so a
// recovery delete cannot erase the durable fact that a peer desynchronised.
pub mod rn_health;

// A handful of things this crate reaches for are genuinely process-global:
// `keystore::set_base_dir_override` / `set_active_account_dir` (an `RwLock`
// inside `keystore`), the file-storage-key slot and the inactivity auto-lock
// timer slot. `cargo test` runs the unit tests of one binary on N threads by
// default, so two tests that each point those globals at their own `TempDir`
// clobber each other: one test's teardown resets the override to `None` while
// another is mid-assertion, and the victim silently resolves to the real user
// config dir (or to a `TempDir` that has already been deleted).
//
// That is exactly how CI failed on `windows-latest`: the duress-gate tests read
// `C:\Users\runneradmin\AppData\Roaming\osl\password_marker.json` and a stale
// `...\Temp\.tmpXXXXXX\base\...`. It never reproduced locally because the local
// runs use `--test-threads=1`.
//
// Every test that installs one of those globals takes this lock, so they run
// one at a time regardless of the harness's thread count. It is re-entrant per
// thread: several tests take the lock and then call a helper (e.g.
// `use_temp_config_dir`) that takes it again, and a plain `Mutex` would
// self-deadlock there.
#[cfg(test)]
pub(crate) mod test_process_globals {
    use std::cell::Cell;
    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    thread_local! {
        static DEPTH: Cell<u32> = const { Cell::new(0) };
    }

    /// Held for as long as the caller owns the process globals. The inner
    /// guard is `None` for re-entrant acquisitions on the same thread.
    pub(crate) struct SerialGuard(Option<MutexGuard<'static, ()>>);

    impl Drop for SerialGuard {
        fn drop(&mut self) {
            // Release the re-entrancy count before the real guard, so the next
            // thread in never observes a stale depth.
            DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
            self.0.take();
        }
    }

    /// Serialize this test against every other test that mutates the
    /// process-global config-dir overrides, file storage key or auto-lock
    /// timer. Poisoning is recovered from: a panicking test still leaves the
    /// globals reset by its own guard, and failing every later test with
    /// "mutex poisoned" would hide the original failure.
    pub(crate) fn serialize() -> SerialGuard {
        let outermost = DEPTH.with(|depth| {
            let current = depth.get();
            depth.set(current + 1);
            current == 0
        });
        if outermost {
            SerialGuard(Some(LOCK.lock().unwrap_or_else(|err| err.into_inner())))
        } else {
            SerialGuard(None)
        }
    }
}

pub use at_rest_boundary::AtRestBoundary;
pub use commands::{
    AeadOpenRequest, AeadSealRequest, AeadSealResponse, FetchPubkeysResponse,
    GenerateIdentityResponse, RegisterResponse, StatusResponse, StegoDecodeResponse,
    StegoEncodeRequest, StegoEncodeResponse, UiSessionEncryptionKeyDto,
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
