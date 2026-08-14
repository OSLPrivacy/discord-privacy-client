//! TASK 0823 - break Home protection summary.
//!
//! Gate 0822 proved the Home summary is one-fact-accurate. This task feeds a
//! **missing trusted-person count** into a *test copy* of a saved account and
//! reads the summary through `cmd_osl_home_protection_summary`.
//!
//! The finish line is that the summary check **exits 1 rather than displaying a
//! false count**. "False count" is not hypothetical here: the account really
//! has two trusted people, and the only other thing the check could do with a
//! `peer_map.json` it cannot load is default to an empty map and print
//! "0 trusted people" - a count that is wrong by two, plus a next safe step
//! ("Add a trusted person") derived from the wrong count.
//!
//! Exit codes have to be observed from outside a test harness (a panicking test
//! exits 101, not 1), so the check itself is a re-exec child - the ignored test
//! `task_0823_summary_check_child`. It loads an account directory off disk
//! exactly as a fresh boot would, and either prints every summary fact and exits
//! 0, or refuses on stderr and exits 1 without printing any count.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::allowed_places::AllowedPlaceRecord;
use ipc::commands::{
    cmd_osl_home_protection_summary, cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule,
    cmd_osl_save_privacy_level_rule_set, cmd_osl_save_verification_warning_choice,
};
use ipc::main_password::set_file_storage_key;
use ipc::peer_map::{PeerEntry, PeerMap};
use ipc::state::{AppState, CloudRegistrationState};
use ipc::tofu::KeyBundle;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

/// The saved account carries exactly this many trusted people. Any count the
/// check displays for a copy whose `peer_map.json` cannot be loaded that is not
/// this number is a false count.
const TRUE_TRUSTED_PEOPLE: usize = 2;
const STORAGE_KEY: [u8; 32] = [0x82; 32];
const CHILD_TEST_NAME: &str = "task_0823_summary_check_child";
const ACCOUNT_DIR_ENV: &str = "OSL_TASK_0823_ACCOUNT_DIR";

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        set_file_storage_key(None);
    }
}

fn identity_bundle(identity: &keystore::Identity) -> KeyBundle {
    KeyBundle {
        ed25519_pub: STANDARD.encode(identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
        ratchet_initial_pub: identity
            .ratchet_initial_pub
            .as_ref()
            .map(|p| STANDARD.encode(p.as_bytes())),
    }
}

fn trusted_peer(discord_id: &str, label: &str) -> (String, PeerEntry) {
    let identity = keystore::generate_identity(label.to_string());
    (
        discord_id.to_string(),
        PeerEntry {
            osl_user_id: Some(label.to_string()),
            discord_id: Some(discord_id.to_string()),
            tofu_key_bundle: Some(identity_bundle(&identity)),
            ..PeerEntry::default()
        },
    )
}

fn protected_state(label: &str) -> AppState {
    let state = AppState::new();
    state.install_identity(keystore::generate_identity(label.to_string()));
    *state.keyserver.lock().unwrap() =
        Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
    state.set_cloud_registration_state(CloudRegistrationState::Registered);
    state
}

/// Save the same account shape task 0820 proved: protected, privacy level
/// `maximum`, verification warning `before sending`, two trusted people, two
/// connected apps.
fn save_protected_account(dir: &Path) {
    let state = protected_state("task-0823-owner");

    cmd_osl_save_privacy_level_rule_set(&state, "maximum".to_string(), Some(dir.to_path_buf()))
        .expect("save privacy level");
    cmd_osl_save_verification_warning_choice(
        &state,
        "before sending".to_string(),
        Some(dir.to_path_buf()),
    )
    .expect("save verification warning");

    let mut peer_map: PeerMap = HashMap::new();
    let (rose_id, rose) = trusted_peer("900000000000082301", "rose-task-0823");
    let (sam_id, sam) = trusted_peer("900000000000082302", "sam-task-0823");
    peer_map.insert(rose_id.clone(), rose);
    peer_map.insert(sam_id.clone(), sam);
    assert_eq!(
        peer_map.len(),
        TRUE_TRUSTED_PEOPLE,
        "the saved account must carry the count the finish line calls true"
    );
    ipc::peer_map::write_peer_map(&dir.join("peer_map.json"), &peer_map)
        .expect("persist trusted people");

    cmd_osl_save_auto_whitelist_rule(
        &state,
        "discord".to_string(),
        "always".to_string(),
        Some(dir.to_path_buf()),
    )
    .expect("save discord app rule");
    cmd_osl_new_place(
        &state,
        AllowedPlaceRecord::discord_direct_message("task-0823-owner", rose_id),
        Some(dir.to_path_buf()),
    )
    .expect("persist discord app place");
    cmd_osl_save_auto_whitelist_rule(
        &state,
        "telegram".to_string(),
        "always".to_string(),
        Some(dir.to_path_buf()),
    )
    .expect("save telegram app rule");
    cmd_osl_new_place(
        &state,
        AllowedPlaceRecord {
            app: "telegram".to_string(),
            account: "task-0823-owner".to_string(),
            kind: "direct_message".to_string(),
            stable_id: format!("telegram:task-0823-owner:direct_message:{sam_id}"),
            place_name: sam_id.clone(),
            person_name: sam_id.clone(),
        },
        Some(dir.to_path_buf()),
    )
    .expect("persist telegram app place");
}

fn copy_dir_recursive(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create copy directory");
    for entry in std::fs::read_dir(from).expect("read account directory") {
        let entry = entry.expect("account directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry file type").is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy account file");
        }
    }
}

