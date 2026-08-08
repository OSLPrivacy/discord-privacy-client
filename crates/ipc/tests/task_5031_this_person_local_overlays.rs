//! TASK 5031 — the THIS PERSON block: rename them and colour them, just for me.
//!
//! Everything this test asserts is produced by the real commands in
//! `ipc::commands`, reading the real on-disk overlay file and the real
//! friend-request state file. Nothing is asserted from a constant that the
//! feature does not have to produce.
//!
//! Two accounts get a directory: `dir_me` is this install, `dir_them` is the
//! other account's install. The rename and the colour are saved in `dir_me`.
//! `dir_them` is measured before and after, because "the other person never
//! learns either" is a claim about bytes leaving this machine, and the only
//! honest local check of it is that nothing was written there at all.

use ipc::commands::{
    cmd_osl_clear_this_person_local_rename, cmd_osl_set_this_person_local_colour,
    cmd_osl_set_this_person_local_rename, cmd_osl_this_person_block, cmd_osl_this_person_places,
    cmd_osl_this_person_safety_number_identity,
};
use ipc::friend_request::{
    load_friend_request_file_state, save_friend_request_file_state, FriendRequestFileState,
    StoredFriendBlockState, StoredFriendRecord, StoredFriendState,
};
use ipc::main_password::set_file_storage_key;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const TASK_5031_FILE_STORAGE_KEY: [u8; 32] = [0x51; 32];

/// The other account's own profile name, as their record publishes it.
const PROFILE_NAME: &str = "Mara Vale";
/// What I call them, here, only here.
const LOCAL_RENAME: &str = "Roof Contact";
/// The colour I gave them, from the palette OSL offers.
const LOCAL_COLOUR: &str = "#a855f7";

const MY_ID: &str = "osl_local_operator";
const THEIR_ID: &str = "osl_remote_mara";

struct FileStorageKeyGuard;

impl Drop for FileStorageKeyGuard {
    fn drop(&mut self) {
        set_file_storage_key(None);
    }
}

fn seed_friend_record(dir: &Path, local_id: &str, remote_id: &str, display_name: &str) {
    let mut state = FriendRequestFileState::new();
    state.friends.push(StoredFriendRecord {
        record_id: format!("record-{remote_id}"),
        local_identity_id: local_id.to_owned(),
        remote_identity_id: remote_id.to_owned(),
        state: StoredFriendState::Accepted,
        display_name: display_name.to_owned(),
        block_state: StoredFriendBlockState::NotBlocked,
        choices: BTreeMap::new(),
    });
    save_friend_request_file_state(dir, &state).expect("seed friend record");
}

/// The other account's own profile record, as it stands on disk right now.
fn profile_record_json(dir: &Path, remote_id: &str) -> String {
    let state = load_friend_request_file_state(dir).expect("load friend record");
    let record = state
        .friends
        .iter()
        .find(|friend| friend.remote_identity_id == remote_id)
        .expect("profile record is present");
    serde_json::to_string(record).expect("serialize profile record")
}

fn profile_record_choice_count(dir: &Path, remote_id: &str) -> usize {
    let state = load_friend_request_file_state(dir).expect("load friend record");
    state
        .friends
        .iter()
        .find(|friend| friend.remote_identity_id == remote_id)
        .expect("profile record is present")
        .choices
        .len()
}

/// Every file in a directory tree, as (relative path, raw bytes on disk).
fn snapshot_dir(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path
                    .strip_prefix(dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, bytes));
            }
        }
    }
    out.sort();
    out
}

fn count_needle_in_snapshot(snapshot: &[(String, Vec<u8>)], needle: &str) -> usize {
    snapshot
        .iter()
        .map(|(_, bytes)| {
            bytes
                .windows(needle.len().max(1))
                .filter(|window| *window == needle.as_bytes())
                .count()
        })
        .sum()
}

