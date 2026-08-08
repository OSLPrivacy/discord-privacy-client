use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use ipc::unfinished_onboarding::{
    account_usability_snapshot, choose_onboarding_restart_page, record_completed_onboarding,
    record_safe_onboarding_restart_page, restart_unfinished_account, write_setup_status,
    AccountSetupStatus, AccountUsabilitySnapshot, OnboardingRestartPage, SafeOnboardingRestartPage,
};

const CHILD_ROOT_ENV: &str = "OSL_TASK_3646_CHILD_ROOT";
const CHILD_STAGE_ENV: &str = "OSL_TASK_3646_CHILD_STAGE";
const CONTROL_PASSWORD: &str = "task-3646-control-password";
const INTERRUPTED_PASSWORD: &str = "task-3646-interrupted-password";

#[derive(Clone, Copy, Debug)]
enum Checkpoint {
    PasswordSaved,
    KeyCreated,
    PhraseDisplayed,
    KitCopied,
    SavedTicked,
}

impl Checkpoint {
    const ALL: [Self; 5] = [
        Self::PasswordSaved,
        Self::KeyCreated,
        Self::PhraseDisplayed,
        Self::KitCopied,
        Self::SavedTicked,
    ];

    fn id(self) -> &'static str {
        match self {
            Self::PasswordSaved => "password_save",
            Self::KeyCreated => "key_creation",
            Self::PhraseDisplayed => "phrase_display",
            Self::KitCopied => "kit_copy",
            Self::SavedTicked => "saved_tick",
        }
    }

    fn parse(value: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|stage| stage.id() == value)
            .unwrap_or_else(|| panic!("unknown task 3646 child stage: {value}"))
    }

    fn safe_page(self) -> SafeOnboardingRestartPage {
        match self {
            // At this point no account key exists, so replaying Create account
            // is the only page whose prerequisites are all durable.
            Self::PasswordSaved => SafeOnboardingRestartPage::CreateAccount,
            // A key now exists beside the password marker. Recovery is the
            // first safe page that can use those durable prerequisites.
            Self::KeyCreated | Self::PhraseDisplayed | Self::KitCopied => {
                SafeOnboardingRestartPage::Recovery
            }
            // The tick is not the recovery-word confirmation. On restart it
            // may lead only to the confirmation page, never to Unlock.
            Self::SavedTicked => SafeOnboardingRestartPage::RecoveryCheck,
        }
    }

    fn has_identity(self) -> bool {
        !matches!(self, Self::PasswordSaved)
    }
}

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-task-3646-preconfirmation-kills-{}-{nonce}",
        std::process::id()
    ))
}

fn active_file_key() -> [u8; 32] {
    ipc::main_password::get_file_storage_key().expect("password operation installs file key")
}

fn save_identity(directory: &Path, entropy: [u8; 16]) {
    let identity = keystore::identity_from_entropy(entropy, "osl-task-3646".to_owned());
    keystore::save_identity(
        &directory.join("identity.json"),
        &identity,
        &keystore::MemorySealer::new(),
    )
    .expect("save canonical account identity");
}

fn seed_completed_account(directory: &Path, entropy: [u8; 16], password: &str) -> [u8; 32] {
    std::fs::create_dir_all(directory).expect("completed account directory");
    ipc::main_password::set_main_password(directory, password).expect("save completed password");
    let key = active_file_key();
    save_identity(directory, entropy);
    write_setup_status(
        directory,
        &AccountSetupStatus::confirmed(1_800_000_000),
        &key,
    )
    .expect("confirm completed account");
    record_completed_onboarding(directory).expect("record completed onboarding");
    key
}

