//! Phase 7a fresh-start flow.
//!
//! Spec: `docs/phase-7-design.md` §1 ("Migration: fresh start, wipe
//! all local state and regenerate identity") and §3.6 ("Account
//! burn aftermath" — same wipe surface).
//!
//! ## Behaviour
//!
//! [`cmd_osl_fresh_start`] takes a config directory and:
//!
//! 1. Wipes `identity.json`, `peer_map.json`, `channels.json`,
//!    `whitelist_state.json`, `pending_invitations.json`.
//! 2. Wipes `store/messages.sqlite` and its WAL / SHM siblings.
//! 3. Generates a fresh [`keystore::Identity`] under the supplied
//!    `user_id` and writes it through [`keystore::save_identity`]
//!    with the best available sealer (TPM / Keyring / NoOp).
//! 4. Writes empty schemas to `peer_map.json`, `channels.json`,
//!    `whitelist_state.json`, `pending_invitations.json`.
//!
//! Each step is independently logged. Missing files at wipe time
//! are not errors — the operation is "ensure this directory looks
//! like a fresh install."
//!
//! ## Phase 7a scope
//!
//! Invokable from a Rust test or the dev console. Not wired into
//! Tauri commands yet — full UI integration arrives in 7d (settings
//! → "Fresh start / reinstall"). Tasks that need the legacy
//! `keyserver.json` (so the client can rejoin the keyserver under
//! the new identity) are also 7d's concern; this command does not
//! touch `keyserver.json`.
//!
//! The store dir lives at `<config_dir>/store/`; the messages
//! sqlite path is `<config_dir>/store/messages.sqlite`.

use crate::peer_map::write_peer_map;
use crate::whitelist_state::write_whitelist_state;
use keystore::{generate_identity, save_identity, select_best_sealer, Identity};
use std::path::{Path, PathBuf};

/// Errors that can surface during a fresh-start operation. Each
/// variant carries the offending path for diagnostic logging.
#[derive(Debug, thiserror::Error)]
pub enum FreshStartError {
    #[error("fresh_start: failed to remove {path}: {source}")]
    RemoveFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("fresh_start: failed to create config dir {path}: {source}")]
    MkdirFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("fresh_start: failed to write empty schema {path}: {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("fresh_start: failed to save fresh identity at {path}: {source}")]
    IdentitySaveFailed {
        path: PathBuf,
        #[source]
        source: keystore::Error,
    },
}

/// Wipe local state under `config_dir` and write a fresh-install
/// directory tree under a brand-new identity keypair.
///
/// `user_id` is the OSL identifier the new identity will register
/// as. Callers that want to preserve the previous user_id should
/// read it before calling (e.g. via [`keystore::load_identity`])
/// and pass it back in. The Phase 7d UI will prompt the user
/// explicitly.
///
/// Returns the freshly generated [`Identity`] so callers can place
/// it in [`crate::state::AppState`] without re-loading from disk.
pub fn cmd_osl_fresh_start(
    config_dir: &Path,
    user_id: String,
) -> Result<Identity, FreshStartError> {
    if !config_dir.exists() {
        std::fs::create_dir_all(config_dir).map_err(|source| FreshStartError::MkdirFailed {
            path: config_dir.to_path_buf(),
            source,
        })?;
    }

    // 1. Wipe top-level JSON state.
    for name in [
        "identity.json",
        "peer_map.json",
        "channels.json",
        "whitelist_state.json",
        "pending_invitations.json",
    ] {
        let path = config_dir.join(name);
        remove_if_present(&path)?;
    }

    // 2. Wipe the SQLite store + its WAL/SHM siblings.
    let store_dir = config_dir.join("store");
    for name in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        let path = store_dir.join(name);
        remove_if_present(&path)?;
    }

    // 3. Generate + persist a fresh identity.
    let identity = generate_identity(user_id);
    let identity_path = config_dir.join("identity.json");
    let sealer = select_best_sealer();
    save_identity(&identity_path, &identity, sealer.as_ref()).map_err(|source| {
        FreshStartError::IdentitySaveFailed {
            path: identity_path.clone(),
            source,
        }
    })?;

    // 4. Write fresh empty schemas.
    let peer_map_path = config_dir.join("peer_map.json");
    write_peer_map(&peer_map_path, &Default::default()).map_err(|source| {
        FreshStartError::WriteFailed {
            path: peer_map_path,
            source,
        }
    })?;

    let channels_path = config_dir.join("channels.json");
    // channels.json shape lives in `keystore::recipients` —
    // `{"channels": {}}`. We don't bring that type in just to
    // serialise an empty map; hard-coded literal is fine.
    std::fs::write(&channels_path, "{\"channels\":{}}\n").map_err(|source| {
        FreshStartError::WriteFailed {
            path: channels_path,
            source,
        }
    })?;

    let whitelist_path = config_dir.join("whitelist_state.json");
    write_whitelist_state(&whitelist_path, &Default::default()).map_err(|source| {
        FreshStartError::WriteFailed {
            path: whitelist_path,
            source,
        }
    })?;

    // 9-C1: pending_invitations.json is obsolete. The fresh-start
    // wipe step above already removed any leftover file; we no longer
    // write an empty stub.

    Ok(identity)
}

/// Best-effort file remove. `NotFound` is success (we're trying to
/// ensure the file isn't there). Anything else surfaces as
/// [`FreshStartError::RemoveFailed`].
fn remove_if_present(path: &Path) -> Result<(), FreshStartError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(FreshStartError::RemoveFailed {
            path: path.to_path_buf(),
            source,
        }),
    }
}
