//! Encrypted, account-scoped dismissal state for contextual coach tips.
//!
//! This state records only explicit owner choices. It never records that a tip
//! was rendered, which route was visited, when anything happened, or how any
//! underlying control was used. Every valid document is padded to the same
//! plaintext length before encryption so a local file-size observer cannot
//! distinguish fresh, partly dismissed, dismiss-all, or reset state.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const COACH_TIP_STATE_FILE: &str = "profile_ui_state_v1.json";
pub const COACH_TIP_STATE_VERSION: u32 = 1;
pub const COACH_TIP_CATALOG_VERSION: u32 = 1;
pub const COACH_TIP_REVISION: u32 = 1;
pub const COACH_TIP_IDS: [&str; 3] = ["protect-message", "private-scan", "switch-profile"];

const COACH_TIP_PLAINTEXT_BYTES: usize = 512;
const MAX_COACH_TIP_SEALED_BYTES: u64 = 4 * 1024;
const STORAGE_LABEL: &str = "OSL profile UI state";

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CoachTipState {
    pub version: u32,
    pub catalog_version: u32,
    pub dismissed: BTreeMap<String, u32>,
}

impl Default for CoachTipState {
    fn default() -> Self {
        Self {
            version: COACH_TIP_STATE_VERSION,
            catalog_version: COACH_TIP_CATALOG_VERSION,
            dismissed: BTreeMap::new(),
        }
    }
}

impl CoachTipState {
    pub fn validate(self) -> Result<Self, String> {
        if self.version != COACH_TIP_STATE_VERSION
            || self.catalog_version != COACH_TIP_CATALOG_VERSION
            || self.dismissed.len() > COACH_TIP_IDS.len()
            || self.dismissed.iter().any(|(id, revision)| {
                !COACH_TIP_IDS.contains(&id.as_str()) || *revision != COACH_TIP_REVISION
            })
        {
            return Err("OSL profile UI state is invalid or unsupported".to_owned());
        }
        Ok(self)
    }
}

pub fn get_active() -> Result<CoachTipState, String> {
    let key = active_file_key()?;
    let path = active_state_path()?;
    Ok(load_with_key(&path, &key)?.unwrap_or_default())
}

pub fn save_active(state: CoachTipState) -> Result<CoachTipState, String> {
    let state = state.validate()?;
    let key = active_file_key()?;
    write_with_key(&active_state_path()?, &state, &key)?;
    Ok(state)
}

/// Explicitly dismiss one known revision. Merely rendering a tip never calls
/// this function.
pub fn dismiss_tip(id: &str) -> Result<CoachTipState, String> {
    if !COACH_TIP_IDS.contains(&id) {
        return Err("OSL profile UI state input is invalid".to_owned());
    }
    let mut state = get_active()?;
    state.dismissed.insert(id.to_owned(), COACH_TIP_REVISION);
    save_active(state)
}

/// Explicit dismiss-all is represented as the three closed catalog entries,
/// not as a behavioral counter or a special inferred flag.
pub fn dismiss_all() -> Result<CoachTipState, String> {
    let mut state = get_active()?;
    state.dismissed = COACH_TIP_IDS
        .iter()
        .map(|id| ((*id).to_owned(), COACH_TIP_REVISION))
        .collect();
    save_active(state)
}

pub fn reset_active() -> Result<CoachTipState, String> {
    save_active(CoachTipState::default())
}

fn active_file_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL main password must be unlocked".to_owned())
}

fn active_state_path() -> Result<PathBuf, String> {
    // `osl_config_dir` is the active slot directory when multiple identities
    // exist and the flat first-account directory otherwise.
    keystore::osl_config_dir()
        .map(|directory| directory.join(COACH_TIP_STATE_FILE))
        .map_err(|_| "OSL active identity storage is unavailable".to_owned())
}

fn load_with_key(path: &Path, key: &[u8; 32]) -> Result<Option<CoachTipState>, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_COACH_TIP_SEALED_BYTES,
        STORAGE_LABEL,
    )?
    else {
        return Ok(None);
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("OSL profile UI state is not encrypted".to_owned());
    }
    let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "OSL profile UI state could not be decrypted".to_owned())?;
    if plaintext.len() != COACH_TIP_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile UI state has an invalid storage size".to_owned());
    }
    let decoded = serde_json::from_slice::<CoachTipState>(&plaintext);
    plaintext.zeroize();
    decoded
        .map_err(|_| "OSL profile UI state is malformed".to_owned())?
        .validate()
        .map(Some)
}

