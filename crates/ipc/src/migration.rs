//! 9-C1 lossless migration: `whitelist_state.json` legacy v1 →
//! envelope v2 + `peer_map.outgoing_whitelists` projection.
//!
//! Pre-C1, multi-user scopes (GC / channel / server) carried the
//! whitelist roster (`members` / `whitelisted_users` arrays) inside
//! `ScopeState`. C1 collapses those into per-peer entries on
//! `PeerEntry::outgoing_whitelists`. The migration:
//!
//! 1. Reads the legacy file raw (so we see fields serde would drop).
//! 2. Builds the simplified `ScopeState` map.
//! 3. Projects each scope's roster into `peer_map[did].outgoing_whitelists`.
//! 4. Writes the envelope back with `migrated_c1 = true`.
//! 5. Writes `peer_map.json` with the new entries.
//!
//! Idempotent: a file already carrying `migrated_c1: true` loads
//! straight into `state.whitelist_state` without touching peer_map.

use std::path::Path;

use crate::peer_map::{write_peer_map, WhitelistEntry};
use crate::scope::{Scope, ScopeKind};
use crate::state::AppState;
use crate::whitelist_state::{
    write_whitelist_state_file, ScopeState, WhitelistState, WhitelistStateFile,
};

/// Summary returned by [`migrate_whitelist_state_in_place`]. Useful
/// for tests and the bootstrap logger.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub was_already_migrated: bool,
    pub scope_entries_loaded: usize,
    pub legacy_scope_entries_migrated: usize,
    pub peer_links_added: usize,
}

/// Errors the migration can surface. All are non-fatal — boot
/// continues with an empty whitelist state.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("read failed: {0}")]
    Read(#[from] std::io::Error),
    #[error("decrypt failed: {0}")]
    Decrypt(String),
    #[error("parse failed: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("whitelist_state.json root is not a JSON object")]
    NotAnObject,
}

/// Result of "file present and decoded?" — separating this from the
/// full migration lets callers (bootstrap) log NotFound at info
/// rather than warn.
pub enum FileStatus {
    Missing,
    Present(serde_json::Value),
}

