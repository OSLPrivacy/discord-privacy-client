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
use keystore::{generate_identity, load_identity, save_identity, select_best_sealer};
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use store::MessageStore;

/// On-disk schema for `<config_dir>/keyserver.json`. `base_url`
/// and `user_id` required; `client_token` optional. The legacy
/// `admin_token` key is accepted only as a migration alias.
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
    /// Coarse pre-shared client token. Required for state-mutating
    /// keyserver routes when the deployed keyserver has
    /// `OSL_KEYSERVER_CLIENT_TOKEN` set. Leave unset in
    /// `keyserver.json` (or set to `null`) when running against an
    /// unsecured local-dev keyserver.
    #[serde(default, alias = "admin_token")]
    client_token: Option<String>,
}

/// Run the autostart sequence. Logs progress at `info!` (visible
/// in `--release` builds with `RUST_LOG=info`) and failures at
/// `warn!`. Never panics, never returns an error — all failure
/// surfaces are tracing events.
///
/// Caller: `tauri::Builder::setup` once per app launch.
pub fn run_autostart(state: &AppState) {
    run_autostart_mode(state, true);
}

/// Load every local OSL artifact without waiting on the key-server network.
/// Desktop shells can render and unlock immediately, then call
/// [`register_after_local_bootstrap`] from a bounded background worker.
pub fn run_autostart_local(state: &AppState) {
    run_autostart_mode(state, false);
}

