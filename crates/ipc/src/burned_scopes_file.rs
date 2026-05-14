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

pub fn load_burned_scopes(path: &Path) -> BurnedScopesFile {
    let Ok(blob) = std::fs::read(path) else {
        return BurnedScopesFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load burned_scopes.json decrypt failed");
            return BurnedScopesFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_burned_scopes(path: &Path, file: &BurnedScopesFile) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize burned_scopes: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt burned_scopes: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}
