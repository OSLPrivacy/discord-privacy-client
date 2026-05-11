//! Per-scope encryption + whitelist state (`whitelist_state.json`).
//!
//! Spec: `docs/phase-7-design.md` §5.2.
//!
//! ## Shape
//!
//! Top-level JSON object keyed by *scope string*:
//!
//! ```text
//! "dm:<discord_id>"                       — DM with that peer
//! "gc:<gc_id>"                            — group chat
//! "server_channel:<server_id>:<ch_id>"    — channel inside a server
//! "server_full:<server_id>"               — entire server
//! ```
//!
//! Each value is a [`ScopeState`]: per-scope encryption toggle plus
//! whether the toggle was auto-enabled by a whitelist or set
//! explicitly by the user. For multi-user scopes (GC, channel,
//! server) the value also carries `full_whitelist` and either
//! `members` (for full-whitelist GCs we know the membership of) or
//! `whitelisted_users` (for per-user whitelists).
//!
//! Phase 7a stores the shape; 7b adds the send-path checks that
//! consult it ("am I allowed to encrypt in this scope?").

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One scope's encryption + whitelist state. Fields default to
/// safe values so a minimal hand-edit (`{}`) still parses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeState {
    /// Whether outgoing messages in this scope encrypt by default.
    /// Drives the per-scope "lock" icon in the channel header
    /// (Phase 7c UI).
    #[serde(default)]
    pub encrypt_toggle: bool,

    /// `true` if the toggle was auto-enabled by adding a whitelist
    /// (§2.3), `false` if the user toggled it explicitly. Lets the
    /// UI distinguish "encryption defaults to on because you
    /// whitelisted someone" from "you turned encryption on
    /// manually." Mostly for the tooltip.
    #[serde(default)]
    pub auto_enabled: bool,

    /// `true` for full-scope whitelists (e.g. full-GC), `false`
    /// for per-user whitelists. Only meaningful for multi-user
    /// scopes; DMs ignore this field.
    #[serde(default)]
    pub full_whitelist: bool,

    /// For full-scope whitelists: the snapshot of scope membership
    /// at whitelist time. Used to compute the per-recipient
    /// wrapped-K list in `wire_v2` send-time (Phase 7b).
    #[serde(default)]
    pub members: Vec<String>,

    /// For per-user whitelists: the explicit allow-list.
    #[serde(default)]
    pub whitelisted_users: Vec<String>,
}

/// Top-level shape: scope string → [`ScopeState`].
pub type WhitelistState = HashMap<String, ScopeState>;

/// Errors returned by the loader. The fresh-start path produces
/// an empty file on first launch, so [`NotFound`] is the
/// common-path for users on a brand-new install — every variant is
/// non-fatal to bootstrap.
#[derive(Debug, thiserror::Error)]
pub enum WhitelistStateError {
    #[error("whitelist_state.json not found at {path}")]
    NotFound { path: PathBuf },

    #[error("whitelist_state.json read failed at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("whitelist_state.json parse failed at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Load whitelist state from `path`. Returns the parsed map on
/// success, [`WhitelistStateError::NotFound`] when the file is
/// absent (caller treats as empty), and the obvious typed errors
/// otherwise.
pub fn load_whitelist_state_from_path(path: &Path) -> Result<WhitelistState, WhitelistStateError> {
    let blob = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WhitelistStateError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(WhitelistStateError::ReadFailed {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    // 7d-B4 (scoped): transparent decrypt for OSL-ENC1 blobs.
    let plain = crate::main_password::maybe_decrypt(&blob).map_err(|e| {
        WhitelistStateError::ParseFailed {
            path: path.to_path_buf(),
            source: serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        }
    })?;
    serde_json::from_slice(&plain).map_err(|source| WhitelistStateError::ParseFailed {
        path: path.to_path_buf(),
        source,
    })
}

/// Serialise + atomically write `state` to `path` (tempfile +
/// rename, so a crash mid-write doesn't truncate the existing
/// file).
pub fn write_whitelist_state(path: &Path, state: &WhitelistState) -> Result<(), std::io::Error> {
    let body = serde_json::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // 7d-B4 (scoped): encrypt on write if a file_storage_key is
    // installed; otherwise pass plain.
    let out_bytes = crate::main_password::maybe_encrypt(body.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out_bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn empty_file_parses_as_empty_map() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("whitelist_state.json");
        fs::write(&path, "{}").unwrap();
        let state = load_whitelist_state_from_path(&path).unwrap();
        assert!(state.is_empty());
    }

    #[test]
    fn round_trip_design_doc_example() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("whitelist_state.json");
        // Example shape from docs/phase-7-design.md §5.2.
        fs::write(
            &path,
            r#"{
              "dm:henry_id": { "encrypt_toggle": true, "auto_enabled": true },
              "gc:1234567890": {
                "encrypt_toggle": true,
                "full_whitelist": true,
                "members": ["liam", "henry", "alice"]
              },
              "server_full:9876": {
                "encrypt_toggle": false,
                "full_whitelist": false,
                "whitelisted_users": []
              }
            }"#,
        )
        .unwrap();
        let state = load_whitelist_state_from_path(&path).unwrap();
        assert_eq!(state.len(), 3);
        let gc = state.get("gc:1234567890").unwrap();
        assert!(gc.full_whitelist);
        assert_eq!(gc.members.len(), 3);

        // Round-trip: write back and reload.
        write_whitelist_state(&path, &state).unwrap();
        let reloaded = load_whitelist_state_from_path(&path).unwrap();
        assert_eq!(reloaded, state);
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("whitelist_state.json");
        assert!(matches!(
            load_whitelist_state_from_path(&path),
            Err(WhitelistStateError::NotFound { .. })
        ));
    }
}
