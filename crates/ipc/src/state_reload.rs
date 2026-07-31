//! Phase 9-D-FIX2: post-gate encrypted-at-rest state reload.
//!
//! Bootstrap's encrypted-state loaders fire before the user enters
//! their password — `file_storage_key` is `None` at that point, so
//! every `OSL-ENC1` file's `maybe_decrypt` call errors and the
//! individual loaders silently return `Default`. Once the gate
//! successfully verifies the main password and installs the key,
//! `AppState` still holds those defaults; without an explicit
//! reload, the user's whitelist, burns, tour state, sender chains,
//! etc. stay blank for the entire session and the next launch
//! repeats the cycle.
//!
//! [`reload_encrypted_state_after_unlock`] re-runs the same loaders
//! bootstrap used (so file-format / migration semantics stay
//! identical), then replaces each `AppState` mutex slot with the
//! disk-read value. Per-file failures are non-fatal and surface via
//! `ReloadReport.errors`.

use std::path::Path;

use crate::peer_map::{load_peer_map_from_path, PeerMapError};
use crate::AppState;

/// Per-file outcome of a post-gate state reload. Bool flags
/// indicate "load was attempted and succeeded for a file that
/// existed on disk" — a `false` flag plus an empty `errors` vec
/// means "file did not exist," which is the normal fresh-install
/// case for everything except `peer_map`.
#[derive(Debug, Default, Clone)]
pub struct ReloadReport {
    pub peer_map_loaded: bool,
    pub peer_map_entries: usize,
    pub whitelist_loaded: bool,
    pub whitelist_scopes: usize,
    pub server_defaults_loaded: bool,
    pub server_defaults_entries: usize,
    pub burned_scopes_loaded: bool,
    pub burned_scopes_count: usize,
    pub prekeys_loaded: bool,
    pub prekey_opks: usize,
    pub sender_keys_loaded: bool,
    pub sender_keys_count: usize,
    pub app_prefs_loaded: bool,
    pub scope_membership_loaded: bool,
    pub scope_membership_observations: usize,
    pub errors: Vec<String>,
    /// 9-PEER-MAP-ENC: tracks the retroactive re-encryption sweep.
    /// `true` if a plaintext-on-disk file was rewritten as OSL-ENC1
    /// using the just-installed file_storage_key. One-shot — once
    /// every file has the magic, this stays `false` on subsequent
    /// reloads.
    pub peer_map_reencrypted: bool,
    pub scope_membership_reencrypted: bool,
    pub self_entry_repaired_post_gate: bool,
}

