//! Receiver-side whitelist invitation queue (`pending_invitations.json`).
//!
//! Spec: `docs/phase-7-design.md` §5.3 and §7.
//!
//! ## Shape
//!
//! Top-level JSON object keyed by *invitation id* (a sender-chosen
//! string, typically `from_<peer>_<scope>`). Values are
//! [`PendingInvitation`] records — sender id, scope description,
//! receipt time, and current status (`pending` | `accepted` |
//! `declined`).
//!
//! On first launch the file is created empty by the fresh-start
//! flow (`crate::fresh_start`). It is appended to whenever the
//! recv-side observer (Phase 7b) receives a type=0x02 whitelist-
//! invitation control message and removed-from whenever the user
//! accepts or declines via the settings UI (Phase 7d).
//!
//! The UI surfaces a persistent banner per entry; banners persist
//! across app restarts until explicit accept/decline.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lifecycle state for a single invitation. `pending` is the only
/// state we strictly need (entries are removed on accept/decline);
/// the `accepted`/`declined` variants exist so the UI can show a
/// brief confirmation banner before the entry is cleaned up.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvitationStatus {
    #[default]
    Pending,
    Accepted,
    Declined,
}

/// One pending invitation. `scope` is the design-doc scope string
/// from §2.1 (`dm`, `gc`, `server_channel`, `server_full`); the
/// optional `scope_id` carries the channel/GC/server id for
/// multi-user scopes (omitted for `dm`, where the sender id is
/// sufficient).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingInvitation {
    /// Discord id of the inviting peer.
    pub from: String,

    /// One of `dm`, `gc`, `server_channel`, `server_full`. Matches
    /// the scope strings used elsewhere in Phase 7.
    pub scope: String,

    /// Channel/GC/server id for multi-user scopes; `None` for DM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,

    /// ISO-8601 instant we observed the invitation.
    pub received_at: String,

    /// Current status (defaults to Pending).
    #[serde(default)]
    pub status: InvitationStatus,
}

/// Top-level shape: invitation id → record.
pub type PendingInvitations = HashMap<String, PendingInvitation>;

/// Errors returned by the loader. Same non-fatal posture as the
/// other Phase 7 schemas.
#[derive(Debug, thiserror::Error)]
pub enum PendingInvitationsError {
    #[error("pending_invitations.json not found at {path}")]
    NotFound { path: PathBuf },

    #[error("pending_invitations.json read failed at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("pending_invitations.json parse failed at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Load pending invitations from `path`. Same shape as the other
/// per-file loaders in this crate.
pub fn load_pending_invitations_from_path(
    path: &Path,
) -> Result<PendingInvitations, PendingInvitationsError> {
    let blob = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(PendingInvitationsError::NotFound {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(PendingInvitationsError::ReadFailed {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    // 7d-B4 (scoped): transparent decrypt for OSL-ENC1 blobs.
    let plain = crate::main_password::maybe_decrypt(&blob).map_err(|e| {
        PendingInvitationsError::ParseFailed {
            path: path.to_path_buf(),
            source: serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        }
    })?;
    serde_json::from_slice(&plain).map_err(|source| PendingInvitationsError::ParseFailed {
        path: path.to_path_buf(),
        source,
    })
}

/// Serialise + atomically write to `path`.
pub fn write_pending_invitations(
    path: &Path,
    invs: &PendingInvitations,
) -> Result<(), std::io::Error> {
    let body = serde_json::to_string_pretty(invs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // 7d-B4 (scoped): encrypt on write if a file_storage_key is set.
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
        let path = dir.path().join("pending_invitations.json");
        fs::write(&path, "{}").unwrap();
        let invs = load_pending_invitations_from_path(&path).unwrap();
        assert!(invs.is_empty());
    }

    #[test]
    fn round_trip_design_doc_example() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pending_invitations.json");
        fs::write(
            &path,
            r#"{
              "from_liam_dm": {
                "from": "liam_id",
                "scope": "dm",
                "received_at": "2026-05-09T12:00:00Z",
                "status": "pending"
              },
              "from_alice_gc_xyz": {
                "from": "alice_id",
                "scope": "gc",
                "scope_id": "xyz",
                "received_at": "2026-05-09T12:01:00Z",
                "status": "pending"
              }
            }"#,
        )
        .unwrap();
        let invs = load_pending_invitations_from_path(&path).unwrap();
        assert_eq!(invs.len(), 2);
        assert_eq!(invs.get("from_liam_dm").unwrap().scope, "dm");
        assert_eq!(
            invs.get("from_alice_gc_xyz").unwrap().scope_id.as_deref(),
            Some("xyz")
        );

        write_pending_invitations(&path, &invs).unwrap();
        let reloaded = load_pending_invitations_from_path(&path).unwrap();
        assert_eq!(reloaded, invs);
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pending_invitations.json");
        assert!(matches!(
            load_pending_invitations_from_path(&path),
            Err(PendingInvitationsError::NotFound { .. })
        ));
    }
}
