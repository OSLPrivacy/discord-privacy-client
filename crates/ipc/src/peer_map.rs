//! Discord-id → peer-state mapping (`peer_map.json`).
//!
//! ## Schema history
//!
//! - **v1 / Phase 5** (legacy): `{ "<discord_id>": "<osl_user_id>" }`
//!   — a flat string-to-string map. Translates Discord snowflakes
//!   to the OSL identifiers the keyserver uses.
//! - **v2 / Phase 7a** (current): `{ "<discord_id>": <PeerEntry> }`
//!   — per `docs/phase-7-design.md` §5.1. Each value is an
//!   object carrying the legacy `osl_user_id` plus the new
//!   whitelist / burn fields the Phase 7 trust model needs.
//!
//! ## Backward compatibility
//!
//! The loader accepts **both** formats per-value via a serde
//! `untagged` enum. A v=1 entry (string value) is upgraded
//! in-memory to a v=2 [`PeerEntry`] with `osl_user_id =
//! Some(value)` and default-empty whitelist/burn arrays. When the
//! upgrade fires, the file is re-written in the new format so the
//! next load is a clean v=2 parse. See [`load_peer_map_from_path`].
//!
//! The fresh-start path (`crate::fresh_start`) writes an empty v=2
//! map, so users running the documented migration never see the
//! legacy path.
//!
//! ## File layout (v=2)
//!
//! ```json
//! {
//!   "1477008451799482419": {
//!     "osl_user_id": "liam",
//!     "pubkey": null,
//!     "discord_id": "1477008451799482419",
//!     "first_seen": "2026-05-09T12:34:56Z",
//!     "incoming_decrypt_accepted": false,
//!     "outgoing_whitelists": [],
//!     "burned_scopes": []
//!   }
//! }
//! ```
//!
//! `osl_user_id` is **kept** as an optional field for v=1
//! interoperability — the existing v=1 decrypt path needs it to
//! resolve the keyserver pubkey lookup. New entries created by
//! v=2 flows may set it lazily when first observed.
//!
//! ## Path resolution
//!
//! Same `osl_config_dir()` convention as `identity.json` /
//! `keyserver.json` (Windows: `%APPDATA%\osl\peer_map.json`;
//! Linux/macOS: `$XDG_CONFIG_HOME/osl/peer_map.json` or
//! `$HOME/.config/osl/peer_map.json`).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One peer's full v=2 record per `docs/phase-7-design.md` §5.1.
///
/// Optional fields default to safe empty values via serde so a
/// minimal hand-edit (`{"osl_user_id": "henry"}`) still loads.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeerEntry {
    /// Legacy keyserver identifier (`"liam"`, `"henry"`, …). Required
    /// by the v=1 decrypt path; optional in the new schema so future
    /// entries discovered via v=2 (which already carries the pubkey
    /// inline) don't have to round-trip the keyserver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub osl_user_id: Option<String>,

    /// Base64-encoded X25519 public key for this peer. Populated by
    /// Phase 7 onboarding flows that observe the peer's pubkey
    /// directly (rather than via keyserver round-trip). Optional
    /// because legacy v=1 entries don't carry it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,

    /// Discord snowflake for the peer — redundant with the map key,
    /// but the design doc lists it explicitly so we serialize it.
    /// Useful for log lines that only carry the value, not the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord_id: Option<String>,

    /// ISO-8601 timestamp of first observation. Optional because
    /// legacy entries upgrading from v=1 don't have a true
    /// first-seen — we record "upgraded at" as a stand-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<String>,

    /// Per-scope record of whether this peer's whitelist
    /// invitation to *us* has been accepted. Key is the scope
    /// storage_key (`crate::scope::Scope::storage_key`); value is
    /// `true` for accept, `false` for explicit decline.
    /// **Missing key = not yet responded** (the recv-path treats
    /// that as "leave cover in place" — see §7).
    ///
    /// Phase 7a shipped this as a single `bool` matching the
    /// design doc §5.1 example. Phase 7b promotes it to a map per
    /// §7.3 ("Henry's client stores:
    /// `incoming_decrypt_accepted[liam][scope] = true`"); the
    /// outer indirection is peer_map itself, leaving this inner
    /// map keyed by scope.
    #[serde(default)]
    pub incoming_decrypt_accepted: std::collections::HashMap<String, bool>,

    /// Our outgoing whitelists for this peer, per scope (§2.1).
    /// Phase 7a stores them; 7b consults them on every send.
    #[serde(default)]
    pub outgoing_whitelists: Vec<WhitelistEntry>,

    /// Burned scopes (§3). Phase 7a stores them; 7b honours them
    /// in the send-path scope check.
    #[serde(default)]
    pub burned_scopes: Vec<BurnedScope>,

    /// Phase 7b: per-scope acceptance status FROM this peer
    /// for our outgoing invitations. Key is scope storage_key;
    /// value is `true` for accepted, `false` for declined.
    /// **Missing key = invitation sent but no response yet.**
    ///
    /// Mirrors `incoming_decrypt_accepted` but in the opposite
    /// direction — that map records *our* response to the peer's
    /// invitations, this one records the *peer's* response to
    /// ours. The UI (Phase 7c) reads this to show
    /// "accepted" / "declined" / "pending" pills next to each
    /// outgoing whitelist entry.
    #[serde(default)]
    pub outgoing_whitelist_responses: std::collections::HashMap<String, bool>,

    /// 7d-FIX3: marker for the local user's own peer_map entry.
    /// Populated by `osl_register_self_snowflake` or by bootstrap
    /// repair (see `verify_peer_map_self_entry`). Lets the
    /// settings-window Identity page identify the self row and
    /// the burn-flow self-id lookup skip mismatched entries.
    /// Default `None` for backward compatibility with pre-FIX3
    /// peer_map.json files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_self: Option<bool>,
}