fn create_interrupted_checkpoint(directory: &Path, checkpoint: Checkpoint) {
    std::fs::create_dir_all(directory).expect("interrupted account directory");
    ipc::main_password::set_main_password(directory, INTERRUPTED_PASSWORD)
        .expect("persist interrupted account password");
    let key = active_file_key();

    // The native unfinished marker is committed in the same trusted password
    // operation before that operation returns to the renderer. It therefore
    // exists at every checkpoint in this test, including password_save.
    write_setup_status(directory, &AccountSetupStatus::unfinished(), &key)
        .expect("persist unfinished account authority");

    if checkpoint.has_identity() {
        save_identity(directory, [0x40 + checkpoint as u8; 16]);
    }
    record_safe_onboarding_restart_page(directory, checkpoint.safe_page())
        .expect("persist checkpoint's safe restart page");
}

fn add_counts(
    first: &AccountUsabilitySnapshot,
    second: &AccountUsabilitySnapshot,
) -> (usize, usize) {
    (
        first.usable_account_count + second.usable_account_count,
        first.usable_key_count + second.usable_key_count,
    )
}

fn snapshot(directory: &Path, key: &[u8; 32]) -> AccountUsabilitySnapshot {
    account_usability_snapshot(directory, key).expect("read account usability snapshot")
}

fn assert_only_control_is_usable(
    checkpoint: Checkpoint,
    moment: &str,
    control: &AccountUsabilitySnapshot,
    interrupted: &AccountUsabilitySnapshot,
) {
    let (accounts, keys) = add_counts(control, interrupted);
    assert_eq!(
        accounts,
        1,
        "checkpoint={} moment={moment}: usable-account count changed",
        checkpoint.id()
    );
    assert_eq!(
        keys,
        1,
        "checkpoint={} moment={moment}: usable-key count changed",
        checkpoint.id()
    );
}

fn kill_child_at_checkpoint(root: &Path, checkpoint: Checkpoint) {
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args([
            "--exact",
            "task_3646_child_persists_one_checkpoint_then_waits_to_be_killed",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ROOT_ENV, root)
        .env(CHILD_STAGE_ENV, checkpoint.id())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start isolated OSL checkpoint process");

    let stdout = child.stdout.take().expect("capture child readiness");
    let expected = format!("TASK3646_CHILD_READY stage={}", checkpoint.id());
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).expect("read child readiness");
        assert_ne!(
            bytes,
            0,
            "checkpoint={} child exited before durable ready marker",
            checkpoint.id()
        );
        if line.contains(&expected) {
            break;
        }
    }

    child.kill().expect("kill OSL checkpoint process");
    let status = child.wait().expect("reap killed OSL checkpoint process");
    assert!(
        !status.success(),
        "checkpoint={} process was not killed",
        checkpoint.id()
    );
}

#[test]
#[ignore = "spawned by the parent test and deliberately killed"]
fn task_3646_child_persists_one_checkpoint_then_waits_to_be_killed() {
    let root = PathBuf::from(std::env::var_os(CHILD_ROOT_ENV).expect("child root env"));
    let checkpoint =
        Checkpoint::parse(&std::env::var(CHILD_STAGE_ENV).expect("child checkpoint env"));
    create_interrupted_checkpoint(&root.join("interrupted"), checkpoint);
    println!("TASK3646_CHILD_READY stage={}", checkpoint.id());
    std::io::stdout().flush().expect("flush child ready marker");

    // The parent must end this process with Child::kill. Returning would turn
    // a crash test into an ordinary graceful-shutdown test.
    loop {
        std::thread::park();
    }
}