/// Re-read every encrypted-at-rest state file from `config_dir`
/// and replace `AppState`'s in-memory copy. Call this immediately
/// after `set_file_storage_key(Some(...))` on a successful gate
/// verify so the user's session sees their actual saved state
/// instead of the empty defaults bootstrap seeded.
///
/// Each file is independent: a parse / decrypt error on one
/// (recorded in `report.errors`) doesn't prevent the others from
/// reloading. The returned `Result` is currently always `Ok` —
/// the signature reserves room for a future catastrophic-failure
/// path (e.g. config dir suddenly inaccessible) without changing
/// callers.
pub fn reload_encrypted_state_after_unlock(
    state: &AppState,
    config_dir: &Path,
) -> Result<ReloadReport, String> {
    let mut report = ReloadReport::default();

    // Commit 3 — wrong-key quarantine for the file_storage_key JSON
    // files. This runs POST-GATE: the caller installs
    // file_storage_key before calling us, so a key IS in the slot
    // here. If a sealed file still won't decrypt with that key it
    // was sealed under a DIFFERENT key (e.g. the pre-burn / pre-
    // password-change file_storage_key, or a non-burn identity regen
    // that orphaned it) and would otherwise make the loader below
    // silently fall back to Default forever. Quarantine it aside
    // (rename, never delete — same non-destructive contract as
    // bootstrap's store self-heal) so the loader recreates a fresh
    // one under the current key. The pre-key path (no key in slot)
    // is left UNTOUCHED — that is bootstrap's expected pre-gate
    // state, not a wrong-key failure.
    for name in [
        "peer_map.json",
        "whitelist_state.json",
        "app_preferences.json",
        "sender_key_state.json",
        // Probe-2 Rust Bug 5: these were sealed under file_storage_key
        // too but were missing from the quarantine sweep. A stale-key
        // blob would silently fall back to default — the burn kill-
        // list and accrued scope-membership would disappear from view
        // (file intact but unreachable) until the next persist
        // overwrote it. Mirror the same non-destructive rename here.
        "burned_scopes.json",
        "membership.json",
    ] {
        let path = config_dir.join(name);
        match quarantine_if_wrong_key(&path) {
            Ok(Some(q)) => {
                tracing::warn!(
                    file = name,
                    quarantined_to = %q.display(),
                    "OSL: state_reload — {name} sealed by a different key; \
                     quarantined (rename, not delete), recreating under \
                     current key"
                );
                // A successful quarantine must be SURFACED, not just logged.
                // Quarantining means a sealed file would not decrypt under the
                // installed key, so the loader is about to fall back to
                // Default. For peer_map/whitelist/burned_scopes/membership
                // that Default IS an empty security policy: unlocking would
                // silently proceed with no peers, no whitelist and an empty
                // burn kill-list. The gate treats a reload error as fatal and
                // keeps the session locked, which is the correct outcome —
                // previously only a failed RENAME was reported, so the
                // ordinary success path lost the file quietly.
                report.errors.push(format!(
                    "{name}: sealed by a different key; quarantined to {}",
                    q.display()
                ));
            }
            Ok(None) => {}
            Err(e) => report.errors.push(format!("{name} quarantine: {e}")),
        }
    }

    // app_preferences is device-level in the multi-account layout, so
    // the generic per-account sweep above only covers the legacy path.
    // Quarantine the actual base-dir file as well when those paths differ.
    let legacy_prefs_path = config_dir.join("app_preferences.json");
    let device_prefs_path = keystore::osl_base_dir()
        .unwrap_or_else(|_| config_dir.to_path_buf())
        .join("app_preferences.json");
    if device_prefs_path != legacy_prefs_path {
        match quarantine_if_wrong_key(&device_prefs_path) {
            Ok(Some(q)) => tracing::warn!(
                quarantined_to = %q.display(),
                "OSL: device app_preferences sealed by a different key; \
                 quarantined (rename, not delete)"
            ),
            Ok(None) => {}
            Err(e) => report
                .errors
                .push(format!("device app_preferences quarantine: {e}")),
        }
    }

    // peer_map.json — explicit error variants distinguish missing
    // (fresh install, normal) from decrypt/parse failure.
    let pm_path = config_dir.join("peer_map.json");
    // 9-PEER-MAP-ENC: sniff the envelope BEFORE the loader runs.
    // `load_peer_map_from_path` now performs the plaintext → OSL-ENC1
    // migration itself whenever a storage key is in slot, so the
    // post-load sniff further down can no longer observe that a
    // migration happened — by then the magic is already there. Record
    // the pre-load state so the report still flags the one-shot
    // re-encryption regardless of which layer actually did the write.
    let peer_map_was_plaintext = std::fs::read(&pm_path)
        .map(|blob| !crate::main_password::has_enc_magic(&blob))
        .unwrap_or(false);
    match load_peer_map_from_path(&pm_path) {
        Ok(map) => {
            report.peer_map_entries = map.len();
            report.peer_map_loaded = true;
            *state.peer_map.lock().expect("peer_map mutex poisoned") = map;
        }
        Err(PeerMapError::NotFound { .. }) => {}
        Err(e) => report.errors.push(format!("peer_map: {e}")),
    }

    // D: close the bootstrap-regen + encrypted-peer_map residual.
    // If THIS launch regenerated the local identity outside a burn,
    // the bootstrap in-memory ratchet clear could not persist (the
    // file_storage_key wasn't installed pre-gate, so write_peer_map
    // refused to clobber the encrypted file) — and the reload above
    // just pulled the stale on-disk ratchet_state back into memory.
    // Now we ARE post-gate (the key is installed before this fn is
    // called), so re-run the clear: it persists durably this time.
    // Gated SOLELY on regenerated-this-launch so a normal unlock
    // never clears anything; consume the flag so a second
    // same-session reload doesn't repeat it (the helper is already
    // idempotent, this is just hygiene).
    if state
        .identity_regenerated_this_launch
        .swap(false, std::sync::atomic::Ordering::SeqCst)
    {
        crate::commands::clear_all_peer_ratchet_state(state);
    }

    // whitelist_state.json — `migrate_whitelist_state_in_place`
    // owns the load path (it also touches peer_map for the C1
    // projection). Idempotent after first migration: a file with
    // `migrated_c1 = true` loads scopes + server_defaults straight
    // into state with no peer_map writes.
    match crate::migration::migrate_whitelist_state_in_place(state, config_dir) {
        Ok(None) => {}
        Ok(Some(r)) => {
            report.whitelist_loaded = true;
            report.whitelist_scopes = r.scope_entries_loaded;
            report.server_defaults_loaded = true;
            report.server_defaults_entries = state
                .server_defaults
                .lock()
                .expect("server_defaults mutex poisoned")
                .len();
        }
        Err(e) => report.errors.push(format!("whitelist_state: {e}")),
    }

    // burned_scopes.json — the loader is infallible by signature
    // (returns default on any failure), but missing files are the
    // fresh-install case, not an error. Use file existence as a
    // proxy for "should have data."
    let bs_path = config_dir.join("burned_scopes.json");
    if bs_path.exists() {
        let bs = crate::burned_scopes_file::load_burned_scopes(&bs_path);
        report.burned_scopes_count = bs.scopes.len();
        report.burned_scopes_loaded = true;
        *state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned") = bs;
    }

    match load_persisted_prekey_state(state, config_dir) {
        Ok(false) => {}
        Ok(true) => {
            report.prekeys_loaded = true;
            report.prekey_opks = state
                .prekey_state
                .lock()
                .expect("prekey_state mutex poisoned")
                .as_ref()
                .map(|prekeys| prekeys.opk_pool.len())
                .unwrap_or(0);
        }
        Err(e) => report.errors.push(format!("prekeys: {e}")),
    }

    // sender_key_state.json — same shape as burned_scopes (loader
    // returns default on failure). Bootstrap pre-FIX2 never loaded
    // this file at all; the reload path is the first time the
    // on-disk sender chains actually populate AppState.
    let sk_path = config_dir.join("sender_key_state.json");
    if sk_path.exists() {
        let sk = crate::sender_key_state::load_sender_key_state(&sk_path);
        report.sender_keys_count = sk.states.len();
        report.sender_keys_loaded = true;
        *state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned") = sk;
    }

    // membership.json — dynamic recipient observations. Same encrypted-at-rest
    // family as peer_map and sender keys; reload it after unlock so the
    // pre-gate default does not survive for the whole session.
    let membership_path = config_dir.join("membership.json");
    match crate::membership::load_scope_membership_from_path(&membership_path) {
        Ok(membership) => {
            report.scope_membership_observations = membership.observed_member_count();
            report.scope_membership_loaded = true;
            *state
                .scope_membership
                .lock()
                .expect("scope_membership mutex poisoned") = membership;
        }
        Err(crate::membership::ScopeMembershipError::NotFound(_)) => {}
        Err(e) => report.errors.push(format!("membership: {e}")),
    }

    // app_preferences.json — holds tour resume state, stego mode,
    // and the VPN-warning dismissal. DEVICE-level: lives at the base
    // dir (multi-account), NOT the per-account `config_dir`, matching
    // run_autostart + persist_app_preferences_now.
    // Prefer the current device-level location. Fall back to the legacy
    // per-account path so upgrades do not silently reset tour/update
    // preferences before the next settings write migrates them.
    let prefs_path = if device_prefs_path.exists() {
        device_prefs_path
    } else {
        legacy_prefs_path
    };
    if prefs_path.exists() {
        let prefs = crate::app_preferences::load_app_preferences(&prefs_path);
        report.app_prefs_loaded = true;
        *state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned") = prefs;
    }

    // 9-PEER-MAP-ENC: retroactive re-encryption of plaintext peer_map.
    // Users on pre-fix builds had bootstrap clobber their encrypted
    // peer_map.json with a plaintext stub on every launch. Now that
    // the password gate has run and file_storage_key is installed,
    // sniff the on-disk file: if no OSL-ENC1 magic, rewrite via
    // write_peer_map which will encrypt-on-write. One-shot; subsequent
    // reloads see the magic and skip.
    if pm_path.exists() {
        if let Ok(blob) = std::fs::read(&pm_path) {
            if !crate::main_password::has_enc_magic(&blob) {
                let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
                match crate::peer_map::write_peer_map(&pm_path, &pm) {
                    Ok(()) => {
                        report.peer_map_reencrypted = true;
                        tracing::info!("OSL: retroactively re-encrypted plaintext peer_map.json");
                    }
                    Err(e) => report.errors.push(format!("peer_map re-encrypt: {e}")),
                }
            } else if peer_map_was_plaintext {
                // The loader's own migration already sealed the file
                // during this reload. Still a one-shot re-encryption of
                // a plaintext peer_map, so report it as one.
                report.peer_map_reencrypted = true;
            }
        }
    }

    // Same retroactive sweep for membership.json. A plaintext membership file is
    // tolerated on read for migration, but once unlock supplies a file key, the
    // next write must restore the OSL-ENC1 envelope instead of leaving accrued
    // observations readable on disk.
    if membership_path.exists() {
        if let Ok(blob) = std::fs::read(&membership_path) {
            if !crate::main_password::has_enc_magic(&blob) {
                let membership = state
                    .scope_membership
                    .lock()
                    .expect("scope_membership mutex poisoned");
                match crate::membership::write_scope_membership(&membership_path, &membership) {
                    Ok(()) => {
                        report.scope_membership_reencrypted = true;
                        tracing::info!("OSL: retroactively re-encrypted plaintext membership.json");
                    }
                    Err(e) => report.errors.push(format!("membership re-encrypt: {e}")),
                }
            }
        }
    }

    // Re-run the self-entry verify now that the disk-load reflects
    // real state (key in slot). Bootstrap's pre-gate verify either
    // ran against an empty in-memory map (encrypted source unreadable)
    // and was refused at the persist step by write_peer_map's guard,
    // OR it ran against a stale state. Either way, with the real map
    // now loaded, a fresh verify is the correct repair point. Persist
    // succeeds here because the key is installed.
    match crate::commands::verify_and_persist_peer_map_self_entry(state) {
        Ok((_, repaired)) => {
            if repaired {
                report.self_entry_repaired_post_gate = true;
                tracing::info!(
                    "OSL: peer_map self-entry repaired post-gate \
                     (would have been refused pre-gate)"
                );
            }
        }
        Err(reason) if reason == "no_discord_snowflake" || reason == "identity_not_loaded" => {
            // Both are expected during early bootstrap; boot.js will
            // retry via cmd_osl_register_self_snowflake once the
            // Discord runtime exposes the snowflake.
        }
        Err(other) => report.errors.push(format!("self_entry_repair: {other}")),
    }

    Ok(report)
}

