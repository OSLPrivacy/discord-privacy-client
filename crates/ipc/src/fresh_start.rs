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
use keystore::{
    generate_identity, load_identity, pending_rotation_from, save_identity,
    save_pending_rotation, select_best_sealer, Identity,
};
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

    // 0. SECURITY FORWARD-FIX: while the OLD identity still exists on
    // disk, pre-sign a Case-C rotation authorizing the NEW identity
    // and persist it (sealed) as `pending_rotation.json`. After the
    // burn the old Ed25519 secret is destroyed, so this is the only
    // moment the client can ever produce the keyserver's required
    // `prev_sig`. Best-effort: a first-ever install has no old
    // identity to rotate from, and any failure here must NOT fail the
    // burn (the wipe + fresh identity still proceed; the register
    // path simply falls back to a plain register as before).
    //
    // The sealer is selected once and reused for the new identity
    // save below so both files seal under the same method tag.
    let sealer = select_best_sealer();
    let old_identity: Option<Identity> = {
        let old_path = config_dir.join("identity.json");
        match load_identity(&old_path, sealer.as_ref()) {
            Ok(id) if !id.user_id.is_empty() => Some(id),
            Ok(_) => {
                tracing::info!(
                    "OSL: fresh_start: existing identity has empty user_id; \
                     nothing to rotate from (skipping pending-rotation mint)"
                );
                None
            }
            Err(e) => {
                tracing::info!(
                    error = %e,
                    "OSL: fresh_start: no readable existing identity; \
                     skipping pending-rotation mint (first-ever install \
                     or unreadable blob — nothing to rotate from)"
                );
                None
            }
        }
    };

    // 1. Wipe top-level JSON state. NOTE: `pending_rotation.json` is
    // intentionally NOT in this list — the pre-signed proof minted in
    // step 0 (written in step 3b, AFTER this purge) must survive a
    // fresh start so the post-burn register can present it.
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

    // 3. Generate + persist a fresh identity (sealed under the same
    // sealer used for the pending-rotation mint in step 0).
    let identity = generate_identity(user_id);
    let identity_path = config_dir.join("identity.json");
    save_identity(&identity_path, &identity, sealer.as_ref()).map_err(|source| {
        FreshStartError::IdentitySaveFailed {
            path: identity_path.clone(),
            source,
        }
    })?;

    // 3b. SECURITY FORWARD-FIX: now that the NEW identity exists,
    // persist the pre-signed Case-C rotation proof — but only if we
    // had a real old identity to rotate from. Written AFTER the
    // step-1 purge and AFTER the new identity save so it deterministi-
    // cally survives the wipe on the happy path. Failure here is
    // logged and swallowed: the burn must still succeed (the register
    // path then falls back to a plain register, exactly as before
    // this fix).
    if let Some(ref old) = old_identity {
        let pending = pending_rotation_from(old, &identity);
        let pending_path = config_dir.join("pending_rotation.json");
        match save_pending_rotation(&pending_path, &pending, sealer.as_ref()) {
            Ok(()) => tracing::info!(
                "OSL: fresh_start: persisted pre-signed Case-C rotation \
                 proof (pending_rotation.json) — post-burn register will \
                 present it so the new key publishes despite the destroyed \
                 old key"
            ),
            Err(e) => tracing::warn!(
                error = %e,
                "OSL: fresh_start: failed to persist pending_rotation.json \
                 (non-fatal; burn proceeds, register falls back to plain \
                 register)"
            ),
        }
    }

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
