//! Layer 10 / Phase 4 autostart bootstrap.
//!
//! Runs once during `tauri::Builder::setup` and populates
//! [`ipc::AppState`] from on-disk config, so the very first call to
//! `osl_encrypt_message` from the Discord webview hits a fully
//! initialised pipeline rather than the
//! `OSL: identity not loaded` / `OSL: key-server not initialised`
//! errors.
//!
//! This is **fail-loud, never fatal**: every step that fails logs
//! a `tracing::warn!` (or `info!` for an expected absence) and
//! returns. Subsequent steps run with whatever state is available.
//! Worst-case fallback: `AppState` stays at its default `(None,
//! None)` and `osl_encrypt_message` continues to surface the
//! identity-missing / keyserver-missing errors normally — exactly
//! the behaviour Phase 3 / Phase 4-pre-bootstrap shipped with.
//!
//! The reasons this isn't a single hard-fail-on-any-error
//! sequence:
//!
//! - **First-boot UX.** A user who has not yet populated
//!   `keyserver.json` should see the app start, log a clear
//!   "no keyserver.json; populate to enable" line, and let them
//!   create the file before their first send. The alternative
//!   (refuse to start) is hostile.
//! - **Partial-success states are useful.** If the keyserver is
//!   down at startup but identity loads fine, the user can still
//!   open Discord; sends will fail closed at the IPC boundary
//!   with a clear `OSL: fetch_pubkeys` error. That's better than
//!   refusing to start the GUI.
//! - **The Tauri webview is the source of truth for the user's
//!   working state.** Logs are diagnostic, not authoritative —
//!   the `osl_encrypt_message` IPC return value is what drives the
//!   UI's "did it work?" signal. Bootstrap just primes the cache.
//!
//! ## Files read (all under [`keystore::osl_config_dir`])
//!
//! - `identity.json` — sealed identity blob (TPM / Keyring / NoOp,
//!   per [`keystore::select_best_sealer`]). If present, loaded;
//!   if absent and we have a `user_id` from `keyserver.json`,
//!   generated fresh and saved.
//! - `keyserver.json` — `{ "base_url": "http://...", "user_id":
//!   "..." }`. If present, used to init [`KeyServerClient`] and
//!   register; if absent, identity-bootstrap is skipped (no
//!   `user_id` to seed `generate_identity` with).
//!
//! `channels.json` is read on-demand by `osl_encrypt_message` per
//! call (see `keystore::recipients`); bootstrap doesn't touch it.

use ipc::AppState;
use keystore::{
    generate_identity, load_identity, save_identity, select_best_sealer,
    KeyServerClient,
};
use serde::Deserialize;
use std::path::PathBuf;

/// On-disk schema for `<config_dir>/keyserver.json`. Both fields
/// required; missing or malformed → bootstrap logs and skips.
#[derive(Debug, Deserialize)]
struct KeyserverConfig {
    /// Base URL of the prototype key server. Plain `http://` only
    /// (the prototype server is plain HTTP — see
    /// `keystore::client::KeyServerClient::new`).
    base_url: String,
    /// User identifier this client registers as. Phase 4 dogfood:
    /// any opaque string the two peers agree on (`"alice"`,
    /// `"bob"`, the Discord username, whatever). Phase 5
    /// integrates this with Discord OAuth.
    user_id: String,
}

/// Run the autostart sequence. Logs progress at `info!` (visible
/// in `--release` builds with `RUST_LOG=info`) and failures at
/// `warn!`. Never panics, never returns an error — all failure
/// surfaces are tracing events.
///
/// Caller: `tauri::Builder::setup` once per app launch.
pub fn run_autostart(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "OSL bootstrap: cannot resolve config directory; \
                 skipping autostart entirely"
            );
            return;
        }
    };
    tracing::info!(config_dir = %dir.display(), "OSL bootstrap: starting");

    let keyserver_cfg = read_keyserver_config(&dir);
    let identity_loaded = load_or_generate_identity(state, &dir, keyserver_cfg.as_ref());
    if let Some(cfg) = keyserver_cfg {
        if identity_loaded {
            init_keyserver_and_register(state, &cfg);
        } else {
            tracing::info!(
                base_url = %cfg.base_url,
                "OSL bootstrap: keyserver.json present but no identity loaded; \
                 skipping init_keyserver + register"
            );
        }
    }

    tracing::info!("OSL bootstrap: done");
}

/// Read `<dir>/keyserver.json`. Returns `None` for any failure
/// (logged at the appropriate level) so the caller can branch
/// without nested error handling.
fn read_keyserver_config(dir: &std::path::Path) -> Option<KeyserverConfig> {
    let path = dir.join("keyserver.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!(
                path = %path.display(),
                "OSL bootstrap: no keyserver.json; skipping keyserver init \
                 (populate {{\"base_url\":\"http://host:port\",\"user_id\":\"name\"}} \
                 to enable)"
            );
            return None;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "OSL bootstrap: keyserver.json read failed; skipping"
            );
            return None;
        }
    };
    match serde_json::from_str::<KeyserverConfig>(&raw) {
        Ok(c) => {
            tracing::info!(
                base_url = %c.base_url,
                user_id = %c.user_id,
                "OSL bootstrap: keyserver.json parsed"
            );
            Some(c)
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "OSL bootstrap: keyserver.json malformed; skipping keyserver init"
            );
            None
        }
    }
}

