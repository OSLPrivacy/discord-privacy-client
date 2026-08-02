//! Regression gate for main-password rotation coverage.
//!
//! This deliberately validates the directory produced by the real state
//! writers.  It does not inspect `AT_REST_STATE_FILES`: omitting a writer's
//! filename from that sweep leaves its real encrypted file under the old key,
//! and this test then cannot decrypt it after a password change.

use std::fs;
use std::sync::Mutex;

use ipc::app_preferences::{write_app_preferences, AppPreferences};
use ipc::burned_scopes_file::{write_burned_scopes, BurnedScopesFile};
use ipc::control_inbox_dead_letter::{write_control_inbox_dead_letter, ControlInboxDeadLetterFile};
use ipc::friend_request::{save_friend_request_file_state, FriendRequestFileState};
use ipc::main_password::{
    change_main_password, has_enc_magic, maybe_decrypt_file, maybe_encrypt, set_file_storage_key,
    set_main_password, verify_main_password,
};
use ipc::membership::{write_scope_membership, ScopeMembership};
use ipc::peer_map::{write_peer_map, PeerMap};
use ipc::scope_blobs_file::{self, ScopeBlobsFile};
use ipc::scope_ttl_file::{write_scope_ttls, ScopeTtlFile};
use ipc::sender_key_state::{write_sender_key_state, SenderKeyStateFile};
use ipc::whitelist_state::{write_whitelist_state_file, WhitelistStateFile};
use tempfile::tempdir;

// The file-storage key is process global.
static KEY_LOCK: Mutex<()> = Mutex::new(());

/// Writers outside this crate, plus the private pending-request writer, share
/// the same on-disk encryption contract but cannot be invoked from this IPC
/// integration test.  These fixtures are real encrypted files in their real
/// filenames, so the rotation is still required to re-key them.
const EXTERNAL_WRITER_FILES: &[&str] = &[
    "pending_friend_requests.json",
    "hub_people.json",
    "hub_security_preferences.json",
    "hub_peer_replay.json",
    "scope_attachments.json",
    "hub_revocation_ledger.json",
    "hub_revocation_outbox.json",
    "hub_revocation_counters.json",
];

fn write_external_writer_fixtures(dir: &std::path::Path) {
    for name in EXTERNAL_WRITER_FILES {
        let sealed = maybe_encrypt(format!(r#"{{"writer":"{name}"}}"#).as_bytes())
            .expect("seal external writer fixture");
        fs::write(dir.join(name), sealed).expect("write external writer fixture");
    }
}

#[test]
fn changing_main_password_rekeys_every_at_rest_file_present_in_the_directory() {
    let _serial = KEY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    set_file_storage_key(None);
    let dir = tempdir().expect("tempdir");

    set_main_password(dir.path(), "password-one").expect("enrol main password");

    write_peer_map(&dir.path().join("peer_map.json"), &PeerMap::default()).expect("write peer map");
    write_whitelist_state_file(
        &dir.path().join("whitelist_state.json"),
        &WhitelistStateFile::default(),
    )
    .expect("write whitelist state");
    write_burned_scopes(
        &dir.path().join("burned_scopes.json"),
        &BurnedScopesFile::default(),
    )
    .expect("write burned scopes");
    write_app_preferences(
        &dir.path().join("app_preferences.json"),
        &AppPreferences::default(),
    )
    .expect("write app preferences");
    write_sender_key_state(
        &dir.path().join("sender_key_state.json"),
        &SenderKeyStateFile::default(),
    )
    .expect("write sender key state");
    write_scope_membership(
        &dir.path().join("membership.json"),
        &ScopeMembership::default(),
    )
    .expect("write membership");
    write_scope_ttls(&dir.path().join("scope_ttl.json"), &ScopeTtlFile::default())
        .expect("write scope TTLs");
    scope_blobs_file::write(
        &dir.path().join("scope_blobs.json"),
        &ScopeBlobsFile::default(),
    )
    .expect("write scope blobs");
    write_control_inbox_dead_letter(
        &dir.path().join("control_inbox_dead_letter.json"),
        &ControlInboxDeadLetterFile::default(),
    )
    .expect("write control-inbox dead letter");
    save_friend_request_file_state(dir.path(), &FriendRequestFileState::default())
        .expect("write friend-request state");
    write_external_writer_fixtures(dir.path());

    change_main_password(dir.path(), "password-one", "password-two")
        .expect("rotate every at-rest file onto the new password key");

    // A cold process must be able to open every encrypted state file that the
    // writers left in this directory.  Enumerating disk, rather than the sweep
    // constant, makes any omitted name fail here.
    set_file_storage_key(None);
    verify_main_password(dir.path(), "password-two").expect("unlock with new password");
    for entry in fs::read_dir(dir.path()).expect("enumerate state directory") {
        let path = entry.expect("directory entry").path();
        let bytes = fs::read(&path).expect("read state file");
        if has_enc_magic(&bytes) {
            maybe_decrypt_file(&path, &bytes).unwrap_or_else(|error| {
                panic!(
                    "password rotation orphaned encrypted state file {}: {error}",
                    path.display()
                )
            });
        }
    }

    set_file_storage_key(None);
}