fn run_autostart_mode(state: &AppState, register_online: bool) {
    // Multi-account: before resolving the config dir, point it at the
    // persisted active account (if any). With no marker this is a
    // no-op and osl_config_dir() returns the shared base exactly as a
    // single-account install always did.
    resolve_active_account_on_launch();
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
    // 9-F0-FIX1: ensure the config dir exists before any loader /
    // persister runs. On a truly clean install (%APPDATA%\osl
    // absent on Windows, ~/.config/osl absent on Linux), the
    // individual writers' create_dir_all calls cover their own
    // parent paths, but `persist_app_preferences_now` writes
    // straight to `<dir>/app_preferences.json.tmp` without a
    // parent-mkdir and surfaces `os error 3 (cannot find path)`.
    // Doing the mkdir once here covers every downstream loader +
    // writer that lives inside `dir`.
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!(
            error = %e,
            path = %dir.display(),
            "OSL bootstrap: create_dir_all failed; downstream writes will likely fail"
        );
    }
    tracing::info!(config_dir = %dir.display(), "OSL bootstrap: starting");

    match ipc::commands::recover_orphaned_account_imports(&dir) {
        Ok(0) => {}
        Ok(count) => tracing::warn!(
            count,
            "OSL bootstrap: restored account state after interrupted import"
        ),
        Err(e) => {
            tracing::error!(
                error = %e,
                "OSL bootstrap: import recovery failed; refusing to load mixed account state"
            );
            return;
        }
    }

    // Device-level config (keyserver URL/token, license, app prefs,
    // password gate) lives at the shared BASE, not per-account — so a
    // multi-account switch doesn't reset prefs or drop a paid license.
    // When no account override is active, base == dir (unchanged).
    let base = keystore::osl_base_dir().unwrap_or_else(|_| dir.clone());

    let keyserver_cfg = read_keyserver_config(&base);
    let (identity_loaded, identity_regenerated) =
        load_or_generate_identity(state, &dir, keyserver_cfg.as_ref());
    // G3-FIX: keyserver.json is an OVERRIDE only. The base URL always
    // resolves (keyserver.json `base_url` if present+valid → else the
    // built-in production default, via the single shared resolver),
    // so a fresh install with NO keyserver.json still builds +
    // installs the KeyServerClient and registers its identity
    // pubkeys. Previously this whole block was skipped when
    // keyserver.json was absent, leaving the machine absent from
    // /v1/pubkeys so no peer could encrypt to it. `client_token`
    // (and identity seeding) still come from keyserver.json when
    // present. Identity lifecycle is unchanged — register still
    // requires a loaded identity.
    if identity_loaded && register_online {
        let base_url = ipc::commands::resolve_keyserver_base_url(&base);
        let client_token = keyserver_cfg.as_ref().and_then(|c| c.client_token.clone());
        init_keyserver_and_register(state, &base_url, client_token);
    } else if identity_loaded {
        let base_url = ipc::commands::resolve_keyserver_base_url(&base);
        let client_token = keyserver_cfg.as_ref().and_then(|c| c.client_token.clone());
        install_keyserver_without_register(state, &base_url, client_token);
        tracing::info!("OSL bootstrap: deferred key-server registration until after local launch");
    } else {
        tracing::info!(
            "OSL bootstrap: no identity loaded; skipping init_keyserver \
             + register (keyserver client installs once an identity exists)"
        );
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

    // Finding 3b companion: if this launch regenerated the local
    // identity outside a burn, every peer's ratchet_state in the
    // just-loaded peer_map was derived from the OLD local identity's
    // SessionContext and is undecryptable. Drop them all so the next
    // v=4 re-handshakes. Done AFTER load_peer_map so it operates on
    // the real map. The in-memory clear protects THIS pre-gate
    // session; but for a password user with an encrypted
    // peer_map.json the persist inside the helper is refused by
    // write_peer_map's clobber guard (file_storage_key not installed
    // pre-gate), so the stale on-disk ratchet survives. D: also raise
    // the launch-scoped flag so state_reload re-runs the clear
    // POST-gate (key installed → persist durable), closing that
    // residual.
    if identity_regenerated {
        ipc::commands::clear_all_peer_ratchet_state(state);
        state
            .identity_regenerated_this_launch
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    // 7d-PIVOT: load whitelist_state.json from disk, then run
    // migration. Pre-PIVOT, encrypt_toggle was coupled to whitelist
    // presence and only existed in memory between mutations + the
    // FIX1 persist side; pre-FIX1 installs may not have a
    // whitelist_state.json at all. PIVOT decouples encrypt_toggle
    // from whitelist and reads it from disk, so we need both the
    // load and a migration step that turns ON encrypt_toggle for
    // scopes that had any whitelisted_users / members entries (the
    // old behaviour was implicit encrypt-when-whitelisted, and
    // users with existing whitelists almost certainly want that
    // behaviour preserved across the upgrade).
    load_and_migrate_whitelist_state(state, &dir);

    // W2: load durable scope-membership accrual (membership.json).
    load_scope_membership(state, &dir);

    // 7d-FIX3b: verify peer_map has a self-entry that matches the
    // loaded identity. If the identity has a snowflake but
    // peer_map doesn't (e.g. after a backup restore, or after a
    // pre-FIX3b install upgraded), repair the entry in place and
    // persist. If the identity has no snowflake yet, defer to
    // boot.js to extract it from Discord runtime and call
    // `osl_register_self_snowflake`.
    if identity_loaded {
        match ipc::commands::verify_and_persist_peer_map_self_entry(state) {
            Ok((snowflake, repaired)) => {
                if repaired {
                    eprintln!("[OSL][bootstrap] self-entry repaired for snowflake={snowflake}");
                } else {
                    eprintln!("[OSL][bootstrap] self-entry verified");
                }
            }
            Err(reason) if reason == "no_discord_snowflake" => {
                eprintln!(
                    "[OSL][bootstrap] no discord snowflake on identity; \
                     deferring self-entry to boot.js"
                );
            }
            Err(reason) if reason == "identity_not_loaded" => {
                tracing::warn!(
                    "OSL bootstrap: verify_peer_map_self_entry reported \
                     identity_not_loaded despite identity_loaded gate"
                );
            }
            Err(other) => {
                tracing::warn!(
                    error = %other,
                    "OSL bootstrap: verify_peer_map_self_entry failed; \
                     continuing — boot.js can retry via osl_register_self_snowflake"
                );
            }
        }
    }

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

    // Phase 9-C1: pending_invitations.json is obsolete (handshake
    // removed). Delete it unconditionally so a downgrade doesn't
    // bring the banner UI back from an old file.
    let pi_path = dir.join("pending_invitations.json");
    if pi_path.exists() {
        match std::fs::remove_file(&pi_path) {
            Ok(()) => tracing::info!(
                path = %pi_path.display(),
                "OSL bootstrap: removed legacy pending_invitations.json (C1)"
            ),
            Err(e) => tracing::warn!(
                error = %e,
                path = %pi_path.display(),
                "OSL bootstrap: could not remove legacy pending_invitations.json"
            ),
        }
    }

    // Phase 9-B1: load app_preferences.json. Missing file → defaults
    // (Mode 0). The send pipeline reads `stego_mode` to pick between
    // DPC0:: and DPC1:: cover envelopes. 9-MODE1-FIX dropped the
    // preview-modal-related fields; legacy files still load via
    // serde's unknown-field tolerance.
    let prefs_path = base.join("app_preferences.json");
    let prefs = ipc::app_preferences::load_app_preferences(&prefs_path);
    tracing::info!(
        mode = ?prefs.stego_mode,
        "OSL bootstrap: app_preferences loaded"
    );
    *state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") = prefs;

    // F2.4: sync cache-only classify of the license state. Stamps
    // AppState.license_state so the very first `osl_get_license_state`
    // read from the webview returns the cached classification — no
    // launch flicker for a paid user. The async keyserver refresh
    // (driven by main.rs setup's 6h-tick task) overwrites this
    // value when it returns. Cache-only here means no network and
    // no PaidOfflineGrace: that case requires a failed online
    // attempt, which only happens in `refresh_license_state`.
    ipc::license_lifecycle::launch_classify(state, &base);
    tracing::info!(
        license_state = ?state
            .license_state
            .lock()
            .expect("license_state mutex poisoned")
            .state,
        "OSL bootstrap: license cache classified"
    );

    // F3.6 pivot: the F3.1 `set_launch_time_once` stamp has been
    // removed alongside the 60-min launch-window model. New tier
    // model is feature-gated: free users get unlimited encrypted
    // text; paid users additionally unlock encrypted attachments
    // + beta-channel access. No clocks anywhere in the tier
    // pipeline; `tier_gate::is_paid_equivalent` is the sole
    // bottom-line check.

    tracing::info!("OSL bootstrap: done");
}

/// Perform the network portion omitted by [`run_autostart_local`]. The caller
/// must run this outside a UI/setup thread.
pub fn register_after_local_bootstrap(state: &AppState) {
    if state
        .identity
        .lock()
        .map(|identity| identity.is_none())
        .unwrap_or(true)
    {
        return;
    }
    let Ok(base) = keystore::osl_base_dir() else {
        return;
    };
    let keyserver_cfg = read_keyserver_config(&base);
    let base_url = ipc::commands::resolve_keyserver_base_url(&base);
    let client_token = keyserver_cfg.and_then(|config| config.client_token);
    init_keyserver_and_register(state, &base_url, client_token);
}

fn install_keyserver_without_register(
    state: &AppState,
    base_url: &str,
    client_token: Option<String>,
) {
    let client = match keystore::KeyServerClient::new(base_url) {
        Ok(client) => client.with_client_token(client_token),
        Err(error) => {
            tracing::warn!(%error, "OSL bootstrap: deferred key-server client could not be created");
            return;
        }
    };
    let mut slot = state.keyserver.lock().expect("keyserver mutex poisoned");
    if slot.is_none() {
        *slot = Some(client);
    }
}

// ============================================================
// Multi-account (#2): each Discord account gets its own OSL identity +
// state under `<base>/accounts/<snowflake>/`. The active account is
// persisted in `<base>/active`. `osl_config_dir()` is overridden to the
// active account's dir, so all ~60 state-file consumers transparently
// read/write the right account. Switching resets account-scoped
// AppState, flips the override, and re-runs the (tested) load sequence.
// ============================================================

/// Account-state files that live PER ACCOUNT. password/lockout markers,
/// app prefs, license, and keyserver.json stay device-level at the base.
const ACCOUNT_STATE_FILES: &[&str] = &[
    "identity.json",
    "peer_map.json",
    "whitelist_state.json",
    "sender_key_state.json",
    "channels.json",
    "burned_scopes.json",
    "membership.json",
    "scope_ttl.json",
    "scope_blobs.json",
];

fn active_marker_path() -> Option<std::path::PathBuf> {
    keystore::osl_base_dir().ok().map(|b| b.join("active"))
}

fn read_active_marker() -> Option<String> {
    let p = active_marker_path()?;
    read_active_marker_at(&p)
}

fn read_active_marker_at(p: &Path) -> Option<String> {
    if let Some(value) = read_snowflake_marker(&p) {
        return Some(value);
    }

    // A portable replacement briefly parks the previous marker in .bak.
    // If the process or machine died in that tiny window, recover the last
    // complete marker; never interpret the incomplete .tmp file.
    let backup = p.with_extension("bak");
    let value = read_snowflake_marker(&backup)?;
    let _ = std::fs::rename(&backup, &p);
    Some(value)
}

fn read_snowflake_marker(path: &Path) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    let s = s.trim();
    if s.chars().all(|c| c.is_ascii_digit()) && (17..=20).contains(&s.len()) {
        return Some(s.to_string());
    }
    None
}

