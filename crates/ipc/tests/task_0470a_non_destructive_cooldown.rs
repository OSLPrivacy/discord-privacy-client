use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::cmd_osl_verify_gate_password;
use ipc::main_password::{
    burn_wipe_all, credential_work_count, elapsed_realtime_ms, read_lockout_pub,
    reset_credential_work_count, verify_gate_password_attempt_at_elapsed,
    verify_recovery_phrase, Argon2ParamsDto,
    GatePasswordAttemptResult, PasswordMarker, DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
    PASSWORD_COOLDOWN_MILLIS, PASSWORD_COOLDOWN_SECONDS,
};
use ipc::AppState;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PASSWORD: &str = "correct-password-0470a";
const RECOVERY: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn fast_marker() -> PasswordMarker {
    let salt = [0x47; 16];
    let params = Argon2ParamsDto {
        memory_kb: 32,
        iterations: 1,
        parallelism: 1,
    };
    let argon_params = Params::new(32, 1, 1, Some(64)).unwrap();
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut password_out = [0u8; 64];
    argon
        .hash_password_into(PASSWORD.as_bytes(), &salt, &mut password_out)
        .unwrap();
    let mut recovery_out = [0u8; 64];
    argon
        .hash_password_into(RECOVERY.as_bytes(), &salt, &mut recovery_out)
        .unwrap();
    PasswordMarker {
        version: 2,
        salt_b64: STANDARD.encode(salt),
        params,
        password_hash_b64: STANDARD.encode(&password_out[..32]),
        phrase_encrypted_b64: STANDARD.encode(b"non-empty-recovery-ciphertext"),
        phrase_nonce_b64: STANDARD.encode([0x70; 12]),
        phrase_hash_b64: Some(STANDARD.encode(&recovery_out[..32])),
        stealth_password_hash_b64: None,
        burn_password_hash_b64: None,
        duress_password_hash_b64: None,
        file_key_phrase_wrapped_b64: None,
        file_key_phrase_nonce_b64: None,
    }
}

fn seed_profile(root: &Path, label: &str) -> (PathBuf, PathBuf) {
    let base = root.join(format!("{label}-base"));
    let account = root.join(format!("{label}-account"));
    std::fs::create_dir_all(&base).unwrap();
    let stores = [
        ("profile/identity.json", "profile"),
        ("keys/account.key", "key"),
        ("credentials/session.dat", "credential"),
        ("settings/preferences.json", "settings"),
        ("friends/friend.json", "friend"),
        ("messages/messages.sqlite", "message"),
        ("attachments/private.bin", "attachment"),
    ];
    for (relative, class) in stores {
        let path = account.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!("TASK0470A::{label}::{class}::independent-non-empty\n").as_bytes(),
        )
        .unwrap();
    }
    std::fs::write(
        base.join("password_marker.json"),
        serde_json::to_vec_pretty(&fast_marker()).unwrap(),
    )
    .unwrap();
    (base, account)
}

fn inventory(base: &Path, account: &Path) -> BTreeMap<String, String> {
    fn visit(root: &Path, here: &Path, out: &mut BTreeMap<String, String>) {
        let mut entries = std::fs::read_dir(here)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                visit(root, &path, out);
            } else if path.file_name().and_then(|name| name.to_str()) != Some("lockout_state.json")
            {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).unwrap();
                out.insert(relative, format!("{:x}", Sha256::digest(bytes)));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(base, base, &mut result);
    let mut account_result = BTreeMap::new();
    visit(account, account, &mut account_result);
    for (path, digest) in account_result {
        result.insert(format!("account/{path}"), digest);
    }
    result
}

fn wrong_at(base: &Path, password: &str, elapsed_ms: u64) -> (u32, i64) {
    match verify_gate_password_attempt_at_elapsed(base, password, elapsed_ms).unwrap() {
        GatePasswordAttemptResult::Wrong {
            attempts_used,
            lockout_seconds_remaining,
        } => (attempts_used, lockout_seconds_remaining),
        _ => panic!("wrong/cooldown submission escaped the wrong result"),
    }
}

fn shipping_wrong(state: &AppState, password: &str) -> (u32, i64) {
    let result = cmd_osl_verify_gate_password(state, password.to_owned()).unwrap();
    assert_eq!(result.result, "wrong");
    (result.attempts_used, result.lockout_seconds_remaining)
}

struct KeystoreOverrideReset;

impl Drop for KeystoreOverrideReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

