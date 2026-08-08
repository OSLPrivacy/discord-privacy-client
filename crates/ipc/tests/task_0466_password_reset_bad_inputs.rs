use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ipc::burned_scopes_file::{
    load_burned_scopes, reset_burn_state_unreadable_for_tests, write_burned_scopes,
    BurnedScopeEntry, BurnedScopesFile,
};
use ipc::main_password::{
    set_file_storage_key, set_main_password, set_main_password_after_recovery,
    verify_main_password, verify_recovery_phrase,
};
use ipc::AppState;
use tempfile::TempDir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

const RECORD: &str = "RUBY-0466";
const OLD_PASSWORD: &str = "old-password-0466";
const NEW_PASSWORD: &str = "new-password-0466";

#[derive(Clone)]
struct ResetAttempt {
    phrase: String,
    new_password: String,
    confirmation: String,
    phrase_present: bool,
}

struct GlobalsReset;

impl Drop for GlobalsReset {
    fn drop(&mut self) {
        set_file_storage_key(None);
        reset_burn_state_unreadable_for_tests();
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create fresh account copy");
    for entry in fs::read_dir(source).expect("read source account") {
        let entry = entry.expect("read source entry");
        let target = destination.join(entry.file_name());
        if entry.file_type().expect("read entry type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy account file");
        }
    }
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("read snapshot directory") {
            let entry = entry.expect("read snapshot entry");
            if entry
                .file_type()
                .expect("read snapshot entry type")
                .is_dir()
            {
                collect(root, &entry.path(), files);
            } else {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .expect("snapshot file remains below root")
                    .to_path_buf();
                files.insert(
                    relative,
                    fs::read(entry.path()).expect("read snapshot file"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn seed_record(dir: &Path) -> String {
    let phrase = set_main_password(dir, OLD_PASSWORD).expect("enrol original password");
    write_burned_scopes(
        &dir.join("burned_scopes.json"),
        &BurnedScopesFile {
            version: 1,
            scopes: vec![BurnedScopeEntry {
                scope_kind: "task-0466-record".to_owned(),
                scope_id: RECORD.to_owned(),
                server_id: None,
                channel_id: None,
                burned_at: 1_754_660_000,
                burned_message_ids: Vec::new(),
            }],
        },
    )
    .expect("seed protected RUBY-0466 record");
    phrase
}

fn readable_record_count(dir: &Path) -> usize {
    reset_burn_state_unreadable_for_tests();
    load_burned_scopes(&dir.join("burned_scopes.json"))
        .scopes
        .iter()
        .filter(|entry| entry.scope_id == RECORD)
        .count()
}

fn submit_reset(state: &AppState, dir: &Path, input: &ResetAttempt) -> Result<(), String> {
    // These are the same two local authority checks exercised through the real
    // account-recovery form in task-0466-password-reset-bad-inputs.test.ts.
    // Keeping them here makes each disk copy explicit: refused form input never
    // reaches the native marker replacement operation.
    if !input.phrase_present || input.phrase.trim().is_empty() {
        return Err("phrase required".to_owned());
    }
    if input.new_password != input.confirmation {
        return Err("passwords differ".to_owned());
    }
    let token =
        verify_recovery_phrase(state, dir, &input.phrase).map_err(|_| "phrase wrong".to_owned())?;
    set_main_password_after_recovery(state, dir, &input.new_password, &token)
}

fn assert_bad_copy(
    label: &str,
    dir: &Path,
    input: &ResetAttempt,
    expected_refusal: &str,
    good_dir: &Path,
    good_snapshot: &BTreeMap<PathBuf, Vec<u8>>,
) {
    set_file_storage_key(None);
    let state = AppState::new();
    let refusal = submit_reset(&state, dir, input).expect_err("bad reset copy must be refused");
    assert_eq!(refusal, expected_refusal);

    set_file_storage_key(None);
    verify_main_password(dir, OLD_PASSWORD).expect("original password still opens bad copy");
    let count = readable_record_count(dir);
    assert_eq!(
        count, 1,
        "bad copy must retain exactly one RUBY-0466 record"
    );

    let good_unchanged = snapshot_tree(good_dir) == *good_snapshot;
    assert!(
        good_unchanged,
        "bad-copy attempt changed the independent good copy"
    );
    println!(
        "TASK0466 bad_copy={label} refusal={refusal} old_password=reads record={RECORD} count={count} good_copy_unchanged={good_unchanged}"
    );
}

#[test]
fn task_0466_reset_refuses_bad_phrase_confirmation_and_phrase_presence() {
    let _serial = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _reset = GlobalsReset;
    let root = TempDir::new().expect("TASK0466 disposable root");
    let base = root.path().join("keystore-base");
    keystore::set_base_dir_override(Some(base));
    keystore::set_active_account_dir(None);
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();

    let seed = root.path().join("seed");
    fs::create_dir_all(&seed).expect("create seed account");
    let phrase = seed_record(&seed);
    let before_count = readable_record_count(&seed);
    assert_eq!(before_count, 1);
    println!("TASK0466 before record={RECORD} readable=true count={before_count}");

    let good_dir = root.path().join("good");
    let bad_phrase_dir = root.path().join("bad-phrase");
    let bad_confirmation_dir = root.path().join("bad-confirmation");
    let bad_presence_dir = root.path().join("bad-phrase-presence");
    for destination in [
        &good_dir,
        &bad_phrase_dir,
        &bad_confirmation_dir,
        &bad_presence_dir,
    ] {
        copy_tree(&seed, destination);
    }

    let good = ResetAttempt {
        phrase: phrase.clone(),
        new_password: NEW_PASSWORD.to_owned(),
        confirmation: NEW_PASSWORD.to_owned(),
        phrase_present: true,
    };

    set_file_storage_key(None);
    submit_reset(&AppState::new(), &good_dir, &good).expect("good reset succeeds");
    set_file_storage_key(None);
    verify_main_password(&good_dir, NEW_PASSWORD).expect("new password opens good copy");
    let good_count = readable_record_count(&good_dir);
    assert_eq!(good_count, 1);
    println!(
        "TASK0466 good_reset=result=password changed matching_new_password=true record={RECORD} readable=true count={good_count}"
    );
    let good_snapshot = snapshot_tree(&good_dir);

    let mut bad_phrase = good.clone();
    let mut words: Vec<&str> = bad_phrase.phrase.split_whitespace().collect();
    words[0] = if words[0] == "abandon" {
        "ability"
    } else {
        "abandon"
    };
    bad_phrase.phrase = words.join(" ");
    assert_bad_copy(
        "phrase",
        &bad_phrase_dir,
        &bad_phrase,
        "phrase wrong",
        &good_dir,
        &good_snapshot,
    );

    let mut bad_confirmation = good.clone();
    bad_confirmation.confirmation = "different-password-0466".to_owned();
    assert_bad_copy(
        "confirmation",
        &bad_confirmation_dir,
        &bad_confirmation,
        "passwords differ",
        &good_dir,
        &good_snapshot,
    );

    let mut bad_presence = good;
    bad_presence.phrase_present = false;
    assert_bad_copy(
        "phrase-presence",
        &bad_presence_dir,
        &bad_presence,
        "phrase required",
        &good_dir,
        &good_snapshot,
    );

    set_file_storage_key(None);
    verify_main_password(&good_dir, NEW_PASSWORD).expect("good copy still uses new password");
    let final_good_count = readable_record_count(&good_dir);
    assert_eq!(final_good_count, 1);
    assert_eq!(snapshot_tree(&good_dir), good_snapshot);
    println!(
        "TASK0466 final good_password=new record={RECORD} readable=true count={final_good_count} good_copy_unchanged=true"
    );
}