#[test]
fn task_5031_local_rename_and_colour_stay_local_and_never_replace_the_profile_name() {
    set_file_storage_key(Some(TASK_5031_FILE_STORAGE_KEY));
    let _guard = FileStorageKeyGuard;

    let root = tempfile::tempdir().expect("temp root");
    let dir_me: PathBuf = root.path().join("me");
    let dir_them: PathBuf = root.path().join("them");
    std::fs::create_dir_all(&dir_me).expect("my dir");
    std::fs::create_dir_all(&dir_them).expect("their dir");

    // My install knows them by the name their profile publishes.
    seed_friend_record(&dir_me, MY_ID, THEIR_ID, PROFILE_NAME);
    // Their install holds their own profile record, from their side.
    seed_friend_record(&dir_them, THEIR_ID, MY_ID, "Operator");

    let their_install_before = snapshot_dir(&dir_them);
    let record_json_before = profile_record_json(&dir_me, THEIR_ID);

    // ---- Before any overlay: all three places show the profile name --------
    let places_before = cmd_osl_this_person_places(dir_me.clone(), THEIR_ID.to_owned())
        .expect("places before overlay");
    let names_before: Vec<String> = places_before.iter().map(|p| p.name.clone()).collect();
    println!(
        "TASK5031 before_overlay places={} names={} all_profile_name={}",
        places_before.len(),
        names_before.join(","),
        names_before.iter().filter(|n| *n == PROFILE_NAME).count()
    );
    assert_eq!(places_before.len(), 3);
    assert!(names_before.iter().all(|name| name == PROFILE_NAME));

    // ---- Save a local colour, then a local rename --------------------------
    let after_colour = cmd_osl_set_this_person_local_colour(
        dir_me.clone(),
        THEIR_ID.to_owned(),
        Some(LOCAL_COLOUR.to_owned()),
    )
    .expect("save local colour");
    let after_rename = cmd_osl_set_this_person_local_rename(
        dir_me.clone(),
        THEIR_ID.to_owned(),
        Some(LOCAL_RENAME.to_owned()),
    )
    .expect("save local rename");
    println!(
        "TASK5031 saved local_rename={} local_colour={} profile_name_in_block={}",
        after_rename.local_rename.clone().unwrap_or_default(),
        after_rename.local_colour.clone().unwrap_or_default(),
        after_rename.profile_name
    );
    assert_eq!(after_colour.local_colour.as_deref(), Some(LOCAL_COLOUR));
    assert_eq!(after_rename.local_rename.as_deref(), Some(LOCAL_RENAME));
    assert_eq!(after_rename.local_colour.as_deref(), Some(LOCAL_COLOUR));
    assert_eq!(after_rename.profile_name, PROFILE_NAME);

    // ---- Finish line 1: sidebar, thread, notifications, all three matching --
    let places = cmd_osl_this_person_places(dir_me.clone(), THEIR_ID.to_owned())
        .expect("places after overlay");
    let place_names: Vec<String> = places
        .iter()
        .map(|p| format!("{}={}", p.place, p.name))
        .collect();
    let place_colours: Vec<String> = places
        .iter()
        .map(|p| format!("{}={}", p.place, p.colour))
        .collect();
    let matching_names = places.iter().filter(|p| p.name == LOCAL_RENAME).count();
    let matching_colours = places.iter().filter(|p| p.colour == LOCAL_COLOUR).count();
    println!(
        "TASK5031 places={} names[{}] colours[{}] matching_name_places={} matching_colour_places={}",
        places.len(),
        place_names.join(" "),
        place_colours.join(" "),
        matching_names,
        matching_colours
    );
    let distinct_places: std::collections::BTreeSet<&str> =
        places.iter().map(|p| p.place).collect();
    println!(
        "TASK5031 distinct_places={} names={:?}",
        distinct_places.len(),
        distinct_places
    );
    assert_eq!(places.len(), 3);
    assert_eq!(distinct_places.len(), 3);
    assert!(distinct_places.contains("sidebar"));
    assert!(distinct_places.contains("thread"));
    assert!(distinct_places.contains("notification"));
    assert_eq!(matching_names, 3);
    assert_eq!(matching_colours, 3);

    // ---- Finish line 2: the other account's profile record holds 0 trace ----
    let record_json_after = profile_record_json(&dir_me, THEIR_ID);
    let rename_hits_in_record = record_json_after.matches(LOCAL_RENAME).count();
    let colour_hits_in_record = record_json_after.matches(LOCAL_COLOUR).count();
    let choice_count = profile_record_choice_count(&dir_me, THEIR_ID);
    println!(
        "TASK5031 profile_record_rename_hits={} profile_record_colour_hits={} profile_record_choices={} record_unchanged={}",
        rename_hits_in_record,
        colour_hits_in_record,
        choice_count,
        record_json_after == record_json_before
    );
    println!("TASK5031 profile_record_json={record_json_after}");
    assert_eq!(rename_hits_in_record, 0);
    assert_eq!(colour_hits_in_record, 0);
    assert_eq!(choice_count, 0);
    assert_eq!(record_json_after, record_json_before);

    let their_install_after = snapshot_dir(&dir_them);
    let rename_hits_their_install = count_needle_in_snapshot(&their_install_after, LOCAL_RENAME);
    let colour_hits_their_install = count_needle_in_snapshot(&their_install_after, LOCAL_COLOUR);
    println!(
        "TASK5031 their_install_files={} their_install_bytes_changed={} rename_hits_their_install={} colour_hits_their_install={}",
        their_install_after.len(),
        usize::from(their_install_after != their_install_before),
        rename_hits_their_install,
        colour_hits_their_install
    );
    assert_eq!(their_install_after, their_install_before);
    assert_eq!(rename_hits_their_install, 0);
    assert_eq!(colour_hits_their_install, 0);

    // ---- The overlay is what actually persisted, read back off disk ---------
    let reloaded = cmd_osl_this_person_block(dir_me.clone(), THEIR_ID.to_owned())
        .expect("reload THIS PERSON block");
    println!(
        "TASK5031 reloaded_rename={} reloaded_colour={} reloaded_profile_name={}",
        reloaded.local_rename.clone().unwrap_or_default(),
        reloaded.local_colour.clone().unwrap_or_default(),
        reloaded.profile_name
    );
    assert_eq!(reloaded.local_rename.as_deref(), Some(LOCAL_RENAME));
    assert_eq!(reloaded.local_colour.as_deref(), Some(LOCAL_COLOUR));

    // ---- Finish line 4: the safety-number screen, rename beside not instead -
    let identity = cmd_osl_this_person_safety_number_identity(dir_me.clone(), THEIR_ID.to_owned())
        .expect("safety number identity");
    let profile_at = identity
        .line
        .find(PROFILE_NAME)
        .expect("safety-number line still carries the profile name");
    let rename_at = identity
        .line
        .find(LOCAL_RENAME)
        .expect("safety-number line carries the local rename beside it");
    println!(
        "TASK5031 safety_number_profile_name={} safety_number_local_rename={} safety_number_line={} profile_at={} rename_at={} replaces={}",
        identity.profile_name,
        identity.local_rename.clone().unwrap_or_default(),
        identity.line,
        profile_at,
        rename_at,
        identity.rename_replaces_profile_name
    );
    assert_eq!(identity.profile_name, PROFILE_NAME);
    assert_eq!(identity.local_rename.as_deref(), Some(LOCAL_RENAME));
    assert!(identity.line.contains(PROFILE_NAME));
    assert!(identity.line.contains(LOCAL_RENAME));
    assert!(profile_at < rename_at, "the profile name comes first");
    assert!(!identity.rename_replaces_profile_name);

    // ---- Finish line 3: clearing the rename restores the profile name -------
    let cleared = cmd_osl_clear_this_person_local_rename(dir_me.clone(), THEIR_ID.to_owned())
        .expect("clear local rename");
    let places_cleared = cmd_osl_this_person_places(dir_me.clone(), THEIR_ID.to_owned())
        .expect("places after clearing");
    let restored_exact = places_cleared
        .iter()
        .filter(|p| p.name == PROFILE_NAME)
        .count();
    let rename_left = places_cleared
        .iter()
        .filter(|p| p.name == LOCAL_RENAME)
        .count();
    println!(
        "TASK5031 cleared_rename={:?} restored_places={} places_still_renamed={} restored_name={} byte_equal={} colour_kept={}",
        cleared.local_rename,
        restored_exact,
        rename_left,
        places_cleared[0].name,
        places_cleared[0].name.as_bytes() == PROFILE_NAME.as_bytes(),
        cleared.local_colour.clone().unwrap_or_default()
    );
    assert_eq!(cleared.local_rename, None);
    assert_eq!(restored_exact, 3);
    assert_eq!(rename_left, 0);
    for place in &places_cleared {
        assert_eq!(place.name.as_bytes(), PROFILE_NAME.as_bytes());
    }
    // Clearing the rename is not clearing the colour: two separate choices.
    assert_eq!(cleared.local_colour.as_deref(), Some(LOCAL_COLOUR));

    let identity_cleared =
        cmd_osl_this_person_safety_number_identity(dir_me.clone(), THEIR_ID.to_owned())
            .expect("safety number identity after clearing");
    println!(
        "TASK5031 cleared_safety_number_line={} cleared_safety_number_rename={:?}",
        identity_cleared.line, identity_cleared.local_rename
    );
    assert_eq!(identity_cleared.line, PROFILE_NAME);
    assert_eq!(identity_cleared.local_rename, None);

    println!(
        "TASK5031 local_only_sentence={}",
        reloaded.local_only_sentence
    );
}