fn read_and_decrypt(path: &Path) -> Result<FileStatus, MigrationError> {
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FileStatus::Missing),
        Err(e) => return Err(MigrationError::Read(e)),
    };
    let plain = crate::main_password::maybe_decrypt(&raw)
        .map_err(|e| MigrationError::Decrypt(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_slice(&plain)?;
    Ok(FileStatus::Present(value))
}

/// Migrate `<dir>/whitelist_state.json` in place. Mutates
/// `state.whitelist_state` and `state.peer_map`, then persists both
/// back to disk. Returns `Ok(None)` when the file is missing.
pub fn migrate_whitelist_state_in_place(
    state: &AppState,
    dir: &Path,
) -> Result<Option<MigrationReport>, MigrationError> {
    let path = dir.join("whitelist_state.json");
    let value = match read_and_decrypt(&path)? {
        FileStatus::Missing => return Ok(None),
        FileStatus::Present(v) => v,
    };

    let already_migrated = value
        .get("migrated_c1")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let scopes_value = value
        .get("scopes")
        .and_then(|s| s.as_object())
        .or_else(|| value.as_object())
        .ok_or(MigrationError::NotAnObject)?;

    let mut simplified: WhitelistState = std::collections::HashMap::new();
    let mut report = MigrationReport {
        was_already_migrated: already_migrated,
        ..Default::default()
    };

    for (scope_key, scope_value) in scopes_value {
        if scope_key == "migrated_c1" || scope_key == "scopes" {
            continue;
        }
        let obj = match scope_value.as_object() {
            Some(o) => o,
            None => continue,
        };
        let encrypt_toggle = obj
            .get("encrypt_toggle")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let auto_enabled = obj
            .get("auto_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Probe-4 fix: W1 / W2 per-channel whitelist flag. Previously
        // dropped here via `..ScopeState::default()`, which meant the
        // channel-header / GC-header whitelist toggle DID persist to
        // whitelist_state.json on disk but was wiped on every load --
        // user saw the toggle reset to OFF on every relaunch. Now
        // read + populate the field round-trip.
        let channel_whitelisted = obj
            .get("channel_whitelisted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        simplified.insert(
            scope_key.clone(),
            ScopeState {
                encrypt_toggle,
                auto_enabled,
                channel_whitelisted,
            },
        );

        if already_migrated {
            continue;
        }

        let scope_parsed = match Scope::parse(scope_key) {
            Some(s) => s,
            None => {
                tracing::warn!(
                    scope_key = %scope_key,
                    "OSL migration: legacy scope_key didn't parse; skipping peer projection"
                );
                continue;
            }
        };
        let full_whitelist = obj
            .get("full_whitelist")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let dids: Vec<String> = if full_whitelist {
            obj.get("members")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            obj.get("whitelisted_users")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

        if dids.is_empty() && scope_parsed.kind != ScopeKind::Dm {
            continue;
        }

        let mut pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
        let dids_to_link: Vec<String> = if scope_parsed.kind == ScopeKind::Dm {
            vec![scope_parsed.id.clone()]
        } else {
            dids
        };
        for did in dids_to_link {
            let pe = pm_guard.entry(did.clone()).or_default();
            let entry = match scope_parsed.kind {
                ScopeKind::Dm => WhitelistEntry::Dm {
                    broadened: false,
                    enabled_at: None,
                },
                ScopeKind::Gc => WhitelistEntry::Gc {
                    id: scope_parsed.id.clone(),
                    user_specific: !full_whitelist,
                },
                ScopeKind::ServerChannel => WhitelistEntry::ServerChannel {
                    server_id: scope_parsed.server_id.clone().unwrap_or_default(),
                    channel_id: scope_parsed.channel_id.clone().unwrap_or_default(),
                    user_specific: !full_whitelist,
                },
                ScopeKind::ServerFull => WhitelistEntry::ServerFull {
                    server_id: scope_parsed.server_id.clone().unwrap_or_default(),
                    user_specific: !full_whitelist,
                },
            };
            let already = pe.outgoing_whitelists.iter().any(|w| match (w, &entry) {
                (WhitelistEntry::Dm { .. }, WhitelistEntry::Dm { .. }) => true,
                (WhitelistEntry::Gc { id: a, .. }, WhitelistEntry::Gc { id: b, .. }) => a == b,
                (
                    WhitelistEntry::ServerChannel {
                        server_id: sa,
                        channel_id: ca,
                        ..
                    },
                    WhitelistEntry::ServerChannel {
                        server_id: sb,
                        channel_id: cb,
                        ..
                    },
                ) => sa == sb && ca == cb,
                (
                    WhitelistEntry::ServerFull { server_id: a, .. },
                    WhitelistEntry::ServerFull { server_id: b, .. },
                ) => a == b,
                _ => false,
            });
            if !already {
                pe.outgoing_whitelists.push(entry);
                report.peer_links_added += 1;
            }
        }
        report.legacy_scope_entries_migrated += 1;
    }

    report.scope_entries_loaded = simplified.len();
    *state
        .whitelist_state
        .lock()
        .expect("whitelist_state mutex poisoned") = simplified.clone();

    // 9-C3: parallel load of `server_defaults`. Legacy files without
    // this field load as an empty map (no migration needed — the
    // field is a pure additive).
    let server_defaults: std::collections::HashMap<String, crate::whitelist_state::ServerDefaults> =
        value
            .get("server_defaults")
            .and_then(|sd| serde_json::from_value(sd.clone()).ok())
            .unwrap_or_default();
    *state
        .server_defaults
        .lock()
        .expect("server_defaults mutex poisoned") = server_defaults.clone();

    if already_migrated {
        return Ok(Some(report));
    }

    let envelope = WhitelistStateFile {
        migrated_c1: true,
        scopes: simplified,
        server_defaults,
    };
    if let Err(e) = write_whitelist_state_file(&path, &envelope) {
        tracing::warn!(error = %e, "OSL migration: failed to stamp migrated_c1 marker");
    }
    let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
    let pm_path = dir.join("peer_map.json");
    if let Err(e) = write_peer_map(&pm_path, &pm_guard) {
        tracing::warn!(error = %e, path = %pm_path.display(), "OSL migration: persist peer_map.json failed");
    }
    Ok(Some(report))
}
