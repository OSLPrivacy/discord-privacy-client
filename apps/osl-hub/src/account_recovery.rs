//! Encrypted, account-scoped recovery-kit completion state.
//!
//! This is deliberately backend state rather than renderer state: duress and
//! burn clear WebView storage, so a recovery kit that has not been confirmed
//! saved must survive that cache clear and be re-offered after relaunch.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub(crate) const RECOVERY_KIT_STATUS_FILE: &str = "recovery_kit_status.json";
const RECOVERY_KIT_STATUS_VERSION: u32 = 1;
const MAX_RECOVERY_KIT_STATUS_PLAINTEXT_BYTES: usize = 128;
const MAX_RECOVERY_KIT_STATUS_SEALED_BYTES: u64 = 4 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryKitStatusDocument {
    version: u32,
    kit_unsaved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_confirmed_at_unix_seconds: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySetupState {
    pub kit_unsaved: bool,
    pub recovery_confirmed_at_unix_seconds: Option<i64>,
}

/// Returns whether the current account has a recovery kit that still needs to
/// be saved. Missing state is the safe legacy value: no kit has been produced.
pub fn recovery_kit_unsaved() -> Result<bool, String> {
    recovery_setup_state().map(|state| state.kit_unsaved)
}

pub fn recovery_setup_state() -> Result<RecoverySetupState, String> {
    let key = active_file_key()?;
    load_recovery_setup_state(&active_recovery_kit_status_path()?, &key)
}

/// Records that a recovery kit was produced but has not been confirmed saved.
pub fn mark_recovery_kit_unsaved() -> Result<(), String> {
    let key = active_file_key()?;
    write_recovery_setup_state(
        &active_recovery_kit_status_path()?,
        &RecoverySetupState {
            kit_unsaved: true,
            recovery_confirmed_at_unix_seconds: None,
        },
        &key,
    )
}

/// Records that the current account owner confirmed saving the recovery kit.
pub fn clear_recovery_kit_unsaved() -> Result<(), String> {
    let key = active_file_key()?;
    let mut state = load_recovery_setup_state(&active_recovery_kit_status_path()?, &key)?;
    state.kit_unsaved = false;
    write_recovery_setup_state(&active_recovery_kit_status_path()?, &state, &key)
}

pub fn record_recovery_word_confirmation(
    current: String,
    entries: Vec<ipc::main_password::RecoveryWordEntry>,
) -> Result<RecoverySetupState, String> {
    // The command takes the DTO shape; `RecoveryWordEntry` is the same pair
    // (position, word) under another lane's name, so convert rather than
    // force one design onto the other.
    let entries: Vec<ipc::commands::RecoveryWordRetypeEntryDto> = entries
        .into_iter()
        .map(|entry| ipc::commands::RecoveryWordRetypeEntryDto {
            position: entry.position as u8,
            word: entry.word,
        })
        .collect();
    let check = ipc::commands::cmd_osl_check_recovery_words(current, entries)?;
    if !check.ok {
        return Err("OSL recovery-word confirmation did not match".to_owned());
    }
    let key = active_file_key()?;
    let mut state = load_recovery_setup_state(&active_recovery_kit_status_path()?, &key)?;
    state.recovery_confirmed_at_unix_seconds = Some(ipc::main_password::now_unix_secs_pub());
    write_recovery_setup_state(&active_recovery_kit_status_path()?, &state, &key)?;
    Ok(state)
}

fn active_file_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

/// Where this account's status file lives.
///
/// `osl_config_dir()`, not `active_account_dir()`. `active_account_dir()` is
/// `None` until a *second* identity exists: `select_active_identity_before_bootstrap`
/// clears it when the registry has no slot marker, and a first account is
/// created flat in the base directory (`password_lifecycle::isolated_account_dir`
/// resolves the same `osl_config_dir()`), only being migrated into a slot later.
/// So on a brand-new profile this returned `Err` and the very first
/// `mark_recovery_kit_unsaved()` — the one the account-creation flow makes
/// immediately after writing `identity.json` and `password_marker.json` —
/// failed, telling the owner that creating the account had failed while the
/// account sat on disk. `osl_config_dir()` is the slot directory whenever one
/// is active and the base directory otherwise, which is exactly where the rest
/// of this account's state already is.
fn active_recovery_kit_status_path() -> Result<PathBuf, String> {
    keystore::osl_config_dir()
        .map(|directory| directory.join(RECOVERY_KIT_STATUS_FILE))
        .map_err(|_| "OSL active identity storage is unavailable".to_owned())
}

fn load_recovery_setup_state(path: &Path, key: &[u8; 32]) -> Result<RecoverySetupState, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_RECOVERY_KIT_STATUS_SEALED_BYTES,
        "OSL recovery-kit status",
    )?
    else {
        return Ok(RecoverySetupState {
            kit_unsaved: false,
            recovery_confirmed_at_unix_seconds: None,
        });
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
    Ok(RecoverySetupState {
        kit_unsaved: document.kit_unsaved,
        recovery_confirmed_at_unix_seconds: document.recovery_confirmed_at_unix_seconds,
    })
}

