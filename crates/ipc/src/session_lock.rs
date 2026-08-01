//! A7: the session lock that actually locks.
//!
//! # Why this module exists
//!
//! OSL advertises an auto-lock. What shipped did not lock anything that
//! matters. Two independent mechanisms had grown up in `main_password`:
//!
//! * [`crate::main_password::lock_main_password_session`] — clears the file
//!   storage key, the identity, the prekey pool, the sender-pubkey cache and
//!   the `MessageStore`. It had **zero non-test callers**.
//! * `run_file_key_inactivity_auto_lock_timer` — reachable from production
//!   (every `get_file_storage_key`), but it clears **only the file storage
//!   key**. The identity secret, the peer map (the trust root), the whitelist,
//!   the sender-key chains and the open `MessageStore` all stayed live, so a
//!   "locked" OSL still decrypted messages.
//!
//! This module supersedes both. [`lock_session`] drops and zeroizes every
//! live secret in [`AppState`], closes the `MessageStore`, and clears the file
//! storage key; [`unlock_session`] re-establishes that state from sealed disk
//! storage so locking is not a one-way trip that bricks the session.
//!
//! # The idle clock
//!
//! `main_password` owns a process-global inactivity timer, but its expiry
//! action is hard-wired to "clear the file key" and its slot is private. This
//! module keeps its own last-activity instant, fed by the same command-entry
//! hook every IPC command already calls, and runs the *full* lock on expiry.
//!
//! The clock is **armed only by an unlock** ([`arm_idle_lock`]). An install
//! that never unlocked anything is never auto-locked, which is what keeps the
//! no-main-password device-fallback configuration working.
//!
//! Activity **latches**: once the idle window has elapsed, later activity does
//! not reopen the window. Otherwise any background command could hold a stale
//! session open forever without a human present.

use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::AppState;

/// Idle window before the session locks itself. Same 15 minutes the
/// password-gate timer used; `docs/design/unlock-and-duress.md` specifies
/// "after 15 minutes of inactivity (re-prompt)".
pub const SESSION_IDLE_LOCK_SECONDS: u64 = keystore::DEFAULT_INACTIVITY_SECONDS;

/// Error surfaced to any caller that reaches a secret-bearing command while
/// the session is locked. Deliberately stable text: the UI matches on it to
/// decide whether to raise the password gate.
pub const SESSION_LOCKED_ERROR: &str = "OSL: session is locked — re-enter your main password";

/// What caused a lock. Recorded for the audit log; every variant performs the
/// identical wipe, because a lock that is weaker for some triggers than others
/// is the defect this module exists to remove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLockTrigger {
    /// The user asked for it (Settings → Lock now).
    Manual,
    /// The idle window elapsed.
    Inactivity,
    /// The OS reported the desktop locked / the session disconnected.
    OsSessionLocked,
    /// A guarded flow (password change, account switch, burn) wants a clean
    /// slate before it proceeds.
    Internal,
}

impl SessionLockTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionLockTrigger::Manual => "manual",
            SessionLockTrigger::Inactivity => "inactivity",
            SessionLockTrigger::OsSessionLocked => "os_session_locked",
            SessionLockTrigger::Internal => "internal",
        }
    }
}

/// Measured outcome of one [`lock_session`] call.
///
/// Every field counts something that was actually taken out of memory, so a
/// regression that stops clearing one slot shows up as a number, not as a
/// comment that stopped being true.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SessionLockReport {
    pub trigger_was_noop: bool,
    pub identity_cleared: bool,
    pub prekey_state_cleared: bool,
    pub message_store_closed: bool,
    pub file_storage_key_cleared: bool,
    pub peer_entries_cleared: usize,
    pub peer_ratchet_states_zeroized: usize,
    pub whitelist_scopes_cleared: usize,
    pub server_defaults_cleared: usize,
    pub sender_key_chains_cleared: usize,
    pub burned_scopes_cleared: usize,
    pub scope_membership_cleared: bool,
    pub sender_pubkey_cache_cleared: bool,
    pub recovery_token_cleared: bool,
    pub key_change_alerts_cleared: usize,
    pub channel_members_cleared: usize,
    pub friend_ids_cleared: usize,
    pub guild_list_cleared: usize,
    pub reassembly_sessions_cleared: usize,
}