/// Load `<config_dir>/prekeys.json` into [`AppState`] when it exists.
///
/// A prekey file is only authority if it is sealed under the active sealer and
/// its signed prekey is bound to the loaded identity. A missing file is the
/// fresh-install case and returns `Ok(false)`. A present but unreadable or
/// unbound file clears the live prekey slot and returns `Err`, refusing to keep
/// a synthetic or stale pool in memory.
pub fn load_persisted_prekey_state(state: &AppState, config_dir: &Path) -> Result<bool, String> {
    let sealer = keystore::select_best_sealer();
    load_persisted_prekey_state_with_sealer(state, config_dir, sealer.as_ref())
}

pub fn load_persisted_prekey_state_with_sealer(
    state: &AppState,
    config_dir: &Path,
    sealer: &dyn keystore::Sealer,
) -> Result<bool, String> {
    let path = config_dir.join("prekeys.json");
    if !path.exists() {
        return Ok(false);
    }

    let loaded = (|| {
        let identity = state
            .identity
            .lock()
            .map_err(|_| "identity mutex poisoned".to_string())?
            .clone()
            .ok_or_else(|| "identity is not loaded".to_string())?;
        let prekeys = keystore::load_prekey_state(&path, sealer)
            .map_err(|_| "local prekey state is unreadable".to_string())?;
        validate_prekey_state_for_identity(&identity, &prekeys)?;
        Ok(prekeys)
    })();

    match loaded {
        Ok(prekeys) => {
            state.set_prekey_state(prekeys);
            Ok(true)
        }
        Err(e) => {
            state.clear_prekey_state();
            Err(e)
        }
    }
}

