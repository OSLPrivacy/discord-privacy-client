//! Phase 3: `scope_ttl.json`.
//!
//! Per-scope cipher-store TTL preference. boot.js reads via
//! `osl_get_scope_ttl(scope)` immediately before each
//! `osl_prose_token_send`, replacing the prior 259200-second (72h)
//! hardcode at every send callsite.
//!
//! Defaults: missing entry → [`DEFAULT_TTL_SECONDS`] (72h). Values outside
//! [`MIN_TTL_SECONDS`]..=[`MAX_TTL_SECONDS`] (1h..=7d) are rejected so a
//! caller can report that the requested value cannot be honoured.
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
use std::fmt;
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

/// A TTL outside the cipher-store's supported range.  This is deliberately
/// returned to the caller rather than silently changing the requested value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeTtlBoundsError {
    pub requested: u32,
}

impl fmt::Display for ScopeTtlBoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TTL {} seconds is outside the supported range of {}..={} seconds",
            self.requested, MIN_TTL_SECONDS, MAX_TTL_SECONDS
        )
    }
}

impl std::error::Error for ScopeTtlBoundsError {}

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
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load scope_ttl.json decrypt failed");
            return ScopeTtlFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_scope_ttls(path: &Path, file: &ScopeTtlFile) -> Result<(), String> {
    let body =
        serde_json::to_vec_pretty(file).map_err(|e| format!("OSL: serialize scope_ttl: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt scope_ttl: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}

pub fn validate_ttl(seconds: u32) -> Result<u32, ScopeTtlBoundsError> {
    if !(MIN_TTL_SECONDS..=MAX_TTL_SECONDS).contains(&seconds) {
        return Err(ScopeTtlBoundsError { requested: seconds });
    }
    Ok(seconds)
}

/// Reads a per-scope TTL with the default fallback applied. An invalid value
/// already persisted on disk is surfaced rather than silently rewritten.
pub fn get_scope_ttl(file: &ScopeTtlFile, storage_key: &str) -> Result<u32, ScopeTtlBoundsError> {
    file.entries
        .get(storage_key)
        .copied()
        .map(validate_ttl)
        .unwrap_or(Ok(DEFAULT_TTL_SECONDS))
}

/// Sets a per-scope TTL if it can be honoured exactly. Invalid input does not
/// mutate the file, allowing the caller to report the unsupported request.
pub fn set_scope_ttl(
    file: &mut ScopeTtlFile,
    storage_key: String,
    seconds: u32,
) -> Result<u32, ScopeTtlBoundsError> {
    let ttl = validate_ttl(seconds)?;
    file.entries.insert(storage_key, ttl);
    Ok(ttl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_missing_scope() {
        let f = ScopeTtlFile::default();
        assert_eq!(get_scope_ttl(&f, "gc:123"), Ok(DEFAULT_TTL_SECONDS));
    }

    #[test]
    fn rejects_below_min_without_mutating_the_scope() {
        let mut f = ScopeTtlFile::default();
        let result = set_scope_ttl(&mut f, "dm:abc".into(), 60);
        assert_eq!(result, Err(ScopeTtlBoundsError { requested: 60 }));
        assert!(!f.entries.contains_key("dm:abc"));
    }

    #[test]
    fn rejects_above_max_without_overwriting_the_existing_value() {
        let mut f = ScopeTtlFile::default();
        set_scope_ttl(&mut f, "dm:abc".into(), 86_400).unwrap();
        let result = set_scope_ttl(&mut f, "dm:abc".into(), 10_000_000);
        assert_eq!(
            result,
            Err(ScopeTtlBoundsError {
                requested: 10_000_000
            })
        );
        assert_eq!(f.entries["dm:abc"], 86_400);
    }

    #[test]
    fn reports_an_invalid_persisted_value() {
        let mut f = ScopeTtlFile::default();
        f.entries.insert("dm:abc".into(), 60);
        assert_eq!(
            get_scope_ttl(&f, "dm:abc"),
            Err(ScopeTtlBoundsError { requested: 60 })
        );
    }

    #[test]
    fn round_trip_in_bounds() {
        let mut f = ScopeTtlFile::default();
        set_scope_ttl(&mut f, "gc:456".into(), 86_400).unwrap();
        assert_eq!(get_scope_ttl(&f, "gc:456"), Ok(86_400));
    }

    #[test]
    fn handles_all_scope_kinds() {
        let mut f = ScopeTtlFile::default();
        set_scope_ttl(&mut f, "dm:1".into(), 7200).unwrap();
        set_scope_ttl(&mut f, "gc:2".into(), 7200).unwrap();
        set_scope_ttl(&mut f, "server_channel:3:4".into(), 7200).unwrap();
        set_scope_ttl(&mut f, "server_full:5".into(), 7200).unwrap();
        assert_eq!(f.entries.len(), 4);
    }
}
