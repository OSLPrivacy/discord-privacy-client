#![cfg(feature = "core")]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ipc::commands::{cmd_osl_check_recovery_words, RecoveryWordRetypeEntryDto};
use ipc::main_password::RecoveryWordEntry;
use osl_privacy_hub::account_recovery::{
    mark_recovery_kit_unsaved, record_recovery_word_confirmation, recovery_setup_state,
    save_normal_setup_completion,
};
use osl_privacy_hub::models::OnboardingPreferences;
use osl_privacy_hub::preferences::PreviewState;

const PASSWORD: &str = "aB3!z9-task-0454";
const FINISH_REFUSAL: &str = "Confirm the requested recovery words before finishing setup";
const CONFIRMATION_REFUSAL: &str = "OSL recovery-word confirmation did not match";

#[test]
fn wrong_missing_reordered_and_correct_words_gate_setup_finish() {
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

    let words = phrase.split_whitespace().collect::<Vec<_>>();
    assert_eq!(words.len(), 12, "fixture phrase must have twelve words");
    let distinct = words
        .iter()
        .position(|word| *word != words[0])
        .expect("fixture phrase must contain at least two distinct words");
    let third = (0..words.len())
        .find(|index| *index != 0 && *index != distinct)
        .unwrap();
    let mut selected = [0usize, distinct, third];
    selected.sort_unstable();
    let correct = selected
        .into_iter()
        .map(|index| RecoveryWordEntry {
            position: index + 1,
            word: words[index].to_owned(),
        })
        .collect::<Vec<_>>();

    let mut wrong = clone_entries(&correct);
    wrong[0].word = "not-a-recovery-word".to_owned();
    refuse_attempt("wrong", &preview, &finish, wrong);

    let missing = clone_entries(&correct[..2]);
    refuse_attempt("missing", &preview, &finish, missing);

    let mut reordered = clone_entries(&correct);
    reordered.rotate_left(1);
    assert!(
        reordered
            .windows(2)
            .any(|pair| pair[0].position >= pair[1].position),
        "the no-order fixture must really lack ascending recovery-word order"
    );
    refuse_attempt("reordered", &preview, &finish, clone_entries(&reordered));
    println!(
        "TASK_0454 fixture_without_ordered_recovery_words direct_ok={} check_failed={}",
        direct_check(&reordered),
        !direct_check(&reordered)
    );

    assert!(
        direct_check(&correct),
        "only the correct ordered words pass"
    );
    let confirmed = record_recovery_word_confirmation(PASSWORD.to_owned(), clone_entries(&correct))
        .expect("correct ordered words record confirmation");
    assert!(confirmed.recovery_confirmed_at_unix_seconds.is_some());
    let saved = save_normal_setup_completion(&preview, finish).expect("confirmed setup finishes");
    assert!(saved.onboarding_complete);
    assert!(
        PreviewState::load(preferences_path)
            .get()
            .unwrap()
            .onboarding_complete
    );
    println!(
        "TASK_0454 attempt=correct ordered_count={} direct_ok=true confirmation_record=true finish=unlocked onboarding_complete={}",
        correct.len(),
        saved.onboarding_complete
    );
}

fn refuse_attempt(
    label: &str,
    preview: &PreviewState,
    finish: &OnboardingPreferences,
    entries: Vec<RecoveryWordEntry>,
) {
    assert!(!direct_check(&entries), "{label} direct check must fail");
    assert_eq!(
        record_recovery_word_confirmation(PASSWORD.to_owned(), clone_entries(&entries))
            .unwrap_err(),
        CONFIRMATION_REFUSAL
    );
    assert_eq!(
        recovery_setup_state()
            .unwrap()
            .recovery_confirmed_at_unix_seconds,
        None
    );
    assert_eq!(
        save_normal_setup_completion(preview, finish.clone()).unwrap_err(),
        FINISH_REFUSAL
    );
    assert!(!preview.get().unwrap().onboarding_complete);
    println!(
        "TASK_0454 attempt={label} supplied_count={} direct_ok=false confirmation_record=false finish=refused",
        entries.len()
    );
}

fn direct_check(entries: &[RecoveryWordEntry]) -> bool {
    let entries = entries
        .iter()
        .map(|entry| RecoveryWordRetypeEntryDto {
            position: entry.position as u8,
            word: entry.word.clone(),
        })
        .collect();
    cmd_osl_check_recovery_words(PASSWORD.to_owned(), entries)
        .expect("direct recovery-word command evaluates the fixture")
        .ok
}

fn clone_entries(entries: &[RecoveryWordEntry]) -> Vec<RecoveryWordEntry> {
    entries
        .iter()
        .map(|entry| RecoveryWordEntry {
            position: entry.position,
            word: entry.word.clone(),
        })
        .collect()
}

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-task-0454-recovery-confirmation-{}-{nonce}",
        std::process::id()
    ))
}