pub fn validate_prekey_state_for_identity(
    identity: &keystore::Identity,
    prekeys: &keystore::PrekeyState,
) -> Result<(), String> {
    validate_spk_for_identity(identity, &prekeys.current_spk, "current")?;
    if let Some(previous) = &prekeys.previous_spk {
        validate_spk_for_identity(identity, previous, "previous")?;
    }
    for opk in &prekeys.opk_pool {
        let secret = crypto::x25519::SecretKey::from_bytes(opk.secret);
        if crypto::x25519::derive_public(&secret).as_bytes() != &opk.public {
            return Err("one-time prekey public key does not match its secret".to_string());
        }
    }
    Ok(())
}

fn validate_spk_for_identity(
    identity: &keystore::Identity,
    spk: &keystore::SpkEntry,
    label: &str,
) -> Result<(), String> {
    let secret = crypto::x25519::SecretKey::from_bytes(spk.secret);
    if crypto::x25519::derive_public(&secret).as_bytes() != &spk.public {
        return Err(format!(
            "{label} signed prekey public key does not match its secret"
        ));
    }
    let signature = crypto::ed25519::Signature::from_bytes(spk.signature);
    let verified = crypto::ed25519::verify(&identity.ed25519_public, &spk.public, &signature)
        .map_err(|_| format!("{label} signed prekey signature is malformed"))?;
    if !verified {
        return Err(format!(
            "{label} signed prekey is not bound to the loaded identity"
        ));
    }
    Ok(())
}

