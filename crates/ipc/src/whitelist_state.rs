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

    /// W1 (Option B): the server-header whitelist = the "GREEN" tier of
    /// the server lock. When `true`, EVERY text channel in this server
    /// encrypts to every OSL member of the whole server. Outranks the
    /// yellow tier and per-channel flags.
    #[serde(default)]
    pub server_header_whitelisted: bool,

    /// Server-lock "YELLOW" tier. When `true` and GREEN
    /// (`server_header_whitelisted`) is OFF, server-channel messages
    /// encrypt to the user's DM-whitelisted peers who are members of
    /// this server — NOT to every OSL server member. With both GREEN
    /// and YELLOW off the server lock is "GREY": encrypt to nobody in
    /// the server (self-only). A per-channel `channel_whitelisted`
    /// flag still overrides for its own channel (everyone in it),
    /// regardless of the yellow/grey server tier.
    #[serde(default)]
    pub server_dm_whitelisted: bool,
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

/// Filename of the whitelist/enrolment state, relative to an account dir.
pub const WHITELIST_STATE_FILE: &str = "whitelist_state.json";

/// TA-T10-003a: JSON key of the burn-ledger enrolment marker inside
/// `whitelist_state.json`.
///
/// The marker records, durably and OUTSIDE `burned_scopes.json`, that this
/// account has burned at least once. Without it the kill list is its own only
/// evidence, so deleting the kill list also erases the proof that it ever had
/// contents and [`crate::burned_scopes_file::load_burned_scopes`] cannot tell
/// a deletion from a first run — which is the fail-open this closes.
///
/// It rides `whitelist_state.json` rather than a marker file of its own
/// because this file is already registered in every account-lifecycle sweep
/// `burned_scopes.json` is registered in (the at-rest rotation list, the
/// encrypted identity export, the hub's identity-switch artifact move, the
/// password lifecycle sweep). A new file would have to be added to sweeps that
/// live outside this crate; miss one and the marker desynchronises from the
/// ledger it guards, which either fails open (the original bug) or strands a
/// working account permanently closed (worse).
///
/// It is deliberately NOT a field on [`WhitelistStateFile`]. Callers across
/// the workspace build that envelope from their own in-memory view for
/// unrelated reasons (a whitelist toggle, the C1 migration, a rollback path);
/// a field any of them could leave at `false` would let an ordinary preference
/// write silently clear a security latch, and clearing it fails OPEN. Keeping
/// it out of the struct means no caller can express "not enrolled" by accident:
/// every write through [`write_whitelist_state_file`] re-attaches whatever is
/// on disk, and only [`write_whitelist_state`], the fresh-start reset writer,
/// drops it — which is the point of that writer.
///
/// KNOWN GAP, needs a fix outside this crate: `apps/osl-hub/src/security.rs`
/// does not use either writer. It serialises `WhitelistStateFile` straight
/// through its own `write_encrypted_json` (three sites, around lines 1232,
/// 1242 and 1448), so a whitelist toggle made through the hub drops the marker
/// and re-opens this hole until the next unlock re-asserts it from a non-empty
/// ledger (see `state_reload`). Routing those three writes through
/// [`write_whitelist_state_file`] closes it; that writer already carries the
/// full envelope, so the "legacy convenience writer drops server_defaults"
/// comment at that call site does not apply to it.
const BURN_LEDGER_ENROLLED_KEY: &str = "burn_ledger_enrolled";

/// Whether this account has ever written a burn into its kill list.
///
/// Returned by [`burn_ledger_enrollment`]. Deliberately three-valued: pre-gate
/// bootstrap runs before the at-rest file key is installed, so "I could not
/// read the whitelist file" is a routine, temporary condition there and must
/// NOT be confused with either answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnLedgerEnrollment {
    /// No burn has ever been recorded for this account (or the account has no
    /// state at all — a genuine first run).
    NeverEnrolled,
    /// At least one burn was recorded. A missing kill list is a deletion.
    Enrolled,
    /// The whitelist file exists but could not be read (no file key installed
    /// yet, or the file is corrupt). No conclusion may be drawn.
    Indeterminate,
}

/// Read the burn-ledger enrolment marker for the account rooted at `dir`.
pub fn burn_ledger_enrollment(dir: &Path) -> BurnLedgerEnrollment {
    match read_whitelist_json(&dir.join(WHITELIST_STATE_FILE)) {
        Ok(value) => {
            if value
                .get(BURN_LEDGER_ENROLLED_KEY)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                BurnLedgerEnrollment::Enrolled
            } else {
                BurnLedgerEnrollment::NeverEnrolled
            }
        }
        // No whitelist file at all is the fresh-install shape, and it is also
        // what `fresh_start` leaves behind after it deletes the file — the
        // deliberate "abandon this account's state" escape hatch. Both must
        // stay usable, so this is the one absence that clears the marker.
        Err(WhitelistStateError::NotFound { .. }) => BurnLedgerEnrollment::NeverEnrolled,
        Err(_) => BurnLedgerEnrollment::Indeterminate,
    }
}

