//! Channel-to-recipients mapping resolved from a JSON config file on
//! disk. Phase 4 of Layer 10 needs to know "for this Discord channel,
//! which user_ids should the message be encrypted to?" without yet
//! having a real key-discovery / channel-introspection backend.
//!
//! The Vencord-style approach (parse the recipient list out of
//! Discord's React state for the active channel) was rejected for
//! Phase 4 because it conflates "decryption client" with "DOM
//! scraper" — Discord's channel-store shape changes shipping-cycle
//! to shipping-cycle, and a wrong recipient set is a fail-open from
//! a privacy standpoint (encrypted to N-1 of N intended recipients
//! still leaks plaintext to Discord). A static JSON config is dumb,
//! auditable, and forces the dogfooding user to opt every channel in
//! explicitly.
//!
//! ## On-disk format
//!
//! Path:
//! - Linux / macOS: `$XDG_CONFIG_HOME/osl/channels.json`, falling
//!   back to `$HOME/.config/osl/channels.json`.
//! - Windows: `%APPDATA%\osl\channels.json`.
//!
//! No alternative paths and no auto-creation. If the file is absent,
//! [`get_recipients`] returns
//! [`RecipientError::ConfigFileMissing`] and the caller (Phase 4
//! `osl_encrypt_message`) fails closed — the user has to set up the
//! file by hand the first time.
//!
//! ```json
//! {
//!   "channels": {
//!     "1234567890123456789": { "recipients": ["111", "222"] },
//!     "9876543210987654321": { "recipients": ["333"] }
//!   }
//! }
//! ```
//!
//! ## API
//!
//! - [`get_recipients(channel_id)`](get_recipients) — load the file
//!   on each call, look up `channel_id`, return the recipient list.
//!   No internal cache: the file is small (one entry per dogfooded
//!   channel) and rereading it on every send means edits land
//!   immediately without a client restart, which matters for the
//!   alpha-prototype "I just added another test account" workflow.
//!
//! Phase 5+ replaces this entirely with a proper channel-membership
//! resolver against the key-server. The wire shape is the same
//! `Vec<String>` so the call site in `osl_encrypt_message` doesn't
//! change.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Deserialize, Serialize)]
struct ChannelsFile {
    channels: BTreeMap<String, ChannelEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChannelEntry {
    recipients: Vec<String>,
}

#[derive(Debug, Error)]
pub enum RecipientError {
    /// No `channels.json` exists at the resolved path. The caller
    /// should fail closed; we deliberately do NOT auto-create the
    /// file because that would mask an unconfigured-recipient bug as
    /// "encrypted to nobody" silently.
    #[error("recipients config not found at {path}; create it before sending")]
    ConfigFileMissing { path: PathBuf },

    /// The config file exists but the channel id is not listed.
    #[error("channel {channel_id} not configured in recipients file at {path}")]
    ChannelNotConfigured { channel_id: String, path: PathBuf },

    /// The channel is listed but has an empty recipient array. We
    /// surface this distinct from "missing channel" so the caller
    /// can show a clearer error message to the user (the
    /// distinction matters: "I forgot to add this channel" vs "I
    /// added the channel but forgot to fill in recipients").
    #[error("channel {channel_id} has empty recipients list in {path}")]
    EmptyRecipients { channel_id: String, path: PathBuf },

