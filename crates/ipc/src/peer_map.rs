//! Discord-id → OSL-user-id mapping (`peer_map.json`).
//!
//! Phase 5 v1 receive-side decoupling: the boot.js DOM observer
//! pulls the *Discord* `user_id` (e.g. `1477008451799482419`) from
//! the rendered message DOM, but the keyserver only knows OSL
//! identifiers (`liam`, `henry`, …). Without a mapping every
//! decrypt attempt 404s on `fetch_pubkeys`. This module loads a
//! hand-edited JSON file that translates between the two.
//!
//! ## File format
//!
//! Plain JSON object, keys are Discord user_id snowflakes (digit
//! strings, 15–22 chars), values are OSL user_ids registered with
//! the keyserver:
//!
//! ```json
//! {
//!   "1477008451799482419": "liam",
//!   "1502770642930634812": "henry"
//! }
//! ```
//!
//! ## Path resolution
//!
//! Same `osl_config_dir()` convention as `identity.json` /
//! `keyserver.json` (Windows: `%APPDATA%\osl\peer_map.json`;
//! Linux/macOS: `$XDG_CONFIG_HOME/osl/peer_map.json` or
//! `$HOME/.config/osl/peer_map.json`). Co-locating all three keeps
//! a single config tree per machine.
//!
//! ## v1 limitations (documented for the user)
//!
//! - Hand-edited. v1 has no in-app UI for adding peers; the user
//!   creates `peer_map.json` themselves before first receive.
//! - Missing-file is fine. The bootstrap loader treats absence as
//!   an empty map and emits an onboarding-friendly hint pointing
//!   at the expected path. The result: receive-side decryption is
//!   a no-op until the user populates the file. (Send-side and
//!   identity bootstrap both still work.)
//! - Malformed JSON is reported as `Err` to the bootstrap loader,
//!   which logs and falls back to an empty map — the app still
//!   starts.
//!
//! ## v2 plan
//!
//! In-app peer-management UI that round-trips through the
//! keyserver: scan added peers, confirm OSL IDs, write the map.
//! The on-disk format stays the same, so v1 hand-edits keep
//! working through the migration.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// not-found (permissions, transient I/O). Bootstrap logs
    /// and continues with an empty map; the user fixes the
    /// permission issue and restarts.
    #[error("peer_map.json read failed at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `serde_json::from_str` rejected the body. Almost always a
    /// trailing-comma / quoting / type-mismatch typo in the
    /// hand-edited file. Bootstrap logs the inner error so the
    /// user can find the line, then continues with an empty map.
    #[error("peer_map.json parse failed at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Discord-id → OSL-user-id map. Values are interned by the JSON
/// loader as plain `String`s — no normalisation, no canonicalisation
/// (the keyserver's allowlist is the source of truth for what
/// counts as a valid OSL user_id).
pub type PeerMap = HashMap<String, String>;

/// Load the peer map from a caller-supplied path. Used by the
/// production loader (which resolves the path via `osl_config_dir`)
/// and by tests (which point at a `tempfile::tempdir()`).
pub fn load_peer_map_from_path(path: &Path) -> Result<PeerMap, PeerMapError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
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
    serde_json::from_str::<PeerMap>(&raw).map_err(|source| PeerMapError::ParseFailed {
        path: path.to_path_buf(),
        source,
    })
}

/// Resolve the OS-default `peer_map.json` path
/// (`<osl_config_dir>/peer_map.json`) and load.
///
/// Returns the resolved path alongside the result so the bootstrap
/// caller can include it in the user-facing onboarding hint
/// without having to re-derive it.
pub fn load_peer_map() -> (PathBuf, Result<PeerMap, PeerMapError>) {
    let dir = match keystore::osl_config_dir() {
        Ok(d) => d,
        Err(e) => {
            // No config dir → no peer_map.json. Synthesize a
            // placeholder path for the error so the caller's
            // logging is still uniform.
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
    fn valid_json_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(
            &path,
            r#"{"1477008451799482419":"liam","1502770642930634812":"henry"}"#,
        )
        .unwrap();
        let map = load_peer_map_from_path(&path).expect("valid JSON should load");
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("1477008451799482419").map(|s| s.as_str()),
            Some("liam")
        );
        assert_eq!(
            map.get("1502770642930634812").map(|s| s.as_str()),
            Some("henry")
        );
        assert_eq!(map.get("9999999999999999999"), None);
    }

    #[test]
    fn malformed_json_is_parse_failed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        // Trailing comma — the canonical hand-edit failure mode.
        fs::write(&path, r#"{"1477008451799482419":"liam",}"#).unwrap();
        let err = load_peer_map_from_path(&path).expect_err("trailing comma should fail to parse");
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
        // Array root — common copy-paste mistake from documentation
        // showing example entries.
        fs::write(&path, r#"["liam","henry"]"#).unwrap();
        let err =
            load_peer_map_from_path(&path).expect_err("array root should fail (expected object)");
        assert!(
            matches!(err, PeerMapError::ParseFailed { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn non_string_value_is_parse_failed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("peer_map.json");
        fs::write(&path, r#"{"1477008451799482419":42}"#).unwrap();
        let err = load_peer_map_from_path(&path)
            .expect_err("non-string value should fail (expected string)");
        assert!(
            matches!(err, PeerMapError::ParseFailed { .. }),
            "got {err:?}"
        );
    }
}
