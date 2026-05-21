//! Phase 4: `scope_blobs.json`.
//!
//! Per-scope list of cipher-store blob IDs this client has uploaded.
//! The scope-burn flow walks the list and calls
//! [`prose_token_burn_id`] on each ID, instantly making every cover
//! that scope produced un-decryptable for everyone — the burner's
//! own client included, since the on-server blob is gone.
//!
//! Recording happens inside the `osl_prose_token_send` Tauri command
//! every time a cover is built (V2 content sends + SKDM/burn-marker
//! covers). Walking + clearing happens inside `osl_scope_burn_blobs`,
//! invoked by boot.js's existing burn-button flow right between the
//! burn-marker send and the local `osl_apply_burn`.
//!
//! Mirrors the [`crate::burned_scopes_file`] /
//! [`crate::scope_ttl_file`] persistence pattern: atomic
//! `.tmp + rename` write + `main_password::maybe_encrypt` envelope.
//!
//! [`prose_token_burn_id`]: crate::prose_token::prose_token_burn_id

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeBlobsFile {
    #[serde(default)]
    pub version: u32,
    /// `Scope::storage_key()` → list of `blob_id` hex strings.
    /// BTree for stable JSON ordering.
    #[serde(default)]
    pub entries: BTreeMap<String, Vec<String>>,
}

pub fn load(path: &Path) -> ScopeBlobsFile {
    let Ok(blob) = std::fs::read(path) else {
        return ScopeBlobsFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load scope_blobs.json decrypt failed");
            return ScopeBlobsFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write(path: &Path, file: &ScopeBlobsFile) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize scope_blobs: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt scope_blobs: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

/// Append `blob_id` to `storage_key`'s list. Duplicates are
/// suppressed so a retried send doesn't bloat the file or cause
/// double-DELETE attempts on burn.
pub fn record_blob(file: &mut ScopeBlobsFile, storage_key: String, blob_id: String) {
    let entry = file.entries.entry(storage_key).or_default();
    if !entry.contains(&blob_id) {
        entry.push(blob_id);
    }
}

/// Removes and returns every blob_id recorded under `storage_key`.
/// Used by the scope-burn flow: take the list, iterate the burns
/// outside the mutex/file lifetime, then persist the cleared state.
pub fn take_blobs(file: &mut ScopeBlobsFile, storage_key: &str) -> Vec<String> {
    file.entries.remove(storage_key).unwrap_or_default()
}

/// Read-only count for diagnostics. Returns 0 when no entry exists.
pub fn count_for(file: &ScopeBlobsFile, storage_key: &str) -> usize {
    file.entries.get(storage_key).map(|v| v.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_take_round_trip() {
        let mut f = ScopeBlobsFile::default();
        record_blob(&mut f, "gc:1".into(), "aa00".into());
        record_blob(&mut f, "gc:1".into(), "bb11".into());
        record_blob(&mut f, "dm:2".into(), "cc22".into());
        let gc_blobs = take_blobs(&mut f, "gc:1");
        assert_eq!(gc_blobs, vec!["aa00".to_string(), "bb11".to_string()]);
        assert!(f.entries.get("gc:1").is_none());
        // Untouched entry survives.
        assert_eq!(count_for(&f, "dm:2"), 1);
    }

    #[test]
    fn record_dedupes() {
        let mut f = ScopeBlobsFile::default();
        record_blob(&mut f, "gc:1".into(), "aa00".into());
        record_blob(&mut f, "gc:1".into(), "aa00".into());
        record_blob(&mut f, "gc:1".into(), "aa00".into());
        assert_eq!(count_for(&f, "gc:1"), 1);
    }

    #[test]
    fn take_empty_scope_returns_vec() {
        let mut f = ScopeBlobsFile::default();
        let v = take_blobs(&mut f, "gc:missing");
        assert!(v.is_empty());
    }

    #[test]
    fn handles_all_scope_kinds() {
        let mut f = ScopeBlobsFile::default();
        record_blob(&mut f, "dm:1".into(), "aa".into());
        record_blob(&mut f, "gc:2".into(), "bb".into());
        record_blob(&mut f, "server_channel:3:4".into(), "cc".into());
        record_blob(&mut f, "server_full:5".into(), "dd".into());
        assert_eq!(f.entries.len(), 4);
    }
}