/// One outgoing whitelist entry for a peer. Variants correspond to
/// the scope hierarchy in `docs/phase-7-design.md` §2.1. The
/// `serde(tag = "scope")` representation matches the JSON shape
/// the design doc shows in §5.1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum WhitelistEntry {
    /// DM whitelist. `broadened = true` automatically grants
    /// decryption access in any GC/server we share with the peer
    /// (§2.1). `enabled_at` is ISO-8601.
    Dm {
        #[serde(default)]
        broadened: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled_at: Option<String>,
    },
    /// Group-chat whitelist. `id` is the GC channel id.
    /// `user_specific = false` means full-GC; `true` means we're
    /// in a per-user list within the GC.
    Gc {
        id: String,
        #[serde(default)]
        user_specific: bool,
    },
    /// Server-channel whitelist.
    ServerChannel {
        server_id: String,
        channel_id: String,
        #[serde(default)]
        user_specific: bool,
    },
    /// Entire-server whitelist.
    ServerFull {
        server_id: String,
        #[serde(default)]
        user_specific: bool,
    },
}

/// One burned scope. Same tag convention as [`WhitelistEntry`].
/// `burned_at` is the ISO-8601 instant we triggered the burn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum BurnedScope {
    Dm {
        burned_at: String,
    },
    Gc {
        id: String,
        burned_at: String,
    },
    ServerChannel {
        server_id: String,
        channel_id: String,
        burned_at: String,
    },
    ServerFull {
        server_id: String,
        burned_at: String,
    },
}

/// Wire representation that accepts either v=1 (string) or v=2
/// (object) values per peer. We deserialize into this and then
/// fold the legacy variant into a [`PeerEntry`].
///
/// `serde(untagged)` tries variants in order until one matches.
/// We put `Modern` first so well-formed v=2 entries parse cleanly;
/// `Legacy` is the fallback for old `{"discord_id": "osl_id"}`
/// files.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PeerEntryRepr {
    Modern(PeerEntry),
    Legacy(String),
}

