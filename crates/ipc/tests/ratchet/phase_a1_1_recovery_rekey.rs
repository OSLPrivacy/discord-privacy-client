//! A1-1 regression: account recovery must not orphan (and therefore must not
//! silently empty) the at-rest state it re-keys.
//!
//! `set_main_password_after_recovery` used to write a fresh-salt marker and
//! stop. Every at-rest file stayed sealed under the OLD password-derived key,
//! which recovery has by definition destroyed — and because the burn kill-list
//! loader failed open, "cannot decrypt" read back as "nothing was ever burned".
//! Recovering your account un-burned every burned message.
//!
//! These tests exercise the real functions against real files on disk and
//! assert on the real bytes, not on source text.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use ipc::burned_scopes_file::{
    burn_state_unreadable, load_burned_scopes, reset_burn_state_unreadable_for_tests,
    write_burned_scopes, BurnedScopeEntry, BurnedScopesFile,
};
use ipc::main_password::{
    has_enc_magic, set_file_storage_key, set_main_password, set_main_password_after_recovery,
    verify_main_password, verify_recovery_phrase,
};
use ipc::state::AppState;
use tempfile::TempDir;

/// `file_storage_key` and the keystore base dir are process globals shared by
/// every test in this binary.
static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

const BURNED_SCOPE_ID: &str = "900000000000000003";
const BURNED_MESSAGE_ID: &str = "1420000000000000001";

fn burn_entry() -> BurnedScopeEntry {
    BurnedScopeEntry {
        scope_kind: "dm".to_string(),
        scope_id: BURNED_SCOPE_ID.to_string(),
        server_id: None,
        channel_id: None,
        burned_at: 1_753_000_000,
        burned_message_ids: vec![BURNED_MESSAGE_ID.to_string()],
    }
}

/// Enrol a password-protected account holding one burned scope, sealed at rest.
/// Returns the recovery phrase.
fn enrol_with_a_burned_scope(dir: &Path) -> String {
    let phrase = set_main_password(dir, "password-one").expect("set main password");
    let bs_path = dir.join("burned_scopes.json");
    write_burned_scopes(
        &bs_path,
        &BurnedScopesFile {
            version: 1,
            scopes: vec![burn_entry()],
        },
    )
    .expect("seed burn kill list");
    assert!(
        has_enc_magic(&fs::read(&bs_path).expect("read seeded kill list")),
        "fixture must be sealed at rest before recovery runs"
    );
    phrase
}

fn assert_kill_list_intact(bs_path: &Path) {
    let loaded = load_burned_scopes(bs_path);
    assert!(
        !burn_state_unreadable(),
        "burn kill list was unreadable after recovery — the burn ledger is gone"
    );
    assert_eq!(
        loaded.scopes.len(),
        1,
        "recovery must preserve the burn kill list, got {:?}",
        loaded.scopes
    );
    assert_eq!(loaded.scopes[0].scope_id, BURNED_SCOPE_ID);
    assert_eq!(
        loaded.scopes[0].burned_message_ids,
        vec![BURNED_MESSAGE_ID.to_string()]
    );
}

#[test]
fn recovery_rekeys_at_rest_state_and_keeps_burned_scopes_burned() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let base = TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();

    let dir = TempDir::new().unwrap();
    let phrase = enrol_with_a_burned_scope(dir.path());
    let bs_path = dir.path().join("burned_scopes.json");

    let state = AppState::new();
    let token = verify_recovery_phrase(&state, dir.path(), &phrase).expect("phrase verifies");
    set_main_password_after_recovery(&state, dir.path(), "password-two", &token)
        .expect("recovery completes");

    // The file must still be sealed (recovery never writes cleartext) ...
    assert!(
        has_enc_magic(&fs::read(&bs_path).expect("read kill list after recovery")),
        "recovery must leave the kill list encrypted at rest"
    );

    // ... and must open under the NEW password from a cold process.
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();
    verify_main_password(dir.path(), "password-two").expect("new password unlocks");
    assert_kill_list_intact(&bs_path);

    // The old password must be dead, and its key must not open the file.
    assert!(verify_main_password(dir.path(), "password-one").is_err());

    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();
    keystore::set_base_dir_override(None);
}

#[test]
fn recovery_refuses_loudly_when_the_marker_predates_the_phrase_wrap() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let base = TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();

    let dir = TempDir::new().unwrap();
    let phrase = enrol_with_a_burned_scope(dir.path());
    let bs_path = dir.path().join("burned_scopes.json");

    // Downgrade the marker to the shape written by a build that predates the
    // phrase-wrapped file key: an already-enrolled user upgrading into this fix.
    let marker_path = dir.path().join("password_marker.json");
    let mut marker: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker_path).unwrap()).unwrap();
    let obj = marker.as_object_mut().unwrap();
    obj.remove("file_key_phrase_wrapped_b64");
    obj.remove("file_key_phrase_nonce_b64");
    let legacy_marker_bytes = serde_json::to_vec_pretty(&marker).unwrap();
    fs::write(&marker_path, &legacy_marker_bytes).unwrap();

    let state = AppState::new();
    let token = verify_recovery_phrase(&state, dir.path(), &phrase).expect("phrase verifies");
    let err = set_main_password_after_recovery(&state, dir.path(), "password-two", &token)
        .expect_err("recovery must refuse rather than orphan the burn ledger");
    assert!(
        err.contains("cannot complete recovery"),
        "refusal must say what happened, got: {err}"
    );

    // Refusing means refusing: nothing on disk moved, and the account still
    // opens with the password it had.
    assert_eq!(
        fs::read(&marker_path).unwrap(),
        legacy_marker_bytes,
        "a refused recovery must not swap the marker"
    );
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();
    verify_main_password(dir.path(), "password-one").expect("old password still unlocks");
    assert_kill_list_intact(&bs_path);

    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();
    keystore::set_base_dir_override(None);
}

#[test]
fn an_unreadable_kill_list_fails_closed_instead_of_reading_as_empty() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let base = TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();

    let dir = TempDir::new().unwrap();
    enrol_with_a_burned_scope(dir.path());
    let bs_path = dir.path().join("burned_scopes.json");
    let sealed = fs::read(&bs_path).unwrap();

    // Same shape as an orphaned file: correct envelope, wrong key.
    set_file_storage_key(Some([0x5a; 32]));
    let loaded = load_burned_scopes(&bs_path);
    assert!(
        loaded.scopes.is_empty(),
        "an undecryptable file yields no rows — the point is that it must not be BELIEVED"
    );
    assert!(
        burn_state_unreadable(),
        "an undecryptable burn kill list must latch fail-closed, not read as 'nothing burned'"
    );

    // And that latch must stop the empty list from being persisted over the
    // real one — that write is what made the loss permanent.
    let err = write_burned_scopes(&bs_path, &BurnedScopesFile::default())
        .expect_err("writing over an unreadable kill list must be refused");
    assert!(err.contains("refusing to write"), "got: {err}");
    assert_eq!(
        fs::read(&bs_path).unwrap(),
        sealed,
        "the on-disk kill list must survive untouched"
    );

    set_file_storage_key(None);
    reset_burn_state_unreadable_for_tests();
    keystore::set_base_dir_override(None);
}