fn write_active_marker(snowflake: &str) -> Result<(), String> {
    let p = active_marker_path().ok_or_else(|| {
        "OSL: active marker: unable to resolve the OSL base directory".to_string()
    })?;
    write_active_marker_at(&p, snowflake)
        .map_err(|e| format!("OSL: active marker: failed to persist {}: {e}", p.display()))?;
    eprintln!(
        "[OSL][multi-account] wrote active marker={snowflake} → {}",
        p.display()
    );
    Ok(())
}

/// Crash-recoverable, portable marker replacement. `rename(tmp, target)`
/// cannot replace an existing file on every supported Windows version, so
/// retain the last complete marker as `.bak` until the new marker is live.
fn write_active_marker_at(path: &Path, snowflake: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("active marker has no parent"))?;
    std::fs::create_dir_all(parent)?;

    let tmp = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)?;
    // Keep the legacy digits-only format so downgrading to an older client
    // cannot make it ignore the per-account directory. Crash safety comes
    // from temp + backup replacement rather than an incompatible envelope.
    file.write_all(snowflake.as_bytes())?;
    file.sync_all()?;
    drop(file);

    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    let had_previous = path.exists();
    if had_previous {
        std::fs::rename(path, &backup)?;
    }
    if let Err(install_error) = std::fs::rename(&tmp, path) {
        if had_previous {
            let _ = std::fs::rename(&backup, path);
        }
        let _ = std::fs::remove_file(&tmp);
        return Err(install_error);
    }
    if had_previous {
        // The new marker is already the committed source of truth. A stale
        // backup is harmless and will be cleaned before the next replacement;
        // do not report a failed switch after the commit point.
        let _ = std::fs::remove_file(&backup);
    }
    Ok(())
}