impl From<PeerEntryRepr> for PeerEntry {
    fn from(repr: PeerEntryRepr) -> Self {
        match repr {
            PeerEntryRepr::Modern(e) => e,
            PeerEntryRepr::Legacy(osl_id) => PeerEntry {
                osl_user_id: Some(osl_id),
                ..PeerEntry::default()
            },
        }
    }
}

/// Discord-id → [`PeerEntry`] map. Keys are Discord snowflakes.
pub type PeerMap = HashMap<String, PeerEntry>;

/// Construct a minimal v=1-compatible [`PeerEntry`] from an
/// `osl_user_id`. Used by tests + the legacy migration to seed
/// the map with just enough info for the v=1 decrypt path.
pub fn legacy_entry(osl_user_id: impl Into<String>) -> PeerEntry {
    PeerEntry {
        osl_user_id: Some(osl_user_id.into()),
        ..PeerEntry::default()
    }
}

/// Convenience: look up the legacy `osl_user_id` for a Discord id.
/// `None` when the peer is unknown OR when the entry was written
/// by a v=2 flow that didn't carry the legacy id.
pub fn osl_user_id_for<'a>(map: &'a PeerMap, discord_id: &str) -> Option<&'a str> {
    map.get(discord_id).and_then(|e| e.osl_user_id.as_deref())
}

/// Errors returned by the loader. The bootstrap caller logs and
/// continues with an empty map for **every** variant — no failure
/// here should keep the app from starting.
#[derive(Debug, thiserror::Error)]
pub enum PeerMapError {
    /// `peer_map.json` doesn't exist. **Common-path on first boot.**
    /// The caller treats this as an empty map.
    #[error("peer_map.json not found at {path}")]
    NotFound { path: PathBuf },

