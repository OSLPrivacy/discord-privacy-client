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
//!
//! - `peer_map.json` — Discord-id → OSL-user-id translation for
//!   the Phase 5 receive-side decoder. Loaded into
//!   `AppState::peer_map`. Missing or malformed file is
//!   non-fatal: we log an onboarding-friendly hint and start
//!   with an empty map (every receive returns `UnknownSender`,
//!   which the JS hook handles silently).

use ipc::peer_map::{load_peer_map_from_path, PeerMapError};
use ipc::AppState;
use keystore::{
    generate_identity, load_identity, save_identity, select_best_sealer, KeyServerClient,
};
use serde::Deserialize;
use std::path::PathBuf;
use store::MessageStore;

/// On-disk schema for `<config_dir>/keyserver.json`. `base_url`
/// and `user_id` required; `admin_token` optional. Missing or
/// malformed file → bootstrap logs and skips.
#[derive(Debug, Deserialize)]
struct KeyserverConfig {
    /// Base URL of the key server. Plain `http://` only (TLS is
    /// terminated upstream by the hosting platform — Cloudflare /
    /// Railway / etc.). See `keystore::client::KeyServerClient::new`.
    base_url: String,
    /// User identifier this client registers as. Phase 4 dogfood:
    /// any opaque string the two peers agree on (`"alice"`,
    /// `"bob"`, the Discord username, whatever). Phase 5
    /// integrates this with Discord OAuth.
    user_id: String,
    /// Phase B pre-shared admin token. Required for state-mutating
    /// keyserver routes when the deployed keyserver has
    /// `OSL_KEYSERVER_ADMIN_TOKEN` set. Leave unset in
    /// `keyserver.json` (or set to `null`) when running against an
    /// unsecured local-dev keyserver.
    #[serde(default)]
    admin_token: Option<String>,
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

    if identity_loaded {
        open_message_store(state, &dir);
    } else {
        tracing::info!(
            "OSL bootstrap: skipping message_store open (no identity loaded; \
             persistence stays disabled until next launch)"
        );
    }

    load_peer_map(state, &dir);

    // 7d-FIX1: load burned_scopes.json into AppState. Best-effort:
    // a missing file is normal on a fresh install; a parse error
    // leaves the ledger empty (no burns are enforced) which is
    // safe behavior — the worst outcome is the receive observer
    // re-decrypts what it previously could decrypt. A future
    // user-initiated burn refills the ledger.
    let bs_path = dir.join("burned_scopes.json");
    let bs = ipc::burned_scopes_file::load_burned_scopes(&bs_path);
    let n = bs.scopes.len();
    *state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned") = bs;
    tracing::info!(entries = n, "OSL bootstrap: burned_scopes loaded");

    tracing::info!("OSL bootstrap: done");
}

/// Open the at-rest-encrypted [`MessageStore`] under
/// `<config_dir>/store/` keyed off the loaded identity secret.
/// On open failure (sealer mismatch, schema-newer-than-binary,
/// disk error), log loudly at `warn!` and leave the store as
/// `None`; the decrypt path swallows persistence errors and the
/// `osl_load_channel_history` IPC returns an empty list, so a
/// store outage doesn't take Discord with it.
///
/// Caller must have already populated `state.identity` (we read
/// the secret bytes from there). This function is a no-op if
/// `state.identity` is `None`, but the caller already gates on
/// `identity_loaded` before invoking this.
fn open_message_store(state: &AppState, dir: &std::path::Path) {
    let store_dir = dir.join("store");
    let secret_bytes: [u8; 32] = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let Some(id) = id_guard.as_ref() else {
            tracing::warn!(
                "OSL bootstrap: open_message_store called without identity; \
                 leaving message_store disabled"
            );
            return;
        };
        // x25519::SecretKey::as_bytes returns &[u8; 32] — copy
        // out so we can drop the identity lock before the
        // potentially slow disk + sqlite path.
        *id.x25519_secret.as_bytes()
    };
    match MessageStore::open(&store_dir, &secret_bytes) {
        Ok(store) => {
            tracing::info!(
                path = %store_dir.display(),
                "OSL bootstrap: message_store opened"
            );
            *state
                .message_store
                .lock()
                .expect("message_store mutex poisoned") = Some(store);
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %store_dir.display(),
                "OSL bootstrap: message_store open failed; persistence disabled \
                 for this session (decrypt path still works, history won't \
                 rehydrate after restart)"
            );
        }
    }
}

/// Read `<dir>/peer_map.json` into `AppState::peer_map`. Every
/// failure mode is non-fatal:
///
/// - File missing: log an onboarding hint with the absolute path
///   so the user knows where to create it; leave the map empty.
/// - Read or parse failure: log the inner error so the user can
///   find the typo; leave the map empty.
///
/// Receive-side decryption is a no-op until the user populates
/// the file; send-side and identity bootstrap are unaffected.
fn load_peer_map(state: &AppState, dir: &std::path::Path) {
    let path = dir.join("peer_map.json");
    match load_peer_map_from_path(&path) {
        Ok(map) => {
            let n = map.len();
            *state.peer_map.lock().expect("peer_map mutex poisoned") = map;
            tracing::info!(
                path = %path.display(),
                entries = n,
                "OSL bootstrap: peer_map loaded"
            );
        }
        Err(PeerMapError::NotFound { .. }) => {
            tracing::info!(
                path = %path.display(),
                "OSL bootstrap: peer_map.json not found, decrypt will skip all \
                 incoming messages — create this file with \
                 {{\"<discord_id>\":\"<osl_user_id>\", ...}} to enable \
                 receive-side decryption"
            );
        }
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                error = %e,
                "OSL bootstrap: peer_map.json could not be loaded; receive-side \
                 decryption disabled until the file is fixed"
            );
        }
    }
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
    // Attach admin token if configured. `with_admin_token` normalises
    // empty strings to `None` so a `"admin_token": ""` in
    // keyserver.json doesn't end up sending a bad header.
    let client = client.with_admin_token(cfg.admin_token.clone());
    tracing::info!(
        base_url = %cfg.base_url,
        admin_token = if cfg.admin_token.as_deref().unwrap_or("").is_empty() {
            "absent (dev mode or local keyserver)"
        } else {
            "present"
        },
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
