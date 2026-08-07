//! TASK 1402 -- the setup account list writes only the ticked account.
//!
//! The screen's own fixture file is the input here. The same two accounts the
//! Linux setup screen renders (`apps/osl-hub-ui/src/scrub-account-choice-fixture.json`)
//! are fed to the commands Continue calls, so this test and the screen cannot
//! drift apart without one of them failing: the ids are read from the file, not
//! retyped.

use std::path::{Path, PathBuf};

use osl_privacy_hub::hub_command_surface::{
    get_scrub_account_permissions_command, save_scrub_account_permissions_command,
};
use osl_privacy_hub::preferences::{PreviewState, ScrubAccountPermissionInput};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repository root above apps/osl-hub")
        .to_path_buf()
}

fn fixture() -> serde_json::Value {
    let path = repo_root().join("apps/osl-hub-ui/src/scrub-account-choice-fixture.json");
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).expect("account-choice fixture is JSON")
}

fn store_path(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("osl-task-1402-{name}-{}-{nonce}", std::process::id()))
        .join("preferences.json")
}

#[test]
fn task_1402_ticking_one_account_and_continuing_writes_only_that_account() {
    let fixture = fixture();
    let owner = fixture["ownerUserId"].as_str().expect("fixture owner");
    let accounts = fixture["accounts"].as_array().expect("fixture accounts");
    assert_eq!(
        accounts.len(),
        2,
        "the setup screen fixture must show two accounts"
    );
    let available = accounts
        .iter()
        .map(|account| {
            account["accountId"]
                .as_str()
                .expect("fixture account id")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let ticked = fixture["tickedAccountId"]
        .as_str()
        .expect("fixture ticked account")
        .to_owned();
    let unticked = available
        .iter()
        .find(|id| **id != ticked)
        .expect("fixture has an unticked account")
        .clone();

    let path = store_path("continue");
    let before = get_scrub_account_permissions_command(&PreviewState::load(path.clone()), owner)
        .expect("read before Continue");
    println!(
        "TASK1402_BEFORE read_ids={} read_count={}",
        before.account_ids.join(","),
        before.account_ids.len()
    );

    // Exactly the payload `continueFromScrubAccountChoice` builds from one tick.
    let saved = save_scrub_account_permissions_command(
        &PreviewState::load(path.clone()),
        owner,
        ScrubAccountPermissionInput {
            available_account_ids: available.clone(),
            selected_account_ids: vec![ticked.clone()],
        },
    )
    .expect("Continue writes the ticked account permission");
    println!(
        "TASK1402_CONTINUE command=save_scrub_account_permissions available_ids={} selected_ids={} saved_ids={}",
        available.join(","),
        ticked,
        saved.account_ids.join(",")
    );

    let read = get_scrub_account_permissions_command(&PreviewState::load(path.clone()), owner)
        .expect("read after Continue");
    println!(
        "TASK1402_AFTER command=get_scrub_account_permissions read_ids={} read_count={} unticked_saved={}",
        read.account_ids.join(","),
        read.account_ids.len(),
        read.account_ids.contains(&unticked)
    );

    assert!(
        before.account_ids.is_empty(),
        "no account may be permitted before Continue"
    );
    assert_eq!(saved.account_ids, vec![ticked.clone()]);
    assert_eq!(read.account_ids, vec![ticked.clone()]);
    assert_eq!(read.account_ids.len(), 1);
    assert!(!read.account_ids.contains(&unticked));

    let _ = std::fs::remove_dir_all(path.parent().expect("store parent"));
}

#[test]
fn task_1402_an_untouched_list_writes_no_account_permission() {
    let fixture = fixture();
    let owner = fixture["ownerUserId"].as_str().expect("fixture owner");
    let available = fixture["accounts"]
        .as_array()
        .expect("fixture accounts")
        .iter()
        .map(|account| {
            account["accountId"]
                .as_str()
                .expect("fixture account id")
                .to_owned()
        })
        .collect::<Vec<_>>();

    // The screen refuses this call, so the store must stay empty even if a
    // caller reaches the command with nothing ticked.
    let path = store_path("empty");
    let saved = save_scrub_account_permissions_command(
        &PreviewState::load(path.clone()),
        owner,
        ScrubAccountPermissionInput {
            available_account_ids: available.clone(),
            selected_account_ids: Vec::new(),
        },
    )
    .expect("an empty selection is a valid, empty write");
    let read = get_scrub_account_permissions_command(&PreviewState::load(path.clone()), owner)
        .expect("read after an empty write");
    println!(
        "TASK1402_NO_TICK saved_count={} read_ids={} read_count={}",
        saved.account_ids.len(),
        read.account_ids.join(","),
        read.account_ids.len()
    );

    assert!(saved.account_ids.is_empty());
    assert!(read.account_ids.is_empty());

    let _ = std::fs::remove_dir_all(path.parent().expect("store parent"));
}
