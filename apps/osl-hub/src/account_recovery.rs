//! Encrypted, account-scoped recovery-kit completion state.
//!
//! This is deliberately backend state rather than renderer state: duress and
//! burn clear WebView storage, so a recovery kit that has not been confirmed
//! saved must survive that cache clear and be re-offered after relaunch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

const RECOVERY_KIT_STATUS_FILE: &str = "recovery_kit_status.json";
const RECOVERY_KIT_STATUS_VERSION: u32 = 1;
const MAX_RECOVERY_KIT_STATUS_PLAINTEXT_BYTES: usize = 128;
const MAX_RECOVERY_KIT_STATUS_SEALED_BYTES: u64 = 4 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryKitStatusDocument {
    version: u32,
    kit_unsaved: bool,
}

/// Returns whether the current account has a recovery kit that still needs to
/// be saved. Missing state is the safe legacy value: no kit has been produced.
pub fn recovery_kit_unsaved() -> Result<bool, String> {
    let key = active_file_key()?;
    load_recovery_kit_status(&active_recovery_kit_status_path()?, &key)
}

/// Records that a recovery kit was produced but has not been confirmed saved.
pub fn mark_recovery_kit_unsaved() -> Result<(), String> {
    let key = active_file_key()?;
    write_recovery_kit_status(&active_recovery_kit_status_path()?, true, &key)
}

/// Records that the current account owner confirmed saving the recovery kit.
pub fn clear_recovery_kit_unsaved() -> Result<(), String> {
    let key = active_file_key()?;
    write_recovery_kit_status(&active_recovery_kit_status_path()?, false, &key)
}

fn active_file_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

fn active_recovery_kit_status_path() -> Result<PathBuf, String> {
    keystore::active_account_dir()
        .map(|directory| directory.join(RECOVERY_KIT_STATUS_FILE))
        .ok_or_else(|| "OSL active identity storage is unavailable".to_owned())
}

fn load_recovery_kit_status(path: &Path, key: &[u8; 32]) -> Result<bool, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_RECOVERY_KIT_STATUS_SEALED_BYTES,
        "OSL recovery-kit status",
    )?
    else {
        return Ok(false);
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL recovery-kit status is not encrypted".to_owned());
    }
    let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL recovery-kit status could not be decrypted".to_owned())?;
    if plaintext.len() > MAX_RECOVERY_KIT_STATUS_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL recovery-kit status exceeds its storage limit".to_owned());
    }
    let decoded = serde_json::from_slice::<RecoveryKitStatusDocument>(&plaintext);
    plaintext.zeroize();
    let document = decoded.map_err(|_| "OSL recovery-kit status is malformed".to_owned())?;
    if document.version != RECOVERY_KIT_STATUS_VERSION {
        return Err("OSL recovery-kit status version is unsupported".to_owned());
    }
    Ok(document.kit_unsaved)
}

fn write_recovery_kit_status(path: &Path, kit_unsaved: bool, key: &[u8; 32]) -> Result<(), String> {
    let document = RecoveryKitStatusDocument {
        version: RECOVERY_KIT_STATUS_VERSION,
        kit_unsaved,
    };
    let mut plaintext = serde_json::to_vec(&document)
        .map_err(|_| "OSL recovery-kit status could not be encoded".to_owned())?;
    if plaintext.len() > MAX_RECOVERY_KIT_STATUS_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL recovery-kit status exceeds its storage limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL recovery-kit status encryption failed".to_owned());
    plaintext.zeroize();
    let sealed = encrypted?;
    if sealed.len() as u64 > MAX_RECOVERY_KIT_STATUS_SEALED_BYTES {
        return Err("OSL encrypted recovery-kit status exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL recovery-kit status")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [0xa9; 32];

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-recovery-kit-status-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn unsaved_recovery_kit_survives_web_storage_clear_and_relaunch() {
        let root = temporary_root("durable-flag");
        let status_path = root.join("account").join(RECOVERY_KIT_STATUS_FILE);
        let web_storage = root.join("web-storage");

        write_recovery_kit_status(&status_path, true, &TEST_KEY).unwrap();
        std::fs::create_dir_all(&web_storage).unwrap();
        std::fs::write(
            web_storage.join("local-storage.json"),
            b"{\"unsaved\":false}",
        )
        .unwrap();
        std::fs::remove_dir_all(&web_storage).unwrap();

        assert!(load_recovery_kit_status(&status_path, &TEST_KEY).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saved_confirmation_clears_the_durable_flag() {
        let root = temporary_root("clear-flag");
        let status_path = root.join(RECOVERY_KIT_STATUS_FILE);

        write_recovery_kit_status(&status_path, true, &TEST_KEY).unwrap();
        write_recovery_kit_status(&status_path, false, &TEST_KEY).unwrap();

        assert!(!load_recovery_kit_status(&status_path, &TEST_KEY).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }
}
