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
use crypto::{aead, hkdf, random, x25519};
use keystore::{generate_identity, select_best_sealer, KeyServerClient};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use store::{StoreError, StoredMessage};

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
/// by the user's Discord snowflake, matching the loaded identity's
/// `user_id` and X25519 public key with `is_self = true`.
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
            None => return Err(format!("OSL: no peer {discord_id} in peer_map")),
        }
    };
    if changed {
        tracing::warn!(
            discord_id = %discord_id,
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
                    && crate::whitelist::can_encrypt_to(
                        &pm,
                        &scope,
                        did.as_str(),
                    )
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
                peer = %peer,
                error = %e,
                "OSL: v=5 reset — could not build SESSION_RESET notice \
                 (peer will recover on first failed decrypt instead)"
            ),
        }
    }

    tracing::warn!(
        scope = %scope_key,
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
            None => return Err(format!("OSL: no peer {peer_discord_id} in peer_map")),
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
pub fn cmd_osl_recover_peer_identity(
    state: &AppState,
    discord_id: String,
) -> Result<bool, String> {
    let changed = refresh_peer_pubkeys_from_keyserver(state, &discord_id)?;
    if changed {
        persist_peer_map_now(state);
        tracing::warn!(
            peer = %discord_id,
            "OSL: stale-identity recovery — re-fetched peer bundle from \
             keyserver; a CHANGED identity is now a pending TOFU alert \
             (loud, one-tap accept), NOT auto-trusted"
        );
    }
    Ok(changed)
}

/// 7d-FIX3b: persist a Discord snowflake on the loaded identity and
/// repair the peer_map self-entry to match. Called from boot.js
/// the first time the runtime exposes the local user's snowflake.
///
/// Validates 17-20 digit format. Rejects mismatch against an
/// existing recorded snowflake (account-change refusal). Idempotent
/// for matching re-registrations (just runs verify).
pub fn cmd_osl_register_self_snowflake(state: &AppState, snowflake: String) -> Result<(), String> {
    let dir = keystore::osl_config_dir()
        .map_err(|e| format!("OSL: register_self_snowflake: config dir: {e}"))?;
    cmd_osl_register_self_snowflake_with_dir(state, snowflake, &dir)
}

/// Test seam: same as [`cmd_osl_register_self_snowflake`] but takes
/// the config dir explicitly so unit tests can point it at a
/// `tempdir()` instead of the real `%APPDATA%\osl` / `~/.config/osl`.
/// Production callers use the no-dir wrapper above.
pub fn cmd_osl_register_self_snowflake_with_dir(
    state: &AppState,
    snowflake: String,
    dir: &std::path::Path,
) -> Result<(), String> {
    osl_trace!("[F0-FIX3-TRACE] cmd_osl_register_self_snowflake entered (snowflake={snowflake})");
    if !snowflake.chars().all(|c| c.is_ascii_digit()) || !(17..=20).contains(&snowflake.len()) {
        return Err(format!(
            "OSL: register_self_snowflake: invalid format \
             (expected 17-20 digit numeric, got {} chars)",
            snowflake.len()
        ));
    }

    // 9-F0-FIX2: V2 clean-install path.
    //
    // Pre-FIX2, this command required `state.identity` to already be
    // populated (bootstrap's `load_or_generate_identity` did the
    // creation, gated on `keyserver.json` being present). V2 retired
    // `keyserver.json`, so bootstrap never auto-creates the identity,
    // and `cmd_osl_set_main_password` doesn't either (it has no
    // user_id to seed with). The first moment we DO have a stable
    // user identifier is when boot.js extracts the Discord snowflake
    // from the React runtime and calls THIS command.
    //
    // If `state.identity` is None at entry, generate a fresh identity
    // with the snowflake as `user_id`, stamp `discord_snowflake`,
    // persist to disk via the configured sealer (TPM / Keyring /
    // NoOp — identity at-rest protection lives in the sealer layer,
    // not the file_storage_key envelope), then fall through to the
    // existing peer_map self-entry repair via run_verify.
    let needs_generation = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        guard.is_none()
    };
    if needs_generation {
        osl_trace!("[F0-FIX3-TRACE] F0-FIX2 auto-gen path entered (state.identity was None)");
        // Finding 3b: regenerates an identity WITHOUT burn_wipe_all,
        // orphaning any store/ DB sealed by the old x25519_secret;
        // reconciliation deliberately relies on bootstrap's
        // open_message_store quarantine self-heal — do NOT add a
        // wipe here without flagging it as a separate proposal.
        let identity = keystore::generate_identity(snowflake.clone());
        let mut snapshot = keystore::Identity::from_bytes(
            identity.user_id.clone(),
            *identity.x25519_secret.as_bytes(),
            *identity.x25519_public.as_bytes(),
            *identity.ed25519_secret.as_bytes(),
            *identity.ed25519_public.as_bytes(),
            *identity.mlkem_secret_bytes(),
            identity.mlkem_public_bytes,
        );
        snapshot.discord_snowflake = Some(snowflake.clone());
        snapshot.ratchet_initial_secret = identity.ratchet_initial_secret.clone();
        snapshot.ratchet_initial_pub = identity.ratchet_initial_pub.clone();

        let path = dir.join("identity.json");
        let sealer = keystore::select_best_sealer();
        osl_trace!(
            "[F0-FIX3-TRACE] save_identity target path={} sealer={}",
            path.display(),
            sealer.method_label()
        );
        // Ensure the parent dir exists — bootstrap's create_dir_all
        // covers this for the production path, but tests pass in a
        // tempdir that may or may not have the leaf created yet.
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match keystore::save_identity(&path, &snapshot, sealer.as_ref()) {
            Ok(()) => {
                osl_trace!("[F0-FIX3-TRACE] identity saved successfully");
            }
            Err(e) => {
                osl_trace!("[F0-FIX3-TRACE] save_identity ERROR: {e}");
                return Err(format!("OSL: save_identity (first-time): {e}"));
            }
        }

        *state.identity.lock().expect("identity mutex poisoned") = Some(snapshot);
        eprintln!("[OSL][f0-fix2] generated + saved fresh identity (user_id={snowflake})");
        // Finding 3b companion: this is a non-burn local identity
        // regen. Any ratchet_state in peer_map was derived from the
        // OLD local identity's SessionContext and is now
        // undecryptable for every peer — drop them all so the next
        // v=4 re-handshakes (burn would have wiped peer_map entirely;
        // this path doesn't).
        clear_all_peer_ratchet_state(state);
        // REGISTER-FIX: this is the V2 clean-install moment the
        // identity first comes into existence. Bootstrap could not
        // register at boot (no identity.json yet) and the password
        // gate fired before this (so its post-unlock hook saw no
        // identity). Without registering HERE, the machine never
        // reaches POST /v1/register until a *second* relaunch. The
        // call is idempotent (upsert) and non-fatal.
        ensure_keyserver_registered(
            state,
            &resolve_keyserver_base_url(dir),
            read_keyserver_admin_token(dir),
        );
        return run_verify(state);
    }

    enum Step {
        Save(keystore::Identity),
        AlreadySet,
    }
    let step = {
        let mut guard = state.identity.lock().expect("identity mutex poisoned");
        let id = guard
            .as_mut()
            .ok_or_else(|| "OSL: register_self_snowflake: identity not loaded".to_string())?;
        if let Some(existing) = &id.discord_snowflake {
            if existing != &snowflake {
                return Err(format!(
                    "OSL: register_self_snowflake: snowflake mismatch \
                     (identity already bound to {}, refusing to retag to {}) — \
                     this could indicate a Discord account change or a \
                     state-corruption bug. Burn account + re-register if intentional.",
                    existing, snowflake
                ));
            }
            Step::AlreadySet
        } else {
            id.discord_snowflake = Some(snowflake.clone());
            let mut snapshot = keystore::Identity::from_bytes(
                id.user_id.clone(),
                *id.x25519_secret.as_bytes(),
                *id.x25519_public.as_bytes(),
                *id.ed25519_secret.as_bytes(),
                *id.ed25519_public.as_bytes(),
                *id.mlkem_secret_bytes(),
                id.mlkem_public_bytes,
            );
            snapshot.discord_snowflake = Some(snowflake.clone());
            Step::Save(snapshot)
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
                read_keyserver_admin_token(dir),
            );
            return run_verify(state);
        }
        Step::Save(snapshot) => snapshot,
    };

    let path = dir.join("identity.json");
    let sealer = keystore::select_best_sealer();
    if let Err(e) = keystore::save_identity(&path, &to_save, sealer.as_ref()) {
        // Roll back the in-memory change so callers can retry
        // without lying about the durable state.
        let mut guard = state.identity.lock().expect("identity mutex poisoned");
        if let Some(id) = guard.as_mut() {
            id.discord_snowflake = None;
        }
        return Err(format!(
            "OSL: register_self_snowflake: save_identity failed: {e}"
        ));
    }
    eprintln!("[OSL][bootstrap] self snowflake registered: {snowflake}");
    // REGISTER-FIX: snowflake just attached to a pre-existing
    // identity and persisted — register against the keyserver now
    // rather than waiting for the next relaunch. Idempotent, non-fatal.
    ensure_keyserver_registered(
        state,
        &resolve_keyserver_base_url(dir),
        read_keyserver_admin_token(dir),
    );
    run_verify(state)
}