/// At launch, set the config-dir override to the active account's dir
/// when a marker names one whose identity exists. Otherwise leave the
/// override unset (single-account / pre-migration → shared base).
fn resolve_active_account_on_launch() {
    let Some(sf) = read_active_marker() else {
        eprintln!("[OSL][multi-account] launch: no active marker — using BASE dir");
        return;
    };
    match keystore::account_dir(&sf) {
        Ok(adir) => {
            let id_path = adir.join("identity.json");
            if id_path.exists() {
                eprintln!(
                    "[OSL][multi-account] launch: active account={sf} → dir={}",
                    adir.display()
                );
                keystore::set_active_account_dir(Some(adir));
            } else {
                eprintln!(
                    "[OSL][multi-account] launch: marker={sf} but {} MISSING — \
                     falling back to BASE dir (this loses per-account state!)",
                    id_path.display()
                );
            }
        }
        Err(e) => {
            eprintln!("[OSL][multi-account] launch: account_dir({sf}) failed: {e} — using BASE");
        }
    }
}

/// Reset every ACCOUNT-SCOPED AppState field to empty so a switch can't
/// leak the previous account's data (peers, sessions, whitelists) into
/// the next — critical for a privacy tool. Device-level state (license,
/// app prefs, password gate) is intentionally left alone.
fn reset_account_scoped_state(state: &AppState) {
    use std::sync::atomic::Ordering;
    *state.identity.lock().expect("identity poisoned") = None;
    *state.keyserver.lock().expect("keyserver poisoned") = None;
    *state
        .registration_alert
        .lock()
        .expect("registration_alert poisoned") = None;
    state.sender_pubkey_cache.clear();
    *state.message_store.lock().expect("message_store poisoned") = None;
    *state.peer_map.lock().expect("peer_map poisoned") = Default::default();
    *state.whitelist_state.lock().expect("whitelist poisoned") = Default::default();
    *state
        .server_defaults
        .lock()
        .expect("server_defaults poisoned") = Default::default();
    *state.sender_key_state.lock().expect("sender_key poisoned") = Default::default();
    *state.burned_scopes.lock().expect("burned poisoned") = Default::default();
    *state.scope_membership.lock().expect("membership poisoned") = Default::default();
    *state
        .channel_members
        .lock()
        .expect("channel_members poisoned") = Default::default();
    *state.key_change_alerts.lock().expect("key_change poisoned") = Default::default();
    *state.friend_ids.lock().expect("friend_ids poisoned") = Default::default();
    *state.guild_list.lock().expect("guild_list poisoned") = Default::default();
    *state
        .recovery_guard
        .lock()
        .expect("recovery_guard poisoned") = Default::default();
    state
        .mode1_reassembly
        .lock()
        .expect("mode1_reassembly poisoned")
        .clear();
    *state
        .recovery_token
        .lock()
        .expect("recovery_token poisoned") = None;
    *state
        .last_persist_error
        .lock()
        .expect("last_persist_error poisoned") = None;
    state
        .identity_regenerated_this_launch
        .store(false, Ordering::SeqCst);
}

/// Reload the currently selected account after a trusted on-disk restore.
/// The caller must hold `account_switch_lock` so no switch can interleave the
/// reset/reload sequence.
pub fn reload_active_account_state(state: &AppState) -> Result<(), String> {
    let expected = read_active_marker().or_else(|| {
        state
            .identity
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|id| id.discord_snowflake.clone()))
    });
    reset_account_scoped_state(state);
    run_autostart(state);
    let loaded = state
        .identity
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|id| id.discord_snowflake.clone()));
    if expected.is_some() && loaded != expected {
        return Err(format!(
            "OSL: account reload mismatch: expected {:?}, loaded {:?}",
            expected, loaded
        ));
    }
    Ok(())
}

