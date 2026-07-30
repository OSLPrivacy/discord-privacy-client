//! Pure functions backing the Tauri command surface.
//!
//! These take an explicit [`AppState`] reference plus primitive
//! arguments, return [`IpcResult`], and contain no Tauri-specific
//! glue. Unit tests exercise them directly.
//!
//! Tauri-attribute wrappers live in [`crate::tauri_glue`].

use crate::state::AppState;
use crate::{IpcError, IpcResult};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{aead, ed25519, hkdf, random, x25519};
use keystore::{generate_identity, select_best_sealer, KeyServerClient};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use store::{MessageStore, StoreError, StoredMessage};

// 9-TD2.3: F0-FIX3 trace logs.
//
// Set the `OSL_TRACE` env var (any value) to surface the snowflake
// registration / identity-generation breadcrumbs on stderr. Default
// off — these were always-on during F0-FIX3 diagnosis and shipped
// that way. PowerShell:
//   $env:OSL_TRACE = "1"; & "C:\Program Files\OSL\osl.exe"
// bash:
//   OSL_TRACE=1 ./osl
macro_rules! osl_trace {
    ($($arg:tt)*) => {
        if std::env::var_os("OSL_TRACE").is_some() {
            eprintln!($($arg)*);
        }
    };
}

// =====================================================================
// 7d-FIX1: persistence write-through helpers.
//
// Pre-FIX1 root cause: mutating commands (set_whitelist,
// unwhitelist_scope, apply_invitation_decision, toggle_scope_encryption,
// etc.) updated `AppState` in memory but NEVER wrote back to disk.
// peer_map.json / whitelist_state.json on disk only ever contained
// what the user hand-edited (or what bootstrap loaded at startup).
// This blocked encryption-at-rest from ever firing: with no write
// path, the `maybe_encrypt` retrofit in the write functions was
// never exercised.
//
// The helpers are best-effort: on disk-write failure we log and
// continue. The in-memory mutation already happened; surfacing a
// disk-write error to the caller would be confusing UX
// ("invitation accepted, but the action failed?") while doing
// nothing useful for the user.
// =====================================================================

/// 7d-FIX3b: ensure peer_map has a well-formed self-entry keyed
/// by the user's Discord snowflake, carrying the separate OSL routing
/// id and X25519 public key with `is_self = true`.
///
/// Called from bootstrap.rs after `load_peer_map`, and from
/// `cmd_osl_register_self_snowflake` after identity gets a new
/// snowflake. Idempotent — a no-op if the entry already matches.
///
/// Memory-only; the caller persists peer_map.json via the
/// `verify_and_persist_peer_map_self_entry` wrapper for production
/// paths. Splitting keeps tests hermetic (no
/// `keystore::osl_config_dir()` writes).
pub fn verify_peer_map_self_entry(state: &AppState) -> Result<(String, bool), String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let (osl_user_id, pubkey_b64, mlkem_b64, snowflake) = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let id = guard
            .as_ref()
            .ok_or_else(|| "identity_not_loaded".to_string())?;
        let snow = id
            .discord_snowflake
            .clone()
            .ok_or_else(|| "no_discord_snowflake".to_string())?;
        let pub_b64 = STANDARD.encode(id.x25519_public.as_bytes());
        let mlkem_b64 = STANDARD.encode(id.mlkem_public_bytes);
        (id.user_id.clone(), pub_b64, mlkem_b64, snow)
    };

    let needs_repair = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        match pm.get(&snowflake) {
            None => true,
            Some(entry) => {
                let user_id_ok = entry.osl_user_id.as_deref() == Some(osl_user_id.as_str());
                let pubkey_ok = entry.pubkey.as_deref() == Some(pubkey_b64.as_str());
                let mlkem_ok = entry.ik_mlkem768_pub.as_deref() == Some(mlkem_b64.as_str());
                let is_self_ok = entry.is_self.unwrap_or(false);
                !(user_id_ok && pubkey_ok && mlkem_ok && is_self_ok)
            }
        }
    };

    if !needs_repair {
        return Ok((snowflake, false));
    }

    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(snowflake.clone()).or_default();
        entry.osl_user_id = Some(osl_user_id);
        entry.pubkey = Some(pubkey_b64);
        entry.ik_mlkem768_pub = Some(mlkem_b64);
        entry.discord_id = Some(snowflake.clone());
        entry.is_self = Some(true);
        // Leave outgoing_whitelists / incoming_decrypt_accepted /
        // burned_scopes alone — self-entry doesn't whitelist itself,
        // but if a prior bug populated those fields we don't want
        // to clobber unrelated state during repair.
    }
    Ok((snowflake, true))
}

/// 7d-FIX3b: production wrapper around `verify_peer_map_self_entry`
/// that persists peer_map.json if a repair happened. Tests use the
/// bare verify and inspect AppState directly.
pub fn verify_and_persist_peer_map_self_entry(state: &AppState) -> Result<(String, bool), String> {
    let result = verify_peer_map_self_entry(state)?;
    if result.1 {
        persist_peer_map_now(state);
    }
    Ok(result)
}

/// v=4 desync fix (finding 3b companion): drop every peer's
/// `ratchet_state` when the LOCAL identity is regenerated outside a
/// burn. The Double Ratchet `SessionContext` binds our own identity
/// X25519/ML-KEM pubs, so a new local identity invalidates *every*
/// peer session — not just one peer's (that case is the TOFU-Changed
/// path). Leaves `ik_ratchet_initial_pub` intact so the next v=4 send
/// re-bootstraps cleanly. Persists only if something actually
/// changed (avoids a needless write / plaintext-clobber attempt when
/// no sessions existed). NOT called from the burn path — burn already
/// wipes the whole peer_map.
pub fn clear_all_peer_ratchet_state(state: &AppState) {
    let mut changed = 0usize;
    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        for entry in pm.values_mut() {
            if entry.ratchet_state.is_some() {
                entry.ratchet_state = None;
                changed += 1;
            }
        }
    }
    if changed > 0 {
        tracing::warn!(
            cleared = changed,
            "OSL: local identity regenerated (non-burn); dropped stale \
             ratchet_state for all peers — next v=4 will re-handshake"
        );
        persist_peer_map_now(state);
    }
}

/// A(a): operator-driven single-peer v=4 session reset. Nulls
/// `ratchet_state` for ONE peer so the next v=4 send re-bootstraps a
/// fresh Double Ratchet via `new_initiator` (and the peer, once it
/// also runs this, hits `new_responder`). Used to recover a desynced
/// ratchet left over from earlier burns / re-registrations when the
/// TOFU-Changed trigger no longer fires (baseline already current).
/// Console-invokable on BOTH ends:
///   window.__TAURI__.core.invoke("osl_reset_v4_session",
///     { discordId: "<peer snowflake>" })
/// Leaves `ik_ratchet_initial_pub` / `pubkey` / `ik_mlkem768_pub`
/// intact (only the live session is dropped). Unknown peer → Err so
/// a console typo is visible. Persists only if something changed.
pub fn cmd_osl_reset_v4_session(state: &AppState, discord_id: String) -> Result<(), String> {
    let changed = {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        match pm.get_mut(&discord_id) {
            Some(entry) => {
                if entry.ratchet_state.is_some() {
                    entry.ratchet_state = None;
                    true
                } else {
                    false
                }
            }
            None => {
                return Err(format!(
                    "OSL: no peer {discord_id} in peer_map",
                    discord_id = crate::log_id::log_id(&discord_id)
                ))
            }
        }
    };
    if changed {
        tracing::warn!(
            discord_id = %crate::log_id::log_id(&discord_id),
            "OSL: v=4 session reset (operator) — dropped ratchet_state; \
             next v=4 will re-handshake"
        );
        persist_peer_map_now(state);
    }
    Ok(())
}

/// SKDM-fix (3/3): operator/programmatic reset of the v=5 sender-key
/// state for ONE scope.
///
/// Probe-3 Option-2 step 1 update: SKDM transport now ships as a
/// single v=3-bundled multi-recipient wire (no longer one v=4 per
/// peer), so a sender-key reset no longer depends on touching any
/// per-peer v=4 ratchet. This function still drops collateral v=4
/// ratchet state for backward compat with peers running pre-Option-2
/// builds — once both sides are on v=3-bundled SKDMs, the v=4 reset
/// becomes a no-op safety net.
///
/// Remedy for a scope poisoned by the pre-fix bug (sender persisted
/// sender-key state while the SKDM wire was discarded, so
/// `needs_install` stays false forever and no SKDM is ever re-emitted).
/// After this, the next v=5 send in `scope` re-enters
/// `needs_install == true` → emits a fresh SKDM (now actually posted
/// to Discord by boot.js, commit 2/3), and each peer's `apply_skdm_recv`
/// installs their receiver chain from the bundled v=3 wire.
///
/// Returns one [`SessionResetNotice`] per peer whose v=4 ratchet was
/// collaterally nulled — boot.js POSTs each to that peer's DM so they
/// auto-re-handshake instead of silently desyncing (the footgun
/// guardrail). Console-invokable:
///   window.__TAURI__.core.invoke("osl_reset_v5_sender_key",
///     { scopeInput: { kind: "gc", id: "<gc channel id>" } })
pub fn cmd_osl_reset_v5_sender_key(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<Vec<SessionResetNotice>, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let scope_key = scope.storage_key();

    // 1. Drop the v=5 sender-key chain for this scope. Lock is
    //    released before persist (persist_sender_key_state_now
    //    re-locks; std Mutex is non-reentrant).
    let v5_cleared = {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        g.states.remove(&scope_key).is_some()
    };
    if v5_cleared {
        persist_sender_key_state_now(state);
    }

    // 2. Reset the PAIRED v=4 ratchet for every non-self peer the
    //    scope can encrypt to. Two-phase under one guard: collect
    //    targets (ends the shared can_encrypt_to borrow) THEN
    //    get_mut. Lock released before persist_peer_map_now.
    let mut peers_reset: Vec<String> = Vec::new();
    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let targets: Vec<String> = pm
            .iter()
            .filter(|(did, entry)| {
                entry.is_self != Some(true)
                    && crate::whitelist::can_encrypt_to(&pm, &scope, did.as_str())
            })
            .map(|(did, _)| did.clone())
            .collect();
        for did in targets {
            if let Some(entry) = pm.get_mut(&did) {
                if entry.ratchet_state.is_some() {
                    entry.ratchet_state = None;
                    peers_reset.push(did);
                }
            }
        }
    }
    if !peers_reset.is_empty() {
        persist_peer_map_now(state);
    }

    // 3. Footgun guardrail: nulling a peer's ratchet here ALSO
    //    desyncs that peer's v=4 DM (the ratchet is shared). Pre-fix,
    //    the other side only discovered this by failing to decrypt.
    //    Now proactively announce a SESSION_RESET per affected peer so
    //    they auto-re-handshake. Best-effort: a peer we can't build a
    //    wire for (missing pubkey / identity) is logged and skipped —
    //    the local reset already happened and must not be undone by a
    //    notification failure. NOT throttled (deliberate operator
    //    action); the recv-side guards still rate-limit honoring.
    let now = now_unix_secs();
    let mut notices: Vec<SessionResetNotice> = Vec::new();
    for peer in &peers_reset {
        match build_session_reset_wire(state, peer, now) {
            Ok(wire) => notices.push(SessionResetNotice {
                peer_discord_id: peer.clone(),
                wire,
            }),
            Err(e) => tracing::warn!(
                peer = %crate::log_id::log_id(peer),
                error = %e,
                "OSL: v=5 reset — could not build SESSION_RESET notice \
                 (peer will recover on first failed decrypt instead)"
            ),
        }
    }

    tracing::warn!(
        scope = %crate::log_id::log_id(&scope_key),
        v5_cleared = v5_cleared,
        peers_reset = peers_reset.len(),
        notices = notices.len(),
        "OSL: v=5 sender-key reset (operator) — dropped sender-key \
         state + paired v=4 ratchet; next v=5 send re-emits SKDM; \
         SESSION_RESET notices queued for affected DM peers"
    );
    Ok(notices)
}

/// Random 16-byte recovery-request nonce (replay-dedupe id; carries
/// no secret material).
fn recovery_nonce() -> [u8; 16] {
    let b = crypto::random::random_bytes(16);
    let mut n = [0u8; 16];
    n.copy_from_slice(&b);
    n
}

/// Build a v=2-wrapped SESSION_RESET wire addressed to one peer. Pure
/// construction — no throttle, no ratchet mutation. Callers decide
/// when/whether to drop the local ratchet and whether to throttle.
fn build_session_reset_wire(
    state: &AppState,
    peer_discord_id: &str,
    now: i64,
) -> Result<String, String> {
    let rst = crate::control_messages::SessionReset {
        requested_at: now,
        nonce: recovery_nonce(),
    };
    let body = crate::control_messages::serialize_session_reset(&rst)
        .map_err(|e| format!("OSL: SESSION_RESET: serialize: {e}"))?;
    let sender_sk = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?
            .x25519_secret
            .clone()
    };
    let peer_pk = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        lookup_peer_pubkey(&pm, peer_discord_id)?
    };
    crate::wire_v2::encrypt_v2(
        &body,
        &[peer_pk],
        crate::wire_v2::MSG_TYPE_SESSION_RESET,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: SESSION_RESET: encrypt_v2: {e}"))
}

/// One collateral SESSION_RESET notice produced by
/// [`cmd_osl_reset_v5_sender_key`]: `wire` is a DPC0:: v=2 message
/// boot.js POSTs to its DM channel with `peer_discord_id` so that
/// peer auto-re-handshakes instead of silently desyncing.
#[derive(Debug, Serialize)]
pub struct SessionResetNotice {
    pub peer_discord_id: String,
    pub wire: String,
}

/// Auto-recovery (build side): construct a v=2-wrapped SKDM_REQUEST
/// addressed to `peer_discord_id` for `scope_input`. Called by boot.js
/// when a v=5 message stays "awaiting SKDM" past its retry budget.
/// Ships v=2 (NOT v=4): the requester may have no usable v=4 session
/// to the peer yet. Outbound-throttled per (peer, kind); a throttled
/// call returns a stable Err so boot.js skips the POST without spam.
/// Returns the DPC0:: wire string for boot.js to POST to the channel.
pub fn cmd_osl_build_skdm_request(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    peer_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let now = now_unix_secs();
    {
        let mut g = state
            .recovery_guard
            .lock()
            .expect("recovery_guard mutex poisoned");
        if !g.should_emit(
            &peer_discord_id,
            crate::recovery::RecoveryKind::SkdmRequest,
            now,
        ) {
            return Err("OSL: SKDM_REQUEST throttled (recently emitted)".to_string());
        }
    }
    let req = crate::control_messages::SkdmRequest {
        scope_storage_key: scope.storage_key(),
        requested_at: now,
        nonce: recovery_nonce(),
    };
    let body = crate::control_messages::serialize_skdm_request(&req)
        .map_err(|e| format!("OSL: SKDM_REQUEST: serialize: {e}"))?;
    let sender_sk = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?
            .x25519_secret
            .clone()
    };
    let peer_pk = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        lookup_peer_pubkey(&pm, &peer_discord_id)?
    };
    crate::wire_v2::encrypt_v2(
        &body,
        &[peer_pk],
        crate::wire_v2::MSG_TYPE_SKDM_REQUEST,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: SKDM_REQUEST: encrypt_v2: {e}"))
}

/// Auto-recovery (build side): drop our own v=4 ratchet for
/// `peer_discord_id` AND construct a v=2-wrapped SESSION_RESET telling
/// that peer to do the same, so the next v=4 send re-handshakes. The
/// local drop + the announcement are atomic from the caller's view
/// (we reset first, then hand boot.js the wire to POST). Called by
/// boot.js when a v=4 message from the peer keeps failing to decrypt
/// (ratchet desync). Outbound-throttled; throttled call returns Err.
pub fn cmd_osl_build_session_reset(
    state: &AppState,
    peer_discord_id: String,
) -> Result<String, String> {
    let now = now_unix_secs();
    {
        let mut g = state
            .recovery_guard
            .lock()
            .expect("recovery_guard mutex poisoned");
        if !g.should_emit(
            &peer_discord_id,
            crate::recovery::RecoveryKind::SessionReset,
            now,
        ) {
            return Err("OSL: SESSION_RESET throttled (recently emitted)".to_string());
        }
    }
    // Reset our side first — same collateral as cmd_osl_reset_v4_session
    // (the v=4 ratchet is shared with this peer's group SKDM path).
    let changed = {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        match pm.get_mut(&peer_discord_id) {
            Some(entry) if entry.ratchet_state.is_some() => {
                entry.ratchet_state = None;
                true
            }
            Some(_) => false,
            None => {
                return Err(format!(
                    "OSL: no peer {peer_discord_id} in peer_map",
                    peer_discord_id = crate::log_id::log_id(&peer_discord_id)
                ))
            }
        }
    };
    if changed {
        persist_peer_map_now(state);
    }
    build_session_reset_wire(state, &peer_discord_id, now)
}

/// Auto-recovery (stale-identity, RECEIVE side): re-fetch
/// `discord_id`'s published bundle from the keyserver. The fetch goes
/// through `refresh_peer_pubkeys_from_keyserver` → `tofu_observe_peer`,
/// so a CHANGED identity raises the existing loud, human-accept
/// key-change (TOFU) alert and is NOT auto-trusted; a first-use /
/// unchanged result updates silently exactly as before. Security
/// posture is therefore unchanged — this only makes the *existing*
/// TOFU prompt actually appear.
///
/// boot.js calls this when an inbound message from the peer fails as
/// "not a recipient of this message": the peer almost certainly
/// reinstalled / re-registered (new identity) and a pure receiver
/// never re-fetched them — only the SEND path force-refreshes, so a
/// passive receiver was stranded forever. Returns whether the local
/// entry changed. A keyserver error is surfaced (boot.js cooldown-
/// logs it); a stale identity is never silently accepted.
pub fn cmd_osl_recover_peer_identity(state: &AppState, discord_id: String) -> Result<bool, String> {
    let changed = refresh_peer_pubkeys_from_keyserver(state, &discord_id)?;
    if changed {
        persist_peer_map_now(state);
        tracing::warn!(
            peer = %crate::log_id::log_id(&discord_id),
            "OSL: stale-identity recovery — re-fetched peer bundle from \
             keyserver; a CHANGED identity is now a pending TOFU alert \
             (loud, one-tap accept), NOT auto-trusted"
        );
    }
    Ok(changed)
}

/// 7d-FIX3b: persist a Discord snowflake on the loaded identity and
/// repair the peer_map self-entry to match.
///
/// Validates 17-20 digit format and requires a signed account-ownership
/// proof bound to the loaded identity before any stamp or disk write.
/// Rejects mismatch against an existing recorded snowflake (account-change
/// refusal). Idempotent for matching re-registrations (just runs verify).
pub fn cmd_osl_register_self_snowflake(
    state: &AppState,
    snowflake: String,
    ownership_proof: Option<keystore::AccountOwnershipProof>,
) -> Result<(), String> {
    let dir = keystore::osl_config_dir()
        .map_err(|e| format!("OSL: register_self_snowflake: config dir: {e}"))?;
    cmd_osl_register_self_snowflake_with_dir(state, snowflake, ownership_proof, &dir)
}

/// Test seam: same as [`cmd_osl_register_self_snowflake`] but takes
/// the config dir explicitly so unit tests can point it at a
/// `tempdir()` instead of the real `%APPDATA%\osl` / `~/.config/osl`.
/// Production callers use the no-dir wrapper above.
pub fn cmd_osl_register_self_snowflake_with_dir(
    state: &AppState,
    snowflake: String,
    ownership_proof: Option<keystore::AccountOwnershipProof>,
    dir: &std::path::Path,
) -> Result<(), String> {
    osl_trace!("[F0-FIX3-TRACE] cmd_osl_register_self_snowflake entered");
    if !snowflake.chars().all(|c| c.is_ascii_digit()) || !(17..=20).contains(&snowflake.len()) {
        return Err(format!(
            "OSL: register_self_snowflake: invalid format \
             (expected 17-20 digit numeric, got {} chars)",
            snowflake.len()
        ));
    }
    verify_register_self_snowflake_ownership_proof(state, &snowflake, ownership_proof.as_ref())?;

    enum Step {
        Save(Box<keystore::Identity>),
        AlreadySet,
    }
    let step = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let id = guard
            .as_ref()
            .ok_or_else(|| "OSL: register_self_snowflake: identity not loaded".to_string())?;
        // Determine what snowflake the stored identity is "bound to":
        //   - Prefer `discord_snowflake` (post-9-F0-FIX2 field).
        //   - Fall back to `user_id` on legacy identity files where
        //     `discord_snowflake` was never written (those files have
        //     user_id == the original Discord snowflake by
        //     construction). Without this fallback the switch
        //     detection silently fails on any identity from before
        //     the field was added.
        let bound_to = id
            .discord_snowflake
            .clone()
            .or_else(|| {
                // Only legacy identities whose user_id is itself a
                // Discord snowflake were implicitly bound that way.
                // Older test/dev identities used names such as
                // "alice" and still need their first explicit bind.
                (id.user_id.chars().all(|c| c.is_ascii_digit())
                    && (17..=20).contains(&id.user_id.len()))
                .then(|| id.user_id.clone())
            })
            .unwrap_or_default();
        if !bound_to.is_empty() && bound_to != snowflake {
            // This command is callable by the remote Discord origin.
            // A reported account mismatch must therefore never erase,
            // unregister, or rotate the locally bound identity. Account
            // changes require an explicit trusted-local flow.
            return Err(format!(
                "OSL: register_self_snowflake: snowflake mismatch with identity bound to {bound_to}",
                bound_to = crate::log_id::log_id(&bound_to)
            ));
        } else if id.discord_snowflake.is_some()
            && !(id.user_id.chars().all(|c| c.is_ascii_digit())
                && (17..=20).contains(&id.user_id.len()))
        {
            Step::AlreadySet
        } else {
            let mut snapshot = id.clone();
            snapshot.discord_snowflake = Some(snowflake.clone());
            if snapshot.user_id.chars().all(|c| c.is_ascii_digit())
                && (17..=20).contains(&snapshot.user_id.len())
            {
                snapshot.user_id = keystore::native_user_id(&snapshot);
            }
            Step::Save(Box::new(snapshot))
        }
    };

    let to_save = match step {
        Step::AlreadySet => {
            // Identity already bound to this snowflake (a relaunch
            // re-calling register_self_snowflake, or recovery after
            // the keyserver row was purged). Re-assert our presence
            // on the keyserver — idempotent upsert, non-fatal.
            ensure_keyserver_registered(
                state,
                &resolve_keyserver_base_url(dir),
                read_keyserver_client_token(dir),
            );
            return run_verify(state);
        }
        Step::Save(snapshot) => *snapshot,
    };

    let path = dir.join("identity.json");
    let sealer = keystore::select_best_sealer();
    if let Err(e) = keystore::save_identity(&path, &to_save, sealer.as_ref()) {
        return Err(format!(
            "OSL: register_self_snowflake: save_identity failed: {e}"
        ));
    }
    state.install_identity(to_save);
    // REGISTER-FIX: snowflake just attached to a pre-existing
    // identity and persisted — register against the keyserver now
    // rather than waiting for the next relaunch. Idempotent, non-fatal.
    ensure_keyserver_registered(
        state,
        &resolve_keyserver_base_url(dir),
        read_keyserver_client_token(dir),
    );
    run_verify(state)
}

fn verify_register_self_snowflake_ownership_proof(
    state: &AppState,
    snowflake: &str,
    ownership_proof: Option<&keystore::AccountOwnershipProof>,
) -> Result<(), String> {
    let proof = ownership_proof.ok_or_else(|| {
        "OSL: register_self_snowflake: account ownership proof required".to_string()
    })?;
    let identity = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "OSL: register_self_snowflake: identity not loaded".to_string())?
    };
    proof.validate_shape().map_err(|_| {
        "OSL: register_self_snowflake: account ownership proof malformed".to_string()
    })?;
    if proof.platform_id != snowflake {
        return Err(
            "OSL: register_self_snowflake: account ownership proof does not match account"
                .to_string(),
        );
    }
    if proof.e.owner_user_id != identity.user_id {
        return Err(
            "OSL: register_self_snowflake: account ownership proof is not bound to this identity"
                .to_string(),
        );
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "OSL: register_self_snowflake: system clock before Unix epoch".to_string())?
        .as_secs();
    if now >= proof.e.expires_at_unix_seconds {
        return Err("OSL: register_self_snowflake: account ownership proof expired".to_string());
    }
    let canonical = proof.canonical_bytes().map_err(|_| {
        "OSL: register_self_snowflake: account ownership proof malformed".to_string()
    })?;
    let signature_bytes = STANDARD.decode(&proof.e.signature_b64).map_err(|_| {
        "OSL: register_self_snowflake: account ownership proof malformed".to_string()
    })?;
    let signature_array: [u8; ed25519::SIGNATURE_SIZE] =
        signature_bytes.try_into().map_err(|_| {
            "OSL: register_self_snowflake: account ownership proof malformed".to_string()
        })?;
    let signature = ed25519::Signature::from_bytes(signature_array);
    let ok = ed25519::verify(&identity.ed25519_public, &canonical, &signature).map_err(|_| {
        "OSL: register_self_snowflake: account ownership proof malformed".to_string()
    })?;
    if !ok {
        return Err(
            "OSL: register_self_snowflake: account ownership proof signature invalid".to_string(),
        );
    }
    Ok(())
}

fn run_verify(state: &AppState) -> Result<(), String> {
    match verify_and_persist_peer_map_self_entry(state) {
        Ok((_snowflake, repaired)) => {
            if repaired {
                eprintln!("[OSL][bootstrap] self-entry repaired");
            } else {
                eprintln!("[OSL][bootstrap] self-entry verified");
            }
            Ok(())
        }
        Err(reason) if reason == "no_discord_snowflake" => {
            eprintln!(
                "[OSL][bootstrap] no discord snowflake on identity; \
                 deferring to boot.js"
            );
            Ok(())
        }
        Err(reason) if reason == "identity_not_loaded" => {
            Err("OSL: register_self_snowflake: identity not loaded".into())
        }
        Err(other) => Err(other),
    }
}

/// 9-TD1.4: stamp `state.last_persist_error` so a follow-up
/// `cmd_osl_take_last_persist_error` call from the JS layer can
/// surface "couldn't save change to disk" to the user. Pre-TD1
/// these failures lived only as `tracing::warn!` lines that nobody
/// read.
fn record_persist_error(state: &AppState, what: &str, err: impl std::fmt::Display) {
    let msg = format!("{what}: {err}");
    tracing::warn!(error = %msg, "OSL: persist failed");
    if let Ok(mut g) = state.last_persist_error.lock() {
        *g = Some(msg);
    }
}

/// 9-TD1.4: read + clear the last-persist-error slot. JS polls this
/// after mutation invokes (whitelist add/remove, burn, settings
/// changes, etc.) to surface persist failures as a toast. Read-once
/// semantics — second call after a fresh persist failure returns
/// `None`. The single-slot design intentionally collapses multiple
/// rapid failures into a single "something failed, please retry"
/// signal; the slot is for UX visibility, not for forensic audit
/// (that lives in `tracing::warn!`).
pub fn cmd_osl_take_last_persist_error(state: &AppState) -> Option<String> {
    state
        .last_persist_error
        .lock()
        .ok()
        .and_then(|mut g| g.take())
}

fn persist_peer_map_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            record_persist_error(state, "peer_map dir resolve", e);
            return;
        }
    };
    let path = dir.join("peer_map.json");

    // TD3-1.4: bootstrap fires `verify_and_persist_peer_map_self_entry`
    // BEFORE the password gate installs `file_storage_key`. If the
    // on-disk peer_map is encrypted (OSL-ENC1 magic), `write_peer_map`
    // refuses (defense-in-depth against clobbering an encrypted file
    // with plaintext) and `record_persist_error` emits a warn. That
    // warn is a launch-log false alarm — `state_reload.rs::
    // reload_encrypted_state_after_unlock` re-runs this exact verify
    // immediately after the gate installs the key, and the persist
    // succeeds there. Short-circuit silently in the pre-gate-encrypted
    // case so a normal launch doesn't surface the warn line.
    if crate::main_password::get_file_storage_key().is_none() {
        if let Ok(existing) = std::fs::read(&path) {
            if crate::main_password::has_enc_magic(&existing) {
                tracing::info!(
                    "OSL: deferring peer_map persist (file_storage_key not yet \
                     installed; post-gate reload will persist)"
                );
                return;
            }
        }
    }

    let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
    if let Err(e) = crate::peer_map::write_peer_map(&path, &pm) {
        record_persist_error(state, "peer_map.json", e);
    }
}

fn persist_sender_key_state_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            record_persist_error(state, "sender_key_state dir resolve", e);
            return;
        }
    };
    let path = dir.join("sender_key_state.json");
    let g = state
        .sender_key_state
        .lock()
        .expect("sender_key_state mutex poisoned");
    if let Err(e) = crate::sender_key_state::write_sender_key_state(&path, &g) {
        record_persist_error(state, "sender_key_state.json", e);
    }
}

pub fn persist_whitelist_state_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            record_persist_error(state, "whitelist_state dir resolve", e);
            return;
        }
    };
    let path = dir.join("whitelist_state.json");
    // 9-C3: write the full envelope (scopes + server_defaults) so a
    // mutation to either map round-trips both. Pre-C3 used the
    // truncated `write_whitelist_state` which only carried scopes —
    // that would have silently wiped server_defaults on every
    // whitelist mutation.
    let ws = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned")
        .clone();
    let sd = state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned")
        .clone();
    let envelope = crate::whitelist_state::WhitelistStateFile {
        migrated_c1: true,
        scopes: ws,
        server_defaults: sd,
    };
    if let Err(e) = crate::whitelist_state::write_whitelist_state_file(&path, &envelope) {
        record_persist_error(state, "whitelist_state.json", e);
    }
}

/// W2: mirror `AppState::scope_membership` to `membership.json`.
/// Best-effort like the other persisters — a failure records a
/// surfaced persist error but never aborts the caller (membership
/// re-accrues from gateway events, so a lost write is recoverable).
pub fn persist_scope_membership_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            record_persist_error(state, "membership dir resolve", e);
            return;
        }
    };
    let path = dir.join("membership.json");
    let snapshot = state
        .scope_membership
        .lock()
        .expect("scope_membership mutex poisoned")
        .clone();
    if let Err(e) = crate::membership::write_scope_membership(&path, &snapshot) {
        record_persist_error(state, "membership.json", e);
    }
}

// ---- DTOs ----

#[derive(Debug, Serialize)]
pub struct GenerateIdentityResponse {
    pub user_id: String,
    pub ik_x25519_pub_b64: String,
    pub ik_mlkem768_pub_b64: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub initial_registration: bool,
    pub registered_at: Option<String>,
    pub last_rotated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FetchPubkeysResponse {
    pub user_id: String,
    pub ik_x25519_pub_b64: String,
    pub ik_mlkem768_pub_b64: String,
    pub registered_at: String,
    pub last_rotated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AeadSealRequest {
    pub key_b64: String,
    pub nonce_b64: String,
    pub ad_b64: Option<String>,
    pub plaintext_b64: String,
}

#[derive(Debug, Serialize)]
pub struct AeadSealResponse {
    pub ciphertext_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct AeadOpenRequest {
    pub key_b64: String,
    pub nonce_b64: String,
    pub ad_b64: Option<String>,
    pub ciphertext_b64: String,
}

#[derive(Debug, Deserialize)]
pub struct StegoEncodeRequest {
    pub ciphertext_b64: String,
}

#[derive(Debug, Serialize)]
pub struct StegoEncodeResponse {
    pub stego_message: String,
}

#[derive(Debug, Serialize)]
pub struct StegoDecodeResponse {
    pub ciphertext_b64: String,
}

// ---- helpers ----

fn b64_to_array<const N: usize>(field: &str, b: &str) -> IpcResult<[u8; N]> {
    let v = STANDARD.decode(b)?;
    if v.len() != N {
        return Err(IpcError::InvalidArgument(format!(
            "{field}: expected {N} bytes, got {}",
            v.len()
        )));
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&v);
    Ok(out)
}

fn b64_to_vec(b: &str) -> IpcResult<Vec<u8>> {
    Ok(STANDARD.decode(b)?)
}

// ---- identity ----

/// Generate a fresh [`keystore::Identity`] in [`AppState`] and return
/// its public bytes. Overwrites any previously-loaded identity.
pub fn cmd_generate_identity(
    state: &AppState,
    user_id: String,
) -> IpcResult<GenerateIdentityResponse> {
    if user_id.trim().is_empty() {
        return Err(IpcError::InvalidArgument(
            "user_id must be non-empty".into(),
        ));
    }
    let identity = generate_identity(user_id.clone());
    let resp = GenerateIdentityResponse {
        user_id: identity.user_id.clone(),
        ik_x25519_pub_b64: STANDARD.encode(identity.x25519_public.as_bytes()),
        ik_mlkem768_pub_b64: STANDARD.encode(identity.mlkem_public_bytes),
    };
    state.install_identity(identity);
    Ok(resp)
}

pub fn cmd_load_identity(state: &AppState, path: String) -> IpcResult<GenerateIdentityResponse> {
    let sealer = select_best_sealer();
    let id = keystore::load_identity(&PathBuf::from(path), sealer.as_ref())?;
    let resp = GenerateIdentityResponse {
        user_id: id.user_id.clone(),
        ik_x25519_pub_b64: STANDARD.encode(id.x25519_public.as_bytes()),
        ik_mlkem768_pub_b64: STANDARD.encode(id.mlkem_public_bytes),
    };
    state.install_identity(id);
    Ok(resp)
}

pub fn cmd_save_identity(state: &AppState, path: String) -> IpcResult<()> {
    let guard = state.identity.lock().expect("identity mutex poisoned");
    let id = guard.as_ref().ok_or(IpcError::IdentityMissing)?;
    let sealer = select_best_sealer();
    keystore::save_identity(&PathBuf::from(path), id, sealer.as_ref())?;
    Ok(())
}

// ---- key server ----

pub fn cmd_init_keyserver(state: &AppState, base_url: String) -> IpcResult<()> {
    let client = KeyServerClient::new(base_url)?;
    *state.keyserver.lock().expect("keyserver mutex poisoned") = Some(client);
    Ok(())
}

pub fn cmd_register(state: &AppState) -> IpcResult<RegisterResponse> {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard.as_ref().ok_or(IpcError::IdentityMissing)?;
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard.as_ref().ok_or(IpcError::KeyserverMissing)?;
    let resp = client.register(identity)?;
    Ok(RegisterResponse {
        user_id: resp.user_id,
        initial_registration: resp.registered_at.is_some(),
        registered_at: resp.registered_at,
        last_rotated_at: resp.last_rotated_at,
    })
}

pub fn cmd_fetch_pubkeys(state: &AppState, user_id: String) -> IpcResult<FetchPubkeysResponse> {
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard.as_ref().ok_or(IpcError::KeyserverMissing)?;
    let resp = client.fetch_pubkeys(&user_id)?;
    Ok(FetchPubkeysResponse {
        user_id: resp.user_id,
        ik_x25519_pub_b64: resp.ik_x25519_pub,
        ik_mlkem768_pub_b64: resp.ik_mlkem768_pub,
        registered_at: resp.registered_at,
        last_rotated_at: resp.last_rotated_at,
    })
}

/// Periodic cadence for the local prekey replenishment backstop.
///
/// This is deliberately slow: event-triggered calls with an observed
/// server remaining count do the precise top-up work. The periodic tick is a
/// launch-lifetime backstop that also catches SPK rotation due dates.
pub const PREKEY_REPLENISH_INTERVAL_SECONDS: u64 = 6 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PrekeyReplenishmentOutcome {
    InitialPublished { opks_published: u32 },
    Replenished { opks_added: u32 },
    Skipped,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrekeyReplenishmentDecision {
    InitialPublish,
    Replenish { server_remaining: u32 },
    Skip,
}

fn decide_prekey_replenishment(
    state: Option<&keystore::PrekeyState>,
    observed_server_remaining: Option<u32>,
    now_unix_seconds: u64,
) -> PrekeyReplenishmentDecision {
    let Some(state) = state else {
        return PrekeyReplenishmentDecision::InitialPublish;
    };
    let server_remaining = observed_server_remaining
        .unwrap_or_else(|| u32::try_from(state.opk_pool.len()).unwrap_or(u32::MAX));
    if state.should_rotate_spk(now_unix_seconds) || state.should_replenish(server_remaining) {
        PrekeyReplenishmentDecision::Replenish { server_remaining }
    } else {
        PrekeyReplenishmentDecision::Skip
    }
}

/// Run one local prekey replenishment scheduler tick.
///
/// Preconditions are intentionally strict. Missing identity or keyserver is a
/// refusal, not permission to synthesize authority. Missing `prekeys.json`
/// means this is the first authorized publication for the already-loaded
/// identity; after that, all top-ups go through
/// [`KeyServerClient::replenish_using_state`].
pub fn run_prekey_replenishment_tick(
    state: &AppState,
    dir: &std::path::Path,
    observed_server_remaining: Option<u32>,
) -> Result<PrekeyReplenishmentOutcome, String> {
    let now_unix_seconds = now_unix_secs().max(0) as u64;
    run_prekey_replenishment_tick_at(state, dir, observed_server_remaining, now_unix_seconds)
}

fn run_prekey_replenishment_tick_at(
    state: &AppState,
    dir: &std::path::Path,
    observed_server_remaining: Option<u32>,
    now_unix_seconds: u64,
) -> Result<PrekeyReplenishmentOutcome, String> {
    let identity = state
        .identity
        .lock()
        .expect("identity mutex poisoned")
        .clone()
        .ok_or_else(|| "OSL: prekey replenish refused: identity is not loaded".to_string())?;
    let client = state
        .keyserver
        .lock()
        .expect("keyserver mutex poisoned")
        .clone()
        .ok_or_else(|| "OSL: prekey replenish refused: keyserver is not configured".to_string())?;

    let path = dir.join("prekeys.json");
    let sealer = keystore::select_best_sealer();
    let existing = if path.exists() {
        let prekeys = keystore::load_prekey_state(&path, sealer.as_ref())
            .map_err(|_| "OSL: prekey replenish refused: local prekey state is unreadable")?;
        crate::state_reload::validate_prekey_state_for_identity(&identity, &prekeys)
            .map_err(|_| "OSL: prekey replenish refused: local prekey state is unbound")?;
        state.set_prekey_state(prekeys.clone());
        Some(prekeys)
    } else {
        None
    };

    match decide_prekey_replenishment(
        existing.as_ref(),
        observed_server_remaining,
        now_unix_seconds,
    ) {
        PrekeyReplenishmentDecision::InitialPublish => {
            let prekeys = keystore::PrekeyState::new(
                &identity,
                keystore::PrekeyConfig::default(),
                now_unix_seconds,
            );
            client
                .replenish_prekeys(&identity, Some(&prekeys.current_spk), &prekeys.opk_pool)
                .map_err(|_| "OSL: prekey initial publication failed".to_string())?;
            keystore::save_prekey_state(&path, &prekeys, sealer.as_ref())
                .map_err(|_| "OSL: prekey state persist failed".to_string())?;
            state.set_prekey_state(prekeys.clone());
            Ok(PrekeyReplenishmentOutcome::InitialPublished {
                opks_published: u32::try_from(prekeys.opk_pool.len()).unwrap_or(u32::MAX),
            })
        }
        PrekeyReplenishmentDecision::Replenish { server_remaining } => {
            let mut prekeys = existing.ok_or_else(|| {
                "OSL: prekey replenish refused: local prekey state is absent".to_string()
            })?;
            let response = client
                .replenish_using_state(&identity, &mut prekeys, server_remaining, now_unix_seconds)
                .map_err(|_| "OSL: prekey replenish failed".to_string())?;
            keystore::save_prekey_state(&path, &prekeys, sealer.as_ref())
                .map_err(|_| "OSL: prekey state persist failed".to_string())?;
            state.set_prekey_state(prekeys);
            Ok(PrekeyReplenishmentOutcome::Replenished {
                opks_added: response.opks_added,
            })
        }
        PrekeyReplenishmentDecision::Skip => Ok(PrekeyReplenishmentOutcome::Skipped),
    }
}

#[cfg(all(test))]
mod prekey_replenishment_scheduler_tests {
    use super::{
        decide_prekey_replenishment, run_prekey_replenishment_tick_at, PrekeyReplenishmentDecision,
    };
    use crate::state::AppState;
    use keystore::{generate_identity, PrekeyConfig, PrekeyState, SPK_ROTATION_INTERVAL_SECONDS};

    #[test]
    fn missing_local_state_plans_initial_publication() {
        assert!(matches!(
            decide_prekey_replenishment(None, None, 1_700_000_000),
            PrekeyReplenishmentDecision::InitialPublish
        ));
    }

    #[test]
    fn observed_remaining_at_threshold_triggers_replenish_using_state_path() {
        let identity = generate_identity("scheduler-test".to_string());
        let state = PrekeyState::new(&identity, PrekeyConfig::default(), 1_700_000_000);

        assert!(matches!(
            decide_prekey_replenishment(Some(&state), Some(25), 1_700_000_001),
            PrekeyReplenishmentDecision::Replenish {
                server_remaining: 25
            }
        ));
    }

    #[test]
    fn periodic_tick_skips_when_local_pool_is_above_threshold() {
        let identity = generate_identity("scheduler-test".to_string());
        let state = PrekeyState::new(&identity, PrekeyConfig::default(), 1_700_000_000);

        assert!(matches!(
            decide_prekey_replenishment(Some(&state), None, 1_700_000_001),
            PrekeyReplenishmentDecision::Skip
        ));
    }

    #[test]
    fn periodic_tick_rotates_spk_when_due_even_without_opk_depletion() {
        let identity = generate_identity("scheduler-test".to_string());
        let state = PrekeyState::new(&identity, PrekeyConfig::default(), 1_700_000_000);

        assert!(matches!(
            decide_prekey_replenishment(
                Some(&state),
                None,
                1_700_000_000 + SPK_ROTATION_INTERVAL_SECONDS
            ),
            PrekeyReplenishmentDecision::Replenish {
                server_remaining: 100
            }
        ));
    }

    #[test]
    fn scheduler_refuses_without_identity_before_touching_disk_or_network() {
        let app = AppState::new();
        let dir = tempfile::tempdir().unwrap();

        let err = match run_prekey_replenishment_tick_at(&app, dir.path(), None, 1_700_000_000) {
            Ok(_) => panic!("missing identity must refuse"),
            Err(err) => err,
        };
        assert!(err.contains("identity is not loaded"), "err: {err}");
    }
}

// ---- AEAD primitive ----

pub fn cmd_aead_seal(req: AeadSealRequest) -> IpcResult<AeadSealResponse> {
    let key = aead::Key::from_bytes(b64_to_array::<{ aead::KEY_SIZE }>("key_b64", &req.key_b64)?);
    let nonce = aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>(
        "nonce_b64",
        &req.nonce_b64,
    )?);
    let ad = match req.ad_b64.as_deref() {
        Some(a) => b64_to_vec(a)?,
        None => Vec::new(),
    };
    let plaintext = b64_to_vec(&req.plaintext_b64)?;
    let ct = aead::seal(&key, &nonce, &ad, &plaintext)?;
    Ok(AeadSealResponse {
        ciphertext_b64: STANDARD.encode(&ct),
    })
}

pub fn cmd_aead_open(req: AeadOpenRequest) -> IpcResult<AeadSealResponse> {
    let key = aead::Key::from_bytes(b64_to_array::<{ aead::KEY_SIZE }>("key_b64", &req.key_b64)?);
    let nonce = aead::Nonce::from_bytes(b64_to_array::<{ aead::NONCE_SIZE }>(
        "nonce_b64",
        &req.nonce_b64,
    )?);
    let ad = match req.ad_b64.as_deref() {
        Some(a) => b64_to_vec(a)?,
        None => Vec::new(),
    };
    let ciphertext = b64_to_vec(&req.ciphertext_b64)?;
    let pt = aead::open(&key, &nonce, &ad, &ciphertext)?;
    // Reuse the seal-response shape — it's just `{ ciphertext_b64 }` —
    // but the field name is generic enough to carry recovered
    // plaintext too. JS callers should treat this as opaque bytes.
    Ok(AeadSealResponse {
        ciphertext_b64: STANDARD.encode(&pt),
    })
}

// ---- stego ----

pub fn cmd_stego_encode(req: StegoEncodeRequest) -> IpcResult<StegoEncodeResponse> {
    let ciphertext = b64_to_vec(&req.ciphertext_b64)?;
    let s = stego::encode_mode0(&ciphertext)?;
    Ok(StegoEncodeResponse { stego_message: s })
}

pub fn cmd_stego_decode(stego_message: String) -> IpcResult<StegoDecodeResponse> {
    let bytes = stego::decode_mode0(&stego_message)?;
    Ok(StegoDecodeResponse {
        ciphertext_b64: STANDARD.encode(&bytes),
    })
}

// ---- introspection ----

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub identity_loaded: bool,
    pub keyserver_initialised: bool,
    pub user_id: Option<String>,
    pub x25519_public_b64: Option<String>,
}

pub fn cmd_status(state: &AppState) -> StatusResponse {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let id_ref = id_guard.as_ref();
    StatusResponse {
        identity_loaded: id_ref.is_some(),
        keyserver_initialised: state.has_keyserver(),
        user_id: id_ref.map(|i| i.user_id.clone()),
        x25519_public_b64: id_ref.map(|i| STANDARD.encode(i.x25519_public.as_bytes())),
    }
}

// X25519 helper (used by tests + for the eventual ratchet-handshake
// command surface). Kept here so the IPC tests can verify the X25519
// glue end-to-end without re-importing the whole crypto crate API.
pub fn cmd_x25519_diffie_hellman(secret_b64: String, peer_public_b64: String) -> IpcResult<String> {
    let secret = x25519::SecretKey::from_bytes(b64_to_array::<{ x25519::SECRET_KEY_SIZE }>(
        "secret_b64",
        &secret_b64,
    )?);
    let peer = x25519::PublicKey::from_bytes(b64_to_array::<{ x25519::PUBLIC_KEY_SIZE }>(
        "peer_public_b64",
        &peer_public_b64,
    )?);
    let shared = x25519::diffie_hellman(&secret, &peer)?;
    Ok(STANDARD.encode(shared.as_bytes()))
}

// ---- Layer 10 / Phase 4: osl_encrypt_message pipeline ----
//
// Wire format (post-stego-decode bytes; the outer `DPC0::` prefix is
// applied by `stego::encode_mode0`):
//
// ```text
// [
//   version:    u8 = 0x01     // hard-coded; future formats bump this
//   N:          u8            // recipient count, 1..=255
//   per-recipient (N times, sender-sorted by user_id ASCII order to
//                  give the receiver a stable index for `pub_hint`
//                  collisions):
//     pub_hint: u8            // low byte of recipient's IK_X25519
//                             // public key — receiver scans for the
//                             // slot whose pub_hint matches their
//                             // own and tries decrypt
//     nonce_k:  [u8; 24]      // XChaCha20-Poly1305 nonce for the
//                             // session-key wrap
//     wrap_k:   [u8; 48]      // 32-byte session key + 16-byte tag
//   nonce_msg:  [u8; 24]      // nonce for the bulk message AEAD
//   ct_msg:     [u8; pt_len + 16]  // ciphertext + tag
// ]
// ```
//
// Why session-key wrap (KEM-then-DEM) over per-recipient AEAD of the
// full plaintext: a 1400-byte Mode-0 budget with N recipients gives
// us roughly `1400 - N*(plaintext_len + 40)` of working room with
// per-recipient AEAD; the wrap scheme drops the per-recipient cost
// to a constant 73 bytes (1 + 24 + 48), so a 1000-byte plaintext
// fits up to N=5 instead of N=1 only. See `docs/design/
// layer-10-discord-internals.md` §13 for the working math.
//
// AEAD associated-data strings are static domain separators — the
// inner JSON-shaped Discord context (reply IDs, attachments, etc.)
// is not bound here because Phase 4 does not yet have a receive-side
// decoder that would re-validate it. Phase 5 binds AD to the full
// PQXDH transcript.
//
// Phase 5+ replaces this entirely with the PQXDH handshake +
// Double Ratchet header keys. The wire shape changes (version byte
// bumps to 0x02+); the IPC contract `(channel_id, plaintext, options)
// -> Result<String, String>` does not.

/// Wire-format version of the Phase 4 OSL framing inside Mode 0
/// payloads. Bump in lockstep with any field-shape change. Phase 5
/// (PQXDH + Double Ratchet) will introduce 0x02+.
pub const OSL_PHASE4_WIRE_VERSION: u8 = 0x01;

/// Maximum plaintext byte length accepted by [`cmd_osl_encrypt_message`].
/// Chosen as a soft UX cap (single chat-input bubble) — the hard
/// cap from Mode 0's 1400-byte budget is computed dynamically per
/// recipient count and may be tighter; the smaller of the two
/// applies.
pub const OSL_PHASE4_PLAINTEXT_BYTE_CAP: usize = 1000;

/// Per-recipient framing cost inside the wire payload:
/// `pub_hint(1) + nonce_k(24) + wrap_k(32 session key + 16 tag)`.
pub const OSL_PHASE4_PER_RECIPIENT_BYTES: usize =
    1 + aead::NONCE_SIZE + aead::KEY_SIZE + aead::TAG_SIZE;

/// Fixed framing cost: `version(1) + N(1) + nonce_msg(24) + tag_msg(16)`.
pub const OSL_PHASE4_FIXED_FRAMING_BYTES: usize = 1 + 1 + aead::NONCE_SIZE + aead::TAG_SIZE;

/// AEAD associated-data: static domain separator for the bulk
/// message ciphertext leg. Static (no transcript binding) is
/// deliberate for Phase 4 — Phase 5 will bind to the PQXDH
/// transcript.
pub const OSL_PHASE4_AD_MSG: &[u8] = b"OSL/P4/msg/v1";

/// AEAD associated-data: static domain separator for the
/// per-recipient session-key wrap leg.
pub const OSL_PHASE4_AD_WRAP: &[u8] = b"OSL/P4/wrap/v1";

/// HKDF info string for deriving the per-recipient wrap key from
/// the X25519 ECDH shared secret. Empty salt — the IKM is already
/// 32 bytes of high-entropy DH output and the AEAD nonces provide
/// per-message uniqueness.
pub const OSL_PHASE4_HKDF_INFO_WRAP: &[u8] = b"OSL/P4/wrap-key/v1";

/// F3.6: error-string prefix for tier-gate-blocked operations.
/// The JSON tail after the colon deserialises to
/// [`crate::tier_gate::TierGateError`]; boot.js's modal handler
/// parses `kind = "paid_feature_required"` and renders the
/// upgrade modal. F3.2's text-encrypt gate retired in F3.6; the
/// surviving gate is the attachment-send check at
/// [`cmd_osl_seal_attachment_with_cover_v3`]. Stable wire string —
/// bump only if the JSON shape changes incompatibly.
pub const OSL_TIER_BLOCKED_PREFIX: &str = "OSL-TIER-BLOCKED:";

/// F3.6 attachment-send tier gate. Called at the top of
/// [`cmd_osl_seal_attachment_with_cover_v3`]. Paid + PaidOfflineGrace
/// callers fall through; Free/Unconfigured/EXPIRED/etc. get the
/// prefixed JSON error boot.js parses to surface the upgrade modal.
///
/// `serde_json::to_string` on `TierGateError` cannot realistically
/// fail; the fallback `{"kind":"paid_feature_required"}` is
/// defensive only.
fn enforce_attachment_tier_gate(state: &AppState) -> Result<(), String> {
    match crate::tier_gate::check_attachment_allowed(state) {
        Ok(()) => Ok(()),
        Err(e) => {
            let json = serde_json::to_string(&e)
                .unwrap_or_else(|_| "{\"kind\":\"paid_feature_required\"}".to_string());
            Err(format!("{OSL_TIER_BLOCKED_PREFIX}{json}"))
        }
    }
}

/// Pure encoder for the Phase 4 wire format. Takes pre-resolved
/// recipient pubkeys and returns the Mode 0 cover string.
///
/// "Pure" in the I/O sense: no [`AppState`] access, no network
/// calls, no filesystem reads. Random bytes (session key + nonces)
/// come from `crypto::random`, so successive calls with identical
/// inputs produce different outputs — this isn't a hash. Tests
/// that need deterministic output should mock or fix the random
/// source separately.
///
/// # Auto-include sender as recipient
///
/// The sender's own X25519 public key is always added to the
/// recipient slot list (deduped against the explicit
/// `recipient_pubkeys`). Two reasons:
///
/// - **Optimistic-render UX.** When the sender hits Enter, the
///   server bounces the encrypted message back as a
///   `MESSAGE_CREATE` event; without a sender slot, the sender's
///   own client can't decrypt their own message and would render
///   the `DPC0::` cover instead of plaintext. The auto-slot fixes
///   this for the common case.
/// - **Search consistency.** Discord's Cmd-F search runs against
///   the rendered message text (post-decrypt). Without a sender
///   slot, the sender can't search their own past messages.
///
/// Cost: one extra slot per message (73 bytes inside the Mode 0
/// payload). Worth it.
///
/// `recipient_pubkeys` order determines the slot order in the
/// wire format up to the sender slot, which is appended last.
/// Callers should pre-sort the input to whatever ordering the
/// receive-side decoder expects (the IPC wrapper
/// [`cmd_osl_encrypt_message`] sorts by recipient `user_id` ASCII
/// before reaching this function).
///
/// Caps enforced here:
/// - Empty plaintext rejected.
/// - `plaintext.len() > OSL_PHASE4_PLAINTEXT_BYTE_CAP`.
/// - Effective recipient count (input + sender, post-dedup) `0`
///   or `> 255`.
/// - Total wire length `> stego::MODE0_MAX_RAW_LEN`.
/// - Stego output length `> 2000` chars (Discord message cap).
///
/// The IPC wrapper rechecks the recipient-count cap defensively;
/// it's the same check, so duplication is fine.
pub fn encrypt_osl_phase4_to_pubkeys(
    sender_secret: &x25519::SecretKey,
    recipient_pubkeys: &[x25519::PublicKey],
    plaintext: &str,
) -> Result<String, String> {
    let plaintext_bytes = plaintext.as_bytes();
    if plaintext_bytes.is_empty() {
        return Err("OSL: refusing to encrypt empty plaintext".to_string());
    }
    if plaintext_bytes.len() > OSL_PHASE4_PLAINTEXT_BYTE_CAP {
        return Err(format!(
            "OSL: plaintext is {} bytes, exceeds soft cap of {}",
            plaintext_bytes.len(),
            OSL_PHASE4_PLAINTEXT_BYTE_CAP
        ));
    }

    // Build the effective slot list: input recipients plus the
    // sender's own pubkey (auto-included). Dedup by raw pubkey
    // bytes so callers passing the sender as an explicit
    // recipient (e.g. tests, or future channels.json that lists
    // self) don't double up.
    let sender_pub = x25519::derive_public(sender_secret);
    let mut effective: Vec<x25519::PublicKey> = Vec::with_capacity(recipient_pubkeys.len() + 1);
    let mut seen_keys: Vec<[u8; x25519::PUBLIC_KEY_SIZE]> = Vec::new();
    for pk in recipient_pubkeys.iter() {
        let bytes = *pk.as_bytes();
        if !seen_keys.iter().any(|b| b == &bytes) {
            seen_keys.push(bytes);
            effective.push(*pk);
        }
    }
    let sender_bytes = *sender_pub.as_bytes();
    if !seen_keys.iter().any(|b| b == &sender_bytes) {
        seen_keys.push(sender_bytes);
        effective.push(sender_pub);
    }

    let n = effective.len();
    if n == 0 {
        return Err("OSL: zero recipients after lookup".to_string());
    }
    if n > 255 {
        return Err(format!(
            "OSL: recipient count {n} exceeds wire-format max of 255"
        ));
    }

    let total_wire_len =
        OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES + plaintext_bytes.len();
    if total_wire_len > stego::MODE0_MAX_RAW_LEN {
        let max_plaintext_for_n = stego::MODE0_MAX_RAW_LEN
            .saturating_sub(OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES);
        return Err(format!(
            "OSL: payload {} bytes exceeds Mode 0 cap {} ({} recipients; \
             max plaintext for this recipient count is {} bytes)",
            total_wire_len,
            stego::MODE0_MAX_RAW_LEN,
            n,
            max_plaintext_for_n
        ));
    }

    let session_key = random::random_aead_key();
    let nonce_msg = random::random_nonce();
    let ct_msg = aead::seal(&session_key, &nonce_msg, OSL_PHASE4_AD_MSG, plaintext_bytes)
        .map_err(|e| format!("OSL: AEAD seal (msg) failed: {e}"))?;

    let mut wire: Vec<u8> = Vec::with_capacity(total_wire_len);
    wire.push(OSL_PHASE4_WIRE_VERSION);
    wire.push(n as u8);

    for (slot_ix, peer_pub) in effective.iter().enumerate() {
        let shared = x25519::diffie_hellman(sender_secret, peer_pub)
            .map_err(|e| format!("OSL: ECDH (slot {slot_ix}): {e}"))?;
        let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
            .map_err(|e| format!("OSL: HKDF wrap-key (slot {slot_ix}): {e}"))?;
        let wrap_key = aead::Key::from_bytes(wrap_key_bytes);

        let nonce_k = random::random_nonce();
        let wrap_ct = aead::seal(
            &wrap_key,
            &nonce_k,
            OSL_PHASE4_AD_WRAP,
            session_key.as_bytes(),
        )
        .map_err(|e| format!("OSL: AEAD seal (wrap) (slot {slot_ix}): {e}"))?;
        if wrap_ct.len() != aead::KEY_SIZE + aead::TAG_SIZE {
            return Err(format!(
                "OSL: wrap ciphertext unexpected length: got {}, want {}",
                wrap_ct.len(),
                aead::KEY_SIZE + aead::TAG_SIZE
            ));
        }

        let peer_pub_bytes = peer_pub.as_bytes();
        wire.push(peer_pub_bytes[0]);
        wire.extend_from_slice(nonce_k.as_bytes());
        wire.extend_from_slice(&wrap_ct);
    }

    wire.extend_from_slice(nonce_msg.as_bytes());
    wire.extend_from_slice(&ct_msg);

    if wire.len() != total_wire_len {
        return Err(format!(
            "OSL: internal wire-length mismatch: built {}, expected {}",
            wire.len(),
            total_wire_len
        ));
    }

    let stego_msg = stego::encode_mode0(&wire).map_err(|e| format!("OSL: stego encode: {e}"))?;
    if stego_msg.len() > 2000 {
        return Err(format!(
            "OSL: stego output {} chars exceeds Discord 2000-char message cap",
            stego_msg.len()
        ));
    }
    Ok(stego_msg)
}

/// Layer 10 / Phase 4 IPC entry point: encrypt `plaintext` for
/// the configured recipients of `channel_id` and return a Mode 0
/// stego cover string suitable for direct insertion as the
/// outbound Discord message body.
///
/// Orchestrates IO around the pure
/// [`encrypt_osl_phase4_to_pubkeys`]:
/// 1. Resolve `channel_id` → list of recipient `user_id`s via
///    [`keystore::get_recipients`].
/// 2. Lock [`AppState`] for the loaded identity + keyserver
///    client.
/// 3. Sort recipient `user_id`s ASCII for stable wire-slot order.
/// 4. Per-recipient: `KeyServerClient::fetch_pubkeys` →
///    decode IK_X25519 base64 → typed `PublicKey`.
/// 5. Hand the resolved pubkey vector to the pure encoder.
///
/// Returns `Result<String, String>` (not [`IpcResult`]) — see
/// `osl_encrypt_message` in `src-tauri/src/main.rs` for the
/// rationale (flat string error across the JS-bootloader
/// fail-closed boundary).
///
/// Failure modes (all fail-closed):
///
/// - missing identity / keyserver in [`AppState`]
/// - unconfigured / empty channel in `channels.json`
/// - per-recipient pubkey fetch / decode failure
/// - any error surfaced by [`encrypt_osl_phase4_to_pubkeys`]
pub fn cmd_osl_encrypt_message(
    state: &AppState,
    channel_id: String,
    plaintext: String,
    _options: serde_json::Value,
) -> Result<String, String> {
    let recipients =
        keystore::get_recipients(&channel_id).map_err(|e| format!("OSL: recipient lookup: {e}"))?;

    // Stable order: sort recipient ids ASCII so that the receiver
    // can scan slots deterministically.
    let mut sorted = recipients;
    sorted.sort();

    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard
        .as_ref()
        .ok_or_else(|| "OSL: key-server not initialised".to_string())?;

    let mut peer_pubkeys: Vec<x25519::PublicKey> = Vec::with_capacity(sorted.len());
    for user_id in &sorted {
        let resp = client.fetch_pubkeys(user_id).map_err(|e| {
            format!(
                "OSL: fetch_pubkeys({user_id}): {e}",
                user_id = crate::log_id::log_id(user_id)
            )
        })?;
        let peer_pub_vec = STANDARD.decode(&resp.ik_x25519_pub).map_err(|e| {
            format!(
                "OSL: decode peer pubkey ({user_id}): {e}",
                user_id = crate::log_id::log_id(user_id)
            )
        })?;
        if peer_pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: peer pubkey wrong length ({user_id}): got {}, want {}",
                peer_pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE,
                user_id = crate::log_id::log_id(user_id)
            ));
        }
        let mut peer_pub_bytes = [0u8; x25519::PUBLIC_KEY_SIZE];
        peer_pub_bytes.copy_from_slice(&peer_pub_vec);
        // Send-side diagnostic, mirroring the receive-side
        // `our_hint=…` / `hints=[…]` block. In dev builds, surface
        // each recipient's pubkey first byte AS FETCHED from the
        // keyserver (no client-side cache) so the user can sanity-
        // check against the keyserver's `ik_x25519_pub` field for
        // the same user_id. If these diverge across consecutive
        // sends, something is rotating mid-session.
        #[cfg(debug_assertions)]
        eprintln!(
            "[OSL] encrypt slot recipient={} pubkey_first_byte=0x{:02x}",
            user_id, peer_pub_bytes[0]
        );
        peer_pubkeys.push(x25519::PublicKey::from_bytes(peer_pub_bytes));
    }

    // Sender-side derived pub. Compare against keyserver-published
    // pub for our own user_id: divergence here means
    // `identity.x25519_public` (uploaded at register) drifted from
    // `derive_public(secret)`. `load_identity` self-heals at load
    // by re-deriving, but a session that started before the heal
    // landed could still surface this.
    #[cfg(debug_assertions)]
    {
        let derived_pub = x25519::derive_public(&identity.x25519_secret);
        let stored_first = identity.x25519_public.as_bytes()[0];
        let derived_first = derived_pub.as_bytes()[0];
        eprintln!(
            "[OSL] encrypt sender_user_id={} derived_first_byte=0x{:02x} \
             stored_first_byte=0x{:02x}{}",
            identity.user_id,
            derived_first,
            stored_first,
            if derived_first != stored_first {
                " DRIFT — register() uploaded stored, encrypt/decrypt use derived"
            } else {
                ""
            }
        );
    }

    encrypt_osl_phase4_to_pubkeys(&identity.x25519_secret, &peer_pubkeys, &plaintext)
}

// ---- Layer 10 / Phase 5: receive-side decoder + IPC command ----

/// Errors returned by the Phase 4 wire-format decoder.
///
/// `Display` strings are user-visible (the IPC bridge maps them
/// straight into the JS hook's reject path), so they're worded as
/// brief diagnostic phrases rather than internal-state dumps.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// The cover string didn't carry the `DPC0::` magic prefix —
    /// not an OSL message at all. The JS hook treats this as
    /// "leave content alone" and never surfaces it as a user
    /// error.
    #[error("cover string missing DPC0:: prefix")]
    BadPrefix,

    /// The base64 body of the cover string failed to decode.
    /// Fragmented send, truncation, or someone manually editing
    /// the cover. Renders as "Failed to decode" in the UI.
    #[error("base64 decode of cover body failed: {0}")]
    Base64(String),

    /// Wire bytes shorter than the minimum framing requires for
    /// the declared recipient count. Always corruption.
    #[error("wire too short: {got} bytes, expected at least {expected}")]
    TooShort { got: usize, expected: usize },

    /// Wire version byte didn't match Phase 4's `0x01`. Indicates
    /// either a future version (Phase 5+ ratchet wire) we can't
    /// yet decode, or junk.
    #[error("unsupported wire version 0x{got:02x} (this client only decodes 0x{expected:02x})")]
    UnsupportedVersion { got: u8, expected: u8 },

    /// Recipient count byte `N` was zero. A well-formed encoder
    /// rejects this; if we see it, the wire is junk.
    #[error("recipient count is zero in wire header")]
    ZeroRecipients,

    /// We are not a recipient of this message — no slot's wrap
    /// AEAD opened under our identity key. The JS hook treats
    /// this as "leave content alone" so non-recipients in a
    /// channel still see the cover string normally.
    #[error("not a recipient of this message")]
    NoMatchingSlot,

    /// A wrap slot opened (revealing a session key candidate),
    /// but the bulk message AEAD failed under that key. Indicates
    /// either a corrupted wire or a deliberate splice. Distinct
    /// from `NoMatchingSlot` for diagnostics.
    #[error("wrap slot opened but message AEAD failed: {0}")]
    MessageAeadFailed(String),

    /// Underlying X25519 / HKDF / AEAD primitive returned an
    /// error not otherwise classified. Surfaces the inner
    /// message verbatim — useful when debugging primitives, never
    /// triggers in normal operation.
    #[error("crypto primitive error: {0}")]
    Crypto(String),

    /// The sender's Discord user_id is not present in
    /// `peer_map.json`, so we can't translate it to an OSL
    /// user_id and the keyserver lookup would 404. Phase 5 v1's
    /// pre-decode resolution failure mode — distinct from
    /// `NoMatchingSlot` (where we ARE configured to talk to the
    /// sender but the message isn't addressed to us).
    ///
    /// JS hook treats this as "skip silently, leave cover in
    /// place" — same UX as `NoMatchingSlot` and `BadPrefix`. The
    /// `discord_id` is included so the hook can dedupe its
    /// onboarding-hint log to one line per unmapped sender
    /// rather than per message.
    #[error("no peer mapping for discord_id={discord_id} (add to peer_map.json)")]
    UnknownSender { discord_id: String },
}

/// Decode the Phase 4 wire-format raw bytes (post-`DPC0::`-strip,
/// post-base64-decode) into the recovered plaintext bytes.
///
/// Pure: takes pre-resolved sender + recipient keys, no `AppState`,
/// no IO. Tests exercise this directly with hand-built keypairs.
///
/// # Constant-time-ish slot iteration
///
/// The loop runs **all** slots that match our `pub_hint` —
/// it does not break on first successful unwrap. Two slots
/// could share a `pub_hint` byte (1/256 probability per
/// collision), and breaking early would let a timing-aware
/// observer narrow down which slot is ours. The cost is one
/// extra AEAD attempt per legitimate hint collision; usually
/// zero such collisions in practice.
///
/// We do still skip slots whose `pub_hint` doesn't match ours.
/// The `pub_hint` is public information (sender writes the
/// recipient's public-key low byte into the wire), so iterating
/// over non-matching slots is wasted work, not a leak.
///
/// We do not (and cannot reasonably) make "are we a recipient at
/// all?" constant-time relative to "we are a recipient" — those
/// states are externally observable via whether we re-dispatch a
/// `MESSAGE_UPDATE` afterwards.
pub fn decrypt_osl_phase4_from_wire(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    wire: &[u8],
) -> Result<Vec<u8>, DecodeError> {
    if wire.len() < OSL_PHASE4_FIXED_FRAMING_BYTES {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: OSL_PHASE4_FIXED_FRAMING_BYTES,
        });
    }
    let version = wire[0];
    if version != OSL_PHASE4_WIRE_VERSION {
        return Err(DecodeError::UnsupportedVersion {
            got: version,
            expected: OSL_PHASE4_WIRE_VERSION,
        });
    }
    let n = wire[1] as usize;
    if n == 0 {
        return Err(DecodeError::ZeroRecipients);
    }
    let expected_min = OSL_PHASE4_FIXED_FRAMING_BYTES + n * OSL_PHASE4_PER_RECIPIENT_BYTES;
    if wire.len() < expected_min {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: expected_min,
        });
    }

    // Compute receiver's own pub_hint to find candidate slots.
    let recipient_pub = x25519::derive_public(recipient_secret);
    let our_hint = recipient_pub.as_bytes()[0];

    // Recover the shared secret + wrap key once — every slot
    // belonging to us derives from the same `(recipient_sk,
    // sender_pk)` pair.
    let shared = x25519::diffie_hellman(recipient_secret, sender_pub)
        .map_err(|e| DecodeError::Crypto(format!("ECDH: {e}")))?;
    let wrap_key_bytes = hkdf::derive_32(&[], shared.as_bytes(), OSL_PHASE4_HKDF_INFO_WRAP)
        .map_err(|e| DecodeError::Crypto(format!("HKDF wrap-key: {e}")))?;
    let wrap_key = aead::Key::from_bytes(wrap_key_bytes);

    // Walk all slots; for any with a matching `pub_hint`, attempt
    // wrap-decrypt. Don't break on first success — see the
    // "constant-time-ish" note in the docstring above.
    let slot_size = OSL_PHASE4_PER_RECIPIENT_BYTES;
    let mut session_key: Option<aead::Key> = None;
    for slot_ix in 0..n {
        let base = 2 + slot_ix * slot_size;
        let hint = wire[base];
        if hint != our_hint {
            continue;
        }
        let nonce_start = base + 1;
        let nonce_end = nonce_start + aead::NONCE_SIZE;
        let wrap_start = nonce_end;
        let wrap_end = wrap_start + aead::KEY_SIZE + aead::TAG_SIZE;
        let mut nonce_bytes = [0u8; aead::NONCE_SIZE];
        nonce_bytes.copy_from_slice(&wire[nonce_start..nonce_end]);
        let nonce = aead::Nonce::from_bytes(nonce_bytes);
        let wrap_ct = &wire[wrap_start..wrap_end];

        if let Ok(plaintext_bytes) = aead::open(&wrap_key, &nonce, OSL_PHASE4_AD_WRAP, wrap_ct) {
            if plaintext_bytes.len() == aead::KEY_SIZE && session_key.is_none() {
                let mut sk = [0u8; aead::KEY_SIZE];
                sk.copy_from_slice(&plaintext_bytes);
                session_key = Some(aead::Key::from_bytes(sk));
                // Deliberately no `break` — see docstring.
            }
            // Wrong-length plaintext from a "successful" open is
            // pathological (AEAD tag matched against a corrupted
            // body). Treat as not-our-slot and keep going.
        }
        // Failed AEAD: not our slot under this hint collision; keep going.
    }
    let session_key = session_key.ok_or(DecodeError::NoMatchingSlot)?;

    // Bulk message decrypt. Position is fixed: nonce at
    // `2 + n * slot_size`, ciphertext to end of wire.
    let msg_nonce_start = 2 + n * slot_size;
    let msg_nonce_end = msg_nonce_start + aead::NONCE_SIZE;
    if wire.len() < msg_nonce_end + aead::TAG_SIZE {
        return Err(DecodeError::TooShort {
            got: wire.len(),
            expected: msg_nonce_end + aead::TAG_SIZE,
        });
    }
    let mut msg_nonce_bytes = [0u8; aead::NONCE_SIZE];
    msg_nonce_bytes.copy_from_slice(&wire[msg_nonce_start..msg_nonce_end]);
    let msg_nonce = aead::Nonce::from_bytes(msg_nonce_bytes);
    let ct_msg = &wire[msg_nonce_end..];
    aead::open(&session_key, &msg_nonce, OSL_PHASE4_AD_MSG, ct_msg)
        .map_err(|e| DecodeError::MessageAeadFailed(e.to_string()))
}

/// Higher-level decoder: takes the on-the-wire `DPC0::<base64>`
/// cover string, strips the prefix, base64-decodes the body, then
/// hands off to [`decrypt_osl_phase4_from_wire`].
///
/// Returns `BadPrefix` for non-OSL content so the JS hook can
/// trivially distinguish "this isn't ours, leave it alone" from
/// "this is ours but we're not a recipient." Same effective UX
/// (cover stays visible) but useful for log-grep separation.
pub fn decrypt_osl_phase4_cover(
    recipient_secret: &x25519::SecretKey,
    sender_pub: &x25519::PublicKey,
    cover: &str,
) -> Result<Vec<u8>, DecodeError> {
    let body = cover.strip_prefix("DPC0::").ok_or(DecodeError::BadPrefix)?;
    let wire = STANDARD
        .decode(body)
        .map_err(|e| DecodeError::Base64(e.to_string()))?;
    decrypt_osl_phase4_from_wire(recipient_secret, sender_pub, &wire)
}

/// Layer 10 / Phase 5 IPC entry point: decrypt an incoming
/// Discord message back to plaintext.
///
/// Takes `sender_discord_id` — the raw Discord snowflake the
/// boot.js receive observer pulled out of the message DOM
/// (`data-author-id`, avatar URL, etc.). Discord IDs aren't
/// keyserver identifiers, so we resolve to OSL `user_id` via
/// `AppState::peer_map` (loaded at bootstrap from
/// `<osl_config_dir>/peer_map.json`) before any keyserver call.
///
/// Caller (the JS hook on `MESSAGE_CREATE`) is expected to:
/// 1. Pre-filter on the `DPC0::` prefix (so this command isn't
///    invoked for every message — the prefix scan in JS is far
///    cheaper than crossing the IPC bridge).
/// 2. Pass the Discord `message.author.id` as `sender_discord_id`.
/// 3. Render the returned plaintext in place of the cover when
///    `Ok(_)` is returned.
/// 4. Leave the cover visible when `Err(_)` returns. Failure
///    branches: peer not in map (`UnknownSender`), not a recipient
///    (`NoMatchingSlot`), key rotation we haven't refetched yet
///    (`MessageAeadFailed`), wire corruption (`TooShort` /
///    `BadPrefix` / etc.).
///
/// Returns `Result<String, String>` (matching encrypt's wire
/// shape). On success, the plaintext is interpreted as UTF-8 and
/// returned verbatim. Non-UTF-8 plaintext returns a
/// `OSL: invalid UTF-8` error — the encoder accepts only UTF-8
/// input, so this should never trigger absent corruption.
///
/// Sender pubkey resolution is cached per `AppState`'s
/// [`crate::state::SenderPubkeyCache`] (30-minute TTL); first hit
/// per sender per window pays a keyserver round-trip, subsequent
/// hits are local. Cache is keyed by **OSL user_id** (post peer-
/// map resolution), not Discord id, so re-mapping a discord_id to
/// a different OSL identity in `peer_map.json` doesn't pollute
/// the cache.
///
/// `_channel_id` is currently unused — the recipient mapping is
/// not channel-keyed on the receive side (any message we can
/// decrypt belongs to us regardless of which channel it landed
/// in). Carried in the IPC signature for symmetry with encrypt
/// and so future per-channel ratchet state can plug in without a
/// wire change.
pub fn cmd_osl_decrypt_message(
    state: &AppState,
    channel_id: String,
    sender_discord_id: String,
    content: String,
) -> Result<String, String> {
    cmd_osl_decrypt_message_with_id(state, None, channel_id, sender_discord_id, content)
}

/// Same as [`cmd_osl_decrypt_message`] but accepts an optional
/// `discord_message_id`. When `Some`, the decrypted plaintext is
/// persisted to [`crate::state::AppState::message_store`] (Phase
/// 5b2). When `None`, the decrypt path runs unchanged with no
/// persistence side-effect (Phase 5b3 will wire boot.js to send
/// the id, at which point persistence becomes the default).
///
/// Persistence failures are logged and swallowed: they never
/// turn a successful decrypt into a user-visible error. The
/// receive-side rendering path is the source of truth for "did
/// it work?"; the store is a best-effort durability layer.
pub fn cmd_osl_decrypt_message_with_id(
    state: &AppState,
    discord_message_id: Option<String>,
    channel_id: String,
    sender_discord_id: String,
    content: String,
) -> Result<String, String> {
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;

    // Discord-id → OSL-user-id translation. Missing mapping is
    // common (every non-peer in a channel triggers it) and is
    // handled silently by the JS hook — surface a typed
    // UnknownSender so the hook can dedupe its log.
    //
    // RECEIVE-PATH GUARANTEE (deliberate): an unmapped sender
    // returns UnknownSender and does NOT consult the keyserver.
    // This is a privacy property — we never emit a keyserver
    // lookup ("received an OSL message from snowflake X") for a
    // sender we have no mapping for, and it's attacker-pokable via
    // junk DPC0:: strings otherwise. The cross-machine fix is
    // send-side + v3/v4 (sender key in-wire / local ratchet), so
    // receive never needs a keyserver sender lookup; do NOT default
    // osl_user_id to the snowflake here.
    let osl_user_id = {
        let map_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        match map_guard
            .get(&sender_discord_id)
            .and_then(|e| e.osl_user_id.clone())
        {
            Some(v) => v,
            None => {
                return Err(format!(
                    "OSL: {}",
                    DecodeError::UnknownSender {
                        discord_id: sender_discord_id,
                    }
                ));
            }
        }
    };

    // Pubkey lookup: cache → keyserver → cache-insert. Keyed by
    // OSL user_id (post-resolution) so the cache is stable across
    // peer_map re-edits.
    let sender_pub = if let Some(cached) = state.sender_pubkey_cache.get(&osl_user_id) {
        cached
    } else {
        if is_discord_snowflake_shaped(&osl_user_id) {
            return Err("OSL: Discord identifiers cannot resolve keys".to_string());
        }
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        let resp = client.fetch_pubkeys(&osl_user_id).map_err(|e| {
            format!(
                "OSL: fetch_pubkeys({osl_user_id}): {e}",
                osl_user_id = crate::log_id::log_id(&osl_user_id)
            )
        })?;
        let pub_vec = STANDARD.decode(&resp.ik_x25519_pub).map_err(|e| {
            format!(
                "OSL: decode sender pubkey ({osl_user_id}): {e}",
                osl_user_id = crate::log_id::log_id(&osl_user_id)
            )
        })?;
        if pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: sender pubkey wrong length ({osl_user_id}): got {}, want {}",
                pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE,
                osl_user_id = crate::log_id::log_id(&osl_user_id)
            ));
        }
        let mut bytes = [0u8; x25519::PUBLIC_KEY_SIZE];
        bytes.copy_from_slice(&pub_vec);
        let pub_key = x25519::PublicKey::from_bytes(bytes);
        // Drop the keyserver lock before inserting into the cache
        // (the cache has its own mutex).
        drop(ks_guard);
        state
            .sender_pubkey_cache
            .insert(osl_user_id.clone(), pub_key);
        pub_key
    };

    let plaintext_bytes =
        match decrypt_osl_phase4_cover(&identity.x25519_secret, &sender_pub, &content) {
            Ok(bytes) => bytes,
            Err(DecodeError::NoMatchingSlot) => {
                // Diagnostic: when NoMatchingSlot fires, surface the
                // wire's slot hints alongside our recipient hint so a
                // post-mortem can tell hint-mismatch (we're really
                // not a recipient) apart from
                // hint-match-but-AEAD-failed (key disagreement —
                // which static-static ECDH should never produce
                // intermittently). Falls back gracefully if the cover
                // is ill-formed.
                let recipient_pub = x25519::derive_public(&identity.x25519_secret);
                let our_hint = recipient_pub.as_bytes()[0];
                let diag = decode_slot_diagnostic(&content);
                return Err(format!(
                    "OSL: not a recipient of this message \
                 [diag: our_hint=0x{our_hint:02x} {diag} osl_user_id={osl_user_id}]",
                    osl_user_id = crate::log_id::log_id(&osl_user_id)
                ));
            }
            Err(e) => return Err(format!("OSL: {e}")),
        };
    let plaintext = String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: decrypted plaintext is not valid UTF-8".to_string())?;

    // Drop the identity guard before touching the store mutex so
    // the two locks never overlap — keeps the lock graph trivially
    // free of cycles even when future callers hold both.
    drop(id_guard);

    if let Some(message_id) = discord_message_id {
        persist_decrypted(
            state,
            message_id,
            channel_id,
            sender_discord_id,
            osl_user_id,
            &plaintext,
        );
    }

    Ok(plaintext)
}

#[cfg(test)]
mod legacy_v1_decrypt_sender_binding_tests {
    use super::*;

    #[test]
    fn legacy_v1_decrypt_fails_closed_under_forged_sender() {
        let recipient = generate_identity("recipient-osl".to_string());
        let alice = generate_identity("alice-osl".to_string());
        let bob = generate_identity("bob-osl".to_string());
        let plaintext = "legacy v1 sender binding proof";

        let cover =
            encrypt_osl_phase4_to_pubkeys(&bob.x25519_secret, &[recipient.x25519_public], plaintext)
                .expect("valid legacy v1 cover");

        let state = AppState::new();
        *state.identity.lock().expect("identity mutex poisoned") = Some(recipient);
        {
            let mut peers = state.peer_map.lock().expect("peer_map mutex poisoned");
            peers.insert(
                "alice-discord".to_string(),
                crate::peer_map::legacy_entry(alice.user_id.clone()),
            );
            peers.insert(
                "bob-discord".to_string(),
                crate::peer_map::legacy_entry(bob.user_id.clone()),
            );
        }
        state
            .sender_pubkey_cache
            .insert(alice.user_id.clone(), alice.x25519_public);
        state
            .sender_pubkey_cache
            .insert(bob.user_id.clone(), bob.x25519_public);

        let opened = cmd_osl_decrypt_message(
            &state,
            "channel".to_string(),
            "bob-discord".to_string(),
            cover.clone(),
        )
        .expect("control decrypt with the real sender should open");
        assert_eq!(opened, plaintext);

        let err = cmd_osl_decrypt_message(
            &state,
            "channel".to_string(),
            "alice-discord".to_string(),
            cover,
        )
        .expect_err("forged claimed sender must fail closed");

        assert!(
            err.contains("not a recipient of this message"),
            "unexpected error: {err}"
        );
    }
}

/// Best-effort persistence of a freshly decrypted message into
/// [`crate::state::AppState::message_store`]. Logs and swallows
/// every failure so a store outage cannot regress decrypt UX.
///
/// Skipped silently when the store is `None` (open failed at
/// bootstrap, or the user is running with persistence disabled).
fn persist_decrypted(
    state: &AppState,
    discord_message_id: String,
    channel_id: String,
    sender_discord_id: String,
    sender_osl_user_id: String,
    plaintext: &str,
) {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        tracing::debug!(
            discord_message_id = %crate::log_id::log_id(&discord_message_id),
            "OSL: message_store disabled; skipping persistence"
        );
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let msg = StoredMessage {
        discord_message_id: discord_message_id.clone(),
        channel_id,
        sender_discord_id,
        sender_osl_user_id,
        plaintext: plaintext.to_string(),
        decrypted_at: now,
        burned: false,
    };
    if let Err(e) = store.put(&msg) {
        tracing::warn!(
            discord_message_id = %crate::log_id::log_id(&discord_message_id),
            error = %e,
            "OSL: message_store.put failed; decrypt UX unaffected"
        );
    }
}

/// Probe-3 fix: gated wrapper around `persist_decrypted` for use by
/// the v=2 / v=3 / v=4 / v=5 receive paths. Skips persistence when
/// the "plaintext" is actually a control sentinel (BURN_APPLIED,
/// SKDM_APPLIED, RECOVERY_IGNORED, SESSION_RESET_APPLIED,
/// ATTACHMENT envelope, LEGACY_HANDSHAKE_IGNORED, MODE1 sentinels,
/// SKDM_REREQUEST prefix) or when the caller didn't supply a
/// message id (history backfill / debug invocations).
///
/// Without this wrapper, the only persist call site was the v=1
/// legacy path (line ~1893), so v=2 / v=3 / v=4 / v=5 inbound
/// messages decrypted successfully but were NEVER written to the
/// durable MessageStore -- on relaunch, `recvLoadHistory` returned
/// an empty list for the channel, every visible message had to
/// re-decrypt from scratch, and any message whose receiver chain or
/// ratchet state had since rotated was unrecoverable. This was the
/// "doesn't save on reopen" symptom.
fn persist_user_plaintext(
    state: &AppState,
    discord_message_id: Option<&str>,
    channel_id: &str,
    sender_discord_id: &str,
    plaintext: &str,
) {
    // Sentinel strings all start with `__OSL_CONTROL_`; the attachment
    // sentinel uses the same prefix via OSL_RESULT_ATTACHMENT_PREFIX,
    // so one starts-with check covers every non-content return path.
    if plaintext.starts_with("__OSL_CONTROL_") {
        return;
    }
    let Some(id) = discord_message_id else {
        return;
    };
    let sender_osl_user_id = state
        .peer_map
        .lock()
        .expect("peer_map mutex poisoned")
        .get(sender_discord_id)
        .and_then(|e| e.osl_user_id.clone())
        .unwrap_or_else(|| sender_discord_id.to_string());
    persist_decrypted(
        state,
        id.to_string(),
        channel_id.to_string(),
        sender_discord_id.to_string(),
        sender_osl_user_id,
        plaintext,
    );
}

/// Probe-2 fix: persist a freshly-sent outbound message so it
/// survives a session restart.
///
/// Before this, outbound plaintext was held only in two in-memory
/// JS Maps (`selfSentPlaintext`, `oslSentWireToPlaintext`). On app
/// restart those were empty, the decrypt dispatcher ran on the user's
/// own v=4 wire, and v=4 correctly returned "not a recipient" because
/// a single-peer Double Ratchet message is encrypted only to the
/// peer's key, never to the sender's. The sender's own messages
/// therefore re-rendered as ciphertext after every restart. This IPC
/// closes the loop by persisting outbound plaintext through the same
/// `MessageStore` the decrypt path uses, so `recvLoadHistory`'s
/// rehydration covers own messages too.
///
/// The row is written with `sender_discord_id` and `sender_osl_user_id`
/// both set to the in-state identity's `user_id` (the Discord
/// snowflake in normal use). Best-effort: persistence-disabled
/// (`message_store == None`), missing identity, or store failures are
/// logged and swallowed — the send itself already succeeded; failing
/// to persist would only confuse the JS layer.
pub fn cmd_osl_persist_outbound(
    state: &AppState,
    channel_id: String,
    discord_message_id: String,
    plaintext: String,
) -> Result<(), String> {
    let self_id = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        match guard.as_ref() {
            Some(id) => id.user_id.clone(),
            None => {
                tracing::debug!(
                    discord_message_id = %crate::log_id::log_id(&discord_message_id),
                    "OSL: persist_outbound: identity not loaded; skipping"
                );
                return Ok(());
            }
        }
    };
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        tracing::debug!(
            discord_message_id = %crate::log_id::log_id(&discord_message_id),
            "OSL: persist_outbound: message_store disabled; skipping"
        );
        return Ok(());
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let msg = StoredMessage {
        discord_message_id: discord_message_id.clone(),
        channel_id,
        sender_discord_id: self_id.clone(),
        sender_osl_user_id: self_id,
        plaintext,
        decrypted_at: now,
        burned: false,
    };
    if let Err(e) = store.put(&msg) {
        tracing::warn!(
            discord_message_id = %crate::log_id::log_id(&discord_message_id),
            error = %e,
            "OSL: persist_outbound: store.put failed (non-fatal)"
        );
    }
    Ok(())
}

/// Persist plaintext already authenticated by a trusted first-party OSL Chat
/// receive path. This is an internal Rust API, not a renderer command: the
/// caller must derive the sender and channel from the verified peer context.
pub fn cmd_osl_persist_inbound(
    state: &AppState,
    channel_id: String,
    message_id: String,
    sender_osl_user_id: String,
    plaintext: String,
) -> Result<(), String> {
    if channel_id.is_empty()
        || channel_id.len() > 160
        || message_id.is_empty()
        || message_id.len() > 96
        || sender_osl_user_id.is_empty()
        || sender_osl_user_id.len() > 160
        || plaintext.is_empty()
    {
        return Err("OSL: invalid first-party chat history row".to_owned());
    }
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    store
        .put(&StoredMessage {
            discord_message_id: message_id,
            channel_id,
            sender_discord_id: sender_osl_user_id.clone(),
            sender_osl_user_id,
            plaintext,
            decrypted_at: now,
            burned: false,
        })
        .map_err(|error| format!("OSL: first-party chat history: {error}"))
}

/// JS-facing DTO mirror of [`store::StoredMessage`]. The store
/// crate intentionally does not depend on `serde` (it's a pure
/// at-rest layer); this DTO crosses the IPC boundary and is the
/// shape boot.js sees on `osl_load_channel_history`.
#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct StoredMessageDto {
    pub discord_message_id: String,
    pub channel_id: String,
    pub sender_discord_id: String,
    pub sender_osl_user_id: String,
    pub plaintext: String,
    pub decrypted_at: i64,
    pub burned: bool,
}

impl From<StoredMessage> for StoredMessageDto {
    fn from(m: StoredMessage) -> Self {
        StoredMessageDto {
            discord_message_id: m.discord_message_id,
            channel_id: m.channel_id,
            sender_discord_id: m.sender_discord_id,
            sender_osl_user_id: m.sender_osl_user_id,
            plaintext: m.plaintext,
            decrypted_at: m.decrypted_at,
            burned: m.burned,
        }
    }
}

/// Default cap for [`cmd_osl_load_channel_history`] when the
/// caller passes `None`. Sized for a typical Discord channel
/// scrollback view (~one screen of messages).
pub const OSL_LOAD_HISTORY_DEFAULT_LIMIT: u32 = 100;

/// Layer 10 / Phase 5b2 IPC entry point: list previously
/// decrypted messages for a channel from the persistent store,
/// newest-first.
///
/// Returns an empty vector (not an error) when the store is
/// `None` — boot.js treats that as "no history to render" and
/// proceeds normally. Any other store error surfaces to the
/// caller as `Err(_)`.
///
/// `limit` defaults to [`OSL_LOAD_HISTORY_DEFAULT_LIMIT`] when
/// `None`. Callers may pass a higher cap if they need bulk
/// scrollback rehydration; the store's `list_by_channel`
/// streams the rows, so memory pressure scales with the cap.
pub fn cmd_osl_load_channel_history(
    state: &AppState,
    channel_id: String,
    limit: Option<u32>,
) -> Result<Vec<StoredMessageDto>, String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(Vec::new());
    };
    let cap = limit.unwrap_or(OSL_LOAD_HISTORY_DEFAULT_LIMIT);
    let rows = store
        .list_by_channel(&channel_id, cap)
        .map_err(|e| format!("OSL: list_by_channel: {e}"))?;
    Ok(rows.into_iter().map(StoredMessageDto::from).collect())
}

/// Largest decrypted attachment we persist to the local cache.
/// Images are worth caching; multi-MB videos aren't (they bloat the
/// DB and re-fetch quickly enough). 8 MiB comfortably covers photos.
const OSL_ATTACHMENT_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Soft cap on cached attachment rows; trimmed oldest-first.
const OSL_ATTACHMENT_CACHE_KEEP: u32 = 600;

/// Beta 1.0: persist a decrypted attachment's bytes to the local
/// sealed store so a channel re-entry / restart rehydrates the image
/// without a CDN re-fetch + re-decrypt. No-op when persistence is
/// disabled, the bytes exceed the size cap, or base64 is malformed.
pub fn cmd_osl_attachment_cache_put(
    state: &AppState,
    discord_message_id: String,
    random_filename: String,
    mime: String,
    bytes_b64: String,
    // Scope + sender so a burn can wipe the burner's cached attachments.
    scope_input: Option<crate::scope::ScopeInput>,
    sender_discord_id: Option<String>,
) -> Result<(), String> {
    let bytes = STANDARD
        .decode(bytes_b64.as_bytes())
        .map_err(|e| format!("OSL: attachment_cache_put base64: {e}"))?;
    if bytes.len() > OSL_ATTACHMENT_CACHE_MAX_BYTES {
        tracing::debug!(
            msg_id = %crate::log_id::log_id(&discord_message_id),
            len = bytes.len(),
            "OSL: attachment_cache_put: over size cap; skipping"
        );
        return Ok(());
    }
    // Resolve scope_type/scope_id for the burn-wipe filter (best-effort).
    let (scope_type, scope_id): (Option<String>, Option<String>) = match scope_input {
        Some(input) => match crate::scope::Scope::try_from(input) {
            Ok(scope) => {
                let (t, i) = scope_storage_pair(&scope);
                (Some(t), Some(i))
            }
            Err(_) => (None, None),
        },
        None => (None, None),
    };
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    store
        .put_attachment(
            &discord_message_id,
            &random_filename,
            &mime,
            &bytes,
            scope_type.as_deref(),
            scope_id.as_deref(),
            sender_discord_id.as_deref(),
        )
        .map_err(|e| format!("OSL: put_attachment: {e}"))?;
    // Best-effort trim so the cache stays bounded. Cheap (one DELETE).
    let _ = store.trim_attachments(OSL_ATTACHMENT_CACHE_KEEP);
    Ok(())
}

/// Beta 1.0: fetch a previously-cached decrypted attachment. Returns
/// `None` (the JS side then fetches + decrypts from the CDN) when
/// persistence is disabled or the attachment isn't cached.
pub fn cmd_osl_attachment_cache_get(
    state: &AppState,
    discord_message_id: String,
    random_filename: String,
) -> Result<Option<AttachmentCacheDto>, String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(None);
    };
    match store
        .get_attachment(&discord_message_id, &random_filename)
        .map_err(|e| format!("OSL: get_attachment: {e}"))?
    {
        Some((mime, bytes)) => Ok(Some(AttachmentCacheDto {
            mime,
            bytes_b64: STANDARD.encode(&bytes),
        })),
        None => Ok(None),
    }
}

/// JS-facing decrypted-attachment payload from the local cache.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentCacheDto {
    pub mime: String,
    pub bytes_b64: String,
}

/// Layer 10 / Phase 6a IPC entry point: re-persist a stored
/// message under a new plaintext after the user edited it
/// through Discord's edit flow.
///
/// Boot.js calls this from the PATCH-response load listener
/// once the edit's outbound network leg succeeds. The flow is:
///
/// 1. User edits a `DPC0::` message in Discord.
/// 2. Boot.js intercepts the PATCH, swaps `content` for a
///    fresh `DPC0::<base64>` cover, lets the request continue.
/// 3. Discord's response acknowledges the edit (200/204).
/// 4. Load listener calls this IPC with the *plaintext the
///    user typed* and the message_id from the URL.
///
/// On a known id: looks up the existing row to preserve
/// channel_id + sender_discord_id + sender_osl_user_id, then
/// upserts with `new_plaintext` and a fresh `decrypted_at`
/// (treating the edit time as the new "decrypted at" since
/// that's the moment the local store learned this plaintext).
/// `burned` is preserved as `false` — burned rows are filtered
/// from `store.get` so we'd already be on the unknown-id path
/// for those.
///
/// On an unknown id: idempotent no-op returning `Ok(())`. The
/// 2-arg signature can't construct a complete row without
/// channel/sender metadata, and the receive observer's normal
/// decrypt-and-persist path handles edit-before-decrypt
/// (which is exotic anyway — we'd have to have edited a
/// message we never saw bounce back, or one whose row was
/// burned). Surfacing an error here would only confuse the
/// boot.js caller, since the receive observer is also racing
/// to persist the same edit through the regular path.
///
/// Persistence is disabled when `state.message_store` is
/// `None`; we return `Ok(())` for the same reason
/// `cmd_osl_burn_message` does.
pub fn cmd_osl_persist_edit(
    state: &AppState,
    discord_message_id: String,
    new_plaintext: String,
    channel_id: Option<String>,
) -> Result<(), String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    let existing = store
        .get(&discord_message_id)
        .map_err(|e| format!("OSL: persist_edit get: {e}"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let updated = match existing {
        Some(prior) => StoredMessage {
            discord_message_id: prior.discord_message_id,
            channel_id: prior.channel_id,
            sender_discord_id: prior.sender_discord_id,
            sender_osl_user_id: prior.sender_osl_user_id,
            plaintext: new_plaintext,
            decrypted_at: now,
            burned: false,
        },
        None => {
            // Probe-2 fix: was a silent no-op when row missing, which
            // bricked editing of any outbound message whose row had
            // never been persisted (every outbound row pre-fix, since
            // outbound persistence didn't exist). With `channel_id`
            // supplied, treat the edit as the first persistence
            // moment for an own outbound message and upsert as self.
            // Without `channel_id` we lack a complete row — preserve
            // the historical idempotent no-op.
            let Some(channel_id) = channel_id else {
                return Ok(());
            };
            let self_id = {
                let id_guard = state.identity.lock().expect("identity mutex poisoned");
                let Some(id) = id_guard.as_ref() else {
                    return Ok(());
                };
                id.user_id.clone()
            };
            StoredMessage {
                discord_message_id: discord_message_id.clone(),
                channel_id,
                sender_discord_id: self_id.clone(),
                sender_osl_user_id: self_id,
                plaintext: new_plaintext,
                decrypted_at: now,
                burned: false,
            }
        }
    };
    store
        .put(&updated)
        .map_err(|e| format!("OSL: persist_edit put: {e}"))?;
    Ok(())
}

/// Layer 10 / Phase 5b2 IPC entry point: mark a message burned
/// in the persistent store. Subsequent `osl_load_channel_history`
/// calls will not return it, and `get`-style lookups skip it.
///
/// Idempotent: a burn against a non-existent
/// `discord_message_id` returns `Ok(())` (the row is gone or
/// was never persisted; either way the caller's intent — "this
/// message must not surface from the store" — is satisfied).
/// All other store errors surface as `Err(_)`.
///
/// Returns `Ok(())` (no-op) when the store is `None` so a UI
/// burn button doesn't error against a persistence-disabled
/// session.
pub fn cmd_osl_burn_message(state: &AppState, discord_message_id: String) -> Result<(), String> {
    let guard = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned");
    let Some(store) = guard.as_ref() else {
        return Ok(());
    };
    match store.mark_burned(&discord_message_id) {
        Ok(()) => Ok(()),
        Err(StoreError::NotFound(_)) => Ok(()),
        Err(e) => Err(format!("OSL: mark_burned: {e}")),
    }
}

/// Pull diagnostic facts out of a Phase 4 cover string for the
/// NoMatchingSlot error path. Returns a single-line summary like
/// `version=0x01 N=2 hints=[0xab,0xcd]`, OR a fallback string
/// describing why the wire couldn't be inspected. Never fails —
/// designed to be safe to call on attacker-controlled covers.
///
/// **Information leak posture.** Slot hints are public (the
/// sender writes them in the clear) and our recipient hint is a
/// derived byte of our public identity key. Both are already
/// observable to anyone watching the channel, so surfacing them
/// in our own logs costs nothing.
fn decode_slot_diagnostic(cover: &str) -> String {
    let body = match cover.strip_prefix("DPC0::") {
        Some(b) => b,
        None => return "wire=<no DPC0:: prefix>".to_string(),
    };
    let raw = match STANDARD.decode(body) {
        Ok(r) => r,
        Err(e) => return format!("wire=<base64 error: {e}>"),
    };
    if raw.len() < 2 {
        return format!("wire=<too short: {} bytes>", raw.len());
    }
    let version = raw[0];
    let n = raw[1] as usize;
    let slot_size = OSL_PHASE4_PER_RECIPIENT_BYTES;
    let needed = OSL_PHASE4_FIXED_FRAMING_BYTES + n * slot_size;
    if raw.len() < needed {
        return format!(
            "wire=<truncated: have {} bytes, need {} for N={}>",
            raw.len(),
            needed,
            n
        );
    }
    let mut hints = String::with_capacity(2 + n * 5);
    hints.push('[');
    for slot_ix in 0..n {
        if slot_ix > 0 {
            hints.push(',');
        }
        let base = 2 + slot_ix * slot_size;
        hints.push_str(&format!("0x{:02x}", raw[base]));
    }
    hints.push(']');
    format!("version=0x{version:02x} N={n} hints={hints}")
}

// ---- Phase 7b: wire v=2 send-path commands ----
//
// All five send-path commands share the same shape: take an
// AppState reference + the caller's intent, construct a v=2 wire
// blob, and return it as a string for boot.js to ship through
// Discord's API. Persistence (writing the on-disk
// peer_map/whitelist_state/pending_invitations side-effects) is
// handled by separate "apply" commands; the send-side stays
// stateless beyond reading current state.

/// Helper: resolve the on-disk pubkey for a single Discord id, or
/// surface a stable string error suitable for boot.js logging.
fn lookup_peer_pubkey(
    peer_map: &crate::peer_map::PeerMap,
    discord_id: &str,
) -> Result<crypto::x25519::PublicKey, String> {
    let entry = peer_map.get(discord_id).ok_or_else(|| {
        format!(
            "OSL: no peer entry for discord_id={discord_id}",
            discord_id = crate::log_id::log_id(discord_id)
        )
    })?;
    let b64 = entry.pubkey.as_deref().ok_or_else(|| {
        format!(
            "OSL: no pubkey for discord_id={discord_id}",
            discord_id = crate::log_id::log_id(discord_id)
        )
    })?;
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| format!("OSL: peer pubkey base64 decode failed: {e}"))?;
    if bytes.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: peer pubkey length {} != {}",
            bytes.len(),
            crypto::x25519::PUBLIC_KEY_SIZE
        ));
    }
    let mut arr = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    arr.copy_from_slice(&bytes);
    Ok(crypto::x25519::PublicKey::from_bytes(arr))
}

/// Current unix-seconds timestamp, falling back to 0 if the clock
/// is somehow before the epoch.
fn now_unix_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Phase 9-B1: shaped output for the send pipeline.
///
/// - `messages` — the cover strings to drop into Discord. Mode 0
///   ships exactly one `DPC0::<b64>` element; Mode 1 ships one or
///   more `DPC1::<sentences>` elements, each carrying one
///   authenticated chunk of the underlying wire bytes.
/// - `session_id` — `Some(_)` only in Mode 1, exposing the random
///   chunk-session id for UI bookkeeping (e.g. progress badges).
///
/// 9-MODE1-FIX: `preview_required` field removed. Mode 1 sends fire
/// chunks immediately with no user-facing confirmation modal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptOutput {
    pub messages: Vec<String>,
    pub session_id: Option<u32>,
    /// Phase 9-A3 SKDM-delivery fix: v=5 group sends produce one
    /// SKDM (Sender Key Distribution Message) v=4 wire per non-self
    /// peer that boot.js must post as its OWN Discord message(s) —
    /// distinct from `messages` (which boot.js treats as Mode-0/1
    /// CONTENT and would reject if >1). Empty for v=3 / v=4-DM
    /// sends. `#[serde(default)]` so old shapes deserialize and the
    /// single-message Mode-0 path is unaffected; always serialized
    /// (possibly `[]`) so boot.js can iterate unconditionally.
    #[serde(default)]
    pub control_messages: Vec<String>,
    /// Per-peer SKDM dispatch outcome (fail-closed policy: a failed
    /// SKDM does NOT abort the content send; boot.js surfaces a
    /// user-visible notice naming the affected peer(s)). Empty for
    /// non-v=5 sends.
    #[serde(default)]
    pub skdm_peer_status: Vec<SkdmPeerStatus>,
}

/// Per-peer outcome of a v=5 SKDM dispatch attempt. Surfaced all
/// the way to boot.js so a failed SKDM names the affected peer in a
/// user-visible notice rather than failing silently (the bug this
/// fixes: the SKDM wire used to be discarded entirely).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkdmPeerStatus {
    pub peer_discord_id: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Internal return of [`cmd_osl_encrypt_message_v2_wire`]: the
/// CONTENT wire plus any v=5 SKDM control wires that must be posted
/// as their own Discord messages, and the per-peer dispatch status.
/// `control_messages` / `skdm_peer_status` are empty for v=3 and
/// v=4-DM sends (only v=5 group sends emit SKDMs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptWire {
    pub content: String,
    pub control_messages: Vec<String>,
    pub skdm_peer_status: Vec<SkdmPeerStatus>,
}

impl EncryptWire {
    /// v=3 / v=4-DM helper: a content wire with no SKDM fan-out.
    fn content_only(content: String) -> Self {
        EncryptWire {
            content,
            control_messages: Vec::new(),
            skdm_peer_status: Vec::new(),
        }
    }
}

fn rn_session_store_from_config_dir() -> Result<crate::wire_rn::RnSessionStore, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(crate::wire_rn::RnSessionStore::new(dir.join("rn")))
}

fn select_rn_wire_path_for_send(
    store: &crate::wire_rn::RnSessionStore,
    peer_discord_id: &str,
    peer_identity_x25519: &[u8; 32],
) -> Result<RnWirePath, String> {
    let pin = store.load_pin(peer_identity_x25519).map_err(|e| {
        format!(
            "OSL: send refused for peer {peer}: OSL-RN version pin could not be read: {e}",
            peer = crate::log_id::log_id(peer_discord_id)
        )
    })?;

    select_rn_wire_path(
        &pin,
        // The send dispatcher has no signed RN capability record in
        // peer_map. Treat that absence as no capability; a stored RN
        // pin still turns it into a refusal instead of permission.
        keystore::client::PeerCapabilities::Absent,
        crate::wire_rn::RnPolicy::Opportunistic,
    )
    .map_err(|e| {
        format!(
            "OSL: send refused for peer {peer}: {e}",
            peer = crate::log_id::log_id(peer_discord_id)
        )
    })
}

fn encrypt_rn_content_send(
    store: &crate::wire_rn::RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_discord_id: &str,
    peer_identity_x25519: &[u8; 32],
    plaintext: &[u8],
) -> Result<EncryptWire, String> {
    crate::wire_rn::send_rn(
        store,
        sealer,
        peer_identity_x25519,
        crate::wire_v2::MSG_TYPE_CONTENT,
        plaintext,
    )
    .map(EncryptWire::content_only)
    .map_err(|e| {
        format!(
            "OSL: RN send refused for peer {peer}: {e}",
            peer = crate::log_id::log_id(peer_discord_id)
        )
    })
}

#[cfg(all(test))]
mod rn_send_selection_tests {
    use super::*;

    #[test]
    fn send_time_selection_allows_legacy_for_absent_pin() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = crate::wire_rn::RnSessionStore::new(dir.path().join("rn"));
        let peer = [17u8; 32];

        assert_eq!(
            select_rn_wire_path_for_send(&store, "123456789012345678", &peer),
            Ok(RnWirePath::LegacyV3),
            "absent pin should allow existing legacy send path"
        );
    }

    #[test]
    fn send_time_selection_refuses_legacy_for_stored_rn_pin() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = crate::wire_rn::RnSessionStore::new(dir.path().join("rn"));
        let peer = [18u8; 32];
        store.raise_pin_to_rn(&peer).expect("raise pin");

        let err = select_rn_wire_path_for_send(&store, "123456789012345678", &peer)
            .expect_err("pinned peer must not use the legacy send path");

        assert!(
            err.contains("refusing to send a legacy v=3 message"),
            "unexpected refusal: {err}"
        );
    }

    #[test]
    fn unit_b59_rn_wire_path_calls_send_rn_and_keeps_gate_disabled() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = crate::wire_rn::RnSessionStore::new(dir.path().join("rn"));
        let sealer = keystore::sealer::MemorySealer::new();
        let peer = [59u8; 32];

        let err = encrypt_rn_content_send(
            &store,
            &sealer,
            "123456789012345678",
            &peer,
            b"b59 plaintext must not escape while RN wire-in is disabled",
        )
        .expect_err("RN call site must refuse through wire_rn::send_rn while the gate is false");

        assert!(
            err.contains("OSL-RN wire-in is disabled"),
            "unexpected RN refusal: {err}"
        );
        assert!(
            !crate::wire_rn::RN_WIRE_IN_ENABLED,
            "unit b59 must not enable the OSL-RN wire-in gate"
        );
    }
}

/// Send-path transport policy after recipient resolution.
///
/// Keep this as the single typed answer for the v=3/v=4/v=5 routing
/// question. Absence of an explicit local authorization must resolve
/// to [`RatchetPolicyDecision::LegacyV3`], never to a ratcheted branch.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RatchetPolicyDecision {
    /// Stateless PQ-hybrid wrapping for the resolved recipients.
    LegacyV3,
    /// Inert retained single-peer DM ratchet path.
    LegacyV4Dm,
    /// Group/server sender-key path.
    SenderKeysV5,
}

impl std::fmt::Debug for RatchetPolicyDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            RatchetPolicyDecision::LegacyV3 => "LegacyV3",
            RatchetPolicyDecision::LegacyV4Dm => "LegacyV4Dm",
            RatchetPolicyDecision::SenderKeysV5 => "SenderKeysV5",
        };
        f.write_str(label)
    }
}

/// Layer 10 / Phase 7b IPC entry point: encrypt a v=2 content
/// message for the whitelist-resolved recipients in `scope`.
///
/// **9-B1 shape change** — this function now returns
/// [`EncryptOutput`] instead of `String`. Callers that only need the
/// first wire string for backwards compatibility can use the
/// [`cmd_osl_encrypt_message_v2_wire`] helper, which preserves the
/// pre-B1 signature for tests and other direct call sites.
///
/// Reads:
/// - `state.identity` for our x25519 (secret + public).
/// - `state.whitelist_state` + `state.peer_map` for scope
///   resolution.
/// - `state.app_preferences` for the Mode 0/Mode 1 selector and
///   preview confirmations.
pub fn cmd_osl_encrypt_message_v2(
    state: &AppState,
    plaintext: String,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<EncryptOutput, String> {
    // F3.6 pivot: text encryption is unconditional for everyone.
    // The F3.2 launch-window gate that lived here is retired
    // alongside the 60-min model; the surviving tier gate fires
    // at `cmd_osl_seal_attachment_with_cover_v3` instead.

    let scope_for_mode: crate::scope::Scope = scope_input
        .clone()
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;

    let EncryptWire {
        content: wire,
        control_messages,
        skdm_peer_status,
    } = cmd_osl_encrypt_message_v2_wire(
        state,
        plaintext,
        scope_input,
        channel_members,
        self_discord_id,
    )?;

    // Mode dispatch: stego_mode selects DPC0:: vs chunked DPC1::.
    // 9-MODE1-RETIRE: Mode 1 is disabled in V2 (template stego is
    // unviable under the PQ-hybrid wire's ~1190-byte wrap leg). Legacy
    // app_preferences.json files with stego_mode=mode1 are coerced
    // silently to Mode 0 here. V3 will re-enable Mode 1 alongside an
    // LLM-cipher revival; the chunking + decode code stays in tree.
    let mode = {
        let prefs = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        prefs.stego_mode
    };

    use crate::app_preferences::StegoMode;
    let mode = if matches!(mode, StegoMode::Mode1) {
        tracing::warn!("Mode 1 disabled in V2; coercing to Mode 0. Legacy config?");
        StegoMode::Mode0
    } else {
        mode
    };

    match mode {
        StegoMode::Mode0 => Ok(EncryptOutput {
            messages: vec![wire],
            session_id: None,
            control_messages,
            skdm_peer_status,
        }),
        StegoMode::Mode1 => {
            // Strip the DPC0:: prefix and recover the raw wire bytes
            // — those are what we chunk into Mode 1 carriers. Each
            // chunk is independently HMAC-authenticated against the
            // conversation salt (see `mode1_chunking`).
            let body = wire
                .strip_prefix("DPC0::")
                .ok_or_else(|| "OSL: Mode 1 wrap expected DPC0:: wire prefix".to_string())?;
            let raw = STANDARD
                .decode(body)
                .map_err(|e| format!("OSL: Mode 1 wrap: base64 decode of wire body failed: {e}"))?;

            let scope_storage_key = scope_for_mode.storage_key();
            let salt = scope_storage_key.clone().into_bytes();
            let cipher = stego::ConversationCipher::from_salt(&salt);
            let session_id = crypto::random::random_u32();

            let chunks = stego::chunk_payload(&salt, session_id, &raw);
            let mut messages = Vec::with_capacity(chunks.len());
            for chunk in &chunks {
                let cover = stego::encode_mode1(&cipher, &chunk.bytes)
                    .map_err(|e| format!("OSL: Mode 1 encode_mode1: {e}"))?;
                messages.push(cover);
            }

            tracing::info!(
                chunks = messages.len(),
                session_id = session_id,
                scope = %crate::log_id::log_id(&scope_storage_key),
                "OSL: mode1 send"
            );

            Ok(EncryptOutput {
                messages,
                session_id: Some(session_id),
                control_messages,
                skdm_peer_status,
            })
        }
    }
}

/// Which wire path a send should use for one recipient.
///
/// Unit b1: this — plus [`select_rn_wire_path`] — is the seam that
/// replaces the previously hardcoded `wire_v2::encrypt_v3` call at
/// the tail of `cmd_osl_encrypt_message_v2_wire` with a real
/// decision routed through `wire_rn::select_wire_version`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RnWirePath {
    /// Route through the existing PQ-hybrid `wire_v2::encrypt_v3` path.
    LegacyV3,
    /// Route through OSL-RN (wire `0x10`). Unreachable while
    /// `wire_rn::RN_WIRE_IN_ENABLED` is `false` — see
    /// `select_rn_wire_path`.
    Rn,
}

/// Choose the wire path for one recipient.
///
/// This never itself decides to downgrade: when
/// `wire_rn::select_wire_version` returns
/// `wire_rn::SelectedVersion::Rn` but OSL-RN wire-in is disabled
/// (`RN_WIRE_IN_ENABLED == false`, the current and only shipped
/// state), sending v=3 instead would be exactly the silent
/// downgrade `wire_rn.rs` exists to prevent, so this refuses
/// instead of falling back.
fn select_rn_wire_path(
    pin: &crate::wire_rn::RnPeerPin,
    peer_capabilities: keystore::client::PeerCapabilities,
    policy: crate::wire_rn::RnPolicy,
) -> Result<RnWirePath, String> {
    match crate::wire_rn::select_wire_version(pin, peer_capabilities, policy) {
        Ok(crate::wire_rn::SelectedVersion::LegacyV3) => Ok(RnWirePath::LegacyV3),
        Ok(crate::wire_rn::SelectedVersion::Rn) => {
            if crate::wire_rn::RN_WIRE_IN_ENABLED {
                Ok(RnWirePath::Rn)
            } else {
                Err(
                    "OSL: peer requires OSL-RN but wire-in is disabled in this build; \
                     refusing to send v=3 (no downgrade)"
                        .to_string(),
                )
            }
        }
        Err(e) => Err(format!("OSL: RN wire-path selection: {e}")),
    }
}

/// Pre-9-B1 entry point that produces a single Mode 0
/// `DPC0::<b64>` wire string. Retained for tests and any caller
/// that wants the wire bytes without the Mode 1 cover layer.
pub fn cmd_osl_encrypt_message_v2_wire(
    state: &AppState,
    plaintext: String,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<EncryptWire, String> {
    // F3.6 pivot: text encryption is unconditional. The F3.2
    // gate here is retired; see the matching note at
    // `cmd_osl_encrypt_message_v2`.

    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    let self_mlkem_pub = identity.mlkem_encapsulation_key();
    drop(id_guard);

    // Probe-3 follow-up: proactively seed scope_membership from the
    // caller-supplied channel_members on every GC send. Without this,
    // a cold post-relaunch scope_membership cache + a GC where the
    // user just toggled `channel_whitelisted = true` produces an
    // empty `gc_dynamic_members` in recipients_for_scope_v3 ->
    // `should_encrypt_to` for each channel-member peer returns false
    // (the GC arm requires `is_gc_member` against the durable store)
    // -> only self resolves -> the send falls through to v=3
    // self-only, and the peer's DOM correctly returns
    // "not a recipient" because they're not in the slot list. Seeding
    // the membership oracle from boot.js's React-derived
    // channel_members closes that gap: the act of sending establishes
    // the membership, so the very first send after a relaunch picks
    // up every member already known to boot.js.
    if scope.kind == crate::scope::ScopeKind::Gc && !channel_members.is_empty() {
        let mut mem = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        mem.note_gc_members(&scope.id, channel_members.iter().cloned());
    }

    // Phase 9-A1: text sends now use v=3 (PQ-hybrid). Capability
    // check happens in recipients_for_scope_v3 — any whitelisted
    // member missing an ML-KEM pubkey fails the send with a
    // pointed error. Phase 9-A1b adds a single keyserver-refresh
    // retry for legacy peers whose entry pre-dates the ML-KEM
    // schema bump: we'll attempt to fetch the missing pubkey and
    // re-run the capability check before surfacing the error.
    let resolve_recipients = || {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        crate::whitelist::recipients_for_scope_v3(
            &pm_guard,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
            &self_mlkem_pub,
        )
    };
    let recipients = match resolve_recipients() {
        Ok(r) => r,
        // REGISTER-FIX: BOTH "peer missing ML-KEM" and the new
        // "peer missing all keys" (wiped + re-whitelisted entry)
        // are recoverable via a one-shot inline keyserver fetch
        // keyed by the peer's Discord snowflake (4(a) — it just
        // works; no manual peer_map editing).
        Err(crate::whitelist::RecipientsV3Error::PeerMissingMlkemPubkey { discord_id })
        | Err(crate::whitelist::RecipientsV3Error::PeerMissingKeys { discord_id }) => {
            match refresh_peer_pubkeys_from_keyserver(state, &discord_id) {
                Ok(_) => {
                    // Keys (x25519 + ML-KEM [+ ratchet]) populated
                    // from the keyserver; retry the capability check.
                    resolve_recipients().map_err(|e| {
                        format!("OSL: v=3 capability check (after keyserver refresh): {e}")
                    })?
                }
                Err(refresh_err) => {
                    return Err(format!(
                        "OSL: can't encrypt to peer {discord_id}: keys \
                         unavailable and keyserver refresh failed: \
                         {refresh_err} (fail-closed; message NOT sent \
                         plaintext or self-only)",
                        discord_id = crate::log_id::log_id(&discord_id)
                    ));
                }
            }
        }
        Err(e) => return Err(format!("OSL: v=3 capability check: {e}")),
    };

    let non_self_peers: Vec<&(String, crate::wire_v2::RecipientV3)> = recipients
        .iter()
        .skip(1) // recipients[0] is (self_discord_id, self) per recipients_for_scope_v3
        .collect();

    if let Some(wire) = try_encrypt_rn_first_contact_from_state(
        state,
        crate::wire_rn::RN_WIRE_IN_ENABLED,
        &scope,
        &non_self_peers,
        plaintext.as_bytes(),
    )? {
        return Ok(EncryptWire::content_only(wire));
    }

    if !non_self_peers.is_empty() {
        let rn_store = rn_session_store_from_config_dir()?;
        for (peer_did, recipient) in &non_self_peers {
            match select_rn_wire_path_for_send(
                &rn_store,
                peer_did,
                recipient.x25519_pub.as_bytes(),
            )? {
                RnWirePath::LegacyV3 => {}
                RnWirePath::Rn => {
                    if non_self_peers.len() != 1 {
                        return Err(
                            "OSL: RN send selected for a multi-recipient scope; refusing \
                             to fan out a one-peer ratchet message"
                                .to_string(),
                        );
                    }
                    let sealer = select_best_sealer();
                    return encrypt_rn_content_send(
                        &rn_store,
                        sealer.as_ref(),
                        peer_did,
                        recipient.x25519_pub.as_bytes(),
                        plaintext.as_bytes(),
                    );
                }
            }
        }
    }

    // DMs no longer route through the legacy v=4 Double Ratchet send
    // path. They fall through to stateless v=3 below, so outbound
    // encryption has no per-peer session state to desynchronize.
    let ratchet_decision = ratchet_policy_decision(
        &scope,
        non_self_peers.len(),
        state
            .sender_keys_enabled
            .load(std::sync::atomic::Ordering::Acquire),
    );
    match ratchet_decision {
        RatchetPolicyDecision::SenderKeysV5 => {
            return encrypt_v5_send(
                state,
                &sender_sk,
                &self_pk,
                &scope,
                &self_discord_id,
                &channel_members,
                &non_self_peers,
                plaintext.as_bytes(),
            );
        }
        RatchetPolicyDecision::LegacyV3 | RatchetPolicyDecision::LegacyV4Dm => {}
    }

    // 7d-PIVOT: encrypt_toggle is no longer coupled to having a
    // peer whitelist. recipients_for_scope_v3 always returns at
    // least self (len >= 1); encrypt-to-self is a valid send
    // result.
    // encrypt_v3 wants keys only; drop the paired discord_ids. Same
    // set, same order — wire output is byte-identical to pre-change.
    let key_recipients: Vec<crate::wire_v2::RecipientV3> =
        recipients.iter().map(|(_, r)| r.clone()).collect();
    crate::wire_v2::encrypt_v3(
        &sender_sk,
        &self_pk,
        &key_recipients,
        crate::wire_v2::MSG_TYPE_CONTENT,
        plaintext.as_bytes(),
    )
    .map_err(|e| format!("OSL: encrypt_v3: {e}"))
    .map(EncryptWire::content_only)
}

#[allow(clippy::too_many_arguments)]
fn try_encrypt_rn_first_contact_from_state(
    state: &AppState,
    rn_wire_in_enabled: bool,
    scope: &crate::scope::Scope,
    non_self_peers: &[&(String, crate::wire_v2::RecipientV3)],
    plaintext: &[u8],
) -> Result<Option<String>, String> {
    if !rn_wire_in_enabled || scope_is_group_or_server(scope) || non_self_peers.len() != 1 {
        return Ok(None);
    }

    let peer_did = non_self_peers
        .first()
        .map(|peer| peer.0.as_str())
        .ok_or_else(|| "OSL: OSL-RN first contact: missing peer".to_string())?;
    let (identity, peer_entry) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?
            .clone();
        drop(id_guard);

        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let peer_entry = pm
            .get(peer_did)
            .cloned()
            .ok_or_else(|| format!("OSL: no peer entry for discord_id={peer_did}", peer_did = crate::log_id::log_id(peer_did)))?;
        (identity, peer_entry)
    };

    let peer_identity = rn_peer_identity_from_entry(peer_did, &peer_entry)?;
    let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: OSL-RN state dir: {e}"))?;
    let store = crate::wire_rn::RnSessionStore::new(dir.join("rn"));
    let sealer = keystore::select_best_sealer();

    let caps = match verified_rn_capabilities_for_live_peer(state, &peer_entry) {
        Ok(caps) => caps,
        Err(e) => {
            if store
                .load_pin(peer_identity.as_bytes())
                .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?
                .is_pinned_to_rn()
            {
                return Err(e);
            }
            return Ok(None);
        }
    };

    let pin = store
        .load_pin(peer_identity.as_bytes())
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?;
    match crate::wire_rn::select_wire_version(&pin, caps, crate::wire_rn::RnPolicy::Opportunistic)
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?
    {
        crate::wire_rn::SelectedVersion::LegacyV3 => return Ok(None),
        crate::wire_rn::SelectedVersion::Rn => {}
    }

    let Some(prekey_bundle) =
        fetch_rn_prekey_bundle_for_peer(state, &identity, peer_did, &peer_entry)?
    else {
        return Err("OSL: OSL-RN first contact: selected OSL-RN but no authenticated prekey bundle is available".to_string());
    };

    let peer_bundle = rn_peer_bundle_from_prekey_response(&prekey_bundle)?;
    try_encrypt_rn_first_contact_with_bundle(
        rn_wire_in_enabled,
        &store,
        sealer.as_ref(),
        &identity.x25519_secret,
        &identity.x25519_public,
        &peer_bundle,
        caps,
        prekey_bundle.ik_mlkem768_pub.as_str(),
        plaintext,
    )
}

fn verified_rn_capabilities_for_live_peer(
    state: &AppState,
    peer_entry: &crate::peer_map::PeerEntry,
) -> Result<keystore::client::PeerCapabilities, String> {
    let Some(osl_user_id) = peer_entry.osl_user_id.as_deref() else {
        return Ok(keystore::client::PeerCapabilities::Absent);
    };
    if is_discord_snowflake_shaped(osl_user_id) {
        return Ok(keystore::client::PeerCapabilities::Absent);
    }
    let resp = {
        let ks = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks
            .as_ref()
            .ok_or_else(|| "OSL: OSL-RN first contact: key-server not initialised".to_string())?;
        client
            .fetch_pubkeys(osl_user_id)
            .map_err(|_| "OSL: OSL-RN first contact: peer key fetch refused".to_string())?
    };
    if !rn_pubkeys_response_matches_live_peer(peer_entry, &resp) {
        return Ok(keystore::client::PeerCapabilities::Unverified);
    }
    if resp.user_id != osl_user_id {
        return Ok(keystore::client::PeerCapabilities::Unverified);
    }
    Ok(keystore::client::verify_peer_capabilities(&resp))
}

fn rn_pubkeys_response_matches_live_peer(
    peer_entry: &crate::peer_map::PeerEntry,
    resp: &keystore::client::PubkeysResponse,
) -> bool {
    let trusted_ed25519 = peer_entry
        .tofu_key_bundle
        .as_ref()
        .map(|bundle| bundle.ed25519_pub.as_str())
        .or(peer_entry.tofu_ed25519_pub.as_deref());

    peer_entry.pubkey.as_deref() == Some(resp.ik_x25519_pub.as_str())
        && peer_entry.ik_mlkem768_pub.as_deref() == Some(resp.ik_mlkem768_pub.as_str())
        && matches!(trusted_ed25519, Some(trusted) if trusted == resp.ik_ed25519_pub)
}

fn fetch_rn_prekey_bundle_for_peer(
    state: &AppState,
    identity: &keystore::Identity,
    peer_did: &str,
    peer_entry: &crate::peer_map::PeerEntry,
) -> Result<Option<keystore::client::PrekeyBundleResponse>, String> {
    let Some(osl_user_id) = peer_entry.osl_user_id.as_deref() else {
        return Ok(None);
    };
    if is_discord_snowflake_shaped(osl_user_id) {
        return Ok(None);
    }
    let bundle = {
        let ks = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks
            .as_ref()
            .ok_or_else(|| "OSL: OSL-RN first contact: key-server not initialised".to_string())?;
        client
            .fetch_prekey_bundle(identity, osl_user_id)
            .map_err(|_| "OSL: OSL-RN first contact: peer prekey fetch refused".to_string())?
    };
    if bundle.user_id != osl_user_id {
        return Err("OSL: OSL-RN first contact: prekey bundle identity mismatch".to_string());
    }
    if peer_entry.pubkey.as_deref() != Some(bundle.ik_x25519_pub.as_str())
        || peer_entry.ik_mlkem768_pub.as_deref() != Some(bundle.ik_mlkem768_pub.as_str())
        || peer_entry
            .tofu_key_bundle
            .as_ref()
            .map(|trusted| trusted.ed25519_pub.as_str())
            .or(peer_entry.tofu_ed25519_pub.as_deref())
            != Some(bundle.ik_ed25519_pub.as_str())
    {
        return Err(format!(
            "OSL: OSL-RN first contact: prekey bundle for peer {peer_did} does not match trusted keys",
            peer_did = crate::log_id::log_id(peer_did)
        ));
    }
    verify_rn_prekey_bundle_signature(&bundle)?;
    Ok(Some(bundle))
}

fn verify_rn_prekey_bundle_signature(
    bundle: &keystore::client::PrekeyBundleResponse,
) -> Result<(), String> {
    let ed = decode_b64_array::<32>("OSL-RN prekey Ed25519", &bundle.ik_ed25519_pub)?;
    let spk = decode_b64_array::<32>("OSL-RN prekey SPK", &bundle.spk_pub)?;
    let sig = decode_b64_array::<64>("OSL-RN prekey signature", &bundle.spk_signature)?;
    let ok = crypto::ed25519::verify(
        &crypto::ed25519::PublicKey::from_bytes(ed),
        &spk,
        &crypto::ed25519::Signature::from_bytes(sig),
    )
    .map_err(|e| format!("OSL: OSL-RN first contact: prekey signature verify: {e}"))?;
    if !ok {
        return Err("OSL: OSL-RN first contact: prekey signature refused".to_string());
    }
    Ok(())
}

fn rn_peer_identity_from_entry(
    peer_did: &str,
    entry: &crate::peer_map::PeerEntry,
) -> Result<osl_ratchet_next::XPublic, String> {
    let b64 = entry.pubkey.as_deref().ok_or_else(|| {
        format!(
            "OSL: OSL-RN first contact: peer {peer_did} missing X25519 key",
            peer_did = crate::log_id::log_id(peer_did)
        )
    })?;
    Ok(osl_ratchet_next::XPublic::from_bytes(decode_b64_array::<32>(
        "OSL-RN peer identity",
        b64,
    )?))
}

fn rn_peer_bundle_from_prekey_response(
    bundle: &keystore::client::PrekeyBundleResponse,
) -> Result<osl_ratchet_next::PeerBundle, String> {
    let identity = osl_ratchet_next::XPublic::from_bytes(decode_b64_array::<32>(
        "OSL-RN IK",
        &bundle.ik_x25519_pub,
    )?);
    let signed_prekey = osl_ratchet_next::XPublic::from_bytes(decode_b64_array::<32>(
        "OSL-RN SPK",
        &bundle.spk_pub,
    )?);
    let one_time_prekey = match bundle.opk.as_ref() {
        Some(opk) => Some((
            opk.id,
            osl_ratchet_next::XPublic::from_bytes(decode_b64_array::<32>(
                "OSL-RN OPK",
                &opk.pub_b64,
            )?),
        )),
        None => None,
    };
    let pq_bytes = decode_b64_array::<{ osl_ratchet_next::MLKEM_EK }>(
        "OSL-RN ML-KEM",
        &bundle.ik_mlkem768_pub,
    )?;
    Ok(osl_ratchet_next::PeerBundle {
        identity,
        signed_prekey,
        one_time_prekey,
        pq_prekey: osl_ratchet_next::KemPublic::from_bytes(&pq_bytes)
            .map_err(|e| format!("OSL: OSL-RN first contact: ML-KEM public key: {e}"))?,
    })
}

fn decode_b64_array<const N: usize>(label: &str, b64: &str) -> Result<[u8; N], String> {
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| format!("OSL: {label} base64 decode failed: {e}"))?;
    let got = bytes.len();
    bytes
        .try_into()
        .map_err(|_| format!("OSL: {label} length {got} != {N}"))
}

#[allow(clippy::too_many_arguments)]
fn try_encrypt_rn_first_contact_with_bundle(
    rn_wire_in_enabled: bool,
    store: &crate::wire_rn::RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    own_identity_secret: &crypto::x25519::SecretKey,
    own_identity_public: &crypto::x25519::PublicKey,
    peer_bundle: &osl_ratchet_next::PeerBundle,
    caps: keystore::client::PeerCapabilities,
    peer_mlkem768_ek_b64: &str,
    plaintext: &[u8],
) -> Result<Option<String>, String> {
    if !rn_wire_in_enabled {
        return Ok(None);
    }

    let peer_identity = *peer_bundle.identity.as_bytes();
    let pin = store
        .load_pin(&peer_identity)
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?;
    match crate::wire_rn::select_wire_version(&pin, caps, crate::wire_rn::RnPolicy::Opportunistic)
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?
    {
        crate::wire_rn::SelectedVersion::LegacyV3 => return Ok(None),
        crate::wire_rn::SelectedVersion::Rn => {}
    }

    let own_secret = osl_ratchet_next::XSecret::from_bytes(*own_identity_secret.as_bytes());
    let peer_mlkem768_ek = STANDARD
        .decode(peer_mlkem768_ek_b64)
        .map_err(|e| format!("OSL: OSL-RN first contact: peer ML-KEM base64: {e}"))?;

    let mut session = match store
        .load_session(&peer_identity, sealer)
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?
    {
        Some(session) => session,
        None => crate::wire_rn::initiate_and_persist(
            store,
            sealer,
            &own_secret,
            own_identity_public.as_bytes(),
            peer_bundle,
            caps,
            &peer_mlkem768_ek,
            crate::wire_rn::RN_CONTEXT_DISCORD_MANUAL,
            osl_ratchet_next::SessionParams::default(),
        )
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?,
    };
    let wire = osl_ratchet_next::encrypt_rn(
        &mut session,
        crate::wire_v2::MSG_TYPE_CONTENT,
        plaintext,
    )
    .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?;
    store
        .save_session(&peer_identity, &session, sealer)
        .map_err(|e| format!("OSL: OSL-RN first contact: {e}"))?;
    Ok(Some(wire))
}

#[cfg(test)]
mod rn_first_contact_command_tests {
    use super::*;
    use base64::Engine as _;
    use keystore::client::{PeerCapabilities, PrekeyBundleResponse, RN_CAP_WIRE_RN};
    use keystore::sealer::MemorySealer;
    use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
    use tempfile::TempDir;

    fn fresh_rn_store() -> (TempDir, crate::wire_rn::RnSessionStore) {
        let dir = TempDir::new().expect("tempdir");
        let store = crate::wire_rn::RnSessionStore::new(dir.path().join("rn"));
        (dir, store)
    }

    #[test]
    fn rn_first_contact_gate_false_does_not_initiate_or_persist() {
        let (_dir, store) = fresh_rn_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(61);
        let (_peer_prekeys, peer_bundle) = fresh_bundle(&mut rng);
        let (own_sk, own_pk) = crypto::x25519::generate_keypair();
        let peer_mlkem_b64 = STANDARD.encode(peer_bundle.pq_prekey.to_bytes());

        let wire = try_encrypt_rn_first_contact_with_bundle(
            false,
            &store,
            &sealer,
            &own_sk,
            &own_pk,
            &peer_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            peer_mlkem_b64.as_str(),
            b"hello",
        )
        .expect("helper");

        assert_eq!(wire, None);
        assert!(store
            .load_session(peer_bundle.identity.as_bytes(), &sealer)
            .expect("load session")
            .is_none());
        assert_eq!(
            store
                .load_pin(peer_bundle.identity.as_bytes())
                .expect("load pin"),
            crate::wire_rn::RnPeerPin::UNKNOWN
        );
    }

    #[test]
    fn rn_first_contact_absent_capability_falls_through_without_persisting() {
        let (_dir, store) = fresh_rn_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(62);
        let (_peer_prekeys, peer_bundle) = fresh_bundle(&mut rng);
        let (own_sk, own_pk) = crypto::x25519::generate_keypair();
        let peer_mlkem_b64 = STANDARD.encode(peer_bundle.pq_prekey.to_bytes());

        let wire = try_encrypt_rn_first_contact_with_bundle(
            true,
            &store,
            &sealer,
            &own_sk,
            &own_pk,
            &peer_bundle,
            PeerCapabilities::Absent,
            peer_mlkem_b64.as_str(),
            b"hello",
        )
        .expect("helper");

        assert_eq!(wire, None);
        assert!(store
            .load_session(peer_bundle.identity.as_bytes(), &sealer)
            .expect("load session")
            .is_none());
    }

    #[test]
    fn rn_first_contact_verified_capability_initiates_persists_and_pins() {
        let (_dir, store) = fresh_rn_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(63);
        let (_peer_prekeys, peer_bundle) = fresh_bundle(&mut rng);
        let (own_sk, own_pk) = crypto::x25519::generate_keypair();
        let peer_mlkem_b64 = STANDARD.encode(peer_bundle.pq_prekey.to_bytes());

        let wire = try_encrypt_rn_first_contact_with_bundle(
            true,
            &store,
            &sealer,
            &own_sk,
            &own_pk,
            &peer_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            peer_mlkem_b64.as_str(),
            b"hello",
        )
        .expect("helper")
        .expect("rn wire");

        assert_eq!(
            osl_ratchet_next::peek_wire_version(&wire),
            Some(osl_ratchet_next::WIRE_VERSION_RN)
        );
        assert!(store
            .load_session(peer_bundle.identity.as_bytes(), &sealer)
            .expect("load session")
            .is_some());
        assert!(store
            .load_pin(peer_bundle.identity.as_bytes())
            .expect("load pin")
            .is_pinned_to_rn());
    }

    #[test]
    fn rn_prekey_bundle_signature_must_verify_under_peer_identity() {
        let peer = keystore::generate_identity("peer".to_string());
        let (_spk_secret, spk_public) = crypto::x25519::generate_keypair();
        let signature = crypto::ed25519::sign(&peer.ed25519_secret, spk_public.as_bytes());
        let bundle = PrekeyBundleResponse {
            user_id: peer.user_id.clone(),
            ik_x25519_pub: STANDARD.encode(peer.x25519_public.as_bytes()),
            ik_ed25519_pub: STANDARD.encode(peer.ed25519_public.as_bytes()),
            ik_mlkem768_pub: STANDARD.encode(peer.mlkem_public_bytes),
            spk_pub: STANDARD.encode(spk_public.as_bytes()),
            spk_signature: STANDARD.encode(signature.as_bytes()),
            spk_rotated_at: "2026-07-29T00:00:00.000Z".to_string(),
            opk: None,
            remaining_opk_count: 0,
            ik_ratchet_initial_pub: None,
        };

        verify_rn_prekey_bundle_signature(&bundle).expect("valid signature");

        let mut forged = bundle;
        forged.spk_signature = STANDARD.encode([0u8; 64]);
        assert!(verify_rn_prekey_bundle_signature(&forged).is_err());
    }
}

/// Phase 9-A3: group/server scopes are eligible for v=5 sender-keys.
fn scope_is_group_or_server(scope: &crate::scope::Scope) -> bool {
    use crate::scope::ScopeKind::*;
    matches!(scope.kind, Gc | ServerChannel | ServerFull)
}

fn ratchet_policy_decision(
    scope: &crate::scope::Scope,
    non_self_peer_count: usize,
    sender_keys_enabled: bool,
) -> RatchetPolicyDecision {
    if sender_keys_enabled && non_self_peer_count > 0 && scope_is_group_or_server(scope) {
        return RatchetPolicyDecision::SenderKeysV5;
    }

    // OPTION B: DMs no longer use the v=4 Double Ratchet. They route
    // through stateless v=3, which eliminates the desync class entirely:
    // no session state to fall out of sync, no bootstrap handshake, and
    // no reset/recovery loop. There is deliberately no implicit input
    // here that authorizes [`RatchetPolicyDecision::LegacyV4Dm`].
    RatchetPolicyDecision::LegacyV3
}

#[cfg(test)]
mod ratchet_policy_decision_tests {
    use super::{ratchet_policy_decision, RatchetPolicyDecision};

    #[test]
    fn dm_sends_stay_on_v3_even_when_sender_keys_are_enabled() {
        assert_eq!(
            ratchet_policy_decision(&crate::scope::Scope::dm("peer"), 1, true),
            RatchetPolicyDecision::LegacyV3
        );
    }

    #[test]
    fn group_sends_need_an_enabled_policy_and_a_non_self_peer_for_v5() {
        let scope = crate::scope::Scope::gc("group");

        assert_eq!(
            ratchet_policy_decision(&scope, 1, false),
            RatchetPolicyDecision::LegacyV3
        );
        assert_eq!(
            ratchet_policy_decision(&scope, 0, true),
            RatchetPolicyDecision::LegacyV3
        );
        assert_eq!(
            ratchet_policy_decision(&scope, 1, true),
            RatchetPolicyDecision::SenderKeysV5
        );
    }

    #[test]
    fn server_scopes_follow_the_same_v5_policy_as_group_scopes() {
        for scope in [
            crate::scope::Scope::server_channel("server", "channel"),
            crate::scope::Scope::server_full("server"),
        ] {
            assert_eq!(
                ratchet_policy_decision(&scope, 1, true),
                RatchetPolicyDecision::SenderKeysV5
            );
        }
    }

    #[test]
    fn no_current_input_authorizes_the_retained_v4_dm_branch() {
        let scopes = [
            crate::scope::Scope::dm("peer"),
            crate::scope::Scope::gc("group"),
            crate::scope::Scope::server_channel("server", "channel"),
            crate::scope::Scope::server_full("server"),
        ];

        for scope in scopes {
            for non_self_peer_count in [0, 1, 2] {
                for sender_keys_enabled in [false, true] {
                    assert_ne!(
                        ratchet_policy_decision(&scope, non_self_peer_count, sender_keys_enabled),
                        RatchetPolicyDecision::LegacyV4Dm
                    );
                }
            }
        }
    }
}

/// Phase 9-A3: 24-hour rotation timer threshold.
const SENDER_KEY_ROTATE_AFTER_SECS: u64 = 24 * 60 * 60;

/// Phase 6.2: how often (in seconds) we re-emit the SKDM bundle for
/// self-heal purposes when the chain hasn't otherwise changed.
/// Pre-6.2 behaviour was "emit on every send", which produced N extra
/// Discord messages for N user sends in an active GC. Now the bundle
/// is emitted on install, on rotate, OR when this many seconds have
/// elapsed since the last emit. Tradeoff: receivers who miss every
/// SKDM in the interval window will have to wait or trigger the
/// SKDM_REQUEST recovery path; in exchange, active conversations
/// shed ~95% of the noise messages.
const SKDM_PERIODIC_EMIT_INTERVAL_SECS: u64 = 5 * 60;

/// Phase 9-A3: decide whether the sender-keys chain for this scope
/// needs to rotate before the next send. Returns `true` when:
/// - the time since `chain_started_at` exceeds 24 hours, OR
/// - the current channel-member set differs from
///   `last_known_members` (any join/leave).
fn sender_key_needs_rotation(
    sender: &crypto::sender_keys::SenderChain,
    current_members: &[String],
    now: u64,
) -> bool {
    if now.saturating_sub(sender.chain_started_at()) >= SENDER_KEY_ROTATE_AFTER_SECS {
        return true;
    }
    let stored: std::collections::BTreeSet<&[u8]> = sender
        .last_known_members()
        .iter()
        .map(|m| m.as_slice())
        .collect();
    let live: std::collections::BTreeSet<&[u8]> =
        current_members.iter().map(|m| m.as_bytes()).collect();
    stored != live
}

/// Phase 9-A3: v=5 group send. On-demand install/rotate of the
/// outbound `SenderChain`, SKDM dispatch to each non-self peer via
/// v=4, then encode the actual message under v=5.
#[allow(clippy::too_many_arguments)]
fn encrypt_v5_send(
    state: &AppState,
    sender_sk: &crypto::x25519::SecretKey,
    self_pk: &crypto::x25519::PublicKey,
    scope: &crate::scope::Scope,
    self_discord_id: &str,
    // Retained in the signature for call-site symmetry with the v=3
    // path; v=5 derives membership from the resolved recipient set
    // (`non_self_peers`), not the raw roster.
    _channel_members: &[String],
    non_self_peers: &[&(String, crate::wire_v2::RecipientV3)],
    plaintext: &[u8],
) -> Result<EncryptWire, String> {
    use crypto::sender_keys::{SenderContext, SenderKeyState, SenderKeyStateOnDisk};

    let scope_key = scope.storage_key();
    let self_mlkem_pub_bytes: Vec<u8> = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        identity.mlkem_public_bytes.to_vec()
    };
    let now: u64 = now_unix_secs().max(0) as u64;

    // Membership tracking for the sender key is derived from the
    // actual recipient set below (`recipient_ids`, from
    // `non_self_peers`), not the channel roster — see the rotation
    // block. `channel_members` / the gateway snapshot are no longer
    // consulted here: the caller already resolved who is a granted
    // recipient (whitelist + lock tier) into `non_self_peers`, and the
    // SKDM only ever goes to that set.

    // Load (or initialize) the per-scope SenderKeyState. We work on
    // a clone to keep the lock window short; persist back after.
    let mut sks: SenderKeyState = {
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        match g.states.get(&scope_key) {
            Some(disk) => disk
                .clone()
                .try_into()
                .map_err(|e| format!("OSL: v=5 send: load sender_key_state: {e}"))?,
            None => SenderKeyState::new(),
        }
    };

    // Rotation/redistribution must track the ACTUAL recipient set —
    // the granted peers who receive the SKDM (`non_self_peers`) — NOT
    // the raw channel roster (`effective_members`). Tracking the roster
    // was the GC bug: a peer you whitelist AFTER the chain exists is
    // already in the roster, so the roster doesn't change, no rotation
    // fires, and they never get the sender key (until the 5-min
    // periodic re-emit) → permanent "not a recipient" in the meantime.
    // Tracking the recipient set means whitelisting a peer changes the
    // set → rotate → SKDM re-distributed to everyone on the very next
    // send.
    let recipient_ids: Vec<String> = non_self_peers.iter().map(|p| p.0.clone()).collect();

    // Decide install / rotate / continue.
    let needs_install = sks.sender_chain().is_none();
    let needs_rotate = sks
        .sender_chain()
        .map(|c| sender_key_needs_rotation(c, &recipient_ids, now))
        .unwrap_or(false);

    let send_skdm = needs_install || needs_rotate;
    if needs_install {
        sks.install_sender()
            .map_err(|e| format!("OSL: v=5 send: install_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = recipient_ids
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    } else if needs_rotate {
        sks.rotate_sender()
            .map_err(|e| format!("OSL: v=5 send: rotate_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = recipient_ids
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    }

    let (chain_id, rotation_root, physical_device_id) = {
        let s = sks
            .sender_chain()
            .ok_or_else(|| "OSL: v=5 send: missing sender chain after install".to_string())?;
        (
            s.current_chain_id(),
            s.rotation_root_bytes(),
            s.physical_device_id(),
        )
    };

    // Self-loopback: install/rotate a self-receiver chain seeded
    // from the same rotation_root so self-decrypts work uniformly.
    if send_skdm {
        let self_bytes = self_discord_id.as_bytes().to_vec();
        if sks
            .receiver_chain_for_physical_device(&self_bytes, physical_device_id)
            .is_some()
        {
            sks.rotate_receiver(&self_bytes, chain_id, &rotation_root, physical_device_id)
                .map_err(|e| format!("OSL: v=5 send: rotate_receiver(self): {e}"))?;
        } else {
            sks.install_receiver(self_bytes, chain_id, &rotation_root, physical_device_id)
                .map_err(|e| format!("OSL: v=5 send: install_receiver(self): {e}"))?;
        }
    }

    // Encrypt the actual message under sender-keys.
    let ctx = SenderContext {
        sender_ik_x25519_pub: *self_pk,
        sender_ik_mlkem_pub: self_mlkem_pub_bytes.clone(),
        group_id: scope_key.clone().into_bytes(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };
    let em = sks
        .encrypt(plaintext, &ctx)
        .map_err(|e| format!("OSL: v=5 send: sender_keys::encrypt: {e}"))?;
    let wire = crate::wire_v2::encrypt_v5(self_pk, crate::wire_v2::MSG_TYPE_CONTENT, 0, &em)
        .map_err(|e| format!("OSL: v=5 send: encrypt_v5: {e}"))?;

    // Persist updated state before any SKDM dispatch — if the SKDM
    // wire goes out but persistence dies between send + crash, we'd
    // be in a position where peers think we've installed and we
    // don't.
    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        g.states
            .insert(scope_key.clone(), SenderKeyStateOnDisk::from(&sks));
        g.version = 1;
    }
    persist_sender_key_state_now(state);

    // Probe-3 Option-2 step 1: dispatch SKDMs as a SINGLE v=3-bundled
    // PQ-hybrid multi-recipient message instead of N separate v=4
    // ratcheted messages.
    //
    // Old design (v=4-per-peer): each SKDM was wrapped via
    // `send_skdm_via_v4` which built a PQXDH bootstrap + advanced the
    // per-peer Double Ratchet. That made SKDM delivery depend on
    // healthy v=4 ratchet state — after any burn / identity rotation
    // / out-of-order key the v=4 ratchet desynced, SKDMs failed to
    // decrypt on the recipient, no receiver chain installed, every
    // v=5 GC message returned "not a recipient", and recovery looped.
    //
    // New design: ONE v=3 wire carrying MSG_TYPE_SENDER_KEY_DISTRIBUTION
    // bundled to all non-self peers via the existing PQ-hybrid
    // multi-recipient wrap. No per-peer ratchet state involved.
    // boot.js posts it as one message; each recipient's v=3 decrypt
    // resolves their slot, the v=2/v=3 dispatcher routes
    // MSG_TYPE_SENDER_KEY_DISTRIBUTION to `apply_skdm_recv` which
    // installs the receiver chain exactly as before.
    // Probe-3 Option-2 step 1 follow-up: previously the bundle emit
    // was gated on `send_skdm` (i.e. ONLY on first install / on
    // rotation). That left the failure mode: if any receiver missed
    // the very first SKDM (offline, app not yet running, broken v=4
    // ratchet in the old design), every subsequent v=5 send produced
    // NO bundle and the receiver stayed permanently locked out
    // (`not a recipient` forever, no in-band recovery signal).
    // Now: always emit on every send while there are peers. The cost
    // is one extra ~2KB v=3 message per v=5 send; the win is that
    // receivers self-heal from any v=5 message they observe, without
    // needing the SKDM_REQUEST/recovery round-trip. apply_skdm_recv
    // is idempotent (existing receiver chain → rotate_receiver to
    // the same chain_id is a no-op; absent → install_receiver).
    let mut skdm_wires: Vec<String> = Vec::new();
    let mut skdm_peer_status: Vec<SkdmPeerStatus> = Vec::new();
    let _ = send_skdm; // retained for self-receiver gate above; no longer gates bundle emit
                       // Phase 6.2: gate the periodic self-heal emit. Always emit on
                       // install/rotate (the chain is fresh, every receiver needs it).
                       // Otherwise emit only if SKDM_PERIODIC_EMIT_INTERVAL_SECS have
                       // elapsed since the last emit. The 0-default on legacy on-disk
                       // records (pre-6.2 SenderChainOnDisk) forces a one-time emit on
                       // load, which is the right thing -- it primes receivers who
                       // might have missed the prior chain.
    let last_emit_at = sks
        .sender_chain()
        .map(|c| c.last_skdm_emit_at())
        .unwrap_or(0);
    let periodic_due = now.saturating_sub(last_emit_at) >= SKDM_PERIODIC_EMIT_INTERVAL_SECS;
    let should_emit_bundle = needs_install || needs_rotate || periodic_due;
    if !non_self_peers.is_empty() && should_emit_bundle {
        // Include self as a recipient so the sender's own DOM
        // (which sees the SKDM bundle round-tripped through Discord
        // like any other channel message) decodes it cleanly instead
        // of raising "not a recipient" for every own send. The
        // self-slot decode hits apply_skdm_recv which is idempotent
        // for an already-installed receiver chain.
        let self_mlkem = {
            let id_guard = state.identity.lock().expect("identity mutex poisoned");
            id_guard
                .as_ref()
                .ok_or_else(|| "OSL: identity not loaded".to_string())?
                .mlkem_encapsulation_key()
        };
        let mut recipients_v3: Vec<crate::wire_v2::RecipientV3> =
            Vec::with_capacity(non_self_peers.len() + 1);
        recipients_v3.push(crate::wire_v2::RecipientV3 {
            x25519_pub: *self_pk,
            mlkem_pub: self_mlkem,
        });
        for (_, r) in non_self_peers.iter() {
            recipients_v3.push(r.clone());
        }
        match send_skdm_via_v3_bundle(
            sender_sk,
            self_pk,
            &recipients_v3,
            &scope_key,
            chain_id,
            &rotation_root,
            physical_device_id.as_bytes(),
        ) {
            Ok(skdm_wire) => {
                skdm_wires.push(skdm_wire);
                // Phase 6.2: stamp the periodic emit clock so the
                // next send within SKDM_PERIODIC_EMIT_INTERVAL_SECS
                // skips bundle emission unless install/rotate
                // independently triggers it.
                if let Some(chain) = sks.sender_chain_mut() {
                    chain.mark_skdm_emitted_at(now);
                }
                for pair in non_self_peers.iter() {
                    skdm_peer_status.push(SkdmPeerStatus {
                        peer_discord_id: pair.0.clone(),
                        ok: true,
                        error: None,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    recipients = non_self_peers.len(),
                    "[OSL] v=5 SKDM bundle dispatch failed (best-effort; \
                     recipients will request via SKDM_REQUEST if they get \
                     a v=5 message before they install)"
                );
                for pair in non_self_peers.iter() {
                    skdm_peer_status.push(SkdmPeerStatus {
                        peer_discord_id: pair.0.clone(),
                        ok: false,
                        error: Some(e.clone()),
                    });
                }
            }
        }
    }

    Ok(EncryptWire {
        content: wire,
        control_messages: skdm_wires,
        skdm_peer_status,
    })
}

/// Probe-3 Option-2 step 1: ship a SenderKeyDistribution payload to
/// ALL non-self peers in ONE v=3-bundled PQ-hybrid multi-recipient
/// wire (formerly: N separate v=4 ratcheted wires via the now-removed
/// `send_skdm_via_v4`).
///
/// The body is the same `SenderKeyDistribution{scope_storage_key,
/// chain_id, rotation_root, sent_at}` payload that the recipient-side
/// `apply_skdm_recv` already understands. The transport switches from
/// v=4 (ratcheted, single-peer, broke whenever the v=4 DR was
/// desynced) to v=3 (PQXDH per slot + ML-KEM hybrid, no ratchet
/// state). One wire instead of N; no dependency on per-peer ratchet
/// liveness.
///
/// Receive: the existing v=3 decrypt dispatcher in
/// `cmd_osl_decrypt_message_v2` recovers the `DecryptedV2` and the
/// `match recovered.msg_type` block routes
/// `MSG_TYPE_SENDER_KEY_DISTRIBUTION` to `apply_skdm_recv`. No JS
/// changes; the existing boot.js code already POSTs every
/// `control_messages[]` entry as its own Discord message and
/// suppresses the `OSL_RESULT_SKDM_APPLIED` sentinel.
fn send_skdm_via_v3_bundle(
    sender_sk: &crypto::x25519::SecretKey,
    self_pk: &crypto::x25519::PublicKey,
    recipients: &[crate::wire_v2::RecipientV3],
    scope_storage_key: &str,
    chain_id: u32,
    rotation_root: &[u8; 32],
    physical_device_id: &[u8; 32],
) -> Result<String, String> {
    let payload = crate::control_messages::SenderKeyDistribution {
        scope_storage_key: scope_storage_key.to_string(),
        chain_id,
        rotation_root: *rotation_root,
        physical_device_id: *physical_device_id,
        sent_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_sender_key_distribution(&payload)
        .map_err(|e| format!("OSL: v=5 SKDM bundle: serialize: {e}"))?;
    crate::wire_v2::encrypt_v3(
        sender_sk,
        self_pk,
        recipients,
        crate::wire_v2::MSG_TYPE_SENDER_KEY_DISTRIBUTION,
        &body,
    )
    .map_err(|e| format!("OSL: v=5 SKDM bundle: encrypt_v3: {e}"))
}

/// Phase 9-A2: symmetric DM conversation_id for the DR session
/// context. Each side derives the same string by sorting the two
/// discord_ids — without this, alice's `Scope::dm(bob).storage_key()
/// = "dm:bob"` and bob's `Scope::dm(alice).storage_key() = "dm:alice"`
/// would mismatch on the DR's canonical AD.
fn dm_conversation_id(self_did: &str, peer_did: &str) -> Vec<u8> {
    let (a, b) = if self_did <= peer_did {
        (self_did, peer_did)
    } else {
        (peer_did, self_did)
    };
    format!("dm:{a}:{b}").into_bytes()
}

#[cfg(test)]
mod retired_v4_outbound_tests {
    #[test]
    fn commands_rs_has_no_legacy_v4_send_entrypoint() {
        let source = include_str!("commands.rs");
        for marker in [
            concat!("fn ", "encrypt", "_v4", "_send"),
            concat!("encrypt", "_v4", "_send("),
            concat!("build", "_v4", "_bootstrap", "_ping"),
            concat!("encrypt", "_v4", "_from", "_ratchet"),
            concat!("DoubleRatchet", "::", "new_", "initiator"),
        ] {
            assert!(
                !source.contains(marker),
                "legacy outbound v4 marker still present: {marker}"
            );
        }
    }
}

/// 7d-PIVOT-FIX2 Bug F: re-engaging a previously-burned scope by
/// sending a fresh encrypted message un-burns it. Idempotent — if
/// the scope isn't currently burned, returns `Ok(false)` and the
/// caller skips the cross-window event emit. The Tauri wrapper
/// (`osl_encrypt_message_v2`) calls this after a successful
/// encrypt and emits `osl:scope_unburned` when this returns true.
///
/// Scope-kind mapping matches `cmd_osl_set_whitelist`'s existing
/// auto-unburn-on-re-whitelist path.
pub fn cmd_osl_unburn_scope_after_encrypt(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> bool {
    // 7d-PIVOT-FIX3 Bug F: match the JS-style kind strings used by
    // `cmd_osl_mark_scope_burned` (the only writer of burned_scopes
    // entries). PIVOT-FIX2's "gc_full"/"server_channel_full" mapping
    // never matched anything in the ledger, so this helper silently
    // no-op'd and `osl:scope_unburned` was never emitted — which is
    // why FIX2's cross-window unburn never actually fired.
    let scope_kind_str = match scope_input.kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc",
        crate::scope::ScopeKind::ServerChannel => "server_channel",
        crate::scope::ScopeKind::ServerFull => "server_full",
    };
    cmd_osl_unburn_scope(state, scope_kind_str.to_string(), scope_input.id).unwrap_or(false)
}

/// Phase 8b: structured per-attachment input for
/// [`cmd_osl_encrypt_attachment_envelope`]. JS builds one of these
/// per file picked, then passes the whole list so the cover
/// references every attachment in the Discord message.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEnvelopeInput {
    pub att_key_b64: String,
    pub original_filename: String,
    pub random_filename: String,
    pub mime_type: String,
}

/// Phase 8 / 8b: encrypt an [`AttachmentEnvelope`] (list of per-
/// attachment entries) as a v=2 `MSG_TYPE_ATTACHMENT` message,
/// distributing every per-attachment AEAD key to every scope-
/// whitelisted recipient. The cover string returned is the
/// message-text payload boot.js drops into the `/messages` POST
/// body (replacing the user's typed plaintext on attachment sends).
/// Discord allows up to 10 attachments per message; the cover
/// covers all of them in a single CBOR list.
pub fn cmd_osl_encrypt_attachment_envelope(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    attachments: Vec<AttachmentEnvelopeInput>,
) -> Result<String, String> {
    if attachments.is_empty() {
        return Err("OSL: attachment envelope has no entries".to_string());
    }
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;

    let mut entries: Vec<crate::control_messages::AttachmentEnvelopeEntry> =
        Vec::with_capacity(attachments.len());
    for input in attachments {
        let key_bytes = STANDARD
            .decode(&input.att_key_b64)
            .map_err(|e| format!("OSL: att_key b64 decode: {e}"))?;
        if key_bytes.len() != 32 {
            return Err(format!(
                "OSL: att_key must be 32 bytes, got {}",
                key_bytes.len()
            ));
        }
        let mut att_key = [0u8; 32];
        att_key.copy_from_slice(&key_bytes);
        entries.push(crate::control_messages::AttachmentEnvelopeEntry {
            att_key,
            original_filename: input.original_filename,
            random_filename: input.random_filename,
            mime_type: input.mime_type,
        });
    }

    let env = crate::control_messages::AttachmentEnvelope {
        attachments: entries,
    };
    let env_bytes = crate::control_messages::serialize_attachment_envelope(&env)
        .map_err(|e| format!("OSL: serialize attachment envelope: {e}"))?;

    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);

    // Same server-aware recipient resolution as text (see
    // cmd_osl_seal_attachment_with_cover_v3) so the image envelope
    // encrypts to the identical set as a text message in this scope.
    let recipients = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        crate::whitelist::recipients_x25519_authz(
            &pm_guard,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    crate::wire_v2::encrypt_v2(
        &env_bytes,
        &recipients,
        crate::wire_v2::MSG_TYPE_ATTACHMENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 (attachment envelope): {e}"))
}

/// Phase 8d output: one-shot seal that returns everything JS needs
/// to upload + reference the file. The cover envelope lives INSIDE
/// `sealed_b64`; no separate cover-on-the-wire is needed.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedAttachmentV2 {
    pub sealed_b64: String,
    pub random_filename: String,
    pub mime_type: String,
}

/// Phase 8d: one-shot seal. Generates a fresh per-attachment AEAD
/// key, builds the v=2 cover (multi-recipient envelope carrying
/// that AEAD key + filenames + MIME) using the existing
/// MSG_TYPE_ATTACHMENT path, seals the file payload with the AEAD
/// key, and assembles the V2 wire bundle. The JS caller never sees
/// the AEAD key — it lives only inside the embedded cover, which
/// only whitelisted recipients can decrypt.
#[allow(clippy::too_many_arguments)]
pub fn cmd_osl_seal_attachment_with_cover_v2(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    original_bytes_b64: String,
    original_filename: String,
    random_filename: String,
) -> Result<SealedAttachmentV2, String> {
    // F3.6-DEFENSE: gate the legacy v2 seal path identically to
    // v3. F3.6 only gated v3 (the production step-2 upload path);
    // v2 is reachable via documented boot.js fallbacks (older
    // v1Send + non-Tauri error fallback), so leaving it ungated
    // would let a free user bypass the attachment paywall by
    // routing through the legacy command. Same
    // `OSL-TIER-BLOCKED:{json}` wire shape — boot.js's existing
    // modal handler parses it identically.
    enforce_attachment_tier_gate(state)?;

    let mime = crate::attachment_wire::mime_for_filename(&original_filename)
        .ok_or_else(|| "OSL: unsupported file extension".to_string())?;
    let original_bytes = STANDARD
        .decode(&original_bytes_b64)
        .map_err(|e| format!("OSL: original_bytes b64 decode: {e}"))?;

    // Fresh attachment AEAD key. Lives only here + inside the
    // ciphered cover; never returned to JS.
    let key_bytes = random::random_bytes(32);
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key_bytes);
    let att_key = crypto::aead::Key::from_bytes(key_arr);

    // Build the CBOR-encoded envelope (single-attachment list).
    let env = crate::control_messages::AttachmentEnvelope {
        attachments: vec![crate::control_messages::AttachmentEnvelopeEntry {
            att_key: key_arr,
            original_filename: original_filename.clone(),
            random_filename: random_filename.clone(),
            mime_type: mime.to_string(),
        }],
    };
    let env_bytes = crate::control_messages::serialize_attachment_envelope(&env)
        .map_err(|e| format!("OSL: serialize attachment envelope: {e}"))?;

    // Resolve identity + recipients for the v=2 multi-recipient
    // wrap of the cover.
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);
    let recipients = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        crate::whitelist::recipients_for_scope(
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    // Cover wire — the same shape MSG_TYPE_ATTACHMENT would have
    // had on the message text; here we embed the raw v=2 wire
    // bytes (no DPC0:: base64-string framing) inside the file.
    let cover_wire_str = crate::wire_v2::encrypt_v2(
        &env_bytes,
        &recipients,
        crate::wire_v2::MSG_TYPE_ATTACHMENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 cover: {e}"))?;
    let cover_bytes = STANDARD
        .decode(
            cover_wire_str
                .strip_prefix("DPC0::")
                .unwrap_or(&cover_wire_str),
        )
        .map_err(|e| format!("OSL: cover wire b64 decode: {e}"))?;

    // Seal the file with the AEAD key + embed the cover.
    let sealed_bytes = crate::attachment_wire::seal_attachment_v2(
        att_key,
        &original_bytes,
        &original_filename,
        &cover_bytes,
    )
    .map_err(|e| format!("OSL: seal_attachment_v2: {e}"))?;

    Ok(SealedAttachmentV2 {
        sealed_b64: STANDARD.encode(&sealed_bytes),
        random_filename,
        mime_type: mime.to_string(),
    })
}

/// Phase 8e: V3 one-shot seal. Same envelope construction as V2 but
/// emits an MP4-wrapped wire (decoy MP4 + `free` box carrying the
/// payload) so the upload MIME is `video/mp4` and Discord renders a
/// video-card preview surface instead of the `.bin` download card.
/// JS calls this with `random_filename` ending in `.mp4` and uploads
/// with `Content-Type: video/mp4`.
#[allow(clippy::too_many_arguments)]
pub fn cmd_osl_seal_attachment_with_cover_v3(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    original_bytes_b64: String,
    original_filename: String,
    random_filename: String,
) -> Result<SealedAttachmentV2, String> {
    // F3.6 attachment-send tier gate. Free users get blocked at
    // the entry with a `OSL-TIER-BLOCKED:{json}` error whose JSON
    // tail parses to `TierGateError::PaidFeatureRequired`. Boot.js
    // detects the prefix and surfaces the upgrade modal.
    enforce_attachment_tier_gate(state)?;

    let mime = crate::attachment_wire::mime_for_filename(&original_filename)
        .ok_or_else(|| "OSL: unsupported file extension".to_string())?;
    let original_bytes = STANDARD
        .decode(&original_bytes_b64)
        .map_err(|e| format!("OSL: original_bytes b64 decode: {e}"))?;

    let key_bytes = random::random_bytes(32);
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key_bytes);
    let att_key = crypto::aead::Key::from_bytes(key_arr);

    let env = crate::control_messages::AttachmentEnvelope {
        attachments: vec![crate::control_messages::AttachmentEnvelopeEntry {
            att_key: key_arr,
            original_filename: original_filename.clone(),
            random_filename: random_filename.clone(),
            mime_type: mime.to_string(),
        }],
    };
    let env_bytes = crate::control_messages::serialize_attachment_envelope(&env)
        .map_err(|e| format!("OSL: serialize attachment envelope: {e}"))?;

    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    drop(id_guard);
    // Resolve recipients with the SAME authority + membership logic as
    // text (recipients_x25519_authz mirrors recipients_for_scope_v3's
    // set) so an image encrypts to the exact same people as a text
    // message in this scope — including the server-lock tiers. The old
    // recipients_for_scope ignored the server/channel lock and used a
    // static member list, so server images could leak to the wrong set.
    let recipients = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        crate::whitelist::recipients_x25519_authz(
            &pm_guard,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        )
    };

    let cover_wire_str = crate::wire_v2::encrypt_v2(
        &env_bytes,
        &recipients,
        crate::wire_v2::MSG_TYPE_ATTACHMENT,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 cover: {e}"))?;
    let cover_bytes = STANDARD
        .decode(
            cover_wire_str
                .strip_prefix("DPC0::")
                .unwrap_or(&cover_wire_str),
        )
        .map_err(|e| format!("OSL: cover wire b64 decode: {e}"))?;

    let sealed_bytes = crate::attachment_wire::seal_attachment_v3(
        att_key,
        &original_bytes,
        &original_filename,
        &cover_bytes,
    )
    .map_err(|e| format!("OSL: seal_attachment_v3: {e}"))?;

    Ok(SealedAttachmentV2 {
        sealed_b64: STANDARD.encode(&sealed_bytes),
        random_filename,
        mime_type: mime.to_string(),
    })
}

fn fetch_wrapped_attachment_key_for_open(
    state: &AppState,
    content_id: &str,
    sender_ref: &str,
) -> Result<[u8; 32], String> {
    if content_id.is_empty() {
        return Err("OSL: wrapped-key open needs a content id".to_string());
    }
    if sender_ref.is_empty() {
        return Err("OSL: wrapped-key open needs a sender binding".to_string());
    }

    let identity = state
        .identity
        .lock()
        .expect("identity mutex poisoned")
        .as_ref()
        .cloned()
        .ok_or_else(|| "OSL: wrapped-key open needs a loaded identity".to_string())?;
    let client = state
        .keyserver
        .lock()
        .expect("keyserver mutex poisoned")
        .as_ref()
        .cloned()
        .ok_or_else(|| "OSL: wrapped-key open needs a key server".to_string())?;

    let expected_sender_osl_id = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(sender_ref)
            .and_then(|entry| entry.osl_user_id.clone())
            .or_else(|| {
                pm.values()
                    .any(|entry| entry.osl_user_id.as_deref() == Some(sender_ref))
                    .then(|| sender_ref.to_string())
            })
            .or_else(|| {
                if sender_ref == identity.user_id.as_str() {
                    Some(identity.user_id.clone())
                } else if identity.discord_snowflake.as_deref() == Some(sender_ref) {
                    Some(identity.user_id.clone())
                } else {
                    None
                }
            })
    }
    .ok_or_else(|| "OSL: wrapped-key open sender is not bound".to_string())?;

    let wrapped = client
        .fetch_wrapped_key(&identity, content_id)
        .map_err(|_| "OSL: wrapped-key open fetch refused".to_string())?;

    if wrapped.content_id.as_str() != content_id
        || wrapped.content_type.as_str() != "attachment"
        || wrapped.system_message_kind.is_some()
        || wrapped.sender_id.as_str() != expected_sender_osl_id.as_str()
        || wrapped.recipient_id.as_str() != identity.user_id.as_str()
        || wrapped.session_version != 1
        || wrapped.blob_version != 1
        || wrapped.share_index != 0
    {
        return Err("OSL: wrapped-key open response did not match this attachment".to_string());
    }

    let key_bytes = STANDARD
        .decode(&wrapped.wrapped_share_blob)
        .map_err(|_| "OSL: wrapped-key open key blob is malformed".to_string())?;
    if key_bytes.len() != 32 {
        return Err("OSL: wrapped-key open key blob has the wrong length".to_string());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&key_bytes);
    Ok(key)
}

/// Phase 8d: one-shot open. Splits the file into (cover, filename,
/// payload), decrypts the cover via the existing v=2 path, recovers
/// the per-attachment AEAD key from the envelope, then decrypts the
/// payload. Backwards-compatible with V1 files (signaled by the
/// empty cover from `open_attachment_v2_split`) — falls back to the
/// caller-supplied legacy `att_key_b64` argument for V1 only. If
/// that legacy local key is absent, the V1 branch consumes the
/// authenticated server wrapped-key row for `discord_message_id`.
///
/// Phase 8e: open path now chains V3 → V2 → V1 magic detection via
/// `open_attachment_v3_split`. JS callers don't need to know which
/// wire version they're feeding in.
pub fn cmd_osl_open_attachment_v2(
    state: &AppState,
    sender_discord_id: String,
    scope_input: Option<crate::scope::ScopeInput>,
    file_bytes_b64: String,
    legacy_att_key_b64: Option<String>,
    discord_message_id: Option<String>,
) -> Result<crate::attachment_wire::OpenedAttachment, String> {
    // Burn enforcement on attachment open. Two layers:
    if let Some(input) = scope_input.as_ref() {
        let scope: crate::scope::Scope = input
            .clone()
            .try_into()
            .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
        // (1) SCOPE burn by the SENDER. A burn destroys the burner's
        // messages AND attachments in the scope — but attachments
        // decrypt from a self-contained att_key in the envelope, not
        // the store wrapped_keys the text-burn wipes, so without this
        // check a burned sender's images stayed openable. If the
        // attachment's sender burned this scope, refuse.
        {
            let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
            if crate::whitelist::is_burned_in_scope(&pm, &scope, &sender_discord_id) {
                tracing::info!(
                    sender = %crate::log_id::log_id(&sender_discord_id),
                    "[OSL] attachment open blocked: scope burned by sender"
                );
                return Err(format!(
                    "OSL: attachment open blocked: scope burned by sender {sender_discord_id}",
                    sender_discord_id = crate::log_id::log_id(&sender_discord_id)
                ));
            }
        }
        // (2) Per-message kill list (9-A1c): refuse a specifically-
        // burned message even if the scope was later unburned.
        if let Some(msg_id) = discord_message_id.as_deref() {
            if is_message_in_burn_kill_list(state, &scope, msg_id) {
                tracing::info!(
                    msg_id = %crate::log_id::log_id(msg_id),
                    "[OSL] attachment open blocked: in_burn_kill_list"
                );
                return Err(format!(
                    "OSL: attachment open blocked: msg={msg_id} reason=in_burn_kill_list",
                    msg_id = crate::log_id::log_id(msg_id)
                ));
            }
        }
    }

    let file_bytes = STANDARD
        .decode(&file_bytes_b64)
        .map_err(|e| format!("OSL: file_bytes b64 decode: {e}"))?;
    let (cover_bytes, filename, payload_bytes) =
        crate::attachment_wire::open_attachment_v3_split(&file_bytes)
            .map_err(|e| format!("OSL: open_attachment_v3_split: {e}"))?;

    // Recover the attachment AEAD key. V2 path: decrypt the
    // embedded cover via v=2 (uses our SK + sender PK + scope
    // whitelist gate). V1 path: trust the caller-supplied
    // att_key_b64 (legacy Phase 8/8c flow), or fetch the keyserver
    // wrapped-key row when the local key was deliberately withheld.
    let att_key_arr: [u8; 32] = if !cover_bytes.is_empty() {
        // Build the DPC0:: string the v=2 decoder expects.
        let cover_wire = format!("DPC0::{}", STANDARD.encode(&cover_bytes));
        // V2 cover MUST be MSG_TYPE_ATTACHMENT — dispatch through
        // cmd_osl_decrypt_message_v2 to honour the scope gate.
        let recovered = cmd_osl_decrypt_message_v2(
            state,
            discord_message_id,
            // channel_id is unused for MSG_TYPE_ATTACHMENT processing.
            String::new(),
            sender_discord_id,
            cover_wire,
            scope_input,
            None,
        )?;
        if !recovered.starts_with(OSL_RESULT_ATTACHMENT_PREFIX) {
            return Err(format!(
                "OSL: V2 cover did not decode to attachment sentinel: {recovered}"
            ));
        }
        let json_part = recovered.trim_start_matches(OSL_RESULT_ATTACHMENT_PREFIX);
        let v: serde_json::Value = serde_json::from_str(json_part)
            .map_err(|e| format!("OSL: V2 cover sentinel JSON: {e}"))?;
        let arr = v["attachments"]
            .as_array()
            .ok_or_else(|| "OSL: V2 cover missing attachments[]".to_string())?;
        if arr.is_empty() {
            return Err("OSL: V2 cover attachments[] is empty".to_string());
        }
        // V2 currently has one entry per file (multi-file
        // messages get one cover per file, embedded in each
        // file's wire).
        let entry = &arr[0];
        let key_b64 = entry["attKey"]
            .as_str()
            .ok_or_else(|| "OSL: V2 cover missing attKey".to_string())?;
        let key_bytes = STANDARD
            .decode(key_b64)
            .map_err(|e| format!("OSL: V2 cover attKey b64: {e}"))?;
        if key_bytes.len() != 32 {
            return Err(format!(
                "OSL: V2 cover attKey length {} != 32",
                key_bytes.len()
            ));
        }
        let mut k = [0u8; 32];
        k.copy_from_slice(&key_bytes);
        k
    } else {
        if let Some(b64) = legacy_att_key_b64 {
            // V1 local-key compatibility path.
            let key_bytes = STANDARD
                .decode(&b64)
                .map_err(|e| format!("OSL: legacy att_key b64: {e}"))?;
            if key_bytes.len() != 32 {
                return Err(format!(
                    "OSL: legacy att_key length {} != 32",
                    key_bytes.len()
                ));
            }
            let mut k = [0u8; 32];
            k.copy_from_slice(&key_bytes);
            k
        } else {
            let content_id = match discord_message_id.as_deref() {
                Some(content_id) => content_id,
                None => return Err("OSL: V1 file with no legacy att_key supplied".to_string()),
            };
            fetch_wrapped_attachment_key_for_open(state, content_id, &sender_discord_id)?
        }
    };
    let file_key = crypto::aead::Key::from_bytes(att_key_arr);
    let plaintext = crypto::attachment::decrypt_attachment(file_key, &payload_bytes)
        .map_err(|e| format!("OSL: decrypt_attachment: {e:?}"))?;
    let mime = crate::attachment_wire::mime_for_filename(&filename)
        .ok_or_else(|| "OSL: unsupported file extension on decrypted name".to_string())?;
    Ok(crate::attachment_wire::OpenedAttachment {
        plaintext_b64: STANDARD.encode(&plaintext),
        original_filename: filename,
        mime_type: mime.to_string(),
    })
}

#[cfg(all(test))]
mod wrapped_key_open_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    fn one_shot_keyserver(response_body: String) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let _ = tx.send(request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://127.0.0.1:{port}"), rx)
    }

    fn state_with_wrapped_key_server(
        identity: keystore::Identity,
        response_body: String,
    ) -> (AppState, mpsc::Receiver<String>) {
        let (base_url, rx) = one_shot_keyserver(response_body);
        let state = AppState::new();
        *state.identity.lock().unwrap() = Some(identity);
        *state.keyserver.lock().unwrap() = Some(KeyServerClient::new(base_url).unwrap());
        (state, rx)
    }

    #[test]
    fn v1_attachment_open_fetches_wrapped_key_when_local_key_absent() {
        let key = [9u8; 32];
        let sealed = crate::attachment_wire::seal_attachment(
            crypto::aead::Key::from_bytes(key),
            b"wrapped-key plaintext",
            "wrapped.png",
        )
        .unwrap();
        let mut recipient = keystore::generate_identity("recipient-osl".to_string());
        recipient.discord_snowflake = Some("recipient-discord".to_string());
        let response_body = serde_json::json!({
            "content_id": "content-1",
            "content_type": "attachment",
            "system_message_kind": null,
            "sender_id": "sender-osl",
            "recipient_id": "recipient-osl",
            "session_version": 1,
            "share_index": 0,
            "wrapped_share_blob": STANDARD.encode(key),
            "blob_version": 1,
            "single_use": true,
            "display_duration_seconds": null,
            "expires_at": "2026-07-29T20:00:00Z",
            "created_at": "2026-07-29T19:00:00Z"
        })
        .to_string();
        let (state, rx) = state_with_wrapped_key_server(recipient, response_body);
        state.peer_map.lock().unwrap().insert(
            "sender-discord".to_string(),
            crate::peer_map::legacy_entry("sender-osl"),
        );

        let opened = cmd_osl_open_attachment_v2(
            &state,
            "sender-discord".to_string(),
            None,
            STANDARD.encode(sealed),
            None,
            Some("content-1".to_string()),
        )
        .unwrap();

        assert_eq!(
            STANDARD.decode(opened.plaintext_b64).unwrap(),
            b"wrapped-key plaintext"
        );
        assert_eq!(opened.original_filename, "wrapped.png");
        let request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(request.starts_with("GET /v1/wrapped-keys/content-1?"));
        assert!(request.contains("requester_id=recipient-osl"));
        assert!(request.contains("recipient_id=recipient-osl"));
    }

    #[test]
    fn v1_attachment_open_refuses_unbound_sender_before_fetch() {
        let key = [7u8; 32];
        let sealed = crate::attachment_wire::seal_attachment(
            crypto::aead::Key::from_bytes(key),
            b"must not open",
            "wrapped.png",
        )
        .unwrap();
        let recipient = keystore::generate_identity("recipient-osl".to_string());
        let response_body = serde_json::json!({
            "content_id": "content-2",
            "content_type": "attachment",
            "system_message_kind": null,
            "sender_id": "sender-osl",
            "recipient_id": "recipient-osl",
            "session_version": 1,
            "share_index": 0,
            "wrapped_share_blob": STANDARD.encode(key),
            "blob_version": 1,
            "single_use": true,
            "display_duration_seconds": null,
            "expires_at": "2026-07-29T20:00:00Z",
            "created_at": "2026-07-29T19:00:00Z"
        })
        .to_string();
        let (state, rx) = state_with_wrapped_key_server(recipient, response_body);

        let err = cmd_osl_open_attachment_v2(
            &state,
            "unbound-sender".to_string(),
            None,
            STANDARD.encode(sealed),
            None,
            Some("content-2".to_string()),
        )
        .unwrap_err();

        assert_eq!(err, "OSL: wrapped-key open sender is not bound");
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    }
}

/// Layer 10 / Phase 7b: send a burn marker for `scope` to the
/// channel members who'd have been able to decrypt content in it
/// (so they wipe their decryption capability).
///
/// Recipient set is computed via `recipients_for_scope` **before**
/// the local burn-state mutation lands — callers must call this
/// before `cmd_osl_apply_burn` updates `peer_map.burned_scopes`,
/// otherwise the burned recipients would be filtered out of the
/// recipient list and never receive the burn notice. The Tauri
/// wrapper in `cmd_osl_unwhitelist_scope` enforces this ordering.
pub fn cmd_osl_send_burn_marker(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<String, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let id_guard = state.identity.lock().expect("identity mutex poisoned");
    let identity = id_guard
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let sender_sk = identity.x25519_secret.clone();
    let self_pk = identity.x25519_public;
    let self_mlkem_pub = identity.mlkem_encapsulation_key();
    drop(id_guard);

    // Probe-2 Rust Bug 8 + Probe-3 follow-up: UNION the legacy
    // per-peer resolver with the scope-flag-aware v=3 resolver so
    // both whitelist models contribute. Legacy alone misses
    // server-header / channel-flag whitelisted peers that have no
    // per-peer outgoing_whitelists entry. v3 alone regresses peers
    // missing ML-KEM keys (pre-9-A1 legacy entries) since v3 is
    // strict about PQ-hybrid keys. Union covers both; the burn
    // marker still rides v=2 (X25519-only) so we only collect
    // X25519 pubs.
    let recipients = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        let mut out: Vec<crypto::x25519::PublicKey> = crate::whitelist::recipients_for_scope(
            &pm_guard,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
        );
        let mut seen: std::collections::HashSet<[u8; 32]> =
            out.iter().map(|pk| *pk.as_bytes()).collect();
        match crate::whitelist::recipients_for_scope_v3(
            &pm_guard,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
            &self_mlkem_pub,
        ) {
            Ok(v3) => {
                for (_did, rcp) in v3 {
                    if seen.insert(*rcp.x25519_pub.as_bytes()) {
                        out.push(rcp.x25519_pub);
                    }
                }
            }
            Err(e) => {
                let scope_storage_key = scope.storage_key();
                tracing::debug!(
                    scope = %crate::log_id::log_id(&scope_storage_key),
                    error = %e,
                    "OSL: burn_marker v3 union failed; using legacy \
                     recipients_for_scope result only"
                );
            }
        }
        out
    };
    if recipients.len() <= 1 {
        return Err("no_whitelisted_recipients".to_string());
    }

    let marker = crate::control_messages::BurnMarker {
        scope,
        burned_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_burn_marker(&marker)
        .map_err(|e| format!("OSL: serialize burn_marker: {e}"))?;
    crate::wire_v2::encrypt_v2(
        &body,
        &recipients,
        crate::wire_v2::MSG_TYPE_BURN,
        &sender_sk,
    )
    .map_err(|e| format!("OSL: encrypt_v2 burn_marker: {e}"))
}

// 9-C1: `cmd_osl_send_whitelist_invitation` /
// `cmd_osl_send_whitelist_response` removed alongside the
// invitation handshake.

// ---- Phase 7b: recv-path branching + helper commands ----
//
// Sentinel return strings for `cmd_osl_decrypt_message_v2` when
// the body is a control message rather than user-visible content.
// boot.js dispatches on these prefixes via `oslHandleDecryptResult`.

/// Returned by the recv path when a v=2 burn marker was
/// processed (peer_map + sqlite mutated). Boot.js re-renders the
/// message as ciphertext when it sees this.
pub const OSL_RESULT_BURN_APPLIED: &str = "__OSL_CONTROL_BURN_APPLIED__";

/// Phase 9-C1: a legacy `MSG_TYPE_WHITELIST_INVITATION` (0x02) or
/// `MSG_TYPE_WHITELIST_RESPONSE` (0x03) message arrived. C1 removed
/// the entire invitation handshake; we silently ignore these so old
/// clients can keep sending them without surfacing as visible
/// ciphertext. boot.js logs + suppresses render.
pub const OSL_RESULT_LEGACY_HANDSHAKE_IGNORED: &str = "__OSL_CONTROL_LEGACY_HANDSHAKE_IGNORED__";

/// Phase 8 attachment-envelope sentinel prefix. The recv path returns
/// `__OSL_CONTROL_ATTACHMENT__|<json-envelope>` when a v=2
/// `MSG_TYPE_ATTACHMENT` message is decrypted; boot.js splits on the
/// `|` and uses the JSON to call `osl_open_attachment` against the
/// CDN-fetched blob.
pub const OSL_RESULT_ATTACHMENT_PREFIX: &str = "__OSL_CONTROL_ATTACHMENT__|";

/// Phase 9-B1: Mode 1 receive sentinels.
///
/// `__OSL_CONTROL_MODE1_INCOMPLETE__|<session_id>|<received>|<total>`
/// — boot.js renders a "(Mode 1 part R/T)" placeholder and waits for
/// the remaining chunks.
pub const OSL_RESULT_MODE1_INCOMPLETE_PREFIX: &str = "__OSL_CONTROL_MODE1_INCOMPLETE__|";

/// `__OSL_CONTROL_MODE1_CONFLICT__` — boot.js drops the in-flight
/// session UI; the chunker on the sender side will need to restart
/// the session.
pub const OSL_RESULT_MODE1_CONFLICT: &str = "__OSL_CONTROL_MODE1_CONFLICT__";

/// `__OSL_CONTROL_MODE1_INVALID__` — chunk bytes failed HMAC or
/// header validation. Boot.js leaves the cover string visible
/// (it's just innocuous English) and logs the rejection.
pub const OSL_RESULT_MODE1_INVALID: &str = "__OSL_CONTROL_MODE1_INVALID__";

const RN_SESSION_DIR_NAME: &str = "rn_sessions";

struct InboundOpened {
    msg_type: u8,
    plaintext: Vec<u8>,
}

/// Phase 7b recv-path entry point. Peeks the wire's version byte
/// after base64 decode and dispatches:
///
/// - v=1 → delegate to legacy `cmd_osl_decrypt_message_with_id`.
/// - v=2 → v=2 decode + match on msg_type:
///   - 0x00 content: return plaintext (9-C1: permissive — no gate).
///   - 0x01 burn marker: apply locally, return `OSL_RESULT_BURN_APPLIED`.
///   - 0x02 / 0x03 legacy handshake: return
///     `OSL_RESULT_LEGACY_HANDSHAKE_IGNORED` (9-C1).
///   - 0x04 attachment envelope: return `OSL_RESULT_ATTACHMENT_PREFIX|<json>`.
/// - v=3 / v=4 / v=5: dispatch to their dedicated decrypt fns above.
/// - v=0x10: OSL-RN bootstrap responder. The branch is wired here but
///   remains compile-time disabled until RN receive is explicitly enabled.
///
/// `scope_input` is optional and currently unused by the gate-free
/// content paths; kept in the signature for the burn / attachment
/// side-effects that still need it.
#[allow(clippy::too_many_arguments)]
pub fn cmd_osl_decrypt_message_v2(
    state: &AppState,
    discord_message_id: Option<String>,
    channel_id: String,
    sender_discord_id: String,
    content: String,
    scope_input: Option<crate::scope::ScopeInput>,
    config_dir: Option<std::path::PathBuf>,
) -> Result<String, String> {
    // 9-B1: Mode 1 envelope handling. If the cover string carries
    // a `DPC1::` prefix, decode it as a Mode 1 chunk and push to
    // the per-channel reassembly buffer. When the buffer completes,
    // re-frame the reassembled wire bytes as `DPC0::<b64>` and fall
    // through to the existing version dispatch below. Incomplete
    // / conflicting / invalid chunks return sentinel strings boot.js
    // renders into UI placeholders.
    let scope_opt: Option<crate::scope::Scope> = match scope_input {
        Some(input) => Some(
            input
                .try_into()
                .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?,
        ),
        None => None,
    };

    let content = if stego::is_mode1(&content) {
        // Mode 1 requires a scope so we know the conversation salt.
        let scope = scope_opt
            .as_ref()
            .ok_or_else(|| "OSL: Mode 1 decode needs scope_input".to_string())?;
        let salt = scope.storage_key().into_bytes();
        let cipher = stego::ConversationCipher::from_salt(&salt);
        let chunk_bytes = match stego::decode_mode1(&cipher, &content) {
            Ok(b) => b,
            Err(e) => {
                tracing::debug!(error = %e, "OSL: Mode 1 decode_mode1 failed");
                return Ok(OSL_RESULT_MODE1_INVALID.to_string());
            }
        };
        let parsed = match stego::parse_chunk(&salt, &chunk_bytes) {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!(error = %e, "OSL: Mode 1 chunk validation failed");
                return Ok(OSL_RESULT_MODE1_INVALID.to_string());
            }
        };
        let now = now_unix_secs() as u64;
        let outcome = {
            let mut bufs = state
                .mode1_reassembly
                .lock()
                .expect("mode1_reassembly mutex poisoned");
            let buf = bufs.entry(channel_id.clone()).or_default();
            buf.push(
                parsed.session_id,
                parsed.chunk_index,
                parsed.total_chunks,
                parsed.payload,
                now,
            )
        };
        match outcome {
            stego::PushOutcome::Incomplete { received, total } => {
                return Ok(format!(
                    "{}{}|{}|{}",
                    OSL_RESULT_MODE1_INCOMPLETE_PREFIX, parsed.session_id, received, total
                ));
            }
            stego::PushOutcome::Conflict => {
                return Ok(OSL_RESULT_MODE1_CONFLICT.to_string());
            }
            stego::PushOutcome::Complete(c) => {
                // Re-frame as DPC0::<b64> and fall through into the
                // version-dispatch block below.
                format!("DPC0::{}", STANDARD.encode(&c.wire_bytes))
            }
        }
    } else {
        content
    };

    // Peek the wire version byte (first byte after DPC0:: base64
    // decode). Routes v=1 → legacy path, v=2 → existing decrypt_v2,
    // v=3 → Phase 9-A1 PQ-hybrid decrypt_v3, and 0x10 → the gated
    // OSL-RN bootstrap responder. Anything else falls through to the
    // legacy v=1 path which surfaces its own errors.
    let version = peek_wire_version(&content);

    // 9-A1c: burn kill list defense-in-depth. If this specific
    // discord_message_id was recorded in the scope's burn entry,
    // refuse to decrypt regardless of whether the scope-level skip
    // cache is currently set. Protects against the manual re-engage
    // path inadvertently reviving old burned ciphertexts.
    if let (Some(scope), Some(msg_id)) = (scope_opt.as_ref(), discord_message_id.as_deref()) {
        if is_message_in_burn_kill_list(state, scope, msg_id) {
            tracing::info!(msg_id = %crate::log_id::log_id(msg_id), "[OSL] decrypt blocked: in_burn_kill_list");
            return Err(format!(
                "OSL: decrypt blocked: msg={msg_id} reason=in_burn_kill_list",
                msg_id = crate::log_id::log_id(msg_id)
            ));
        }
    }

    let recovered = match version {
        Some(crate::wire_v2::WIRE_VERSION_V2) => {
            // v=2 path — X25519-only wrap.
            let id_guard = state.identity.lock().expect("identity mutex poisoned");
            let identity = id_guard
                .as_ref()
                .ok_or_else(|| "OSL: identity not loaded".to_string())?;
            let our_sk = identity.x25519_secret.clone();
            drop(id_guard);
            // v=2 needs the sender pubkey out-of-band (from peer_map
            // or keyserver) because the wire only carries the recipient
            // pubkey-hash prefix.
            let sender_pub = resolve_sender_pubkey(state, &sender_discord_id)?;
            tracing::debug!(wire_version = "v2", "v=2 decode dispatched");
            let opened = crate::wire_v2::decrypt_v2(&content, &our_sk, &sender_pub)
                .map_err(|e| format!("OSL: {e}"))?;
            InboundOpened {
                msg_type: opened.msg_type,
                plaintext: opened.plaintext,
            }
        }
        Some(crate::wire_v2::WIRE_VERSION_V3) => {
            // v=3 path — PQ-hybrid wrap. The wire carries the sender
            // key, but attribution is permitted only when that key is
            // the locally pinned key for the claimed peer.
            let expected_sender =
                resolve_pinned_sender_pubkey(state, &sender_discord_id).map_err(str::to_owned)?;
            let id_guard = state.identity.lock().expect("identity mutex poisoned");
            let identity = id_guard
                .as_ref()
                .ok_or_else(|| "OSL: identity not loaded".to_string())?;
            let our_sk = identity.x25519_secret.clone();
            let our_mlkem_sk = identity.mlkem_decapsulation_key();
            drop(id_guard);
            tracing::debug!(wire_version = "v3", "v=3 decode dispatched");
            let opened = crate::wire_v2::decrypt_v3_for_sender(
                &content,
                &our_sk,
                &our_mlkem_sk,
                &expected_sender,
            )
            .map_err(|_| "OSL: v3 authenticated sender refused".to_string())?;
            InboundOpened {
                msg_type: opened.msg_type,
                plaintext: opened.plaintext,
            }
        }
        Some(crate::wire_v2::WIRE_VERSION_V4) => {
            // Phase 9-A2: v=4 ratcheted single-recipient decode.
            // Parses the wire, runs PQXDH wrap-leg verification,
            // bootstraps OR loads the live DR, advances it via
            // dr.decrypt(...), persists the updated state, and
            // returns the recovered plaintext.
            tracing::debug!(wire_version = "v4", "v=4 decode dispatched");
            let sender_did_for_persist = sender_discord_id.clone();
            let result = decrypt_v4_recv(
                state,
                sender_discord_id,
                content,
                scope_opt,
                config_dir.as_deref(),
            )?;
            // Probe-3 fix: persist user-visible plaintext into the
            // durable MessageStore so a relaunch's recvLoadHistory
            // rehydrates the channel. Sentinel results (control
            // messages, attachments) are skipped by the helper.
            persist_user_plaintext(
                state,
                discord_message_id.as_deref(),
                &channel_id,
                &sender_did_for_persist,
                &result,
            );
            return Ok(result);
        }
        Some(crate::wire_v2::WIRE_VERSION_V5) => {
            // Phase 9-A3: v=5 sender-keys group decode.
            tracing::debug!(wire_version = "v5", "v=5 decode dispatched");
            let sender_did_for_persist = sender_discord_id.clone();
            let result = decrypt_v5_recv(state, sender_discord_id, content, scope_opt)?;
            persist_user_plaintext(
                state,
                discord_message_id.as_deref(),
                &channel_id,
                &sender_did_for_persist,
                &result,
            );
            return Ok(result);
        }
        Some(osl_ratchet_next::WIRE_VERSION_RN) => {
            tracing::debug!(wire_version = "rn", "OSL-RN bootstrap decode dispatched");
            accept_rn_bootstrap_inbound_unknown(
                state,
                &content,
                config_dir.as_deref(),
                crate::wire_rn::RN_WIRE_IN_ENABLED,
            )?
        }
        _ => {
            // v=1 or unknown: preserve the existing Phase 5 path.
            return cmd_osl_decrypt_message_with_id(
                state,
                discord_message_id,
                channel_id,
                sender_discord_id,
                content,
            );
        }
    };

    match recovered.msg_type {
        crate::wire_v2::MSG_TYPE_CONTENT => {
            // 9-C1: permissive decrypt. If we have the keys, we
            // decrypt. Discord's own block feature is the user-facing
            // trust boundary, not an OSL-internal per-scope accept
            // gate. The prior `should_decrypt_from` check is gone.
            let _ = scope_opt;
            let plaintext = String::from_utf8(recovered.plaintext)
                .map_err(|_| "OSL: decrypted plaintext is not valid UTF-8".to_string())?;
            // Probe-3 fix: persist user-visible plaintext into the
            // durable MessageStore so a relaunch's recvLoadHistory
            // rehydrates the channel. v=2 / v=3 content paths share
            // this; sentinel/control paths return their own strings
            // and do not reach here.
            persist_user_plaintext(
                state,
                discord_message_id.as_deref(),
                &channel_id,
                &sender_discord_id,
                &plaintext,
            );
            Ok(plaintext)
        }
        crate::wire_v2::MSG_TYPE_BURN => {
            let marker = crate::control_messages::deserialize_burn_marker(&recovered.plaintext)
                .map_err(|e| format!("OSL: deserialize burn_marker: {e}"))?;
            apply_burn_recv(state, &sender_discord_id, &marker)?;
            Ok(OSL_RESULT_BURN_APPLIED.to_string())
        }
        // 9-C1: legacy whitelist invitation (0x02) + response (0x03).
        // The handshake was removed; we suppress these so old peers'
        // pre-C1 wire bytes don't render as visible ciphertext.
        // Match raw values rather than reintroduce constants.
        0x02 | 0x03 => {
            tracing::info!(
                msg_type = recovered.msg_type,
                sender = %crate::log_id::log_id(&sender_discord_id),
                "OSL: legacy handshake message ignored (C1 removed the invitation flow)"
            );
            Ok(OSL_RESULT_LEGACY_HANDSHAKE_IGNORED.to_string())
        }
        crate::wire_v2::MSG_TYPE_ATTACHMENT => {
            // 9-C1: permissive — no per-scope accept gate.
            let _ = scope_opt;
            let env =
                crate::control_messages::deserialize_attachment_envelope(&recovered.plaintext)
                    .map_err(|e| format!("OSL: deserialize attachment envelope: {e}"))?;
            // 8b: serialize the full attachments list for JS — JS
            // dispatches on the sentinel prefix and iterates the
            // `attachments` array, feeding each entry into
            // `osl_open_attachment` along with the matching CDN-
            // fetched file bytes.
            let attachments_json: Vec<serde_json::Value> = env
                .attachments
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "attKey": STANDARD.encode(e.att_key),
                        "originalFilename": e.original_filename,
                        "randomFilename": e.random_filename,
                        "mimeType": e.mime_type,
                    })
                })
                .collect();
            let json = serde_json::json!({ "attachments": attachments_json });
            Ok(format!("{OSL_RESULT_ATTACHMENT_PREFIX}{json}"))
        }
        crate::wire_v2::MSG_TYPE_SKDM_REQUEST => {
            let _ = scope_opt;
            apply_skdm_request_recv(state, &sender_discord_id, &recovered.plaintext)
        }
        crate::wire_v2::MSG_TYPE_SESSION_RESET => {
            let _ = scope_opt;
            apply_session_reset_recv(state, &sender_discord_id, &recovered.plaintext)
        }
        crate::wire_v2::MSG_TYPE_SENDER_KEY_DISTRIBUTION => {
            // Probe-3 Option-2 step 1: SKDMs now ride v=3 (bundled
            // multi-recipient PQ-hybrid) instead of v=4 (per-peer
            // ratcheted). The v=4 path already routes this msg_type
            // to apply_skdm_recv for backward compat with old peers
            // still emitting v=4 SKDMs; this v=2/v=3 arm handles the
            // new transport.
            let _ = scope_opt;
            apply_skdm_recv(state, &sender_discord_id, &recovered.plaintext)
        }
        other => Err(format!(
            "OSL: v=2 msg_type 0x{other:02x} not supported by this client"
        )),
    }
}

/// Peek the wire version byte. Returns `None` for non-DPC0::
/// content or malformed base64.
fn peek_wire_version(cover: &str) -> Option<u8> {
    let body = cover.strip_prefix("DPC0::")?;
    let bytes = STANDARD.decode(body).ok()?;
    bytes.first().copied()
}

fn rn_session_store(config_dir: Option<&Path>) -> Result<crate::wire_rn::RnSessionStore, String> {
    let dir = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => keystore::osl_config_dir()
            .map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?,
    };
    Ok(crate::wire_rn::RnSessionStore::new(
        dir.join(RN_SESSION_DIR_NAME),
    ))
}

fn local_rn_prekeys_from_identity(
    identity: &keystore::Identity,
) -> Result<osl_ratchet_next::LocalPrekeys, String> {
    let signed_prekey = identity
        .ratchet_initial_secret
        .as_ref()
        .ok_or_else(|| "OSL: secure message identity is not ready".to_string())?;
    let pq_prekey = osl_ratchet_next::KemSecret::from_bytes(identity.mlkem_secret_bytes())
        .map_err(|_| "OSL: secure message identity is not ready".to_string())?;
    Ok(osl_ratchet_next::LocalPrekeys {
        identity: osl_ratchet_next::XSecret::from_bytes(*identity.x25519_secret.as_bytes()),
        signed_prekey: osl_ratchet_next::XSecret::from_bytes(*signed_prekey.as_bytes()),
        one_time_prekeys: Vec::new(),
        pq_prekey,
    })
}

fn accept_rn_bootstrap_inbound_unknown(
    state: &AppState,
    content: &str,
    config_dir: Option<&Path>,
    rn_wire_in_enabled: bool,
) -> Result<InboundOpened, String> {
    if !rn_wire_in_enabled {
        return Err("OSL: secure message format is not available".to_string());
    }

    accept_rn_bootstrap_inbound_unknown_with_selected_sealer(
        state,
        content,
        config_dir,
        rn_wire_in_enabled,
    )
}

fn accept_rn_bootstrap_inbound_unknown_with_selected_sealer(
    state: &AppState,
    content: &str,
    config_dir: Option<&Path>,
    rn_wire_in_enabled: bool,
) -> Result<InboundOpened, String> {
    if !rn_wire_in_enabled {
        return Err("OSL: secure message format is not available".to_string());
    }

    let (local, own_identity_public, own_mlkem768_ek) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        (
            local_rn_prekeys_from_identity(identity)?,
            *identity.x25519_public.as_bytes(),
            identity.mlkem_public_bytes,
        )
    };

    let store = rn_session_store(config_dir)?;
    let (_session, opened) = crate::wire_rn::accept_and_persist(
        &store,
        &local,
        &own_identity_public,
        &own_mlkem768_ek,
        content,
        crate::wire_rn::RN_CONTEXT_DISCORD_MANUAL,
        osl_ratchet_next::SessionParams::default(),
    )
    .map_err(|_| "OSL: secure message could not be opened".to_string())?;

    Ok(InboundOpened {
        msg_type: opened.msg_type,
        plaintext: opened.plaintext,
    })
}

fn accept_rn_bootstrap_inbound_unknown_with_sealer(
    state: &AppState,
    content: &str,
    config_dir: Option<&Path>,
    sealer: &dyn keystore::sealer::Sealer,
    rn_wire_in_enabled: bool,
) -> Result<InboundOpened, String> {
    if !rn_wire_in_enabled {
        return Err("OSL: secure message format is not available".to_string());
    }

    let (local, own_identity_public, own_mlkem768_ek) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        (
            local_rn_prekeys_from_identity(identity)?,
            *identity.x25519_public.as_bytes(),
            identity.mlkem_public_bytes,
        )
    };

    let store = rn_session_store(config_dir)?;
    let (_session, opened) = crate::wire_rn::accept_and_persist_with_sealer(
        &store,
        sealer,
        &local,
        &own_identity_public,
        &own_mlkem768_ek,
        content,
        crate::wire_rn::RN_CONTEXT_DISCORD_MANUAL,
        osl_ratchet_next::SessionParams::default(),
    )
    .map_err(|_| "OSL: secure message could not be opened".to_string())?;

    Ok(InboundOpened {
        msg_type: opened.msg_type,
        plaintext: opened.plaintext,
    })
}

#[cfg(all(test))]
mod rn_inbound_unknown_tests {
    use super::*;
    use keystore::sealer::MemorySealer;
    use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
    use tempfile::TempDir;

    fn identity_from_rn_prekeys(
        user_id: &str,
        local: &osl_ratchet_next::LocalPrekeys,
        bundle: &osl_ratchet_next::PeerBundle,
    ) -> keystore::Identity {
        let (ed_secret, ed_public) = crypto::ed25519::generate_keypair();
        let mut identity = keystore::Identity::from_bytes(
            user_id.to_string(),
            *local.identity.as_bytes(),
            *local.identity.public().as_bytes(),
            *ed_secret.as_bytes(),
            *ed_public.as_bytes(),
            local.pq_prekey.to_bytes(),
            bundle.pq_prekey.to_bytes(),
        );
        identity.ratchet_initial_secret = Some(crypto::x25519::SecretKey::from_bytes(
            *local.signed_prekey.as_bytes(),
        ));
        identity.ratchet_initial_pub = Some(crypto::x25519::PublicKey::from_bytes(
            *bundle.signed_prekey.as_bytes(),
        ));
        identity
    }

    fn rn_bootstrap_fixture() -> (
        AppState,
        TempDir,
        String,
        [u8; 32],
        keystore::sealer::MemorySealer,
    ) {
        let mut rng = seeded_rng(0xB62);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_identity, alice_identity_pub) =
            osl_ratchet_next::primitives::x25519_keypair(&mut rng);
        let binding = osl_ratchet_next::Negotiation::for_rn(
            bob_bundle.identity.as_bytes(),
            &bob_bundle.pq_prekey.to_bytes(),
            alice_identity_pub.as_bytes(),
            crate::wire_rn::RN_CONTEXT_DISCORD_MANUAL,
        )
        .digest()
        .expect("negotiation binding");
        let mut alice = osl_ratchet_next::Session::initiate_bound(
            &alice_identity,
            &bob_bundle,
            Some(&binding),
            osl_ratchet_next::SessionParams::default(),
            &mut rng,
        )
        .expect("initiate");
        let wire = alice
            .encrypt(crate::wire_v2::MSG_TYPE_CONTENT, b"rn hello", &mut rng)
            .expect("encrypt");

        let state = AppState::new();
        *state.identity.lock().expect("identity mutex poisoned") =
            Some(identity_from_rn_prekeys("bob", &bob_prekeys, &bob_bundle));
        (
            state,
            TempDir::new().expect("tempdir"),
            wire,
            *alice_identity_pub.as_bytes(),
            MemorySealer::new(),
        )
    }

    #[test]
    fn rn_wire_version_is_recognized_but_refused_while_compile_gate_is_false() {
        let (state, dir, wire, _peer, _sealer) = rn_bootstrap_fixture();

        let err = cmd_osl_decrypt_message_v2(
            &state,
            Some("msg-rn-disabled".to_string()),
            "channel-rn-disabled".to_string(),
            "sender-rn-disabled".to_string(),
            wire,
            None,
            Some(dir.path().to_path_buf()),
        )
        .expect_err("production command must not accept RN while hard gate is false");

        assert!(
            err.contains("secure message format is not available"),
            "{err}"
        );
        assert!(
            !dir.path().join(RN_SESSION_DIR_NAME).exists(),
            "disabled RN branch must refuse before creating responder state"
        );
    }

    #[test]
    fn enabled_test_helper_accepts_bootstrap_and_persists_responder_state() {
        let (state, dir, wire, peer_identity, sealer) = rn_bootstrap_fixture();
        let opened = accept_rn_bootstrap_inbound_unknown_with_sealer(
            &state,
            &wire,
            Some(dir.path()),
            &sealer,
            true,
        )
        .expect("accept RN bootstrap");

        assert_eq!(opened.msg_type, crate::wire_v2::MSG_TYPE_CONTENT);
        assert_eq!(opened.plaintext, b"rn hello".to_vec());

        let store = crate::wire_rn::RnSessionStore::new(dir.path().join(RN_SESSION_DIR_NAME));
        assert!(store
            .load_session_with_sealer(&peer_identity, &sealer)
            .expect("load session")
            .is_some());
        assert!(store
            .load_pin(&peer_identity)
            .expect("load pin")
            .is_pinned_to_rn());
    }

    #[test]
    fn unit_b78_initiator_and_command_responder_bootstrap_in_one_process() {
        assert!(
            !crate::wire_rn::RN_WIRE_IN_ENABLED,
            "b78 must not enable the production OSL-RN wire-in gate"
        );

        let mut rng = seeded_rng(0xB78);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let bob_identity_public = bob_prekeys.identity.public();
        let bob_mlkem768_ek = bob_bundle.pq_prekey.to_bytes();
        let (alice_identity, alice_identity_public) =
            osl_ratchet_next::primitives::x25519_keypair(&mut rng);

        let alice_dir = TempDir::new().expect("alice tempdir");
        let alice_store = crate::wire_rn::RnSessionStore::new(alice_dir.path().join("rn"));
        let sealer = MemorySealer::new();
        let mut alice_session = crate::wire_rn::initiate_and_persist(
            &alice_store,
            &sealer,
            &alice_identity,
            alice_identity_public.as_bytes(),
            &bob_bundle,
            keystore::client::PeerCapabilities::Verified(keystore::client::RN_CAP_WIRE_RN),
            &bob_mlkem768_ek,
            crate::wire_rn::RN_CONTEXT_DISCORD_MANUAL,
            osl_ratchet_next::SessionParams::default(),
        )
        .expect("initiator persists bootstrap session");

        let wire = alice_session
            .encrypt(
                crate::wire_v2::MSG_TYPE_CONTENT,
                b"b78 bootstrap hello",
                &mut rng,
            )
            .expect("bootstrap encrypt");

        let bob_state = AppState::new();
        *bob_state.identity.lock().expect("identity mutex poisoned") = Some(
            identity_from_rn_prekeys("bob-b78", &bob_prekeys, &bob_bundle),
        );
        let bob_dir = TempDir::new().expect("bob tempdir");
        let opened = accept_rn_bootstrap_inbound_unknown_with_sealer(
            &bob_state,
            &wire,
            Some(bob_dir.path()),
            &sealer,
            true,
        )
        .expect("command responder accepts bootstrap");

        assert_eq!(opened.msg_type, crate::wire_v2::MSG_TYPE_CONTENT);
        assert_eq!(opened.plaintext, b"b78 bootstrap hello".to_vec());

        assert!(alice_store
            .load_session(bob_bundle.identity.as_bytes(), &sealer)
            .expect("load alice session")
            .is_some());
        assert!(alice_store
            .load_pin(bob_bundle.identity.as_bytes())
            .expect("load alice pin")
            .is_pinned_to_rn());

        let bob_store =
            crate::wire_rn::RnSessionStore::new(bob_dir.path().join(RN_SESSION_DIR_NAME));
        assert!(bob_store
            .load_session(alice_identity_public.as_bytes(), &sealer)
            .expect("load bob session")
            .is_some());
        assert!(bob_store
            .load_pin(alice_identity_public.as_bytes())
            .expect("load bob pin")
            .is_pinned_to_rn());
        assert_eq!(
            bob_identity_public.as_bytes(),
            bob_bundle.identity.as_bytes(),
            "fixture identity must match its published bundle"
        );
    }
}

/// Phase 9-A2: receive-side v=4 dispatch. Returns the message-type
/// string (matching the v=2/v=3 dispatcher's return convention).
/// Persists advanced DR state to peer_map on success.
fn decrypt_v4_recv(
    state: &AppState,
    sender_discord_id: String,
    content: String,
    scope_opt: Option<crate::scope::Scope>,
    _config_dir: Option<&std::path::Path>,
) -> Result<String, String> {
    use crypto::ratchet::{DoubleRatchet, RatchetStateOnDisk, SessionContext, SESSION_VERSION_V1};

    let (our_sk, our_mlkem_sk, our_pk, self_mlkem_pub_bytes) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        (
            identity.x25519_secret.clone(),
            identity.mlkem_decapsulation_key(),
            identity.x25519_public,
            identity.mlkem_public_bytes.to_vec(),
        )
    };

    // Send-vs-receive stale-key triage: log the receiver's OWN
    // identity X25519 slot-hash that decrypt_v4's slot scan compares
    // each wire slot against. If this differs from the sender's
    // `recipient_slot_hash` log line, the sender wrapped to a key
    // this machine's current identity no longer holds (NoMatchingSlot
    // = "not a recipient of this message"). Logged BEFORE decrypt_v4
    // so it is visible even when the slot scan fails.
    tracing::info!(
        target: "osl::v4",
        sender_did = %sender_discord_id,
        our_x25519_b64 = %STANDARD.encode(our_pk.as_bytes()),
        our_slot_hash = %STANDARD.encode(crate::wire_v2::pubkey_hash_prefix(&our_pk)),
        "OSL: v=4 recv — slot scan will match against our identity X25519"
    );

    let parsed = crate::wire_v2::decrypt_v4(&content, &our_sk, &our_mlkem_sk)
        .map_err(|e| format!("OSL: v=4 decode: {e}"))?;

    // Peer ML-KEM pub for the AD binding (from peer_map; not on the
    // wire). If absent, the AD won't match the sender's and the DR
    // body AEAD will fail with a clear error.
    let peer_mlkem_pub_bytes: Vec<u8> = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm_guard
            .get(&sender_discord_id)
            .and_then(|pe| pe.ik_mlkem768_pub.as_deref())
            .and_then(|b64| STANDARD.decode(b64).ok())
            .unwrap_or_default()
    };

    // Self's discord id is needed for the symmetric conversation_id
    // used as the DR's AD binding. Read from peer_map (the local
    // entry with is_self=true) — falls back to "self" if not yet
    // registered (verify path will populate it later).
    let self_did = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm_guard
            .iter()
            .find_map(|(did, pe)| {
                if pe.is_self == Some(true) {
                    Some(did.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "self".to_string())
    };

    let ctx = SessionContext {
        local_ik_x25519_pub: our_pk,
        local_ik_mlkem_pub: self_mlkem_pub_bytes,
        peer_ik_x25519_pub: parsed.sender_ik_pub,
        peer_ik_mlkem_pub: peer_mlkem_pub_bytes,
        conversation_id: dm_conversation_id(&self_did, &sender_discord_id),
        session_version: SESSION_VERSION_V1,
    };

    // Load OR bootstrap. Sender sets bootstrap=true on the first
    // message it ever sends in a given DR session (i.e. while it
    // still has no `ratchet_state` itself). The receiver may have
    // already bootstrapped from a prior out-of-order arrival — in
    // that case we just load the existing state and the
    // bootstrap-flag-was-true case is idempotent.
    let existing_state = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm_guard
            .get(&sender_discord_id)
            .and_then(|pe| pe.ratchet_state.clone())
    };
    let mut dr: DoubleRatchet = match (existing_state, parsed.bootstrap) {
        (Some(disk), _) => disk
            .try_into()
            .map_err(|e| format!("OSL: v=4: load ratchet state: {e}"))?,
        (None, true) => {
            let ratchet_initial_secret = {
                let id_guard = state.identity.lock().expect("identity mutex poisoned");
                id_guard
                    .as_ref()
                    .and_then(|i| i.ratchet_initial_secret.clone())
                    .ok_or_else(|| {
                        "OSL: v=4 bootstrap: local identity missing \
                         ratchet_initial_secret"
                            .to_string()
                    })?
            };
            DoubleRatchet::new_responder(&parsed.session_key, &ratchet_initial_secret, ctx)
                .map_err(|e| format!("OSL: v=4 bootstrap: new_responder: {e}"))?
        }
        (None, false) => {
            // Act-on-symptom evidence: a real desync from this peer.
            // Authorizes honoring a later SESSION_RESET from them.
            state
                .recovery_guard
                .lock()
                .expect("recovery_guard mutex poisoned")
                .note_v4_failure(&sender_discord_id, now_unix_secs());
            return Err(format!(
                "OSL: v=4 continuation: peer {sender_discord_id} has no ratchet_state \
                 — bootstrap flag was false but local state is None (desync)",
                sender_discord_id = crate::log_id::log_id(&sender_discord_id)
            ));
        }
    };

    let em = crypto::ratchet::EncryptedMessage {
        header_nonce: parsed.enc_header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.body_nonce,
        ciphertext: parsed.body_ct,
    };
    let plaintext_bytes = match dr.decrypt(&em) {
        Ok(pt) => pt,
        Err(e) => {
            // Act-on-symptom evidence: this peer's v=4 traffic failed
            // to decrypt (e.g. "header AEAD failed" ratchet desync).
            // Recorded for SESSION_RESET forensics. Authenticated,
            // fresh, non-replayed resets no longer require this
            // symptom to self-heal one-directional desyncs.
            state
                .recovery_guard
                .lock()
                .expect("recovery_guard mutex poisoned")
                .note_v4_failure(&sender_discord_id, now_unix_secs());
            return Err(format!("OSL: v=4 dr.decrypt: {e}"));
        }
    };

    // Persist updated DR state.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(sender_discord_id.clone()).or_default();
        pe.ratchet_state = Some(RatchetStateOnDisk::from(&dr));
    }
    persist_peer_map_now(state);

    // Phase 9-A3: v=4 now carries MSG_TYPE_SENDER_KEY_DISTRIBUTION
    // for group/server scopes' sender-keys bootstrap. Route that
    // body to the SKDM handler — the plaintext stays in Rust, JS
    // never sees it.
    if parsed.msg_type == crate::wire_v2::MSG_TYPE_SENDER_KEY_DISTRIBUTION {
        return apply_skdm_recv(state, &sender_discord_id, &plaintext_bytes);
    }

    // For all other v=4 msg_types, only MSG_TYPE_CONTENT is currently
    // supported. Burn / invitation / response routes remain on v=3.
    if parsed.msg_type != crate::wire_v2::MSG_TYPE_CONTENT {
        return Err(format!(
            "OSL: v=4 msg_type 0x{:02x} not supported in v=4 (use v=3 for fan-out)",
            parsed.msg_type
        ));
    }

    // 9-C1: permissive decrypt — no per-scope accept gate.
    let _ = scope_opt;
    String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: v=4 decrypted plaintext is not valid UTF-8".to_string())
}

/// Phase 9-A3: SKDM control sentinel — boot.js ignores messages
/// that decode to this string instead of rendering them.
pub const OSL_RESULT_SKDM_APPLIED: &str = "__OSL_CONTROL_SKDM_APPLIED__";

/// Auto-recovery: an inbound SKDM_REQUEST was honored and produced a
/// fresh SKDM wire for the requester. Full result is this prefix +
/// the DPC0:: wire string; boot.js POSTs that wire back to the
/// originating channel (the requester's recv path then applies it).
pub const OSL_RESULT_SKDM_REREQUEST_PREFIX: &str = "__OSL_CONTROL_SKDM_REREQUEST__|";

/// Auto-recovery: an inbound SESSION_RESET was honored — we dropped
/// our v=4 ratchet for the sender; the next v=4 send re-handshakes.
/// Control sentinel; boot.js suppresses render (no user content).
pub const OSL_RESULT_SESSION_RESET_APPLIED: &str = "__OSL_CONTROL_SESSION_RESET_APPLIED__";

/// Auto-recovery: an inbound recovery request was dropped by a guard
/// (stale / replayed / throttled / no corroborating local symptom).
/// Control sentinel; boot.js suppresses render. Distinct from
/// "applied" so logs can tell a no-op from an action.
pub const OSL_RESULT_RECOVERY_IGNORED: &str = "__OSL_CONTROL_RECOVERY_IGNORED__";

/// Auto-recovery inbound handler for `MSG_TYPE_SKDM_REQUEST` (0x06):
/// a peer says it never received our sender-key for `scope`. If we
/// genuinely have a sender chain for that scope and the request
/// passes the throttle/replay/staleness guards, re-emit ONE SKDM
/// (v=4-wrapped) addressed to that requester only, bypassing the
/// normal "already installed" short-circuit. Returns the SKDM wire
/// behind [`OSL_RESULT_SKDM_REREQUEST_PREFIX`] for boot.js to POST,
/// or [`OSL_RESULT_RECOVERY_IGNORED`] for any no-op path.
fn apply_skdm_request_recv(
    state: &AppState,
    requester_discord_id: &str,
    payload_bytes: &[u8],
) -> Result<String, String> {
    let req = crate::control_messages::deserialize_skdm_request(payload_bytes)
        .map_err(|e| format!("OSL: SKDM_REQUEST: deserialize: {e}"))?;
    let now = now_unix_secs();

    {
        let mut g = state
            .recovery_guard
            .lock()
            .expect("recovery_guard mutex poisoned");
        if !g.accept_inbound(
            requester_discord_id,
            crate::recovery::RecoveryKind::SkdmRequest,
            &req.nonce,
            req.requested_at,
            now,
        ) {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                reason = "recovery_guard_rejected",
                requested_at = req.requested_at,
                now = now,
                "OSL: SKDM_REQUEST IGNORED — guard rejected (replay/staleness/throttle)"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        }
    }

    let scope = match crate::scope::Scope::parse(&req.scope_storage_key) {
        Some(s) => s,
        None => {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                scope_storage_key = %crate::log_id::log_id(&req.scope_storage_key),
                reason = "invalid_scope",
                "OSL: SKDM_REQUEST IGNORED — scope_storage_key didn't parse"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        }
    };
    let scope_key = scope.storage_key();

    // We can only redistribute a chain we actually own. If there is
    // no sender chain for this scope we are not a v=5 sender here —
    // benign no-op (the requester is asking the wrong peer, or the
    // scope was never keyed).
    let (chain_id, rotation_root, physical_device_id) = {
        use crypto::sender_keys::SenderKeyState;
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        let Some(disk) = g.states.get(&scope_key) else {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                scope = %crate::log_id::log_id(&scope_key),
                reason = "no_sender_key_state",
                known_scopes = g.states.len(),
                "OSL: SKDM_REQUEST IGNORED — no sender_key_state for scope \
                 (we never sent a v=5 message here; nothing to redistribute)"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        };
        let sks: SenderKeyState = disk
            .clone()
            .try_into()
            .map_err(|e| format!("OSL: SKDM_REQUEST: load sender_key_state: {e}"))?;
        match sks.sender_chain() {
            Some(c) => (
                c.current_chain_id(),
                c.rotation_root_bytes(),
                c.physical_device_id(),
            ),
            None => {
                tracing::warn!(
                    requester = %crate::log_id::log_id(requester_discord_id),
                    scope = %crate::log_id::log_id(&scope_key),
                    reason = "no_sender_chain",
                    "OSL: SKDM_REQUEST IGNORED — sender_key_state exists \
                     but has no sender_chain (we never bootstrapped one)"
                );
                return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
            }
        }
    };

    // Self identity material for the v=4 wrap.
    let (sender_sk, self_pk, self_mlkem_pub, self_discord_id) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: SKDM_REQUEST: identity not loaded".to_string())?;
        let self_did = {
            let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm.iter()
                .find_map(|(did, pe)| (pe.is_self == Some(true)).then(|| did.clone()))
        };
        (
            identity.x25519_secret.clone(),
            identity.x25519_public,
            identity.mlkem_encapsulation_key(),
            self_did,
        )
    };
    let Some(self_discord_id) = self_discord_id else {
        return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
    };

    // Resolve the requester's RecipientV3 via the same vetted path
    // the send loop uses (handles key resolution consistently). The
    // channel-member list for resolution is the gateway snapshot for
    // this scope's channel (DM/GC: scope.id; server: scope.channel_id).
    let channel_members: Vec<String> = {
        let cm = state
            .channel_members
            .lock()
            .expect("channel_members mutex poisoned");
        let cache_key = scope.channel_id.clone().unwrap_or_else(|| scope.id.clone());
        cm.get(&cache_key).cloned().unwrap_or_default()
    };
    let recipients = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        crate::whitelist::recipients_for_scope_v3(
            &pm,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
            &self_mlkem_pub,
        )
        .map_err(|e| format!("OSL: SKDM_REQUEST: resolve recipients: {e}"))?
    };
    // Probe-3 follow-up: after a relaunch the `channel_members`
    // cache is empty until the gateway feed populates it, and
    // Discord doesn't always re-ship CHANNEL_CREATE/UPDATE for
    // every GC on reconnect. That left both sides stuck in a
    // mutual-IGNORED loop: each one's SKDM_REQUEST arrived at the
    // other, but `recipients_for_scope_v3` returned only `[self]`
    // (no members in the cache), so the requester wasn't found
    // and we IGNORED. Fall back to a direct peer_map lookup when
    // the whitelist resolver doesn't surface the requester. Safe:
    // the requester already proved themselves by getting their
    // v=2-wrapped request through our v=2 decrypt (mutual ECDH
    // auth), and providing them with the sender key only lets
    // them decode messages they were already able to receive —
    // no new content is exposed.
    let direct_recipient_owned: Option<crate::wire_v2::RecipientV3> = if recipients
        .iter()
        .any(|(did, _)| did.as_str() == requester_discord_id)
    {
        None
    } else {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(requester_discord_id).and_then(|entry| {
            let x_b64 = entry.pubkey.as_ref()?;
            let mlkem_b64 = entry.ik_mlkem768_pub.as_ref()?;
            let x_bytes = STANDARD.decode(x_b64).ok()?;
            if x_bytes.len() != crypto::x25519::PUBLIC_KEY_SIZE {
                return None;
            }
            let mlkem_bytes = STANDARD.decode(mlkem_b64).ok()?;
            if mlkem_bytes.len() != crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE {
                return None;
            }
            let mut x_arr = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
            x_arr.copy_from_slice(&x_bytes);
            let mut mlkem_arr = [0u8; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE];
            mlkem_arr.copy_from_slice(&mlkem_bytes);
            Some(crate::wire_v2::RecipientV3 {
                x25519_pub: crypto::x25519::PublicKey::from_bytes(x_arr),
                mlkem_pub: crypto::ml_kem_768::EncapsulationKey::from_bytes(&mlkem_arr),
            })
        })
    };
    let peer_recipient: &crate::wire_v2::RecipientV3 = match recipients
        .iter()
        .find(|(did, _)| did.as_str() == requester_discord_id)
    {
        Some((_, r)) => r,
        None => match direct_recipient_owned.as_ref() {
            Some(r) => {
                tracing::info!(
                    requester = %crate::log_id::log_id(requester_discord_id),
                    scope = %crate::log_id::log_id(&scope_key),
                    "OSL: SKDM_REQUEST: requester not in channel_members \
                     (gateway cache cold); falling back to direct peer_map \
                     RecipientV3 lookup so the request doesn't IGNORE-loop"
                );
                r
            }
            None => {
                // Requester not whitelisted-resolvable AND not
                // directly known by peer_map — genuinely nothing
                // we can send them.
                return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
            }
        },
    };

    // Probe-3 Option-2 step 1: emit the recovery SKDM through the
    // same v=3 bundle path as the main send loop (single-recipient
    // slice). Transport no longer depends on v=4 ratchet liveness.
    let wire = send_skdm_via_v3_bundle(
        &sender_sk,
        &self_pk,
        std::slice::from_ref(peer_recipient),
        &scope_key,
        chain_id,
        &rotation_root,
        physical_device_id.as_bytes(),
    )?;
    tracing::info!(
        requester = %crate::log_id::log_id(requester_discord_id),
        scope = %crate::log_id::log_id(&scope_key),
        "OSL: SKDM_REQUEST honored — re-emitting v=3-bundled SKDM"
    );
    Ok(format!("{OSL_RESULT_SKDM_REREQUEST_PREFIX}{wire}"))
}

/// Auto-recovery inbound handler for `MSG_TYPE_SESSION_RESET` (0x07):
/// the sender says our shared v=4 ratchet is desynced and they have
/// dropped their side. Honor it when it passes the
/// staleness/replay/honor-throttle guards.
///
/// Act-on-symptom DOWNGRADE (one-directional-desync fix): a
/// SESSION_RESET only reaches this function after it has been
/// successfully `wire_v2`-decrypted — i.e. it was PQ-hybrid wrapped to
/// our identity using the peer's identity secret. A third party who
/// can merely post into the channel cannot forge one that decrypts, so
/// the original "could be spammed by anyone" threat is already closed
/// by that authentication for SESSION_RESET specifically. Requiring an
/// *additional* local decrypt failure before honoring it broke the
/// common real case: a one-directional ratchet desync (peer→us fails,
/// us→peer still works) leaves the side that must reset with no local
/// symptom, so the reset was ignored forever and the session never
/// healed without two console commands. We now honor an authenticated,
/// non-replayed, non-throttled reset regardless of corroboration; the
/// symptom is still recorded and logged (`corroborated`) for forensics
/// but is no longer a gate. Residual risk: a peer holding valid keys
/// can induce at most one (idempotent, cheap) re-handshake per
/// `RECOVERY_MIN_INTERVAL_SECS` — a throttled self-inflicted nuisance,
/// not a third-party DoS, no secret exposure, no MITM gain. On honor,
/// drop our `ratchet_state` for the peer so the next v=4 send
/// re-handshakes.
fn apply_session_reset_recv(
    state: &AppState,
    sender_discord_id: &str,
    payload_bytes: &[u8],
) -> Result<String, String> {
    let rst = crate::control_messages::deserialize_session_reset(payload_bytes)
        .map_err(|e| format!("OSL: SESSION_RESET: deserialize: {e}"))?;
    let now = now_unix_secs();

    let corroborated;
    {
        let mut g = state
            .recovery_guard
            .lock()
            .expect("recovery_guard mutex poisoned");
        let passes_guards = g.accept_inbound(
            sender_discord_id,
            crate::recovery::RecoveryKind::SessionReset,
            &rst.nonce,
            rst.requested_at,
            now,
        );
        // Staleness / replay / honor-throttle still hard-gate. The
        // act-on-symptom check is now an observability signal only
        // (see fn doc): authenticated + fresh + un-throttled resets
        // are honored even with no corroborating local failure so a
        // one-directional desync self-heals in a single round.
        if !passes_guards {
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        }
        corroborated = g.had_recent_v4_failure(sender_discord_id, now);
    }

    let changed = {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        match pm.get_mut(sender_discord_id) {
            Some(entry) if entry.ratchet_state.is_some() => {
                entry.ratchet_state = None;
                true
            }
            _ => false,
        }
    };
    if changed {
        persist_peer_map_now(state);
        tracing::warn!(
            peer = %crate::log_id::log_id(sender_discord_id),
            corroborated = corroborated,
            "OSL: SESSION_RESET honored — dropped v=4 ratchet; next v=4 \
             re-handshakes (corroborated=false means a one-directional \
             desync healed off the authenticated reset alone)"
        );
    }
    Ok(OSL_RESULT_SESSION_RESET_APPLIED.to_string())
}

#[cfg(test)]
mod unit_b20_independent_review_remediation {
    use super::*;

    const PEER: &str = "900000000000000020";

    fn session_reset_payload(requested_at: i64, nonce: [u8; 16]) -> Vec<u8> {
        crate::control_messages::serialize_session_reset(&crate::control_messages::SessionReset {
            requested_at,
            nonce,
        })
        .expect("serialize SESSION_RESET")
    }

    #[test]
    fn remediate_independent_review_findings() {
        let state = AppState::new();
        state
            .peer_map
            .lock()
            .expect("peer_map mutex poisoned")
            .insert(PEER.to_string(), crate::peer_map::PeerEntry::default());
        let now = now_unix_secs();
        assert!(
            !state
                .recovery_guard
                .lock()
                .expect("recovery_guard mutex poisoned")
                .had_recent_v4_failure(PEER, now),
            "test precondition: no local v4 decrypt symptom was recorded"
        );

        let applied =
            apply_session_reset_recv(&state, PEER, &session_reset_payload(now, [0x20; 16]))
                .expect("fresh authenticated reset should be handled");
        assert_eq!(applied, OSL_RESULT_SESSION_RESET_APPLIED);

        let replay =
            apply_session_reset_recv(&state, PEER, &session_reset_payload(now, [0x20; 16]))
                .expect("replayed reset should be classified, not thrown as plaintext");
        assert_eq!(replay, OSL_RESULT_RECOVERY_IGNORED);

        let stale_requested_at = now - crate::recovery::RECOVERY_FRESHNESS_SECS - 1;
        let stale = apply_session_reset_recv(
            &state,
            PEER,
            &session_reset_payload(stale_requested_at, [0x21; 16]),
        )
        .expect("stale reset should be classified, not thrown as plaintext");
        assert_eq!(stale, OSL_RESULT_RECOVERY_IGNORED);

        assert!(
            !crate::wire_rn::RN_WIRE_IN_ENABLED,
            "b20 remediation must not enable the RN wire-in gate"
        );
    }
}

/// Phase 9-A3: install or rotate a peer's `ReceiverChain` for the
/// scope named in the SKDM payload. Persists `sender_key_state.json`
/// after mutation. Returns the sentinel string so the dispatcher
/// surfaces "control handled, no user content" to the JS layer.
fn apply_skdm_recv(
    state: &AppState,
    sender_discord_id: &str,
    payload_bytes: &[u8],
) -> Result<String, String> {
    use crypto::sender_keys::{PhysicalDeviceId, SenderKeyState, SenderKeyStateOnDisk};
    let payload = crate::control_messages::deserialize_sender_key_distribution(payload_bytes)
        .map_err(|e| format!("OSL: SKDM: deserialize: {e}"))?;

    let scope_key = payload.scope_storage_key.clone();
    // Reject SKDMs for an unknown scope_kind (defensive — boot.js
    // should never produce one). `Scope::parse` returns None for
    // malformed storage keys.
    if crate::scope::Scope::parse(&scope_key).is_none() {
        return Err(format!(
            "OSL: SKDM: payload.scope_storage_key '{scope_key}' is not a valid scope",
            scope_key = crate::log_id::log_id(&scope_key)
        ));
    }

    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        let entry = g.states.entry(scope_key.clone()).or_default();
        let mut live: SenderKeyState = entry
            .clone()
            .try_into()
            .map_err(|e| format!("OSL: SKDM: load existing state: {e}"))?;
        let peer_bytes = sender_discord_id.as_bytes().to_vec();
        let physical_device_id = PhysicalDeviceId::from_bytes(payload.physical_device_id)
            .map_err(|e| format!("OSL: SKDM: physical_device_id binding invalid or absent: {e}"))?;
        if live
            .receiver_chain_for_physical_device(&peer_bytes, physical_device_id)
            .is_some()
        {
            live.rotate_receiver(
                &peer_bytes,
                payload.chain_id,
                &payload.rotation_root,
                physical_device_id,
            )
            .map_err(|e| format!("OSL: SKDM: rotate_receiver: {e}"))?;
        } else {
            live.install_receiver(
                peer_bytes,
                payload.chain_id,
                &payload.rotation_root,
                physical_device_id,
            )
            .map_err(|e| format!("OSL: SKDM: install_receiver: {e}"))?;
        }
        *entry = SenderKeyStateOnDisk::from(&live);
        g.version = 1;
    }
    persist_sender_key_state_now(state);
    tracing::info!(
        sender = %crate::log_id::log_id(sender_discord_id),
        scope = %crate::log_id::log_id(&scope_key),
        chain_id = payload.chain_id,
        "[OSL] SKDM applied: receiver chain installed/rotated"
    );
    Ok(OSL_RESULT_SKDM_APPLIED.to_string())
}

// ===== Phase 6.4: control-message inbox (OOB delivery) =====

/// Phase 6.4: post a single control wire (SKDM, burn marker,
/// SKDM_REQUEST, recovery SKDM) to the keyserver inbox for a
/// recipient. Replaces the prior Discord-channel cover delivery
/// path: control bytes never touch Discord, eliminating ciphertext
/// noise and cipher-store cover load.
///
/// `wire_string` is the full `DPC0::<base64>` wire as produced by
/// the v=3/v=4 send paths. Stripping + decoding happens here so JS
/// stays out of the raw-bytes business.
pub fn cmd_osl_control_inbox_post(
    state: &AppState,
    recipient_id: String,
    scope_input: crate::scope::ScopeInput,
    wire_string: String,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: control_inbox_post scope: {e}"))?;
    let scope_id = scope.storage_key();

    let body = wire_string
        .strip_prefix("DPC0::")
        .ok_or_else(|| "OSL: control_inbox_post: wire missing DPC0:: prefix".to_string())?;
    let bundle = STANDARD
        .decode(body)
        .map_err(|e| format!("OSL: control_inbox_post: base64 decode: {e}"))?;

    // The inbox is keyed by the recipient's verified OSL routing id.
    // The caller supplies a carrier-local Discord id, so the peer map
    // must already contain the friend-code/TOFU binding. Never fall
    // back to the snowflake: it is not a keyserver identity.
    let recipient_osl_id = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(&recipient_id)
            .and_then(|e| e.osl_user_id.clone())
            .ok_or_else(|| "OSL: recipient has no verified OSL identity".to_string())?
    };
    if recipient_osl_id.chars().all(|c| c.is_ascii_digit())
        && (17..=20).contains(&recipient_osl_id.len())
    {
        return Err("OSL: Discord identifiers cannot address the keyserver".to_string());
    }

    // IMPORTANT: clone the identity + keyserver client out from under
    // the AppState mutexes, then DROP the guards BEFORE the network
    // call. Holding state.identity across the HTTP roundtrip blocks
    // every send (encrypt needs state.identity) — that was the
    // "messages take insanely long to send" regression.
    let identity = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?
            .clone()
    };
    let client = {
        let g = state.keyserver.lock().expect("keyserver mutex poisoned");
        g.as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?
            .clone()
    };
    client
        .post_control_inbox(&identity, &recipient_osl_id, &scope_id, &bundle)
        .map_err(|_| "OSL: control inbox delivery refused".to_string())?;
    Ok(())
}

/// Phase 6.4: drain the local user's keyserver inbox.
///
/// For each item, reconstruct the `DPC0::<b64>` wire from raw bundle
/// bytes, dispatch through [`cmd_osl_decrypt_message_v2`], and on
/// success DELETE the row so it isn't re-applied next poll. The
/// dispatched msg_types are control wires (SKDM, BURN, SKDM_REQUEST,
/// SESSION_RESET), all of which return an `OSL_RESULT_*` sentinel —
/// CONTENT/ATTACHMENT should never appear here (sender-side gate).
///
/// `channel_id` is passed empty: control wires don't persist user
/// plaintext, and they don't consult the Mode-1 reassembly buffer.
/// `scope_input` is None: v=3/v=4 SKDM/burn/session-reset paths
/// don't read the outer scope (the bundle carries its own
/// `scope_storage_key`).
///
/// Drain result. `applied` is the count of successfully-applied items;
/// `terminal` is the count of dead-lettered terminal rows
/// (permanently undeliverable + quarantined);
/// `errors` carries the per-item dispatch failures (decrypt/apply) so
/// the JS layer can surface WHY an SKDM didn't install instead of just
/// seeing applied=0.
#[derive(serde::Serialize)]
pub struct ControlInboxDrainReport {
    pub applied: u32,
    pub fetched: u32,
    pub terminal: u32,
    pub errors: Vec<String>,
}

fn control_inbox_dead_letter_path(config_dir: Option<&Path>) -> Result<PathBuf, String> {
    let dir = match config_dir {
        Some(dir) => dir.to_path_buf(),
        None => keystore::osl_config_dir()
            .map_err(|e| format!("OSL: control_inbox_dead_letter dir resolve: {e}"))?,
    };
    Ok(dir.join("control_inbox_dead_letter.json"))
}

fn dead_letter_control_inbox_terminal_before<F>(
    path: &Path,
    ledger: &mut crate::control_inbox_dead_letter::ControlInboxDeadLetterFile,
    row_id: &str,
    reason: &'static str,
    after_recorded: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    ledger.mark_terminal(row_id, reason);
    crate::control_inbox_dead_letter::write_control_inbox_dead_letter(path, ledger)?;
    after_recorded()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlInboxDispatchGate {
    Dispatch,
    SkipTerminal,
    SkipBackoff,
}

fn control_inbox_dispatch_gate(
    ledger: &crate::control_inbox_dead_letter::ControlInboxDeadLetterFile,
    row_id: &str,
    now_unix_secs: i64,
) -> ControlInboxDispatchGate {
    if ledger.is_terminal(row_id) {
        return ControlInboxDispatchGate::SkipTerminal;
    }
    if ledger.should_skip_for_backoff(row_id, now_unix_secs) {
        return ControlInboxDispatchGate::SkipBackoff;
    }
    ControlInboxDispatchGate::Dispatch
}

pub fn cmd_osl_control_inbox_drain(
    state: &AppState,
    config_dir: Option<std::path::PathBuf>,
) -> Result<ControlInboxDrainReport, String> {
    // Snapshot identity + client out from under the AppState mutexes
    // and DROP the guards BEFORE any network call. The drain runs on a
    // 10s timer; holding state.identity across the GET + per-item
    // DELETE network roundtrips blocked every concurrent send (encrypt
    // needs state.identity), which is what made sends "take an
    // insanely long time." With clones, the locks are held only for
    // the microseconds it takes to clone.
    let identity = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?
            .clone()
    };
    let client = {
        let g = state.keyserver.lock().expect("keyserver mutex poisoned");
        g.as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?
            .clone()
    };
    let items = client
        .get_control_inbox(&identity)
        .map_err(|e| format!("OSL: control_inbox_drain GET: {e}"))?;
    let item_count = items.len();
    let dead_letter_path = control_inbox_dead_letter_path(config_dir.as_deref())?;
    let mut dead_letter =
        crate::control_inbox_dead_letter::load_control_inbox_dead_letter(&dead_letter_path);
    let now = now_unix_secs();

    let mut applied: u32 = 0;
    let mut errors: Vec<String> = Vec::new();
    for item in items {
        if is_discord_snowflake_shaped(&item.sender_id) {
            let retire = dead_letter_control_inbox_terminal_before(
                &dead_letter_path,
                &mut dead_letter,
                &item.id,
                crate::control_inbox_dead_letter::REASON_DISCORD_SNOWFLAKE_SENDER,
                || {
                    client
                        .delete_control_inbox(&identity, &item.id)
                        .map_err(|e| format!("OSL: control_inbox DELETE: {e}"))
                },
            );
            match retire {
                Ok(()) => tracing::warn!(
                    inbox_id = %item.id,
                    sender = %crate::log_id::log_id(&item.sender_id),
                    "[OSL] control_inbox item permanently undeliverable; \
                     dead-lettered and retired"
                ),
                Err(e) => {
                    tracing::warn!(
                        inbox_id = %item.id,
                        sender = %crate::log_id::log_id(&item.sender_id),
                        error = %e,
                        "[OSL] control_inbox item permanently undeliverable; \
                         dead-letter recorded before retirement failed"
                    );
                    errors.push(format!("control_inbox row retire failed: {e}"));
                }
            }
            continue;
        }
        match control_inbox_dispatch_gate(&dead_letter, &item.id, now) {
            ControlInboxDispatchGate::Dispatch => {}
            ControlInboxDispatchGate::SkipTerminal => {
                tracing::debug!(
                    inbox_id = %item.id,
                    "[OSL] control_inbox item skipped: terminal dead-letter entry"
                );
                continue;
            }
            ControlInboxDispatchGate::SkipBackoff => {
                tracing::debug!(
                    inbox_id = %item.id,
                    "[OSL] control_inbox item skipped: retry backoff active"
                );
                continue;
            }
        }
        let bundle = match STANDARD.decode(&item.bundle_b64) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    inbox_id = %item.id,
                    error = %e,
                    "[OSL] control_inbox_drain: skip undecodable bundle"
                );
                errors.push(format!("from={} b64 decode failed: {e}", item.sender_id));
                continue;
            }
        };
        // Native-overlay relay notices are user content transported through
        // this authenticated inbox, not core control messages. Leave them in
        // place without attempting dispatch, deletion, or error reporting;
        // the separately-capable trusted overlay drain owns them.
        if crate::wire_v2::is_native_overlay_relay_bundle(&bundle) {
            continue;
        }
        let content = format!("DPC0::{}", STANDARD.encode(&bundle));
        // The inbox stores sender_id as the sender's OSL user_id, but
        // the decrypt/apply path (resolve_sender_pubkey, peer_map
        // ratchet/SKDM keying) works in DISCORD IDs. Reverse-map via
        // peer_map so a sender whose osl_user_id != Discord snowflake
        // still resolves to the right peer entry. Falls back to the
        // raw value when they're identical or there's no mapping.
        let sender_discord_id = {
            let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm.iter()
                .find(|(_, e)| e.osl_user_id.as_deref() == Some(item.sender_id.as_str()))
                .map(|(did, _)| did.clone())
                .unwrap_or_else(|| item.sender_id.clone())
        };
        let res = cmd_osl_decrypt_message_v2(
            state,
            None,
            String::new(),
            sender_discord_id.clone(),
            content,
            None,
            config_dir.clone(),
        );
        match res {
            Ok(sentinel) => {
                tracing::info!(
                    inbox_id = %item.id,
                    sender = %crate::log_id::log_id(&item.sender_id),
                    scope = %crate::log_id::log_id(&item.scope_id),
                    sentinel = %sentinel,
                    "[OSL] control_inbox item applied"
                );
                // Uses the cloned identity + client (no AppState locks
                // held across this network DELETE).
                if let Err(e) = client.delete_control_inbox(&identity, &item.id) {
                    // Best-effort: failing to delete just means a
                    // duplicate apply next poll, which the SKDM /
                    // burn handlers tolerate (rotate is idempotent
                    // on same chain_id; burn is idempotent on
                    // scope+timestamp).
                    tracing::warn!(
                        inbox_id = %item.id,
                        error = %e,
                        "[OSL] control_inbox DELETE failed (will retry)"
                    );
                }
                if sentinel == OSL_RESULT_SESSION_RESET_APPLIED {
                    tracing::info!(
                        peer = %crate::log_id::log_id(&item.sender_id),
                        "[OSL] SESSION_RESET applied; outbound v=4 bootstrap ping retired"
                    );
                } else if let Some(resp_wire) =
                    sentinel.strip_prefix(OSL_RESULT_SKDM_REREQUEST_PREFIX)
                {
                    // An SKDM_REQUEST honored via the inbox: post the
                    // rebuilt sender-key bundle BACK to the requester's
                    // inbox. The channel-recv path does this post in
                    // boot.js (it has channelId in scope), but the inbox
                    // drain must do it here — otherwise the response is
                    // built and DROPPED, the requester stays "awaiting
                    // SKDM" forever, and GC messages never decrypt. Post
                    // to the requester's OSL user_id (item.sender_id),
                    // same scope.
                    if let Some(b64) = resp_wire.strip_prefix("DPC0::") {
                        match STANDARD.decode(b64) {
                            Ok(resp_bundle) => {
                                match client.post_control_inbox(
                                    &identity,
                                    &item.sender_id,
                                    &item.scope_id,
                                    &resp_bundle,
                                ) {
                                    Ok(_) => tracing::info!(
                                        requester = %crate::log_id::log_id(&item.sender_id),
                                        scope = %crate::log_id::log_id(&item.scope_id),
                                        "[OSL] SKDM_REQUEST honored via inbox — \
                                         sender key posted back to requester"
                                    ),
                                    Err(e) => tracing::warn!(
                                        requester = %crate::log_id::log_id(&item.sender_id),
                                        error = %e,
                                        "[OSL] SKDM response post-back failed"
                                    ),
                                }
                            }
                            Err(e) => tracing::warn!(
                                error = %e,
                                "[OSL] SKDM response wire base64 decode failed"
                            ),
                        }
                    }
                }
                applied = applied.saturating_add(1);
            }
            Err(e) => {
                let outcome = dead_letter.record_dispatch_failure(&item.id, now);
                if let Err(write_err) =
                    crate::control_inbox_dead_letter::write_control_inbox_dead_letter(
                        &dead_letter_path,
                        &dead_letter,
                    )
                {
                    tracing::warn!(
                        inbox_id = %item.id,
                        error = %write_err,
                        "[OSL] control_inbox dead-letter ledger write failed"
                    );
                    errors.push(format!(
                        "control_inbox dead-letter write failed: {write_err}"
                    ));
                }
                match outcome {
                    crate::control_inbox_dead_letter::DispatchFailureOutcome::RetryScheduled {
                        attempts,
                        next_attempt_at,
                    } => tracing::warn!(
                        inbox_id = %item.id,
                        sender = %crate::log_id::log_id(&item.sender_id),
                        scope = %crate::log_id::log_id(&item.scope_id),
                        attempts = attempts,
                        next_attempt_at = next_attempt_at,
                        error = %e,
                        "[OSL] control_inbox item dispatch failed; retry scheduled"
                    ),
                    crate::control_inbox_dead_letter::DispatchFailureOutcome::Terminal {
                        attempts,
                    } => tracing::warn!(
                        inbox_id = %item.id,
                        sender = %crate::log_id::log_id(&item.sender_id),
                        scope = %crate::log_id::log_id(&item.scope_id),
                        attempts = attempts,
                        error = %e,
                        "[OSL] control_inbox item dispatch failed; dead-lettered terminal"
                    ),
                }
                errors.push(format!(
                    "from={} scope={}: {e}",
                    item.sender_id, item.scope_id
                ));
            }
        }
    }
    let terminal = dead_letter.terminal_count();
    tracing::info!(
        fetched = item_count,
        applied = applied,
        terminal = terminal,
        "[OSL] control_inbox drain done"
    );
    Ok(ControlInboxDrainReport {
        applied,
        fetched: item_count as u32,
        terminal,
        errors,
    })
}

#[cfg(test)]
mod control_inbox_dead_letter_policy_tests {
    use super::{
        control_inbox_dispatch_gate, dead_letter_control_inbox_terminal_before,
        is_discord_snowflake_shaped, ControlInboxDispatchGate,
    };
    use crate::control_inbox_dead_letter::{
        load_control_inbox_dead_letter, ControlInboxDeadLetterFile, MAX_CONTROL_INBOX_ATTEMPTS,
        REASON_DISCORD_SNOWFLAKE_SENDER, REASON_MAX_ATTEMPTS,
    };
    use std::cell::Cell;

    #[test]
    fn snowflake_sender_dead_letters_before_retirement_and_is_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control_inbox_dead_letter.json");
        let mut ledger = ControlInboxDeadLetterFile::default();
        let row_id = "row-a";
        let sender_id = "123456789012345678";
        let retired = Cell::new(false);

        assert!(is_discord_snowflake_shaped(sender_id));
        dead_letter_control_inbox_terminal_before(
            &path,
            &mut ledger,
            row_id,
            REASON_DISCORD_SNOWFLAKE_SENDER,
            || {
                let on_disk = load_control_inbox_dead_letter(&path);
                let entry = on_disk.entries.get(row_id).expect("dead-letter entry");
                assert_eq!(
                    entry.terminal_reason.as_deref(),
                    Some(REASON_DISCORD_SNOWFLAKE_SENDER)
                );
                retired.set(true);
                Ok(())
            },
        )
        .unwrap();

        assert!(retired.get());
        assert_eq!(
            control_inbox_dispatch_gate(&ledger, row_id, 2_000),
            ControlInboxDispatchGate::SkipTerminal
        );
    }

    #[test]
    fn quarantined_control_inbox_row_is_retained_and_skipped_by_next_drain() {
        let mut ledger = ControlInboxDeadLetterFile::default();
        let row_id = "row-a";
        let sender_id = "osl-user-id";
        let mut delete_called = false;

        assert!(!is_discord_snowflake_shaped(sender_id));
        for _ in 0..MAX_CONTROL_INBOX_ATTEMPTS {
            ledger.record_dispatch_failure(row_id, 1_000);
        }
        let entry = ledger.entries.get(row_id).expect("dead-letter entry");
        assert_eq!(entry.terminal_reason.as_deref(), Some(REASON_MAX_ATTEMPTS));

        match control_inbox_dispatch_gate(&ledger, row_id, 2_000) {
            ControlInboxDispatchGate::Dispatch => {
                delete_called = true;
            }
            ControlInboxDispatchGate::SkipTerminal => {}
            ControlInboxDispatchGate::SkipBackoff => panic!("terminal row must not be in backoff"),
        }

        assert!(!delete_called, "quarantined rows must be retained");
    }
}

/// Phase 9-A3: v=5 receive dispatch. Parses the wire, applies the
/// kill-list gate (same as v=4), looks up the matching
/// `SenderKeyState` + `ReceiverChain`, runs the sender-keys
/// `decrypt`, persists, returns plaintext.
fn decrypt_v5_recv(
    state: &AppState,
    sender_discord_id: String,
    content: String,
    scope_opt: Option<crate::scope::Scope>,
) -> Result<String, String> {
    use crypto::sender_keys::{SenderContext, SenderKeyState, SenderKeyStateOnDisk};

    let parsed =
        crate::wire_v2::decrypt_v5(&content).map_err(|e| format!("OSL: v=5 decode: {e}"))?;

    let scope = scope_opt
        .ok_or_else(|| "OSL: v=5 decode: scope required for sender-keys lookup".to_string())?;
    let scope_key = scope.storage_key();

    // Sender ML-KEM pub for AD binding. Two cases:
    //   - Sender is the local user (self-decrypt loopback): pull
    //     bytes from `identity.mlkem_public_bytes` so AD matches the
    //     sender-side encrypt path exactly.
    //   - Sender is a peer: pull bytes from peer_map. Empty Vec if
    //     not yet known — AD will then differ and AEAD fails with
    //     a clear error.
    let sender_mlkem_pub_bytes: Vec<u8> = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        let is_self = parsed.sender_ik_pub.as_bytes() == identity.x25519_public.as_bytes();
        if is_self {
            identity.mlkem_public_bytes.to_vec()
        } else {
            drop(id_guard);
            let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm_guard
                .get(&sender_discord_id)
                .and_then(|pe| pe.ik_mlkem768_pub.as_deref())
                .and_then(|b64| STANDARD.decode(b64).ok())
                .unwrap_or_default()
        }
    };

    let ctx = SenderContext {
        sender_ik_x25519_pub: parsed.sender_ik_pub,
        sender_ik_mlkem_pub: sender_mlkem_pub_bytes,
        group_id: scope_key.clone().into_bytes(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };

    // Load the per-scope SenderKeyState. If absent → no SKDM has
    // arrived yet → return a clear retry-worthy error.
    let mut sks: SenderKeyState = {
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        match g.states.get(&scope_key) {
            Some(disk) => disk.clone().try_into().map_err(|e| {
                format!(
                    "OSL: v=5 decode: load sender_key_state for scope {scope_key}: {e}",
                    scope_key = crate::log_id::log_id(&scope_key)
                )
            })?,
            None => {
                return Err(format!(
                    "OSL: v=5 decode: no installed sender-key state for peer \
                     {sender_discord_id} in scope {scope_key} — awaiting SKDM",
                    sender_discord_id = crate::log_id::log_id(&sender_discord_id),
                    scope_key = crate::log_id::log_id(&scope_key)
                ));
            }
        }
    };

    let peer_bytes = sender_discord_id.as_bytes().to_vec();
    if sks.receiver_chain(&peer_bytes).is_none() {
        return Err(format!(
            "OSL: v=5 decode: no installed sender-key state for peer \
             {sender_discord_id} in scope {scope_key} — awaiting SKDM",
            sender_discord_id = crate::log_id::log_id(&sender_discord_id),
            scope_key = crate::log_id::log_id(&scope_key)
        ));
    }

    let em = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    let plaintext_bytes = sks
        .decrypt_from(&peer_bytes, &em, &ctx)
        .map_err(|e| format!("OSL: v=5 decode: decrypt_from: {e}"))?;

    // Persist updated state.
    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        g.states
            .insert(scope_key.clone(), SenderKeyStateOnDisk::from(&sks));
        g.version = 1;
    }
    persist_sender_key_state_now(state);

    // 9-C1: permissive decrypt — no per-scope accept gate. The
    // self-sender pubkey-comparison bypass that previously guarded
    // the gate is also gone (the gate is gone, so the bypass is
    // moot).
    let _ = (scope, sender_discord_id);

    String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: v=5 decrypted plaintext is not valid UTF-8".to_string())
}

/// Resolve sender pubkey. Prefers `peer_map[sender].pubkey` (v=2
/// invitations carry pubkeys, so by content-message time we
/// expect this populated); falls back to the legacy
/// keyserver round-trip via `peer_map[sender].osl_user_id`.
fn resolve_sender_pubkey(
    state: &AppState,
    sender_discord_id: &str,
) -> Result<crypto::x25519::PublicKey, String> {
    let osl_user_id_for_lookup = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        if let Ok(pk) = lookup_peer_pubkey(&pm_guard, sender_discord_id) {
            return Ok(pk);
        }
        // Fall back to keyserver path: need the osl_user_id.
        pm_guard
            .get(sender_discord_id)
            .and_then(|e| e.osl_user_id.clone())
    };
    // RECEIVE-PATH GUARANTEE (deliberate): an unmapped sender
    // returns UnknownSender and does NOT consult the keyserver
    // (privacy: no metadata leak of "received an OSL message from
    // snowflake X"; attacker-pokable via junk DPC0:: otherwise).
    // The cross-machine fix is send-side + v3/v4, so receive never
    // needs a keyserver sender lookup — do NOT default to snowflake.
    let osl_user_id = osl_user_id_for_lookup.ok_or_else(|| {
        format!(
            "OSL: {}",
            DecodeError::UnknownSender {
                discord_id: sender_discord_id.to_string(),
            }
        )
    })?;
    if let Some(cached) = state.sender_pubkey_cache.get(&osl_user_id) {
        return Ok(cached);
    }
    if is_discord_snowflake_shaped(&osl_user_id) {
        return Err("OSL: Discord identifiers cannot resolve keys".to_string());
    }
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard
        .as_ref()
        .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
    let resp = client.fetch_pubkeys(&osl_user_id).map_err(|e| {
        format!(
            "OSL: fetch_pubkeys({osl_user_id}): {e}",
            osl_user_id = crate::log_id::log_id(&osl_user_id)
        )
    })?;
    let pub_vec = STANDARD.decode(&resp.ik_x25519_pub).map_err(|e| {
        format!(
            "OSL: decode sender pubkey ({osl_user_id}): {e}",
            osl_user_id = crate::log_id::log_id(&osl_user_id)
        )
    })?;
    if pub_vec.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: sender pubkey wrong length ({osl_user_id}): got {}",
            pub_vec.len(),
            osl_user_id = crate::log_id::log_id(&osl_user_id)
        ));
    }
    let mut bytes = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    bytes.copy_from_slice(&pub_vec);
    let pub_key = crypto::x25519::PublicKey::from_bytes(bytes);
    drop(ks_guard);
    state.sender_pubkey_cache.insert(osl_user_id, pub_key);
    Ok(pub_key)
}

/// Resolve only the sender key already attached to the claimed peer.
///
/// v3 authenticates the in-band sender key, so falling back to a fresh
/// keyserver response would merely compare one attacker-controlled key
/// with another. Attribution requires a local peer-map pin.
fn resolve_pinned_sender_pubkey(
    state: &AppState,
    sender_discord_id: &str,
) -> Result<crypto::x25519::PublicKey, &'static str> {
    let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
    lookup_peer_pubkey(&pm, sender_discord_id).map_err(|_| "OSL: v3 sender identity is not pinned")
}

#[cfg(test)]
mod v3_pinned_sender_command_tests {
    use super::*;

    fn pin_peer_x25519(state: &AppState, discord_id: &str, pubkey: crypto::x25519::PublicKey) {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let peer = pm.entry(discord_id.to_string()).or_default();
        peer.pubkey = Some(STANDARD.encode(pubkey.as_bytes()));
        peer.discord_id = Some(discord_id.to_string());
    }

    #[test]
    fn v3_pinned_sender_rejects_forged_sender() {
        const ALICE_DID: &str = "900000000000000101";
        const MALLORY_DID: &str = "900000000000000102";
        const CHANNEL_ID: &str = "900000000000000199";

        let state = AppState::new();
        let bob = generate_identity("bob-a85".to_string());
        let bob_recipient = crate::wire_v2::RecipientV3 {
            x25519_pub: bob.x25519_public,
            mlkem_pub: bob.mlkem_encapsulation_key(),
        };
        state.install_identity(bob);

        let alice = generate_identity("alice-a85".to_string());
        let mallory = generate_identity("mallory-a85".to_string());
        pin_peer_x25519(&state, ALICE_DID, alice.x25519_public);

        let wire = crate::wire_v2::encrypt_v3(
            &mallory.x25519_secret,
            &mallory.x25519_public,
            &[bob_recipient],
            crate::wire_v2::MSG_TYPE_CONTENT,
            b"forged sender body",
        )
        .expect("forge a syntactically valid v3 wire for Bob");

        let err = cmd_osl_decrypt_message_v2(
            &state,
            None,
            CHANNEL_ID.to_string(),
            ALICE_DID.to_string(),
            wire.clone(),
            None,
            None,
        )
        .expect_err("claimed Alice sender must not authenticate Mallory's in-band v3 sender key");
        assert!(
            err.contains("v3 authenticated sender refused"),
            "expected authenticated sender refusal, got {err}"
        );
        assert!(
            !err.contains("not pinned"),
            "Alice was pinned; the refusal must come from sender-key mismatch, got {err}"
        );

        pin_peer_x25519(&state, MALLORY_DID, mallory.x25519_public);
        let opened = cmd_osl_decrypt_message_v2(
            &state,
            None,
            CHANNEL_ID.to_string(),
            MALLORY_DID.to_string(),
            wire,
            None,
            None,
        )
        .expect("same wire decrypts when the claimed sender matches the local pin");
        assert_eq!(opened, "forged sender body");
    }
}

/// Populate a peer from a signed keyserver bundle. The response proof
/// and complete key shapes are checked before any live key is changed.
/// Separated from HTTP so tests can exercise the mutation boundary.
fn fetched_key_bundle(resp: &keystore::client::PubkeysResponse) -> crate::tofu::KeyBundle {
    crate::tofu::KeyBundle {
        ed25519_pub: resp.ik_ed25519_pub.clone(),
        x25519_pub: resp.ik_x25519_pub.clone(),
        mlkem768_pub: resp.ik_mlkem768_pub.clone(),
        ratchet_initial_pub: resp.ik_ratchet_initial_pub.clone(),
    }
}

fn trusted_key_bundle(entry: &crate::peer_map::PeerEntry) -> Option<crate::tofu::KeyBundle> {
    entry.tofu_key_bundle.clone().or_else(|| {
        Some(crate::tofu::KeyBundle {
            ed25519_pub: entry.tofu_ed25519_pub.clone()?,
            x25519_pub: entry.pubkey.clone()?,
            mlkem768_pub: entry.ik_mlkem768_pub.clone()?,
            ratchet_initial_pub: entry.ik_ratchet_initial_pub.clone(),
        })
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn verified_identity_bundle_from_fetch_response(
    resp: &keystore::client::PubkeysResponse,
    pinned_signer: &crypto::ed25519::PublicKey,
    revision: u64,
    identity_bundle_signature_b64: &str,
    last_known_revision: Option<u64>,
) -> IpcResult<keystore::identity_bundle::IdentityBundle> {
    keystore::client::validate_peer_bundle(resp).map_err(|_| {
        IpcError::InvalidArgument("keyserver identity record proof invalid".to_string())
    })?;

    let mut bundle = keystore::identity_bundle::IdentityBundle {
        ed25519_identity_pub: b64_to_array(
            "identity bundle Ed25519 key",
            &resp.ik_ed25519_pub,
        )?,
        x25519_identity_pub: b64_to_array("identity bundle X25519 key", &resp.ik_x25519_pub)?,
        mlkem768_identity_pub: b64_to_array(
            "identity bundle ML-KEM key",
            &resp.ik_mlkem768_pub,
        )?,
        capability_bundle: resp.rn_capabilities.unwrap_or(0),
        revision,
        signature: [0u8; crypto::ed25519::SIGNATURE_SIZE],
    };
    bundle.signature = b64_to_array(
        "identity bundle signature",
        identity_bundle_signature_b64,
    )?;

    keystore::identity_bundle::BundleVerifyPolicy::new()
        .verify(&bundle, pinned_signer, last_known_revision)
        .map_err(|_| {
            IpcError::InvalidArgument("identity bundle verification failed".to_string())
        })?;
    Ok(bundle)
}

#[cfg(test)]
mod production_identity_bundle_pipeline_tests {
    use super::*;
    use keystore::client::{PrekeyBundleOpk, PrekeyBundleResponse, PubkeysResponse};
    use keystore::identity_bundle::{BundleField, BundleMergeError, IdentityBundle};
    use keystore::{Identity, PrekeyConfig, PrekeyState};

    fn fetched_pubkeys(identity: &Identity, capabilities: u32) -> PubkeysResponse {
        let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
        let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
        let mlkem768 = STANDARD.encode(identity.mlkem_public_bytes);
        let ratchet = identity
            .ratchet_initial_pub
            .as_ref()
            .map(|p| STANDARD.encode(p.as_bytes()));
        let reg_msg = keystore::client::reg_msg_with_capabilities(
            &identity.user_id,
            &x25519,
            &ed25519,
            &mlkem768,
            ratchet.as_deref(),
            capabilities,
        );
        let reg_sig = crypto::ed25519::sign(&identity.ed25519_secret, &reg_msg);

        PubkeysResponse {
            user_id: identity.user_id.clone(),
            ik_x25519_pub: x25519,
            ik_ed25519_pub: ed25519,
            ik_mlkem768_pub: mlkem768,
            registered_at: "2026-07-30T00:00:00Z".to_string(),
            last_rotated_at: None,
            ik_ratchet_initial_pub: ratchet,
            rn_capabilities: Some(capabilities),
            registration_sig: Some(STANDARD.encode(reg_sig.as_bytes())),
        }
    }

    fn full_identity_bundle_signature(
        identity: &Identity,
        capabilities: u32,
        revision: u64,
    ) -> String {
        let bundle = IdentityBundle {
            ed25519_identity_pub: *identity.ed25519_public.as_bytes(),
            x25519_identity_pub: *identity.x25519_public.as_bytes(),
            mlkem768_identity_pub: identity.mlkem_public_bytes,
            capability_bundle: capabilities,
            revision,
            signature: [0u8; crypto::ed25519::SIGNATURE_SIZE],
        };
        let sig = crypto::ed25519::sign(&identity.ed25519_secret, &bundle.signed_bytes());
        STANDARD.encode(sig.as_bytes())
    }

    fn prekey_response(
        identity: &Identity,
        fetched: &PubkeysResponse,
        prekeys: &PrekeyState,
        remaining_opk_count: u32,
    ) -> PrekeyBundleResponse {
        let opk = prekeys.opk_pool.first().expect("prekey state has OPKs");
        PrekeyBundleResponse {
            user_id: identity.user_id.clone(),
            ik_x25519_pub: fetched.ik_x25519_pub.clone(),
            ik_ed25519_pub: fetched.ik_ed25519_pub.clone(),
            ik_mlkem768_pub: fetched.ik_mlkem768_pub.clone(),
            spk_pub: STANDARD.encode(prekeys.current_spk.public),
            spk_signature: STANDARD.encode(prekeys.current_spk.signature),
            spk_rotated_at: keystore::iso_8601_from_unix_seconds(
                prekeys.current_spk.rotated_at_unix_seconds,
            ),
            opk: Some(PrekeyBundleOpk {
                id: opk.id,
                pub_b64: STANDARD.encode(opk.public),
            }),
            remaining_opk_count,
            ik_ratchet_initial_pub: fetched.ik_ratchet_initial_pub.clone(),
        }
    }

    #[test]
    fn production_identity_bundle_pipeline() {
        let state = AppState::new();
        let peer_discord_id = "900000000000000001";
        let capabilities = keystore::client::RN_CAP_WIRE_RN;
        let revision = 1;
        let peer = keystore::generate_identity("pipeline-peer".to_string());
        let fetched = fetched_pubkeys(&peer, capabilities);
        let signature = full_identity_bundle_signature(&peer, capabilities, revision);

        let verified = verified_identity_bundle_from_fetch_response(
            &fetched,
            &peer.ed25519_public,
            revision,
            &signature,
            None,
        )
        .expect("fetched production identity bundle verifies");
        assert_eq!(verified.revision, revision);
        assert_eq!(verified.capability_bundle, capabilities);

        let mut tampered_fetch = fetched_pubkeys(&peer, capabilities);
        tampered_fetch.ik_x25519_pub =
            STANDARD.encode([0x55u8; crypto::x25519::PUBLIC_KEY_SIZE]);
        let err = verified_identity_bundle_from_fetch_response(
            &tampered_fetch,
            &peer.ed25519_public,
            revision,
            &signature,
            None,
        )
        .expect_err("registration proof tampering must be refused");
        assert!(
            err.to_string()
                .contains("keyserver identity record proof invalid"),
            "unexpected error: {err}"
        );

        let prekeys = PrekeyState::new(&peer, PrekeyConfig::default(), 1_700_000_000);
        let response = prekey_response(&peer, &fetched, &prekeys, 99);
        let merged = verified
            .merge_prekey_bundle_response(&response, None)
            .expect("prekey response identity fields match verified bundle");
        assert_eq!(merged.identity, verified);
        assert_eq!(merged.prekey.remaining_opk_count, 99);
        assert_eq!(
            merged.prekey.opk,
            Some((prekeys.opk_pool[0].id, prekeys.opk_pool[0].public))
        );

        let attacker = keystore::generate_identity("pipeline-attacker".to_string());
        let attacker_fetch = fetched_pubkeys(&attacker, capabilities);
        let mut substituted = prekey_response(&peer, &fetched, &prekeys, 99);
        substituted.ik_ed25519_pub = attacker_fetch.ik_ed25519_pub;
        assert_eq!(
            verified.merge_prekey_bundle_response(&substituted, None),
            Err(BundleMergeError::IdentityKeyMismatch {
                field: BundleField::Ed25519IdentityKey
            })
        );

        populate_peer_from_fetch_response(&state, peer_discord_id, &fetched)
            .expect("verified fetch populates peer_map");
        let pm = state.peer_map.lock().unwrap();
        let entry = pm.get(peer_discord_id).expect("peer cached");
        assert_eq!(entry.osl_user_id.as_deref(), Some(peer.user_id.as_str()));
        assert_eq!(entry.pubkey.as_deref(), Some(fetched.ik_x25519_pub.as_str()));
        assert_eq!(
            entry.tofu_key_bundle.as_ref(),
            Some(&crate::tofu::KeyBundle {
                ed25519_pub: fetched.ik_ed25519_pub.clone(),
                x25519_pub: fetched.ik_x25519_pub.clone(),
                mlkem768_pub: fetched.ik_mlkem768_pub.clone(),
                ratchet_initial_pub: fetched.ik_ratchet_initial_pub.clone(),
            })
        );
        assert!(
            state.key_change_alerts.lock().unwrap().is_empty(),
            "first verified fetch must seed TOFU without a false alert"
        );
    }
}

pub fn populate_peer_from_fetch_response(
    state: &AppState,
    discord_id: &str,
    resp: &keystore::client::PubkeysResponse,
) -> Result<bool, String> {
    if !keystore::client::verify_peer_bundle(resp) {
        return Err("OSL: keyserver bundle proof invalid".to_string());
    }
    let expected_user_id = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(discord_id)
            .and_then(|entry| entry.osl_user_id.clone())
    };
    if expected_user_id
        .as_deref()
        .is_some_and(|expected| expected != resp.user_id)
    {
        return Err("OSL: keyserver bundle identity mismatch".to_string());
    }
    if resp.ik_x25519_pub.is_empty()
        || resp.ik_ed25519_pub.is_empty()
        || resp.ik_mlkem768_pub.is_empty()
    {
        return Err("OSL: keyserver bundle incomplete".to_string());
    }
    // Validate decode shape early so we error before mutating peer_map.
    let x_vec = STANDARD.decode(&resp.ik_x25519_pub).map_err(|e| {
        format!(
            "OSL: decode X25519 pubkey for {discord_id}: {e}",
            discord_id = crate::log_id::log_id(discord_id)
        )
    })?;
    if x_vec.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: X25519 pubkey for {discord_id} wrong length: got {}",
            x_vec.len(),
            discord_id = crate::log_id::log_id(discord_id)
        ));
    }
    // Probe-2 Rust Bug 6 (security): peek the TOFU outcome before
    // writing any live messaging keys. The OLD code unconditionally
    // overwrote `entry.pubkey` (X25519) and `entry.ik_mlkem768_pub`
    // BEFORE calling `tofu_observe_peer`. On an un-accepted key change
    // (TOFU `Changed`), the Ed25519 baseline correctly stayed at the
    // old key — but the live encryption keys for v=2 / v=3 / v=4
    // were already pointing at the NEW (potentially attacker)
    // X25519+ML-KEM, so the user thought they declined the change
    // yet outbound sends silently encrypted to the new key. Now we
    // gate the live-key write on the TOFU outcome: on `Changed`,
    // skip the X25519/ML-KEM/ratchet update and let the existing
    // alert path raise the blocking accept banner. Once the user
    // accepts via `cmd_osl_accept_key_change`, the next fetch
    // reclassifies as `Unchanged` and the live keys flow through.
    let fetched_bundle = fetched_key_bundle(resp);
    let tofu_outcome_peek = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let baseline = pm.get(discord_id).and_then(trusted_key_bundle);
        crate::tofu::classify(baseline.as_ref(), &fetched_bundle)
    };
    let live_writable = !matches!(tofu_outcome_peek, crate::tofu::TofuOutcome::Changed { .. });
    let mut mlkem_added = false;
    if !resp.ik_mlkem768_pub.is_empty() {
        let mlkem_vec = STANDARD.decode(&resp.ik_mlkem768_pub).map_err(|e| {
            format!(
                "OSL: decode ML-KEM pubkey for {discord_id}: {e}",
                discord_id = crate::log_id::log_id(discord_id)
            )
        })?;
        if mlkem_vec.len() != crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE {
            return Err(format!(
                "OSL: ML-KEM pubkey for {discord_id} wrong length: got {} (expected {})",
                mlkem_vec.len(),
                crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE,
                discord_id = crate::log_id::log_id(discord_id)
            ));
        }
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm_guard.entry(discord_id.to_string()).or_default();
        let had_mlkem = entry.ik_mlkem768_pub.is_some();
        if live_writable {
            entry.pubkey = Some(resp.ik_x25519_pub.clone());
            entry.ik_mlkem768_pub = Some(resp.ik_mlkem768_pub.clone());
            entry.ik_ratchet_initial_pub = resp.ik_ratchet_initial_pub.clone();
            mlkem_added = !had_mlkem;
        }
        entry
            .discord_id
            .get_or_insert_with(|| discord_id.to_string());
        // Preserve the service-neutral routing identifier returned by
        // the signed record. Discord snowflakes never enter this field.
        entry
            .osl_user_id
            .get_or_insert_with(|| resp.user_id.clone());
    } else {
        // X25519 only; same gating.
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm_guard.entry(discord_id.to_string()).or_default();
        if live_writable {
            entry.pubkey = Some(resp.ik_x25519_pub.clone());
            entry.ik_ratchet_initial_pub = resp.ik_ratchet_initial_pub.clone();
        }
        entry
            .discord_id
            .get_or_insert_with(|| discord_id.to_string());
        entry
            .osl_user_id
            .get_or_insert_with(|| resp.user_id.clone());
    }

    if !live_writable {
        tracing::warn!(
            "OSL: peer bundle changed but was not accepted; refusing \
             to replace live encryption keys"
        );
    }

    // Compare the complete bundle with the trusted baseline. A change
    // raises the blocking alert without replacing live keys.
    tofu_observe_peer(state, discord_id, &fetched_bundle)?;

    Ok(mlkem_added)
}

/// Apply [`crate::tofu::classify`] to a freshly fetched complete
/// bundle. Pure decision logic lives in `crate::tofu`; this is the
/// AppState, alert and persistence wiring.
///
/// On a TOFU `Changed`, the peer's `ratchet_state` must be dropped
/// EXACTLY ONCE — on first detection of a given new key. This fn
/// decides "is this a newly-observed change?":
///
/// - no pending alert → yes (first detection: drop once)
/// - pending alert, SAME new key → no (already handled: keep ratchet so a
///   re-bootstrapped session can survive and deliver while the user verifies)
/// - pending alert, DIFFERENT key → yes (the key changed AGAIN: the old
///   session is invalid; drop)
///
/// Pure so it is unit-tested without an `AppState`.
fn tofu_change_is_newly_observed(
    pending: Option<&crate::tofu::KeyBundle>,
    fetched: &crate::tofu::KeyBundle,
) -> bool {
    match pending {
        Some(bundle) => bundle != fetched,
        None => true,
    }
}

#[cfg(test)]
mod tofu_change_idempotency_tests {
    use super::{is_discord_snowflake_shaped, tofu_change_is_newly_observed as f};
    use crate::tofu::KeyBundle;

    fn bundle(x: &str) -> KeyBundle {
        KeyBundle {
            ed25519_pub: "ed".to_owned(),
            x25519_pub: x.to_owned(),
            mlkem768_pub: "mlkem".to_owned(),
            ratchet_initial_pub: None,
        }
    }

    #[test]
    fn first_detection_no_pending_alert_is_newly_observed() {
        // No alert yet → first detection → drop ratchet once.
        assert!(f(None, &bundle("new")));
    }

    #[test]
    fn same_pending_change_is_not_newly_observed() {
        // The exact scenario that bricked DMs: every v=4 send
        // re-fetches and re-observes the SAME changed key. Must NOT
        // be treated as new (so the ratchet is not nuked every send).
        let pending = bundle("new");
        assert!(!f(Some(&pending), &pending));
    }

    #[test]
    fn key_changed_again_is_newly_observed() {
        // Pending alert was for KEY_A but the peer rotated AGAIN to
        // KEY_B → the bootstrapped session is invalid → drop again.
        assert!(f(Some(&bundle("a")), &bundle("b")));
    }

    #[test]
    fn repeated_calls_drop_exactly_once() {
        // Simulate the per-send refresh loop: first call drops, all
        // subsequent identical calls do not.
        let fetched = bundle("rotated");
        let mut pending: Option<KeyBundle> = None;
        let mut drops = 0;
        for _ in 0..50 {
            if f(pending.as_ref(), &fetched) {
                drops += 1;
                pending = Some(fetched.clone()); // alert now raised
            }
        }
        assert_eq!(drops, 1, "ratchet must be dropped exactly once");
    }

    #[test]
    fn discord_snowflake_shaped_true_for_17_through_20_digits() {
        assert!(is_discord_snowflake_shaped("12345678901234567"));
        assert!(is_discord_snowflake_shaped("123456789012345678"));
        assert!(is_discord_snowflake_shaped("1234567890123456789"));
        assert!(is_discord_snowflake_shaped("12345678901234567890"));
    }

    #[test]
    fn discord_snowflake_shaped_false_for_16_and_21_digits() {
        assert!(!is_discord_snowflake_shaped("1234567890123456"));
        assert!(!is_discord_snowflake_shaped("123456789012345678901"));
    }

    #[test]
    fn discord_snowflake_shaped_false_for_non_numeric_osl_user_ids() {
        assert!(!is_discord_snowflake_shaped("osl-user-id"));
        assert!(!is_discord_snowflake_shaped("alice"));
    }

    #[test]
    fn discord_snowflake_shaped_false_for_empty_string() {
        assert!(!is_discord_snowflake_shaped(""));
    }
}

fn tofu_observe_peer(
    state: &AppState,
    discord_id: &str,
    fetched: &crate::tofu::KeyBundle,
) -> Result<(), String> {
    use crate::tofu::{classify, safety_number, TofuOutcome};

    let (outcome, osl_user_id) = {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(discord_id.to_string()).or_default();
        let baseline = trusted_key_bundle(entry);
        let outcome = classify(baseline.as_ref(), fetched);
        if matches!(outcome, TofuOutcome::FirstUse) {
            entry.tofu_ed25519_pub = Some(fetched.ed25519_pub.clone());
            entry.tofu_key_bundle = Some(fetched.clone());
        }
        (outcome, entry.osl_user_id.clone())
    };

    match outcome {
        TofuOutcome::FirstUse => {
            // Make the new baseline durable so the next launch
            // compares against it instead of re-seeding.
            persist_peer_map_now(state);
            // Clear any stale alert (e.g. a prior changed→accepted
            // cycle that left a dangling entry).
            state
                .key_change_alerts
                .lock()
                .expect("key_change_alerts mutex poisoned")
                .remove(discord_id);
        }
        TofuOutcome::Unchanged => {
            state
                .key_change_alerts
                .lock()
                .expect("key_change_alerts mutex poisoned")
                .remove(discord_id);
        }
        TofuOutcome::Changed { old } => {
            // The peer's identity key changed (burn / re-registration
            // / regen). Any existing Double Ratchet was derived from
            // the OLD SessionContext, so it can no longer open this
            // peer's wire — it must be dropped so the next v=4
            // send/recv re-bootstraps a fresh session.
            //
            // BUT: this fn runs on EVERY keyserver refresh, and the
            // v=4 send path force-refreshes before EVERY send. The
            // baseline only advances on an explicit user accept, so a
            // real, un-accepted change stays `Changed` forever. The
            // old code nulled `ratchet_state` on every call → every
            // send re-bootstrapped (huge handshake wire each time),
            // the session could NEVER establish, and the peer saw a
            // permanent `not a recipient` / `header AEAD failed`,
            // with the accept banner the only escape — which itself
            // was invisible until the anchor-resolver fix. So drop
            // EXACTLY ONCE, on first detection of a given new key;
            // re-observing the SAME pending change must leave a
            // freshly bootstrapped ratchet intact so messaging can
            // actually proceed while the user verifies. Security is
            // unchanged: the drop still happens on detection, the
            // blocking alert is still raised, and the baseline still
            // only moves on an explicit accept.
            let pending_same = {
                let g = state
                    .key_change_alerts
                    .lock()
                    .expect("key_change_alerts mutex poisoned");
                g.get(discord_id).map(|a| a.pending_bundle.clone())
            };
            let newly_observed = tofu_change_is_newly_observed(pending_same.as_ref(), fetched);
            if !newly_observed {
                // Same change we already alerted on — do NOT re-nuke
                // a (possibly freshly re-bootstrapped) ratchet, do
                // not reset the alert's first_observed. Idempotent.
                tracing::debug!(
                    "OSL: peer bundle change re-observed; existing \
                     blocking alert retained"
                );
                return Ok(());
            }
            {
                let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
                if let Some(entry) = pm.get_mut(discord_id) {
                    entry.ratchet_state = None;
                }
            }
            persist_peer_map_now(state);
            let alert = crate::state::KeyChangeAlert {
                discord_id: discord_id.to_string(),
                osl_user_id,
                old_ed25519_pub: old.ed25519_pub,
                new_ed25519_pub: fetched.ed25519_pub.clone(),
                new_safety_number: safety_number(fetched).map_err(str::to_owned)?,
                first_observed: keystore::iso_8601_from_unix_seconds(now_unix_secs().max(0) as u64),
                pending_bundle: fetched.clone(),
            };
            tracing::error!("OSL: peer bundle changed; raising blocking key-change alert");
            state
                .key_change_alerts
                .lock()
                .expect("key_change_alerts mutex poisoned")
                .insert(discord_id.to_string(), alert);
        }
    }
    Ok(())
}

// =====================================================================
// REGISTER-FIX: TOFU + registration-conflict IPC surface. These are
// the user-visible, non-warn-swallowed security signals: a peer's
// identity key changed, or our own registration was refused because
// the user_id is held by a different key.
// =====================================================================

/// Read + clear the one-shot registration-conflict alert (set when
/// `/v1/register` returned 403). `Some` means the user MUST be shown
/// a blocking warning; the JS layer surfaces it then it's consumed.
pub fn cmd_osl_take_registration_alert(state: &AppState) -> Result<Option<String>, String> {
    Ok(state
        .registration_alert
        .lock()
        .expect("registration_alert mutex poisoned")
        .take())
}

/// All pending peer key-change alerts (TOFU). Stable order by
/// Discord id so the settings UI list doesn't jump.
pub fn cmd_osl_list_key_change_alerts(
    state: &AppState,
) -> Result<Vec<crate::state::KeyChangeAlert>, String> {
    let g = state
        .key_change_alerts
        .lock()
        .expect("key_change_alerts mutex poisoned");
    let mut v: Vec<crate::state::KeyChangeAlert> = g.values().cloned().collect();
    v.sort_by(|a, b| a.discord_id.cmp(&b.discord_id));
    Ok(v)
}

/// User ACCEPTED a peer's new identity key after completing the
/// out-of-band safety-number check: adopt it as the new trusted TOFU
/// baseline, persist, and clear the alert.
pub fn cmd_osl_accept_key_change(
    state: &AppState,
    discord_id: String,
    ceremony_proof: crate::trust_ceremony_proof::TrustCeremonyProof,
) -> Result<(), String> {
    let new_bundle = {
        let g = state
            .key_change_alerts
            .lock()
            .expect("key_change_alerts mutex poisoned");
        match g.get(&discord_id) {
            Some(a) => {
                ceremony_proof
                    .verify_safety_number(&a.new_safety_number)
                    .map_err(|error| error.to_string())?;
                a.pending_bundle.clone()
            }
            None => return Err("OSL: no pending key-change".to_string()),
        }
    };
    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(discord_id.clone()).or_default();
        entry.tofu_ed25519_pub = Some(new_bundle.ed25519_pub.clone());
        entry.pubkey = Some(new_bundle.x25519_pub.clone());
        entry.ik_mlkem768_pub = Some(new_bundle.mlkem768_pub.clone());
        entry.ik_ratchet_initial_pub = new_bundle.ratchet_initial_pub.clone();
        entry.tofu_key_bundle = Some(new_bundle);
        // v=4 desync fix (defensive): an accepted key change is an
        // identity rotation — any ratchet_state was derived from the
        // pre-rotation SessionContext and is undecryptable. Drop it
        // so the next v=4 re-handshakes. (tofu_observe_peer's Changed
        // branch already clears this on observation; this covers the
        // path where the entry was rebuilt between observe and accept.)
        entry.ratchet_state = None;
    }
    persist_peer_map_now(state);
    state
        .key_change_alerts
        .lock()
        .expect("key_change_alerts mutex poisoned")
        .remove(&discord_id);
    tracing::warn!("OSL: user accepted peer bundle change; baseline updated");
    Ok(())
}

/// User DECLINED a peer's new identity key: clear the alert but keep
/// the OLD trusted baseline. The alert re-raises on the next fetch
/// while the key stays changed (so it can't be silently forgotten).
pub fn cmd_osl_decline_key_change(state: &AppState, discord_id: String) -> Result<(), String> {
    let removed = state
        .key_change_alerts
        .lock()
        .expect("key_change_alerts mutex poisoned")
        .remove(&discord_id)
        .is_some();
    if !removed {
        return Err("OSL: no pending key-change".to_string());
    }
    tracing::warn!("OSL: user declined peer bundle change; old baseline kept");
    Ok(())
}

/// Safety number for a peer's complete trusted key bundle.
pub fn cmd_osl_peer_safety_number(state: &AppState, discord_id: String) -> Result<String, String> {
    let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
    let entry = pm
        .get(&discord_id)
        .ok_or_else(|| "OSL: unknown peer".to_string())?;
    let bundle = trusted_key_bundle(entry)
        .ok_or_else(|| "OSL: peer has no trusted key bundle".to_string())?;
    crate::tofu::safety_number(&bundle).map_err(str::to_owned)
}

/// Safety number for our complete public-key bundle.
pub fn cmd_osl_self_safety_number(state: &AppState) -> Result<String, String> {
    let g = state.identity.lock().expect("identity mutex poisoned");
    let id = g
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let bundle = crate::tofu::KeyBundle {
        ed25519_pub: STANDARD.encode(id.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(id.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(id.mlkem_public_bytes),
        ratchet_initial_pub: id
            .ratchet_initial_pub
            .map(|key| STANDARD.encode(key.as_bytes())),
    };
    crate::tofu::safety_number(&bundle).map_err(str::to_owned)
}

/// True when this string is shaped like a Discord snowflake and therefore can
/// never resolve as a keyserver identity (migration 0029 refuses them).
pub fn is_discord_snowflake_shaped(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_digit()) && (17..=20).contains(&value.len())
}

/// Phase 9-A1b: keyserver-refresh helper. Looks up the peer's
/// osl_user_id, queries the keyserver, and writes both pubkeys
/// into peer_map via [`populate_peer_from_fetch_response`]. Used
/// by the v=3 send path to recover from a missing-ML-KEM
/// PeerEntry (legacy entries from before Phase 9-A1).
///
/// Returns:
/// - `Ok(true)` if the ML-KEM pubkey was newly added,
/// - `Ok(false)` if the peer had no osl_user_id (can't fetch) OR
///   the keyserver returned no ML-KEM,
/// - `Err(msg)` if the keyserver request itself errored.
fn refresh_peer_pubkeys_from_keyserver(state: &AppState, discord_id: &str) -> Result<bool, String> {
    let osl_user_id = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(discord_id)
            .and_then(|e| e.osl_user_id.clone())
            .ok_or_else(|| "OSL: peer has no verified OSL identity".to_string())?
    };
    if is_discord_snowflake_shaped(&osl_user_id) {
        return Err("OSL: Discord identifiers cannot resolve keys".to_string());
    }
    let resp = {
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        client
            .fetch_pubkeys(&osl_user_id)
            .map_err(|_| "OSL: peer key fetch refused".to_string())?
    };
    let added = populate_peer_from_fetch_response(state, discord_id, &resp)?;
    Ok(added)
}

/// Apply a received burn marker:
/// - Append a [`crate::peer_map::BurnedScope`] entry to
///   `peer_map[sender].burned_scopes` (idempotent — duplicates
///   skipped).
/// - Shred matching sender rows in local `messages.sqlite`
///   (best-effort; failures logged, not propagated).
/// Legacy `MSG_TYPE_BURN` (0x01) receive handler.
///
/// # Two separate properties
///
/// The row-selection boundary is sender-scoped:
/// `wipe_wrapped_keys_in_scope(.., Some(sender_discord_id))`. A claimed sender's
/// marker must not blank our own or other members' local rows. This selection
/// property does not itself prove visible-row authorship or destroy anyone's
/// remote decryption authority.
///
/// The persistence model is **not**. It records a permanent scope-level entry in
/// `peer_map.burned_scopes`, which has no epoch and no sequence bound, so one
/// stale or replayed marker kills that conversation forever — including content
/// sent after it. That is a permanent denial of service.
///
/// [`crate::revocation`] is the replacement, and it does not reproduce it: burns
/// carry a monotonic epoch and a `burn_upto_seq`, and even an inbound legacy
/// `0x01` is converted to a bounded revocation at "everything of theirs I
/// currently hold" (see [`crate::revocation::legacy_burn_notice`] and
/// `security::apply_legacy_peer_burn`). New code must not call this path or copy
/// its `burned_scopes` write; it remains only so already-shipped legacy clients
/// keep behaving as they do today.
fn apply_burn_recv(
    state: &AppState,
    sender_discord_id: &str,
    marker: &crate::control_messages::BurnMarker,
) -> Result<(), String> {
    use crate::peer_map::BurnedScope as B;
    let burned_at_iso = format_iso8601_secs(marker.burned_at).unwrap_or_else(|| "?".to_string());
    let entry = match marker.scope.kind {
        crate::scope::ScopeKind::Dm => B::Dm {
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::Gc => B::Gc {
            id: marker.scope.id.clone(),
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::ServerChannel => B::ServerChannel {
            server_id: marker.scope.server_id.clone().unwrap_or_default(),
            channel_id: marker.scope.channel_id.clone().unwrap_or_default(),
            burned_at: burned_at_iso,
        },
        crate::scope::ScopeKind::ServerFull => B::ServerFull {
            server_id: marker.scope.server_id.clone().unwrap_or_default(),
            burned_at: burned_at_iso,
        },
    };
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(sender_discord_id.to_string()).or_default();
        if !pe.burned_scopes.iter().any(|b| same_burn(b, &entry)) {
            pe.burned_scopes.push(entry);
        }
    }
    // Shred matching local sqlite rows for the scope (best-effort).
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        let (scope_type, scope_id) = scope_storage_pair(&marker.scope);
        // Burn only the BURNER's messages on our side — sender_discord_id
        // is the peer who burned. Their burn must not blank our own or
        // other members' messages in the same channel.
        if let Err(e) =
            store.wipe_wrapped_keys_in_scope(&scope_type, &scope_id, Some(sender_discord_id))
        {
            tracing::warn!(error = %e, "OSL: wipe wrapped_keys failed; burn proceeded in peer_map only");
        }
        // Evict the burner's cached decrypted attachments in this scope
        // so their already-rendered images vanish, not just stop
        // re-decrypting.
        let _ = store.wipe_attachments_in_scope(&scope_type, &scope_id, Some(sender_discord_id));
    }
    // 7d-FIX1: persist peer_map (the burned-scope entry is new state).
    persist_peer_map_now(state);
    Ok(())
}

// 9-C1: `enqueue_invitation_recv` + `apply_response_recv` removed
// alongside the invitation handshake.

fn same_burn(a: &crate::peer_map::BurnedScope, b: &crate::peer_map::BurnedScope) -> bool {
    use crate::peer_map::BurnedScope as B;
    match (a, b) {
        (B::Dm { .. }, B::Dm { .. }) => true,
        (B::Gc { id: a, .. }, B::Gc { id: bid, .. }) => a == bid,
        (
            B::ServerChannel {
                server_id: sa,
                channel_id: ca,
                ..
            },
            B::ServerChannel {
                server_id: sb,
                channel_id: cb,
                ..
            },
        ) => sa == sb && ca == cb,
        (B::ServerFull { server_id: a, .. }, B::ServerFull { server_id: b, .. }) => a == b,
        _ => false,
    }
}

fn scope_storage_pair(s: &crate::scope::Scope) -> (String, String) {
    use crate::scope::ScopeKind as K;
    let kind = match s.kind {
        K::Dm => "dm",
        K::Gc => "gc",
        K::ServerChannel => "server_channel",
        K::ServerFull => "server_full",
    };
    (kind.to_string(), s.id.clone())
}

/// Stringify a unix-seconds timestamp for storage in the
/// `burned_at` / `enabled_at` / `received_at` ISO-style fields on
/// `peer_map` / `pending_invitations`. The design doc shows
/// ISO-8601, but Phase 7b stores the raw unix-seconds string
/// (e.g. `"1700000000"`) to avoid pulling a date-formatting
/// dep — every consumer in the v=7 codepaths treats these fields
/// as opaque strings already. Phase 7c can swap to true
/// ISO-8601 when the UI needs it (and the JS layer renders
/// formatted dates from a fresh `Date.now()` anyway).
fn format_iso8601_secs(unix_secs: i64) -> Option<String> {
    if unix_secs < 0 {
        return None;
    }
    Some(unix_secs.to_string())
}

// ---- Phase 7b: helper Tauri-callable commands ----

/// Apply a local burn for `scope`. Updates self-side state only —
/// the wire burn marker is sent by `cmd_osl_unwhitelist_scope` /
/// `cmd_osl_send_burn_marker` before this is called, so by the
/// time we're here the recipients have already been notified.
///
/// Effects:
/// - Shreds matching local `messages.sqlite` rows: ciphertext and nonce are
///   zeroed and the legacy `wrapped_key` column is cleared. That column is
///   local store state, not evidence of the unwired server wrapped-key service.
///   This removes this store's cached history; it does not destroy a
///   per-message key or anyone's long-term decryption authority.
/// - **Does not** mutate any peer_map entry. Peer-side burn
///   tracking lives in their burned_scopes; ours is implicit
///   via the shredded local rows + the scope no longer being in
///   whitelist_state.
pub fn cmd_osl_apply_burn(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let (scope_type, scope_id) = scope_storage_pair(&scope);
    // A burn destroys YOUR OWN messages only — NOT every sender's in the
    // channel. Resolve self's Discord id and scope the wipe to it, so a
    // server burn doesn't blank the whole channel (everyone reverting to
    // the DPC0 cover). None falls back to full-scope (account-burn case).
    let self_did = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref().and_then(|id| id.discord_snowflake.clone())
    };
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        store
            .wipe_wrapped_keys_in_scope(&scope_type, &scope_id, self_did.as_deref())
            .map_err(|e| format!("OSL: wipe wrapped_keys: {e}"))?;
        // Also evict OUR cached decrypted attachments in this scope so a
        // previously-rendered image doesn't linger from cache after the
        // burn (the open-gate only blocks re-decryption).
        let _ = store.wipe_attachments_in_scope(&scope_type, &scope_id, self_did.as_deref());
    }
    // Record the burn on our OWN self-entry so OUR attachments in this
    // scope are gated too. Local text history is shredded by the row wipe above;
    // attachments decrypt from a self-contained att_key, so the
    // attachment-open gate consults this burned_scopes ledger
    // (is_burned_in_scope for the sender == us). Without this, your own
    // images stayed openable after you burned the scope.
    if let Some(self_did) = self_did.as_deref() {
        use crate::peer_map::BurnedScope as B;
        let burned_at = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
        let entry = match scope.kind {
            crate::scope::ScopeKind::Dm => B::Dm { burned_at },
            crate::scope::ScopeKind::Gc => B::Gc {
                id: scope.id.clone(),
                burned_at,
            },
            crate::scope::ScopeKind::ServerChannel => B::ServerChannel {
                server_id: scope.server_id.clone().unwrap_or_default(),
                channel_id: scope.channel_id.clone().unwrap_or_default(),
                burned_at,
            },
            crate::scope::ScopeKind::ServerFull => B::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                burned_at,
            },
        };
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm.entry(self_did.to_string()).or_default();
        if !pe.burned_scopes.iter().any(|b| same_burn(b, &entry)) {
            pe.burned_scopes.push(entry);
        }
        drop(pm);
        persist_peer_map_now(state);
    }
    Ok(())
}

// 9-C1: `cmd_osl_accept_invitation` / `cmd_osl_decline_invitation`
// / `apply_invitation_decision` removed alongside the invitation
// handshake.

/// Accept a typed friend request and adopt its exact scoped grant.
///
/// The request itself must already be a [`crate::friend_request::FriendRequest`],
/// which means the grant was minted from complete TOFU-trusted key bundles and
/// matched to the request parties. This command still requires an explicit
/// local peer binding because the typed trust object deliberately carries no
/// Discord map key.
pub fn cmd_osl_accept_friend_request(
    state: &AppState,
    requester_discord_id: String,
    request: crate::friend_request::FriendRequest,
) -> Result<(), String> {
    guard_friend_request_peer_binding(state, &requester_discord_id)?;

    let scope = request.scope_grant.scope().clone();
    adopt_friend_request_scope(state, &requester_discord_id, &scope)?;

    let scope_kind_str = match scope.kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc_full",
        crate::scope::ScopeKind::ServerChannel => "server_channel_full",
        crate::scope::ScopeKind::ServerFull => "server_full",
    };
    let _ = cmd_osl_unburn_scope(state, scope_kind_str.to_string(), scope.id);

    Ok(())
}

fn guard_friend_request_peer_binding(
    state: &AppState,
    requester_discord_id: &str,
) -> Result<(), String> {
    if requester_discord_id.trim().is_empty() {
        return Err("OSL: friend request peer binding is missing".to_string());
    }

    let g = state.identity.lock().expect("identity mutex poisoned");
    if let Some(id) = g.as_ref() {
        if id
            .discord_snowflake
            .as_deref()
            .is_some_and(|self_sf| self_sf == requester_discord_id)
        {
            return Err("OSL: refusing friend request for local self binding".to_string());
        }
    }

    Ok(())
}

fn adopt_friend_request_scope(
    state: &AppState,
    requester_discord_id: &str,
    scope: &crate::scope::Scope,
) -> Result<(), String> {
    let enabled_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());

    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard
            .entry(requester_discord_id.to_string())
            .or_default();
        pe.discord_id
            .get_or_insert_with(|| requester_discord_id.to_string());
        pe.outgoing_whitelists
            .retain(|w| !whitelist_entry_matches(w, scope));
        let new_entry = match scope.kind {
            crate::scope::ScopeKind::Dm => crate::peer_map::WhitelistEntry::Dm {
                broadened: false,
                enabled_at: Some(enabled_at_iso),
            },
            crate::scope::ScopeKind::Gc => crate::peer_map::WhitelistEntry::Gc {
                id: scope.id.clone(),
                user_specific: true,
            },
            crate::scope::ScopeKind::ServerChannel => {
                crate::peer_map::WhitelistEntry::ServerChannel {
                    server_id: scope.server_id.clone().unwrap_or_default(),
                    channel_id: scope.channel_id.clone().unwrap_or_default(),
                    user_specific: true,
                }
            }
            crate::scope::ScopeKind::ServerFull => crate::peer_map::WhitelistEntry::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                user_specific: true,
            },
        };
        pe.outgoing_whitelists.push(new_entry);
        pe.burned_scopes.retain(|b| !burn_matches_scope(b, scope));
    }

    {
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let ws = ws_guard.entry(scope.storage_key()).or_default();
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
    }

    persist_peer_map_now(state);
    persist_whitelist_state_now(state);
    Ok(())
}

#[cfg(test)]
mod friend_request_acceptance_tests {
    use super::cmd_osl_accept_friend_request;
    use crate::friend_request::{
        FriendPeer, FriendRequest, FriendScopeGrant, VerifiedFriendAuthority,
    };
    use crate::peer_map::WhitelistEntry;
    use crate::scope::Scope;
    use crate::tofu::KeyBundle;
    use crate::AppState;

    const REQUESTER_DID: &str = "900000000000000001";

    fn bundle(label: &str) -> KeyBundle {
        KeyBundle {
            ed25519_pub: format!("{label}-ed25519"),
            x25519_pub: format!("{label}-x25519"),
            mlkem768_pub: format!("{label}-mlkem768"),
            ratchet_initial_pub: Some(format!("{label}-ratchet")),
        }
    }

    fn authority(label: &str) -> VerifiedFriendAuthority {
        VerifiedFriendAuthority::from_tofu_trusted_key_bundle(&bundle(label)).unwrap()
    }

    fn request_for(scope: Scope) -> FriendRequest {
        let requester_authority = authority("requester");
        let target_authority = authority("target");
        let grant = FriendScopeGrant::new(&requester_authority, &target_authority, scope);
        FriendRequest::new(
            FriendPeer::from_authority(requester_authority),
            FriendPeer::from_authority(target_authority),
            Some(grant),
        )
        .unwrap()
    }

    #[test]
    fn cmd_osl_accept_friend_request_accepts_request_and_adopts_scoped_trust() {
        let state = AppState::new();
        let scope = Scope::gc("friend-gc");
        let request = request_for(scope.clone());

        cmd_osl_accept_friend_request(&state, REQUESTER_DID.to_string(), request).unwrap();

        let pm = state.peer_map.lock().unwrap();
        let peer = pm.get(REQUESTER_DID).unwrap();
        assert_eq!(peer.discord_id.as_deref(), Some(REQUESTER_DID));
        assert_eq!(peer.outgoing_whitelists.len(), 1);
        assert!(matches!(
            &peer.outgoing_whitelists[0],
            WhitelistEntry::Gc {
                id,
                user_specific: true,
            } if id == "friend-gc"
        ));
        drop(pm);

        let ws = state.whitelist_state.lock().unwrap();
        let adopted = ws.get(&scope.storage_key()).unwrap();
        assert!(adopted.encrypt_toggle);
        assert!(adopted.auto_enabled);
    }

    #[test]
    fn cmd_osl_accept_friend_request_refuses_missing_peer_binding() {
        let state = AppState::new();
        let request = request_for(Scope::dm(REQUESTER_DID));

        let err = cmd_osl_accept_friend_request(&state, " ".to_string(), request).unwrap_err();

        assert_eq!(err, "OSL: friend request peer binding is missing");
        assert!(state.peer_map.lock().unwrap().is_empty());
        assert!(state.whitelist_state.lock().unwrap().is_empty());
    }
}

/// Remove a whitelist entry for `peer` in `scope`. Returns the
/// wire-format burn marker the caller must send through Discord's
/// API so the peer's client records its own local burn/refusal state.
///
/// Behaviour:
/// - Compute burn marker recipients via `recipients_for_scope`
///   **before** mutating state (so the burned peer is still in
///   the recipient list).
/// - Encrypt as v=2 type=0x01.
/// - Mutate local state:
///   - Remove the `WhitelistEntry` from
///     `peer_map[peer].outgoing_whitelists` that matches `scope`.
///   - Append a `BurnedScope` to the same peer's `burned_scopes`.
///   - If `scope.kind == Dm` and `revoke_broadened`: clear the
///     `broadened` flag on any DM whitelist entry for this peer.
///     Per §3.4 this revokes their cross-scope grant in shared
///     GCs/servers without burning those scopes individually.
///   - Drop the scope's entry from `whitelist_state`.
/// - Call `cmd_osl_apply_burn` to shred matching local stored rows.
///
/// Returns the wire-format burn marker string.
pub fn cmd_osl_unwhitelist_scope(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    revoke_broadened: bool,
) -> Result<String, String> {
    // 1. Build the burn marker wire BEFORE mutating state. This is
    //    the ONLY in-Discord-specific step; the local mutation that
    //    follows is shared verbatim with the settings-side
    //    `cmd_osl_local_unwhitelist_scope` via
    //    `local_unwhitelist_apply` so the two paths cannot drift.
    let wire =
        cmd_osl_send_burn_marker(state, scope_input.clone(), channel_members, self_discord_id)?;
    // in-Discord burn path: WITH the local-row shred — byte-
    // identical to pre-repair behaviour. Do not change.
    local_unwhitelist_apply(
        state,
        peer_discord_id,
        scope_input,
        revoke_broadened,
        /* wipe_local_decrypt */ true,
    )?;
    Ok(wire)
}

/// Bug C (whitelist repair): settings-side LOCAL-ONLY unwhitelist.
///
/// Identical local state mutation to [`cmd_osl_unwhitelist_scope`]
/// (same `local_unwhitelist_apply` helper — no drift) but emits NO
/// burn-marker wire and never calls `cmd_osl_send_burn_marker`.
///
/// Operator-accepted semantics: a peer removed from a scope here is
/// removed from OUR outgoing whitelist; we send nothing, so the
/// removed peer can still decrypt ciphertext we sent BEFORE the
/// removal. This is intended "removed locally" behaviour for the
/// out-of-Discord Whitelist Manager, which has no channel roster /
/// self-id context to address a burn marker to anyway.
///
/// Adjustment (operator decision): this path also does NOT shred OUR
/// local stored rows — after "Remove" the operator can still read
/// previously-exchanged messages in that scope; they just stop
/// encrypting to that peer going forward. Pure whitelist removal.
pub fn cmd_osl_local_unwhitelist_scope(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    revoke_broadened: bool,
) -> Result<(), String> {
    local_unwhitelist_apply(
        state,
        peer_discord_id,
        scope_input,
        revoke_broadened,
        /* wipe_local_decrypt */ false,
    )
}

/// Shared local-state half of un-whitelisting. Owns every mutation
/// `cmd_osl_unwhitelist_scope` performs AFTER the wire is built —
/// peer_map retain-filter + BurnedScope marker + revoke_broadened,
/// the whitelist_state encrypt_toggle/scope-row-drop invariant,
/// (conditionally) the local-row shred, and the atomic persist of
/// both files. Keeping this in one place is the "cannot drift"
/// guarantee: the two callers run byte-identical local effects and
/// differ in EXACTLY two orthogonal axes —
///   1. wire emission: only `cmd_osl_unwhitelist_scope` builds one
///      (its step 1, before this helper);
///   2. `wipe_local_decrypt`: `true` for the in-Discord burn path
///      (preserves today's behaviour exactly), `false` for the
///      settings local path (operator keeps local read history).
///
/// Everything else is shared and cannot diverge.
fn local_unwhitelist_apply(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    revoke_broadened: bool,
    wipe_local_decrypt: bool,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;

    // 2. Mutate peer_map for the named peer.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(peer_discord_id.clone()).or_default();
        pe.outgoing_whitelists
            .retain(|w| !whitelist_entry_matches(w, &scope));
        let burned_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
        let burn = match scope.kind {
            crate::scope::ScopeKind::Dm => crate::peer_map::BurnedScope::Dm {
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::Gc => crate::peer_map::BurnedScope::Gc {
                id: scope.id.clone(),
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::ServerChannel => crate::peer_map::BurnedScope::ServerChannel {
                server_id: scope.server_id.clone().unwrap_or_default(),
                channel_id: scope.channel_id.clone().unwrap_or_default(),
                burned_at: burned_at_iso,
            },
            crate::scope::ScopeKind::ServerFull => crate::peer_map::BurnedScope::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                burned_at: burned_at_iso,
            },
        };
        if !pe.burned_scopes.iter().any(|b| same_burn(b, &burn)) {
            pe.burned_scopes.push(burn);
        }
        if revoke_broadened && scope.kind == crate::scope::ScopeKind::Dm {
            for w in pe.outgoing_whitelists.iter_mut() {
                if let crate::peer_map::WhitelistEntry::Dm { broadened, .. } = w {
                    *broadened = false;
                }
            }
        }
    }

    // 3. Remove the named peer from the scope's whitelist
    //    entry, but KEEP `encrypt_toggle` intact (7d-PIVOT
    //    decision Q3:B — scope burn destroys data + removes
    //    whitelist for this peer, but the user's per-scope
    //    encrypt preference survives). The scope entry stays
    //    in whitelist_state as long as encrypt_toggle is set
    //    or any other peers remain whitelisted; otherwise we
    //    drop the empty entry to keep the file compact.
    {
        // 9-C1: membership lives on PeerEntry only. The per-scope
        // ScopeState carries just the encrypt-toggle / auto-enabled
        // flag pair; drop the entire entry if the toggle is off so
        // the file stays compact.
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let key = scope.storage_key();
        let drop_entry = if let Some(entry) = ws_guard.get_mut(&key) {
            !entry.encrypt_toggle
        } else {
            false
        };
        if drop_entry {
            ws_guard.remove(&key);
        }
    }

    // 4. Shred matching local store rows — ONLY when the caller is the
    //    in-Discord burn path. This is the SOLE behavioural difference
    //    between the two callers. `cmd_osl_apply_burn` calls the legacy-
    //    named `store.wipe_wrapped_keys_in_scope`, which zeroes local
    //    ciphertext/nonces, clears the local `wrapped_key` column, and
    //    marks those rows burned. This removes this store's cached history;
    //    it is not remote wrapped-key deletion or cryptographic erasure of
    //    carrier ciphertext sealed to long-term recipient keys. The `BurnedScope`
    //    marker pushed above is OUTBOUND-only bookkeeping — the
    //    decrypt path never consults `peer_map.burned_scopes` — so
    //    it stays in BOTH paths and the two cannot drift on it.
    //    settings "Remove" passes `false`: pure whitelist removal,
    //    local read history preserved.
    if wipe_local_decrypt {
        let _ = cmd_osl_apply_burn(state, (&scope).into());
    }

    // 7d-FIX1: persist peer_map + whitelist_state.
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    Ok(())
}

fn whitelist_entry_matches(w: &crate::peer_map::WhitelistEntry, s: &crate::scope::Scope) -> bool {
    use crate::peer_map::WhitelistEntry as W;
    use crate::scope::ScopeKind as K;
    match (w, &s.kind) {
        (W::Dm { .. }, K::Dm) => true,
        (W::Gc { id, .. }, K::Gc) => id == &s.id,
        (
            W::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            K::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (W::ServerFull { server_id, .. }, K::ServerFull) => Some(server_id) == s.server_id.as_ref(),
        _ => false,
    }
}

/// Defense-in-depth self-guard for the whitelist write commands.
///
/// Rejects `peer_discord_id` when it equals the loaded identity's
/// own Discord snowflake. A correct UI never whitelists self; this
/// exists so a future peer-resolution regression in the injection
/// layer (the Symptom-2 bug class — boot.js handing back the local
/// user's snowflake) can never again silently key `peer_map` by
/// self, fetch self's keyserver keys, and collapse sends to
/// encrypt-to-self. If the identity carries no snowflake yet there
/// is nothing to compare against, so the guard is a no-op (the
/// normal pre-snowflake state is unaffected).
fn guard_not_self(state: &AppState, peer_discord_id: &str) -> Result<(), String> {
    let g = state.identity.lock().expect("identity mutex poisoned");
    if let Some(id) = g.as_ref() {
        if let Some(self_sf) = id.discord_snowflake.as_deref() {
            if self_sf == peer_discord_id {
                return Err(format!(
                    "OSL: refusing to whitelist yourself — peer id {peer_discord_id} \
                     is this client's own identity snowflake. This indicates a \
                     peer-resolution bug in the UI (it handed back your own id \
                     instead of the conversation peer's); the whitelist was NOT \
                     written. Please report this."
                ));
            }
        }
    }
    Ok(())
}

/// Set a whitelist for `peer_discord_id` in `scope`. Local-only:
/// it mutates client state and returns `()`. There is NO wire and
/// NO invitation (the 9-C1 handshake was removed; decrypt is
/// permissive). The peer needs no acceptance — once they have our
/// keys their recv path simply decrypts.
///
/// Behaviour:
/// - Mutate peer_map: append/replace the per-peer `WhitelistEntry`
///   for the scope. For DM scope, the `broadened` flag carries
///   through; any prior `BurnedScope` for the same scope is evicted
///   (re-whitelisting after a burn is allowed).
/// - Mutate whitelist_state: ensure the scope has a `ScopeState`
///   with `encrypt_toggle = true; auto_enabled = true` (§2.3
///   auto-enable). 9-C1: membership lives per-peer on PeerEntry;
///   ScopeState carries only the encrypt-toggle pair.
/// - Persist peer_map + whitelist_state atomically.
pub fn cmd_osl_set_whitelist(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    broadened: bool,
) -> Result<(), String> {
    // Whitelist repair (Bug A cleanup): the dead `from_discord_id`
    // param (9-C1 handshake leftover, "kept for binding
    // compatibility") is now removed end-to-end — Rust signature,
    // main.rs wrapper, and the boot.js caller.
    //
    // SELF-GUARD (defense-in-depth): never whitelist the local
    // identity as a "peer". A correct UI never does this; if it
    // happens, a peer-resolution regression is feeding us our own
    // snowflake (the Symptom-2 bug class). Fail closed + loud so it
    // can never silently key peer_map by self again.
    guard_not_self(state, &peer_discord_id)?;
    let scope: crate::scope::Scope = scope_input
        .clone()
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let enabled_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());

    // 1. peer_map.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(peer_discord_id.clone()).or_default();
        // De-dupe: remove any prior entry for the same scope
        // shape before appending the new one.
        pe.outgoing_whitelists
            .retain(|w| !whitelist_entry_matches(w, &scope));
        let new_entry = match scope.kind {
            crate::scope::ScopeKind::Dm => crate::peer_map::WhitelistEntry::Dm {
                broadened,
                enabled_at: Some(enabled_at_iso),
            },
            crate::scope::ScopeKind::Gc => crate::peer_map::WhitelistEntry::Gc {
                id: scope.id.clone(),
                user_specific: true,
            },
            crate::scope::ScopeKind::ServerChannel => {
                crate::peer_map::WhitelistEntry::ServerChannel {
                    server_id: scope.server_id.clone().unwrap_or_default(),
                    channel_id: scope.channel_id.clone().unwrap_or_default(),
                    user_specific: true,
                }
            }
            crate::scope::ScopeKind::ServerFull => crate::peer_map::WhitelistEntry::ServerFull {
                server_id: scope.server_id.clone().unwrap_or_default(),
                user_specific: true,
            },
        };
        pe.outgoing_whitelists.push(new_entry);
        // Also evict any prior burned-scope entry for the same
        // scope shape — re-whitelisting after a burn is allowed
        // and the §3.5 semantics say "fresh keys → new messages
        // encrypt and decrypt normally."
        pe.burned_scopes.retain(|b| !burn_matches_scope(b, &scope));
    }

    // 2. whitelist_state.
    {
        // 9-C1: ScopeState carries only the encrypt-toggle pair.
        // The per-peer membership lives on PeerEntry above.
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let ws = ws_guard.entry(scope.storage_key()).or_default();
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
    }

    // 7d-FIX1: persist BOTH files. Encryption-at-rest is applied
    // transparently by write_peer_map / write_whitelist_state via
    // `maybe_encrypt` when a main password is set.
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    // 7d-FIX1 decision-B: re-whitelisting a scope removes it from
    // the global burned-scopes ledger so the receive observer
    // stops skipping its messages. Old burned ciphertext stays
    // unreadable (wrapped_keys gone), but NEW messages decrypt
    // normally.
    let scope_kind_str = match scope.kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc_full",
        crate::scope::ScopeKind::ServerChannel => "server_channel_full",
        crate::scope::ScopeKind::ServerFull => "server_full",
    };
    let _ = cmd_osl_unburn_scope(state, scope_kind_str.to_string(), scope.id.clone());
    let _ = scope_input;
    let _ = peer_discord_id;

    // 9-C1: no more wire invitation — recv path is permissive.
    Ok(())
}

/// 9-C1 Stage 3: bulk-whitelist N peers in a single scope. Used by
/// the tri-state header icon's "encrypt with everyone" flow — one
/// click promotes every channel member instead of N round-trips
/// through `cmd_osl_set_whitelist`. Returns the count of peers
/// whose `outgoing_whitelists` was actually mutated (skips no-ops
/// where the entry was already present).
pub fn cmd_osl_bulk_set_whitelist(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    // SELF-GUARD (defense-in-depth): a correct caller filters self
    // out of the member list before bulk-whitelisting. If self is
    // present, a peer-resolution regression produced the roster —
    // fail closed + loud rather than key peer_map by self.
    for did in &member_dids {
        guard_not_self(state, did)?;
    }
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let enabled_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
    let mut affected = 0usize;
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        for did in &member_dids {
            let pe = pm_guard.entry(did.clone()).or_default();
            if pe.discord_id.is_none() {
                pe.discord_id = Some(did.clone());
            }
            let already = pe
                .outgoing_whitelists
                .iter()
                .any(|w| whitelist_entry_matches(w, &scope));
            if already {
                continue;
            }
            let new_entry = match scope.kind {
                crate::scope::ScopeKind::Dm => crate::peer_map::WhitelistEntry::Dm {
                    broadened: false,
                    enabled_at: Some(enabled_at_iso.clone()),
                },
                crate::scope::ScopeKind::Gc => crate::peer_map::WhitelistEntry::Gc {
                    id: scope.id.clone(),
                    user_specific: false,
                },
                crate::scope::ScopeKind::ServerChannel => {
                    crate::peer_map::WhitelistEntry::ServerChannel {
                        server_id: scope.server_id.clone().unwrap_or_default(),
                        channel_id: scope.channel_id.clone().unwrap_or_default(),
                        user_specific: false,
                    }
                }
                crate::scope::ScopeKind::ServerFull => {
                    crate::peer_map::WhitelistEntry::ServerFull {
                        server_id: scope.server_id.clone().unwrap_or_default(),
                        user_specific: false,
                    }
                }
            };
            pe.outgoing_whitelists.push(new_entry);
            pe.burned_scopes.retain(|b| !burn_matches_scope(b, &scope));
            affected += 1;
        }
    }
    {
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let ws = ws_guard.entry(scope.storage_key()).or_default();
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
    }
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    Ok(affected)
}

/// 9-C1 Stage 3: bulk-unwhitelist N peers from a single scope.
/// Symmetric to `cmd_osl_bulk_set_whitelist`. Drops each named
/// peer's matching `WhitelistEntry`, adds a fresh `BurnedScope` to
/// their `burned_scopes`. The scope's `encrypt_toggle` is left
/// alone — the caller's confirm-modal UX decides whether to also
/// flip the toggle off (a separate command).
///
/// Returns the count of peers actually mutated. Skips no-ops.
/// Unlike the single-peer `cmd_osl_unwhitelist_scope`, this does
/// NOT emit a burn-marker wire; the caller dispatches that
/// separately if it wants peer notification.
pub fn cmd_osl_bulk_unwhitelist_scope(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let burned_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
    let mut affected = 0usize;
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        for did in &member_dids {
            let pe = match pm_guard.get_mut(did) {
                Some(p) => p,
                None => continue,
            };
            let before = pe.outgoing_whitelists.len();
            pe.outgoing_whitelists
                .retain(|w| !whitelist_entry_matches(w, &scope));
            if pe.outgoing_whitelists.len() == before {
                // No matching entry to remove — skip the burn-marker
                // bookkeeping to avoid spuriously marking peers as
                // burned for scopes they never had.
                continue;
            }
            let burn = match scope.kind {
                crate::scope::ScopeKind::Dm => crate::peer_map::BurnedScope::Dm {
                    burned_at: burned_at_iso.clone(),
                },
                crate::scope::ScopeKind::Gc => crate::peer_map::BurnedScope::Gc {
                    id: scope.id.clone(),
                    burned_at: burned_at_iso.clone(),
                },
                crate::scope::ScopeKind::ServerChannel => {
                    crate::peer_map::BurnedScope::ServerChannel {
                        server_id: scope.server_id.clone().unwrap_or_default(),
                        channel_id: scope.channel_id.clone().unwrap_or_default(),
                        burned_at: burned_at_iso.clone(),
                    }
                }
                crate::scope::ScopeKind::ServerFull => crate::peer_map::BurnedScope::ServerFull {
                    server_id: scope.server_id.clone().unwrap_or_default(),
                    burned_at: burned_at_iso.clone(),
                },
            };
            if !pe.burned_scopes.iter().any(|b| same_burn(b, &burn)) {
                pe.burned_scopes.push(burn);
            }
            affected += 1;
        }
    }
    if affected > 0 {
        let _ = cmd_osl_apply_burn(state, (&scope).into());
    }
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);
    Ok(affected)
}

fn burn_matches_scope(b: &crate::peer_map::BurnedScope, s: &crate::scope::Scope) -> bool {
    use crate::peer_map::BurnedScope as B;
    use crate::scope::ScopeKind as K;
    match (b, &s.kind) {
        (B::Dm { .. }, K::Dm) => true,
        (B::Gc { id, .. }, K::Gc) => id == &s.id,
        (
            B::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            K::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (B::ServerFull { server_id, .. }, K::ServerFull) => Some(server_id) == s.server_id.as_ref(),
        _ => false,
    }
}

// ---- Phase 7c: UI-supporting read/write commands ----
//
// These thin wrappers expose pieces of whitelist_state +
// pending_invitations to boot.js so the channel-header encrypt
// toggle, burn button, and invitation banner can render their
// initial state without each having to walk the full schema.

/// Per-scope encryption posture for the channel-header lock icon.
///
/// - `encrypt_toggle`: the user's current ON/OFF state for
///   encryption in this scope. Drives the icon's "open lock vs
///   closed lock" visual.
///
/// 9-C1: `has_whitelist` retained for backwards-compat with boot.js
/// callers. New code should consume
/// [`cmd_osl_get_scope_whitelist_summary`] for the tri-state icon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ScopeEncryptionState {
    pub encrypt_toggle: bool,
    /// 9-C1: always reports `true` for any scope with an
    /// `encrypt_toggle == true` entry. Membership is no longer
    /// scope-side, so an existing toggle is the closest analog to
    /// the old "any recipient whitelisted in this scope" flag.
    pub has_whitelist: bool,
}

pub fn cmd_osl_get_scope_encryption_state(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<ScopeEncryptionState, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let encrypt_toggle = ws_guard
        .get(&key)
        .map(|s| s.encrypt_toggle)
        .unwrap_or(false);
    Ok(ScopeEncryptionState {
        encrypt_toggle,
        has_whitelist: encrypt_toggle,
    })
}

/// 9-C1: per-channel whitelist intersection summary for the
/// tri-state icon. Computes how many of the supplied
/// `channel_members` are whitelisted for the given scope. JS
/// passes the live channel members (typically from the React
/// fiber walk or the gateway-fed `channel_members` cache).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ScopeWhitelistSummary {
    pub encrypt_toggle: bool,
    pub whitelisted_count: usize,
    pub total_members: usize,
    /// One of `"all"`, `"some"`, `"none"`, `"unknown"`. `"unknown"`
    /// fires when `total_members == 0` — boot.js hits this on
    /// server channels whose roster hasn't arrived yet via the
    /// gateway tap.
    pub state: String,
}

pub fn cmd_osl_get_scope_whitelist_summary(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<ScopeWhitelistSummary, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let encrypt_toggle = {
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        ws_guard
            .get(&scope.storage_key())
            .map(|s| s.encrypt_toggle)
            .unwrap_or(false)
    };
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    let non_self_members: Vec<&String> = channel_members
        .iter()
        .filter(|m| **m != self_discord_id)
        .collect();
    let total_members = non_self_members.len();
    if total_members == 0 {
        return Ok(ScopeWhitelistSummary {
            encrypt_toggle,
            whitelisted_count: 0,
            total_members: 0,
            state: "unknown".to_string(),
        });
    }
    let whitelisted_count = non_self_members
        .iter()
        .filter(|m| crate::whitelist::can_encrypt_to(&pm_guard, &scope, m))
        .count();
    let summary_state = if whitelisted_count == 0 {
        "none"
    } else if whitelisted_count == total_members {
        "all"
    } else {
        "some"
    }
    .to_string();
    Ok(ScopeWhitelistSummary {
        encrypt_toggle,
        whitelisted_count,
        total_members,
        state: summary_state,
    })
}

/// Layer 10 / Phase 7c: flip `encrypt_toggle` for a scope.
/// Returns the new value (post-flip) so boot.js doesn't need to
/// follow up with a read.
///
/// Refuses to enable the toggle when `has_whitelist == false` —
/// per design doc §2.4, the toggle is grayed-out / unavailable
/// in that state. boot.js gates the click handler on
/// `has_whitelist`, but we double-check here so a buggy caller
/// can't end up with encrypt-to-nobody enabled.
pub fn cmd_osl_toggle_scope_encryption(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<bool, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let mut ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let entry = ws_guard.entry(key).or_default();
    // 7d-PIVOT: encrypt_toggle is now independent of whitelist
    // existence. Toggling ON with no whitelist is the
    // "encrypt-to-self-only" mode — your messages encrypt and
    // only you can decrypt them. The previous
    // `encrypt_toggle_refused_no_whitelist` early-error has been
    // removed.
    entry.encrypt_toggle = !entry.encrypt_toggle;
    // Mark `auto_enabled = false` since this is a manual
    // user action — distinguishes the §2.3 auto-enable from a
    // later user toggle in the UI's tooltip.
    entry.auto_enabled = false;
    let new_toggle = entry.encrypt_toggle;
    drop(ws_guard);
    // 7d-FIX1: persist the new toggle state.
    persist_whitelist_state_now(state);
    Ok(new_toggle)
}

/// 7d-PIVOT: explicit set (not toggle) of a scope's encrypt state.
/// Used by the composer toggle UI which knows the desired end state
/// rather than just "flip whatever it was." Idempotent — no-op when
/// the requested state already matches. Persists on change.
pub fn cmd_osl_set_scope_encrypt(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    enabled: bool,
) -> Result<bool, String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let key = scope.storage_key();
    let mut ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let entry = ws_guard.entry(key).or_default();
    if entry.encrypt_toggle == enabled {
        return Ok(enabled);
    }
    entry.encrypt_toggle = enabled;
    entry.auto_enabled = false;
    drop(ws_guard);
    persist_whitelist_state_now(state);
    Ok(enabled)
}

// 9-C1: `PendingInvitationDto` / `cmd_osl_list_pending_invitations`
// removed alongside the invitation handshake.

/// Phase 7c bug-fix #1 (round 3): return the local user's
/// **Discord snowflake** by reverse-lookup against `peer_map`.
///
/// Naming hazard: `Identity::user_id` is the **OSL** user_id
/// (a logical username like "liam"), NOT a Discord snowflake.
/// The injection layer needs the Discord snowflake for send
/// pipelines (`self_discord_id` excludes self from channel-
/// member walks). The snowflake is configured in
/// `peer_map.json` keyed by snowflake with
/// `PeerEntry::osl_user_id` as the value — so we walk the map
/// and return the key whose entry matches our `Identity`.
///
/// Failure modes (all flat-string `Err`):
///   - `"OSL: identity not loaded"` — bootstrap hasn't run
///     or identity.json is missing.
///   - `"OSL: self not registered in peer_map.json (osl_user_id=<name>);
///     add an entry mapping your Discord snowflake to
///     {"osl_user_id":"<name>"}"` — identity loaded but no
///     peer_map row has a matching `osl_user_id`. JS toasts
///     this so the user can fix peer_map.json without grepping
///     logs.
pub fn cmd_osl_get_self_user_id(state: &AppState) -> Result<String, String> {
    let osl_user_id = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        identity.user_id.clone()
    };
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    for (discord_id, entry) in pm_guard.iter() {
        if entry.osl_user_id.as_deref() == Some(osl_user_id.as_str()) {
            return Ok(discord_id.clone());
        }
    }
    Err(format!(
        "OSL: self not registered in peer_map.json \
         (osl_user_id={osl_user_id}); add an entry mapping your Discord \
         snowflake to {{\"osl_user_id\":\"{osl_user_id}\"}}",
        osl_user_id = crate::log_id::log_id(&osl_user_id)
    ))
}

// =====================================================================
// Phase 7d-A: settings-menu data sources
// =====================================================================

/// 7d-A: payload backing the Identity page of the settings modal.
/// All fields are display-only — JS renders them in a read-only
/// monospace block. Missing data points (e.g. snowflake not yet
/// in peer_map, keyserver.json absent) come back as the literal
/// string `"Unknown"` rather than an `Err`, so the page always
/// renders even when bootstrap was partially successful.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdentityInfoDto {
    pub osl_user_id: String,
    pub discord_snowflake: String,
    pub pubkey: String,
    pub keyserver_url: String,
}

/// 7d-A: assemble the Identity page payload from `AppState` plus a
/// best-effort read of `keyserver.json` for the configured base
/// URL. The Tauri shell exposes this via `osl_get_identity_info`.
pub fn cmd_osl_get_identity_info(state: &AppState) -> Result<IdentityInfoDto, String> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let (osl_user_id, pubkey_b64) = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        (
            identity.user_id.clone(),
            STANDARD.encode(identity.x25519_public.as_bytes()),
        )
    };
    // Snowflake: reverse-lookup peer_map (same shape as
    // cmd_osl_get_self_user_id). Display-only — "Unknown" on miss.
    let snowflake = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let mut found = None;
        for (discord_id, entry) in pm_guard.iter() {
            if entry.osl_user_id.as_deref() == Some(osl_user_id.as_str()) {
                found = Some(discord_id.clone());
                break;
            }
        }
        found.unwrap_or_else(|| "Unknown".to_string())
    };
    // Keyserver URL: the override-or-default the client actually
    // uses (keyserver.json `base_url` if present+valid → else the
    // built-in production default). Same single resolver every other
    // consumer uses, so this display value can't disagree with what
    // license/bootstrap actually talk to.
    let keyserver_url = match keystore::osl_config_dir() {
        Ok(dir) => resolve_keyserver_base_url(&dir),
        Err(_) => "Unknown".to_string(),
    };
    Ok(IdentityInfoDto {
        osl_user_id,
        discord_snowflake: snowflake,
        pubkey: pubkey_b64,
        keyserver_url,
    })
}

/// F2.1: validate a license key against the keyserver's
/// `/v1/license/validate` endpoint.
///
/// Reads the keyserver base URL from `<config_dir>/keyserver.json`
/// the same way `cmd_osl_get_identity_info` does — best-effort
/// inline read, no shared bootstrap helper (the bootstrap loader
/// is private to bootstrap.rs).
///
/// This sub-phase does NOT cache the result; F2.2 layers the
/// sealed `license.json` cache on top. `state` is accepted for
/// forward-compatibility with F2.2's cache writes.
///
/// Error surface (load-bearing for F2.4 offline-grace logic):
///   - "OSL: keyserver not configured" — no `keyserver.json` or
///     missing `base_url` field. Caller treats as `UNKNOWN`.
///   - "OSL-VALIDATE-ERR:{json}" with `kind = "unreachable"` —
///     network / TLS / DNS failure (`keystore::Error::Transport`).
///     Caller honours cached state when within the 7-day grace
///     window (F2.4).
///   - "OSL-VALIDATE-ERR:{json}" with `kind = "rejected"` — non-2xx
///     response. Caller treats the cached state as stale; do NOT
///     honour offline grace.
///   - "OSL-VALIDATE-ERR:{json}" with `kind = "malformed"` — 200
///     but body didn't deserialise. Caller treats as stale.
///   - "OSL-VALIDATE-ERR:{json}" with `kind = "other"` — defensive
///     catch-all. Treat as stale; surface generic error copy.
///
/// F3.2 retired the F2.1 freeform string prefixes for the
/// validate paths; the JSON tail shape is defined by
/// [`ValidateLicenseError`].
pub fn cmd_osl_validate_license(
    state: &AppState,
    license_key: String,
) -> Result<keystore::LicenseValidateResponse, String> {
    // license.json + keyserver.json are DEVICE-level (base), matching
    // where launch_classify reads them — otherwise the license cache
    // would be written into the per-account subdir and lost on relaunch.
    let dir =
        keystore::osl_base_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    // Fresh installs have no keyserver.json — fall back to the
    // built-in production URL. The file is now an OVERRIDE only
    // (dev/staging), not a hard requirement.
    let base_url = resolve_keyserver_base_url(&dir);
    cmd_osl_validate_license_with_dir_and_url(state, license_key, &dir, &base_url)
}

/// F3.2: typed error surface for [`cmd_osl_validate_license_with_dir_and_url`].
/// Retires the F2.1 string-prefix-dispatch the F2.1 ship report
/// flagged as temporary. The four variants cover every failure
/// path the inner client can produce:
///
/// - [`Self::Unreachable`] — network / TLS / DNS failure
///   ([`keystore::Error::Transport`]). F2.4's offline-grace honours
///   the cached state when this variant fires.
/// - [`Self::Rejected`] — keyserver answered with a non-2xx
///   ([`keystore::Error::HttpStatus`]). Cache treated as stale; no
///   grace extension.
/// - [`Self::Malformed`] — keyserver answered 200 but the body
///   didn't deserialise ([`keystore::Error::Json`]). Same cache
///   policy as `Rejected`.
/// - [`Self::Other`] — defensive catch-all for unreachable-in-
///   practice variants (Io, Sealer, Base64, BlobVersionMismatch,
///   BlobMethodMismatch) plus client-construction errors.
///
/// Wire shape: the IPC returns `Err(format!("OSL-VALIDATE-ERR:{}",
/// serde_json::to_string(&v).unwrap()))`. F2.3's
/// `friendlyValidateError` in `settings_window.html` parses the
/// JSON tail after the prefix.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidateLicenseError {
    Unreachable { message: String },
    Rejected { status: u16, body: String },
    Malformed { message: String },
    Other { message: String },
}

/// F3.2: error-string prefix for typed validate-license rejections.
/// JSON tail deserialises to [`ValidateLicenseError`]. Stable wire
/// string — bump only if the JSON shape changes incompatibly.
pub const OSL_VALIDATE_ERR_PREFIX: &str = "OSL-VALIDATE-ERR:";

fn validate_err(v: ValidateLicenseError) -> String {
    // Inline the unreachable serde fallback. `serde_json::to_string`
    // on this enum cannot realistically fail; the literal default
    // matches the Other-variant shape so a JS parser hitting it
    // gracefully degrades to the generic copy.
    let json = serde_json::to_string(&v).unwrap_or_else(|_| {
        "{\"kind\":\"other\",\"message\":\"validate-err serde failed\"}".to_string()
    });
    format!("{OSL_VALIDATE_ERR_PREFIX}{json}")
}

/// Test-seam variant of [`cmd_osl_validate_license`]. Takes the
/// config dir + keyserver base URL explicitly so unit tests can
/// point at a `tempdir()` + an in-process mock server instead of
/// the real `%APPDATA%\osl` / `keyserver.oslprivacy.com`.
///
/// Cache-write policy (load-bearing for F2.4):
///
/// - Ok(response) → save cache, bump `last_validated_at` to now()
/// - Err(Transport) → DO NOT touch cache (keyserver unreachable; F2.4
///   honours the stale cache during 7-day grace)
/// - Err(HttpStatus / Json) → DO NOT touch cache (keyserver answered,
///   treat cache as stale)
///
/// Error shape (F3.2): all rejection paths return
/// `Err(format!("OSL-VALIDATE-ERR:{json}"))` where the JSON
/// deserialises to [`ValidateLicenseError`]. The F2.1 string-prefix
/// dispatch is retired; F2.3's `friendlyValidateError` JS helper
/// has been updated to parse the new shape in the same sub-phase.
pub fn cmd_osl_validate_license_with_dir_and_url(
    state: &AppState,
    license_key: String,
    dir: &std::path::Path,
    base_url: &str,
) -> Result<keystore::LicenseValidateResponse, String> {
    let client = keystore::KeyServerClient::new(base_url).map_err(|e| {
        validate_err(ValidateLicenseError::Other {
            message: format!("client init: {e}"),
        })
    })?;
    match client.validate_license(&license_key) {
        Ok(resp) => {
            // F2.4 tidy-up: only persist the cache when the
            // keyserver returned a durable, recognized status. A
            // 200 with UNKNOWN or checksum_ok:false means the
            // user mistyped or supplied a never-issued key —
            // writing license.json with status=UNKNOWN would
            // leave a junk cache that the launch hook + 6h
            // refresh would then keep re-classifying as Free.
            let durable = resp.checksum_ok && resp.status != "UNKNOWN";
            if durable {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let inner = keystore::LicenseCacheInner {
                    license_plaintext: license_key.clone(),
                    last_validated_status: resp.status.clone(),
                    current_period_end: resp.current_period_end,
                    last_validated_at: now,
                    checksum_ok: resp.checksum_ok,
                };
                let sealer = keystore::select_best_sealer();
                let path = dir.join("license.json");
                if let Err(e) = keystore::save_license_cache(&path, &inner, sealer.as_ref()) {
                    // Don't fail the command — the validation
                    // itself was successful from the user's POV.
                    // Log so operators see persistent breakage.
                    eprintln!(
                        "[OSL] WARN save_license_cache failed: {e}; \
                         validation succeeded but result not cached"
                    );
                } else {
                    // F2.4: stamp AppState in-memory too so a
                    // subsequent get_license_state read sees the
                    // fresh value without waiting for a relaunch
                    // or the 6h cron.
                    *state
                        .license_state
                        .lock()
                        .expect("license_state mutex poisoned") =
                        keystore::LicenseStateDto::from_cache(&inner);
                }
            }
            Ok(resp)
        }
        Err(keystore::Error::Transport(msg)) => {
            Err(validate_err(ValidateLicenseError::Unreachable {
                message: msg,
            }))
        }
        Err(keystore::Error::HttpStatus { status, body }) => {
            Err(validate_err(ValidateLicenseError::Rejected {
                status,
                body,
            }))
        }
        Err(keystore::Error::Json(e)) => Err(validate_err(ValidateLicenseError::Malformed {
            message: e.to_string(),
        })),
        Err(e) => Err(validate_err(ValidateLicenseError::Other {
            message: e.to_string(),
        })),
    }
}

/// F2.4: read the in-memory license classification stamped by
/// the launch hook + 6h refresh task. Cheap — a single mutex
/// lock + clone. F3's ad gate will hit this on every render of
/// the main webview's hooked surfaces; the AppState read keeps
/// it sub-microsecond.
///
/// File I/O happens at launch (via
/// [`crate::license_lifecycle::launch_classify`]) and on each
/// background refresh — never on this read path.
pub fn cmd_osl_get_license_state(state: &AppState) -> Result<keystore::LicenseStateDto, String> {
    Ok(state
        .license_state
        .lock()
        .expect("license_state mutex poisoned")
        .clone())
}

/// Test-seam variant. Loads the cache from `dir` via
/// [`crate::license_lifecycle::launch_classify`] (which stamps
/// AppState), then returns the freshly-stamped value. The F2.2
/// integration tests use this to verify file→DTO classification
/// without going through the production launch path.
pub fn cmd_osl_get_license_state_with_dir(
    state: &AppState,
    dir: &std::path::Path,
) -> Result<keystore::LicenseStateDto, String> {
    crate::license_lifecycle::launch_classify(state, dir);
    cmd_osl_get_license_state(state)
}

/// F2.2: idempotently delete the cached license. Settings →
/// Account → "Clear license" calls this. Missing file is not an
/// error — the desired post-state is "no cache", regardless of
/// where we started.
pub fn cmd_osl_clear_license(state: &AppState) -> Result<(), String> {
    let dir =
        keystore::osl_base_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    cmd_osl_clear_license_with_dir(state, &dir)
}

/// Test-seam variant of [`cmd_osl_clear_license`]. Also stamps
/// `license_state` to Unconfigured in memory so a follow-up
/// `cmd_osl_get_license_state` doesn't keep returning the
/// pre-clear Paid value for the rest of the session.
///
/// F3.6 pivot: the F3.1 `tier_gate::clear_ad_unlock` call that
/// lived here is removed alongside the ad-unlock window model.
/// Nothing to wipe in tier state on clear — the next gate read
/// derives from license_state directly.
pub fn cmd_osl_clear_license_with_dir(
    state: &AppState,
    dir: &std::path::Path,
) -> Result<(), String> {
    *state
        .license_state
        .lock()
        .expect("license_state mutex poisoned") = keystore::LicenseStateDto::unconfigured();
    let path = dir.join("license.json");
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("OSL: clear_license: {e}")),
    }
}

/// F3.6: read-only snapshot of the tier-gate state. Cheap — one
/// mutex lock + a string clone. Boot.js's read-through cache and
/// the settings Account page both consume this.
///
/// Cross-window grants live in both `main.json` (boot.js consumes
/// for the attachment-gate fast path + future paid-feature checks)
/// AND `settings-window.json` (Account page Free-tier subsection
/// renders the upgrade CTA off this).
///
/// F3.6 pivot: dropped `free_window_active` and `free_window_end`
/// (cf. F3.1) — there's no window any more. Added
/// `attachment_send_allowed` as a named alias for `is_paid` so
/// future paid features (beta channels etc.) can add their own
/// `*_allowed` flags without DTO-shape churn.
pub fn cmd_osl_get_tier_gate_status(state: &AppState) -> Result<TierGateStatusDto, String> {
    let is_paid = crate::tier_gate::is_paid_equivalent(state);
    let raw_license_state = state
        .license_state
        .lock()
        .expect("license_state mutex poisoned")
        .raw_status
        .clone();
    Ok(TierGateStatusDto {
        is_paid,
        attachment_send_allowed: is_paid,
        raw_license_state,
    })
}

/// F3.6 DTO returned by [`cmd_osl_get_tier_gate_status`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TierGateStatusDto {
    /// `true` iff `LicenseState::Paid` or `PaidOfflineGrace`.
    /// The load-bearing flag — every paid-feature flag below is
    /// currently an alias for this one, kept named separately so
    /// future paid features can flip independently.
    pub is_paid: bool,
    /// `true` iff the user may invoke
    /// `cmd_osl_seal_attachment_with_cover_v3`. Today identical
    /// to `is_paid`; named separately so the JS gate has a
    /// feature-specific flag to consult.
    pub attachment_send_allowed: bool,
    /// `LicenseStateDto.raw_status` mirror ("ACTIVE", "Free",
    /// "Unconfigured", "EXPIRED", etc.). Surfaced for diagnostic
    /// rendering on the settings Account page.
    pub raw_license_state: String,
}

/// The only remote keyserver origin trusted by a release client.
pub const DEFAULT_KEYSERVER_BASE_URL: &str = "https://keyserver.oslprivacy.com";

/// Best-effort read of `<config_dir>/keyserver.json` → `base_url`.
/// Mirrors the inline helper in `cmd_osl_get_identity_info`; returns
/// `None` on any failure (file missing, malformed JSON, no
/// `base_url` field).
fn read_keyserver_base_url(dir: &std::path::Path) -> Option<String> {
    let path = dir.join("keyserver.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("base_url")?.as_str().map(|s| s.to_string())
}

fn is_loopback_keyserver_override(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    url.host_str()
        .and_then(|host| {
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .ok()
        })
        .is_some_and(|ip| ip.is_loopback())
}

fn resolve_keyserver_base_url_with_policy(
    dir: &std::path::Path,
    allow_debug_override: bool,
) -> String {
    if allow_debug_override {
        if let Some(value) = read_keyserver_base_url(dir) {
            let canonical = value.trim_end_matches('/');
            if is_loopback_keyserver_override(canonical) {
                return canonical.to_string();
            }
        }
    }
    DEFAULT_KEYSERVER_BASE_URL.to_string()
}

/// Resolve the keyserver base URL. Release builds always return the exact
/// pinned production HTTPS origin and never read a user-controlled remote
/// override. Debug/test builds may use an explicit `keyserver.json` override,
/// but only when it is an HTTP(S) numeric loopback URL.
pub fn resolve_keyserver_base_url(dir: &std::path::Path) -> String {
    resolve_keyserver_base_url_with_policy(dir, cfg!(debug_assertions))
}

#[cfg(test)]
mod keyserver_origin_policy_tests {
    use super::{resolve_keyserver_base_url_with_policy, DEFAULT_KEYSERVER_BASE_URL};

    #[test]
    fn release_policy_ignores_even_a_well_formed_local_override() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keyserver.json"),
            br#"{"base_url":"http://127.0.0.1:8787"}"#,
        )
        .unwrap();
        assert_eq!(
            resolve_keyserver_base_url_with_policy(dir.path(), false),
            DEFAULT_KEYSERVER_BASE_URL,
        );
    }
}

#[cfg(test)]
mod initial_prekey_publish_tests {
    use super::{
        publish_initial_prekeys_at, InitialPrekeyPublishFailure, InitialPrekeyPublishOutcome,
    };

    #[test]
    fn creates_sealed_state_then_uploads_initial_spk_and_opk_pool() {
        let dir = tempfile::tempdir().unwrap();
        let prekey_path = dir.path().join("prekeys.json");
        let marker_path = dir.path().join("prekeys.initial-published");
        let sealer = keystore::MemorySealer::new();
        let identity = keystore::generate_identity("test-user".to_string());
        let mut calls = 0usize;
        let mut uploaded_spk = false;
        let mut uploaded_opks = 0usize;

        let result =
            publish_initial_prekeys_at(&prekey_path, &marker_path, &sealer, &identity, |state| {
                calls += 1;
                uploaded_spk = !state.current_spk.public.iter().all(|b| *b == 0);
                uploaded_opks = state.opk_pool.len();
                assert!(
                    prekey_path.exists(),
                    "prekey state must be durable before network upload"
                );
                Ok(keystore::ReplenishResponse {
                    user_id: identity.user_id.clone(),
                    opks_added: state.opk_pool.len() as u32,
                })
            });

        assert_eq!(result, Ok(InitialPrekeyPublishOutcome::Published));
        assert_eq!(calls, 1);
        assert!(uploaded_spk);
        assert_eq!(
            uploaded_opks,
            keystore::PrekeyConfig::default().opk_pool_target as usize
        );
        assert!(marker_path.exists());
        let loaded = keystore::load_prekey_state(&prekey_path, &sealer).unwrap();
        assert_eq!(loaded.opk_pool.len(), uploaded_opks);
    }

    #[test]
    fn published_marker_suppresses_duplicate_replenish() {
        let dir = tempfile::tempdir().unwrap();
        let prekey_path = dir.path().join("prekeys.json");
        let marker_path = dir.path().join("prekeys.initial-published");
        let sealer = keystore::MemorySealer::new();
        let identity = keystore::generate_identity("test-user".to_string());
        let state = keystore::PrekeyState::new(&identity, keystore::PrekeyConfig::default(), 42);
        keystore::save_prekey_state(&prekey_path, &state, &sealer).unwrap();
        std::fs::write(&marker_path, b"published\n").unwrap();

        let result =
            publish_initial_prekeys_at(&prekey_path, &marker_path, &sealer, &identity, |_| {
                panic!("already-published prekeys must not be uploaded again")
            });

        assert_eq!(result, Ok(InitialPrekeyPublishOutcome::AlreadyPublished));
    }

    #[test]
    fn upload_failure_leaves_state_retryable_without_marker() {
        let dir = tempfile::tempdir().unwrap();
        let prekey_path = dir.path().join("prekeys.json");
        let marker_path = dir.path().join("prekeys.initial-published");
        let sealer = keystore::MemorySealer::new();
        let identity = keystore::generate_identity("test-user".to_string());

        let result =
            publish_initial_prekeys_at(&prekey_path, &marker_path, &sealer, &identity, |_| {
                Err(keystore::Error::Transport("offline".to_string()))
            });

        assert_eq!(result, Err(InitialPrekeyPublishFailure::Upload));
        assert!(prekey_path.exists());
        assert!(!marker_path.exists());
    }
}

/// Best-effort read of `<config_dir>/keyserver.json` → `client_token`.
/// `keyserver.json` is an OVERRIDE only (dev/staging); a fresh
/// production install has no such file and registers against an
/// unsecured-route prod keyserver with no token. Empty string is
/// The old `admin_token` field is a read-only migration alias; deployed
/// clients must never receive the operator token again.
fn read_keyserver_client_token(dir: &std::path::Path) -> Option<String> {
    let path = dir.join("keyserver.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("client_token")
        .or_else(|| v.get("admin_token"))?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

const PREKEY_STATE_FILE: &str = "prekeys.json";
const PREKEY_INITIAL_PUBLISHED_MARKER: &str = "prekeys.initial-published";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialPrekeyPublishOutcome {
    AlreadyPublished,
    Published,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialPrekeyPublishFailure {
    LoadExistingState,
    SaveState,
    Upload,
    MarkPublished,
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn publish_initial_prekeys_after_register(client: &KeyServerClient, identity: &keystore::Identity) {
    let dir = match keystore::osl_config_dir() {
        Ok(dir) => dir,
        Err(_) => {
            tracing::warn!(
                "OSL: ensure_keyserver_registered: cannot resolve config dir; \
                 skipping initial prekey publish"
            );
            return;
        }
    };
    let prekey_path = dir.join(PREKEY_STATE_FILE);
    let marker_path = dir.join(PREKEY_INITIAL_PUBLISHED_MARKER);
    let sealer = keystore::select_best_sealer();
    match publish_initial_prekeys_at(
        &prekey_path,
        &marker_path,
        sealer.as_ref(),
        identity,
        |state| client.replenish_prekeys(identity, Some(&state.current_spk), &state.opk_pool),
    ) {
        Ok(InitialPrekeyPublishOutcome::AlreadyPublished) => {
            tracing::info!("OSL: ensure_keyserver_registered: initial prekeys already published");
        }
        Ok(InitialPrekeyPublishOutcome::Published) => {
            tracing::info!("OSL: ensure_keyserver_registered: initial prekeys published");
        }
        Err(failure) => {
            tracing::warn!(
                failure = ?failure,
                "OSL: ensure_keyserver_registered: initial prekey publish skipped"
            );
        }
    }
}

fn publish_initial_prekeys_at<F>(
    prekey_path: &Path,
    marker_path: &Path,
    sealer: &dyn keystore::Sealer,
    identity: &keystore::Identity,
    mut upload: F,
) -> Result<InitialPrekeyPublishOutcome, InitialPrekeyPublishFailure>
where
    F: FnMut(&keystore::PrekeyState) -> keystore::Result<keystore::ReplenishResponse>,
{
    if marker_path.exists() {
        if prekey_path.exists() {
            let existing = keystore::load_prekey_state(prekey_path, sealer)
                .map_err(|_| InitialPrekeyPublishFailure::LoadExistingState)?;
            if !prekey_state_is_bound_to_identity(&existing, identity) {
                return Err(InitialPrekeyPublishFailure::LoadExistingState);
            }
            return Ok(InitialPrekeyPublishOutcome::AlreadyPublished);
        }
        return Err(InitialPrekeyPublishFailure::LoadExistingState);
    }

    let state = if prekey_path.exists() {
        let existing = keystore::load_prekey_state(prekey_path, sealer)
            .map_err(|_| InitialPrekeyPublishFailure::LoadExistingState)?;
        if !prekey_state_is_bound_to_identity(&existing, identity) {
            return Err(InitialPrekeyPublishFailure::LoadExistingState);
        }
        existing
    } else {
        let state = keystore::PrekeyState::new(
            identity,
            keystore::PrekeyConfig::default(),
            unix_timestamp_seconds(),
        );
        keystore::save_prekey_state(prekey_path, &state, sealer)
            .map_err(|_| InitialPrekeyPublishFailure::SaveState)?;
        state
    };

    upload(&state).map_err(|_| InitialPrekeyPublishFailure::Upload)?;
    if let Some(parent) = marker_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|_| InitialPrekeyPublishFailure::MarkPublished)?;
        }
    }
    std::fs::write(marker_path, b"published\n")
        .map_err(|_| InitialPrekeyPublishFailure::MarkPublished)?;
    Ok(InitialPrekeyPublishOutcome::Published)
}

/// REGISTER-FIX: the single shared implementation of "install the
/// keyserver client into `AppState` and POST our identity pubkeys to
/// `/v1/register`". Both the boot-time path
/// (`bootstrap::init_keyserver_and_register`, which now delegates
/// here) and the post-unlock / post-snowflake runtime paths call
/// THIS function, so the two can never drift.
///
/// Why a runtime caller is needed at all: `run_autostart` executes at
/// cold boot. On a V2 clean install there is no `identity.json` yet
/// (bootstrap no longer auto-creates it), so the boot-time register
/// is skipped and never retried — the machine stays absent from
/// `/v1/pubkeys` and no peer can encrypt to it. The runtime callers
/// close that gap the moment an identity actually exists in state
/// (first ever: the Discord-snowflake registration; every relaunch:
/// the password-gate unlock).
///
/// Idempotency contract (this WILL run on every unlock):
/// - `/v1/register` is a server-side upsert keyed by `user_id`
///   (`keyserver-cf` `upsertUser`: SELECT then UPDATE-or-INSERT).
///   `registered_at` is stamped once on first INSERT and never
///   rewritten; the UPDATE branch writes back the *same* stable
///   public keys the loaded identity always derives
///   (`build_register_request` reads the on-disk identity), so
///   re-registering rotates nothing and is a no-op beyond bumping a
///   `last_rotated_at` metadata timestamp.
/// - No identity in state → no-op (logged), so an early unlock
///   before the snowflake exists doesn't error.
/// - If a keyserver client is already installed, we do NOT
///   re-install or overwrite it (no double-install); we still
///   re-attempt `register` through a freshly-built client so a
///   prior transient failure self-heals on the next unlock.
///
/// Failure posture is identical to the boot path: every failure is a
/// `tracing` event, never a panic, never an `Err` — the app keeps
/// running and the next unlock / next launch retries (chosen over an
/// in-call retry loop so we neither block the unlock UI nor spam the
/// keyserver).
pub fn ensure_keyserver_registered(state: &AppState, base_url: &str, client_token: Option<String>) {
    // Identity gate — nothing to register until one exists.
    {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        if id_guard.is_none() {
            state.set_cloud_registration_state(crate::state::CloudRegistrationState::NotAttempted);
            tracing::info!(
                "OSL: ensure_keyserver_registered: no identity in state; \
                 skipping register (will retry on next unlock once the \
                 Discord-snowflake identity exists)"
            );
            return;
        }
    }
    state.set_cloud_registration_state(crate::state::CloudRegistrationState::Pending);

    // Pure construction (no IO) — safe to build even when a client is
    // already installed; we use it for the register attempt and only
    // conditionally adopt it as the installed client below.
    let client = match KeyServerClient::new(base_url) {
        Ok(c) => c.with_client_token(client_token.clone()),
        Err(e) => {
            state.set_cloud_registration_state(crate::state::CloudRegistrationState::Offline);
            tracing::warn!(
                error = %e,
                base_url = %base_url,
                "OSL: ensure_keyserver_registered: KeyServerClient::new \
                 failed; skipping register"
            );
            return;
        }
    };

    // SECURITY FORWARD-FIX: load any pre-signed Case-C rotation proof
    // minted at the last burn. The config dir + sealer are derived
    // the same way every other caller in this file does
    // (`keystore::osl_config_dir()` + `keystore::select_best_sealer()`).
    // A missing/unreadable proof is the common case (no burn pending)
    // and must never fail or panic — fall through to plain register.
    let pending_rotation_path = match keystore::osl_config_dir() {
        Ok(dir) => Some(dir.join("pending_rotation.json")),
        Err(e) => {
            tracing::warn!(
                error = %e,
                "OSL: ensure_keyserver_registered: cannot resolve config \
                 dir for pending_rotation.json; proceeding without rotation \
                 proof"
            );
            None
        }
    };
    let pending_rotation: Option<keystore::PendingRotation> = match pending_rotation_path.as_ref() {
        Some(path) => {
            let sealer = keystore::select_best_sealer();
            match keystore::load_pending_rotation(path, sealer.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "OSL: ensure_keyserver_registered: failed to load \
                         pending_rotation.json (proceeding without it; \
                         plain register fallback)"
                    );
                    None
                }
            }
        }
        None => None,
    };

    // Register while holding only the identity lock (mirrors the
    // boot path's lock discipline — identity scope, then keyserver
    // scope, never nested — so there is no lock-order deadlock with
    // any other subsystem).
    {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let Some(id) = id_guard.as_ref() else {
            // Cleared between the gate check and here (e.g. a
            // concurrent burn). Nothing to do.
            return;
        };

        // Decide whether the stored proof actually authorizes the
        // CURRENT in-state identity. A stale proof (from an older
        // burn whose new identity was itself later burned) must NOT
        // be presented — delete it and behave as plain register.
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine as _;
        let cur_ed_b64 = B64.encode(id.ed25519_public.as_bytes());
        let usable_proof: Option<keystore::PendingRotation> = match pending_rotation {
            Some(ref p) if p.new_ik_ed25519_pub == cur_ed_b64 => Some(p.clone()),
            Some(_) => {
                tracing::warn!(
                    "OSL: ensure_keyserver_registered: stored \
                     pending_rotation.json does not authorize the current \
                     identity (stale); deleting it and registering plainly"
                );
                if let Some(path) = pending_rotation_path.as_ref() {
                    if let Err(e) = keystore::delete_pending_rotation(path) {
                        tracing::warn!(
                            error = %e,
                            "OSL: ensure_keyserver_registered: failed to \
                             delete stale pending_rotation.json (non-fatal)"
                        );
                    }
                }
                None
            }
            None => None,
        };

        // Helper closures share the alert-clearing + proof-clearing
        // success path so the Case-C and Case-C→fallback branches
        // can't drift.
        let clear_success = |resp: &keystore::RegisterResponse| {
            state.set_cloud_registration_state(crate::state::CloudRegistrationState::Registered);
            tracing::info!(
                user_id = %crate::log_id::log_id(&resp.user_id),
                initial = resp.registered_at.is_some(),
                status = resp.status.as_deref().unwrap_or(
                    if resp.registered_at.is_some() { "registered" } else { "ok" }
                ),
                "OSL: ensure_keyserver_registered: registered with \
                 key-server (rotation proof path)"
            );
            if let Some(path) = pending_rotation_path.as_ref() {
                if let Err(e) = keystore::delete_pending_rotation(path) {
                    tracing::warn!(
                        error = %e,
                        "OSL: ensure_keyserver_registered: failed to delete \
                         consumed pending_rotation.json (non-fatal; the \
                         new_ed binding check prevents a stale re-present)"
                    );
                }
            }
            *state
                .registration_alert
                .lock()
                .expect("registration_alert mutex poisoned") = None;
            publish_initial_prekeys_after_register(&client, id);
        };

        if let Some(proof) = usable_proof {
            match client.register_with_rotation(id, &proof) {
                // Case C accepted ("rotated"), or "noop" / Case-A
                // `registered_at` — any 2xx means the server now
                // holds our CURRENT key. Consume the proof.
                Ok(resp) => clear_success(&resp),
                // The pre-signed rotation was rejected. One-shot
                // fallback: a prior attempt may already have rotated
                // the server onto this key, in which case a plain
                // register is a Case-B noop. If THAT succeeds the
                // server is already on the new key — consume the
                // proof + clear the alert. Otherwise keep the proof
                // (a later launch retries) and raise the 403 alert
                // exactly as the no-proof path does.
                Err(keystore::Error::HttpStatus { status: 403, .. }) => {
                    tracing::warn!(
                        "OSL: ensure_keyserver_registered: pre-signed \
                         rotation rejected (403); trying one-shot plain \
                         register fallback (server may already be on the \
                         new key from a prior attempt)"
                    );
                    match client.register(id) {
                        Ok(resp) => clear_success(&resp),
                        Err(keystore::Error::HttpStatus { status: 403, body }) => {
                            state.set_cloud_registration_state(
                                crate::state::CloudRegistrationState::Conflict,
                            );
                            let msg = format!(
                                "Your OSL identity could not be registered: the keyserver \
                                 reports this account is already registered with a DIFFERENT \
                                 security key. This can mean someone else claimed your \
                                 identity, or you lost your previous key. Encrypted messaging \
                                 to you may be unsafe until resolved. (server: {body})"
                            );
                            tracing::error!(
                                detail = %body,
                                "OSL: ensure_keyserver_registered: REGISTRATION \
                                 CONFLICT (403) after rotation proof + plain \
                                 fallback both rejected; keeping proof for a \
                                 later retry, surfacing blocking alert"
                            );
                            *state
                                .registration_alert
                                .lock()
                                .expect("registration_alert mutex poisoned") = Some(msg);
                        }
                        Err(e) => {
                            state.set_cloud_registration_state(
                                crate::state::CloudRegistrationState::Offline,
                            );
                            tracing::warn!(
                                error = %e,
                                "OSL: ensure_keyserver_registered: plain register \
                                 fallback failed (non-fatal; proof kept, retried \
                                 on next unlock / launch)"
                            )
                        }
                    }
                }
                Err(e) => {
                    state.set_cloud_registration_state(
                        crate::state::CloudRegistrationState::Offline,
                    );
                    tracing::warn!(
                        error = %e,
                        "OSL: ensure_keyserver_registered: register_with_rotation \
                         failed (non-fatal; proof kept, retried on next unlock / \
                         launch)"
                    )
                }
            }
        } else {
            match client.register(id) {
                Ok(_) => {
                    state.set_cloud_registration_state(
                        crate::state::CloudRegistrationState::Registered,
                    );
                    tracing::info!("OSL: identity registered with key-server");
                    // B: a successful register (registered / noop, no 403)
                    // is authoritative proof there is NO key conflict.
                    // Clear any stale 403 alert so a successfully-
                    // registered client shows no alarm — symmetric to
                    // tofu_observe_peer clearing key_change_alerts on
                    // Unchanged/FirstUse. (Known follow-up, deliberately
                    // not done here: a banner already painted in THIS
                    // session from a 403 polled before this success is
                    // not retracted — needs the JS auto-dismiss change.)
                    *state
                        .registration_alert
                        .lock()
                        .expect("registration_alert mutex poisoned") = None;
                    publish_initial_prekeys_after_register(&client, id);
                }
                // REGISTER-FIX: the ONE response we must NOT warn-swallow.
                // 403 = our user_id is held by a DIFFERENT Ed25519 key
                // (someone squatted our snowflake, or we lost our key).
                // Peers will fetch the other key and be unable to talk to
                // us / could be MITM'd. Raise a blocking, user-visible
                // alert + log at error, not warn.
                Err(keystore::Error::HttpStatus { status: 403, body }) => {
                    state.set_cloud_registration_state(
                        crate::state::CloudRegistrationState::Conflict,
                    );
                    let msg = format!(
                        "Your OSL identity could not be registered: the keyserver \
                     reports this account is already registered with a DIFFERENT \
                     security key. This can mean someone else claimed your \
                     identity, or you lost your previous key. Encrypted messaging \
                     to you may be unsafe until resolved. (server: {body})"
                    );
                    tracing::error!(
                        detail = %body,
                        "OSL: ensure_keyserver_registered: REGISTRATION CONFLICT \
                         (403) — user_id held by a different key; surfacing blocking \
                         alert (NOT swallowed)"
                    );
                    *state
                        .registration_alert
                        .lock()
                        .expect("registration_alert mutex poisoned") = Some(msg);
                }
                Err(e) => {
                    state.set_cloud_registration_state(
                        crate::state::CloudRegistrationState::Offline,
                    );
                    tracing::warn!(
                        error = %e,
                        "OSL: ensure_keyserver_registered: key-server register \
                         failed (non-fatal; retried on next unlock / launch)"
                    )
                }
            }
        }
    }

    // Install the client only if the slot is empty. Re-running on a
    // later unlock must not stomp the client bootstrap (or an earlier
    // unlock) already installed.
    {
        let mut ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        if ks_guard.is_none() {
            *ks_guard = Some(client);
            tracing::info!(
                "OSL: ensure_keyserver_registered: keyserver client installed \
                 (was absent — boot-time install had been skipped)"
            );
        }
    }
}

fn ensure_prekeys_after_registration(client: &KeyServerClient, identity: &keystore::Identity) {
    let dir = match keystore::osl_config_dir() {
        Ok(dir) => dir,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "OSL: prekey onboarding: cannot resolve config directory; \
                 skipping prekey publish"
            );
            return;
        }
    };
    if let Err(error) = provision_initial_prekeys(client, identity, &dir) {
        tracing::warn!(
            error = %error,
            "OSL: prekey onboarding: initial prekey publish failed \
             (non-fatal; identity registration remains authoritative)"
        );
    }
}

fn provision_initial_prekeys(
    client: &KeyServerClient,
    identity: &keystore::Identity,
    dir: &Path,
) -> Result<(), String> {
    let path = dir.join("prekeys.json");
    let sealer = keystore::select_best_sealer();
    if path.exists() {
        match keystore::load_prekey_state(&path, sealer.as_ref()) {
            Ok(existing) if prekey_state_is_bound_to_identity(&existing, identity) => {
                tracing::info!(
                    "OSL: prekey onboarding: sealed prekey state already exists; \
                     leaving it unchanged"
                );
                return Ok(());
            }
            Ok(_) => {
                return Err(
                    "existing prekeys.json is not bound to the current identity; refusing to overwrite it"
                        .to_owned(),
                );
            }
            Err(error) => {
                return Err(format!(
                    "existing prekeys.json could not be loaded; refusing to overwrite it: {error}"
                ));
            }
        }
    }

    let state = keystore::PrekeyState::new(
        identity,
        keystore::PrekeyConfig::default(),
        crate::main_password::now_unix_secs_pub() as u64,
    );
    keystore::save_prekey_state(&path, &state, sealer.as_ref())
        .map_err(|error| format!("save prekeys.json: {error}"))?;
    client
        .replenish_prekeys(identity, Some(&state.current_spk), &state.opk_pool)
        .map_err(|error| format!("POST /v1/prekey-bundle/replenish: {error}"))?;
    tracing::info!(
        opks = state.opk_pool.len(),
        "OSL: prekey onboarding: initial prekey batch published"
    );
    Ok(())
}

fn prekey_state_is_bound_to_identity(
    state: &keystore::PrekeyState,
    identity: &keystore::Identity,
) -> bool {
    crypto::ed25519::verify(
        &identity.ed25519_public,
        &state.current_spk.public,
        &crypto::ed25519::Signature::from_bytes(state.current_spk.signature),
    )
    .unwrap_or(false)
}

/// 7d-A: one row in the Whitelist Manager's flat table. The
/// Tauri shell exposes a `Vec<WhitelistRowDto>` via
/// `osl_list_all_whitelists`.
///
/// Field shapes:
///   - `scope_kind`: one of "dm", "gc_full", "gc_per_user",
///     "server_channel_full", "server_channel_per_user",
///     "server_full". JS uses this to render the human-readable
///     scope label and to build the right `ScopeInput` when the
///     user clicks Remove / Burn.
///   - `scope_id`: the raw scope id (peer snowflake for DM,
///     channel id for GC, channel id for server_channel,
///     server id for server_full). NOT the storage key.
///   - `server_id` / `channel_id`: populated when the kind
///     carries them; null otherwise.
///   - `encrypt_toggle`: pulled from `whitelist_state` by
///     storage_key; false when the scope has no state entry.
///   - `broadened`: only meaningful for DM scope; always false
///     for other kinds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WhitelistRowDto {
    pub peer_discord_id: String,
    pub peer_username: String,
    pub scope_kind: String,
    pub scope_id: String,
    pub server_id: Option<String>,
    pub channel_id: Option<String>,
    pub encrypt_toggle: bool,
    pub broadened: bool,
}

/// 7d-A: flatten every peer's outgoing_whitelists into a single
/// list of DTOs for the settings-menu Whitelist Manager. Order
/// is stable: peers sorted by Discord snowflake (string), then
/// scopes in the order they were added (`Vec` preserves insert
/// order).
pub fn cmd_osl_list_all_whitelists(state: &AppState) -> Result<Vec<WhitelistRowDto>, String> {
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    let ws_guard = state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned");
    let mut peers: Vec<(&String, &crate::peer_map::PeerEntry)> = pm_guard.iter().collect();
    peers.sort_by(|a, b| a.0.cmp(b.0));
    let mut out: Vec<WhitelistRowDto> = Vec::new();
    for (discord_id, entry) in peers {
        // Bug D (whitelist repair): never display the bare string
        // "Unknown". Resolution order: keyserver osl_user_id if
        // known → else the Discord snowflake (the map key). The
        // snowflake is always present and unambiguous. (A
        // human-readable Discord username would require plumbing the
        // Discord-origin webview's gateway username cache
        // cross-window into this pure-Rust command + the settings
        // window, which has no Discord context — deferred; the
        // snowflake fallback fully satisfies "never Unknown".)
        let username = entry
            .osl_user_id
            .clone()
            .unwrap_or_else(|| discord_id.clone());
        for w in &entry.outgoing_whitelists {
            let (scope_kind, scope_id, server_id, channel_id, broadened, storage_key) = match w {
                crate::peer_map::WhitelistEntry::Dm { broadened, .. } => (
                    "dm".to_string(),
                    discord_id.clone(),
                    None,
                    Some(discord_id.clone()),
                    *broadened,
                    format!("dm:{discord_id}"),
                ),
                crate::peer_map::WhitelistEntry::Gc { id, user_specific } => {
                    let kind = if *user_specific {
                        "gc_per_user".to_string()
                    } else {
                        "gc_full".to_string()
                    };
                    (
                        kind,
                        id.clone(),
                        None,
                        Some(id.clone()),
                        false,
                        format!("gc:{id}"),
                    )
                }
                crate::peer_map::WhitelistEntry::ServerChannel {
                    server_id,
                    channel_id,
                    user_specific,
                } => {
                    let kind = if *user_specific {
                        "server_channel_per_user".to_string()
                    } else {
                        "server_channel_full".to_string()
                    };
                    let combined = format!("{server_id}:{channel_id}");
                    (
                        kind,
                        combined.clone(),
                        Some(server_id.clone()),
                        Some(channel_id.clone()),
                        false,
                        format!("server_channel:{combined}"),
                    )
                }
                crate::peer_map::WhitelistEntry::ServerFull {
                    server_id,
                    user_specific,
                } => {
                    let kind = if *user_specific {
                        "server_full_per_user".to_string()
                    } else {
                        "server_full".to_string()
                    };
                    (
                        kind,
                        server_id.clone(),
                        Some(server_id.clone()),
                        None,
                        false,
                        format!("server_full:{server_id}"),
                    )
                }
            };
            let encrypt_toggle = ws_guard
                .get(&storage_key)
                .map(|s| s.encrypt_toggle)
                .unwrap_or(false);
            out.push(WhitelistRowDto {
                peer_discord_id: discord_id.clone(),
                peer_username: username.clone(),
                scope_kind,
                scope_id,
                server_id,
                channel_id,
                encrypt_toggle,
                broadened,
            });
        }
    }

    // Probe-3 follow-up: the channel-header / GC-header / server-header
    // whitelist buttons (the Option-B scope-flag model) write into
    // `whitelist_state.json` keyed by scope storage_key — NOT into any
    // peer's `outgoing_whitelists`. The first pass above only walks
    // outgoing_whitelists, so a GC / server whitelisted via the
    // scope-flag button was invisible in the settings list ("isn't
    // listed at all"). Add a second pass surfacing those entries as
    // "(All OSL members)" rows so the user can see + toggle + remove
    // them from settings. Skip scopes that already have at least one
    // per-peer row (avoid duplicating rows that are already visible).
    let already_listed: std::collections::HashSet<String> = out
        .iter()
        .map(|r| match r.scope_kind.as_str() {
            "gc_full" | "gc_per_user" => format!("gc:{}", r.scope_id),
            "server_channel_full" | "server_channel_per_user" => {
                format!("server_channel:{}", r.scope_id)
            }
            "server_full" | "server_full_per_user" => format!("server_full:{}", r.scope_id),
            "dm" => format!("dm:{}", r.scope_id),
            _ => String::new(),
        })
        .collect();
    // Pass 2a: per-scope channel/GC whitelist flag lives on ScopeState
    // (whitelist_state.json).
    for (storage_key, scope_state) in ws_guard.iter() {
        if !scope_state.channel_whitelisted {
            continue;
        }
        if already_listed.contains(storage_key) {
            continue;
        }
        let scope = match crate::scope::Scope::parse(storage_key) {
            Some(s) => s,
            None => continue,
        };
        let (scope_kind, scope_id, server_id, channel_id) = match scope.kind {
            crate::scope::ScopeKind::Gc => (
                "gc_full".to_string(),
                scope.id.clone(),
                None,
                Some(scope.id.clone()),
            ),
            crate::scope::ScopeKind::ServerChannel => {
                let server = scope.server_id.clone().unwrap_or_default();
                let channel = scope.channel_id.clone().unwrap_or_default();
                (
                    "server_channel_full".to_string(),
                    format!("{server}:{channel}"),
                    Some(server),
                    Some(channel),
                )
            }
            // channel_whitelisted is only meaningful on gc:/server_channel:
            // scopes per its doc; skip anything else defensively.
            _ => continue,
        };
        out.push(WhitelistRowDto {
            peer_discord_id: String::new(),
            peer_username: "(All OSL members)".to_string(),
            scope_kind,
            scope_id,
            server_id,
            channel_id,
            encrypt_toggle: scope_state.encrypt_toggle,
            broadened: false,
        });
    }
    // Pass 2b: server-header whitelist flag lives on ServerDefaults,
    // keyed per server_id. Surface each server-header-on server as a
    // synthetic server_full row (consistent with how the existing
    // settings UI handles whole-server whitelist entries).
    let sd_guard = state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned");
    for (server_id, defaults) in sd_guard.iter() {
        if !defaults.server_header_whitelisted {
            continue;
        }
        let key = format!("server_full:{server_id}");
        if already_listed.contains(&key) {
            continue;
        }
        // ScopeState for server_full (if any) carries the encrypt
        // toggle; default false when no entry exists.
        let encrypt_toggle = ws_guard
            .get(&key)
            .map(|s| s.encrypt_toggle)
            .unwrap_or(false);
        out.push(WhitelistRowDto {
            peer_discord_id: String::new(),
            peer_username: "(All OSL members)".to_string(),
            scope_kind: "server_full".to_string(),
            scope_id: server_id.clone(),
            server_id: Some(server_id.clone()),
            channel_id: None,
            encrypt_toggle,
            broadened: false,
        });
    }
    drop(sd_guard);
    Ok(out)
}

// =====================================================================
// Phase 7d-B1: main-password gate commands. Each delegates to the
// `crate::main_password` module which holds the marker/lockout file
// layout, argon2id derivation, AES-GCM phrase blob, and BIP39
// phrase generation. Tauri wrappers in `src-tauri/src/main.rs`.
// =====================================================================

pub use crate::main_password::{LockoutStatusDto, PasswordStatusDto};

// The password gate (marker, lockout, stealth/burn passwords, the
// derived file_storage_key) is DEVICE-level, NOT per-account: one
// password unlocks the device, and the file_storage_key it yields
// encrypts every account's files. So password ops resolve the BASE
// dir, not the active-account dir (osl_config_dir) — otherwise
// multi-account points them at accounts/<sf>/ and the gate "can't
// find the password".
fn password_dir() -> Result<std::path::PathBuf, String> {
    keystore::osl_base_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))
}

pub fn cmd_osl_password_status() -> Result<PasswordStatusDto, String> {
    let dir = password_dir()?;
    Ok(crate::main_password::password_status(&dir))
}

pub fn cmd_osl_set_main_password(password: String) -> Result<String, String> {
    let dir = password_dir()?;
    crate::main_password::set_main_password(&dir, &password)
}

pub fn cmd_osl_change_main_password(current: String, new: String) -> Result<String, String> {
    let dir = password_dir()?;
    crate::main_password::change_main_password(&dir, &current, &new)
}

pub fn cmd_osl_remove_main_password(current: String) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::remove_main_password(&dir, &current)
}

pub fn cmd_osl_view_recovery_phrase(current: String) -> Result<String, String> {
    let dir = password_dir()?;
    crate::main_password::view_recovery_phrase(&dir, &current)
}

/// Device transfer: reveal the 12-word phrase that recovers THIS OSL
/// identity (account) on another device. Reads the entropy the
/// identity was derived from and renders it as a BIP39 mnemonic. Only
/// works on the original device (the one holding the sealed identity).
/// Returns an error for legacy random-key identities (no entropy
/// stored — they predate transfer support).
pub fn cmd_osl_view_identity_recovery_phrase(state: &AppState) -> Result<String, String> {
    let entropy = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        let id = g
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        id.recovery_entropy.ok_or_else(|| {
            "OSL: this account predates transfer support and has no recovery \
             phrase. Create a fresh account (it will get one) to enable transfer."
                .to_string()
        })?
    };
    let mnemonic = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &entropy)
        .map_err(|e| format!("OSL: bip39 encode: {e}"))?;
    Ok(mnemonic.to_string())
}

/// Device transfer: rebuild this device's OSL identity from a 12-word
/// recovery phrase. Derives the SAME keys as the original device
/// (deterministic from the phrase's entropy), overwrites the local
/// identity, and re-registers with the keyserver — which recognizes
/// the identity because the public keys match (no key-change conflict).
/// The Discord account must already be active so we know which
/// snowflake to bind the recovered identity to.
pub fn cmd_osl_recover_identity_from_phrase(
    state: &AppState,
    phrase: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    cmd_osl_recover_identity_from_phrase_with_dir(state, phrase, &dir)
}

pub fn cmd_osl_recover_identity_from_phrase_with_dir(
    state: &AppState,
    phrase: String,
    dir: &std::path::Path,
) -> Result<(), String> {
    let mnemonic = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, phrase.trim())
        .map_err(|_| {
            "OSL: that doesn't look like a valid 12-word recovery phrase \
             (check spelling + word order)."
                .to_string()
        })?;
    let entropy_vec = mnemonic.to_entropy();
    if entropy_vec.len() != 16 {
        return Err("OSL: recovery phrase must be exactly 12 words.".to_string());
    }
    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&entropy_vec);

    // Bind to the currently-active Discord account.
    let snowflake = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref()
            .and_then(|i| i.discord_snowflake.clone())
            .ok_or_else(|| {
                "OSL: open Discord and let it load first — no active account to \
                 attach the recovered identity to yet."
                    .to_string()
            })?
    };

    let mut recovered = keystore::native_identity_from_entropy(entropy);
    recovered.discord_snowflake = Some(snowflake.clone());

    // A legacy identity can have recovery_entropy added later without its
    // original random keys becoming phrase-derived.  Do not let such a
    // phrase silently replace that identity.  A locally matching key is the
    // normal seed-restore case; otherwise require the signed keyserver
    // row to match the phrase-derived Ed25519 key before touching
    // disk.  In particular, a network failure is not permission to replace
    // conflicting local evidence.
    let local_ed_matches = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref()
            .map(|id| recovery_identity_key_matches(id, &recovered))
            .unwrap_or(false)
    };
    if !local_ed_matches {
        let client = {
            let g = state.keyserver.lock().expect("keyserver mutex poisoned");
            g.clone()
        }
        .ok_or_else(|| {
            "OSL: can't verify this recovery phrase because the keyserver is unavailable; \
             refusing to replace a different local identity. Use a full account export or retry \
             after the keyserver connects."
                .to_string()
        })?;
        match client.fetch_pubkeys(&recovered.user_id) {
            Ok(row) => {
                let recovered_ed = STANDARD.encode(recovered.ed25519_public.as_bytes());
                if row.ik_ed25519_pub != recovered_ed {
                    return Err(
                        "OSL: this phrase does not match the identity already registered for \
                         this Discord account. Legacy/random-key accounts must be moved with a \
                         full account export; the local identity was not changed."
                            .to_string(),
                    );
                }
            }
            Err(keystore::Error::HttpStatus { status: 404, .. }) => {}
            Err(e) => {
                return Err(format!(
                    "OSL: can't verify this recovery phrase against the existing account \
                     ({e}); refusing to replace a different local identity. Use a full account \
                     export or retry when the keyserver is reachable."
                ));
            }
        }
    }

    let path = dir.join("identity.json");
    let sealer = keystore::select_best_sealer();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    keystore::save_identity(&path, &recovered, sealer.as_ref())
        .map_err(|e| format!("OSL: recover: save_identity: {e}"))?;
    state.install_identity(recovered);

    // The freshly-generated (pre-recovery) identity's peer ratchet
    // state is moot now; clear it so nothing stale lingers.
    clear_all_peer_ratchet_state(state);

    // Re-register: same public keys as the original device, so the
    // keyserver upsert is recognized with no key-change/TOFU conflict.
    ensure_keyserver_registered(
        state,
        &resolve_keyserver_base_url(dir),
        read_keyserver_client_token(dir),
    );
    verify_and_persist_peer_map_self_entry(state).map(|_| ())
}

fn recovery_identity_key_matches(
    existing: &keystore::Identity,
    recovered: &keystore::Identity,
) -> bool {
    existing.ed25519_public.as_bytes() == recovered.ed25519_public.as_bytes()
}

/// Legacy-account upgrade: if the active identity has no recovery
/// phrase (created before seed-phrase support), assign one now. SAFE —
/// the entropy is recovery metadata + the data-export encryption secret;
/// it does NOT touch the existing keys (those ride along in the data
/// export for transfer). Idempotent. Called by boot.js on launch.
pub fn cmd_osl_ensure_recovery_phrase(state: &AppState) -> Result<(), String> {
    let already = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        match g.as_ref() {
            Some(id) => id.recovery_entropy.is_some(),
            None => return Ok(()), // no identity yet; nothing to do
        }
    };
    if already {
        return Ok(());
    }
    let bytes = crypto::random::random_bytes(16);
    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&bytes);
    {
        let mut g = state.identity.lock().expect("identity mutex poisoned");
        if let Some(id) = g.as_mut() {
            id.recovery_entropy = Some(entropy);
        }
    }
    // Persist (re-seal) so the phrase survives restart.
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    let g = state.identity.lock().expect("identity mutex poisoned");
    if let Some(id) = g.as_ref() {
        let sealer = keystore::select_best_sealer();
        keystore::save_identity(&dir.join("identity.json"), id, sealer.as_ref())
            .map_err(|e| format!("OSL: ensure_recovery_phrase: save: {e}"))?;
        tracing::info!("OSL: assigned a recovery phrase to a legacy account");
    }
    Ok(())
}

/// Magic header for an encrypted data-export blob.
const OSL_EXPORT_MAGIC: &[u8] = b"OSLDATA1";

/// Account data files included in a transfer export, relative to the
/// config dir. Excludes identity.json (rebuilt from the phrase),
/// keyserver/license config + password/lockout markers + UI prefs
/// (device-specific). The message store (sealed under the identity
/// x25519 secret, which the phrase reproduces) is included so history
/// transfers and decrypts on the new device.
const OSL_EXPORT_FILES: &[&str] = &[
    "peer_map.json",
    "whitelist_state.json",
    "sender_key_state.json",
    "channels.json",
    "burned_scopes.json",
    "membership.json",
    "scope_ttl.json",
    "scope_blobs.json",
    "store/messages.sqlite",
    "store/messages.sqlite-wal",
    "store/messages.sqlite-shm",
];

const OSL_EXPORT_STORE_FILES: &[&str] = &[
    "store/messages.sqlite",
    "store/messages.sqlite-wal",
    "store/messages.sqlite-shm",
];

fn is_store_backup_file(rel: &str) -> bool {
    OSL_EXPORT_STORE_FILES.contains(&rel)
}

pub fn guard_backup_destination(rel: &str, destination_encrypted: bool) -> Result<(), String> {
    if !is_store_backup_file(rel) {
        return Ok(());
    }
    crate::mandatory_storage_key_policy::MandatoryStorageKeyPolicy::new()
        .authorize_write(rel, destination_encrypted)
        .map_err(|_| {
            format!(
                "OSL: import: refusing to write {} backup for {rel} into {} without an encrypted destination",
                crate::at_rest_boundary::AtRestBoundary::MessageStore,
                crate::at_rest_boundary::AtRestBoundary::BackupRollbackCopies
            )
        })?;
    Ok(())
}

fn export_aead_key(entropy: &[u8; 16]) -> Result<crypto::aead::Key, String> {
    let k = crypto::hkdf::derive_32(b"OSL-data-export-v1", entropy, b"aead-key")
        .map_err(|e| format!("OSL: export key derive: {e}"))?;
    Ok(crypto::aead::Key::from_bytes(k))
}

fn require_recovery_entropy(state: &AppState, why: &str) -> Result<[u8; 16], String> {
    let g = state.identity.lock().expect("identity mutex poisoned");
    let id = g
        .as_ref()
        .ok_or_else(|| "OSL: identity not loaded".to_string())?;
    id.recovery_entropy.ok_or_else(|| format!("OSL: {why}"))
}

fn decode_export_identity(
    idobj: &serde_json::Value,
    entropy: [u8; 16],
) -> Result<keystore::Identity, String> {
    let dec = |k: &str, n: usize| -> Result<Vec<u8>, String> {
        let s = idobj
            .get(k)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("OSL: import: identity.{k} missing"))?;
        let b = STANDARD
            .decode(s)
            .map_err(|e| format!("OSL: import: identity.{k} b64: {e}"))?;
        if b.len() != n {
            return Err(format!("OSL: import: identity.{k} wrong length"));
        }
        Ok(b)
    };
    let x_sec: [u8; 32] = dec("x25519_secret", 32)?.try_into().unwrap();
    let x_pub: [u8; 32] = dec("x25519_public", 32)?.try_into().unwrap();
    let ed_sec: [u8; 32] = dec("ed25519_secret", 32)?.try_into().unwrap();
    let ed_pub: [u8; 32] = dec("ed25519_public", 32)?.try_into().unwrap();
    let mlkem_sec: [u8; crypto::ml_kem_768::DECAPSULATION_KEY_SIZE] =
        dec("mlkem_secret", crypto::ml_kem_768::DECAPSULATION_KEY_SIZE)?
            .try_into()
            .unwrap();
    let mlkem_pub: [u8; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE] =
        dec("mlkem_public", crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE)?
            .try_into()
            .unwrap();
    let snowflake = idobj
        .get("snowflake")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "OSL: import: identity.snowflake missing".to_string())?;
    let x_secret = crypto::x25519::SecretKey::from_bytes(x_sec);
    if crypto::x25519::derive_public(&x_secret).as_bytes() != &x_pub {
        return Err("OSL: import: X25519 public key does not match its secret".to_string());
    }
    let ed_secret = crypto::ed25519::SecretKey::from_bytes(ed_sec);
    if crypto::ed25519::derive_public(&ed_secret).as_bytes() != &ed_pub {
        return Err("OSL: import: Ed25519 public key does not match its secret".to_string());
    }
    let mut id = keystore::Identity::from_bytes(
        snowflake.to_string(),
        x_sec,
        x_pub,
        ed_sec,
        ed_pub,
        mlkem_sec,
        mlkem_pub,
    );
    id.discord_snowflake = Some(snowflake.to_string());
    id.recovery_entropy = Some(entropy);
    Ok(id)
}

/// Decode and normalize every import entry before any live path is touched.
/// JSON state is always written through the destination's at-rest policy;
/// SQLite files deliberately remain opaque bytes.
fn decode_export_files(
    files: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut decoded = Vec::with_capacity(files.len());
    for (rel, value) in files {
        if !OSL_EXPORT_FILES.contains(&rel.as_str()) {
            return Err(format!("OSL: import: disallowed file path {rel}"));
        }
        let encoded = value
            .as_str()
            .ok_or_else(|| format!("OSL: import: file {rel} is not base64 text"))?;
        let mut bytes = STANDARD
            .decode(encoded)
            .map_err(|e| format!("OSL: import: file {rel} b64: {e}"))?;
        if rel.ends_with(".json") {
            // New exports carry plaintext inside their outer AEAD.  Accept a
            // previously-normalized entry only by unwrapping it first, never
            // by feeding OSL-ENC1 back into maybe_encrypt.
            if crate::main_password::has_enc_magic(&bytes) {
                bytes = crate::main_password::maybe_decrypt(&bytes)
                    .map_err(|e| format!("OSL: import: decrypt {rel}: {e}"))?;
            }
            serde_json::from_slice::<serde_json::Value>(&bytes)
                .map_err(|e| format!("OSL: import: file {rel} is invalid JSON: {e}"))?;
            bytes = crate::main_password::maybe_encrypt(&bytes)
                .map_err(|e| format!("OSL: import: encrypt {rel}: {e}"))?;
        }
        decoded.push((rel.clone(), bytes));
    }
    Ok(decoded)
}

/// Commit a fully validated/staged account import with rollback. Existing
/// targets are first moved into the stage directory, which also removes any
/// destination files omitted by the source export (preventing stale state
/// from a different account surviving the restore). Because all paths live
/// on one filesystem, each rename is atomic and works on Windows where
/// `rename(new, existing)` would otherwise fail.
fn commit_staged_account_import(
    dir: &Path,
    stage: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<(), String> {
    let backup_root = stage.join(".backup");
    let mut targets: Vec<&str> = OSL_EXPORT_FILES.to_vec();
    targets.push("identity.json");
    let mut backed_up: Vec<(&str, PathBuf)> = Vec::new();
    let mut installed: Vec<PathBuf> = Vec::new();

    let rollback = |backed_up: &[(&str, PathBuf)], installed: &[PathBuf]| -> Result<(), String> {
        let mut errors = Vec::new();
        for live in installed.iter().rev() {
            if let Err(e) = std::fs::remove_file(live) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    errors.push(format!("remove installed {}: {e}", live.display()));
                }
            }
        }
        for (rel, backup) in backed_up.iter().rev() {
            let live = dir.join(rel);
            if let Some(parent) = live.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    errors.push(format!("restore mkdir {}: {e}", parent.display()));
                    continue;
                }
            }
            if let Err(e) = std::fs::rename(backup, &live) {
                errors.push(format!(
                    "restore {} from {}: {e}",
                    live.display(),
                    backup.display()
                ));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    };

    let fail = |reason: String, backed_up: &[(&str, PathBuf)], installed: &[PathBuf]| match rollback(
        backed_up, installed,
    ) {
        Ok(()) => reason,
        Err(rollback_error) => format!(
            "{reason}; ROLLBACK INCOMPLETE: {rollback_error}. Recovery files were retained in {}",
            stage.display()
        ),
    };

    for rel in &targets {
        let live = dir.join(rel);
        if !live.exists() {
            continue;
        }
        guard_backup_destination(rel, crate::main_password::get_file_storage_key().is_some())
            .map_err(|e| fail(e, &backed_up, &installed))?;
        let backup = backup_root.join(rel);
        if let Some(parent) = backup.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err(fail(
                    format!("OSL: import: backup mkdir {rel}: {e}"),
                    &backed_up,
                    &installed,
                ));
            }
        }
        if let Err(e) = std::fs::rename(&live, &backup) {
            return Err(fail(
                format!("OSL: import: backup existing {rel}: {e}"),
                &backed_up,
                &installed,
            ));
        }
        backed_up.push((rel, backup));
    }

    let mut staged_rel: Vec<&str> = files.iter().map(|(rel, _)| rel.as_str()).collect();
    staged_rel.push("identity.json");
    for rel in staged_rel {
        let live = dir.join(rel);
        if let Some(parent) = live.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Err(fail(
                    format!("OSL: import: destination mkdir {rel}: {e}"),
                    &backed_up,
                    &installed,
                ));
            }
        }
        if let Err(e) = std::fs::rename(stage.join(rel), &live) {
            return Err(fail(
                format!("OSL: import: install {rel}: {e}"),
                &backed_up,
                &installed,
            ));
        }
        installed.push(live);
    }
    Ok(())
}

/// Restore any account import interrupted between per-file atomic renames.
/// A successful import normally deletes its stage immediately. If the process
/// dies first, preferring the complete pre-import backup is safer than loading
/// a mixed generation of identity, policy, and SQLite files.
pub fn recover_orphaned_account_imports(dir: &Path) -> Result<usize, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(format!("OSL: import recovery: read dir: {e}")),
    };
    let mut recovered = 0;
    for entry in entries {
        let entry = entry.map_err(|e| format!("OSL: import recovery: entry: {e}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(".osl-import-") || !entry.path().is_dir() {
            continue;
        }
        let stage = entry.path();
        let backup_root = stage.join(".backup");
        if !backup_root.exists() {
            std::fs::remove_dir_all(&stage).map_err(|e| {
                format!(
                    "OSL: import recovery: remove uncommitted stage {}: {e}",
                    stage.display()
                )
            })?;
            continue;
        }

        let mut targets: Vec<&str> = OSL_EXPORT_FILES.to_vec();
        targets.push("identity.json");
        for rel in targets {
            let live = dir.join(rel);
            let backup = backup_root.join(rel);
            let staged = stage.join(rel);
            if backup.exists() {
                if live.exists() {
                    std::fs::remove_file(&live).map_err(|e| {
                        format!("OSL: import recovery: remove {}: {e}", live.display())
                    })?;
                }
                if let Some(parent) = live.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        format!("OSL: import recovery: mkdir {}: {e}", parent.display())
                    })?;
                }
                std::fs::rename(&backup, &live).map_err(|e| {
                    format!(
                        "OSL: import recovery: restore {} from {}: {e}",
                        live.display(),
                        backup.display()
                    )
                })?;
            } else if !staged.exists() && live.exists() {
                // This target was absent before import and the staged file was
                // already installed. Rollback removes that newly-created file.
                std::fs::remove_file(&live).map_err(|e| {
                    format!(
                        "OSL: import recovery: remove new file {}: {e}",
                        live.display()
                    )
                })?;
            }
        }
        std::fs::remove_dir_all(&stage).map_err(|e| {
            format!(
                "OSL: import recovery: remove recovered stage {}: {e}",
                stage.display()
            )
        })?;
        recovered += 1;
    }
    Ok(recovered)
}

/// Temporarily closes the live SQLite connection for a byte-level account
/// snapshot/restore. Dropping the last connection checkpoints WAL. The guard
/// reopens only when a store was open before the operation, including errors.
struct MessageStorePause<'a> {
    state: &'a AppState,
    dir: PathBuf,
    was_open: bool,
}

impl<'a> MessageStorePause<'a> {
    fn new(state: &'a AppState, dir: &Path) -> Result<Self, String> {
        let store = state
            .message_store
            .lock()
            .map_err(|_| "OSL: message_store lock poisoned".to_string())?
            .take();
        let was_open = store.is_some();
        drop(store);
        Ok(Self {
            state,
            dir: dir.to_path_buf(),
            was_open,
        })
    }
}

impl Drop for MessageStorePause<'_> {
    fn drop(&mut self) {
        if !self.was_open {
            return;
        }
        let secret = self
            .state
            .identity
            .lock()
            .ok()
            .and_then(|identity| identity.as_ref().map(|id| *id.x25519_secret.as_bytes()));
        let Some(secret) = secret else {
            return;
        };
        match MessageStore::open(&self.dir.join("store"), &secret) {
            Ok(store) => {
                if let Ok(mut slot) = self.state.message_store.lock() {
                    *slot = Some(store);
                }
            }
            Err(e) => tracing::error!(
                error = %e,
                path = %self.dir.display(),
                "OSL: failed to reopen message store after account I/O"
            ),
        }
    }
}

/// Device transfer: export this account's DATA (contacts, whitelists,
/// group keys, message history) as a single blob, encrypted under a key
/// derived from the recovery-phrase entropy. Returns the blob base64 so
/// the Settings UI can offer it as a download. The same 12-word phrase
/// that recovers the identity on the new device also decrypts this — so
/// the export file is safe to move between devices.
pub fn cmd_osl_export_data(state: &AppState) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    cmd_osl_export_data_with_dir(state, &dir)
}

fn cmd_osl_export_data_with_dir(state: &AppState, dir: &Path) -> Result<String, String> {
    let entropy = require_recovery_entropy(
        state,
        "this account has no recovery phrase, so its data can't be exported \
         (only seed-phrase accounts are transferable — make a fresh account)",
    )?;
    let _store_pause = MessageStorePause::new(state, dir)?;
    let mut files = serde_json::Map::new();
    for rel in OSL_EXPORT_FILES {
        let p = dir.join(rel);
        if let Ok(bytes) = std::fs::read(&p) {
            // The outer export AEAD is the cross-device protection.  Do not
            // carry a source device's password-derived OSL-ENC1 wrapper into
            // it: the destination has a different file key.  Failure to
            // unwrap a protected file means the source is locked/corrupt, so
            // fail the whole export rather than create a deceptive backup.
            let bytes = if crate::main_password::has_enc_magic(&bytes) {
                crate::main_password::maybe_decrypt(&bytes)
                    .map_err(|e| format!("OSL: export: decrypt {rel}: {e}"))?
            } else {
                bytes
            };
            files.insert(
                (*rel).to_string(),
                serde_json::Value::String(STANDARD.encode(&bytes)),
            );
        }
    }
    // Include the RAW identity keys so the export is a COMPLETE,
    // portable account backup: transferable even for legacy random-key
    // accounts (whose phrase can't re-derive keys) and across devices
    // (the on-disk identity.json is device-sealed; these raw bytes get
    // re-sealed locally on import). Encrypted under the phrase like the
    // rest of the bundle.
    let identity_obj = {
        let g = state.identity.lock().expect("identity mutex poisoned");
        g.as_ref().map(|id| {
            serde_json::json!({
                "x25519_secret": STANDARD.encode(id.x25519_secret.as_bytes()),
                "x25519_public": STANDARD.encode(id.x25519_public.as_bytes()),
                "ed25519_secret": STANDARD.encode(id.ed25519_secret.as_bytes()),
                "ed25519_public": STANDARD.encode(id.ed25519_public.as_bytes()),
                "mlkem_secret": STANDARD.encode(id.mlkem_secret_bytes()),
                "mlkem_public": STANDARD.encode(id.mlkem_public_bytes),
                "snowflake": id.discord_snowflake.clone(),
                "entropy": id.recovery_entropy.as_ref().map(|e| STANDARD.encode(e)),
            })
        })
    };
    let bundle = serde_json::json!({ "version": 1, "files": files, "identity": identity_obj });
    let plaintext =
        serde_json::to_vec(&bundle).map_err(|e| format!("OSL: export serialize: {e}"))?;

    let key = export_aead_key(&entropy)?;
    let nonce_bytes = crypto::random::random_bytes(crypto::aead::NONCE_SIZE);
    let mut na = [0u8; crypto::aead::NONCE_SIZE];
    na.copy_from_slice(&nonce_bytes);
    let nonce = crypto::aead::Nonce::from_bytes(na);
    let ct = crypto::aead::seal(&key, &nonce, OSL_EXPORT_MAGIC, &plaintext)
        .map_err(|e| format!("OSL: export encrypt: {e}"))?;

    let mut out = Vec::with_capacity(OSL_EXPORT_MAGIC.len() + crypto::aead::NONCE_SIZE + ct.len());
    out.extend_from_slice(OSL_EXPORT_MAGIC);
    out.extend_from_slice(nonce.as_bytes());
    out.extend_from_slice(&ct);
    Ok(STANDARD.encode(&out))
}

/// Device transfer: restore a full account export onto THIS device
/// using its 12-word phrase. The phrase decrypts the blob; the bundle
/// carries the raw identity keys + all data. We reconstruct the
/// identity (re-sealing locally — the source's on-disk seal was
/// device-bound) and write the data files back. A relaunch loads the
/// restored account. Self-contained: works for legacy and seed-phrase
/// accounts alike, and the local pre-import identity is irrelevant.
pub fn cmd_osl_recover_account_from_export(
    state: &AppState,
    blob_b64: String,
    phrase: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    cmd_osl_recover_account_from_export_with_dir(state, blob_b64, phrase, &dir)
}

fn cmd_osl_recover_account_from_export_with_dir(
    state: &AppState,
    blob_b64: String,
    phrase: String,
    dir: &Path,
) -> Result<(), String> {
    // Phrase -> entropy -> export-decryption key.
    let mnemonic = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, phrase.trim())
        .map_err(|_| "OSL: that isn't a valid 12-word recovery phrase.".to_string())?;
    let ev = mnemonic.to_entropy();
    if ev.len() != 16 {
        return Err("OSL: recovery phrase must be exactly 12 words.".to_string());
    }
    let mut entropy = [0u8; 16];
    entropy.copy_from_slice(&ev);

    let raw = STANDARD
        .decode(blob_b64.trim())
        .map_err(|e| format!("OSL: import: base64 decode: {e}"))?;
    let prefix = OSL_EXPORT_MAGIC.len() + crypto::aead::NONCE_SIZE;
    if raw.len() < prefix || &raw[..OSL_EXPORT_MAGIC.len()] != OSL_EXPORT_MAGIC {
        return Err("OSL: that file isn't an OSL account export.".to_string());
    }
    let mut na = [0u8; crypto::aead::NONCE_SIZE];
    na.copy_from_slice(&raw[OSL_EXPORT_MAGIC.len()..prefix]);
    let nonce = crypto::aead::Nonce::from_bytes(na);
    let key = export_aead_key(&entropy)?;
    let plaintext =
        crypto::aead::open(&key, &nonce, OSL_EXPORT_MAGIC, &raw[prefix..]).map_err(|_| {
            "OSL: couldn't decrypt — the phrase doesn't match this export file, \
             or the file is corrupt."
                .to_string()
        })?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|e| format!("OSL: import parse: {e}"))?;
    if bundle.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("OSL: import: unsupported or missing export version".to_string());
    }

    // Validate every field and decode every file before releasing the SQLite
    // handle or replacing any live state.  This is deliberately strict: a
    // partial/malformed bundle must be a no-op.
    let idobj = bundle
        .get("identity")
        .filter(|v| v.is_object())
        .ok_or_else(|| "OSL: import: identity missing".to_string())?;
    let id = decode_export_identity(idobj, entropy)?;
    let imported_snowflake = id.discord_snowflake.as_deref().unwrap_or_default();
    let active_snowflake = state
        .identity
        .lock()
        .expect("identity mutex poisoned")
        .as_ref()
        .and_then(|current| current.discord_snowflake.as_deref())
        .map(str::to_string);
    if active_snowflake.as_deref() != Some(imported_snowflake) {
        return Err(format!(
            "OSL: import belongs to Discord account {imported_snowflake}, not the currently active account. Switch Discord accounts first; nothing was changed.",
            imported_snowflake = crate::log_id::log_id(imported_snowflake)
        ));
    }
    let files = bundle
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| "OSL: import: files missing".to_string())?;
    let files = decode_export_files(files)?;

    // Stage the complete replacement first.  Renames are atomic per path;
    // malformed data cannot reach a live path because all validation and all
    // staging writes finish before this point.
    std::fs::create_dir_all(dir).map_err(|e| format!("OSL: import: mkdir: {e}"))?;
    let stage = dir.join(format!(
        ".osl-import-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    std::fs::create_dir(&stage).map_err(|e| format!("OSL: import: stage: {e}"))?;
    let stage_result = (|| -> Result<(), String> {
        for (rel, bytes) in &files {
            let p = stage.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("OSL: import: stage mkdir {rel}: {e}"))?;
            }
            std::fs::write(&p, bytes)
                .map_err(|e| format!("OSL: import: stage write {rel}: {e}"))?;
        }
        let sealer = keystore::select_best_sealer();
        keystore::save_identity(&stage.join("identity.json"), &id, sealer.as_ref())
            .map_err(|e| format!("OSL: import: stage identity: {e}"))?;
        Ok(())
    })();
    if let Err(e) = stage_result {
        let _ = std::fs::remove_dir_all(&stage);
        return Err(e);
    }

    let _store_pause = MessageStorePause::new(state, dir)?;
    if let Err(e) = commit_staged_account_import(dir, &stage, &files) {
        // Keep the stage on every commit failure. It may contain the only
        // remaining rollback copy if Windows/AV held a destination file.
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&stage);
    state.install_identity(id);
    Ok(())
}

#[cfg(test)]
mod account_transfer_tests {
    use super::*;
    use tempfile::TempDir;

    static FILE_KEY_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn state_with_entropy(entropy: [u8; 16]) -> AppState {
        let state = AppState::new();
        *state.identity.lock().unwrap() =
            Some(keystore::identity_from_entropy(entropy, "42".into()));
        state
    }

    fn open_export(encoded: &str, entropy: [u8; 16]) -> serde_json::Value {
        let raw = STANDARD.decode(encoded).unwrap();
        let offset = OSL_EXPORT_MAGIC.len() + crypto::aead::NONCE_SIZE;
        let mut nonce = [0; crypto::aead::NONCE_SIZE];
        nonce.copy_from_slice(&raw[OSL_EXPORT_MAGIC.len()..offset]);
        let plaintext = crypto::aead::open(
            &export_aead_key(&entropy).unwrap(),
            &crypto::aead::Nonce::from_bytes(nonce),
            OSL_EXPORT_MAGIC,
            &raw[offset..],
        )
        .unwrap();
        serde_json::from_slice(&plaintext).unwrap()
    }

    #[test]
    fn encrypted_export_is_normalized_to_plaintext_inside_outer_aead() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        let entropy = [7; 16];
        let key = [3; 32];
        std::fs::write(
            dir.path().join("peer_map.json"),
            crate::main_password::encrypt_at_rest(br#"{"peer":"ok"}"#, &key).unwrap(),
        )
        .unwrap();
        crate::main_password::set_file_storage_key(Some(key));
        let encoded =
            cmd_osl_export_data_with_dir(&state_with_entropy(entropy), dir.path()).unwrap();
        crate::main_password::set_file_storage_key(None);
        let bundle = open_export(&encoded, entropy);
        let peer = bundle["files"]["peer_map.json"].as_str().unwrap();
        assert_eq!(STANDARD.decode(peer).unwrap(), br#"{"peer":"ok"}"#);
    }

    #[test]
    fn locked_export_of_encrypted_state_fails() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("peer_map.json"),
            crate::main_password::encrypt_at_rest(b"{}", &[3; 32]).unwrap(),
        )
        .unwrap();
        crate::main_password::set_file_storage_key(None);
        let err =
            cmd_osl_export_data_with_dir(&state_with_entropy([8; 16]), dir.path()).unwrap_err();
        assert!(err.contains("decrypt peer_map.json"), "{err}");
    }

    #[test]
    fn import_reencrypts_state_json_for_destination_key() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        crate::main_password::set_file_storage_key(Some([9; 32]));
        let mut files = serde_json::Map::new();
        files.insert(
            "peer_map.json".into(),
            serde_json::Value::String(STANDARD.encode(b"{}")),
        );
        let decoded = decode_export_files(&files).unwrap();
        crate::main_password::set_file_storage_key(None);
        assert!(crate::main_password::has_enc_magic(&decoded[0].1));
        assert_eq!(
            crate::main_password::decrypt_at_rest(&decoded[0].1, &[9; 32]).unwrap(),
            b"{}"
        );
        crate::main_password::set_file_storage_key(None);
    }

    #[test]
    fn guard_backup_destination_refuses_unencrypted_store_backup() {
        let _guard = FILE_KEY_TEST_LOCK.lock().unwrap();
        crate::main_password::set_file_storage_key(None);
        let dir = TempDir::new().unwrap();
        let stage = dir.path().join("stage");
        std::fs::create_dir_all(stage.join("store")).unwrap();
        std::fs::create_dir_all(dir.path().join("store")).unwrap();
        std::fs::write(
            dir.path().join("store/messages.sqlite"),
            b"live store backup source",
        )
        .unwrap();
        std::fs::write(stage.join("identity.json"), b"new staged identity").unwrap();

        let err = commit_staged_account_import(dir.path(), &stage, &[]).unwrap_err();
        assert!(err.contains("message_store"), "{err}");
        assert!(err.contains("backup_rollback_copies"), "{err}");
        assert!(err.contains("unencrypted destination"), "{err}");
        assert_eq!(
            std::fs::read(dir.path().join("store/messages.sqlite")).unwrap(),
            b"live store backup source"
        );
        assert!(
            !stage.join(".backup/store/messages.sqlite").exists(),
            "refusal must happen before a plaintext Store rollback copy is written"
        );
        assert!(stage.join("identity.json").exists());

        let direct_err = guard_backup_destination("store/messages.sqlite-wal", false).unwrap_err();
        assert!(direct_err.contains("message_store"), "{direct_err}");
        assert!(guard_backup_destination("store/messages.sqlite-shm", true).is_ok());
        assert!(guard_backup_destination("peer_map.json", false).is_ok());
    }

    #[test]
    fn staged_import_replaces_existing_files_and_removes_omitted_stale_state() {
        let dir = TempDir::new().unwrap();
        let stage = dir.path().join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::write(dir.path().join("peer_map.json"), b"old-peer").unwrap();
        std::fs::write(dir.path().join("membership.json"), b"stale-membership").unwrap();
        std::fs::write(dir.path().join("identity.json"), b"old-identity").unwrap();
        std::fs::write(stage.join("peer_map.json"), b"new-peer").unwrap();
        std::fs::write(stage.join("identity.json"), b"new-identity").unwrap();
        let files = vec![("peer_map.json".to_string(), b"new-peer".to_vec())];

        commit_staged_account_import(dir.path(), &stage, &files).unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("peer_map.json")).unwrap(),
            b"new-peer"
        );
        assert_eq!(
            std::fs::read(dir.path().join("identity.json")).unwrap(),
            b"new-identity"
        );
        assert!(!dir.path().join("membership.json").exists());
    }

    #[test]
    fn malformed_import_does_not_overwrite_live_files() {
        let dir = TempDir::new().unwrap();
        let live = dir.path().join("peer_map.json");
        std::fs::write(&live, b"old").unwrap();
        let entropy = [4; 16];
        let bundle = serde_json::json!({"version": 1, "identity": {}, "files": {"peer_map.json": STANDARD.encode(b"{}")}});
        let plaintext = serde_json::to_vec(&bundle).unwrap();
        let nonce = crypto::aead::Nonce::from_bytes([2; crypto::aead::NONCE_SIZE]);
        let ct = crypto::aead::seal(
            &export_aead_key(&entropy).unwrap(),
            &nonce,
            OSL_EXPORT_MAGIC,
            &plaintext,
        )
        .unwrap();
        let mut raw = OSL_EXPORT_MAGIC.to_vec();
        raw.extend_from_slice(nonce.as_bytes());
        raw.extend_from_slice(&ct);
        let phrase = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &entropy)
            .unwrap()
            .to_string();
        assert!(cmd_osl_recover_account_from_export_with_dir(
            &AppState::new(),
            STANDARD.encode(raw),
            phrase,
            dir.path()
        )
        .is_err());
        assert_eq!(std::fs::read(&live).unwrap(), b"old");
    }

    #[test]
    fn legacy_phrase_only_identity_is_not_a_key_match() {
        let dir = TempDir::new().unwrap();
        let mut legacy = keystore::generate_identity("42".into());
        legacy.discord_snowflake = Some("42".into());
        let before = *legacy.ed25519_public.as_bytes();
        let state = AppState::new();
        *state.identity.lock().unwrap() = Some(legacy);
        let phrase = bip39::Mnemonic::from_entropy_in(bip39::Language::English, &[5; 16])
            .unwrap()
            .to_string();
        let err =
            cmd_osl_recover_identity_from_phrase_with_dir(&state, phrase, dir.path()).unwrap_err();
        assert!(err.contains("full account export"), "{err}");
        assert_eq!(
            state
                .identity
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .ed25519_public
                .as_bytes(),
            &before
        );
        assert!(!dir.path().join("identity.json").exists());
    }
}

pub fn cmd_osl_verify_main_password(password: String) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::verify_main_password(&dir, &password)
}

pub fn cmd_osl_verify_recovery_phrase(state: &AppState, phrase: String) -> Result<String, String> {
    let dir = password_dir()?;
    crate::main_password::verify_recovery_phrase(state, &dir, &phrase)
}

pub fn cmd_osl_set_main_password_after_recovery(
    state: &AppState,
    new_password: String,
    token: String,
) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::set_main_password_after_recovery(state, &dir, &new_password, &token)
}

pub fn cmd_osl_lockout_status() -> Result<LockoutStatusDto, String> {
    let dir = password_dir()?;
    Ok(crate::main_password::lockout_status(&dir))
}

// =====================================================================
// Phase 7d-B2: stealth password operations.
// =====================================================================

pub fn cmd_osl_set_stealth_password(
    current_main: String,
    new_stealth: String,
) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::set_stealth_password(&dir, &current_main, &new_stealth)
}

pub fn cmd_osl_remove_stealth_password(current_main: String) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::remove_stealth_password(&dir, &current_main)
}

pub fn cmd_osl_stealth_password_status() -> Result<PasswordStatusDto, String> {
    let dir = password_dir()?;
    Ok(PasswordStatusDto {
        is_set: crate::main_password::stealth_password_status(&dir),
    })
}

// =====================================================================
// Phase 7d-B3: burn password operations.
// =====================================================================

pub fn cmd_osl_set_burn_password(current_main: String, new_burn: String) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::set_burn_password(&dir, &current_main, &new_burn)
}

pub fn cmd_osl_remove_burn_password(current_main: String) -> Result<(), String> {
    let dir = password_dir()?;
    crate::main_password::remove_burn_password(&dir, &current_main)
}

pub fn cmd_osl_burn_password_status() -> Result<PasswordStatusDto, String> {
    let dir = password_dir()?;
    Ok(PasswordStatusDto {
        is_set: crate::main_password::burn_password_status(&dir),
    })
}

// =====================================================================
// Phase 7d-B2/B3: gate-side single-call password verify across the
// three roles. Returns one of "main" | "stealth" | "burn" | "wrong"
// + the same lockout fields as `verify_main_password`. All three
// successful entries reset the shared counter (so an attacker
// observing repeated entries can't distinguish "main" from
// "stealth"/"burn" via counter dynamics).
// =====================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct GateVerifyDto {
    pub result: String,
    pub lockout_seconds_remaining: i64,
    pub attempts_used: u32,
}

pub fn cmd_osl_verify_gate_password(
    state: &AppState,
    password: String,
) -> Result<GateVerifyDto, String> {
    use crate::main_password::GateMatch;
    let dir = password_dir()?;
    // Lockout-window check first (same as verify_main_password).
    let mut lock = crate::main_password::read_lockout_pub(&dir);
    let now = crate::main_password::now_unix_secs_pub();
    if let Some(until) = lock.password_locked_until {
        if now < until {
            return Ok(GateVerifyDto {
                result: "wrong".to_string(),
                lockout_seconds_remaining: until - now,
                attempts_used: lock.password_failed_attempts,
            });
        }
    }
    let marker = crate::main_password::read_marker_pub(&dir)?;
    let outcome = crate::main_password::verify_gate_password_with_marker(&marker, &password)?;
    match outcome {
        GateMatch::Main(file_key) => {
            crate::main_password::set_file_storage_key(Some(file_key));
            // 9-D-FIX2: reload every encrypted-at-rest state file
            // now that `file_storage_key` is in slot. Bootstrap
            // attempted these reads pre-gate with no key, so each
            // file's `maybe_decrypt` errored and AppState ended up
            // seeded with defaults. Without this reload the user's
            // whitelist, burns, sender chains, tour state, and
            // stego-mode pref stay blank for the whole session and
            // the tour replays on every launch.
            //
            // Multi-account: the account-level encrypted files
            // (peer_map, whitelist, sender_key, burned, membership)
            // live in the ACTIVE-account dir (osl_config_dir), NOT the
            // base (`dir` here is the device-level password dir). Pass
            // the account dir or the post-gate reload pulls everything
            // from the base → whitelist + sender keys come back EMPTY
            // every unlock. (app_preferences is device-level and the
            // reload reads it from the base internally.)
            let account_dir = keystore::osl_config_dir().unwrap_or_else(|_| dir.clone());
            match crate::state_reload::reload_encrypted_state_after_unlock(state, &account_dir) {
                Ok(r) => tracing::info!(
                    peer_map_entries = r.peer_map_entries,
                    whitelist_scopes = r.whitelist_scopes,
                    server_defaults_entries = r.server_defaults_entries,
                    burned_scopes_count = r.burned_scopes_count,
                    sender_keys_count = r.sender_keys_count,
                    app_prefs_loaded = r.app_prefs_loaded,
                    errors = ?r.errors,
                    "OSL: state reloaded post-gate"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    "OSL: post-gate state reload failed"
                ),
            }
            // REGISTER-FIX: the boot-time keyserver register runs at
            // cold boot and is skipped whenever no identity was
            // loadable then (the V2 clean-install case, and any boot
            // where the sealed identity could not be read). It is
            // never retried — so a machine that booted without a
            // loadable identity stays absent from /v1/pubkeys and no
            // peer can encrypt to it. This is the post-unlock retry:
            // by the time the main password verifies, bootstrap has
            // already run and (on a relaunch) loaded identity.json,
            // so state.identity is populated here. Idempotent — see
            // `ensure_keyserver_registered`'s upsert contract; it is
            // a no-op if no identity exists yet (first install, where
            // the identity is born later in the Discord-snowflake
            // path, which carries its own hook).
            ensure_keyserver_registered(
                state,
                &resolve_keyserver_base_url(&dir),
                read_keyserver_client_token(&dir),
            );
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "main".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Stealth => {
            // Shared counter reset on any successful entry — see
            // security rationale in the spec (prevents attacker
            // distinguishing main from stealth via counter dynamics).
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "stealth".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Burn => {
            lock.password_failed_attempts = 0;
            lock.password_locked_until = None;
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "burn".to_string(),
                lockout_seconds_remaining: 0,
                attempts_used: 0,
            })
        }
        GateMatch::Wrong => {
            lock.password_failed_attempts = lock.password_failed_attempts.saturating_add(1);
            let secs =
                crate::main_password::password_lockout_secs_pub(lock.password_failed_attempts);
            lock.password_locked_until = if secs > 0 { Some(now + secs) } else { None };
            let _ = crate::main_password::write_lockout_pub(&dir, &lock);
            Ok(GateVerifyDto {
                result: "wrong".to_string(),
                lockout_seconds_remaining: secs,
                attempts_used: lock.password_failed_attempts,
            })
        }
    }
}

/// 7d-B2: hide the OSL config dir + record stealth-active for the
/// session so initialization_script can suppress boot.js injection.
pub fn cmd_osl_stealth_mode_engage(state: &AppState) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    let _ = crate::main_password::stealth_hide_dir(&dir);
    *state
        .stealth_active
        .lock()
        .expect("stealth_active mutex poisoned") = true;
    Ok(())
}

// =====================================================================
// Phase 7d-FIX1: scope burn data destruction + burned-scope ledger.
// =====================================================================

#[derive(Debug, Clone, serde::Serialize)]
pub struct BurnScopeDataDto {
    pub rows_destroyed: usize,
    pub channel_id: String,
}

/// Destroy local message rows for the channel(s) covered by
/// `scope`. Per spec 7d-FIX1 Task 3a + 7d-D Task 2:
///   - DM and server_channel_full scopes resolve to a single
///     channel_id and `DELETE FROM messages WHERE channel_id = ?`.
///   - gc_full (7d-D): scope_id IS the GC channel_id — same
///     single-channel DELETE as DM.
///   - gc_per_user and server_full / server_full_per_user remain
///     NOT implemented in this phase (they'd require either
///     per-sender row filtering or enumerating multiple
///     channel_ids); we return a not-implemented error string
///     so the JS caller can surface it but the rest of the burn
///     flow keeps going.
pub fn cmd_osl_burn_scope_data(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
) -> Result<BurnScopeDataDto, String> {
    let channel_id = match scope_kind.as_str() {
        "dm" => scope_id.clone(),
        "server_channel_full" | "server_channel_per_user" | "server_channel" => {
            if let Some((_, ch)) = scope_id.split_once(':') {
                ch.to_string()
            } else {
                scope_id.clone()
            }
        }
        // 7d-D Task 2: gc_full's scope_id is the GC channel_id.
        // Same single-channel destroy path as DM.
        "gc_full" | "gc" => scope_id.clone(),
        "gc_per_user" => {
            return Err(format!(
                "OSL: burn_scope_data: gc_per_user burn not yet implemented (scope_id={scope_id}) — \
                 deferred to a later cleanup pass, see 7d-D spec",
                scope_id = crate::log_id::log_id(&scope_id)
            ));
        }
        "server_full" | "server_full_per_user" => {
            return Err(format!(
                "OSL: burn_scope_data: server_full burn not yet implemented (scope_id={scope_id}) — \
                 deferred, see 7d-D spec",
                scope_id = crate::log_id::log_id(&scope_id)
            ));
        }
        other => {
            return Err(format!("OSL: burn_scope_data: unknown scope_kind={other}"));
        }
    };
    let rows = if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        store
            .delete_messages_in_channel(&channel_id)
            .map_err(|e| format!("OSL: delete_messages_in_channel: {e}"))?
    } else {
        0
    };
    eprintln!("[OSL][burn] destroyed {rows} rows for channel {channel_id}");
    // 9-B1: drop any in-flight Mode 1 reassembly buffers for this
    // channel so chunked-but-not-yet-complete covers can't surface
    // as plaintext after the burn.
    drop_mode1_reassembly_for_channel(state, &channel_id);
    let _ = server_id;
    Ok(BurnScopeDataDto {
        rows_destroyed: rows,
        channel_id,
    })
}

pub fn cmd_osl_mark_scope_burned(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
    channel_id: Option<String>,
    burned_message_ids: Vec<String>,
) -> Result<(), String> {
    use crate::burned_scopes_file::BurnedScopeEntry;
    let now = now_unix_secs();
    // Beta 1.0: also drop any cached decrypted attachments for the
    // burned messages, so a burned image can't be rehydrated from the
    // local attachment cache after the burn.
    if !burned_message_ids.is_empty() {
        let guard = state
            .message_store
            .lock()
            .expect("message_store mutex poisoned");
        if let Some(store) = guard.as_ref() {
            for mid in &burned_message_ids {
                let _ = store.delete_attachments_for_message(mid);
            }
        }
    }
    let entry = BurnedScopeEntry {
        scope_kind: scope_kind.clone(),
        scope_id: scope_id.clone(),
        server_id,
        channel_id,
        burned_at: now,
        burned_message_ids: burned_message_ids.clone(),
    };
    {
        let mut g = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        if let Some(existing) = g
            .scopes
            .iter_mut()
            .find(|e| e.scope_kind == scope_kind && e.scope_id == scope_id)
        {
            // 9-A1c: repeat burns on the same scope union new IDs
            // into the existing kill list rather than overwriting.
            for id in burned_message_ids {
                if !existing.burned_message_ids.contains(&id) {
                    existing.burned_message_ids.push(id);
                }
            }
        } else {
            g.scopes.push(entry);
        }
        g.version = 1;
    }
    persist_burned_scopes_now(state);
    Ok(())
}

/// Phase 9-B1: drop any in-flight Mode 1 reassembly sessions
/// belonging to the channel `channel_id`. Called from the burn
/// pipeline so a freshly-burned scope's chunked-but-not-yet-complete
/// covers can't unexpectedly resolve to plaintext after the burn.
pub(crate) fn drop_mode1_reassembly_for_channel(state: &AppState, channel_id: &str) {
    let mut bufs = state
        .mode1_reassembly
        .lock()
        .expect("mode1_reassembly mutex poisoned");
    bufs.remove(channel_id);
}

/// 9-A1c: burn kill list lookup. Returns true iff the given
/// `message_id` was recorded under the burn entry that matches
/// `scope`. Comparison uses the scope's `(kind, id)` pair, which
/// is how `BurnedScopeEntry` rows are keyed.
pub(crate) fn is_message_in_burn_kill_list(
    state: &AppState,
    scope: &crate::scope::Scope,
    message_id: &str,
) -> bool {
    let scope_kind = scope_kind_to_str(scope.kind);
    let scope_id = scope.id.as_str();
    let g = state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned");
    g.scopes.iter().any(|e| {
        e.scope_kind == scope_kind
            && e.scope_id == scope_id
            && e.burned_message_ids.iter().any(|m| m == message_id)
    })
}

/// snake_case form of a `ScopeKind` matching the JS-side strings
/// passed into `osl_mark_scope_burned`. Kept local rather than
/// added to the `Scope` impl to avoid expanding the public API
/// surface for a single internal call site.
fn scope_kind_to_str(kind: crate::scope::ScopeKind) -> &'static str {
    match kind {
        crate::scope::ScopeKind::Dm => "dm",
        crate::scope::ScopeKind::Gc => "gc",
        crate::scope::ScopeKind::ServerChannel => "server_channel",
        crate::scope::ScopeKind::ServerFull => "server_full",
    }
}

/// Returns `Ok(true)` if a burned-scopes entry was removed,
/// `Ok(false)` if there was nothing to remove (idempotent no-op).
/// 7d-PIVOT-FIX2: callers use the boolean to emit the
/// `osl:scope_unburned` cross-window event so the JS-side
/// `__oslBurnedScopes` cache stays in sync.
pub fn cmd_osl_unburn_scope(
    state: &AppState,
    scope_kind: String,
    scope_id: String,
) -> Result<bool, String> {
    let removed = {
        let mut g = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        let before = g.scopes.len();
        g.scopes
            .retain(|e| !(e.scope_kind == scope_kind && e.scope_id == scope_id));
        let after = g.scopes.len();
        if after < before {
            g.version = 1;
            true
        } else {
            false
        }
    };
    if removed {
        persist_burned_scopes_now(state);
    }
    Ok(removed)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BurnedScopeDto {
    pub scope_kind: String,
    pub scope_id: String,
    pub server_id: Option<String>,
    pub channel_id: Option<String>,
    pub burned_at: i64,
}

/// Phase 9-A3: boot.js pushes the current channel-member set
/// (gateway-derived) so the v=5 send dispatch can detect
/// membership changes and trigger rotation. Stored in-memory only;
/// the SenderChain's `last_known_members` snapshot is the
/// persistent record.
pub fn cmd_osl_membership_update(
    state: &AppState,
    channel_id: String,
    member_ids: Vec<String>,
) -> Result<(), String> {
    let mut g = state
        .channel_members
        .lock()
        .expect("channel_members mutex poisoned");
    g.insert(channel_id, member_ids);
    Ok(())
}

/// Phase 9-A3: read back the cached members for a channel. Returns
/// an empty vec when boot.js hasn't pushed yet (or the channel is
/// genuinely empty).
pub fn cmd_osl_membership_get(state: &AppState, channel_id: String) -> Result<Vec<String>, String> {
    let g = state
        .channel_members
        .lock()
        .expect("channel_members mutex poisoned");
    Ok(g.get(&channel_id).cloned().unwrap_or_default())
}

/// W2: durable membership accrual. boot.js gateway taps call this
/// with the scope they observed members in. ServerChannel rolls up
/// into the server key (server-header enumeration); Gc records the
/// GC. Dm / ServerFull are no-ops (DM membership is trivial; server-
/// wide accrues from its channels). Persists `membership.json`.
pub fn cmd_osl_note_scope_membership(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    member_ids: Vec<String>,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let changed = {
        let mut m = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        match scope.kind {
            crate::scope::ScopeKind::ServerChannel => match (&scope.server_id, &scope.channel_id) {
                (Some(srv), Some(chan)) => {
                    m.note_server_channel_members(srv, chan, &member_ids);
                    true
                }
                _ => false,
            },
            crate::scope::ScopeKind::Gc => {
                m.note_gc_members(&scope.id, &member_ids);
                true
            }
            // Dm: the peer IS the scope (no accrual needed).
            // ServerFull: accrues via its channels' observations.
            crate::scope::ScopeKind::Dm | crate::scope::ScopeKind::ServerFull => false,
        }
    };
    if changed {
        persist_scope_membership_now(state);
    }
    Ok(())
}

/// W2: one server's whitelist + encryption flags, for the header
/// button + sidebar UI to render tri-state.
#[derive(Debug, Serialize)]
pub struct ServerWhitelistStateDto {
    /// `ServerDefaults.server_header_whitelisted` for the server = the
    /// server-lock GREEN tier (all OSL server members).
    pub server_header: bool,
    /// `ServerDefaults.server_dm_whitelisted` = the server-lock YELLOW
    /// tier (DM-whitelisted peers who are server members). GREEN
    /// outranks YELLOW; both false = GREY (nobody).
    pub server_dm: bool,
    /// `ScopeState.channel_whitelisted` for the queried channel
    /// (false when no channel scope was supplied).
    pub channel: bool,
    /// `ScopeState.encrypt_toggle` for the queried channel.
    pub channel_encrypt: bool,
}

/// W2: read the server-header flag (per server) + the per-channel
/// whitelist/encrypt flags for `channel_scope_input` (a
/// `server_channel` ScopeInput; pass the current channel). Drives
/// the header button + the new sidebar per-channel button.
pub fn cmd_osl_get_server_whitelist_state(
    state: &AppState,
    server_id: String,
    channel_scope_input: Option<crate::scope::ScopeInput>,
) -> Result<ServerWhitelistStateDto, String> {
    let (server_header, server_dm) = {
        let sd = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        sd.get(&server_id)
            .map(|d| (d.server_header_whitelisted, d.server_dm_whitelisted))
            .unwrap_or((false, false))
    };
    let (channel, channel_encrypt) = match channel_scope_input {
        Some(si) => {
            let scope: crate::scope::Scope = si
                .try_into()
                .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
            let ws = state
                .whitelist_state
                .lock()
                .expect("whitelist_state mutex poisoned");
            ws.get(&scope.storage_key())
                .map(|s| (s.channel_whitelisted, s.encrypt_toggle))
                .unwrap_or((false, false))
        }
        None => (false, false),
    };
    Ok(ServerWhitelistStateDto {
        server_header,
        server_dm,
        channel,
        channel_encrypt,
    })
}

/// Server-lock tri-state setter. `state` is "grey" | "yellow" |
/// "green". GREEN = encrypt to all OSL server members; YELLOW =
/// encrypt to DM-whitelisted peers who are server members; GREY =
/// nobody (self-only). The header button cycles grey→yellow→green and
/// press-and-hold resets to grey. ON (yellow/green) also flips
/// `encrypt_by_default` so the channels actually encrypt; GREY leaves
/// `encrypt_by_default` as the user set it (clearing the lock narrows
/// recipients, it doesn't silently stop encrypting).
pub fn cmd_osl_set_server_lock(
    state: &AppState,
    server_id: String,
    lock_state: String,
) -> Result<(), String> {
    let (green, yellow) = match lock_state.as_str() {
        "green" => (true, false),
        "yellow" => (false, true),
        "grey" | "gray" | "off" => (false, false),
        other => return Err(format!("OSL: set_server_lock: bad state '{other}'")),
    };
    {
        let mut sd = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let entry = sd.entry(server_id.clone()).or_default();
        entry.server_header_whitelisted = green;
        entry.server_dm_whitelisted = yellow;
        if green || yellow {
            entry.encrypt_by_default = true;
        }
    }
    persist_whitelist_state_now(state);
    Ok(())
}

/// W2: the server-header whitelist button. When turned ON it also
/// enables encryption server-wide (confirmed decision #2): sets
/// `encrypt_by_default` so existing+future channels encrypt, and the
/// caller (boot.js) additionally flips the visible channel's
/// encrypt_toggle via the existing scope-encrypt command. Turning it
/// OFF clears only the whitelist flag (encryption is left as the
/// user set it — disabling whitelist shouldn't silently also stop
/// encrypting and leak nothing, it just narrows recipients).
pub fn cmd_osl_set_server_header_whitelist(
    state: &AppState,
    server_id: String,
    on: bool,
) -> Result<(), String> {
    {
        let mut sd = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let entry = sd.entry(server_id.clone()).or_default();
        entry.server_header_whitelisted = on;
        if on {
            entry.encrypt_by_default = true;
        }
    }
    persist_whitelist_state_now(state);
    Ok(())
}

/// W2 + GC follow-up: the per-channel sidebar / GC-header whitelist
/// button. Sets `ScopeState.channel_whitelisted` for a `server_channel`
/// OR a `gc` scope (both use the same scope-flag + dynamic-membership
/// model). ON also flips `encrypt_toggle` so messages actually
/// encrypt (decision #2). OFF clears only the whitelist flag.
pub fn cmd_osl_set_channel_whitelist(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
    on: bool,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    if !matches!(
        scope.kind,
        crate::scope::ScopeKind::ServerChannel | crate::scope::ScopeKind::Gc
    ) {
        return Err("OSL: set_channel_whitelist requires a server_channel or gc scope".to_string());
    }
    {
        let mut ws = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let entry = ws.entry(scope.storage_key()).or_default();
        entry.channel_whitelisted = on;
        if on {
            entry.encrypt_toggle = true;
            entry.auto_enabled = true;
        }
    }
    persist_whitelist_state_now(state);
    Ok(())
}

// ---- Phase 9-C2: friend list + guild list (ephemeral gateway snapshots) ----

/// Guild metadata shipped from boot.js gateway tap to the settings
/// window's Bulk Whitelist modal. Member list may be partial on large
/// guilds — Discord only ships ~100 online members at GUILD_CREATE.
///
/// 9-C3 added `channel_ids` so the Server-Defaults "apply to existing
/// channels" flow can iterate the full channel inventory of a guild
/// without a second round-trip.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct GuildDto {
    pub id: String,
    pub name: String,
    pub member_ids: Vec<String>,
    #[serde(default)]
    pub channel_ids: Vec<String>,
}

/// 9-C2: boot.js pushes the user's friend-ids snapshot here on
/// each gateway READY. Ephemeral — not persisted; repopulated on
/// reconnect. Read via [`cmd_osl_get_friend_ids`].
pub fn cmd_osl_set_friend_ids(state: &AppState, ids: Vec<String>) -> Result<(), String> {
    let mut g = state.friend_ids.lock().expect("friend_ids mutex poisoned");
    *g = ids;
    Ok(())
}

pub fn cmd_osl_get_friend_ids(state: &AppState) -> Result<Vec<String>, String> {
    let g = state.friend_ids.lock().expect("friend_ids mutex poisoned");
    Ok(g.clone())
}

/// Outcome for [`cmd_osl_decline_or_revoke_friend_request`].
///
/// Deliberately carries no peer id, account id, storage key, handle or
/// credential. Callers already know which request they acted on; the IPC
/// response only says whether local authority was removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FriendRequestDecision {
    DeclinedPending,
    RevokedAcceptedGrant,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequestDecisionResult {
    pub decision: FriendRequestDecision,
    pub revoked_grant: bool,
}

/// Decline a pending friend request or revoke a previously accepted one.
///
/// This command never treats absence as permission. If there is no existing
/// scoped grant for `peer_discord_id` and `scope_input`, the operation is a
/// closed no-op decline: no peer_map entry is created, no whitelist is written,
/// and no burn marker is inferred. If an accepted scoped grant exists, the
/// command removes exactly that grant through the same local unwhitelist path
/// used by the rest of the whitelist surface.
pub fn cmd_osl_decline_or_revoke_friend_request(
    state: &AppState,
    peer_discord_id: String,
    scope_input: crate::scope::ScopeInput,
    revoke_broadened: bool,
) -> Result<FriendRequestDecisionResult, String> {
    if peer_discord_id.trim().is_empty() {
        return Err("OSL: friend request peer is missing".to_string());
    }
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let scope_binds_peer = scope.kind != crate::scope::ScopeKind::Dm || scope.id == peer_discord_id;
    if !scope_binds_peer {
        return Ok(FriendRequestDecisionResult {
            decision: FriendRequestDecision::DeclinedPending,
            revoked_grant: false,
        });
    }

    let accepted_grant_exists = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm_guard
            .get(&peer_discord_id)
            .map(|pe| {
                pe.outgoing_whitelists
                    .iter()
                    .any(|w| whitelist_entry_matches(w, &scope))
            })
            .unwrap_or(false)
    };

    if !accepted_grant_exists {
        return Ok(FriendRequestDecisionResult {
            decision: FriendRequestDecision::DeclinedPending,
            revoked_grant: false,
        });
    }

    local_unwhitelist_apply(
        state,
        peer_discord_id,
        crate::scope::ScopeInput::from(&scope),
        revoke_broadened,
        /* wipe_local_decrypt */ false,
    )?;

    Ok(FriendRequestDecisionResult {
        decision: FriendRequestDecision::RevokedAcceptedGrant,
        revoked_grant: true,
    })
}

/// 9-C2: boot.js pushes the user's guild-list snapshot here on
/// each GUILD_CREATE. Ephemeral. Read via [`cmd_osl_get_guild_list`].
pub fn cmd_osl_set_guild_list(state: &AppState, guilds: Vec<GuildDto>) -> Result<(), String> {
    let mut g = state.guild_list.lock().expect("guild_list mutex poisoned");
    *g = guilds;
    Ok(())
}

pub fn cmd_osl_get_guild_list(state: &AppState) -> Result<Vec<GuildDto>, String> {
    let g = state.guild_list.lock().expect("guild_list mutex poisoned");
    Ok(g.clone())
}

/// 9-C2: bulk-whitelist N peers under DM scope (one DM scope per
/// peer; each peer's DM scope flips encrypt_toggle=true alongside
/// adding the Dm whitelist entry). Mirrors `cmd_osl_bulk_set_whitelist`
/// in shape but iterates one scope per peer rather than one scope
/// for all peers — DM scopes are inherently per-peer-keyed.
///
/// Single peer_map + whitelist_state persistence at end.
/// Returns the count of peers whose `outgoing_whitelists` was
/// actually mutated (skips no-ops where the DM entry was already
/// present).
pub fn cmd_osl_bulk_set_dm_whitelist(
    state: &AppState,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    let enabled_at_iso = format_iso8601_secs(now_unix_secs()).unwrap_or_else(|| "?".to_string());
    let mut affected = 0usize;
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let mut ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        for did in &member_dids {
            let scope = crate::scope::Scope::dm(did);
            let pe = pm_guard.entry(did.clone()).or_default();
            if pe.discord_id.is_none() {
                pe.discord_id = Some(did.clone());
            }
            let already = pe
                .outgoing_whitelists
                .iter()
                .any(|w| matches!(w, crate::peer_map::WhitelistEntry::Dm { .. }));
            if !already {
                pe.outgoing_whitelists
                    .push(crate::peer_map::WhitelistEntry::Dm {
                        broadened: false,
                        enabled_at: Some(enabled_at_iso.clone()),
                    });
                pe.burned_scopes.retain(|b| !burn_matches_scope(b, &scope));
                affected += 1;
            }
            let entry = ws_guard.entry(scope.storage_key()).or_default();
            entry.encrypt_toggle = true;
            entry.auto_enabled = true;
        }
    }
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);
    Ok(affected)
}

// ---- Phase 9-C3: server-wide channel-encryption defaults ----

/// DTO mirroring one (server_id → ServerDefaults) entry for the
/// settings + sidebar UIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ServerDefaultDto {
    pub server_id: String,
    pub encrypt_by_default: bool,
}

/// 9-C3: write the per-server "encrypt new channels by default" flag.
/// Persists to disk via the existing whitelist_state.json envelope.
pub fn cmd_osl_set_server_default(
    state: &AppState,
    server_id: String,
    encrypt_by_default: bool,
) -> Result<(), String> {
    if server_id.is_empty() {
        return Err("OSL: server_id is empty".to_string());
    }
    {
        let mut sd = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        sd.entry(server_id.clone()).or_default().encrypt_by_default = encrypt_by_default;
    }
    persist_whitelist_state_now(state);
    Ok(())
}

/// 9-C3: read all server-default entries, sorted by server_id for
/// deterministic UI rendering.
pub fn cmd_osl_get_server_defaults(state: &AppState) -> Result<Vec<ServerDefaultDto>, String> {
    let sd = state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned");
    let mut out: Vec<ServerDefaultDto> = sd
        .iter()
        .map(|(server_id, v)| ServerDefaultDto {
            server_id: server_id.clone(),
            encrypt_by_default: v.encrypt_by_default,
        })
        .collect();
    out.sort_by(|a, b| a.server_id.cmp(&b.server_id));
    Ok(out)
}

/// 9-C3: retroactively flip `ScopeState.encrypt_toggle = true` for
/// every existing channel in `server_id`, drawing the channel
/// inventory from `state.guild_list`. Returns the count of channels
/// whose ScopeState was mutated (channels already on stay no-op).
/// Single persist at end.
pub fn cmd_osl_apply_server_default_to_existing_channels(
    state: &AppState,
    server_id: String,
) -> Result<usize, String> {
    if server_id.is_empty() {
        return Err("OSL: server_id is empty".to_string());
    }
    let channel_ids: Vec<String> = {
        let gl = state.guild_list.lock().expect("guild_list mutex poisoned");
        gl.iter()
            .find(|g| g.id == server_id)
            .map(|g| g.channel_ids.clone())
            .unwrap_or_default()
    };
    if channel_ids.is_empty() {
        return Ok(0);
    }
    let mut affected = 0usize;
    {
        let mut ws = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        for ch_id in &channel_ids {
            let scope = crate::scope::Scope::server_channel(&server_id, ch_id);
            let entry = ws.entry(scope.storage_key()).or_default();
            if !entry.encrypt_toggle {
                entry.encrypt_toggle = true;
                entry.auto_enabled = true;
                affected += 1;
            }
        }
    }
    persist_whitelist_state_now(state);
    Ok(affected)
}

// ---- Phase 9-B1: app preferences ----

/// DTO mirroring [`crate::app_preferences::AppPreferences`] for the JS bridge.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppPreferencesDto {
    pub stego_mode: crate::app_preferences::StegoMode,
}

pub fn cmd_osl_get_app_preferences(state: &AppState) -> Result<AppPreferencesDto, String> {
    let g = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned");
    Ok(AppPreferencesDto {
        stego_mode: g.stego_mode,
    })
}

pub fn cmd_osl_set_app_preferences(
    state: &AppState,
    dto: AppPreferencesDto,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.stego_mode = dto.stego_mode;
    }
    if let Some(dir) = config_dir {
        let g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        let path = dir.join("app_preferences.json");
        crate::app_preferences::write_app_preferences(&path, &g)?;
    }
    Ok(())
}

// ---- G3.3: auto-updater channel ----
//
// Channel persists in the SAME app_preferences.json as every other
// client setting (reuses the existing mechanism — no new persistence
// layer). These are dedicated get/set commands rather than going
// through `AppPreferencesDto` because that DTO's set path overwrites
// the whole struct; a focused mutation here can't clobber stego_mode
// / tour state, and the stego settings page can't clobber the channel.
//
// SECURITY NOTE: channel is a UX affordance, NOT a security boundary
// (see `app_preferences::UpdateChannel`). Free users forcing Beta
// only get a slightly-newer build; real paid features are gated
// elsewhere. Don't add server-side channel enforcement.

pub fn cmd_osl_get_update_channel(
    state: &AppState,
) -> Result<crate::app_preferences::UpdateChannel, String> {
    let g = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned");
    Ok(g.update_channel)
}

pub fn cmd_osl_set_update_channel(
    state: &AppState,
    channel: crate::app_preferences::UpdateChannel,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.update_channel = channel;
    }
    if let Some(dir) = config_dir {
        let g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        let path = dir.join("app_preferences.json");
        crate::app_preferences::write_app_preferences(&path, &g)?;
    }
    Ok(())
}

// ---- Phase 9-D: onboarding tour + VPN warning ----

/// DTO mirroring [`crate::app_preferences::TourState`]. One
/// round-trip lets boot.js + settings query the onboarding state.
/// W4 removed the VPN-warning suppression flag that used to ride
/// here alongside the tour fields.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct TourStateDto {
    pub completed: bool,
    pub skipped: bool,
    pub last_slide: u8,
}

pub fn cmd_osl_tour_get_state(state: &AppState) -> Result<TourStateDto, String> {
    // Robust onboarding guard: if a main password already exists, the
    // user has clearly been through setup before — report the tour as
    // completed so it never re-runs, regardless of whether the tour
    // flag itself persisted. The password marker is device-level (base)
    // and reliable; the tour flag has been flaky across the
    // multi-account dir changes. (Setting a password is part of the
    // tour, so any onboarded user has one.)
    if let Ok(dir) = keystore::osl_base_dir() {
        if crate::main_password::marker_exists(&dir) {
            return Ok(TourStateDto {
                completed: true,
                skipped: false,
                last_slide: 0,
            });
        }
    }
    let g = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned");
    Ok(TourStateDto {
        completed: g.tour.completed,
        skipped: g.tour.skipped,
        last_slide: g.tour.last_slide,
    })
}

fn persist_app_preferences_now(state: &AppState, config_dir: Option<std::path::PathBuf>) {
    // Probe-2 Rust Bug 7: previously this returned silently when
    // `config_dir` was None. Every boot.js caller of cmd_osl_tour_*
    // omits config_dir (the IPC argument doesn't exist on the JS
    // side), so the tour state mutations never actually persisted
    // — the user re-saw the tour intro every launch.
    //
    // Multi-account fix: app_preferences (stego mode, tour state,
    // update channel) is DEVICE-level and run_autostart READS it from
    // osl_base_dir(); persisting to osl_config_dir() (the active-account
    // subdir) wrote it where the next launch never reads → the tour +
    // password setup re-ran every launch. Default to osl_base_dir() so
    // read and write agree. An explicit `config_dir` (test path) wins.
    let dir = match config_dir {
        Some(d) => d,
        None => match keystore::osl_base_dir() {
            Ok(d) => d,
            Err(e) => {
                record_persist_error(state, "app_preferences.json", e);
                return;
            }
        },
    };
    let g = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned");
    let path = dir.join("app_preferences.json");
    if let Err(e) = crate::app_preferences::write_app_preferences(&path, &g) {
        drop(g);
        record_persist_error(state, "app_preferences.json", e);
    }
}

pub fn cmd_osl_tour_advance(
    state: &AppState,
    slide: u8,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.tour.last_slide = slide;
    }
    persist_app_preferences_now(state, config_dir);
    Ok(())
}

pub fn cmd_osl_tour_complete(
    state: &AppState,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.tour.completed = true;
        g.tour.last_slide = 9;
    }
    persist_app_preferences_now(state, config_dir);
    Ok(())
}

pub fn cmd_osl_tour_skip(
    state: &AppState,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.tour.skipped = true;
        g.tour.completed = true;
    }
    persist_app_preferences_now(state, config_dir);
    Ok(())
}

pub fn cmd_osl_tour_reset(
    state: &AppState,
    config_dir: Option<std::path::PathBuf>,
) -> Result<(), String> {
    {
        let mut g = state
            .app_preferences
            .lock()
            .expect("app_preferences mutex poisoned");
        g.version = crate::app_preferences::APP_PREFERENCES_VERSION;
        g.tour = crate::app_preferences::TourState::default();
    }
    persist_app_preferences_now(state, config_dir);
    Ok(())
}

// W4: cmd_osl_vpn_warning_dismiss_forever / cmd_osl_vpn_warning_reset
// removed with the rest of the VPN feature (broken heuristic +
// IP-leaking external call; see project memory). The Tauri wrappers,
// boot.js installer, settings row, and ACL entries went too.

pub fn cmd_osl_list_burned_scopes(state: &AppState) -> Result<Vec<BurnedScopeDto>, String> {
    let g = state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned");
    Ok(g.scopes
        .iter()
        .map(|e| BurnedScopeDto {
            scope_kind: e.scope_kind.clone(),
            scope_id: e.scope_id.clone(),
            server_id: e.server_id.clone(),
            channel_id: e.channel_id.clone(),
            burned_at: e.burned_at,
        })
        .collect())
}

fn persist_burned_scopes_now(state: &AppState) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            record_persist_error(state, "burned_scopes dir resolve", e);
            return;
        }
    };
    let path = dir.join("burned_scopes.json");
    let g = state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned");
    if let Err(e) = crate::burned_scopes_file::write_burned_scopes(&path, &g) {
        drop(g);
        record_persist_error(state, "burned_scopes.json", e);
    }
}

/// 7d-B3: wipe every OSL file. Also clears in-memory AppState so the
/// current session doesn't surface previously-decrypted state.
///
/// SECURITY FORWARD-FIX (post-a4dfc44): routes through
/// [`crate::fresh_start::cmd_osl_fresh_start`] so a pre-signed Case-C
/// rotation proof is minted from the OLD identity while it still
/// exists in memory. Without this routing, the burn destroys the old
/// Ed25519 secret with NO proof minted -> keyserver Case-C requires a
/// signature from the dead key -> register 403 -> permanent
/// not-a-recipient. The `a4dfc44` fix only ever ran from
/// `cmd_osl_fresh_start`, which had no Tauri binding; the user-facing
/// burn went through this function and was still re-bricking.
///
/// Wipe coverage: `fresh_start` removes `identity.json`,
/// `peer_map.json`, `channels.json`, `whitelist_state.json`,
/// `pending_invitations.json`, and the entire `store/` dir; we
/// inline-wipe the burn-only extras (password marker, lockout,
/// burned-scopes ledger, app preferences, sender-key state, scope
/// membership) so a burn truly leaves no file sealed by the old
/// identity/password. `pending_rotation.json` is deliberately omitted
/// from the wipe list — it must survive so the next register can
/// present it (matching `fresh_start.rs`).
///
/// AppState clear (post-probe-2): every in-memory field that could
/// surface pre-burn data is reset. The previous version only cleared
/// 5 of the ~18 fields, letting sender-key chains, scope membership,
/// server defaults, channel members, key-change alerts, friend ids,
/// guild list, recovery guard, sender-pubkey cache, registration
/// alert, recovery token, mode-1 reassembly buffers, and persist-error
/// state survive until restart. `license_state` is intentionally
/// preserved (license is independent of identity); `stealth_active`
/// is intentionally preserved (a burn under stealth coercion must
/// keep the vanilla-Discord facade for the rest of the session).
pub fn cmd_osl_burn_engage(state: &AppState) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;

    // Sign the keyserver unregister request INSIDE the lock guard
    // (Identity isn't Clone — but the resulting signature bytes are
    // plain Vec<u8>, so we capture (user_id, sig_b64, timestamp_ms)
    // and drop the lock before the blocking HTTP call). Must happen
    // with the OLD ed25519_secret, which is destroyed once
    // `fresh_start` overwrites identity.json.
    let unregister_request: Option<(String, String, i64)> = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        match guard.as_ref() {
            Some(id) => {
                let timestamp_ms: i64 = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let sig = keystore::sign_unregister(id, timestamp_ms);
                use base64::engine::general_purpose::STANDARD as B64;
                use base64::Engine as _;
                let sig_b64 = B64.encode(sig.as_bytes());
                Some((id.user_id.clone(), sig_b64, timestamp_ms))
            }
            None => None,
        }
    };
    let preserved_user_id = unregister_request
        .as_ref()
        .map(|(uid, _, _)| uid.clone())
        .unwrap_or_default();

    // Tell the keyserver to delete this user_id's row BEFORE we wipe
    // local identity. Without this, the next register hits Case C
    // ("different security key") and rejects forever. Best-effort:
    // network failure logs + continues, since the burn still has
    // local value (data wiped on this device) and the user can
    // retry the keyserver part later.
    if let Some((user_id, sig_b64, timestamp_ms)) = unregister_request {
        let base_url = resolve_keyserver_base_url(&dir);
        let client_token = read_keyserver_client_token(&dir);
        match keystore::client::KeyServerClient::new(base_url) {
            Ok(c) => {
                let client = c.with_client_token(client_token);
                match client.unregister_signed(&user_id, &sig_b64, timestamp_ms) {
                    Ok(()) => {
                        tracing::info!(
                            user_id = %crate::log_id::log_id(&user_id),
                            "OSL: burn_engage: keyserver unregister succeeded"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            user_id = %crate::log_id::log_id(&user_id),
                            error = %e,
                            "OSL: burn_engage: keyserver unregister failed; \
                             local wipe still proceeds. If re-registration \
                             fails post-burn, retry unregister manually."
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "OSL: burn_engage: KeyServerClient::new failed; \
                     skipping keyserver unregister (local wipe still proceeds)"
                );
            }
        }
    }

    cmd_osl_burn_engage_finish(state, &dir, preserved_user_id)
}

fn cmd_osl_burn_engage_finish(
    state: &AppState,
    dir: &std::path::Path,
    preserved_user_id: String,
) -> Result<(), String> {
    // Release the SQLite connection BEFORE fresh_start tries to
    // delete messages.sqlite. Without this, Windows returns
    // ERROR_SHARING_VIOLATION (os error 32) on every burn because
    // `MessageStore` is still open inside `state.message_store`.
    // Dropping the Option's contents closes the connection (rusqlite
    // Connection::drop releases the file handle + WAL/SHM siblings).
    {
        let taken = state
            .message_store
            .lock()
            .expect("message_store mutex poisoned")
            .take();
        drop(taken);
    }

    // Route through the canonical fresh-start path so the pre-signed
    // Case-C rotation proof is minted+persisted while the old Ed25519
    // secret still exists in memory. This is the WHOLE POINT of the
    // a4dfc44 forward fix; without this routing it never ran.
    let new_identity = crate::fresh_start::cmd_osl_fresh_start(dir, preserved_user_id)
        .map_err(|e| format!("OSL: burn fresh-start failed: {e}"))?;

    // Burn-only additional wipes that `fresh_start` does not cover.
    // Deliberately do NOT call `burn_wipe_all` here — it would re-wipe
    // `identity.json` (the new one `fresh_start` just saved) and would
    // also wipe `pending_rotation.json` if it gets added to that list.
    for name in [
        "password_marker.json",
        "lockout_state.json",
        "burned_scopes.json",
        "app_preferences.json",
        "sender_key_state.json",
        // Probe-2 Rust Bug 3: membership.json was leaking across burns
        // (scope-membership accrual survived intact and fed the new
        // identity's recipient resolution). Wipe it explicitly.
        "membership.json",
    ] {
        let path = dir.join(name);
        if path.exists() {
            // Probe-5 F2 fix: surface wipe failures instead of
            // silently swallowing. Previously `let _ = remove_file`
            // dropped any EBUSY / EACCES (Windows AV scanner holding
            // the file, etc.); the user thought the burn succeeded
            // but stale `sender_key_state.json` (containing the OLD
            // identity's sender chains) survived on disk and would
            // be reloaded post-gate at the next launch, defeating
            // the "no pre-burn group state survives" guarantee.
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::error!(
                    file = name,
                    path = %path.display(),
                    error = %e,
                    "OSL: burn_engage: file wipe failed -- stale state \
                     may survive into the next session"
                );
                record_persist_error(state, name, e);
            }
        }
    }

    crate::main_password::set_file_storage_key(None);

    // Drop in-memory state. Every mutex below is reset so any code that
    // queries state between this function returning and the webview
    // navigating away sees a fully-zeroed session — no pre-burn key
    // material, recipient sets, or membership accrual remain visible.
    state.install_identity(new_identity);
    *state.keyserver.lock().expect("keyserver mutex poisoned") = None;
    *state
        .registration_alert
        .lock()
        .expect("registration_alert mutex poisoned") = None;
    state
        .key_change_alerts
        .lock()
        .expect("key_change_alerts mutex poisoned")
        .clear();
    state.sender_pubkey_cache.clear();
    state
        .peer_map
        .lock()
        .expect("peer_map mutex poisoned")
        .clear();
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") = None;
    state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned")
        .clear();
    *state
        .recovery_token
        .lock()
        .expect("recovery_token mutex poisoned") = None;
    *state
        .burned_scopes
        .lock()
        .expect("burned_scopes mutex poisoned") =
        crate::burned_scopes_file::BurnedScopesFile::default();
    *state
        .sender_key_state
        .lock()
        .expect("sender_key_state mutex poisoned") =
        crate::sender_key_state::SenderKeyStateFile::default();
    state
        .channel_members
        .lock()
        .expect("channel_members mutex poisoned")
        .clear();
    *state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned") =
        crate::app_preferences::AppPreferences::default();
    state
        .mode1_reassembly
        .lock()
        .expect("mode1_reassembly mutex poisoned")
        .clear();
    state
        .friend_ids
        .lock()
        .expect("friend_ids mutex poisoned")
        .clear();
    state
        .guild_list
        .lock()
        .expect("guild_list mutex poisoned")
        .clear();
    state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned")
        .clear();
    *state
        .last_persist_error
        .lock()
        .expect("last_persist_error mutex poisoned") = None;
    *state
        .recovery_guard
        .lock()
        .expect("recovery_guard mutex poisoned") = crate::recovery::RecoveryGuard::default();
    *state
        .scope_membership
        .lock()
        .expect("scope_membership mutex poisoned") = crate::membership::ScopeMembership::default();
    Ok(())
}

// =====================================================================
// Phase F0: deep-link smoke test
//
// Pure URL parser for the `osl://...` scheme registered by
// tauri-plugin-deep-link. Lives here (rather than in `src-tauri/`)
// so it can be unit-tested without spinning up a Tauri runtime —
// the `ipc` crate intentionally has no Tauri dep, per the design
// note at the top of `lib.rs`.
//
// F0 scope: prove the parser handles every shape the smoke-test
// matrix throws at it (URLs with token, without token, multiple
// query params, malformed input). F2 replaces this with the real
// `cmd_osl_redeem_unlock` that validates tokens against the
// keyserver and resets the foreground-time ad timer.
//
// Note: this is NOT a full URI parser. We don't need percent-
// decoding, fragment handling, or host validation for F0 — the
// only URLs we'll ever see are `osl://<path>?token=<opaque>`.
// F2 may swap this for `url::Url` if more robustness is needed,
// but a hand-rolled split keeps the F0 dep footprint at zero.
// =====================================================================

/// Structured result of parsing an `osl://...` URL. Returned to JS
/// by `cmd_osl_test_deep_link` so boot.js can `console.log` the
/// fields independently of the Rust-side `tracing` output.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OslTestDeepLinkResponse {
    /// Scheme portion (`"osl"` for legal inputs).
    pub scheme: String,
    /// Path portion (everything between `://` and the first `?`, if any).
    /// For `osl://test?token=ABC`, path is `"test"`.
    pub path: String,
    /// Value of the `token` query parameter if present. `None` if
    /// the URL has no query string, or has a query string without
    /// a `token` key.
    pub token: Option<String>,
    /// Full URL as received, for boot.js console-log fidelity.
    pub url: String,
}

/// Phase F0 smoke-test command: parse an osl:// URL and return the
/// scheme/path/token. Logs to the Rust console at INFO level so
/// the manual verification matrix can confirm Rust-side reception
/// works independently of the JS event channel.
pub fn cmd_osl_test_deep_link(url: String) -> Result<OslTestDeepLinkResponse, String> {
    tracing::info!(
        target: "osl::deep_link",
        url = %url,
        "[OSL deep-link] received"
    );

    let (scheme, path, token) = parse_osl_url(&url)?;

    tracing::info!(
        target: "osl::deep_link",
        token = ?token,
        path = %path,
        "[OSL deep-link] parsed token"
    );

    Ok(OslTestDeepLinkResponse {
        scheme,
        path,
        token,
        url,
    })
}

/// Split `osl://<path>?<query>` into (scheme, path, token).
/// Token is `Some(_)` iff the query contains a `token=...` pair.
fn parse_osl_url(url: &str) -> Result<(String, String, Option<String>), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("invalid URL: missing scheme separator in {url:?}"))?;

    if scheme.is_empty() {
        return Err(format!("invalid URL: empty scheme in {url:?}"));
    }

    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };

    let token = query.and_then(|q| {
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            if k == "token" {
                Some(v.to_string())
            } else {
                None
            }
        })
    });

    Ok((scheme.to_string(), path.to_string(), token))
}

#[cfg(test)]
mod test_deep_link_parser {
    use super::*;

    #[test]
    fn parses_url_with_token() {
        let result = cmd_osl_test_deep_link("osl://test?token=ABC123".to_string()).unwrap();
        assert_eq!(result.scheme, "osl");
        assert_eq!(result.path, "test");
        assert_eq!(result.token.as_deref(), Some("ABC123"));
        assert_eq!(result.url, "osl://test?token=ABC123");
    }

    #[test]
    fn parses_url_without_token() {
        let result = cmd_osl_test_deep_link("osl://invalid".to_string()).unwrap();
        assert_eq!(result.scheme, "osl");
        assert_eq!(result.path, "invalid");
        assert_eq!(result.token, None);
    }

    #[test]
    fn parses_url_with_query_but_no_token() {
        let result = cmd_osl_test_deep_link("osl://test?foo=bar".to_string()).unwrap();
        assert_eq!(result.token, None);
    }

    #[test]
    fn parses_url_with_multiple_query_params_picks_token() {
        let result =
            cmd_osl_test_deep_link("osl://unlock?foo=bar&token=DEF&baz=qux".to_string()).unwrap();
        assert_eq!(result.token.as_deref(), Some("DEF"));
    }

    #[test]
    fn parses_token_in_first_position() {
        let result = cmd_osl_test_deep_link("osl://unlock?token=XYZ&other=1".to_string()).unwrap();
        assert_eq!(result.token.as_deref(), Some("XYZ"));
    }

    #[test]
    fn empty_token_value_is_still_some_empty() {
        let result = cmd_osl_test_deep_link("osl://test?token=".to_string()).unwrap();
        assert_eq!(result.token.as_deref(), Some(""));
    }

    #[test]
    fn rejects_url_without_scheme_separator() {
        let result = cmd_osl_test_deep_link("not-a-url".to_string());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("missing scheme separator"), "msg: {msg}");
    }

    #[test]
    fn rejects_empty_scheme() {
        let result = cmd_osl_test_deep_link("://test?token=ABC".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn round_trips_full_url_in_response() {
        let input = "osl://complex?token=AAA&other=BBB".to_string();
        let result = cmd_osl_test_deep_link(input.clone()).unwrap();
        assert_eq!(result.url, input);
    }
}

// =====================================================================
// G3.1: update-check command surface.
//
// `crates/ipc` deliberately carries no Tauri dependency (see lib.rs
// docs — keeps these tests portable, no gtk/webkit2gtk tree). The
// actual `tauri-plugin-updater` `check()` call therefore lives in
// the `#[tauri::command] osl_check_for_updates` wrapper in
// `src-tauri/src/main.rs`; that wrapper extracts primitives from the
// plugin's `Update` and feeds them here. This pure mapper owns the
// "which of the three JS-facing states" decision so it unit-tests
// without a webview runtime (same split as `cmd_osl_test_deep_link`).
//
// G3.1 is check-only: no download, no install, no signature check
// (G3.2), no UI / channel selection (G3.3).
// =====================================================================

/// Primitive view of a `tauri-plugin-updater` `Update`, extracted by
/// the Tauri wrapper so this crate stays Tauri-free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Version offered by the manifest (e.g. `"0.0.2"`).
    pub version: String,
    /// Release notes from the manifest, if any.
    pub notes: Option<String>,
    /// Installer URL from the manifest's platform entry.
    pub url: String,
}

/// JS-facing result of an update check. `status` is the discriminant
/// so the G3.3 UI can `switch` on it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateCheckResult {
    /// Running version is current — nothing to do.
    UpToDate { current: String },
    /// A newer build is available (not downloaded/installed in G3.1).
    UpdateAvailable {
        current: String,
        next: String,
        notes: String,
        url: String,
    },
    /// The check itself failed (network, manifest, plugin, etc.).
    Error { message: String },
}

/// Pure mapper: turn a `tauri-plugin-updater` check outcome into the
/// JS-facing [`UpdateCheckResult`].
///
/// - `Err(msg)`              → `Error { message }`
/// - `Ok(None)`              → `UpToDate { current }`
/// - `Ok(Some(info))`        → `UpdateAvailable { .. }`
///
/// `current` is the running app version (from Tauri's package info).
/// This function intentionally performs **no** download or install —
/// it only classifies the result for the UI (G3.3).
pub fn cmd_osl_check_for_updates(
    current: String,
    outcome: Result<Option<UpdateInfo>, String>,
) -> UpdateCheckResult {
    match outcome {
        Err(message) => {
            tracing::warn!(
                target: "osl::updater",
                %current,
                %message,
                "[OSL updater] check failed"
            );
            UpdateCheckResult::Error { message }
        }
        Ok(None) => {
            tracing::info!(
                target: "osl::updater",
                %current,
                "[OSL updater] up to date"
            );
            UpdateCheckResult::UpToDate { current }
        }
        Ok(Some(info)) => {
            tracing::info!(
                target: "osl::updater",
                %current,
                next = %info.version,
                "[OSL updater] update available"
            );
            UpdateCheckResult::UpdateAvailable {
                current,
                next: info.version,
                notes: info.notes.unwrap_or_default(),
                url: info.url,
            }
        }
    }
}

/// G3.3: JS-facing result of an install attempt. The *success* path
/// is not represented here — a successful `download_and_install`
/// relaunches the process, so JS never receives a value in that
/// case. This enum only covers the cases where the command returns
/// normally without restarting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateInstallResult {
    /// `check()` came back empty — nothing to install (the normal
    /// 204 "no update" case; benign).
    NoUpdate,
    /// Download / signature-verify / install failed. `message` is
    /// safe to show; on signature failure NOTHING was installed.
    Error { message: String },
}

/// Unit b1: `RnWirePath` dispatch seam acceptance tests.
///
/// `select_rn_wire_path` is a pure function of (pin, capabilities,
/// policy) — no `AppState`/Tauri scaffolding needed, following the
/// precedent in `test_deep_link_parser`.
#[cfg(test)]
mod unit_b1_rn_wire_path_dispatch {
    use super::*;
    use crate::wire_rn::{RnPeerPin, RnPolicy};
    use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};

    /// (1) Gate off (the real, shipped `RN_WIRE_IN_ENABLED` constant —
    /// not a test override) + an unpinned peer with no advertised
    /// capabilities is exactly what every real send looks like today
    /// (no code path raises a pin or advertises capabilities in this
    /// build). Dispatch must select the v3 path, unchanged from
    /// pre-unit-b1 behavior.
    #[test]
    fn gate_off_unpinned_peer_dispatches_legacy_v3_unchanged() {
        assert!(
            !crate::wire_rn::RN_WIRE_IN_ENABLED,
            "this test asserts against the real production gate value"
        );
        let pin = RnPeerPin::UNKNOWN;
        let result = select_rn_wire_path(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic);
        assert_eq!(result, Ok(RnWirePath::LegacyV3));
    }

    /// (2a) A peer pinned to OSL-RN, whose capability record does not
    /// verify support (the honest state for every peer in this build,
    /// since capability advertisement is absent end-to-end) must never
    /// be dispatched as `LegacyV3` — it must refuse instead.
    #[test]
    fn pinned_peer_without_verified_support_refuses_not_downgrades() {
        let mut pin = RnPeerPin::UNKNOWN;
        pin.raise_to_rn();
        assert!(pin.is_pinned_to_rn());

        let result = select_rn_wire_path(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic);
        assert!(result.is_err(), "pinned peer must refuse, got {result:?}");
        assert_ne!(result, Ok(RnWirePath::LegacyV3));
    }

    /// (2b) A peer pinned to OSL-RN whose capability record *does*
    /// verify RN support selects `Rn` at the `wire_rn::select_wire_version`
    /// layer — but this build ships with `RN_WIRE_IN_ENABLED == false`,
    /// so `select_rn_wire_path` must still refuse rather than silently
    /// falling back to v3. No input to this function can produce a
    /// `LegacyV3` result for a pinned peer.
    #[test]
    fn pinned_peer_with_verified_support_still_never_downgrades_while_gate_is_off() {
        let mut pin = RnPeerPin::UNKNOWN;
        pin.raise_to_rn();

        let result = select_rn_wire_path(
            &pin,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            RnPolicy::Opportunistic,
        );
        assert!(
            result.is_err(),
            "gate is off in this build, so RN selection must refuse, not silently send v3: {result:?}"
        );
        assert_ne!(result, Ok(RnWirePath::LegacyV3));
    }

    /// (3) `RnPolicy::Required` against a peer with no verified RN
    /// support must refuse — never silently proceed on the legacy
    /// path when the caller explicitly required RN.
    #[test]
    fn required_policy_against_non_rn_peer_refuses() {
        let pin = RnPeerPin::UNKNOWN;
        let result = select_rn_wire_path(&pin, PeerCapabilities::Absent, RnPolicy::Required);
        assert!(
            result.is_err(),
            "Required policy must refuse, got {result:?}"
        );
        assert_ne!(result, Ok(RnWirePath::LegacyV3));
    }
}
