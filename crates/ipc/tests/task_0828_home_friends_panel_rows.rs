//! TASK 0828 - write OSL Friends panel data.
//!
//! Finish line: a direct read returns exactly the 3 fixture friend rows and 0
//! non-friend pictures, and the non-friend fixture's id appears 0 times in the
//! response.
//!
//! The fixture deliberately stacks the deck against the reader:
//!   * one accepted friend who permitted a picture,
//!   * one accepted friend with no picture at all,
//!   * one accepted friend whose picture is saved but who is NOT on the
//!     permitted list, so the row must fall back to its initial,
//!   * one non-friend (a pending request) who has a picture forced straight
//!     into the saved file AND who is on the permitted list - the row must not
//!     exist and neither their id nor their picture may appear anywhere.

use ipc::commands::{
    cmd_osl_accept_saved_friend_request, cmd_osl_create_friend_request,
    cmd_osl_read_home_friend_rows, cmd_osl_save_home_friend_picture, cmd_osl_set_friend_ids,
};
use ipc::friend_request::{load_friend_request_file_state, save_friend_request_file_state};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const LOCAL_ID: &str = "AMBER-0828";
const FRIEND_WITH_PICTURE: &str = "900000000000082801";
const FRIEND_WITHOUT_PICTURE: &str = "900000000000082802";
const FRIEND_PICTURE_NOT_PERMITTED: &str = "900000000000082803";
const NON_FRIEND: &str = "900000000000082804";

const PERMITTED_PICTURE: &str =
    "data:image/gif;base64,R0lGODlhAQABAIAAAP8AACwAAAAAAQABAAACAkQBADs=";
const WITHHELD_PICTURE: &str = "data:image/gif;base64,R0lGODlhAQABAIAAAAD/ACwAAAAAAQABAAACAkQBADs=";
const NON_FRIEND_PICTURE: &str =
    "data:image/gif;base64,R0lGODlhAQABAIAAAAAA/ywAAAAAAQABAAACAkQBADs=";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ConfigDirGuard
}

fn create_request(state: &AppState, label: &str, peer_id: &str, username: &str) -> String {
    let request_id = format!("REQ-0828-{label}");
    let scope = Scope::dm(peer_id);
    cmd_osl_create_friend_request(
        state,
        request_id.clone(),
        LOCAL_ID.to_string(),
        peer_id.to_string(),
        username.to_string(),
        (&scope).into(),
    )
    .expect("create seeded friend request");
    request_id
}

/// The saved friend identifier the panel routes a row by.
fn saved_friend_id(dir: &Path, remote_identity_id: &str) -> String {
    load_friend_request_file_state(dir)
        .expect("read saved friend file")
        .friends
        .iter()
        .find(|friend| friend.remote_identity_id == remote_identity_id)
        .expect("saved friend record")
        .record_id
        .clone()
}

/// Force a picture onto a saved record without going through the command, so
/// the reader is proved to withhold a picture that really is on disk.
fn force_saved_picture(dir: &Path, remote_identity_id: &str, picture: &str) {
    let mut file = load_friend_request_file_state(dir).expect("read saved friend file");
    let record = file
        .friends
        .iter_mut()
        .find(|friend| friend.remote_identity_id == remote_identity_id)
        .expect("saved friend record");
    record.picture = Some(picture.to_string());
    save_friend_request_file_state(dir, &file).expect("write saved friend file");
}

