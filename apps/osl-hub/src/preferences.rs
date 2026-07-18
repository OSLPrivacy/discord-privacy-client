use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::models::OnboardingPreferences;

const PREVIEW_STATE_VERSION: u8 = 1;
const MAX_PREFERENCES_BYTES: u64 = 16 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreferencesDocument {
    version: u8,
    onboarding: OnboardingPreferences,
}

impl Default for PreferencesDocument {
    fn default() -> Self {
        Self {
            version: PREVIEW_STATE_VERSION,
            onboarding: OnboardingPreferences::default(),
        }
    }
}

pub struct PreviewState {
    path: PathBuf,
    onboarding: Mutex<OnboardingPreferences>,
}

impl PreviewState {
    pub fn load(path: PathBuf) -> Self {
        let onboarding = read_preferences(&path)
            .map(|document| document.onboarding.fail_closed())
            .unwrap_or_default();

        Self {
            path,
            onboarding: Mutex::new(onboarding),
        }
    }

    pub fn get(&self) -> Result<OnboardingPreferences, String> {
        self.onboarding
            .lock()
            .map(|preferences| preferences.clone())
            .map_err(|_| "preview preferences lock is unavailable".to_owned())
    }

    pub fn save(
        &self,
        preferences: OnboardingPreferences,
    ) -> Result<OnboardingPreferences, String> {
        let preferences = preferences.fail_closed();
        write_preferences(&self.path, &preferences)
            .map_err(|error| format!("could not save preview preferences: {error}"))?;

        let mut current = self
            .onboarding
            .lock()
            .map_err(|_| "preview preferences lock is unavailable".to_owned())?;
        *current = preferences.clone();
        Ok(preferences)
    }

    pub fn reset(&self) -> Result<OnboardingPreferences, String> {
        self.save(OnboardingPreferences::default())
    }
}

fn read_preferences(path: &Path) -> Option<PreferencesDocument> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PREFERENCES_BYTES,
        "preview preferences",
    )
    .ok()
    .flatten()?;
    let document = serde_json::from_slice::<PreferencesDocument>(&bytes).ok()?;
    (document.version == PREVIEW_STATE_VERSION).then_some(document)
}

fn write_preferences(path: &Path, preferences: &OnboardingPreferences) -> Result<(), String> {
    let document = PreferencesDocument {
        version: PREVIEW_STATE_VERSION,
        onboarding: preferences.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|_| "preferences could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_PREFERENCES_BYTES {
        return Err("preview preferences exceed the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "preview preferences")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PlacementMode, SendMode};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("osl-hub-preview-{}-{nonce}", std::process::id()))
            .join("preferences.json")
    }

    #[test]
    fn preferences_round_trip_without_platform_data() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let expected = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::SingleEnter,
            placement_mode: PlacementMode::Compatibility,
            show_plaintext_preview: false,
            acknowledge_experimental_send_risk: true,
        };

        state.save(expected.clone()).expect("save preferences");
        assert_eq!(PreviewState::load(path.clone()).get().unwrap(), expected);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_or_oversized_files_fail_closed_to_defaults() {
        let path = temporary_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; MAX_PREFERENCES_BYTES as usize + 1]).unwrap();

        assert_eq!(
            PreviewState::load(path.clone()).get().unwrap(),
            OnboardingPreferences::default()
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn oversized_recovery_backup_is_never_restored_or_read() {
        let path = temporary_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let backup = path.with_extension("bak");
        fs::write(&backup, vec![b'x'; MAX_PREFERENCES_BYTES as usize + 1]).unwrap();

        assert_eq!(
            PreviewState::load(path.clone()).get().unwrap(),
            OnboardingPreferences::default()
        );
        assert!(!path.exists());
        assert!(backup.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn persisted_experimental_mode_without_acknowledgement_reopens_setup() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let unsafe_preferences = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::DoubleEnter,
            placement_mode: PlacementMode::Compatibility,
            show_plaintext_preview: true,
            acknowledge_experimental_send_risk: false,
        };

        let saved = state.save(unsafe_preferences).expect("save preferences");
        assert!(!saved.onboarding_complete);
        assert!(
            !PreviewState::load(path.clone())
                .get()
                .unwrap()
                .onboarding_complete
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_primary_recovers_last_committed_preferences() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let expected = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::Manual,
            placement_mode: PlacementMode::Atomic,
            show_plaintext_preview: false,
            acknowledge_experimental_send_risk: false,
        };
        state.save(expected.clone()).unwrap();
        fs::rename(&path, path.with_extension("bak")).unwrap();

        let recovered = PreviewState::load(path.clone());
        assert_eq!(recovered.get().unwrap(), expected);
        assert!(path.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
