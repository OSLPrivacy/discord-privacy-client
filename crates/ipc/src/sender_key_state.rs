//! Phase 9-A3: `sender_key_state.json`.
//!
//! Scope-keyed map of `SenderKeyStateOnDisk` entries. One row per
//! group/server scope that the local user has ever participated in
//! as either sender or receiver. The crate is responsible only for
//! at-rest serialization; rotation triggers, SKDM dispatch, and
//! membership-change detection live in `commands.rs`.
//!
//! Same OSL-ENC1 envelope as `peer_map.json` / `burned_scopes.json` /
//! `whitelist_state.json`. Plain-JSON fallback when no main password
//! is installed; encryption activates transparently once the user
//! sets one.

use crypto::sender_keys::SenderKeyStateOnDisk;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SenderKeyStateFile {
    #[serde(default)]
    pub version: u32,
    /// Key = `scope.storage_key()` (e.g. `"gc:1234"`,
    /// `"server_channel:9876:5432"`).
    #[serde(default)]
    pub states: HashMap<String, SenderKeyStateOnDisk>,
}

pub fn load_sender_key_state(path: &Path) -> SenderKeyStateFile {
    let Ok(blob) = std::fs::read(path) else {
        return SenderKeyStateFile::default();
    };
    let plain = match crate::main_password::maybe_decrypt(&blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load sender_key_state.json decrypt failed");
            return SenderKeyStateFile::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_sender_key_state(path: &Path, file: &SenderKeyStateFile) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(file)
        .map_err(|e| format!("OSL: serialize sender_key_state: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt sender_key_state: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}