/// Wrong-key discrimination for one file_storage_key-sealed JSON
/// file. Returns `Ok(Some(quarantine_path))` if the file was sealed
/// under a key the currently-installed `file_storage_key` can't
/// reproduce and was renamed aside; `Ok(None)` if there is nothing
/// to do (missing / plaintext / decrypts fine / no key in slot —
/// the expected pre-gate path); `Err` only on a rename I/O failure.
fn quarantine_if_wrong_key(path: &Path) -> Result<Option<std::path::PathBuf>, String> {
    let blob = match std::fs::read(path) {
        Ok(b) => b,
        // Missing/unreadable: not our concern — the loader handles
        // absence as the normal fresh-install case.
        Err(_) => return Ok(None),
    };
    // Plaintext (no OSL-ENC1 magic) decrypts fine by definition.
    if !crate::main_password::has_enc_magic(&blob) {
        return Ok(None);
    }
    // No key in the slot ⇒ expected pre-gate path. Leave UNTOUCHED;
    // never quarantine on the not-unlocked case.
    let key = match crate::main_password::get_file_storage_key() {
        Some(k) => k,
        None => return Ok(None),
    };
    // Key installed AND the blob still won't decrypt ⇒ sealed under
    // a different key. Quarantine.
    if crate::main_password::decrypt_at_rest(&blob, &key).is_ok() {
        return Ok(None);
    }
    quarantine_aside(path).map(Some).map_err(|e| e.to_string())
}

/// Self-heal primitive (mirrors bootstrap's `quarantine_aside`):
/// rename `path` aside to `<name>.quarantine-<unix-secs>` so the
/// normal writer can create a fresh one. NON-destructive (rename,
/// never delete) and reversible — the user can still recover the
/// blob out-of-band if they recover the old key.
fn quarantine_aside(path: &Path) -> std::io::Result<std::path::PathBuf> {
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
