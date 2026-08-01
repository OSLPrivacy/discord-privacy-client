//! Regression: a damaged Space roster must never be mistaken for the last
//! usable membership snapshot.
//!
//! Sending is permitted only after the caller obtains a roster from
//! `load_space_roster`.  Therefore an unreadable on-disk roster must return
//! an error, even when that same process loaded a valid roster earlier.

use std::sync::Mutex;

use ipc::main_password::set_file_storage_key;
use ipc::space_roster::{
    load_space_roster, write_space_roster, SpaceRosterFileError, SPACE_ROSTER_FILE,
};
use tempfile::tempdir;

// The at-rest key is process-global.  Keep this regression independent of
// whatever order the Rust test runner chooses for the tests in this binary.
static FILE_STORAGE_KEY: Mutex<()> = Mutex::new(());

#[test]
fn unreadable_roster_never_returns_the_last_known_membership() {
    let _serial = FILE_STORAGE_KEY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = tempdir().expect("create roster fixture directory");
    let path = directory.path().join(SPACE_ROSTER_FILE);
    let prior_roster = br#"{"version":1,"spaces":[{"members":["still-member"]}]}"#;

    set_file_storage_key(Some([0x3c; 32]));
    write_space_roster(&path, prior_roster).expect("seed encrypted roster");
    assert_eq!(
        load_space_roster(&path).expect("seed roster is readable"),
        prior_roster,
        "the fixture must prove a prior roster was available before corruption"
    );

    // This models a torn or tampered-at-rest roster after the process has
    // already had a valid membership snapshot.  Returning that snapshot here
    // would send to a member who may have been removed.
    std::fs::write(&path, b"not an OSL encrypted roster").expect("damage roster on disk");
    let refusal = load_space_roster(&path).expect_err(
        "an unreadable roster must refuse the send path instead of using cached membership",
    );
    assert!(
        matches!(refusal, SpaceRosterFileError::DecryptFailed { .. }),
        "corrupt roster must be an explicit refusal, got {refusal:?}"
    );

    set_file_storage_key(None);
}
