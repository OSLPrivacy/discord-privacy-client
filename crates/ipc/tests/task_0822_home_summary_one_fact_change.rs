//! TASK 0822 - change one saved protection fact, then read the Home summary
//! directly and see only that fact (and the next safe step it moves) change.
//!
//! Every case here builds ONE saved account, photographs its summary, changes a
//! single saved protection fact on disk, and reads the summary again through
//! `cmd_osl_home_protection_summary`. Two things have to hold:
//!
//!   * the set of summary fields that changed is exactly the matching fact plus
//!     the next safe step - not "at least", not "roughly";
//!   * the set of saved files that changed on disk is exactly the one file the
//!     change touched, so "only one fact changed" is measured and not assumed.
//!
//! The primary case is also pinned to a committed fixture. Pointing the check at
//! the fixture written BEFORE the change - the one without the changed
//! protection fact - has to make it fail; `task_0822_check_fails_...` asserts
//! that, and `OSL_TASK_0822_FIXTURE=<path>` lets the same swap be done from the
//! command line.
//!
//! Regenerate the fixtures with `OSL_TASK_0822_WRITE=1`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::allowed_places::AllowedPlaceRecord;
use ipc::commands::{
    cmd_osl_home_protection_summary, cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule,
    cmd_osl_save_privacy_level_rule_set, cmd_osl_save_verification_warning_choice,
    HomeProtectionSummaryDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::peer_map::{PeerEntry, PeerMap};
use ipc::state::{AppState, CloudRegistrationState};
use ipc::tofu::KeyBundle;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The file storage key is process-wide, so the two tests in this binary take
/// turns rather than trusting `--test-threads=1` to be passed.
static KEY_LOCK: Mutex<()> = Mutex::new(());

fn with_file_storage_key<T>(body: impl FnOnce() -> T) -> T {
    let _guard = KEY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    set_file_storage_key(Some([0x82; 32]));
    let out = body();
    set_file_storage_key(None);
    out
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

fn trusted_peer(discord_id: &str, label: &str) -> PeerEntry {
    let identity = keystore::generate_identity(label.to_string());
    PeerEntry {
        osl_user_id: Some(label.to_string()),
        discord_id: Some(discord_id.to_string()),
        tofu_key_bundle: Some(identity_bundle(&identity)),
        ..PeerEntry::default()
    }
}

const OWNER: &str = "task-0822-owner";
const PERSON_ID: &str = "900000000000082201";

/// A saved account with one of each protection fact: protected, privacy level
/// "maximum", verification warning "before sending", one trusted person, one
/// connected app. One of each, so removing one is a change of exactly one fact.
struct SavedAccount {
    dir: tempfile::TempDir,
    place_stable_id: String,
}

impl SavedAccount {
    fn build() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = AppState::new();
        state.install_identity(keystore::generate_identity(OWNER.to_string()));
        *state.keyserver.lock().unwrap() =
            Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
        state.set_cloud_registration_state(CloudRegistrationState::Registered);

        cmd_osl_save_privacy_level_rule_set(
            &state,
            "maximum".to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save privacy level");
        cmd_osl_save_verification_warning_choice(
            &state,
            "before sending".to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save verification warning");

        let mut peer_map: PeerMap = HashMap::new();
        peer_map.insert(
            PERSON_ID.to_string(),
            trusted_peer(PERSON_ID, "rose-task-0822"),
        );
        ipc::peer_map::write_peer_map(&dir.path().join("peer_map.json"), &peer_map)
            .expect("persist the trusted person");

        cmd_osl_save_auto_whitelist_rule(
            &state,
            "discord".to_string(),
            "always".to_string(),
            Some(dir.path().to_path_buf()),
        )
        .expect("save discord app rule");
        let place = AllowedPlaceRecord::discord_direct_message(OWNER, PERSON_ID.to_string());
        let place_stable_id = place.stable_id.clone();
        cmd_osl_new_place(&state, place, Some(dir.path().to_path_buf()))
            .expect("persist the connected app place");

        Self {
            dir,
            place_stable_id,
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Read the Home summary the way the app would after a restart: a fresh
    /// state, everything loaded back off disk, then the direct command.
    fn read_summary(&self) -> HomeProtectionSummaryDto {
        let state = AppState::new();
        state.install_identity(keystore::generate_identity(OWNER.to_string()));
        *state.keyserver.lock().unwrap() =
            Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
        state.set_cloud_registration_state(CloudRegistrationState::Registered);
        *state.app_preferences.lock().unwrap() =
            ipc::app_preferences::load_app_preferences(&self.path().join("app_preferences.json"));
        *state.peer_map.lock().unwrap() =
            match ipc::peer_map::load_peer_map_from_path(&self.path().join("peer_map.json")) {
                Ok(map) => map,
                Err(ipc::peer_map::PeerMapError::NotFound { .. }) => HashMap::new(),
                Err(error) => panic!("load persisted peer map: {error:?}"),
            };
        cmd_osl_home_protection_summary(&state, Some(self.path().to_path_buf()))
            .expect("direct Home summary")
    }

    /// Every saved file under the account directory, by content digest. Two of
    /// these maps say which saved files a change actually touched.
    fn saved_files(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        collect_files(self.path(), self.path(), &mut out);
        out
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in std::fs::read_dir(dir).expect("read saved directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
            continue;
        }
        let bytes = std::fs::read(&path).expect("read saved file");
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let name = path
            .strip_prefix(root)
            .expect("path under root")
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(name, digest);
    }
}

fn changed_keys(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut names: Vec<String> = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|name| before.get(name) != after.get(name))
        .collect();
    names.sort();
    names
}

/// The summary as a flat `field` / `field.sub_field` map, so "which facts
/// changed" is a set difference and not an eyeball.
fn flat_fields(summary: &HomeProtectionSummaryDto) -> BTreeMap<String, String> {
    let value = serde_json::to_value(summary).expect("serialise summary");
    let mut out = BTreeMap::new();
    flatten(&value, "", &mut out);
    out
}

fn flatten(value: &Value, prefix: &str, out: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(child, &name, out);
            }
        }
        other => {
            out.insert(prefix.to_string(), other.to_string());
        }
    }
}

fn changed_summary_fields(
    before: &HomeProtectionSummaryDto,
    after: &HomeProtectionSummaryDto,
) -> Vec<String> {
    changed_keys(&flat_fields(before), &flat_fields(after))
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn before_fixture() -> PathBuf {
    fixture_dir().join("task-0822-home-summary-before-change.json")
}

fn after_fixture() -> PathBuf {
    fixture_dir().join("task-0822-home-summary-after-change.json")
}

fn write_fixture(path: &Path, summary: &HomeProtectionSummaryDto) {
    std::fs::create_dir_all(fixture_dir()).expect("fixture directory");
    let mut json = serde_json::to_string_pretty(summary).expect("serialise summary");
    json.push('\n');
    std::fs::write(path, json).expect("write fixture");
    println!("TASK0822 wrote {}", path.display());
}

/// The check the finish line talks about: does this fixture carry the changed
/// protection fact, and is it what the command returns after the change?
///
/// Returns the first disagreement rather than panicking, so the negative
/// control can assert that a fixture WITHOUT the change makes it fail.
fn check_fixture_has_the_changed_fact(
    path: &Path,
    before_change: &HomeProtectionSummaryDto,
    after_change: &HomeProtectionSummaryDto,
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let fixture: HomeProtectionSummaryDto =
        serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;

    let fixture_fields = flat_fields(&fixture);
    let before_fields = flat_fields(before_change);
    let after_fields = flat_fields(after_change);

    // 1. The fixture has to carry the changed protection fact - not the value
    //    the account had before the change.
    let changed = changed_keys(&before_fields, &after_fields);
    let mut wrong = Vec::new();
    let mut all_stale = true;
    for field in &changed {
        let want = after_fields.get(field).cloned().unwrap_or_default();
        let got = fixture_fields.get(field).cloned().unwrap_or_default();
        if got != want {
            let stale = before_fields.get(field).cloned().unwrap_or_default();
            all_stale &= got == stale;
            wrong.push(format!("{field} is {got}, not {want}"));
        }
    }
    if !wrong.is_empty() {
        let note = if all_stale {
            " - every one of them is the value from BEFORE the change, so this fixture does not carry the changed protection fact"
        } else {
            ""
        };
        return Err(format!(
            "{}: the saved protection fact moved {} field(s) this fixture disagrees with: {}{note}",
            path.display(),
            wrong.len(),
            wrong.join("; ")
        ));
    }

    // 2. And nothing else may drift either.
    if fixture_fields != after_fields {
        let drift = changed_keys(&fixture_fields, &after_fields);
        return Err(format!(
            "{}: disagrees with the direct summary on {}",
            path.display(),
            drift.join(", ")
        ));
    }
    Ok(())
}

#[test]
fn task_0822_one_saved_protection_fact_changes_only_its_summary_fact_and_next_step() {
    with_file_storage_key(|| {
        // Each case: a name, the change to one saved protection fact, the saved
        // files that may change, and the summary fields that may change.
        type Change = Box<dyn Fn(&SavedAccount)>;
        let cases: Vec<(&str, Change, Vec<&str>, Vec<&str>)> = vec![
            (
                "the one trusted person is removed",
                Box::new(|account: &SavedAccount| {
                    ipc::peer_map::write_peer_map(
                        &account.path().join("peer_map.json"),
                        &HashMap::new(),
                    )
                    .expect("remove the trusted person");
                }),
                vec!["peer_map.json"],
                vec!["next_safe_step", "trusted_people", "trusted_people_count"],
            ),
            (
                "the one connected app place is removed",
                Box::new(|account: &SavedAccount| {
                    let removed = ipc::allowed_places::remove_allowed_place_record(
                        account.path(),
                        &account.place_stable_id,
                    )
                    .expect("remove the connected app place");
                    assert!(removed, "the place to remove was not there to remove");
                }),
                vec!["allowed_places.sqlite"],
                vec![
                    "allowed_place_count",
                    "apps",
                    "connected_app_count",
                    "next_safe_step",
                ],
            ),
            (
                "the saved privacy level drops from maximum to basic",
                Box::new(|account: &SavedAccount| {
                    let state = AppState::new();
                    *state.app_preferences.lock().unwrap() =
                        ipc::app_preferences::load_app_preferences(
                            &account.path().join("app_preferences.json"),
                        );
                    cmd_osl_save_privacy_level_rule_set(
                        &state,
                        "basic".to_string(),
                        Some(account.path().to_path_buf()),
                    )
                    .expect("save the new privacy level");
                }),
                vec!["app_preferences.json"],
                vec![
                    "privacy_level",
                    "protection_choices.app_exceptions",
                    "protection_choices.cleanup",
                    "protection_choices.contact_rules",
                    "protection_choices.label",
                    "protection_choices.level",
                    "protection_choices.warnings",
                ],
            ),
            (
                "the saved verification warning moves to every time",
                Box::new(|account: &SavedAccount| {
                    let state = AppState::new();
                    *state.app_preferences.lock().unwrap() =
                        ipc::app_preferences::load_app_preferences(
                            &account.path().join("app_preferences.json"),
                        );
                    cmd_osl_save_verification_warning_choice(
                        &state,
                        "every time".to_string(),
                        Some(account.path().to_path_buf()),
                    )
                    .expect("save the new verification warning");
                }),
                vec!["app_preferences.json"],
                vec!["verification_warning"],
            ),
        ];

        for (name, apply, expected_files, expected_fields) in cases {
            let account = SavedAccount::build();
            let before = account.read_summary();
            let files_before = account.saved_files();

            apply(&account);

            let after = account.read_summary();
            let files_after = account.saved_files();

            let touched = changed_keys(&files_before, &files_after);
            let fields = changed_summary_fields(&before, &after);

            println!("TASK0822 case={name}");
            println!("TASK0822   saved_files_changed={}", touched.join("|"));
            println!("TASK0822   summary_fields_changed={}", fields.join("|"));
            println!(
                "TASK0822   next_safe_step: {:?} -> {:?}",
                before.next_safe_step, after.next_safe_step
            );

            assert_eq!(
                touched, expected_files,
                "case '{name}' changed saved files other than the one protection fact it was meant to"
            );
            assert_eq!(
                fields, expected_fields,
                "case '{name}' moved summary facts other than the matching one and the next step"
            );
        }

        // The primary case again, this time pinned to committed fixtures: the
        // account before the change, and the account after it.
        let account = SavedAccount::build();
        let before = account.read_summary();
        ipc::peer_map::write_peer_map(&account.path().join("peer_map.json"), &HashMap::new())
            .expect("remove the trusted person");
        let after = account.read_summary();

        println!(
            "TASK0822 before.trusted_people_count={} before.trusted_people={:?} before.next_safe_step={:?}",
            before.trusted_people_count, before.trusted_people, before.next_safe_step
        );
        println!(
            "TASK0822 after.trusted_people_count={} after.trusted_people={:?} after.next_safe_step={:?}",
            after.trusted_people_count, after.trusted_people, after.next_safe_step
        );

        assert_eq!(before.trusted_people_count, 1);
        assert_eq!(before.trusted_people, "1 trusted person");
        assert_eq!(before.next_safe_step, "Open a protected conversation");
        assert_eq!(after.trusted_people_count, 0);
        assert_eq!(after.trusted_people, "0 trusted people");
        assert_eq!(after.next_safe_step, "Add a trusted person");
        // The facts the change did not touch stayed put.
        assert_eq!(before.protection_state, after.protection_state);
        assert_eq!(before.privacy_level, after.privacy_level);
        assert_eq!(before.protection_choices, after.protection_choices);
        assert_eq!(before.verification_warning, after.verification_warning);
        assert_eq!(before.connected_app_count, after.connected_app_count);
        assert_eq!(before.allowed_place_count, after.allowed_place_count);
        assert_eq!(before.apps, after.apps);

        if std::env::var("OSL_TASK_0822_WRITE").as_deref() == Ok("1") {
            write_fixture(&before_fixture(), &before);
            write_fixture(&after_fixture(), &after);
        }

        let fixture = std::env::var("OSL_TASK_0822_FIXTURE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| after_fixture());
        println!("TASK0822 checking fixture {}", fixture.display());
        if let Err(error) = check_fixture_has_the_changed_fact(&fixture, &before, &after) {
            panic!("TASK0822 fixture check failed: {error}");
        }
        println!(
            "TASK0822 fixture {} carries the changed protection fact and matches the direct summary",
            fixture.display()
        );
    });
}

#[test]
fn task_0822_check_fails_on_a_fixture_without_the_changed_protection_fact() {
    with_file_storage_key(|| {
        let account = SavedAccount::build();
        let before = account.read_summary();
        ipc::peer_map::write_peer_map(&account.path().join("peer_map.json"), &HashMap::new())
            .expect("remove the trusted person");
        let after = account.read_summary();

        // Pointed at the fixture written after the change: passes.
        let good = check_fixture_has_the_changed_fact(&after_fixture(), &before, &after);
        println!("TASK0822 check(after-change fixture) = {good:?}");
        assert!(
            good.is_ok(),
            "the fixture holding the changed protection fact should pass: {good:?}"
        );

        // Pointed at the fixture written before the change - same account, same
        // command, but without the changed protection fact: fails.
        let bad = check_fixture_has_the_changed_fact(&before_fixture(), &before, &after);
        let message = bad.expect_err(
            "a fixture without the changed protection fact must make the check fail, or the check proves nothing",
        );
        println!("TASK0822 check(before-change fixture) failed with: {message}");
        assert!(
            message.contains("does not carry the changed protection fact"),
            "the failure should say the fixture lacks the change, got: {message}"
        );
        assert!(
            message.contains("trusted_people_count"),
            "the failure should name the fact that differs, got: {message}"
        );
    });
}