/// The pure helpers the three surfaces share.
///
/// These live here rather than in `#[cfg(test)] mod tests` inside the module
/// because the `ipc` lib-test target does not compile at this commit for
/// unrelated reasons (`wire_rn.rs` calls a `RnSessionStore::pin_path` that does
/// not exist), so an in-module test would never actually run.
#[test]
fn task_5031_overlay_helpers_normalise_and_derive() {
    use ipc::this_person_overlay::{
        derived_person_colour, normalise_local_colour, normalise_local_rename,
        THIS_PERSON_COLOUR_CHOICES,
    };

    assert_eq!(
        normalise_local_rename(Some("  Roof Contact  "))
            .unwrap()
            .as_deref(),
        Some("Roof Contact")
    );
    assert_eq!(normalise_local_rename(Some("   ")).unwrap(), None);
    assert_eq!(normalise_local_rename(None).unwrap(), None);
    assert!(normalise_local_rename(Some("<b>Roof</b>")).is_err());
    assert!(normalise_local_rename(Some(&"a".repeat(65))).is_err());

    assert_eq!(
        normalise_local_colour(Some("#A855F7")).unwrap().as_deref(),
        Some("#a855f7")
    );
    assert_eq!(normalise_local_colour(Some("")).unwrap(), None);
    assert!(normalise_local_colour(Some("#000000")).is_err());

    let derived = derived_person_colour(THEIR_ID);
    println!(
        "TASK5031 derived_colour={derived} stable={} in_palette={}",
        derived == derived_person_colour(THEIR_ID),
        THIS_PERSON_COLOUR_CHOICES.contains(&derived.as_str())
    );
    assert_eq!(derived, derived_person_colour(THEIR_ID));
    assert!(THIS_PERSON_COLOUR_CHOICES.contains(&derived.as_str()));
}