fn write_recovery_setup_state(
    path: &Path,
    state: &RecoverySetupState,
    key: &[u8; 32],
) -> Result<(), String> {
    let document = RecoveryKitStatusDocument {
        version: RECOVERY_KIT_STATUS_VERSION,
        kit_unsaved: state.kit_unsaved,
        recovery_confirmed_at_unix_seconds: state.recovery_confirmed_at_unix_seconds,
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

        write_recovery_setup_state(
            &status_path,
            &RecoverySetupState {
                kit_unsaved: true,
                recovery_confirmed_at_unix_seconds: None,
            },
            &TEST_KEY,
        )
        .unwrap();
        std::fs::create_dir_all(&web_storage).unwrap();
        std::fs::write(
            web_storage.join("local-storage.json"),
            b"{\"unsaved\":false}",
        )
        .unwrap();
        std::fs::remove_dir_all(&web_storage).unwrap();

        assert!(
            load_recovery_setup_state(&status_path, &TEST_KEY)
                .unwrap()
                .kit_unsaved
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Regression: the reminder must be writable on a BRAND-NEW profile.
    ///
    /// This is the case the two tests around it miss, because they call the
    /// private writer with a path they built themselves. The account-creation
    /// flow calls the public entry points, and on a first run there is no
    /// identity slot yet — `active_account_dir()` is `None` and the account
    /// lives flat in the base directory. `mark_recovery_kit_unsaved()` used to
    /// return `Err` there, which the UI reported as "OSL could not save the
    /// recovery-kit reminder", i.e. as a failed account creation, moments
    /// after `identity.json` and `password_marker.json` had been written.
    #[test]
    fn first_run_flat_account_can_mark_and_read_the_reminder() {
        let _serial = crate::global_keystore_test_lock();
        let root = temporary_root("first-run-flat");
        std::fs::create_dir_all(&root).unwrap();

        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                ipc::main_password::set_file_storage_key(None);
                keystore::set_active_account_dir(None);
                keystore::set_base_dir_override(None);
            }
        }
        let _restore = Restore;

        // Exactly the state `select_active_identity_before_bootstrap` leaves
        // behind when the registry holds no slot: base override set, no active
        // account directory.
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        // Installed by `setup_main_password` immediately before the flow marks
        // the kit unsaved.
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));

        assert_eq!(
            recovery_kit_unsaved(),
            Ok(false),
            "a fresh profile has no kit"
        );
        mark_recovery_kit_unsaved().expect("a first account can record its unsaved recovery kit");
        assert_eq!(recovery_kit_unsaved(), Ok(true));
        assert!(
            root.join(RECOVERY_KIT_STATUS_FILE).is_file(),
            "the reminder must land beside the flat account's identity.json"
        );

        clear_recovery_kit_unsaved().expect("the owner can confirm the kit is saved");
        assert_eq!(recovery_kit_unsaved(), Ok(false));

        let _ = std::fs::remove_dir_all(root);
    }

    /// The multi-account layout must keep using the active slot directory, not
    /// the shared base, or two identities would share one reminder.
    #[test]
    fn a_selected_identity_slot_keeps_its_own_reminder() {
        let _serial = crate::global_keystore_test_lock();
        let root = temporary_root("slot-scoped");
        let slot = root.join("identities").join("slot-a");
        std::fs::create_dir_all(&slot).unwrap();

        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                ipc::main_password::set_file_storage_key(None);
                keystore::set_active_account_dir(None);
                keystore::set_base_dir_override(None);
            }
        }
        let _restore = Restore;

        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(Some(slot.clone()));
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));

        mark_recovery_kit_unsaved().unwrap();
        assert!(slot.join(RECOVERY_KIT_STATUS_FILE).is_file());
        assert!(!root.join(RECOVERY_KIT_STATUS_FILE).exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn saved_confirmation_clears_the_durable_flag() {
        let root = temporary_root("clear-flag");
        let status_path = root.join(RECOVERY_KIT_STATUS_FILE);

        write_recovery_setup_state(
            &status_path,
            &RecoverySetupState {
                kit_unsaved: true,
                recovery_confirmed_at_unix_seconds: None,
            },
            &TEST_KEY,
        )
        .unwrap();
        write_recovery_setup_state(
            &status_path,
            &RecoverySetupState {
                kit_unsaved: false,
                recovery_confirmed_at_unix_seconds: None,
            },
            &TEST_KEY,
        )
        .unwrap();

        assert!(
            !load_recovery_setup_state(&status_path, &TEST_KEY)
                .unwrap()
                .kit_unsaved
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_0452_recovery_word_retype_records_only_confirmation_time() {
        let _serial = crate::global_keystore_test_lock();
        let root = temporary_root("task-0452");
        std::fs::create_dir_all(&root).unwrap();

        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                ipc::main_password::set_file_storage_key(None);
                keystore::set_active_account_dir(None);
                keystore::set_base_dir_override(None);
            }
        }
        let _restore = Restore;

        const PASSWORD: &str = "aB3!z9-task-0452";
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        let phrase = ipc::main_password::set_main_password(&root, PASSWORD).unwrap();
        mark_recovery_kit_unsaved().unwrap();

        let words = phrase.split_whitespace().collect::<Vec<_>>();
        assert_eq!(words.len(), 12);
        let selected_positions = [1usize, 3, 12];
        let selected_entries = selected_positions
            .iter()
            .map(|position| ipc::main_password::RecoveryWordEntry {
                position: *position,
                word: words[*position - 1].to_owned(),
            })
            .collect::<Vec<_>>();
        let mut wrong_entries = selected_entries
            .iter()
            .map(|entry| ipc::main_password::RecoveryWordEntry {
                position: entry.position,
                word: entry.word.clone(),
            })
            .collect::<Vec<_>>();
        wrong_entries[1].word = words
            .iter()
            .copied()
            .find(|word| *word != wrong_entries[1].word)
            .unwrap()
            .to_owned();

        assert_eq!(
            record_recovery_word_confirmation(PASSWORD.to_owned(), wrong_entries).unwrap_err(),
            "OSL recovery-word confirmation did not match"
        );
        assert_eq!(
            recovery_setup_state()
                .unwrap()
                .recovery_confirmed_at_unix_seconds,
            None
        );

        let before = ipc::main_password::now_unix_secs_pub();
        let state =
            record_recovery_word_confirmation(PASSWORD.to_owned(), selected_entries.clone())
                .unwrap();
        let confirmed_at = state
            .recovery_confirmed_at_unix_seconds
            .expect("confirmation time is recorded");
        assert!(confirmed_at >= before);
        assert!(state.kit_unsaved);

        let key = ipc::main_password::get_file_storage_key().unwrap();
        let sealed = std::fs::read(root.join(RECOVERY_KIT_STATUS_FILE)).unwrap();
        let plaintext = ipc::main_password::decrypt_at_rest(&sealed, &key).unwrap();
        let setup_state_json = String::from_utf8(plaintext).unwrap();
        let stored_recovery_word_text_count = selected_entries
            .iter()
            .filter(|entry| setup_state_json.contains(entry.word.as_str()))
            .count();
        assert_eq!(stored_recovery_word_text_count, 0);
        assert!(setup_state_json.contains("recovery_confirmed_at_unix_seconds"));

        println!(
            "TASK_0452_RECOVERY_CONFIRMATION confirmed_at_unix_seconds={confirmed_at} stored_recovery_word_text_count={stored_recovery_word_text_count} setup_state={setup_state_json}"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