/// One-time migration: an existing single-account install has its files
/// flat at the base. Copy them (NON-destructively — the flat originals
/// remain as a backup) into `accounts/<bound_snowflake>/` and write the
/// active marker, bringing that account into the multi-account system.
fn migrate_flat_account_if_needed(bound_snowflake: &str) -> Result<(), String> {
    let base = keystore::osl_base_dir()
        .map_err(|e| format!("OSL: flat-account migration: base dir: {e}"))?;
    // Already migrated (a marker exists) → nothing to do.
    if read_active_marker().is_some() {
        return Ok(());
    }
    let adir = keystore::account_dir(bound_snowflake)
        .map_err(|e| format!("OSL: flat-account migration: account dir: {e}"))?;
    if adir.join("identity.json").exists() {
        // Account dir already populated; just adopt it.
        validate_account_identity(&adir, bound_snowflake)?;
        write_active_marker(bound_snowflake)?;
        keystore::set_active_account_dir(Some(adir));
        return Ok(());
    }

    if adir.exists()
        && std::fs::read_dir(&adir)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "OSL: flat-account migration: {} is partially populated; refusing to overwrite it",
            adir.display()
        ));
    }
    if adir.exists() {
        std::fs::remove_dir(&adir)
            .map_err(|e| format!("OSL: flat-account migration: remove empty target: {e}"))?;
    }
    let parent = adir
        .parent()
        .ok_or_else(|| "OSL: flat-account migration: account dir has no parent".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("OSL: flat-account migration: create accounts dir: {e}"))?;
    let stage = parent.join(format!(
        ".{bound_snowflake}.migrate-{}.tmp",
        std::process::id()
    ));
    if stage.exists() {
        std::fs::remove_dir_all(&stage)
            .map_err(|e| format!("OSL: flat-account migration: clear stale stage: {e}"))?;
    }

    let copy_result = (|| -> Result<(), String> {
        std::fs::create_dir_all(stage.join("store")).map_err(|e| format!("create stage: {e}"))?;
        for rel in ACCOUNT_STATE_FILES {
            let src = base.join(rel);
            if src.exists() {
                std::fs::copy(&src, stage.join(rel)).map_err(|e| format!("copy {rel}: {e}"))?;
            }
        }
        for db in [
            "messages.sqlite",
            "messages.sqlite-wal",
            "messages.sqlite-shm",
        ] {
            let src = base.join("store").join(db);
            if src.exists() {
                std::fs::copy(&src, stage.join("store").join(db))
                    .map_err(|e| format!("copy store/{db}: {e}"))?;
            }
        }
        validate_account_identity(&stage, bound_snowflake)?;
        std::fs::rename(&stage, &adir).map_err(|e| format!("install stage: {e}"))?;
        Ok(())
    })();
    if let Err(e) = copy_result {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(format!("OSL: flat-account migration failed: {e}"));
    }
    write_active_marker(bound_snowflake)?;
    keystore::set_active_account_dir(Some(adir));
    tracing::info!(
        snowflake = %bound_snowflake,
        "OSL multi-account: migrated flat install into per-account dir (originals kept as backup)"
    );
    Ok(())
}

fn validate_account_identity(dir: &Path, snowflake: &str) -> Result<(), String> {
    let path = dir.join("identity.json");
    let sealer = keystore::select_best_sealer();
    let identity = load_identity(&path, sealer.as_ref())
        .map_err(|e| format!("identity {} is unreadable: {e}", path.display()))?;
    let bound = identity
        .discord_snowflake
        .as_deref()
        .unwrap_or(identity.user_id.as_str());
    if bound != snowflake {
        return Err(format!(
            "identity {} is bound to {bound}, not {snowflake}",
            path.display()
        ));
    }
    Ok(())
}