fn write_with_key(path: &Path, state: &CoachTipState, key: &[u8; 32]) -> Result<(), String> {
    let state = state.clone().validate()?;
    let mut plaintext = serde_json::to_vec(&state)
        .map_err(|_| "OSL profile UI state could not be encoded".to_owned())?;
    if plaintext.len() > COACH_TIP_PLAINTEXT_BYTES {
        plaintext.zeroize();
        return Err("OSL profile UI state exceeds its storage limit".to_owned());
    }
    plaintext.resize(COACH_TIP_PLAINTEXT_BYTES, b' ');
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, key)
        .map_err(|_| "OSL profile UI state encryption failed".to_owned());
    plaintext.zeroize();
    let sealed = encrypted?;
    if sealed.len() as u64 > MAX_COACH_TIP_SEALED_BYTES {
        return Err("OSL encrypted profile UI state exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, STORAGE_LABEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: [u8; 32] = [0x68; 32];
    const KEY_B: [u8; 32] = [0x54; 32];

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-profile-ui-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn assert_observer_recovers_zero_meaning(bytes: &[u8]) {
        assert!(ipc::main_password::has_enc_magic(bytes));
        let observed = String::from_utf8_lossy(bytes);
        for forbidden in [
            "protect-message",
            "private-scan",
            "switch-profile",
            "dismissed",
            "catalogVersion",
            "route",
            "shown",
            "clicked",
            "timestamp",
        ] {
            assert_eq!(
                observed.matches(forbidden).count(),
                0,
                "observer recovered {forbidden}"
            );
        }
    }

    fn assert_path_recovers_zero_meaning(path: &Path) {
        let observed = path.to_string_lossy().to_ascii_lowercase();
        for forbidden in ["coach", "tip", "dismiss", "behavio"] {
            assert_eq!(
                observed.matches(forbidden).count(),
                0,
                "path observer recovered {forbidden}"
            );
        }
    }

    #[test]
    fn explicit_dismiss_restart_dismiss_all_and_reset_are_durable_and_opaque() {
        let root = temporary_root("lifecycle");
        let path = root.join(COACH_TIP_STATE_FILE);
        assert_path_recovers_zero_meaning(&path);

        let fresh = CoachTipState::default();
        write_with_key(&path, &fresh, &KEY_A).unwrap();
        let fresh_raw = std::fs::read(&path).unwrap();
        assert_observer_recovers_zero_meaning(&fresh_raw);
        assert_eq!(load_with_key(&path, &KEY_A).unwrap(), Some(fresh));

        let mut one = CoachTipState::default();
        one.dismissed
            .insert("protect-message".to_owned(), COACH_TIP_REVISION);
        write_with_key(&path, &one, &KEY_A).unwrap();
        let one_raw = std::fs::read(&path).unwrap();
        let one_backup = std::fs::read(path.with_extension("bak")).unwrap();
        assert_eq!(fresh_raw.len(), one_raw.len());
        assert_eq!(one_raw.len(), one_backup.len());
        assert_observer_recovers_zero_meaning(&one_raw);
        assert_observer_recovers_zero_meaning(&one_backup);
        assert_eq!(load_with_key(&path, &KEY_A).unwrap(), Some(one));

        let all = CoachTipState {
            dismissed: COACH_TIP_IDS
                .iter()
                .map(|id| ((*id).to_owned(), COACH_TIP_REVISION))
                .collect(),
            ..CoachTipState::default()
        };
        write_with_key(&path, &all, &KEY_A).unwrap();
        assert_eq!(load_with_key(&path, &KEY_A).unwrap(), Some(all));
        assert_eq!(std::fs::read(&path).unwrap().len(), fresh_raw.len());

        write_with_key(&path, &CoachTipState::default(), &KEY_A).unwrap();
        let reset_after_restart = load_with_key(&path, &KEY_A).unwrap().unwrap();
        assert!(reset_after_restart.dismissed.is_empty());
        assert_eq!(std::fs::read(&path).unwrap().len(), fresh_raw.len());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_profiles_remain_independent_and_wrong_profile_key_is_refused() {
        let root = temporary_root("profiles");
        let profile_a = root.join("profile-a").join(COACH_TIP_STATE_FILE);
        let profile_b = root.join("profile-b").join(COACH_TIP_STATE_FILE);
        assert_path_recovers_zero_meaning(&profile_a);
        assert_path_recovers_zero_meaning(&profile_b);
        let mut state_a = CoachTipState::default();
        state_a
            .dismissed
            .insert("private-scan".to_owned(), COACH_TIP_REVISION);

        write_with_key(&profile_a, &state_a, &KEY_A).unwrap();
        write_with_key(&profile_b, &CoachTipState::default(), &KEY_B).unwrap();

        assert_eq!(load_with_key(&profile_a, &KEY_A).unwrap(), Some(state_a));
        assert_eq!(
            load_with_key(&profile_b, &KEY_B).unwrap(),
            Some(CoachTipState::default())
        );
        assert!(load_with_key(&profile_a, &KEY_B).is_err());
        assert!(load_with_key(&profile_b, &KEY_A).is_err());
        assert_observer_recovers_zero_meaning(&std::fs::read(profile_a).unwrap());
        assert_observer_recovers_zero_meaning(&std::fs::read(profile_b).unwrap());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn schema_is_closed_and_file_is_registered_for_migration_rotation_and_sync() {
        let mut unknown = CoachTipState::default();
        unknown.dismissed.insert("unknown-tip".to_owned(), 1);
        assert!(unknown.validate().is_err());
        let mut wrong_revision = CoachTipState::default();
        wrong_revision
            .dismissed
            .insert("protect-message".to_owned(), 2);
        assert!(wrong_revision.validate().is_err());
        assert!(serde_json::from_str::<CoachTipState>(
            r#"{"version":1,"catalogVersion":1,"dismissed":{},"shown":true}"#
        )
        .is_err());

        assert!(crate::identity_registry::account_artifacts().contains(&COACH_TIP_STATE_FILE));
        assert!(ipc::main_password::AT_REST_STATE_FILES.contains(&COACH_TIP_STATE_FILE));
        assert!(ipc::commands::osl_export_files().contains(&COACH_TIP_STATE_FILE));
    }

    #[test]
    fn plaintext_malformed_and_non_fixed_size_documents_fail_closed() {
        let root = temporary_root("invalid");
        let path = root.join(COACH_TIP_STATE_FILE);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&path, br#"{"version":1,"catalogVersion":1,"dismissed":{}}"#).unwrap();
        assert!(load_with_key(&path, &KEY_A).is_err());

        let short = ipc::main_password::encrypt_at_rest(
            br#"{"version":1,"catalogVersion":1,"dismissed":{}}"#,
            &KEY_A,
        )
        .unwrap();
        std::fs::write(&path, short).unwrap();
        assert!(load_with_key(&path, &KEY_A).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