/// Record that this account has burned at least once.
///
/// Read-modify-write on the raw JSON so every unrelated field survives, and so
/// this needs no cooperation from the in-memory whitelist envelope. When the
/// file does not exist yet (first burn on an account that has never toggled a
/// whitelist) a minimal envelope is laid down; a later
/// [`write_whitelist_state_file`] fills in the real scopes and carries the
/// marker forward stickily.
///
/// Refuses to write over a file it could not read: overwriting an
/// undecryptable whitelist with a near-empty stub would destroy real user
/// state to record a flag.
pub fn mark_burn_ledger_enrolled(dir: &Path) -> Result<(), WhitelistStateError> {
    let path = dir.join(WHITELIST_STATE_FILE);
    let mut value = match read_whitelist_json(&path) {
        Ok(v) => v,
        Err(WhitelistStateError::NotFound { .. }) => serde_json::json!({
            "migrated_c1": true,
            "scopes": {},
            "server_defaults": {},
        }),
        Err(e) => return Err(e),
    };
    let Some(obj) = value.as_object_mut() else {
        return Err(WhitelistStateError::ParseFailed {
            path: path.clone(),
            source: serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "whitelist_state.json is not a JSON object",
            )),
        });
    };
    if obj
        .get(BURN_LEDGER_ENROLLED_KEY)
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Ok(()); // already marked; no write needed
    }
    obj.insert(
        BURN_LEDGER_ENROLLED_KEY.to_string(),
        serde_json::Value::Bool(true),
    );
    write_whitelist_json(&path, &value)
}

/// Read + decrypt + parse `whitelist_state.json` as raw JSON, without imposing
/// the [`WhitelistStateFile`] shape. Used by the enrolment-marker paths, which
/// must see fields the struct deliberately does not carry.
fn read_whitelist_json(path: &Path) -> Result<serde_json::Value, WhitelistStateError> {
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
    let plain = crate::main_password::maybe_decrypt_file(path, &blob).map_err(|e| {
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

/// Encrypt + atomically write raw whitelist JSON. Mirrors the tempfile+rename
/// discipline of the typed writers so a crash mid-write cannot truncate.
fn write_whitelist_json(path: &Path, value: &serde_json::Value) -> Result<(), WhitelistStateError> {
    let invalid = |e: std::io::Error| WhitelistStateError::ReadFailed {
        path: path.to_path_buf(),
        source: e,
    };
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| invalid(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    let out_bytes = crate::main_password::maybe_encrypt(body.as_bytes())
        .map_err(|e| invalid(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out_bytes).map_err(invalid)?;
    std::fs::rename(&tmp, path).map_err(invalid)?;
    Ok(())
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
    let plain = crate::main_password::maybe_decrypt_file(path, &blob).map_err(|e| {
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
    //
    // TA-T10-003a: this writer is also the fresh-start RESET writer, and it
    // deliberately does NOT carry the burn-ledger enrolment marker forward.
    // `fresh_start` deletes this file and then calls us to lay down an empty
    // one; that is the supported way for a user to abandon an account whose
    // kill list can no longer be proven intact, and it is what keeps a
    // fail-closed latch from being a permanent brick. Making the marker sticky
    // here would turn that escape hatch into a no-op. Everything else goes
    // through `write_whitelist_state_file`, where the marker IS sticky.
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
    // TA-T10-003a: the burn-ledger enrolment marker is STICKY across every
    // mutating write. Callers build this envelope from their own in-memory
    // view — a whitelist toggle, the C1 migration, the hub's rollback path —
    // and none of them know about the marker. Re-reading it from disk and
    // re-attaching it here means an ordinary preference write cannot clear a
    // security latch as a side effect, which would fail OPEN. Only deleting
    // the file (what `fresh_start` does) resets enrolment.
    //
    // Indeterminate — the existing file is there but undecryptable — does NOT
    // set the marker: we are about to overwrite that file anyway, and asserting
    // enrolment from a byte string we could not read would be inventing
    // evidence in the direction that permanently closes an account.
    let mut body = serde_json::to_value(file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if burn_ledger_enrollment(path.parent().unwrap_or_else(|| Path::new(".")))
        == BurnLedgerEnrollment::Enrolled
    {
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                BURN_LEDGER_ENROLLED_KEY.to_string(),
                serde_json::Value::Bool(true),
            );
        }
    }
    let body = serde_json::to_string_pretty(&body)
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

    struct FileKeyReset;

    impl Drop for FileKeyReset {
        fn drop(&mut self) {
            crate::main_password::set_file_storage_key(None);
        }
    }

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
        let _serial = crate::test_process_globals::serialize();
        let _reset = FileKeyReset;
        crate::main_password::set_file_storage_key(Some([0x71; 32]));
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