/// Measured outcome of one [`unlock_session`] call.
#[derive(Debug, Default, Clone)]
pub struct SessionUnlockReport {
    pub identity_reloaded: bool,
    pub message_store_reopened: bool,
    pub reload: crate::state_reload::ReloadReport,
}

// ---------------------------------------------------------------------------
// Idle clock
// ---------------------------------------------------------------------------

static LAST_ACTIVITY: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

fn last_activity_slot() -> &'static Mutex<Option<Instant>> {
    LAST_ACTIVITY.get_or_init(|| Mutex::new(None))
}

fn idle_window() -> Duration {
    Duration::from_secs(SESSION_IDLE_LOCK_SECONDS)
}

/// Start the idle clock for a freshly unlocked session. Until this runs, the
/// idle lock is inert — a never-unlocked process must not lock itself.
pub fn arm_idle_lock() {
    arm_idle_lock_at(Instant::now());
}

pub fn arm_idle_lock_at(now: Instant) {
    *last_activity_slot()
        .lock()
        .expect("session idle clock mutex poisoned") = Some(now);
}

/// Stop the idle clock. Called by [`lock_session`]: a locked session has
/// nothing left to lock, and a disarmed clock cannot fire again until the next
/// unlock re-arms it.
pub fn disarm_idle_lock() {
    *last_activity_slot()
        .lock()
        .expect("session idle clock mutex poisoned") = None;
}

/// True when the idle clock is running and its window has already elapsed.
pub fn idle_lock_is_due_at(now: Instant) -> bool {
    let slot = last_activity_slot()
        .lock()
        .expect("session idle clock mutex poisoned");
    match *slot {
        Some(last) => now.saturating_duration_since(last) >= idle_window(),
        None => false,
    }
}

/// Record user/command activity. Extends the idle window **unless it has
/// already elapsed**, in which case the session is due to lock and no amount
/// of later traffic may reopen it.
pub fn note_activity() {
    note_activity_at(Instant::now());
}

pub fn note_activity_at(now: Instant) {
    let mut slot = last_activity_slot()
        .lock()
        .expect("session idle clock mutex poisoned");
    match *slot {
        // Latched: the window elapsed, so this activity does not count.
        Some(last) if now.saturating_duration_since(last) >= idle_window() => {}
        Some(_) => *slot = Some(now),
        // Not armed — nothing to extend.
        None => {}
    }
}

/// Run the inactivity lock. Returns `true` when this call locked the session.
pub fn run_idle_session_lock(state: &AppState) -> bool {
    run_idle_session_lock_at(state, Instant::now())
}

pub fn run_idle_session_lock_at(state: &AppState, now: Instant) -> bool {
    if !idle_lock_is_due_at(now) {
        return false;
    }
    lock_session(state, SessionLockTrigger::Inactivity);
    true
}

// ---------------------------------------------------------------------------
// Lock
// ---------------------------------------------------------------------------

/// True while this state still holds live secret material. Used to decide
/// whether a lock is a real transition or a no-op, and by the command guard to
/// answer "is this session locked?" without duplicating the field list.
pub fn session_holds_live_secrets(state: &AppState) -> bool {
    if state.has_identity() {
        return true;
    }
    if crate::main_password::get_file_storage_key().is_some() {
        return true;
    }
    if state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .is_some()
    {
        return true;
    }
    if !state
        .peer_map
        .lock()
        .expect("peer_map mutex poisoned")
        .is_empty()
    {
        return true;
    }
    if !state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned")
        .is_empty()
    {
        return true;
    }
    if !state
        .sender_key_state
        .lock()
        .expect("sender_key_state mutex poisoned")
        .states
        .is_empty()
    {
        return true;
    }
    false
}

