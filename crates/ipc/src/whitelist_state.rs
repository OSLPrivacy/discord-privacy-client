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

/// One scope's encryption state. 9-C1 collapsed the membership tables
/// (`full_whitelist`, `members`, `whitelisted_users`) into the per-peer
/// `outgoing_whitelists` on `PeerEntry` — see the bootstrap migration
/// for the one-shot lossless move. What remains is the user's per-scope
/// "encrypt by default?" toggle plus a UI hint flag.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeState {
    /// Whether outgoing messages in this scope encrypt by default.
    /// Drives the composer pill in the channel header.
    #[serde(default)]
    pub encrypt_toggle: bool,

    /// `true` if the toggle was auto-enabled by adding a whitelist
    /// (§2.3), `false` if the user toggled it explicitly. UI hint
    /// for the composer tooltip.
    #[serde(default)]
    pub auto_enabled: bool,

    /// W1 (Option B): per-channel whitelist flag. When `true`,
    /// outgoing messages in THIS `server_channel:<srv>:<chan>` scope
    /// encrypt to every OSL member of the channel (resolved
    /// dynamically from `ScopeMembership`). Inert while the parent
    /// server's `server_header_whitelisted` is on — the server header
    /// REPLACES per-channel (locked precedence). Only meaningful on
    /// ServerChannel scope entries; ignored for dm:/gc:/server_full:.
    #[serde(default)]
    pub channel_whitelisted: bool,
}

/// Top-level shape: scope string → [`ScopeState`]. The optional
/// `migrated_c1` sentinel sits alongside scope entries via the
/// [`WhitelistStateFile`] envelope; this raw map type stays the
/// in-memory representation.
pub type WhitelistState = HashMap<String, ScopeState>;

/// 9-C3: per-server "encrypt new channels by default" preference.
/// Stored alongside the scopes map (rather than as a `server_full:*`
/// scope entry) because the semantics are distinct: this controls
/// **auto-application of encrypt_toggle to new ServerChannel
/// scopes**, not "encrypt to everyone in the server" (the latter is
/// a whitelist-coverage concept handled via per-peer outgoing
/// whitelists). Keeping the two separate avoids overloading
/// ScopeState with three different opt-in surfaces.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerDefaults {
    /// When `true`, newly-created channels in this server auto-flip
    /// their `ScopeState.encrypt_toggle` to `true` on CHANNEL_CREATE.
    /// Users can override per-channel via the tri-state header icon.
    #[serde(default)]
    pub encrypt_by_default: bool,

    /// W1 (Option B): the server-header whitelist. When `true`,
    /// EVERY text channel in this server encrypts to every OSL member
    /// of the whole server (the `server:<id>` membership roll-up),
    /// and per-channel `channel_whitelisted` flags are IGNORED for
    /// this server (header REPLACE semantics — locked precedence
    /// DM > server-header > channel). Set/cleared by the channel-
    /// header button (W2).
    #[serde(default)]
    pub server_header_whitelisted: bool,
}

/// 9-C1: on-disk envelope around [`WhitelistState`] carrying the
/// one-shot migration marker. The loader unwraps; older v1 JSON
/// files (no envelope) also parse since the on-disk format is a
/// flat object keyed by scope string. See
/// [`load_whitelist_state_from_path`].
///
/// 9-C3 added `server_defaults`. Legacy files load cleanly with an
/// empty map via `#[serde(default)]`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WhitelistStateFile {
    #[serde(default)]
    pub migrated_c1: bool,
    #[serde(default)]
    pub scopes: WhitelistState,
    #[serde(default)]
    pub server_defaults: HashMap<String, ServerDefaults>,
}

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

/// Load whitelist state from `path`. Returns the parsed map plus the
/// migration marker on success, [`WhitelistStateError::NotFound`]
/// when the file is absent. Tolerates both legacy v1 (`{ "dm:peer":
/// {...} }` keyed-by-scope) and 9-C1 v2 (`{ "migrated_c1": true,
/// "scopes": {...} }` envelope) shapes.
pub fn load_whitelist_state_file(path: &Path) -> Result<WhitelistStateFile, WhitelistStateError> {
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
    let plain = crate::main_password::maybe_decrypt(&blob).map_err(|e| {
        WhitelistStateError::ParseFailed {
            path: path.to_path_buf(),
            source: serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        }
    })?;
    // Try the C1 envelope first; fall back to a flat map for v1.
    let value: serde_json::Value =
        serde_json::from_slice(&plain).map_err(|source| WhitelistStateError::ParseFailed {
            path: path.to_path_buf(),
            source,
        })?;
    let is_envelope = value.get("scopes").is_some() || value.get("migrated_c1").is_some();
    if is_envelope {
        let file: WhitelistStateFile =
            serde_json::from_value(value).map_err(|source| WhitelistStateError::ParseFailed {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(file)
    } else {
        // Legacy v1: the top-level object IS the scope-keyed map.
        let scopes: WhitelistState =
            serde_json::from_value(value).map_err(|source| WhitelistStateError::ParseFailed {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(WhitelistStateFile {
            migrated_c1: false,
            scopes,
            server_defaults: HashMap::default(),
        })
    }
}

/// Backwards-compat wrapper: returns just the inner [`WhitelistState`]
/// map for callers that don't need the migration marker.
pub fn load_whitelist_state_from_path(path: &Path) -> Result<WhitelistState, WhitelistStateError> {
    Ok(load_whitelist_state_file(path)?.scopes)
}

/// Serialise + atomically write `state` to `path` (tempfile +
/// rename, so a crash mid-write doesn't truncate the existing
/// file). 9-C1: writes the envelope form unconditionally; the
/// loader still reads pre-envelope v1 files via the fallback
/// branch above.
pub fn write_whitelist_state(path: &Path, state: &WhitelistState) -> Result<(), std::io::Error> {
    // Note: this writer drops `server_defaults` because the legacy
    // signature only takes the scopes map. Callers that need to
    // round-trip both fields MUST go through
    // `write_whitelist_state_file` (which `persist_whitelist_state_now`
    // does post-9-C3). Used today only by `fresh_start` (writes an
    // empty file at first launch) — no risk of clobbering real data.
    let file = WhitelistStateFile {
        migrated_c1: true,
        scopes: state.clone(),
        server_defaults: HashMap::default(),
    };
    let body = serde_json::to_string_pretty(&file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let out_bytes = crate::main_password::maybe_encrypt(body.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out_bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 9-C1: write the full envelope (including the `migrated_c1`
/// marker) — used by the bootstrap migration to stamp the marker
/// independently of mutating writes.
pub fn write_whitelist_state_file(
    path: &Path,
    file: &WhitelistStateFile,
) -> Result<(), std::io::Error> {
    let body = serde_json::to_string_pretty(file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
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
    fn legacy_v1_parses_with_extra_fields_dropped() {
        // 9-C1: legacy v1 files carry the now-removed
        // `full_whitelist` / `members` / `whitelisted_users` fields.
        // Serde silently ignores unknown fields; the bootstrap
        // migration is responsible for projecting the membership
        // data into peer_map before this file's next write.
        let dir = tempdir().unwrap();
        let path = dir.path().join("whitelist_state.json");
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
        assert!(gc.encrypt_toggle);

        // Round-trip via the post-C1 envelope.
        write_whitelist_state(&path, &state).unwrap();
        let reloaded = load_whitelist_state_from_path(&path).unwrap();
        assert_eq!(reloaded, state);
        // Migration marker now present on disk.
        let file = load_whitelist_state_file(&path).unwrap();
        assert!(file.migrated_c1);
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