    /// `read_to_string` failed for any reason other than NotFound
    /// (permissions, IO error). NotFound is mapped to
    /// `ConfigFileMissing` for a clearer error message.
    #[error("failed to read recipients config at {path}: {source}")]
    FileReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// JSON malformed.
    #[error("failed to parse recipients config at {path}: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// Neither `APPDATA` (Windows) nor `HOME` / `XDG_CONFIG_HOME`
    /// (Linux/macOS) is set. Almost never happens in practice; we
    /// surface it as a distinct error rather than panicking so the
    /// IPC layer can convert it to a user-visible string.
    #[error("could not resolve OS config directory: no APPDATA / HOME / XDG_CONFIG_HOME")]
    NoConfigDir,
}

/// Compute the OSL config directory for the current OS, using only
/// `std::env`. Order:
///
/// 1. Windows: `%APPDATA%\osl`.
/// 2. Otherwise: `$XDG_CONFIG_HOME/osl` if `XDG_CONFIG_HOME` is set,
///    else `$HOME/.config/osl`.
///
/// Returns [`RecipientError::NoConfigDir`] if no env var is set.
///
/// Public so the autostart / bootstrap path in `src-tauri` can put
/// `identity.json` and `keyserver.json` in the same directory as
/// `channels.json`. (Re-using a single path resolver keeps all
/// three files co-located without each callsite duplicating the
/// XDG / APPDATA fallback chain.)
///
/// Multi-account: when set, every consumer of [`osl_config_dir`] reads
/// and writes inside this per-account directory instead of the shared
/// base. `None` (the default) preserves the original single-account
/// behavior exactly — `osl_config_dir() == osl_base_dir()`. Set by the
/// bootstrap (from the persisted active-account marker) and by an
/// in-session account switch. A process-global is the pragmatic choice:
/// `osl_config_dir` is a free function called from ~60 sites that can't
/// thread an account parameter, and the active account is a
/// process-wide fact.
static ACTIVE_ACCOUNT_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);
static BASE_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Override the process-wide OSL base directory for a trusted application
/// shell. The standalone Hub uses its own namespace so launching it never
/// silently signs into or mutates the original Discord client's account.
/// Existing clients that do not call this retain the historical OS default.
pub fn set_base_dir_override(dir: Option<PathBuf>) {
    if let Ok(mut guard) = BASE_DIR_OVERRIDE.write() {
        *guard = dir;
    }
}

/// Point all subsequent `osl_config_dir()` resolution at `dir`
/// (`Some`) or back at the shared base (`None`).
pub fn set_active_account_dir(dir: Option<PathBuf>) {
    if let Ok(mut g) = ACTIVE_ACCOUNT_DIR.write() {
        *g = dir;
    }
}

/// The currently-active per-account directory, if one is set.
pub fn active_account_dir() -> Option<PathBuf> {
    ACTIVE_ACCOUNT_DIR.read().ok().and_then(|g| g.clone())
}

/// Per-account directory for `snowflake`: `<base>/accounts/<snowflake>`.
pub fn account_dir(snowflake: &str) -> Result<PathBuf, RecipientError> {
    let mut p = osl_base_dir()?;
    p.push("accounts");
    p.push(snowflake);
    Ok(p)
}

/// The shared OSL base directory (`%APPDATA%/osl` or `$XDG_CONFIG_HOME/
/// osl`). Holds the multi-account registry (`accounts/`, `active`
/// marker) and, for single-account installs, the account files
/// directly.
pub fn osl_base_dir() -> Result<PathBuf, RecipientError> {
    if let Some(dir) = BASE_DIR_OVERRIDE
        .read()
        .ok()
        .and_then(|guard| guard.clone())
    {
        return Ok(dir);
    }
    // Windows: Roaming AppData. We don't fall back to LOCALAPPDATA
    // because the `osl` config is meant to be roamable (the same
    // user, same identity, same channel mappings should follow
    // them across machines if they're on a Windows roaming
    // profile).
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            if !appdata.is_empty() {
                let mut p = PathBuf::from(appdata);
                p.push("osl");
                return Ok(p);
            }
        }
        Err(RecipientError::NoConfigDir)
    }

    // Linux / macOS path. XDG takes precedence (matches dirs-rs and
    // most Unix tools); HOME/.config is the fallback.
    #[cfg(not(windows))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                let mut p = PathBuf::from(xdg);
                p.push("osl");
                return Ok(p);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                let mut p = PathBuf::from(home);
                p.push(".config");
                p.push("osl");
                return Ok(p);
            }
        }
        Err(RecipientError::NoConfigDir)
    }
}