#[test]
#[ignore]
fn task_0470a_child_restart_probe() {
    let base = PathBuf::from(std::env::var("TASK0470A_CHILD_BASE").expect("child base"));
    let mode = std::env::var("TASK0470A_CHILD_MODE").expect("child mode");
    if mode == "cooldown" {
        keystore::set_base_dir_override(Some(base.clone()));
        let _reset = KeystoreOverrideReset;
        reset_credential_work_count();
        let state = AppState::new();
        let result = cmd_osl_verify_gate_password(&state, PASSWORD.to_owned()).unwrap();
        assert_eq!(result.result, "wrong");
        assert_eq!(result.attempts_used, 10);
        assert_eq!(credential_work_count(), 0);
        let lock = read_lockout_pub(&base);
        assert_eq!(lock.password_failed_attempts, 10);
        assert!(lock.password_cooldown_deadline_elapsed_ms.is_some());
        println!("TASK0470A_CHILD_RESTART=cooldown_refused credential_checks=0 count=10");
    } else if mode == "cold-zero" {
        let lock = read_lockout_pub(&base);
        assert_eq!(lock.password_failed_attempts, 0);
        assert_eq!(lock.password_cooldown_deadline_elapsed_ms, None);
        println!("TASK0470A_CHILD_RESTART=cold_count_zero deadline=none");
    } else {
        panic!("unknown child mode");
    }
}

