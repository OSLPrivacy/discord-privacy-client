//! TASK 0821 - the fixtures the Home summary screenshot renders ARE the direct
//! command's answer.
//!
//! The screen capture cannot call into Rust, so the two models it photographs
//! are written here by `cmd_osl_home_protection_summary` itself and committed
//! next to the capture. This test recomputes both from saved state and refuses
//! to pass unless the committed files still match byte for byte, so "the action
//! route matches direct data exactly" cannot drift into "the action route
//! matches a JSON file somebody typed".
//!
//! Regenerate with `OSL_TASK_0821_WRITE=1`.

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
use std::collections::HashMap;
use std::path::PathBuf;

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

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/osl-hub-ui/screenshots/fixtures")
        .canonicalize()
        .expect("osl-hub-ui screenshot fixture directory")
}

/// The saved state of TASK 0820: protected account, two trusted people, two
/// connected apps.
fn ready_summary() -> HomeProtectionSummaryDto {
    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    saving_state.install_identity(keystore::generate_identity("task-0821-owner".to_string()));
    *saving_state.keyserver.lock().unwrap() =
        Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
    saving_state.set_cloud_registration_state(CloudRegistrationState::Registered);

    cmd_osl_save_privacy_level_rule_set(
        &saving_state,
        "maximum".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save privacy level");
    cmd_osl_save_verification_warning_choice(
        &saving_state,
        "before sending".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save verification warning");

    let mut peer_map: PeerMap = HashMap::new();
    let (rose_id, rose) = trusted_peer("900000000000082101", "rose-task-0821");
    let (sam_id, sam) = trusted_peer("900000000000082102", "sam-task-0821");
    peer_map.insert(rose_id.clone(), rose);
    peer_map.insert(sam_id.clone(), sam);
    ipc::peer_map::write_peer_map(&dir.path().join("peer_map.json"), &peer_map)
        .expect("persist trusted people");

    cmd_osl_save_auto_whitelist_rule(
        &saving_state,
        "discord".to_string(),
        "always".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save discord app rule");
    cmd_osl_new_place(
        &saving_state,
        AllowedPlaceRecord::discord_direct_message("task-0821-owner", rose_id),
        Some(dir.path().to_path_buf()),
    )
    .expect("persist discord app place");
    cmd_osl_save_auto_whitelist_rule(
        &saving_state,
        "telegram".to_string(),
        "always".to_string(),
        Some(dir.path().to_path_buf()),
    )
    .expect("save telegram app rule");
    cmd_osl_new_place(
        &saving_state,
        AllowedPlaceRecord {
            app: "telegram".to_string(),
            account: "task-0821-owner".to_string(),
            kind: "direct_message".to_string(),
            stable_id: format!("telegram:task-0821-owner:direct_message:{sam_id}"),
            place_name: sam_id.clone(),
            person_name: sam_id.clone(),
        },
        Some(dir.path().to_path_buf()),
    )
    .expect("persist telegram app place");

    let loaded_state = AppState::new();
    loaded_state.install_identity(keystore::generate_identity("task-0821-owner".to_string()));
    *loaded_state.keyserver.lock().unwrap() =
        Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
    loaded_state.set_cloud_registration_state(CloudRegistrationState::Registered);
    *loaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));
    *loaded_state.peer_map.lock().unwrap() =
        ipc::peer_map::load_peer_map_from_path(&dir.path().join("peer_map.json"))
            .expect("load persisted peer map");

    cmd_osl_home_protection_summary(&loaded_state, Some(dir.path().to_path_buf()))
        .expect("direct Home summary")
}

/// A first run: nothing saved at all. This is the empty state the screenshot
/// has to differ from.
fn empty_summary() -> HomeProtectionSummaryDto {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = AppState::new();
    cmd_osl_home_protection_summary(&state, Some(dir.path().to_path_buf()))
        .expect("direct Home summary for an empty account")
}

fn check_fixture(name: &str, summary: &HomeProtectionSummaryDto) {
    let path = fixture_dir().join(name);
    let mut json = serde_json::to_string_pretty(summary).expect("serialise summary");
    json.push('\n');
    if std::env::var("OSL_TASK_0821_WRITE").as_deref() == Ok("1") {
        std::fs::write(&path, &json).expect("write fixture");
        println!("TASK0821 wrote {}", path.display());
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert_eq!(
        committed,
        json,
        "{} is not what cmd_osl_home_protection_summary returns; the screenshot would be photographing something the backend never said",
        path.display()
    );
    println!("TASK0821 {name} matches direct data: {} bytes", json.len());
}

#[test]
fn task_0821_screenshot_fixtures_are_the_direct_home_summary() {
    set_file_storage_key(Some([0x82; 32]));
    let _key_guard = FileStorageKeyGuard;

    let ready = ready_summary();
    println!("TASK0821 ready.protection_state={}", ready.protection_state);
    println!(
        "TASK0821 ready.trusted_people_count={} ready.trusted_people={}",
        ready.trusted_people_count, ready.trusted_people
    );
    println!(
        "TASK0821 ready.connected_app_count={} ready.apps={}",
        ready.connected_app_count, ready.apps
    );
    println!("TASK0821 ready.next_safe_step={}", ready.next_safe_step);

    assert_eq!(ready.protection_state, "protected");
    assert_eq!(ready.trusted_people_count, 2);
    assert_eq!(ready.trusted_people, "2 trusted people");
    assert_eq!(ready.connected_app_count, 2);
    assert_eq!(ready.apps, "2 connected apps");
    assert_eq!(ready.next_safe_step, "Open a protected conversation");

    let empty = empty_summary();
    println!("TASK0821 empty.protection_state={}", empty.protection_state);
    println!(
        "TASK0821 empty.trusted_people_count={} empty.trusted_people={}",
        empty.trusted_people_count, empty.trusted_people
    );
    println!(
        "TASK0821 empty.connected_app_count={} empty.apps={}",
        empty.connected_app_count, empty.apps
    );
    println!("TASK0821 empty.next_safe_step={}", empty.next_safe_step);

    assert_eq!(empty.protection_state, "needs-attention");
    assert_eq!(empty.trusted_people_count, 0);
    assert_eq!(empty.trusted_people, "0 trusted people");
    assert_eq!(empty.connected_app_count, 0);
    assert_eq!(empty.apps, "0 connected apps");
    assert_eq!(empty.next_safe_step, "Finish account protection");

    // The two states have to be genuinely different, or a screenshot that
    // differs from the empty capture would prove nothing about the data.
    assert_ne!(ready.next_safe_step, empty.next_safe_step);

    check_fixture("task-0821-home-summary-direct.json", &ready);
    check_fixture("task-0821-home-summary-empty.json", &empty);
}
