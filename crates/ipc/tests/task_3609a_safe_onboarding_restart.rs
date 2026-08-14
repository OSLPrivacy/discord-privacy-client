use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ipc::unfinished_onboarding::{
    choose_onboarding_restart_page, record_completed_onboarding,
    record_safe_onboarding_restart_page, OnboardingRestartPage, SafeOnboardingRestartPage,
    ONBOARDING_RESTART_FILE,
};

const CONTROL_PASSWORD: &str = "safe-restart-control-3609a";

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-task-3609a-restart-{}-{nonce}",
        std::process::id()
    ))
}

fn seed_locked_partial_account(directory: &Path) {
    std::fs::create_dir_all(directory).expect("partial account directory");
    std::fs::write(directory.join("identity.json"), b"fixture-identity")
        .expect("partial identity fixture");
    std::fs::write(
        directory.join("password_marker.json"),
        b"fixture-password-marker",
    )
    .expect("partial password fixture");
}

fn fingerprint(key: &[u8]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn five_interruptions_choose_their_safe_page_and_completed_control_unlocks_exact_key() {
    let root = temp_root();
    std::fs::create_dir_all(&root).expect("fixture root");

    let create_dir = root.join("after-identity-before-password");
    std::fs::create_dir_all(&create_dir).expect("create fixture directory");
    std::fs::write(create_dir.join("identity.json"), b"provisional-identity")
        .expect("provisional identity");

    let fixtures = [
        (
            "after_identity_before_password",
            create_dir,
            SafeOnboardingRestartPage::CreateAccount,
            false,
        ),
        (
            "recovery_words_shown",
            root.join("recovery"),
            SafeOnboardingRestartPage::Recovery,
            true,
        ),
        (
            "recovery_retype_started",
            root.join("recovery-check"),
            SafeOnboardingRestartPage::RecoveryCheck,
            true,
        ),
        (
            "privacy_choices_started",
            root.join("privacy"),
            SafeOnboardingRestartPage::Privacy,
            true,
        ),
        (
            "app_choice_started",
            root.join("apps"),
            SafeOnboardingRestartPage::Apps,
            true,
        ),
    ];

    let mut returned_pages = Vec::new();
    for (name, directory, expected, persist_page) in &fixtures {
        if *persist_page {
            seed_locked_partial_account(directory);
            record_safe_onboarding_restart_page(directory, *expected)
                .expect("record exact safe page");
        }
        let returned = choose_onboarding_restart_page(directory);
        assert_eq!(returned, OnboardingRestartPage::Safe(*expected));
        assert!(!returned.is_unlock(), "interrupted fixture returned Unlock");
        println!(
            "TASK_3609A_INTERRUPTED fixture={name} expected_page={} returned_page={} route={}",
            expected.page_name(),
            returned.page_name(),
            returned.route(),
        );
        returned_pages.push(returned);
    }

    // A renderer-controlled or corrupted value cannot smuggle Unlock into the
    // unfinished allowlist. Strict decoding makes it the safe default.
    let injected = root.join("injected-unlock");
    seed_locked_partial_account(&injected);
    std::fs::write(
        injected.join(ONBOARDING_RESTART_FILE),
        br#"{"version":1,"state":"unfinished","page":"unlock"}"#,
    )
    .expect("injected marker fixture");
    assert_eq!(
        choose_onboarding_restart_page(&injected),
        OnboardingRestartPage::Safe(SafeOnboardingRestartPage::CreateAccount)
    );

    // The control uses the real password marker and canonical sealed identity.
    // A cold restart clears the process key, chooses Unlock only for the
    // explicit completed state, verifies the real password, then opens the
    // exact same public-key fingerprint.
    let control_dir = root.join("completed-control");
    std::fs::create_dir_all(&control_dir).expect("control directory");
    ipc::main_password::set_main_password(&control_dir, CONTROL_PASSWORD)
        .expect("set control password");
    let sealer = keystore::MemorySealer::new();
    let control_identity =
        keystore::identity_from_entropy([0x5a; 16], "osl-task-3609a-control".to_owned());
    let expected_fingerprint = fingerprint(control_identity.ed25519_public.as_bytes());
    keystore::save_identity(
        &control_dir.join("identity.json"),
        &control_identity,
        &sealer,
    )
    .expect("save control identity");
    record_completed_onboarding(&control_dir).expect("complete control onboarding");

    ipc::main_password::set_file_storage_key(None);
    let control_restart = choose_onboarding_restart_page(&control_dir);
    assert_eq!(control_restart, OnboardingRestartPage::Unlock);
    ipc::main_password::verify_main_password(&control_dir, CONTROL_PASSWORD)
        .expect("completed control unlocks");
    let unlocked_control = keystore::load_identity(&control_dir.join("identity.json"), &sealer)
        .expect("load unlocked control identity");
    let unlocked_fingerprint = fingerprint(unlocked_control.ed25519_public.as_bytes());
    assert_eq!(unlocked_fingerprint, expected_fingerprint);

    let interrupted_unlock_count = returned_pages
        .iter()
        .filter(|page| page.is_unlock())
        .count();
    assert_eq!(returned_pages.len(), 5);
    assert_eq!(interrupted_unlock_count, 0);
    println!(
        "TASK_3609A_FINISH interrupted_fixture_count={} interrupted_unlock_count={interrupted_unlock_count} control_restart_page={} control_key_fingerprint={unlocked_fingerprint}",
        returned_pages.len(),
        control_restart.page_name(),
    );

    ipc::main_password::set_file_storage_key(None);
    let _ = std::fs::remove_dir_all(root);
}