/// Drop and zeroize every live secret this process holds for the account, and
/// close the open [`store::MessageStore`].
///
/// This is the function every lock trigger must call. It is idempotent: a
/// second call on an already-locked state reports `trigger_was_noop` and
/// changes nothing.
pub fn lock_session(state: &AppState, trigger: SessionLockTrigger) -> SessionLockReport {
    let mut report = SessionLockReport {
        trigger_was_noop: !session_holds_live_secrets(state),
        ..SessionLockReport::default()
    };

    // 1. Close the message store first. It is the largest decrypting surface
    //    and it holds an open SQLite handle keyed by the identity secret;
    //    dropping it both closes the DB and releases that key copy. Do this
    //    before clearing the identity so no window exists where the store is
    //    open with no identity to justify it.
    if let Some(store) = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .take()
    {
        drop(store);
        report.message_store_closed = true;
    }

    // 2. Identity secret + prekey pool. `clear_identity` also clears prekeys;
    //    record both so a future split of those two shows up here.
    report.identity_cleared = state.has_identity();
    report.prekey_state_cleared = state.has_prekey_state();
    state.clear_identity();

    // 3. Peer map. This is the trust root AND the historical home of persisted
    //    per-peer Double Ratchet state. `RatchetStateOnDisk` is
    //    `ZeroizeOnDrop`, so taking each one and dropping it wipes the root
    //    key, chain keys and skipped message keys rather than releasing them
    //    to the allocator intact.
    {
        let mut pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        report.peer_entries_cleared = pm.len();
        for entry in pm.values_mut() {
            if let Some(ratchet) = entry.ratchet_state.take() {
                drop(ratchet);
                report.peer_ratchet_states_zeroized += 1;
            }
        }
        pm.clear();
    }

    // 4. Sender-key chains. These carry live group chain keys; a locked
    //    session that kept them could still derive message keys for every
    //    group the user is in.
    {
        let mut sk = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        report.sender_key_chains_cleared = sk.states.len();
        *sk = crate::sender_key_state::SenderKeyStateFile::default();
    }

    // 5. Policy state. Not key material, but it is the user's private social
    //    graph and their encrypt/burn policy; leaving it readable in a locked
    //    process is exactly the disclosure the lock is supposed to prevent.
    {
        let mut ws = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        report.whitelist_scopes_cleared = ws.len();
        ws.clear();
    }
    {
        let mut sd = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        report.server_defaults_cleared = sd.len();
        sd.clear();
    }
    {
        let mut burned = state
            .burned_scopes
            .lock()
            .expect("burned_scopes mutex poisoned");
        report.burned_scopes_cleared = burned.scopes.len();
        *burned = crate::burned_scopes_file::BurnedScopesFile::default();
    }
    {
        let mut membership = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        report.scope_membership_cleared = membership.observed_member_count() > 0;
        *membership = crate::membership::ScopeMembership::default();
    }

    // 6. Ephemeral caches that still describe who the user talks to.
    report.sender_pubkey_cache_cleared = true;
    state.sender_pubkey_cache.clear();
    {
        let mut alerts = state
            .key_change_alerts
            .lock()
            .expect("key_change_alerts mutex poisoned");
        report.key_change_alerts_cleared = alerts.len();
        alerts.clear();
    }
    {
        let mut members = state
            .channel_members
            .lock()
            .expect("channel_members mutex poisoned");
        report.channel_members_cleared = members.len();
        members.clear();
    }
    {
        let mut friends = state.friend_ids.lock().expect("friend_ids mutex poisoned");
        report.friend_ids_cleared = friends.len();
        friends.clear();
    }
    {
        let mut guilds = state.guild_list.lock().expect("guild_list mutex poisoned");
        report.guild_list_cleared = guilds.len();
        guilds.clear();
    }
    {
        // Half-assembled Mode 1 plaintext lives here.
        let mut reassembly = state
            .mode1_reassembly
            .lock()
            .expect("mode1_reassembly mutex poisoned");
        report.reassembly_sessions_cleared = reassembly.len();
        reassembly.clear();
    }

    // 7. One-time recovery token — it is a bearer credential for setting a new
    //    main password, so a locked screen must not still be holding one.
    {
        let mut token = state
            .recovery_token
            .lock()
            .expect("recovery_token mutex poisoned");
        report.recovery_token_cleared = token.take().is_some();
    }

    // 8. Finally the file storage key. `set_file_storage_key(None)` zeroizes
    //    the outgoing key bytes and disarms the password gate's own timer.
    report.file_storage_key_cleared = crate::main_password::get_file_storage_key().is_some();
    crate::main_password::set_file_storage_key(None);

    // 9. Stop our idle clock. The next unlock re-arms it.
    disarm_idle_lock();

    tracing::info!(
        trigger = trigger.as_str(),
        identity_cleared = report.identity_cleared,
        message_store_closed = report.message_store_closed,
        peer_entries_cleared = report.peer_entries_cleared,
        peer_ratchet_states_zeroized = report.peer_ratchet_states_zeroized,
        whitelist_scopes_cleared = report.whitelist_scopes_cleared,
        sender_key_chains_cleared = report.sender_key_chains_cleared,
        file_storage_key_cleared = report.file_storage_key_cleared,
        "OSL: session locked"
    );

    report
}

