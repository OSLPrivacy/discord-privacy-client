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

#[test]
fn task_0820_direct_home_summary_returns_all_four_facts_from_saved_state() {
    set_file_storage_key(Some([0x82; 32]));
    let _key_guard = FileStorageKeyGuard;

    let dir = tempfile::tempdir().expect("tempdir");
    let saving_state = AppState::new();
    saving_state.install_identity(keystore::generate_identity("task-0820-owner".to_string()));
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
    let (rose_id, rose) = trusted_peer("900000000000082001", "rose-task-0820");
    let (sam_id, sam) = trusted_peer("900000000000082002", "sam-task-0820");
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
        AllowedPlaceRecord::discord_direct_message("task-0820-owner", rose_id),
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
            account: "task-0820-owner".to_string(),
            kind: "direct_message".to_string(),
            stable_id: format!("telegram:task-0820-owner:direct_message:{sam_id}"),
        },
        Some(dir.path().to_path_buf()),
    )
    .expect("persist telegram app place");

    let loaded_state = AppState::new();
    loaded_state.install_identity(keystore::generate_identity("task-0820-owner".to_string()));
    *loaded_state.keyserver.lock().unwrap() =
        Some(keystore::KeyServerClient::new("http://127.0.0.1:8200").unwrap());
    loaded_state.set_cloud_registration_state(CloudRegistrationState::Registered);
    *loaded_state.app_preferences.lock().unwrap() =
        ipc::app_preferences::load_app_preferences(&dir.path().join("app_preferences.json"));
    *loaded_state.peer_map.lock().unwrap() =
        ipc::peer_map::load_peer_map_from_path(&dir.path().join("peer_map.json"))
            .expect("load persisted peer map");

    let summary = cmd_osl_home_protection_summary(&loaded_state, Some(dir.path().to_path_buf()))
        .expect("direct Home summary");

    println!(
        "TASK0820 home.protection_state={}",
        summary.protection_state
    );
    println!("TASK0820 home.privacy_level={}", summary.privacy_level);
    println!(
        "TASK0820 home.protection_choices=warnings={},cleanup={},app_exceptions={},contact_rules={},verification_warning={}",
        summary.protection_choices.warnings,
        summary.protection_choices.cleanup,
        summary.protection_choices.app_exceptions,
        summary.protection_choices.contact_rules,
        summary.verification_warning
    );
    println!(
        "TASK0820 home.trusted_people_count={}",
        summary.trusted_people_count
    );
    println!("TASK0820 home.trusted_people={}", summary.trusted_people);
    println!(
        "TASK0820 home.connected_app_count={}",
        summary.connected_app_count
    );
    println!("TASK0820 home.apps={}", summary.apps);
    println!("TASK0820 home.next_safe_step={}", summary.next_safe_step);

    assert_eq!(summary.protection_state, "protected");
    assert_eq!(summary.privacy_level, "maximum");
    assert_eq!(
        summary.protection_choices.warnings,
        "before_send_and_public_post_warnings"
    );
    assert_eq!(
        summary.protection_choices.cleanup,
        "attachment_cleaning_plus_7_day_review"
    );
    assert_eq!(
        summary.protection_choices.app_exceptions,
        "app_exceptions_restricted"
    );
    assert_eq!(
        summary.protection_choices.contact_rules,
        "protected_contacts_required"
    );
    assert_eq!(summary.verification_warning, "before sending");
    assert_eq!(summary.trusted_people_count, 2);
    assert_eq!(summary.trusted_people, "2 trusted people");
    assert_eq!(summary.connected_app_count, 2);
    assert_eq!(summary.allowed_place_count, 2);
    assert_eq!(summary.apps, "2 connected apps");
    assert_eq!(summary.next_safe_step, "Open a protected conversation");
}