/// Decide how to populate `state.identity`:
///
/// - `identity.json` exists → load (any sealer error logged and
///   we proceed without identity).
/// - `identity.json` missing AND `keyserver.json` present →
///   generate fresh using the keyserver-config user_id, save.
/// - `identity.json` missing AND `keyserver.json` missing → skip
///   (no `user_id` available to seed `generate_identity`).
///
/// Returns `true` iff `state.identity` is now `Some(_)`.
///
/// Mismatch case: if both files exist but
/// `identity.user_id != keyserver.user_id`, the loaded identity
/// wins (it's already been registered) and we log a warning.
fn load_or_generate_identity(
    state: &AppState,
    dir: &std::path::Path,
    keyserver_cfg: Option<&KeyserverConfig>,
) -> bool {
    let path = dir.join("identity.json");
    let sealer = select_best_sealer();

    if path.exists() {
        match load_identity(&path, sealer.as_ref()) {
            Ok(id) => {
                if let Some(cfg) = keyserver_cfg {
                    if id.user_id != cfg.user_id {
                        tracing::warn!(
                            identity_user_id = %id.user_id,
                            keyserver_user_id = %cfg.user_id,
                            "OSL bootstrap: keyserver.json user_id differs from \
                             identity.json user_id; using identity's (already \
                             registered against the keyserver). Edit keyserver.json \
                             or regenerate identity to resolve."
                        );
                    }
                }
                tracing::info!(
                    user_id = %id.user_id,
                    path = %path.display(),
                    "OSL bootstrap: identity loaded"
                );
                *state.identity.lock().expect("identity mutex poisoned") = Some(id);
                return true;
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "OSL bootstrap: identity load failed; continuing without identity"
                );
                return false;
            }
        }
    }

    // identity.json missing; only proceed if keyserver.json gave us a user_id.
    let cfg = match keyserver_cfg {
        Some(c) => c,
        None => {
            tracing::info!(
                "OSL bootstrap: no identity.json and no keyserver.json; \
                 skipping identity bootstrap (populate keyserver.json to seed \
                 first-boot identity generation)"
            );
            return false;
        }
    };

    let id = generate_identity(cfg.user_id.clone());
    tracing::info!(
        user_id = %id.user_id,
        "OSL bootstrap: generated fresh identity"
    );
    if let Err(e) = ensure_parent_exists(&path) {
        tracing::warn!(
            error = %e,
            path = %path.display(),
            "OSL bootstrap: create_dir_all for identity path failed"
        );
        // Try save anyway — it might succeed if the dir actually
        // exists despite create_dir_all's complaint.
    }
    match save_identity(&path, &id, sealer.as_ref()) {
        Ok(()) => tracing::info!(
            path = %path.display(),
            "OSL bootstrap: identity saved"
        ),
        Err(e) => tracing::warn!(
            error = %e,
            path = %path.display(),
            "OSL bootstrap: identity save failed; identity exists in memory \
             but won't survive a restart"
        ),
    }
    *state.identity.lock().expect("identity mutex poisoned") = Some(id);
    true
}

/// Init [`KeyServerClient`] and call `register`. Both steps log
/// and continue on failure — `register` failure leaves the
/// keyserver client populated, so subsequent `fetch_pubkeys`
/// calls can still succeed (in case the failure was transient).
fn init_keyserver_and_register(state: &AppState, cfg: &KeyserverConfig) {
    let client = match KeyServerClient::new(&cfg.base_url) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                base_url = %cfg.base_url,
                "OSL bootstrap: KeyServerClient::new failed; skipping register"
            );
            return;
        }
    };
    tracing::info!(
        base_url = %cfg.base_url,
        "OSL bootstrap: KeyServerClient initialised"
    );

    {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let id = id_guard
            .as_ref()
            .expect("load_or_generate_identity returned true");
        match client.register(id) {
            Ok(resp) => tracing::info!(
                user_id = %resp.user_id,
                initial = resp.registered_at.is_some(),
                "OSL bootstrap: registered with key-server"
            ),
            Err(e) => tracing::warn!(
                error = %e,
                "OSL bootstrap: key-server register failed; \
                 fetch_pubkeys may still work if peers are already registered"
            ),
        }
    }

    // Install the client AFTER attempting register, so that even
    // if register fails the client is available for fetch_pubkeys
    // calls (the prototype keyserver doesn't strictly require us
    // to be registered to read other users' pubkeys).
    *state.keyserver.lock().expect("keyserver mutex poisoned") = Some(client);
}

/// Convenience: ensure the parent directory of `path` exists
/// (recursive). No-op if it already does.
fn ensure_parent_exists(path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
    } else {
        Ok(())
    }
}