#[test]
fn five_literal_kills_preserve_only_the_control_then_one_complete_run_adds_one() {
    let root = temp_root();
    std::fs::create_dir_all(&root).expect("task root");

    for (index, checkpoint) in Checkpoint::ALL.into_iter().enumerate() {
        let scenario = root.join(checkpoint.id());
        let control_dir = scenario.join("control");
        let interrupted_dir = scenario.join("interrupted");
        let control_key =
            seed_completed_account(&control_dir, [0x10 + index as u8; 16], CONTROL_PASSWORD);
        let control_before = snapshot(&control_dir, &control_key);
        assert_eq!(control_before.account_name, "finished");
        assert_eq!(control_before.usable_account_count, 1);
        assert_eq!(control_before.usable_key_count, 1);

        kill_child_at_checkpoint(&scenario, checkpoint);

        // A genuine cold restart begins without the old process's file key.
        ipc::main_password::set_file_storage_key(None);
        let restart_page = choose_onboarding_restart_page(&interrupted_dir);
        assert_eq!(
            restart_page,
            OnboardingRestartPage::Safe(checkpoint.safe_page()),
            "checkpoint={} restarted at an unsafe page",
            checkpoint.id()
        );
        assert!(!restart_page.is_unlock());

        ipc::main_password::verify_main_password(&interrupted_dir, INTERRUPTED_PASSWORD)
            .expect("unlock interrupted status after cold restart");
        let interrupted_key = active_file_key();
        let interrupted_before = snapshot(&interrupted_dir, &interrupted_key);
        assert_eq!(interrupted_before.account_name, "unfinished");
        assert_only_control_is_usable(
            checkpoint,
            "before_kill_restart",
            &control_before,
            &interrupted_before,
        );

        let fresh_process_state = ipc::AppState::new();
        assert!(
            restart_unfinished_account(&fresh_process_state, &interrupted_dir, &interrupted_key,)
                .expect("restart unfinished account"),
            "checkpoint={} was not recognized as unfinished",
            checkpoint.id()
        );

        let interrupted_after = snapshot(&interrupted_dir, &interrupted_key);
        let control_after = snapshot(&control_dir, &control_key);
        assert_eq!(interrupted_after.account_name, "unfinished");
        assert_only_control_is_usable(
            checkpoint,
            "after_kill_restart",
            &control_after,
            &interrupted_after,
        );
        assert!(
            !interrupted_dir.join("identity.json").exists(),
            "TASK3646 extra key checkpoint={} path={}",
            checkpoint.id(),
            interrupted_dir.join("identity.json").display()
        );
        assert!(
            control_dir.join("identity.json").is_file(),
            "checkpoint={} control key was removed",
            checkpoint.id()
        );

        println!(
            "TASK3646_KILL checkpoint={} before_accounts=1 before_keys=1 after_accounts=1 after_keys=1 account_name={} safe_page={} safe_route={} extra_interrupted_keys=0",
            checkpoint.id(),
            interrupted_after.account_name,
            restart_page.page_name(),
            restart_page.route(),
        );
    }

    // This scenario takes the same durable path without a kill and records the
    // recovery-word confirmation before completion. The new account is now a
    // second usable account/key rather than another provisional key.
    let uninterrupted = root.join("uninterrupted");
    let uninterrupted_control = uninterrupted.join("control");
    let uninterrupted_new = uninterrupted.join("new-account");
    let uninterrupted_control_key =
        seed_completed_account(&uninterrupted_control, [0x70; 16], CONTROL_PASSWORD);
    let uninterrupted_new_key =
        seed_completed_account(&uninterrupted_new, [0x71; 16], INTERRUPTED_PASSWORD);
    let control = snapshot(&uninterrupted_control, &uninterrupted_control_key);
    let completed = snapshot(&uninterrupted_new, &uninterrupted_new_key);
    let (accounts, keys) = add_counts(&control, &completed);
    assert_eq!(accounts, 2);
    assert_eq!(keys, 2);
    assert_eq!(
        choose_onboarding_restart_page(&uninterrupted_new).page_name(),
        "Unlock"
    );
    println!(
        "TASK3646_UNINTERRUPTED usable_accounts={accounts} usable_keys={keys} account_name={} restart_page=Unlock",
        completed.account_name,
    );
    println!(
        "TASK3646_FINISH killed_checkpoints=5 control_before_accounts=1 control_before_keys=1 control_after_accounts=1 control_after_keys=1 interrupted_name=unfinished uninterrupted_accounts={accounts} uninterrupted_keys={keys}"
    );

    ipc::main_password::set_file_storage_key(None);
    let _ = std::fs::remove_dir_all(root);
}