    /// `read_to_string` failed for a reason other than
    /// not-found (permissions, transient I/O).
    #[error("peer_map.json read failed at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `serde_json::from_str` rejected the body.
    #[error("peer_map.json parse failed at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Write-back of the upgraded v=2 file failed. The in-memory
    /// map still loaded successfully; only the persistence step
    /// of the migration failed. Bootstrap logs and continues.
    #[error("peer_map.json migration write-back failed at {path}: {source}")]
    WriteBackFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Load the peer map from a caller-supplied path.
///
/// Detects format per-value via [`PeerEntryRepr`]. If any legacy
/// (string) value is observed during load, the file is rewritten
/// in the new v=2 format before returning. A failure to write back
/// is surfaced as [`PeerMapError::WriteBackFailed`]; the caller
/// can choose to log and continue with the in-memory map.
pub fn load_peer_map_from_path(path: &Path) -> Result<PeerMap, PeerMapError> {
    let blob = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PeerMapError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(PeerMapError::ReadFailed {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    // 7d-B4 (scoped): transparently decrypt if the on-disk bytes
    // start with the OSL-ENC1 magic. Requires
    // `main_password::set_file_storage_key` to have been called
    // (verify_main_password / gate-main-success). For plain JSON
    // (no marker set, or password removed), pass through.
    let plain =
        crate::main_password::maybe_decrypt(&blob).map_err(|e| PeerMapError::ParseFailed {
            path: path.to_path_buf(),
            source: serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        })?;
    let raw_map: HashMap<String, PeerEntryRepr> =
        serde_json::from_slice(&plain).map_err(|source| PeerMapError::ParseFailed {
            path: path.to_path_buf(),
            source,
        })?;

    // Detect legacy-format presence before converting (so we know
    // whether to write back).
    let any_legacy = raw_map
        .values()
        .any(|v| matches!(v, PeerEntryRepr::Legacy(_)));

    let map: PeerMap = raw_map
        .into_iter()
        .map(|(k, v)| (k, PeerEntry::from(v)))
        .collect();

    if any_legacy {
        write_peer_map(path, &map).map_err(|source| PeerMapError::WriteBackFailed {
            path: path.to_path_buf(),
            source,
        })?;
    }

    Ok(map)
}

/// Serialise + write `map` to `path` atomically (via a tempfile +
/// rename, so a crash mid-write doesn't truncate the existing
/// file).
pub fn write_peer_map(path: &Path, map: &PeerMap) -> Result<(), std::io::Error> {
    let body = serde_json::to_string_pretty(map)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // 7d-B4 (scoped): if a file_storage_key is installed (main
    // password active), encrypt before write. Plain JSON otherwise.
    let out_bytes = crate::main_password::maybe_encrypt(body.as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out_bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Resolve the OS-default `peer_map.json` path
/// (`<osl_config_dir>/peer_map.json`) and load.
pub fn load_peer_map() -> (PathBuf, Result<PeerMap, PeerMapError>) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            let placeholder = PathBuf::from("<no-config-dir>/peer_map.json");
            return (
                placeholder.clone(),
                Err(PeerMapError::ReadFailed {
                    path: placeholder,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("could not resolve OSL config directory: {e}"),
                    ),
                }),
            );
        }
    };
    let path = dir.join("peer_map.json");
    let result = load_peer_map_from_path(&path);
    (path, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn missing_file_is_not_found_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        let err = load_peer_map_from_path(&path).expect_err("missing file should error");
        assert!(matches!(err, PeerMapError::NotFound { .. }), "got {err:?}");
    }

    #[test]
    fn legacy_string_values_upgrade_in_place() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(
            &path,
            r#"{"1477008451799482419":"liam","1502770642930634812":"henry"}"#,
        )
        .unwrap();
        let map = load_peer_map_from_path(&path).expect("legacy format should load");
        assert_eq!(map.len(), 2);
        assert_eq!(osl_user_id_for(&map, "1477008451799482419"), Some("liam"));
        assert_eq!(osl_user_id_for(&map, "1502770642930634812"), Some("henry"));
        // File should now be in v=2 format with object values.
        let after = fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("\"osl_user_id\""),
            "expected upgrade write-back to add osl_user_id field, got: {after}"
        );
        // Second load on the upgraded file: no rewrite, modern parse.
        let map2 = load_peer_map_from_path(&path).expect("second load");
        assert_eq!(map2, map);
    }

    #[test]
    fn modern_object_values_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(
            &path,
            r#"{
              "1477008451799482419": {
                "osl_user_id": "liam",
                "discord_id": "1477008451799482419",
                "first_seen": "2026-05-09T12:00:00Z",
                "incoming_decrypt_accepted": { "dm:1477008451799482419": true },
                "outgoing_whitelists": [
                  { "scope": "dm", "broadened": true, "enabled_at": "2026-05-09T12:00:00Z" }
                ],
                "burned_scopes": []
              }
            }"#,
        )
        .unwrap();
        let map = load_peer_map_from_path(&path).expect("modern format should load");
        let entry = map.get("1477008451799482419").unwrap();
        assert_eq!(entry.osl_user_id.as_deref(), Some("liam"));
        assert_eq!(
            entry
                .incoming_decrypt_accepted
                .get("dm:1477008451799482419"),
            Some(&true)
        );
        assert_eq!(entry.outgoing_whitelists.len(), 1);
        match &entry.outgoing_whitelists[0] {
            WhitelistEntry::Dm { broadened, .. } => assert!(*broadened),
            other => panic!("expected DM whitelist, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_is_parse_failed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(&path, r#"{"1477008451799482419":"liam",}"#).unwrap();
        let err = load_peer_map_from_path(&path).expect_err("trailing comma should fail");
        assert!(
            matches!(err, PeerMapError::ParseFailed { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn empty_object_loads_as_empty_map() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(&path, r#"{}"#).unwrap();
        let map = load_peer_map_from_path(&path).expect("empty object is valid");
        assert!(map.is_empty());
    }

    #[test]
    fn non_object_root_is_parse_failed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(&path, r#"["liam","henry"]"#).unwrap();
        let err =
            load_peer_map_from_path(&path).expect_err("array root should fail (expected object)");
        assert!(
            matches!(err, PeerMapError::ParseFailed { .. }),
            "got {err:?}"
        );
    }
}