// ---------------------------------------------------------------------------
// Unlock
// ---------------------------------------------------------------------------

/// Re-establish the live session after a lock.
///
/// The caller must already have proven the main password and installed the
/// file storage key (that is what `cmd_osl_verify_main_password` does); this
/// refuses otherwise rather than half-restoring a session with no key.
///
/// Ordering matters: the identity has to come back before the message store,
/// because the store is sealed by the identity's X25519 secret.
pub fn unlock_session(
    state: &AppState,
    account_dir: &Path,
) -> Result<SessionUnlockReport, String> {
    if crate::main_password::get_file_storage_key().is_none() {
        return Err(SESSION_LOCKED_ERROR.to_string());
    }
    let mut report = SessionUnlockReport::default();

    // 1. Identity, from device-sealed storage. A missing identity.json is the
    //    legitimate "password set, identity not created yet" state, not a
    //    failure. An identity that is present but will not open IS a failure —
    //    silently continuing would leave the session half-unlocked.
    if !state.has_identity() {
        let identity_path = account_dir.join("identity.json");
        if identity_path.exists() {
            let sealer = keystore::select_best_sealer();
            let identity = keystore::load_identity(&identity_path, sealer.as_ref())
                .map_err(|e| format!("OSL: sealed identity could not be reopened: {e}"))?;
            *state.identity.lock().expect("identity mutex poisoned") = Some(identity);
            report.identity_reloaded = true;
        }
    }

    // 2. Encrypted-at-rest state (peer map, whitelist, server defaults, burn
    //    list, prekeys, sender keys, membership, preferences). Same loader the
    //    post-gate reload has always used, so file-format and migration
    //    semantics are unchanged.
    report.reload = crate::state_reload::reload_encrypted_state_after_unlock(state, account_dir)?;

    // 3. Message store, keyed off the now-loaded identity secret.
    report.message_store_reopened = reopen_message_store(state, account_dir);

    // 4. Restart the idle clock for the new session.
    arm_idle_lock();

    Ok(report)
}