/// The active config directory: the per-account override when set,
/// else the shared base. Every state file (identity, peer_map,
/// whitelist, message store, …) resolves through here, so switching the
/// override transparently switches which account's files are used.
pub fn osl_config_dir() -> Result<PathBuf, RecipientError> {
    if let Some(dir) = active_account_dir() {
        return Ok(dir);
    }
    osl_base_dir()
}

/// Lower-level form of [`get_recipients`] that takes the
/// `channels.json` path explicitly. Used by tests and any caller
/// that wants to override the OS-default path resolution (e.g.
/// integration tests pointing at a `tempfile::tempdir()`).
///
/// The `channels.json` filename convention is enforced by
/// [`get_recipients`], not here — `path` may be any file the
/// caller chooses, as long as its content matches the expected
/// schema.
pub fn get_recipients_from_path(
    path: &Path,
    channel_id: &str,
) -> Result<Vec<String>, RecipientError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(RecipientError::ConfigFileMissing {
                path: path.to_path_buf(),
            });
        }
        Err(e) => {
            return Err(RecipientError::FileReadFailed {
                path: path.to_path_buf(),
                source: e,
            });
        }
    };

    let parsed: ChannelsFile =
        serde_json::from_str(&raw).map_err(|e| RecipientError::ParseFailed {
            path: path.to_path_buf(),
            source: e,
        })?;

    let entry =
        parsed
            .channels
            .get(channel_id)
            .ok_or_else(|| RecipientError::ChannelNotConfigured {
                channel_id: channel_id.to_string(),
                path: path.to_path_buf(),
            })?;

    if entry.recipients.is_empty() {
        return Err(RecipientError::EmptyRecipients {
            channel_id: channel_id.to_string(),
            path: path.to_path_buf(),
        });
    }

    Ok(entry.recipients.clone())
}

/// Look up the recipient user-ids for a given Discord channel id.
///
/// Resolves the `channels.json` path via [`osl_config_dir`] +
/// `"channels.json"` then delegates to [`get_recipients_from_path`].
/// Reads on every call — see module docs for why there's no cache.
///
/// Error variants and conditions: see
/// [`get_recipients_from_path`].
pub fn get_recipients(channel_id: &str) -> Result<Vec<String>, RecipientError> {
    let path = osl_config_dir()?.join("channels.json");
    get_recipients_from_path(&path, channel_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(dir: &Path, body: &str) -> PathBuf {
        let osl_dir = dir.join("osl");
        std::fs::create_dir_all(&osl_dir).unwrap();
        let p = osl_dir.join("channels.json");
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn lookup_hits_configured_channel() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(
            tmp.path(),
            r#"{"channels":{"123":{"recipients":["aaa","bbb"]}}}"#,
        );
        let r = get_recipients_from_path(&path, "123").unwrap();
        assert_eq!(r, vec!["aaa".to_string(), "bbb".to_string()]);
    }

    #[test]
    fn missing_channel_is_not_configured_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(tmp.path(), r#"{"channels":{"123":{"recipients":["aaa"]}}}"#);
        let err = get_recipients_from_path(&path, "999").unwrap_err();
        assert!(matches!(err, RecipientError::ChannelNotConfigured { .. }));
    }

    #[test]
    fn empty_recipients_is_distinct_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(tmp.path(), r#"{"channels":{"123":{"recipients":[]}}}"#);
        let err = get_recipients_from_path(&path, "123").unwrap_err();
        assert!(matches!(err, RecipientError::EmptyRecipients { .. }));
    }

    #[test]
    fn missing_file_is_distinct_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("osl").join("channels.json");
        let err = get_recipients_from_path(&path, "123").unwrap_err();
        assert!(matches!(err, RecipientError::ConfigFileMissing { .. }));
    }

    #[test]
    fn malformed_json_is_parse_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_config(tmp.path(), "{not json");
        let err = get_recipients_from_path(&path, "123").unwrap_err();
        assert!(matches!(err, RecipientError::ParseFailed { .. }));
    }
}
