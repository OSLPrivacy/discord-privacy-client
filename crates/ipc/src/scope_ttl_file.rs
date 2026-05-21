//! Phase 3: `scope_ttl.json`.
//!
//! Per-scope cipher-store TTL preference. boot.js reads via
//! `osl_get_scope_ttl(scope)` immediately before each
//! `osl_prose_token_send`, replacing the prior 259200-second (72h)
//! hardcode at every send callsite.
//!
//! Defaults: missing entry → [`DEFAULT_TTL_SECONDS`] (72h). All
//! stored values are clamped to [`MIN_TTL_SECONDS`]..=[`MAX_TTL_SECONDS`]
//! (1h..=7d) per the roadmap.
//!
//! Mirrors the [`crate::burned_scopes_file`] persistence pattern:
//! small struct, atomic `.tmp + rename` write, OSL-ENC1 envelope via
//! `main_password::maybe_encrypt` when a file storage key is
//! configured.
//!
//! Covers every scope kind uniformly because [`Scope::storage_key`]
//! produces stable prefixes for dm / gc / server_channel / server_full.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 1 hour — minimum sensible TTL. Anything shorter risks the
/// receiver fetching after the blob has expired even on fast paths.
pub const MIN_TTL_SECONDS: u32 = 3_600;

/// 7 days — maximum TTL. The cipher-store itself enforces a
/// matching ceiling so values beyond this clamp down server-side.
pub const MAX_TTL_SECONDS: u32 = 604_800;

/// 72 hours — default for any scope without an explicit setting.
/// Matches the prior hardcoded value at every prose_token_send
/// callsite in boot.js.
pub const DEFAULT_TTL_SECONDS: u32 = 259_200;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeTtlFile {
    #[serde(default)]
    pub version: u32,
    /// Map of `Scope::storage_key()` → `ttl_seconds`. BTree for
    /// stable JSON ordering on disk.
    #[serde(default)]
    pub entries: BTreeMap<String, u32>,
}

pub fn load_scope_ttls(path: &Path) -> ScopeTtlFile {
    let Ok(blob) = std::fs::read(path) else {
        return ScopeTtlFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load scope_ttl.json decrypt failed");
            return ScopeTtlFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_scope_ttls(path: &Path, file: &ScopeTtlFile) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize scope_ttl: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt scope_ttl: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

pub fn clamp_ttl(seconds: u32) -> u32 {
    seconds.clamp(MIN_TTL_SECONDS, MAX_TTL_SECONDS)
}

/// Reads a per-scope TTL with bounds + default fallback applied.
/// Always returns a usable value; callers never have to handle
/// `Option`.
pub fn get_scope_ttl(file: &ScopeTtlFile, storage_key: &str) -> u32 {
    file.entries
        .get(storage_key)
        .copied()
        .map(clamp_ttl)
        .unwrap_or(DEFAULT_TTL_SECONDS)
}

/// Sets a per-scope TTL, clamping to bounds. Returns the effective
/// (post-clamp) value so the caller can echo it back to the UI.
pub fn set_scope_ttl(file: &mut ScopeTtlFile, storage_key: String, seconds: u32) -> u32 {
    let clamped = clamp_ttl(seconds);
    file.entries.insert(storage_key, clamped);
    clamped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_missing_scope() {
        let f = ScopeTtlFile::default();
        assert_eq!(get_scope_ttl(&f, "gc:123"), DEFAULT_TTL_SECONDS);
    }

    #[test]
    fn clamp_below_min() {
        let mut f = ScopeTtlFile::default();
        let stored = set_scope_ttl(&mut f, "dm:abc".into(), 60);
        assert_eq!(stored, MIN_TTL_SECONDS);
        assert_eq!(get_scope_ttl(&f, "dm:abc"), MIN_TTL_SECONDS);
    }

    #[test]
    fn clamp_above_max() {
        let mut f = ScopeTtlFile::default();
        let stored = set_scope_ttl(&mut f, "dm:abc".into(), 10_000_000);
        assert_eq!(stored, MAX_TTL_SECONDS);
    }

    #[test]
    fn round_trip_in_bounds() {
        let mut f = ScopeTtlFile::default();
        set_scope_ttl(&mut f, "gc:456".into(), 86_400);
        assert_eq!(get_scope_ttl(&f, "gc:456"), 86_400);
    }

    #[test]
    fn handles_all_scope_kinds() {
        let mut f = ScopeTtlFile::default();
        set_scope_ttl(&mut f, "dm:1".into(), 7200);
        set_scope_ttl(&mut f, "gc:2".into(), 7200);
        set_scope_ttl(&mut f, "server_channel:3:4".into(), 7200);
        set_scope_ttl(&mut f, "server_full:5".into(), 7200);
        assert_eq!(f.entries.len(), 4);
    }
}