fn run_verify(state: &AppState) -> Result<(), String> {
    match verify_and_persist_peer_map_self_entry(state) {
        Ok((snowflake, repaired)) => {
            if repaired {
                eprintln!("[OSL][bootstrap] self-entry repaired for snowflake={snowflake}");
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
    *state.identity.lock().expect("identity mutex poisoned") = Some(identity);
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
    *state.identity.lock().expect("identity mutex poisoned") = Some(id);
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
            effective.push(pk.clone());
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
        let resp = client
            .fetch_pubkeys(user_id)
            .map_err(|e| format!("OSL: fetch_pubkeys({user_id}): {e}"))?;
        let peer_pub_vec = STANDARD
            .decode(&resp.ik_x25519_pub)
            .map_err(|e| format!("OSL: decode peer pubkey ({user_id}): {e}"))?;
        if peer_pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: peer pubkey wrong length ({user_id}): got {}, want {}",
                peer_pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE
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
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        let resp = client
            .fetch_pubkeys(&osl_user_id)
            .map_err(|e| format!("OSL: fetch_pubkeys({osl_user_id}): {e}"))?;
        let pub_vec = STANDARD
            .decode(&resp.ik_x25519_pub)
            .map_err(|e| format!("OSL: decode sender pubkey ({osl_user_id}): {e}"))?;
        if pub_vec.len() != x25519::PUBLIC_KEY_SIZE {
            return Err(format!(
                "OSL: sender pubkey wrong length ({osl_user_id}): got {}, want {}",
                pub_vec.len(),
                x25519::PUBLIC_KEY_SIZE
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
            .insert(osl_user_id.clone(), pub_key.clone());
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
                 [diag: our_hint=0x{our_hint:02x} {diag} osl_user_id={osl_user_id}]"
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
            discord_message_id = %discord_message_id,
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
            discord_message_id = %discord_message_id,
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
                    discord_message_id = %discord_message_id,
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
            discord_message_id = %discord_message_id,
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
            discord_message_id = %discord_message_id,
            error = %e,
            "OSL: persist_outbound: store.put failed (non-fatal)"
        );
    }
    Ok(())
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
    let entry = peer_map
        .get(discord_id)
        .ok_or_else(|| format!("OSL: no peer entry for discord_id={discord_id}"))?;
    let b64 = entry
        .pubkey
        .as_deref()
        .ok_or_else(|| format!("OSL: no pubkey for discord_id={discord_id}"))?;
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

            let salt = scope_for_mode.storage_key().into_bytes();
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
                scope = %scope_for_mode.storage_key(),
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
                         plaintext or self-only)"
                    ));
                }
            }
        }
        Err(e) => return Err(format!("OSL: v=3 capability check: {e}")),
    };

    // Phase 9-A2: single-peer (DM-shaped) sends route through v=4
    // when the peer is ratchet-eligible. `recipients[0]` is always
    // self; non-self recipients are the actual peers. v=4 fires
    // exactly when there's one non-self recipient.
    //
    // GC Step 2: a group scope must NEVER take the v=4 single-peer
    // DM branch — even a gc:/server scope that currently resolves
    // to exactly one OSL peer is a group and belongs on v=5
    // sender-keys (the v4 branch's fail-closed keyserver refresh
    // also doesn't fit the "skip non-OSL members" group model).
    // Gate the single-peer branch on a non-group scope; group
    // scopes fall through to the v=5 router below.
    let non_self_peers: Vec<&(String, crate::wire_v2::RecipientV3)> = recipients
        .iter()
        .skip(1) // recipients[0] is (self_discord_id, self) per recipients_for_scope_v3
        .collect();
    if non_self_peers.len() == 1 && !scope_is_group_or_server(&scope) {
        let peer_did_opt = derive_v4_peer_discord_id(state, &channel_members, &self_discord_id);
        if let Some(peer_did) = peer_did_opt {
            // Probe peer_map for v=4 eligibility. Eligible iff (a)
            // peer entry has ratchet_state (continuation) or (b)
            // entry has ik_ratchet_initial_pub (bootstrap target).
            let mut eligible = false;
            {
                let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
                if let Some(pe) = pm_guard.get(&peer_did) {
                    eligible = pe.ratchet_state.is_some() || pe.ik_ratchet_initial_pub.is_some();
                }
            }
            // Phase 9-A1b precedent: refresh-on-error retry. If the
            // entry has ML-KEM (so v=3 would work) but no ratchet
            // pub, attempt a single keyserver fetch to populate it
            // before deciding v=4 vs v=3.
            if !eligible {
                if let Ok(true) = refresh_peer_pubkeys_from_keyserver(state, &peer_did) {
                    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
                    if let Some(pe) = pm_guard.get(&peer_did) {
                        eligible =
                            pe.ratchet_state.is_some() || pe.ik_ratchet_initial_pub.is_some();
                    }
                }
            }
            if eligible {
                // Cross-machine decrypt fix: v=4 is single-recipient
                // and unforgiving. When the peer entry already has a
                // ratchet pub the `!eligible` refresh above is skipped,
                // so a stale `peer_map.pubkey` (e.g. the peer's
                // pre-burn X25519, never re-fetched) survives and we
                // wrap to a key the peer no longer holds → the peer
                // sees "not a recipient of this message". Force a
                // keyserver refresh for THIS recipient and re-resolve
                // so we encrypt to the current X25519. FAIL-CLOSED: a
                // refresh failure used to be swallowed (`let _ = …`),
                // silently encrypting to a possibly-stale key. We now
                // surface the error and refuse to send — a stale-key
                // mis-encrypt is undiagnosable from the peer side; a
                // surfaced send error is not.
                if let Err(e) = refresh_peer_pubkeys_from_keyserver(state, &peer_did) {
                    return Err(format!(
                        "OSL: v=4 send: keyserver refresh for recipient \
                         {peer_did} failed: {e} — refusing to encrypt with \
                         a possibly-stale X25519 key (fail-closed; message \
                         NOT sent)"
                    ));
                }
                let fresh_recipients = resolve_recipients()
                    .map_err(|e| format!("OSL: v=4 recipient refresh: {e}"))?;
                // v=4 keeps its own peer_did (derive_v4_peer_discord_id,
                // untouched); only the keys are extracted from the pair.
                let fresh_peer = &fresh_recipients
                    .get(1)
                    .ok_or_else(|| {
                        format!(
                            "OSL: v=4 send: recipient {peer_did} vanished \
                             from peer_map after keyserver refresh"
                        )
                    })?
                    .1;
                return encrypt_v4_send(
                    state,
                    &sender_sk,
                    &self_pk,
                    &peer_did,
                    fresh_peer,
                    &scope,
                    plaintext.as_bytes(),
                    &self_discord_id,
                )
                .map(EncryptWire::content_only);
            }
        }
    }

    // Phase 9-A3 / GC Step 2: groups + server channels route to v=5
    // (sender-keys). DM-shape (handled above) routes to v=4.
    // Threshold lowered from >=2 to >=1: a gc:/server scope with at
    // least one OSL-resolvable peer is still a group and must use
    // sender-keys, not v=4 (the single-peer DM path is now gated
    // off for group scopes above). With 0 resolvable OSL peers it
    // falls through to v=3 self-only (non-OSL members see DPC0::,
    // per decision (a)). Anything else falls through to v=3.
    if !non_self_peers.is_empty() && scope_is_group_or_server(&scope) {
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

/// Phase 9-A3: group/server scopes are eligible for v=5 sender-keys.
fn scope_is_group_or_server(scope: &crate::scope::Scope) -> bool {
    use crate::scope::ScopeKind::*;
    matches!(scope.kind, Gc | ServerChannel | ServerFull)
}

/// Phase 9-A3: 24-hour rotation timer threshold.
const SENDER_KEY_ROTATE_AFTER_SECS: u64 = 24 * 60 * 60;

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
    channel_members: &[String],
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

    // Bug 3 (a): server-channel sends are roster-independent. The
    // SKDM fan-out + sender-key rotation membership for a
    // ServerChannel scope is enumerated from peer_map (every peer
    // whitelisted for this exact server_channel:<guild>:<channel>
    // scope, via the same `can_encrypt_to` gate recipient
    // resolution uses), NOT the gateway/React roster caches, which
    // are empty for server channels. Self excluded by discord_id
    // AND the `is_self` marker. Gc / Dm keep the
    // gateway-snapshot-then-caller-list preference unchanged.
    let server_channel_members: Vec<String> =
        if scope.kind == crate::scope::ScopeKind::ServerChannel {
            let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm.iter()
                .filter(|(did, entry)| {
                    did.as_str() != self_discord_id
                        && entry.is_self != Some(true)
                        && crate::whitelist::can_encrypt_to(&pm, scope, did.as_str())
                })
                .map(|(did, _)| did.clone())
                .collect()
        } else {
            Vec::new()
        };
    // Prefer the boot.js-pushed gateway snapshot of current channel
    // members (state.channel_members) over the caller's
    // `channel_members` list, which is built from React fiber props
    // at send time and may be stale on server channels. The cache
    // is populated by `cmd_osl_membership_update` from boot.js
    // gateway hooks. Empty cache → fall through to caller list.
    let live_members: Vec<String> = {
        let cm = state
            .channel_members
            .lock()
            .expect("channel_members mutex poisoned");
        // Channel id for the cache key — for DM/GC scopes the
        // scope's id field IS the channel id; for server channels
        // it's scope.channel_id.
        let cache_key = scope.channel_id.clone().unwrap_or_else(|| scope.id.clone());
        cm.get(&cache_key).cloned().unwrap_or_default()
    };
    let effective_members: &[String] =
        if scope.kind == crate::scope::ScopeKind::ServerChannel {
            &server_channel_members
        } else if live_members.is_empty() {
            channel_members
        } else {
            &live_members
        };

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

    // Decide install / rotate / continue.
    let needs_install = sks.sender_chain().is_none();
    let needs_rotate = sks
        .sender_chain()
        .map(|c| sender_key_needs_rotation(c, effective_members, now))
        .unwrap_or(false);

    let send_skdm = needs_install || needs_rotate;
    if needs_install {
        sks.install_sender()
            .map_err(|e| format!("OSL: v=5 send: install_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = effective_members
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    } else if needs_rotate {
        sks.rotate_sender()
            .map_err(|e| format!("OSL: v=5 send: rotate_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = effective_members
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    }

    let (chain_id, rotation_root) = {
        let s = sks
            .sender_chain()
            .ok_or_else(|| "OSL: v=5 send: missing sender chain after install".to_string())?;
        (s.current_chain_id(), s.rotation_root_bytes())
    };

    // Self-loopback: install/rotate a self-receiver chain seeded
    // from the same rotation_root so self-decrypts work uniformly.
    if send_skdm {
        let self_bytes = self_discord_id.as_bytes().to_vec();
        if sks.receiver_chain(&self_bytes).is_some() {
            sks.rotate_receiver(&self_bytes, chain_id, &rotation_root)
                .map_err(|e| format!("OSL: v=5 send: rotate_receiver(self): {e}"))?;
        } else {
            sks.install_receiver(self_bytes, chain_id, &rotation_root)
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
    if !non_self_peers.is_empty() {
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
        ) {
            Ok(skdm_wire) => {
                skdm_wires.push(skdm_wire);
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
) -> Result<String, String> {
    let payload = crate::control_messages::SenderKeyDistribution {
        scope_storage_key: scope_storage_key.to_string(),
        chain_id,
        rotation_root: *rotation_root,
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

/// Phase 9-A2: pick out the peer discord_id from
/// `channel_members` so the v=4 dispatch can look up the peer's
/// ratchet eligibility. Returns `None` when no non-self member is
/// present (encrypt-to-self only, no peer to ratchet against).
fn derive_v4_peer_discord_id(
    _state: &AppState,
    channel_members: &[String],
    self_discord_id: &str,
) -> Option<String> {
    channel_members
        .iter()
        .find(|m| m.as_str() != self_discord_id)
        .cloned()
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

/// Phase 9-A2: v=4 send. Loads peer's ratchet state (bootstrap iff
/// None), runs `DoubleRatchet::encrypt`, persists the advanced DR
/// state, and ships the wire blob.
fn encrypt_v4_send(
    state: &AppState,
    sender_sk: &crypto::x25519::SecretKey,
    self_pk: &crypto::x25519::PublicKey,
    peer_did: &str,
    recipient: &crate::wire_v2::RecipientV3,
    scope: &crate::scope::Scope,
    plaintext: &[u8],
    self_discord_id: &str,
) -> Result<String, String> {
    use crypto::ratchet::{DoubleRatchet, RatchetStateOnDisk, SessionContext, SESSION_VERSION_V1};
    let _ = scope; // reserved for non-DM scopes in a future phase

    // Send-vs-receive stale-key triage: log the EXACT recipient
    // X25519 (and its slot-hash prefix — the value written into the
    // wire and compared by the peer's decrypt_v4 slot scan) that
    // this message is wrapped to. Compare side-by-side with the
    // receiver's `decrypt_v4_recv` log: equal ⇒ keys aligned;
    // different ⇒ pinpoints which machine holds the stale key.
    tracing::info!(
        target: "osl::v4",
        peer_did = %peer_did,
        recipient_x25519_b64 = %STANDARD.encode(recipient.x25519_pub.as_bytes()),
        recipient_slot_hash =
            %STANDARD.encode(crate::wire_v2::pubkey_hash_prefix(&recipient.x25519_pub)),
        "OSL: v=4 send — wrapping to recipient X25519"
    );

    // Snapshot self ML-KEM pubkey for the SessionContext binding.
    let self_mlkem_pub_bytes: Vec<u8> = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        identity.mlkem_public_bytes.to_vec()
    };
    // And peer's ML-KEM pub from peer_map for the AD binding.
    let peer_mlkem_pub_bytes: Vec<u8> = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard
            .get(peer_did)
            .ok_or_else(|| format!("OSL: v=4 send: peer {peer_did} not in peer_map"))?;
        let b64 = pe
            .ik_mlkem768_pub
            .as_deref()
            .ok_or_else(|| format!("OSL: v=4 send: peer {peer_did} missing ik_mlkem768_pub"))?;
        STANDARD
            .decode(b64)
            .map_err(|e| format!("OSL: v=4 send: peer ik_mlkem768_pub b64: {e}"))?
    };

    let ctx = SessionContext {
        local_ik_x25519_pub: *self_pk,
        local_ik_mlkem_pub: self_mlkem_pub_bytes,
        peer_ik_x25519_pub: recipient.x25519_pub,
        peer_ik_mlkem_pub: peer_mlkem_pub_bytes,
        conversation_id: dm_conversation_id(self_discord_id, peer_did),
        session_version: SESSION_VERSION_V1,
    };

    // Single PQXDH run per send. session_key serves both as the DR
    // bootstrap seed (when bootstrapping) AND as the input to the
    // wrap leg's HKDF — the receiver derives the same session_key
    // from pqxdh::respond on the wire's handshake bytes.
    let (session_key, handshake) = crypto::pqxdh::initiate(
        sender_sk,
        &recipient.x25519_pub,
        &recipient.x25519_pub,
        None,
        &recipient.mlkem_pub,
    )
    .map_err(|e| format!("OSL: v=4 send: pqxdh::initiate: {e}"))?;

    // Load (or bootstrap) the live DR.
    let (mut dr, bootstrap) = {
        let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard
            .get(peer_did)
            .cloned()
            .ok_or_else(|| format!("OSL: v=4 send: peer {peer_did} not in peer_map"))?;
        match pe.ratchet_state {
            Some(disk) => {
                let dr: DoubleRatchet = disk
                    .try_into()
                    .map_err(|e| format!("OSL: v=4 send: load ratchet state: {e}"))?;
                (dr, false)
            }
            None => {
                let peer_ratchet_b64 = pe.ik_ratchet_initial_pub.as_deref().ok_or_else(|| {
                    format!("OSL: v=4 send: peer {peer_did} ratchet bootstrap pub missing")
                })?;
                let peer_ratchet_bytes = STANDARD
                    .decode(peer_ratchet_b64)
                    .map_err(|e| format!("OSL: v=4 send: peer ratchet pub b64: {e}"))?;
                if peer_ratchet_bytes.len() != 32 {
                    return Err(format!(
                        "OSL: v=4 send: peer ratchet pub length {} != 32",
                        peer_ratchet_bytes.len()
                    ));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&peer_ratchet_bytes);
                let peer_ratchet_pub = crypto::x25519::PublicKey::from_bytes(arr);
                let dr = DoubleRatchet::new_initiator(&session_key, &peer_ratchet_pub, ctx.clone())
                    .map_err(|e| format!("OSL: v=4 send: new_initiator: {e}"))?;
                (dr, true)
            }
        }
    };

    let em = dr
        .encrypt(plaintext)
        .map_err(|e| format!("OSL: v=4 send: dr.encrypt: {e}"))?;
    let wire = crate::wire_v2::encrypt_v4_from_ratchet(
        self_pk,
        recipient,
        &session_key,
        &handshake,
        crate::wire_v2::MSG_TYPE_CONTENT,
        bootstrap,
        &em,
    )
    .map_err(|e| format!("OSL: v=4 send: encrypt_v4: {e}"))?;

    // Persist the advanced DR state on the peer entry.
    {
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let pe = pm_guard.entry(peer_did.to_string()).or_default();
        pe.ratchet_state = Some(RatchetStateOnDisk::from(&dr));
    }
    persist_peer_map_now(state);
    Ok(wire)
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

/// Phase 8d: one-shot open. Splits the file into (cover, filename,
/// payload), decrypts the cover via the existing v=2 path, recovers
/// the per-attachment AEAD key from the envelope, then decrypts the
/// payload. Backwards-compatible with V1 files (signaled by the
/// empty cover from `open_attachment_v2_split`) — falls back to the
/// caller-supplied legacy `att_key_b64` argument for V1 only.
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
    // 9-A1c: burn kill list defense-in-depth on attachment open.
    // If this specific message was burned, refuse to recover its
    // attachment even if the scope was later unburned.
    if let (Some(input), Some(msg_id)) = (scope_input.as_ref(), discord_message_id.as_deref()) {
        let scope: crate::scope::Scope = input
            .clone()
            .try_into()
            .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
        if is_message_in_burn_kill_list(state, &scope, msg_id) {
            tracing::info!(
                msg_id = %msg_id,
                "[OSL] attachment open blocked: in_burn_kill_list"
            );
            return Err(format!(
                "OSL: attachment open blocked: msg={msg_id} reason=in_burn_kill_list"
            ));
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
    // att_key_b64 (legacy Phase 8/8c flow).
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
        // V1 path: caller must supply the AEAD key.
        let b64 = legacy_att_key_b64
            .ok_or_else(|| "OSL: V1 file with no legacy att_key supplied".to_string())?;
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
        let mut out: Vec<crypto::x25519::PublicKey> =
            crate::whitelist::recipients_for_scope(
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
                tracing::debug!(
                    scope = %scope.storage_key(),
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
    // v=3 → Phase 9-A1 PQ-hybrid decrypt_v3. Anything else falls
    // through to the legacy v=1 path which surfaces its own errors.
    let version = peek_wire_version(&content);

    // 9-A1c: burn kill list defense-in-depth. If this specific
    // discord_message_id was recorded in the scope's burn entry,
    // refuse to decrypt regardless of whether the scope-level skip
    // cache is currently set. Protects against the manual re-engage
    // path inadvertently reviving old burned ciphertexts.
    if let (Some(scope), Some(msg_id)) = (scope_opt.as_ref(), discord_message_id.as_deref()) {
        if is_message_in_burn_kill_list(state, scope, msg_id) {
            tracing::info!(msg_id = %msg_id, "[OSL] decrypt blocked: in_burn_kill_list");
            return Err(format!(
                "OSL: decrypt blocked: msg={msg_id} reason=in_burn_kill_list"
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
            crate::wire_v2::decrypt_v2(&content, &our_sk, &sender_pub)
                .map_err(|e| format!("OSL: {e}"))?
        }
        Some(crate::wire_v2::WIRE_VERSION_V3) => {
            // v=3 path — PQ-hybrid wrap. Sender ik pubkey is in the
            // wire global header, so no peer_map lookup needed for
            // the decrypt itself.
            let id_guard = state.identity.lock().expect("identity mutex poisoned");
            let identity = id_guard
                .as_ref()
                .ok_or_else(|| "OSL: identity not loaded".to_string())?;
            let our_sk = identity.x25519_secret.clone();
            let our_mlkem_sk = identity.mlkem_decapsulation_key();
            drop(id_guard);
            tracing::debug!(wire_version = "v3", "v=3 decode dispatched");
            crate::wire_v2::decrypt_v3(&content, &our_sk, &our_mlkem_sk)
                .map_err(|e| format!("OSL: {e}"))?
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
                sender = %sender_discord_id,
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
            Ok(format!("{}{}", OSL_RESULT_ATTACHMENT_PREFIX, json))
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
                 — bootstrap flag was false but local state is None (desync)"
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
            // Recorded so a subsequent SESSION_RESET from this peer is
            // honored only when backed by a real local failure.
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
                requester = %requester_discord_id,
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
                requester = %requester_discord_id,
                scope_storage_key = %req.scope_storage_key,
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
    let (chain_id, rotation_root) = {
        use crypto::sender_keys::SenderKeyState;
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        let Some(disk) = g.states.get(&scope_key) else {
            tracing::warn!(
                requester = %requester_discord_id,
                scope = %scope_key,
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
            Some(c) => (c.current_chain_id(), c.rotation_root_bytes()),
            None => {
                tracing::warn!(
                    requester = %requester_discord_id,
                    scope = %scope_key,
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
                    requester = %requester_discord_id,
                    scope = %scope_key,
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
    )?;
    tracing::info!(
        requester = %requester_discord_id,
        scope = %scope_key,
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
            peer = %sender_discord_id,
            corroborated = corroborated,
            "OSL: SESSION_RESET honored — dropped v=4 ratchet; next v=4 \
             re-handshakes (corroborated=false means a one-directional \
             desync healed off the authenticated reset alone)"
        );
    }
    Ok(OSL_RESULT_SESSION_RESET_APPLIED.to_string())
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
    use crypto::sender_keys::{SenderKeyState, SenderKeyStateOnDisk};
    let payload = crate::control_messages::deserialize_sender_key_distribution(payload_bytes)
        .map_err(|e| format!("OSL: SKDM: deserialize: {e}"))?;

    let scope_key = payload.scope_storage_key.clone();
    // Reject SKDMs for an unknown scope_kind (defensive — boot.js
    // should never produce one). `Scope::parse` returns None for
    // malformed storage keys.
    if crate::scope::Scope::parse(&scope_key).is_none() {
        return Err(format!(
            "OSL: SKDM: payload.scope_storage_key '{scope_key}' is not a valid scope"
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
        if live.receiver_chain(&peer_bytes).is_some() {
            live.rotate_receiver(&peer_bytes, payload.chain_id, &payload.rotation_root)
                .map_err(|e| format!("OSL: SKDM: rotate_receiver: {e}"))?;
        } else {
            live.install_receiver(peer_bytes, payload.chain_id, &payload.rotation_root)
                .map_err(|e| format!("OSL: SKDM: install_receiver: {e}"))?;
        }
        *entry = SenderKeyStateOnDisk::from(&live);
        g.version = 1;
    }
    persist_sender_key_state_now(state);
    tracing::info!(
        sender = %sender_discord_id,
        scope = %scope_key,
        chain_id = payload.chain_id,
        "[OSL] SKDM applied: receiver chain installed/rotated"
    );
    Ok(OSL_RESULT_SKDM_APPLIED.to_string())
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
                format!("OSL: v=5 decode: load sender_key_state for scope {scope_key}: {e}")
            })?,
            None => {
                return Err(format!(
                    "OSL: v=5 decode: no installed sender-key state for peer \
                     {sender_discord_id} in scope {scope_key} — awaiting SKDM"
                ));
            }
        }
    };

    let peer_bytes = sender_discord_id.as_bytes().to_vec();
    if sks.receiver_chain(&peer_bytes).is_none() {
        return Err(format!(
            "OSL: v=5 decode: no installed sender-key state for peer \
             {sender_discord_id} in scope {scope_key} — awaiting SKDM"
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
    let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
    let client = ks_guard
        .as_ref()
        .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
    let resp = client
        .fetch_pubkeys(&osl_user_id)
        .map_err(|e| format!("OSL: fetch_pubkeys({osl_user_id}): {e}"))?;
    let pub_vec = STANDARD
        .decode(&resp.ik_x25519_pub)
        .map_err(|e| format!("OSL: decode sender pubkey ({osl_user_id}): {e}"))?;
    if pub_vec.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: sender pubkey wrong length ({osl_user_id}): got {}",
            pub_vec.len()
        ));
    }
    let mut bytes = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
    bytes.copy_from_slice(&pub_vec);
    let pub_key = crypto::x25519::PublicKey::from_bytes(bytes);
    drop(ks_guard);
    state.sender_pubkey_cache.insert(osl_user_id, pub_key);
    Ok(pub_key)
}

/// Phase 9-A1b: populate peer's X25519 and ML-KEM pubkeys from a
/// keyserver `FetchPubkeysResponse`. Pure function — separated
/// from the HTTP fetch so tests can drive it without standing up
/// a mock keyserver. Returns true if the ML-KEM pubkey was
/// newly added (entry previously had None and the response had
/// a non-empty value).
pub fn populate_peer_from_fetch_response(
    state: &AppState,
    discord_id: &str,
    resp: &keystore::client::PubkeysResponse,
) -> Result<bool, String> {
    if resp.ik_x25519_pub.is_empty() {
        return Err(format!(
            "OSL: keyserver response for {discord_id} missing ik_x25519_pub"
        ));
    }
    // Validate decode shape early so we error before mutating peer_map.
    let x_vec = STANDARD
        .decode(&resp.ik_x25519_pub)
        .map_err(|e| format!("OSL: decode X25519 pubkey for {discord_id}: {e}"))?;
    if x_vec.len() != crypto::x25519::PUBLIC_KEY_SIZE {
        return Err(format!(
            "OSL: X25519 pubkey for {discord_id} wrong length: got {}",
            x_vec.len()
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
    let tofu_outcome_peek = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let baseline = pm
            .get(discord_id)
            .and_then(|e| e.tofu_ed25519_pub.clone());
        crate::tofu::classify(baseline.as_deref(), &resp.ik_ed25519_pub)
    };
    let live_writable = !matches!(
        tofu_outcome_peek,
        crate::tofu::TofuOutcome::Changed { .. }
    );
    let mut mlkem_added = false;
    if !resp.ik_mlkem768_pub.is_empty() {
        let mlkem_vec = STANDARD
            .decode(&resp.ik_mlkem768_pub)
            .map_err(|e| format!("OSL: decode ML-KEM pubkey for {discord_id}: {e}"))?;
        if mlkem_vec.len() != crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE {
            return Err(format!(
                "OSL: ML-KEM pubkey for {discord_id} wrong length: got {} (expected {})",
                mlkem_vec.len(),
                crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE
            ));
        }
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm_guard.entry(discord_id.to_string()).or_default();
        let had_mlkem = entry.ik_mlkem768_pub.is_some();
        if live_writable {
            entry.pubkey = Some(resp.ik_x25519_pub.clone());
            entry.ik_mlkem768_pub = Some(resp.ik_mlkem768_pub.clone());
            if let Some(ratchet) = resp.ik_ratchet_initial_pub.clone() {
                entry.ik_ratchet_initial_pub = Some(ratchet);
            }
            mlkem_added = !had_mlkem;
        }
        entry
            .discord_id
            .get_or_insert_with(|| discord_id.to_string());
        // REGISTER-FIX: leave a fully-consistent entry. osl_user_id
        // (== the keyserver user_id == the Discord snowflake in V2)
        // was never written here, which is why a keyless entry could
        // never self-heal and v=4 DM sends never became eligible.
        // Safe to set even on Changed — it's an identifier, not a key.
        entry
            .osl_user_id
            .get_or_insert_with(|| discord_id.to_string());
    } else {
        // X25519 only; same gating.
        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm_guard.entry(discord_id.to_string()).or_default();
        if live_writable {
            entry.pubkey = Some(resp.ik_x25519_pub.clone());
            if let Some(ratchet) = resp.ik_ratchet_initial_pub.clone() {
                entry.ik_ratchet_initial_pub = Some(ratchet);
            }
        }
        entry
            .discord_id
            .get_or_insert_with(|| discord_id.to_string());
        entry
            .osl_user_id
            .get_or_insert_with(|| discord_id.to_string());
    }

    if !live_writable {
        tracing::warn!(
            discord_id = %discord_id,
            "OSL: TOFU peek: peer Ed25519 changed but NOT yet accepted; \
             refusing to overwrite live X25519/ML-KEM/ratchet bootstrap \
             keys so outbound sends cannot silently encrypt to the new \
             (possibly attacker) key. The pending key-change alert will \
             surface; on accept, the next fetch reclassifies as \
             Unchanged and the live keys flow through."
        );
    }

    // REGISTER-FIX (TOFU): compare the peer's Ed25519 identity key
    // against the trusted first-seen baseline. NEVER blocks this
    // function — it records the baseline (first use) or raises a
    // blocking, user-visible KeyChangeAlert (NOT warn-swallowed) on
    // a change. On Changed we additionally drop the ratchet exactly
    // once (see fn-doc on `tofu_change_is_newly_observed`).
    tofu_observe_peer(state, discord_id, &resp.ik_ed25519_pub);

    Ok(mlkem_added)
}

/// REGISTER-FIX (TOFU): apply [`crate::tofu::classify`] to a peer's
/// freshly-fetched Ed25519 pub. On first use, record + persist the
/// baseline. On a change, raise a `KeyChangeAlert` (held until the
/// user accepts/declines) and clear it again once the key matches.
/// Pure decision logic lives in `crate::tofu`; this is the AppState
/// + peer_map + persistence wiring.
/// On a TOFU `Changed`, the peer's `ratchet_state` must be dropped
/// EXACTLY ONCE — on first detection of a given new key. This fn
/// decides "is this a newly-observed change?":
/// - no pending alert            → yes (first detection: drop once)
/// - pending alert, SAME new key → no  (already handled: keep ratchet
///                                       so a re-bootstrapped session
///                                       can survive and deliver while
///                                       the user verifies)
/// - pending alert, DIFFERENT key→ yes (the key changed AGAIN: the
///                                       old session is invalid; drop)
/// Pure so it is unit-tested without an `AppState`.
fn tofu_change_is_newly_observed(
    pending_alert_new_key: Option<&str>,
    fetched: &str,
) -> bool {
    match pending_alert_new_key {
        Some(k) => k != fetched,
        None => true,
    }
}

#[cfg(test)]
mod tofu_change_idempotency_tests {
    use super::tofu_change_is_newly_observed as f;

    #[test]
    fn first_detection_no_pending_alert_is_newly_observed() {
        // No alert yet → first detection → drop ratchet once.
        assert!(f(None, "NEWKEY"));
    }

    #[test]
    fn same_pending_change_is_not_newly_observed() {
        // The exact scenario that bricked DMs: every v=4 send
        // re-fetches and re-observes the SAME changed key. Must NOT
        // be treated as new (so the ratchet is not nuked every send).
        assert!(!f(Some("NEWKEY"), "NEWKEY"));
    }

    #[test]
    fn key_changed_again_is_newly_observed() {
        // Pending alert was for KEY_A but the peer rotated AGAIN to
        // KEY_B → the bootstrapped session is invalid → drop again.
        assert!(f(Some("KEY_A"), "KEY_B"));
    }

    #[test]
    fn repeated_calls_drop_exactly_once() {
        // Simulate the per-send refresh loop: first call drops, all
        // subsequent identical calls do not.
        let fetched = "ROTATED";
        let mut pending: Option<String> = None;
        let mut drops = 0;
        for _ in 0..50 {
            if f(pending.as_deref(), fetched) {
                drops += 1;
                pending = Some(fetched.to_string()); // alert now raised
            }
        }
        assert_eq!(drops, 1, "ratchet must be dropped exactly once");
    }
}

fn tofu_observe_peer(state: &AppState, discord_id: &str, fetched_ed25519_b64: &str) {
    use crate::tofu::{classify, safety_number, TofuOutcome};

    let (outcome, osl_user_id) = {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(discord_id.to_string()).or_default();
        let outcome = classify(entry.tofu_ed25519_pub.as_deref(), fetched_ed25519_b64);
        if matches!(outcome, TofuOutcome::FirstUse) {
            // Trust-on-FIRST-use: record the baseline. A change is
            // only ever adopted later by an explicit user accept.
            entry.tofu_ed25519_pub = Some(fetched_ed25519_b64.to_string());
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
                g.get(discord_id)
                    .map(|a| a.new_ed25519_pub.clone())
            };
            let newly_observed = tofu_change_is_newly_observed(
                pending_same.as_deref(),
                fetched_ed25519_b64,
            );
            if !newly_observed {
                // Same change we already alerted on — do NOT re-nuke
                // a (possibly freshly re-bootstrapped) ratchet, do
                // not reset the alert's first_observed. Idempotent.
                tracing::debug!(
                    discord_id = %discord_id,
                    "OSL: TOFU — peer key change re-observed (alert \
                     already pending); ratchet left intact so the \
                     session can establish pending user accept"
                );
                return;
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
                old_ed25519_pub: old,
                new_ed25519_pub: fetched_ed25519_b64.to_string(),
                new_safety_number: safety_number(fetched_ed25519_b64),
                first_observed: keystore::iso_8601_from_unix_seconds(
                    now_unix_secs().max(0) as u64,
                ),
            };
            tracing::error!(
                discord_id = %discord_id,
                "OSL: TOFU — peer Ed25519 identity key CHANGED; raising \
                 blocking key-change alert (NOT swallowed). Messaging \
                 continues; user must verify + accept or decline."
            );
            state
                .key_change_alerts
                .lock()
                .expect("key_change_alerts mutex poisoned")
                .insert(discord_id.to_string(), alert);
        }
    }
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

/// User ACCEPTED a peer's new identity key: adopt it as the new
/// trusted TOFU baseline, persist, and clear the alert. (User did
/// the out-of-band safety-number check, or accepts the risk.)
pub fn cmd_osl_accept_key_change(
    state: &AppState,
    discord_id: String,
) -> Result<(), String> {
    let new_key = {
        let g = state
            .key_change_alerts
            .lock()
            .expect("key_change_alerts mutex poisoned");
        match g.get(&discord_id) {
            Some(a) => a.new_ed25519_pub.clone(),
            None => return Err(format!("OSL: no pending key-change for {discord_id}")),
        }
    };
    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let entry = pm.entry(discord_id.clone()).or_default();
        entry.tofu_ed25519_pub = Some(new_key);
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
    tracing::warn!(
        discord_id = %discord_id,
        "OSL: TOFU — user ACCEPTED peer key change; baseline updated"
    );
    Ok(())
}

/// User DECLINED a peer's new identity key: clear the alert but keep
/// the OLD trusted baseline. The alert re-raises on the next fetch
/// while the key stays changed (so it can't be silently forgotten).
pub fn cmd_osl_decline_key_change(
    state: &AppState,
    discord_id: String,
) -> Result<(), String> {
    let removed = state
        .key_change_alerts
        .lock()
        .expect("key_change_alerts mutex poisoned")
        .remove(&discord_id)
        .is_some();
    if !removed {
        return Err(format!("OSL: no pending key-change for {discord_id}"));
    }
    tracing::warn!(
        discord_id = %discord_id,
        "OSL: TOFU — user DECLINED peer key change; old baseline kept \
         (alert re-raises on next fetch while the key differs)"
    );
    Ok(())
}

/// Safety number for a peer's CURRENT trusted Ed25519 baseline
/// (peer_map `tofu_ed25519_pub`). For out-of-band verification in
/// the whitelist/peer UI. Errors if the peer has no recorded key.
pub fn cmd_osl_peer_safety_number(
    state: &AppState,
    discord_id: String,
) -> Result<String, String> {
    let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
    let entry = pm
        .get(&discord_id)
        .ok_or_else(|| format!("OSL: unknown peer {discord_id}"))?;
    let key = entry
        .tofu_ed25519_pub
        .as_deref()
        .ok_or_else(|| format!("OSL: no trusted key for {discord_id} yet"))?;
    Ok(crate::tofu::safety_number(key))
}

/// Safety number for OUR OWN Ed25519 identity pub, so the user can
/// read it out to a peer for mutual out-of-band verification.
pub fn cmd_osl_self_safety_number(state: &AppState) -> Result<String, String> {
    let g = state.identity.lock().expect("identity mutex poisoned");
    let id = g.as_ref().ok_or_else(|| "OSL: identity not loaded".to_string())?;
    let b64 = STANDARD.encode(id.ed25519_public.as_bytes());
    Ok(crate::tofu::safety_number(&b64))
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
    // REGISTER-FIX: in V2 a peer's osl_user_id IS their Discord
    // snowflake (the keyserver is keyed by snowflake). A wiped /
    // re-whitelisted entry has osl_user_id=None — default to the
    // snowflake so a keyless entry self-heals instead of failing
    // forever (the old `return Ok(false)` here was the dead end).
    let osl_user_id = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(discord_id)
            .and_then(|e| e.osl_user_id.clone())
            .unwrap_or_else(|| discord_id.to_string())
    };
    let resp = {
        let ks_guard = state.keyserver.lock().expect("keyserver mutex poisoned");
        let client = ks_guard
            .as_ref()
            .ok_or_else(|| "OSL: key-server not initialised".to_string())?;
        client
            .fetch_pubkeys(&osl_user_id)
            .map_err(|e| format!("OSL: keyserver fetch_pubkeys({osl_user_id}): {e}"))?
    };
    let added = populate_peer_from_fetch_response(state, discord_id, &resp)?;
    tracing::info!(
        discord_id = %discord_id,
        osl_user_id = %osl_user_id,
        ml_kem_added = added,
        "OSL: keyserver pubkey refresh"
    );
    Ok(added)
}

/// Apply a received burn marker:
/// - Append a [`crate::peer_map::BurnedScope`] entry to
///   `peer_map[sender].burned_scopes` (idempotent — duplicates
///   skipped).
/// - Wipe `wrapped_key` on matching rows in `messages.sqlite`
///   (best-effort; failures logged, not propagated).
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
    // Wipe sqlite wrapped_keys for the scope (best-effort).
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        let (scope_type, scope_id) = scope_storage_pair(&marker.scope);
        if let Err(e) = store.wipe_wrapped_keys_in_scope(&scope_type, &scope_id) {
            tracing::warn!(error = %e, "OSL: wipe wrapped_keys failed; burn proceeded in peer_map only");
        }
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
/// - Wipes `wrapped_key` on matching rows in messages.sqlite
///   (so we can't re-decrypt our outgoing history in `scope`).
/// - **Does not** mutate any peer_map entry. Peer-side burn
///   tracking lives in their burned_scopes; ours is implicit
///   via the wiped wrapped_keys + the scope no longer being in
///   whitelist_state.
pub fn cmd_osl_apply_burn(
    state: &AppState,
    scope_input: crate::scope::ScopeInput,
) -> Result<(), String> {
    let scope: crate::scope::Scope = scope_input
        .try_into()
        .map_err(|e: crate::scope::ScopeError| format!("OSL: {e}"))?;
    let (scope_type, scope_id) = scope_storage_pair(&scope);
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
    {
        store
            .wipe_wrapped_keys_in_scope(&scope_type, &scope_id)
            .map_err(|e| format!("OSL: wipe wrapped_keys: {e}"))?;
    }
    Ok(())
}

// 9-C1: `cmd_osl_accept_invitation` / `cmd_osl_decline_invitation`
// / `apply_invitation_decision` removed alongside the invitation
// handshake.

/// Remove a whitelist entry for `peer` in `scope`. Returns the
/// wire-format burn marker the caller must send through Discord's
/// API so the peer's client wipes its decrypt capability.
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
/// - Call `cmd_osl_apply_burn` to wipe wrapped_keys.
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
    // in-Discord burn path: WITH the wrapped-keys wipe — byte-
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
/// Adjustment (operator decision): this path also does NOT wipe OUR
/// local wrapped keys — after "Remove" the operator can still read
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
/// (conditionally) the wrapped-keys wipe, and the atomic persist of
/// both files. Keeping this in one place is the "cannot drift"
/// guarantee: the two callers run byte-identical local effects and
/// differ in EXACTLY two orthogonal axes —
///   1. wire emission: only `cmd_osl_unwhitelist_scope` builds one
///      (its step 1, before this helper);
///   2. `wipe_local_decrypt`: `true` for the in-Discord burn path
///      (preserves today's behaviour exactly), `false` for the
///      settings local path (operator keeps local read history).
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

    // 4. Wipe wrapped_keys — ONLY when the caller is the in-Discord
    //    burn path. This is the SOLE behavioural difference between
    //    the two callers. `cmd_osl_apply_burn` calls
    //    `store.wipe_wrapped_keys_in_scope`, which NULLs the local
    //    `wrapped_key` rows for the scope — that is precisely what
    //    destroys OUR ability to re-decrypt the scope's old
    //    messages. Local decrypt is gated solely by wrapped-key
    //    presence (see `cmd_osl_apply_burn` docs); the `BurnedScope`
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
        // REGISTER-FIX: seed osl_user_id (== the peer's Discord
        // snowflake == their keyserver user_id in V2) so the
        // keyserver fetch below — and every later send/receive —
        // can resolve this peer. Without this, a whitelisted peer
        // stayed permanently keyless (encrypt-to-self-only on send,
        // UnknownSender on receive).
        pe.osl_user_id
            .get_or_insert_with(|| peer_discord_id.clone());
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
        let ws = ws_guard
            .entry(scope.storage_key())
            .or_insert_with(crate::whitelist_state::ScopeState::default);
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
    }

    // 7d-FIX1: persist BOTH files. Encryption-at-rest is applied
    // transparently by write_peer_map / write_whitelist_state via
    // `maybe_encrypt` when a main password is set.
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    // REGISTER-FIX: pull the peer's real keys (x25519 + ML-KEM
    // [+ ratchet]) from the keyserver NOW so whitelisting yields a
    // key-complete entry — the user shouldn't have to send/receive
    // first to trigger a lazy fetch. Best-effort: if the keyserver
    // is unreachable / not yet installed at this moment, the
    // entry's seeded osl_user_id lets the send-path's
    // PeerMissingKeys → refresh retry self-heal later. A failure
    // here must NOT fail the whitelist op.
    match refresh_peer_pubkeys_from_keyserver(state, &peer_discord_id) {
        Ok(_) => {
            persist_peer_map_now(state);
        }
        Err(e) => {
            tracing::info!(
                peer = %peer_discord_id,
                error = %e,
                "OSL: whitelist-time keyserver key fetch deferred \
                 (will self-heal on next send/receive)"
            );
        }
    }

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
            // REGISTER-FIX: seed osl_user_id (== snowflake in V2) so
            // the post-loop keyserver refresh + later send/receive
            // can resolve each bulk-whitelisted peer.
            pe.osl_user_id.get_or_insert_with(|| did.clone());
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
        let ws = ws_guard
            .entry(scope.storage_key())
            .or_insert_with(crate::whitelist_state::ScopeState::default);
        ws.encrypt_toggle = true;
        ws.auto_enabled = true;
    }
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);

    // REGISTER-FIX: best-effort keyserver key fetch for every
    // bulk-whitelisted peer so the entries are key-complete (same
    // rationale as cmd_osl_set_whitelist). Per-peer failures are
    // non-fatal — the seeded osl_user_id lets the send-path
    // PeerMissingKeys → refresh retry self-heal later.
    let mut any_fetched = false;
    for did in &member_dids {
        if refresh_peer_pubkeys_from_keyserver(state, did).is_ok() {
            any_fetched = true;
        }
    }
    if any_fetched {
        persist_peer_map_now(state);
    }
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
    let entry = ws_guard
        .entry(key)
        .or_insert_with(crate::whitelist_state::ScopeState::default);
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
    let entry = ws_guard
        .entry(key)
        .or_insert_with(crate::whitelist_state::ScopeState::default);
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
         snowflake to {{\"osl_user_id\":\"{osl_user_id}\"}}"
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
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
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
///   - Ok(response)               → save cache, bump
///                                  last_validated_at to now()
///   - Err(Transport)             → DO NOT touch cache (keyserver
///                                  unreachable; F2.4 honours the
///                                  stale cache during 7-day grace)
///   - Err(HttpStatus / Json)     → DO NOT touch cache (keyserver
///                                  answered, treat cache as stale)
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
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
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

/// Built-in production keyserver. Used when `keyserver.json` is
/// absent or carries no `base_url` (the fresh-install case — the
/// installer/onboarding never writes that file). `keyserver.json`
/// remains an OVERRIDE for dev/staging; it is no longer required
/// for the client to function.
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

/// Resolve the keyserver base URL: the `keyserver.json` override
/// when present and well-formed, otherwise the built-in production
/// default. This is the function the license paths use so a fresh
/// install (no `keyserver.json`) still reaches prod instead of
/// failing closed with "keyserver not configured".
pub fn resolve_keyserver_base_url(dir: &std::path::Path) -> String {
    read_keyserver_base_url(dir).unwrap_or_else(|| DEFAULT_KEYSERVER_BASE_URL.to_string())
}

/// Best-effort read of `<config_dir>/keyserver.json` → `admin_token`.
/// `keyserver.json` is an OVERRIDE only (dev/staging); a fresh
/// production install has no such file and registers against an
/// unsecured-route prod keyserver with no token. Empty string is
/// treated as absent (mirrors `KeyServerClient::with_admin_token`).
fn read_keyserver_admin_token(dir: &std::path::Path) -> Option<String> {
    let path = dir.join("keyserver.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("admin_token")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
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
pub fn ensure_keyserver_registered(
    state: &AppState,
    base_url: &str,
    admin_token: Option<String>,
) {
    // Identity gate — nothing to register until one exists.
    {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        if id_guard.is_none() {
            tracing::info!(
                "OSL: ensure_keyserver_registered: no identity in state; \
                 skipping register (will retry on next unlock once the \
                 Discord-snowflake identity exists)"
            );
            return;
        }
    }

    // Pure construction (no IO) — safe to build even when a client is
    // already installed; we use it for the register attempt and only
    // conditionally adopt it as the installed client below.
    let client = match KeyServerClient::new(base_url) {
        Ok(c) => c.with_admin_token(admin_token.clone()),
        Err(e) => {
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
    let pending_rotation: Option<keystore::PendingRotation> =
        match pending_rotation_path.as_ref() {
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
            tracing::info!(
                user_id = %resp.user_id,
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
                        Err(e) => tracing::warn!(
                            error = %e,
                            "OSL: ensure_keyserver_registered: plain register \
                             fallback failed (non-fatal; proof kept, retried \
                             on next unlock / launch)"
                        ),
                    }
                }
                Err(e) => tracing::warn!(
                    error = %e,
                    "OSL: ensure_keyserver_registered: register_with_rotation \
                     failed (non-fatal; proof kept, retried on next unlock / \
                     launch)"
                ),
            }
        } else {
        match client.register(id) {
            Ok(resp) => {
                tracing::info!(
                    user_id = %resp.user_id,
                    initial = resp.registered_at.is_some(),
                    status = resp.status.as_deref().unwrap_or(
                        if resp.registered_at.is_some() { "registered" } else { "ok" }
                    ),
                    "OSL: ensure_keyserver_registered: registered with key-server"
                );
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
            }
            // REGISTER-FIX: the ONE response we must NOT warn-swallow.
            // 403 = our user_id is held by a DIFFERENT Ed25519 key
            // (someone squatted our snowflake, or we lost our key).
            // Peers will fetch the other key and be unable to talk to
            // us / could be MITM'd. Raise a blocking, user-visible
            // alert + log at error, not warn.
            Err(keystore::Error::HttpStatus { status: 403, body }) => {
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
            Err(e) => tracing::warn!(
                error = %e,
                "OSL: ensure_keyserver_registered: key-server register \
                 failed (non-fatal; retried on next unlock / launch)"
            ),
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

pub fn cmd_osl_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(crate::main_password::password_status(&dir))
}

pub fn cmd_osl_set_main_password(password: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_main_password(&dir, &password)
}

pub fn cmd_osl_change_main_password(current: String, new: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::change_main_password(&dir, &current, &new)
}

pub fn cmd_osl_remove_main_password(current: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_main_password(&dir, &current)
}

pub fn cmd_osl_view_recovery_phrase(current: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::view_recovery_phrase(&dir, &current)
}

pub fn cmd_osl_verify_main_password(password: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::verify_main_password(&dir, &password)
}

pub fn cmd_osl_verify_recovery_phrase(state: &AppState, phrase: String) -> Result<String, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::verify_recovery_phrase(state, &dir, &phrase)
}

pub fn cmd_osl_set_main_password_after_recovery(
    state: &AppState,
    new_password: String,
    token: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_main_password_after_recovery(state, &dir, &new_password, &token)
}

pub fn cmd_osl_lockout_status() -> Result<LockoutStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(crate::main_password::lockout_status(&dir))
}

// =====================================================================
// Phase 7d-B2: stealth password operations.
// =====================================================================

pub fn cmd_osl_set_stealth_password(
    current_main: String,
    new_stealth: String,
) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_stealth_password(&dir, &current_main, &new_stealth)
}

pub fn cmd_osl_remove_stealth_password(current_main: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_stealth_password(&dir, &current_main)
}

pub fn cmd_osl_stealth_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    Ok(PasswordStatusDto {
        is_set: crate::main_password::stealth_password_status(&dir),
    })
}

// =====================================================================
// Phase 7d-B3: burn password operations.
// =====================================================================

pub fn cmd_osl_set_burn_password(current_main: String, new_burn: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::set_burn_password(&dir, &current_main, &new_burn)
}

pub fn cmd_osl_remove_burn_password(current_main: String) -> Result<(), String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
    crate::main_password::remove_burn_password(&dir, &current_main)
}

pub fn cmd_osl_burn_password_status() -> Result<PasswordStatusDto, String> {
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
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
    let dir =
        keystore::osl_config_dir().map_err(|e| format!("OSL: cannot resolve config dir: {e}"))?;
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
            match crate::state_reload::reload_encrypted_state_after_unlock(state, &dir) {
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
                read_keyserver_admin_token(&dir),
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
                 deferred to a later cleanup pass, see 7d-D spec"
            ));
        }
        "server_full" | "server_full_per_user" => {
            return Err(format!(
                "OSL: burn_scope_data: server_full burn not yet implemented (scope_id={scope_id}) — \
                 deferred, see 7d-D spec"
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
    let entry = BurnedScopeEntry {
        scope_kind: scope_kind.clone(),
        scope_id: scope_id.clone(),
        server_id,
        channel_id,
        burned_at: now as i64,
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
            crate::scope::ScopeKind::ServerChannel => {
                match (&scope.server_id, &scope.channel_id) {
                    (Some(srv), Some(chan)) => {
                        m.note_server_channel_members(srv, chan, &member_ids);
                        true
                    }
                    _ => false,
                }
            }
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
    /// `ServerDefaults.server_header_whitelisted` for the server.
    pub server_header: bool,
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
    let server_header = state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned")
        .get(&server_id)
        .map(|d| d.server_header_whitelisted)
        .unwrap_or(false);
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
        channel,
        channel_encrypt,
    })
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
        return Err(
            "OSL: set_channel_whitelist requires a server_channel or gc scope".to_string(),
        );
    }
    {
        let mut ws = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let entry = ws
            .entry(scope.storage_key())
            .or_insert_with(crate::whitelist_state::ScopeState::default);
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
            // Probe-2 Rust Bug 4: previously this path left
            // `osl_user_id`/`pubkey`/`ik_mlkem768_pub`/
            // `ik_ratchet_initial_pub` all None — every freshly
            // bulk-whitelisted peer hit `PeerMissingKeys` on the
            // first send. The v=3 path's refresh-on-error retry
            // sometimes rescued it (only because
            // `refresh_peer_pubkeys_from_keyserver` defaults
            // `osl_user_id` to the snowflake), but if keyserver
            // was unreachable at first send the entire bulk set
            // was dead. Seed `osl_user_id` synchronously here so
            // the resolver has a complete identifier; the keyserver
            // refresh after the locks drop populates the rest.
            if pe.osl_user_id.is_none() {
                pe.osl_user_id = Some(did.clone());
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
            let entry = ws_guard
                .entry(scope.storage_key())
                .or_insert_with(crate::whitelist_state::ScopeState::default);
            entry.encrypt_toggle = true;
            entry.auto_enabled = true;
        }
    }
    persist_peer_map_now(state);
    persist_whitelist_state_now(state);
    // Probe-2 Rust Bug 4: best-effort proactive keyserver refresh so
    // the first send to each peer has X25519 / ML-KEM / ratchet pubs
    // already populated. Failures are logged + swallowed — the v=3
    // send-side refresh-on-error path is still the safety net.
    for did in &member_dids {
        if let Err(e) = refresh_peer_pubkeys_from_keyserver(state, did) {
            tracing::debug!(
                discord_id = %did,
                error = %e,
                "OSL: bulk_set_dm_whitelist: proactive keyserver refresh \
                 failed (non-fatal; first send will retry)"
            );
        }
    }
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
        sd.entry(server_id.clone())
            .or_insert_with(crate::whitelist_state::ServerDefaults::default)
            .encrypt_by_default = encrypt_by_default;
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
            let entry = ws
                .entry(scope.storage_key())
                .or_insert_with(crate::whitelist_state::ScopeState::default);
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
    // — the user re-saw the tour intro every launch. Default to
    // `keystore::osl_config_dir()` so the no-arg path is functional;
    // an explicit `config_dir` (test path) still wins.
    let dir = match config_dir {
        Some(d) => d,
        None => match keystore::osl_config_dir() {
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

    // Preserve the user_id (Discord snowflake) across the burn so the
    // post-burn re-registration ties the new key to the same account.
    let preserved_user_id = state
        .identity
        .lock()
        .expect("identity mutex poisoned")
        .as_ref()
        .map(|id| id.user_id.clone())
        .unwrap_or_default();

    // Route through the canonical fresh-start path so the pre-signed
    // Case-C rotation proof is minted+persisted while the old Ed25519
    // secret still exists in memory. This is the WHOLE POINT of the
    // a4dfc44 forward fix; without this routing it never ran.
    let new_identity = crate::fresh_start::cmd_osl_fresh_start(&dir, preserved_user_id)
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
    *state.identity.lock().expect("identity mutex poisoned") = Some(new_identity);
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
        .expect("app_preferences mutex poisoned") = crate::app_preferences::AppPreferences::default();
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