/// A rename is a label, not a licence to redraw someone as someone else.
#[test]
fn task_5031_refuses_a_rename_that_could_disguise_the_row() {
    set_file_storage_key(Some(TASK_5031_FILE_STORAGE_KEY));
    let _guard = FileStorageKeyGuard;

    let root = tempfile::tempdir().expect("temp root");
    let dir_me = root.path().join("me");
    std::fs::create_dir_all(&dir_me).expect("my dir");
    seed_friend_record(&dir_me, MY_ID, THEIR_ID, PROFILE_NAME);

    let bidi = cmd_osl_set_this_person_local_rename(
        dir_me.clone(),
        THEIR_ID.to_owned(),
        Some("Mara\u{202e}elaV".to_owned()),
    );
    let newline = cmd_osl_set_this_person_local_rename(
        dir_me.clone(),
        THEIR_ID.to_owned(),
        Some("Mara\nVale".to_owned()),
    );
    let bad_colour = cmd_osl_set_this_person_local_colour(
        dir_me.clone(),
        THEIR_ID.to_owned(),
        Some("#000000".to_owned()),
    );
    let unknown_person = cmd_osl_set_this_person_local_rename(
        dir_me.clone(),
        "osl_nobody".to_owned(),
        Some("Ghost".to_owned()),
    );
    println!(
        "TASK5031 refused_bidi={} refused_newline={} refused_colour={} refused_unknown_person={}",
        bidi.clone().unwrap_err(),
        newline.clone().unwrap_err(),
        bad_colour.clone().unwrap_err(),
        unknown_person.clone().unwrap_err()
    );
    assert!(bidi.is_err());
    assert!(newline.is_err());
    assert!(bad_colour.is_err());
    assert!(unknown_person.is_err());

    let places = cmd_osl_this_person_places(dir_me.clone(), THEIR_ID.to_owned())
        .expect("places after refusals");
    let still_profile = places.iter().filter(|p| p.name == PROFILE_NAME).count();
    println!("TASK5031 places_still_profile_name_after_refusals={still_profile}");
    assert_eq!(still_profile, 3);
}