/// Run the summary check over `account_dir` as its own process so the exit code
/// is a real exit code.
fn run_summary_check(account_dir: &Path) -> Output {
    let exe = std::env::current_exe().expect("path of this test binary");
    Command::new(exe)
        .args([
            "--exact",
            CHILD_TEST_NAME,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ACCOUNT_DIR_ENV, account_dir)
        .output()
        .expect("run the summary check")
}

fn report(case: &str, output: &Output) -> (String, String) {
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    println!("TASK0823_{case}_EXIT={:?}", output.status.code());
    println!("TASK0823_{case}_STDOUT={stdout}");
    println!("TASK0823_{case}_STDERR={stderr}");
    (stdout, stderr)
}

/// The check itself. Not a test of this task's claim - it is the thing under
/// test, re-exec'd by the tests below. Ignored so a plain run never picks it up.
#[test]
#[ignore = "re-exec child: the summary check under test"]
fn task_0823_summary_check_child() {
    let dir = std::path::PathBuf::from(
        std::env::var(ACCOUNT_DIR_ENV).expect("account directory to read the summary from"),
    );
    set_file_storage_key(Some(STORAGE_KEY));
    let _key_guard = FileStorageKeyGuard;

    let state = protected_state("task-0823-owner");
    *state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.join("app_preferences.json"));

    // The one fact this check refuses to guess at. `load_peer_map_from_path`
    // is the product's own loader; on a missing file it errors `NotFound`
    // (crates/ipc/src/peer_map.rs). Defaulting to an empty map here is exactly
    // the false count the task forbids, so the check stops instead - before any
    // count has been printed.
    let peer_map_path = dir.join("peer_map.json");
    match ipc::peer_map::load_peer_map_from_path(&peer_map_path) {
        Ok(peer_map) => *state.peer_map.lock().unwrap() = peer_map,
        Err(error) => {
            eprintln!("TASK0823_FAULT=trusted_person_count_missing {error}");
            std::process::exit(1);
        }
    }

    let summary = match cmd_osl_home_protection_summary(&state, Some(dir.clone())) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("TASK0823_FAULT=summary_unavailable {error}");
            std::process::exit(1);
        }
    };

    println!(
        "TASK0823 home.protection_state={}",
        summary.protection_state
    );
    println!("TASK0823 home.privacy_level={}", summary.privacy_level);
    println!(
        "TASK0823 home.verification_warning={}",
        summary.verification_warning
    );
    println!(
        "TASK0823 home.trusted_people_count={}",
        summary.trusted_people_count
    );
    println!("TASK0823 home.trusted_people={}", summary.trusted_people);
    println!(
        "TASK0823 home.connected_app_count={}",
        summary.connected_app_count
    );
    println!("TASK0823 home.apps={}", summary.apps);
    println!("TASK0823 home.next_safe_step={}", summary.next_safe_step);
    std::process::exit(0);
}

