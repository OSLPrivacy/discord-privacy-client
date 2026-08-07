//! Phase 7d-FIX1: `burned_scopes.json`.
//!
//! Tracks which scopes the user has explicitly burned via the
//! channel-header burn button. The boot.js receive observer
//! consults this list (via `osl_list_burned_scopes` at install
//! time) and skips decrypt dispatch for any message whose scope
//! is in here — old DPC0:: ciphertext stays as ciphertext in the
//! UI, no re-decrypt loop.
//!
//! Lives in a SEPARATE file rather than as a field on
//! `whitelist_state.json` because that file's JSON shape is a
//! flat HashMap<scope_storage_key, ScopeState> — adding a
//! top-level array would require a struct wrapper that breaks
//! every existing `.insert()` / `.get()` callsite. Same on-disk
//! encryption-at-rest path via `main_password::maybe_encrypt` so
//! the file follows the rest of the 7d-B4 (scoped) treatment.
//!
//! Entries are removed when:
//! - The user re-whitelists the same scope (set_whitelist evicts
//!   matching entries — already implemented in 7d-B2 for the
//!   per-peer burned_scopes, here we mirror to the global list).
//! - The user explicitly removes via `osl_unburn_scope` (e.g.
//!   from a future settings UI).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub const BURNED_SCOPES_FILE_NAME: &str = "burned_scopes.json";

/// The burned-scope ledger lives beside the rest of the account's saved state
/// files and is sealed by the same file-storage key as those files.
pub fn path_in_config_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(BURNED_SCOPES_FILE_NAME)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BurnedScopesFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub scopes: Vec<BurnedScopeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BurnedScopeEntry {
    pub scope_kind: String,
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    pub burned_at: i64,
    // 9-A1c: defense-in-depth burn kill list. Discord message IDs
    // present in the channel at burn time are recorded here and
    // checked at decrypt-entry; even if the per-channel skip cache
    // is later cleared (manual re-engage), these specific messages
    // remain blocked from decryption.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub burned_message_ids: Vec<String>,
}

/// A1-1 / fail-closed: set once a `burned_scopes.json` that EXISTS could not be
/// decrypted or parsed. See `load_burned_scopes` for why this is a latch and
/// not a per-call return value.
static BURN_STATE_UNREADABLE: AtomicBool = AtomicBool::new(false);

/// True when this process has seen a burn kill-list it could not read.
///
/// While set, `is_message_in_burn_kill_list` treats EVERY message as still
/// burned and `write_burned_scopes` refuses to write. Callers must not
/// interpret an empty in-memory list as "nothing is burned" while this holds.
pub fn burn_state_unreadable() -> bool {
    BURN_STATE_UNREADABLE.load(Ordering::SeqCst)
}

/// Test-only escape hatch: the latch is a process global, so a test that
/// deliberately corrupts a kill list has to put the process back.
pub fn reset_burn_state_unreadable_for_tests() {
    BURN_STATE_UNREADABLE.store(false, Ordering::SeqCst);
}

/// Clear the fail-closed latch because the account it was protecting has been
/// destroyed and replaced.
///
/// This is the ONLY non-test way out of the latch, and it is deliberately
/// bound to the fresh-start path: that path mints a new identity, so nothing
/// the old kill list covered is decryptable by the new account anyway, and
/// there is therefore no burn promise left to keep. Without it a user whose
/// kill list was corrupted or deleted would be stuck — every message blocked,
/// every launch, forever — with the product's own reset button unable to help
/// them, which is a worse failure than the one this file guards against.
pub fn clear_burn_state_for_replaced_account() {
    BURN_STATE_UNREADABLE.store(false, Ordering::SeqCst);
}

/// Load the burn kill list.
///
/// FAIL CLOSED. This used to swallow a decrypt failure and return an EMPTY
/// `BurnedScopesFile`, which reads downstream as "no scope was ever burned":
/// every burned conversation silently became decryptable again, and the next
/// write persisted the empty list, making the loss permanent. That is how a
/// key-derivation bug in `change_main_password` turned into "changing your
/// password un-burns every burned message".
///
/// The safe reading of "I cannot open the kill list" is EVERYTHING IS STILL
/// BURNED, not "nothing is burned" — a burn is a promise not to decrypt, and a
/// promise you cannot read is not a promise you may ignore. Because the
/// consumers iterate the list rather than query it, "everything" cannot be
/// expressed as rows; it is expressed as the `BURN_STATE_UNREADABLE` latch,
/// which `is_message_in_burn_kill_list` honours by returning `true`
/// unconditionally and which blocks writes so the unreadable file on disk is
/// never overwritten by an empty one.
///
/// TA-T10-003a closed the matching hole on the MISSING-file path. "The kill
/// list is not there" had exactly one reading — fresh install — and so
/// deleting `burned_scopes.json` was a one-step un-burn: the loader returned
/// an empty ledger, `is_message_in_burn_kill_list` answered `false` for every
/// message, and the next write persisted the empty list. "I cannot read the
/// promise" and "the promise is gone" are the same situation for the user
/// whose messages are at stake, so they now get the same answer. The two are
/// told apart by the account's burn-ledger enrolment marker
/// (`whitelist_state.json`), which is set the first time a burn is recorded
/// and cleared only by `fresh_start` deleting the file:
///
/// - marker absent  → no burn was ever recorded → genuine first run, stay open;
/// - marker present → the ledger existed and is gone → fail closed;
/// - marker unreadable → no conclusion (pre-gate bootstrap has no file key
///   yet); the post-unlock `state_reload` pass re-runs this with the key
///   installed and decides then.
pub fn load_burned_scopes(path: &Path) -> BurnedScopesFile {
    let Ok(blob) = std::fs::read(path) else {
        // Absent file. Deliberately does NOT clear the latch — deleting an
        // unreadable kill list must not be a way to unburn.
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        if crate::whitelist_state::burn_ledger_enrollment(dir)
            == crate::whitelist_state::BurnLedgerEnrollment::Enrolled
        {
            tracing::error!(
                path = %crate::log_id::redact_path(path),
                "OSL: burned_scopes.json is missing but this account has recorded burns — \
                 treating all scopes as still burned"
            );
            BURN_STATE_UNREADABLE.store(true, Ordering::SeqCst);
        }
        return BurnedScopesFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "OSL: burned_scopes.json unreadable — treating all scopes as still burned");
            BURN_STATE_UNREADABLE.store(true, Ordering::SeqCst);
            return BurnedScopesFile::default();
        }
    };
    match serde_json::from_slice::<BurnedScopesFile>(&plain) {
        Ok(file) => {
            BURN_STATE_UNREADABLE.store(false, Ordering::SeqCst);
            file
        }
        Err(e) => {
            tracing::error!(error = %e, "OSL: burned_scopes.json unparseable — treating all scopes as still burned");
            BURN_STATE_UNREADABLE.store(true, Ordering::SeqCst);
            BurnedScopesFile::default()
        }
    }
}

pub fn write_burned_scopes(path: &Path, file: &BurnedScopesFile) -> Result<(), String> {
    if burn_state_unreadable() {
        return Err(
            "OSL: refusing to write burned_scopes.json — the existing kill list could not be \
             read, so writing would replace it with an incomplete list and un-burn messages"
                .to_string(),
        );
    }
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize burned_scopes: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt burned_scopes: {e}"))?;
    crate::recoverable_file::write_recoverable(path, &out)
        .map_err(|e| format!("OSL: recoverable write {}: {e}", path.display()))?;
    Ok(())
}