#[test]
fn direct_read_returns_exactly_the_three_fixture_friend_rows_and_no_non_friend_pictures() {
    let _lock = CONFIG_DIR_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().expect("tempdir");
    let _config_dir = use_temp_config_dir(dir.path());
    let state = AppState::new();

    let with_picture = create_request(&state, "with-picture", FRIEND_WITH_PICTURE, "Ada Friend");
    let without_picture = create_request(
        &state,
        "without-picture",
        FRIEND_WITHOUT_PICTURE,
        "Bo Friend",
    );
    let not_permitted = create_request(
        &state,
        "not-permitted",
        FRIEND_PICTURE_NOT_PERMITTED,
        "Cleo Friend",
    );
    // Stays pending: a non-friend.
    create_request(&state, "non-friend", NON_FRIEND, "Dex Stranger");

    cmd_osl_accept_saved_friend_request(&state, with_picture).expect("accept friend 1");
    cmd_osl_accept_saved_friend_request(&state, without_picture).expect("accept friend 2");
    cmd_osl_accept_saved_friend_request(&state, not_permitted).expect("accept friend 3");

    // The permitted list carries the non-friend too: being permitted is not
    // enough on its own, the person must be an accepted friend.
    cmd_osl_set_friend_ids(
        &state,
        vec![
            FRIEND_WITH_PICTURE.to_string(),
            FRIEND_WITHOUT_PICTURE.to_string(),
            NON_FRIEND.to_string(),
        ],
    )
    .expect("set permitted picture list");

    cmd_osl_save_home_friend_picture(
        &state,
        LOCAL_ID.to_string(),
        FRIEND_WITH_PICTURE.to_string(),
        Some(PERMITTED_PICTURE.to_string()),
    )
    .expect("save permitted friend picture");
    cmd_osl_save_home_friend_picture(
        &state,
        LOCAL_ID.to_string(),
        FRIEND_PICTURE_NOT_PERMITTED.to_string(),
        Some(WITHHELD_PICTURE.to_string()),
    )
    .expect("save unpermitted friend picture");

    // A picture may not be saved for a person who is not an accepted friend.
    let refusal = cmd_osl_save_home_friend_picture(
        &state,
        LOCAL_ID.to_string(),
        NON_FRIEND.to_string(),
        Some(NON_FRIEND_PICTURE.to_string()),
    )
    .expect_err("non-friend picture must be refused");
    assert_eq!(refusal, "OSL: friend picture is not permitted");
    // ...so put one on disk by hand anyway and prove the reader drops it.
    force_saved_picture(dir.path(), NON_FRIEND, NON_FRIEND_PICTURE);

    let panel = cmd_osl_read_home_friend_rows(&state).expect("read Home friend rows");
    let response = serde_json::to_string(&panel).expect("serialize Home friend rows");

    let expected_ids = [
        saved_friend_id(dir.path(), FRIEND_WITH_PICTURE),
        saved_friend_id(dir.path(), FRIEND_WITHOUT_PICTURE),
        saved_friend_id(dir.path(), FRIEND_PICTURE_NOT_PERMITTED),
    ];
    let non_friend_saved_id = saved_friend_id(dir.path(), NON_FRIEND);

    // 0 non-friend pictures, in the rows and in the response text.
    let non_friend_rows = panel
        .rows
        .iter()
        .filter(|row| row.osl_user_id == NON_FRIEND || row.friend_id == non_friend_saved_id)
        .count();
    let non_friend_picture_count = response.matches(NON_FRIEND_PICTURE).count()
        + panel
            .rows
            .iter()
            .filter(|row| row.picture.as_deref() == Some(NON_FRIEND_PICTURE))
            .count();
    let withheld_picture_count = response.matches(WITHHELD_PICTURE).count();
    let non_friend_id_count = response.matches(NON_FRIEND).count();
    let non_friend_saved_id_count = response.matches(non_friend_saved_id.as_str()).count();

    // The non-friend's picture really is on disk; the reader is what withholds it.
    let saved_non_friend_picture = load_friend_request_file_state(dir.path())
        .expect("read saved friend file")
        .friends
        .iter()
        .find(|friend| friend.remote_identity_id == NON_FRIEND)
        .and_then(|friend| friend.picture.clone());

    // Every counter is printed before it is asserted, so a broken reader shows
    // its real numbers rather than only the first assertion it trips.
    println!(
        "TASK_0828 friend_row_count={} picture_count={} non_friend_rows={} non_friend_picture_count={} withheld_picture_count={} non_friend_id_count={} non_friend_saved_id_count={}",
        panel.friend_count,
        panel.picture_count,
        non_friend_rows,
        non_friend_picture_count,
        withheld_picture_count,
        non_friend_id_count,
        non_friend_saved_id_count
    );
    for row in &panel.rows {
        println!(
            "TASK_0828 row friend_id={} osl_user_id={} username={} picture_status={} initial={} initial_colour={} picture_bytes={}",
            row.friend_id,
            row.osl_user_id,
            row.username,
            row.picture_status,
            row.initial,
            row.initial_colour,
            row.picture.as_deref().map(str::len).unwrap_or(0)
        );
    }
    println!(
        "TASK_0828 non_friend_picture_on_disk_bytes={} save_refusal={}",
        saved_non_friend_picture
            .as_deref()
            .map(str::len)
            .unwrap_or(0),
        refusal
    );

    // Exactly the 3 fixture friend rows.
    assert_eq!(panel.friend_count, 3);
    assert_eq!(panel.rows.len(), 3);
    assert_eq!(
        panel
            .rows
            .iter()
            .map(|row| row.username.as_str())
            .collect::<Vec<_>>(),
        vec!["Ada Friend", "Bo Friend", "Cleo Friend"]
    );
    assert_eq!(
        panel
            .rows
            .iter()
            .map(|row| row.friend_id.clone())
            .collect::<Vec<_>>(),
        expected_ids.to_vec()
    );
    assert_eq!(
        panel
            .rows
            .iter()
            .map(|row| row.osl_user_id.as_str())
            .collect::<Vec<_>>(),
        vec![
            FRIEND_WITH_PICTURE,
            FRIEND_WITHOUT_PICTURE,
            FRIEND_PICTURE_NOT_PERMITTED
        ]
    );
    for row in &panel.rows {
        assert!(row.friend_id.starts_with("friend:"), "saved friend id");
        assert!(!row.username.is_empty(), "username");
        assert!(row.initial_colour.starts_with('#'), "initial colour");
    }

    // Permitted picture, or the initial.
    assert_eq!(panel.rows[0].picture.as_deref(), Some(PERMITTED_PICTURE));
    assert_eq!(panel.rows[0].picture_status, "image-present");
    assert_eq!(panel.rows[1].picture, None);
    assert_eq!(panel.rows[1].picture_status, "image-absent");
    assert_eq!(panel.rows[1].initial, "B");
    assert_eq!(panel.rows[2].picture, None);
    assert_eq!(panel.rows[2].picture_status, "image-absent");
    assert_eq!(panel.rows[2].initial, "C");
    assert_eq!(panel.picture_count, 1);
    assert_eq!(panel.rows[0].initial, "A");

    assert_eq!(non_friend_rows, 0);
    assert_eq!(non_friend_picture_count, 0);
    assert_eq!(withheld_picture_count, 0);
    assert_eq!(non_friend_id_count, 0);
    assert_eq!(non_friend_saved_id_count, 0);

    // The non-friend's picture really is on disk; the reader is what withholds it.
    assert_eq!(
        saved_non_friend_picture.as_deref(),
        Some(NON_FRIEND_PICTURE)
    );
}
