use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ipc::unfinished_onboarding::{
    account_usability_snapshot, restart_pre_password_unfinished_account,
    restart_unfinished_account, write_setup_status, AccountSetupStatus, RECOVERY_KIT_STATUS_FILE,
};

const FILE_KEY: [u8; 32] = [0x36; 32];

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-task-3609-unfinished-{}-{nonce}",
        std::process::id()
    ))
}

fn save_identity(
    directory: &Path,
    entropy: [u8; 16],
    sealer: &keystore::MemorySealer,
) -> keystore::Identity {
    std::fs::create_dir_all(directory).expect("account directory");
    let identity = keystore::identity_from_entropy(entropy, "osl-task-3609".to_owned());
    keystore::save_identity(&directory.join("identity.json"), &identity, sealer)
        .expect("save canonical identity key");
    identity
}

#[test]
fn completed_control_and_restarted_interruption_leave_one_usable_account_and_key() {
    let root = temp_root();
    let control_dir = root.join("control");
    let interrupted_dir = root.join("interrupted");
    let sealer = keystore::MemorySealer::new();

    let control_identity = save_identity(&control_dir, [0x11; 16], &sealer);
    let control_public_key = control_identity.ed25519_public;
    write_setup_status(
        &control_dir,
        &AccountSetupStatus::confirmed(1_800_000_000),
        &FILE_KEY,
    )
    .expect("confirm control recovery words");

    let old_interrupted_identity = save_identity(&interrupted_dir, [0x22; 16], &sealer);
    let old_interrupted_public_key = old_interrupted_identity.ed25519_public;
    std::fs::write(interrupted_dir.join("prekeys.json"), b"old-prekeys")
        .expect("old interrupted prekeys");
    write_setup_status(
        &interrupted_dir,
        &AccountSetupStatus::unfinished(),
        &FILE_KEY,
    )
    .expect("mark interrupted account unfinished");

    let state = ipc::AppState::new();
    state
        .try_install_identity(old_interrupted_identity)
        .expect("install interrupted identity in live state");
    assert!(
        restart_unfinished_account(&state, &interrupted_dir, &FILE_KEY)
            .expect("start unfinished account again")
    );
    assert!(!state.has_identity(), "old live key must be cleared");
    assert!(
        !interrupted_dir.join("identity.json").exists(),
        "old sealed identity key must be removed before replacement"
    );
    assert!(
        !interrupted_dir.join("prekeys.json").exists(),
        "old prekeys must be removed before replacement"
    );
    assert!(
        interrupted_dir.join(RECOVERY_KIT_STATUS_FILE).is_file(),
        "unfinished status must survive key cleanup"
    );

    let replacement_identity = save_identity(&interrupted_dir, [0x33; 16], &sealer);
    assert_ne!(
        replacement_identity.ed25519_public, old_interrupted_public_key,
        "starting again must not reuse the interrupted key"
    );

    let control_after = keystore::load_identity(&control_dir.join("identity.json"), &sealer)
        .expect("control key remains usable");
    assert_eq!(
        control_after.ed25519_public, control_public_key,
        "retrying the second account must not replace the completed control key"
    );

    let control = account_usability_snapshot(&control_dir, &FILE_KEY).expect("control snapshot");
    let interrupted =
        account_usability_snapshot(&interrupted_dir, &FILE_KEY).expect("interrupted snapshot");
    let usable_account_count = control.usable_account_count + interrupted.usable_account_count;
    let usable_key_count = control.usable_key_count + interrupted.usable_key_count;

    assert_eq!(usable_account_count, 1);
    assert_eq!(usable_key_count, 1);
    assert_eq!(interrupted.account_name, "unfinished");
    assert_eq!(control.usable_key_count, 1);

    // The still-earlier interruption (identity created, password not yet set)
    // also retries safely without pretending it is a legacy completed account.
    let pre_password_dir = root.join("pre-password-interruption");
    let pre_password_identity = save_identity(&pre_password_dir, [0x44; 16], &sealer);
    let pre_password_state = ipc::AppState::new();
    pre_password_state
        .try_install_identity(pre_password_identity)
        .expect("install pre-password interrupted identity");
    let pre_password =
        account_usability_snapshot(&pre_password_dir, &FILE_KEY).expect("pre-password snapshot");
    assert_eq!(pre_password.account_name, "unfinished");
    assert_eq!(pre_password.usable_key_count, 0);
    assert!(
        restart_pre_password_unfinished_account(&pre_password_state, &pre_password_dir,)
            .expect("restart pre-password interruption")
    );
    assert!(!pre_password_dir.join("identity.json").exists());

    println!(
        "TASK_3609_FINISH usable_account_count={usable_account_count} usable_key_count={usable_key_count} interrupted_account_name={} control_usable_key_count={} old_interrupted_key_replaced=true",
        interrupted.account_name,
        control.usable_key_count,
    );

    let _ = std::fs::remove_dir_all(root);
}