/// Reopen `<account_dir>/store` under the loaded identity secret. Returns
/// whether a store is now installed.
///
/// The reopen goes through [`crate::commands::open_production_message_store`],
/// i.e. `MessageStore::open_anchored` with `KeystoreBackedAnchor::production()`
/// — the same path bootstrap and `MessageStorePause` use. This is not a
/// stylistic preference. `MessageStore::open` passes `provider: None`, which
/// (a) skips `AnchorBinding::reconcile_existing`, so a coherent SQLite
/// rollback/replay staged while the session was locked is never detected, and
/// (b) leaves the live store with `anchor == None`, so every write for the rest
/// of the session commits without advancing the keystore anchor — desynchronizing
/// the persisted anchor from the database for the next anchored open. An
/// unanchored reopen would therefore make "lock, then unlock" a way to strip
/// rollback protection off a running session.
///
/// A failed open is deliberately non-fatal and mirrors bootstrap: the decrypt
/// path swallows persistence errors, so a store outage must not take the
/// unlocked session down with it. It is NOT silently ignored — the caller can
/// see `message_store_reopened == false` and the warn is emitted here.
fn reopen_message_store(state: &AppState, account_dir: &Path) -> bool {
    let secret_bytes: [u8; 32] = {
        let guard = state.identity.lock().expect("identity mutex poisoned");
        match guard.as_ref() {
            Some(identity) => *identity.x25519_secret.as_bytes(),
            None => return false,
        }
    };
    let store_dir = account_dir.join("store");
    match crate::commands::open_production_message_store(&store_dir, &secret_bytes) {
        Ok(store) => {
            *state
                .message_store
                .lock()
                .expect("message_store mutex poisoned") = Some(store);
            true
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %store_dir.display(),
                "OSL: message_store could not be reopened after unlock; \
                 persistence stays disabled for this session"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn reset_clock() {
        disarm_idle_lock();
    }

    #[test]
    fn idle_lock_is_inert_until_armed() {
        let _guard = crate::test_process_globals::serialize();
        reset_clock();
        let now = Instant::now();
        assert!(
            !idle_lock_is_due_at(now + Duration::from_secs(SESSION_IDLE_LOCK_SECONDS * 10)),
            "a process that never unlocked must never auto-lock itself"
        );
        reset_clock();
    }

    #[test]
    fn activity_extends_the_window_until_it_latches() {
        let _guard = crate::test_process_globals::serialize();
        reset_clock();
        let t0 = Instant::now();
        arm_idle_lock_at(t0);

        let inside = t0 + Duration::from_secs(SESSION_IDLE_LOCK_SECONDS - 1);
        assert!(!idle_lock_is_due_at(inside));
        note_activity_at(inside);
        assert!(
            !idle_lock_is_due_at(t0 + Duration::from_secs(SESSION_IDLE_LOCK_SECONDS + 1)),
            "activity inside the window must push the deadline out"
        );

        let expired = inside + Duration::from_secs(SESSION_IDLE_LOCK_SECONDS);
        assert!(idle_lock_is_due_at(expired));
        note_activity_at(expired);
        assert!(
            idle_lock_is_due_at(expired),
            "once the window has elapsed, later traffic must not reopen the session"
        );
        reset_clock();
    }

    #[test]
    fn lock_is_idempotent_and_reports_a_noop_the_second_time() {
        let _guard = crate::test_process_globals::serialize();
        reset_clock();
        crate::main_password::set_file_storage_key(None);
        let state = AppState::new();
        state
            .peer_map
            .lock()
            .unwrap()
            .insert("900000000000000001".to_string(), Default::default());

        let first = lock_session(&state, SessionLockTrigger::Manual);
        assert!(!first.trigger_was_noop);
        assert_eq!(first.peer_entries_cleared, 1);

        let second = lock_session(&state, SessionLockTrigger::Manual);
        assert!(second.trigger_was_noop);
        assert_eq!(second.peer_entries_cleared, 0);
        reset_clock();
    }

    #[test]
    fn unlock_refuses_while_no_file_storage_key_is_installed() {
        let _guard = crate::test_process_globals::serialize();
        reset_clock();
        crate::main_password::set_file_storage_key(None);
        let state = AppState::new();
        let dir = std::env::temp_dir().join("osl-session-lock-refusal");
        let err = unlock_session(&state, &dir).expect_err("locked session must refuse to reopen");
        assert_eq!(err, SESSION_LOCKED_ERROR);
        reset_clock();
    }
}
