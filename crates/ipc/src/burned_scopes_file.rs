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
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

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
/// never overwritten by an empty one. A missing file is still the ordinary
/// fresh-install case and clears nothing.
pub fn load_burned_scopes(path: &Path) -> BurnedScopesFile {
    let Ok(blob) = std::fs::read(path) else {
        // Absent file: fresh install. Deliberately does NOT clear the latch —
        // deleting an unreadable kill list must not be a way to unburn.
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
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}