/// Instant account switch (#2). Idempotent when `snowflake` is already
/// active. Otherwise: reset account-scoped state, point the config dir
/// at `accounts/<snowflake>/` (generating a fresh seed-phrase identity
/// there if it's a brand-new account), and re-run the load sequence so
/// AppState reflects that account. The previous account's files stay on
/// disk in its own dir, ready to switch back to.
pub fn switch_account(state: &AppState, snowflake: String) -> Result<bool, String> {
    if !snowflake.chars().all(|c| c.is_ascii_digit()) || !(17..=20).contains(&snowflake.len()) {
        return Err(format!(
            "OSL: switch_account: invalid snowflake '{snowflake}'"
        ));
    }
    let _switch_guard = state
        .account_switch_lock
        .lock()
        .map_err(|_| "OSL: switch_account: switch lock poisoned".to_string())?;

    // What account is active right now?
    let current = read_active_marker().or_else(|| {
        state
            .identity
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|i| i.discord_snowflake.clone()))
    });
    eprintln!(
        "[OSL][multi-account] switch_account(req={snowflake}) current={:?} \
         active_dir={:?}",
        current,
        keystore::active_account_dir().map(|d| d.display().to_string())
    );

    // First-run migration: bring the existing flat account into the
    // system under its own snowflake before doing anything else.
    let mut migrated_flat_account = false;
    if read_active_marker().is_none() {
        if let Some(cur) = current.clone() {
            // Close SQLite before copying its main/WAL files. Copying a live
            // WAL database file-by-file can produce an internally inconsistent
            // account snapshot. Dropping the last connection checkpoints it.
            let old_store = state
                .message_store
                .lock()
                .map_err(|_| "OSL: switch_account: message_store lock poisoned".to_string())?
                .take();
            drop(old_store);
            if let Err(e) = migrate_flat_account_if_needed(&cur) {
                // Migration does not flip the marker/override on failure. Reopen
                // the still-active flat store before returning the error.
                if let Ok(base) = keystore::osl_base_dir() {
                    open_message_store(state, &base);
                }
                return Err(e);
            }
            migrated_flat_account = true;
        }
    }

    if current.as_deref() == Some(snowflake.as_str()) {
        // Already this account — just make sure the override points at
        // its dir (post-migration) and we're registered.
        let adir = keystore::account_dir(&snowflake)
            .map_err(|e| format!("OSL: switch_account: account_dir: {e}"))?;
        validate_account_identity(&adir, &snowflake)?;
        write_active_marker(&snowflake)?;
        keystore::set_active_account_dir(Some(adir));
        if migrated_flat_account {
            // The account did not change, but every state-file consumer and
            // the SQLite handle must move from the legacy flat directory to
            // the new per-account directory immediately.
            reset_account_scoped_state(state);
            run_autostart(state);
            let reloaded = state
                .identity
                .lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|id| id.discord_snowflake.clone()))
                .as_deref()
                == Some(snowflake.as_str());
            if !reloaded {
                return Err(format!(
                    "OSL: switch_account: migrated account {snowflake} failed to reload"
                ));
            }
        }
        return Ok(false);
    }

    tracing::info!(
        from = ?current,
        to = %snowflake,
        "OSL multi-account: switching account"
    );

    // Prepare and validate the target completely before touching the active
    // marker, process-global directory, or current account's AppState.
    let adir = keystore::account_dir(&snowflake)
        .map_err(|e| format!("OSL: switch_account: account_dir: {e}"))?;
    std::fs::create_dir_all(&adir)
        .map_err(|e| format!("OSL: switch_account: create account dir: {e}"))?;

    // The Discord web origin may request a switch only to an account already
    // provisioned on this device. It is not a trusted authority for creating
    // and registering a cryptographic identity for an arbitrary snowflake.
    // New-account creation belongs in an explicit trusted-local confirmation
    // flow (Settings/onboarding).
    if !adir.join("identity.json").exists() {
        return Err(format!(
            "OSL: account {snowflake} is not provisioned on this device. Open OSL Settings to add it; no identity was created."
        ));
    }
    validate_account_identity(&adir, &snowflake)?;

    let previous_marker = current.clone();
    let previous_dir = keystore::active_account_dir();
    write_active_marker(&snowflake)?;
    keystore::set_active_account_dir(Some(adir.clone()));
    reset_account_scoped_state(state);

    // Re-run the full, tested load sequence against the new active dir.
    run_autostart(state);
    let loaded_target = state
        .identity
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|id| id.discord_snowflake.clone()))
        .as_deref()
        == Some(snowflake.as_str());
    if loaded_target {
        return Ok(true);
    }

    // The target was valid on disk but failed to load into AppState. Restore
    // the previous durable/process pointers and reload it before reporting
    // failure, so an Err never leaves the client half-switched.
    let marker_error = if let Some(rollback_marker) = previous_marker.as_deref() {
        write_active_marker(rollback_marker).err()
    } else if let Some(path) = active_marker_path() {
        std::fs::remove_file(&path)
            .err()
            .map(|e| format!("OSL: switch_account: clear failed target marker: {e}"))
    } else {
        Some("OSL: switch_account: no prior marker and marker path unavailable".to_string())
    };
    keystore::set_active_account_dir(previous_dir);
    reset_account_scoped_state(state);
    run_autostart(state);
    let restored = previous_marker.as_deref().unwrap_or("legacy base account");
    if let Some(marker_error) = marker_error {
        Err(format!(
            "OSL: switch_account: target {snowflake} failed to load; restored {restored} in memory, but durable marker rollback failed: {marker_error}"
        ))
    } else {
        Err(format!(
            "OSL: switch_account: target {snowflake} failed to load; restored {restored}"
        ))
    }
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
            // Self-heal. The store is sealed by
            // identity.x25519_secret and this fn is only called with
            // an identity loaded (caller gates on `identity_loaded`),
            // so a failed open is the WRONG-IDENTITY case — e.g. a
            // non-burn identity regen (see finding 3b) left a store/
            // DB sealed by the old secret. There is NO "password not
            // entered yet" ambiguity for the store: it has no
            // password-gate dependency, so quarantine here cannot
            // race a transient not-unlocked state. Rename the stale
            // dir aside (reversible — the old DB is only decryptable
            // by the old identity anyway) and recreate a fresh store
            // under the current identity so persistence comes back
            // instead of staying permanently disabled.
            tracing::warn!(
                error = %e,
                path = %store_dir.display(),
                "OSL bootstrap: message_store open failed (wrong identity); \
                 quarantining stale store and recreating"
            );
            match quarantine_aside(&store_dir) {
                Ok(q) => {
                    tracing::warn!(
                        quarantined_to = %q.display(),
                        "OSL bootstrap: stale store quarantined (rename, not delete)"
                    );
                    match MessageStore::open(&store_dir, &secret_bytes) {
                        Ok(store) => {
                            tracing::info!(
                                path = %store_dir.display(),
                                "OSL bootstrap: message_store recreated under current identity"
                            );
                            *state
                                .message_store
                                .lock()
                                .expect("message_store mutex poisoned") = Some(store);
                        }
                        Err(e2) => {
                            tracing::warn!(
                                error = %e2,
                                path = %store_dir.display(),
                                "OSL bootstrap: message_store still failed after \
                                 quarantine; persistence disabled this session"
                            );
                        }
                    }
                }
                Err(qe) => {
                    tracing::warn!(
                        error = %qe,
                        path = %store_dir.display(),
                        "OSL bootstrap: could not quarantine stale store; \
                         persistence disabled this session"
                    );
                }
            }
        }
    }
}