/// The finish line: a test copy whose trusted-person count is missing makes the
/// summary check exit 1, and no count of any kind reaches the display.
#[test]
fn task_0823_missing_trusted_person_count_exits_1_not_a_false_count() {
    set_file_storage_key(Some(STORAGE_KEY));
    let _key_guard = FileStorageKeyGuard;

    let root = tempfile::tempdir().expect("tempdir");
    let account = root.path().join("account");
    std::fs::create_dir_all(&account).expect("create account dir");
    save_protected_account(&account);

    // What the truth is, read from the untouched saved account by the very same
    // check. Without this the "false count" half of the finish line would be an
    // assertion about nothing.
    let truth = run_summary_check(&account);
    let (truth_stdout, _) = report("TRUTH", &truth);
    assert_eq!(
        truth.status.code(),
        Some(0),
        "the untouched saved account must read cleanly"
    );
    assert!(
        truth_stdout.contains(&format!("home.trusted_people_count={TRUE_TRUSTED_PEOPLE}")),
        "saved account should report {TRUE_TRUSTED_PEOPLE} trusted people, got:\n{truth_stdout}"
    );

    // The test copy, with the trusted-person count made missing in the copy
    // only. The saved account above still has its two trusted people.
    let copy = root.path().join("account-test-copy");
    copy_dir_recursive(&account, &copy);
    let copied_peer_map = copy.join("peer_map.json");
    assert!(copied_peer_map.exists(), "the copy should start complete");
    std::fs::remove_file(&copied_peer_map).expect("make the trusted-person count missing");
    assert!(
        !copied_peer_map.exists(),
        "the copy's trusted-person count must be unknowable"
    );
    assert!(
        account.join("peer_map.json").exists(),
        "the original saved account must be untouched"
    );

    let output = run_summary_check(&copy);
    let (stdout, stderr) = report("MISSING_COUNT", &output);

    assert_eq!(
        output.status.code(),
        Some(1),
        "the summary check must exit 1 on a missing trusted-person count"
    );
    for forbidden in [
        "trusted_people_count=",
        "trusted people",
        "trusted person",
        "next_safe_step=",
    ] {
        assert!(
            !stdout.contains(forbidden),
            "the check displayed {forbidden:?} for a count it could not know:\n{stdout}"
        );
    }
    assert!(
        stderr.contains("TASK0823_FAULT=trusted_person_count_missing"),
        "the check must say which fact is missing, got:\n{stderr}"
    );
    assert!(
        stderr.contains("peer_map.json not found"),
        "the refusal should name the missing saved file, got:\n{stderr}"
    );
}

/// The check is not a check that always fails: an intact test copy of the same
/// account reads the true count and exits 0.
#[test]
fn task_0823_full_test_copy_exits_0_with_the_true_count() {
    set_file_storage_key(Some(STORAGE_KEY));
    let _key_guard = FileStorageKeyGuard;

    let root = tempfile::tempdir().expect("tempdir");
    let account = root.path().join("account");
    std::fs::create_dir_all(&account).expect("create account dir");
    save_protected_account(&account);

    let copy = root.path().join("account-test-copy");
    copy_dir_recursive(&account, &copy);

    let output = run_summary_check(&copy);
    let (stdout, _) = report("FULL_COPY", &output);

    assert_eq!(
        output.status.code(),
        Some(0),
        "a complete test copy must read cleanly"
    );
    assert!(
        stdout.contains(&format!("home.trusted_people_count={TRUE_TRUSTED_PEOPLE}")),
        "expected the true count {TRUE_TRUSTED_PEOPLE}:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "home.trusted_people={TRUE_TRUSTED_PEOPLE} trusted people"
        )),
        "expected the true trusted-people sentence:\n{stdout}"
    );
    assert!(
        stdout.contains("home.next_safe_step=Open a protected conversation"),
        "expected the next step of a fully set-up account:\n{stdout}"
    );
}