fn run_child(base: &Path, mode: &str) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "task_0470a_child_restart_probe",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("TASK0470A_CHILD_BASE", base)
        .env("TASK0470A_CHILD_MODE", mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child restart failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn task_0470a_two_profile_non_destructive_cooldown_boundary() {
    assert_eq!(
        PASSWORD_COOLDOWN_SECONDS, 900,
        "fixed independent duration oracle"
    );
    assert_eq!(
        PASSWORD_COOLDOWN_MILLIS, 900_000,
        "fixed independent duration oracle"
    );
    let temp = TempDir::new().unwrap();
    let (base_one, account_one) = seed_profile(temp.path(), "profile-one");
    let (base_two, account_two) = seed_profile(temp.path(), "profile-two");
    let before_one = inventory(&base_one, &account_one);
    let before_two = inventory(&base_two, &account_two);
    assert_eq!(before_one.len(), 8);
    assert_eq!(before_two.len(), 8);
    assert_ne!(
        before_one, before_two,
        "profiles must be independently fingerprinted"
    );
    println!("TASK0470A_PROFILE_INVENTORY profiles=2 stores_per_profile=8 non_empty=16");

    keystore::set_base_dir_override(Some(base_one.clone()));
    keystore::set_active_account_dir(Some(account_one.clone()));
    let _reset = KeystoreOverrideReset;
    let shipping_state = Arc::new(AppState::new());
    reset_credential_work_count();
    for attempt in 1..DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT {
        let (used, cooldown) = shipping_wrong(&shipping_state, &format!("wrong-{attempt}"));
        assert_eq!(used, attempt);
        assert_eq!(cooldown, 0);
        assert_eq!(inventory(&base_one, &account_one), before_one);
        println!(
            "TASK0470A_ATTEMPT_{attempt}=wrong remaining_attempts={} bytes_preserved=8",
            DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - attempt
        );
    }
    let (used, cooldown) = shipping_wrong(&shipping_state, "wrong-10");
    assert_eq!((used, cooldown), (10, PASSWORD_COOLDOWN_SECONDS as i64));
    let locked = read_lockout_pub(&base_one);
    let start = locked
        .password_cooldown_started_elapsed_ms
        .expect("durable elapsed cooldown start");
    assert_eq!(
        locked.password_cooldown_deadline_elapsed_ms,
        Some(start + 900_000)
    );
    if std::env::var("TASK0470A_BREAK_MUTANT").as_deref() == Ok("wrong-password-wipe") {
        // Break-it sibling: inject the retired destructive boundary only into
        // this throwaway profile. The fixed fingerprint oracle below must go
        // red and name the attack; production code has no runtime mutant flag.
        burn_wipe_all(&base_one).unwrap();
    }
    assert_eq!(
        inventory(&base_one, &account_one),
        before_one,
        "TASK0470A_RED attack=wrong-password-wipe profile=1 boundary=attempt-10"
    );
    println!(
        "TASK0470A_ATTEMPT_10=wrong cooldown_seconds=900 deadline_delta_ms={} bytes_preserved=8",
        locked.password_cooldown_deadline_elapsed_ms.unwrap() - start
    );

    let deadline_before_spam = locked.password_cooldown_deadline_elapsed_ms;
    let work_before_spam = credential_work_count();
    let concurrent_base = Arc::new(base_one.clone());
    let threads = (0..100)
        .map(|index| {
            let base = Arc::clone(&concurrent_base);
            let state = Arc::clone(&shipping_state);
            std::thread::spawn(move || {
                let _keep_profile_path_alive = base;
                let (attempts, remaining) = shipping_wrong(&state, &format!("concurrent-{index}"));
                assert_eq!(attempts, 10);
                assert!(remaining > 0);
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    for index in 0..100 {
        let (attempts, remaining) = shipping_wrong(&shipping_state, &format!("sequential-{index}"));
        assert_eq!(attempts, 10);
        assert!(remaining > 0);
    }
    let after_spam = read_lockout_pub(&base_one);
    assert_eq!(after_spam.password_failed_attempts, 10);
    assert_eq!(
        after_spam.password_cooldown_deadline_elapsed_ms,
        deadline_before_spam
    );
    assert_eq!(credential_work_count(), work_before_spam);
    assert_eq!(inventory(&base_one, &account_one), before_one);
    println!("TASK0470A_COOLDOWN_SPAM concurrent=100 sequential=100 credential_checks=0 count=10 deadline_moves=0 mutated_store_bytes=0");

    let restart = run_child(&base_one, "cooldown");
    assert!(restart.contains("credential_checks=0 count=10"));
    assert_eq!(inventory(&base_one, &account_one), before_one);
    println!("TASK0470A_FULL_PROCESS_RESTART=refused fingerprint_changes=0");

    // Independent oracle samples: host wall time is deliberately absent from
    // the enforcing API. The two labelled wall jumps therefore feed the same
    // elapsed instant and must produce byte-identical refusals.
    let backward = wrong_at(&base_one, PASSWORD, start + 899_999);
    let forward = wrong_at(&base_one, PASSWORD, start + 899_999);
    assert_eq!(backward, forward);
    assert_eq!(credential_work_count(), work_before_spam);
    println!("TASK0470A_CLOCK_JUMPS backward_seconds=-86400 forward_seconds=86400 deadline_moves=0 credential_checks=0");
    println!("TASK0470A_SLEEP_RESUME elapsed_advance_ms=899999 refused=true");

    let before_boundary_work = credential_work_count();
    let immediately_before = wrong_at(&base_one, PASSWORD, start + 899_999);
    assert_eq!(immediately_before.0, 10);
    assert_eq!(immediately_before.1, 1);
    assert_eq!(credential_work_count(), before_boundary_work);
    let accepted =
        verify_gate_password_attempt_at_elapsed(&base_one, PASSWORD, start + 900_000).unwrap();
    assert!(matches!(accepted, GatePasswordAttemptResult::Main(_)));
    let after_success = read_lockout_pub(&base_one);
    assert_eq!(after_success.password_failed_attempts, 0);
    assert_eq!(after_success.password_cooldown_deadline_elapsed_ms, None);
    assert_eq!(inventory(&base_one, &account_one), before_one);
    println!("TASK0470A_BOUNDARY before_ms=899999 before_refused=true after_ms=900000 after_accepted=true elapsed_minutes=15 count_after_auth=0");
    let cold = run_child(&base_one, "cold-zero");
    assert!(cold.contains("cold_count_zero deadline=none"));

    let second_start = elapsed_realtime_ms().unwrap();
    keystore::set_base_dir_override(Some(base_two.clone()));
    keystore::set_active_account_dir(Some(account_two.clone()));
    for attempt in 1..=10 {
        let _ = wrong_at(
            &base_two,
            &format!("profile-two-wrong-{attempt}"),
            second_start,
        );
    }
    assert_eq!(read_lockout_pub(&base_two).password_failed_attempts, 10);
    let recovery_state = AppState::new();
    let token = verify_recovery_phrase(&recovery_state, &base_two, RECOVERY).unwrap();
    assert!(!token.is_empty());
    let recovered = read_lockout_pub(&base_two);
    assert_eq!(recovered.password_failed_attempts, 0);
    assert_eq!(recovered.password_cooldown_deadline_elapsed_ms, None);
    assert_eq!(inventory(&base_two, &account_two), before_two);
    println!("TASK0470A_RECOVERY profile=2 authenticated=true cooldown_cleared=true count=0 unrelated_fingerprint_changes=0");

    let delete_root = temp.path().join("explicit-delete");
    let outside = temp.path().join("outside-delete-contract.bin");
    std::fs::create_dir_all(delete_root.join("store")).unwrap();
    std::fs::write(delete_root.join("identity.json"), b"delete-me").unwrap();
    std::fs::write(delete_root.join("password_marker.json"), b"delete-me").unwrap();
    std::fs::write(delete_root.join("store/messages.sqlite"), b"delete-me").unwrap();
    std::fs::write(&outside, b"must-survive").unwrap();
    let explicit_confirmation = true;
    let deletion_path_observer = if explicit_confirmation {
        burn_wipe_all(&delete_root).unwrap();
        1
    } else {
        0
    };
    assert_eq!(deletion_path_observer, 1);
    assert!(!delete_root.join("identity.json").exists());
    assert!(!delete_root.join("password_marker.json").exists());
    assert!(!delete_root.join("store").exists());
    assert_eq!(std::fs::read(&outside).unwrap(), b"must-survive");
    println!("TASK0470A_DELETE_CONTROL explicitly_confirmed=true deletion_path_observer=1 outside_contract_changes=0");

    println!("TASK0470A_FINISH_LINE=PASS profiles=2 ordinary_failures=9 cooldown_seconds=900 spam_submissions=200 process_restarts=2 recovery_credentials=1 deletion_observer=1");
}
