#![cfg(feature = "core")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::account_recovery::{
    mark_recovery_kit_unsaved, record_recovery_word_confirmation, save_normal_setup_completion,
};
use osl_privacy_hub::models::OnboardingPreferences;
use osl_privacy_hub::preferences::PreviewState;

const PASSWORD: &str = "aB3!z9-task-0453";

#[test]
fn direct_finish_fails_before_confirmation_and_succeeds_after_it() {
    assert!(
        include_str!("../src/main.rs")
            .contains("account_recovery::save_normal_setup_completion(&state, preferences)"),
        "the shipping save_onboarding_preferences command must use the guarded finish path"
    );

    let root = temporary_root();
    std::fs::create_dir_all(&root).unwrap();

    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _restore = Restore(root.clone());

    keystore::set_base_dir_override(Some(root.clone()));
    keystore::set_active_account_dir(None);
    let phrase = ipc::main_password::set_main_password(&root, PASSWORD).unwrap();
    mark_recovery_kit_unsaved().unwrap();

    let preferences_path = root.join("preferences.json");
    let preview = PreviewState::load(preferences_path.clone());
    let finish = OnboardingPreferences {
        onboarding_complete: true,
        ..OnboardingPreferences::default()
    };

    let before_error = save_normal_setup_completion(&preview, finish.clone()).unwrap_err();
    assert_eq!(
        before_error,
        "Confirm the requested recovery words before finishing setup"
    );
    assert!(!preview.get().unwrap().onboarding_complete);
    assert!(!preferences_path.exists());

    let words = phrase.split_whitespace().collect::<Vec<_>>();
    let entries = [1usize, 3, 12]
        .into_iter()
        .map(|position| ipc::main_password::RecoveryWordEntry {
            position,
            word: words[position - 1].to_owned(),
        })
        .collect();
    let confirmed = record_recovery_word_confirmation(PASSWORD.to_owned(), entries).unwrap();
    assert!(confirmed.recovery_confirmed_at_unix_seconds.is_some());

    let saved = save_normal_setup_completion(&preview, finish).unwrap();
    assert!(saved.onboarding_complete);
    assert!(
        PreviewState::load(preferences_path)
            .get()
            .unwrap()
            .onboarding_complete
    );

    println!(
        "TASK_0453_DIRECT_FINISH before=refused error={before_error:?} confirmation_record=present after=succeeded onboarding_complete={}",
        saved.onboarding_complete
    );
}

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-task-0453-recovery-finish-{}-{nonce}",
        std::process::id()
    ))
}