/// Self-heal primitive: rename `path` aside to
/// `<name>.quarantine-<unix-secs>` so the normal writer can create a
/// fresh one. NON-destructive (rename, never delete) and reversible
/// — the caller is reconciling state sealed by a key the current
/// identity/password can no longer produce; the user can still
/// recover the quarantined blob out-of-band if they recover the old
/// key. Works for both files and directories.
fn quarantine_aside(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let fname = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "osl-state".to_string());
    let qpath = path.with_file_name(format!("{fname}.quarantine-{ts}"));
    std::fs::rename(path, &qpath)?;
    Ok(qpath)
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

/// 7d-PIVOT: read `<dir>/whitelist_state.json` into
/// `AppState::whitelist_state`, then apply the PIVOT migration:
/// any scope that has at least one peer entry (members or
/// whitelisted_users) and an `encrypt_toggle == false` gets bumped
/// to `encrypt_toggle = true`. This preserves the pre-PIVOT
/// "implicit encrypt-when-whitelisted" behaviour for existing
/// users on first launch after the upgrade.
///
/// All failures are non-fatal — missing file is the normal
/// fresh-install case, parse errors leave the map empty (every
/// scope falls back to encrypt_toggle = false). Migration only
/// fires for scopes already in the loaded state; brand-new scopes
/// start with encrypt_toggle = false.
fn load_and_migrate_whitelist_state(state: &AppState, dir: &std::path::Path) {
    let path = dir.join("whitelist_state.json");
    match ipc::migration::migrate_whitelist_state_in_place(state, dir) {
        Ok(None) => {
            tracing::info!(
                path = %path.display(),
                "OSL bootstrap: whitelist_state.json not found (fresh install)"
            );
        }
        Ok(Some(report)) => {
            if report.was_already_migrated {
                tracing::info!(
                    path = %path.display(),
                    entries = report.scope_entries_loaded,
                    "OSL bootstrap: whitelist_state loaded (already migrated)"
                );
            } else {
                tracing::info!(
                    path = %path.display(),
                    entries = report.scope_entries_loaded,
                    migrated_scope_entries = report.legacy_scope_entries_migrated,
                    migrated_peer_links = report.peer_links_added,
                    "OSL bootstrap: migrated N whitelist entries from whitelist_state to peer_map (C1)"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "OSL bootstrap: whitelist_state migration failed; skipping"
            );
        }
    }
}

/// W2: read `<dir>/membership.json` into `AppState::scope_membership`.
/// Missing file is the fresh-install common case (non-fatal — the
/// store starts empty and re-accrues from gateway events).
fn load_scope_membership(state: &AppState, dir: &std::path::Path) {
    let path = dir.join("membership.json");
    match ipc::membership::load_scope_membership_from_path(&path) {
        Ok(m) => {
            *state
                .scope_membership
                .lock()
                .expect("scope_membership mutex poisoned") = m;
            tracing::info!(
                path = %path.display(),
                "OSL bootstrap: scope-membership loaded"
            );
        }
        Err(ipc::membership::ScopeMembershipError::NotFound(_)) => {
            tracing::info!(
                path = %path.display(),
                "OSL bootstrap: membership.json not found (fresh install)"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "OSL bootstrap: membership.json could not be loaded; \
                 server/channel recipient resolution starts empty and \
                 re-accrues from gateway events"
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
                "OSL bootstrap: no keyserver.json; using built-in production \
                 keyserver default (the file is an OVERRIDE only — populate \
                 {{\"base_url\":\"http://host:port\",\"user_id\":\"name\"}} for \
                 dev/staging)"
            );
            return None;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %path.display(),
                "OSL bootstrap: keyserver.json read failed; falling back to \
                 the built-in production keyserver default"
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
                "OSL bootstrap: keyserver.json malformed; falling back to the \
                 built-in production keyserver default"
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
/// Returns `(identity_loaded, regenerated)`. `regenerated` is true
/// only when this call generated a brand-new identity (non-burn
/// regen path) — the caller uses it to drop now-orphaned per-peer
/// ratchet_state (finding 3b companion).
fn load_or_generate_identity(
    state: &AppState,
    dir: &std::path::Path,
    keyserver_cfg: Option<&KeyserverConfig>,
) -> (bool, bool) {
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
                return (true, false);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "OSL bootstrap: identity load failed; continuing without identity"
                );
                return (false, false);
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
            return (false, false);
        }
    };

    // Finding 3b: this regenerates an identity WITHOUT calling
    // burn_wipe_all, so a surviving store/ DB (sealed by the old
    // x25519_secret) is now orphaned. That is deliberately left to
    // open_message_store's quarantine self-heal to reconcile — do
    // NOT "fix" this by adding a wipe here without raising it as a
    // separate proposal first.
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
    (true, true)
}

/// Init the keyserver client and call `register`.
///
/// REGISTER-FIX: this is now a thin delegate to
/// [`ipc::commands::ensure_keyserver_registered`] — the single
/// shared implementation that both this boot-time path and the
/// runtime post-unlock / post-snowflake paths go through, so they
/// cannot drift. Behaviour for the boot case is unchanged: at cold
/// boot `state.keyserver` is empty and (because the caller already
/// gated on `identity_loaded`) `state.identity` is populated, so the
/// shared helper builds the client, attempts `register`, and installs
/// the client — exactly as before. Both steps log and continue on
/// failure; a `register` failure still leaves the client installed so
/// subsequent `fetch_pubkeys` calls can succeed.
fn init_keyserver_and_register(state: &AppState, base_url: &str, client_token: Option<String>) {
    ipc::commands::ensure_keyserver_registered(state, base_url, client_token);
}

/// Convenience: ensure the parent directory of `path` exists
/// (recursive). No-op if it already does.
fn ensure_parent_exists(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod multi_account_marker_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-bootstrap-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn marker_replacement_leaves_only_a_complete_value() {
        let dir = temp_dir("replace");
        let path = dir.join("active");
        write_active_marker_at(&path, "111111111111111111").expect("first marker");
        write_active_marker_at(&path, "222222222222222222").expect("replacement marker");

        assert_eq!(
            read_active_marker_at(&path).as_deref(),
            Some("222222222222222222")
        );
        assert!(!path.with_extension("tmp").exists());
        assert!(!path.with_extension("bak").exists());
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn incomplete_temp_is_never_accepted_and_complete_backup_recovers() {
        let dir = temp_dir("recovery");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("active");
        let backup = path.with_extension("bak");
        let tmp = path.with_extension("tmp");
        std::fs::write(&backup, "333333333333333333").expect("backup");
        std::fs::write(&tmp, "4444").expect("partial temp");

        assert_eq!(
            read_active_marker_at(&path).as_deref(),
            Some("333333333333333333")
        );
        assert!(path.exists(), "backup should be restored to active");
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn malformed_marker_is_rejected() {
        let dir = temp_dir("malformed");
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("active");
        std::fs::write(&path, "not-a-snowflake").expect("write malformed");
        assert_eq!(read_active_marker_at(&path), None);
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
