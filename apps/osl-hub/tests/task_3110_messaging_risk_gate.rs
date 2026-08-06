#![cfg(feature = "core")]

#[path = "native_discord_receive_e2e.rs"]
mod native_discord_receive_e2e;

use osl_privacy_hub::broker::{
    activate_owned_manual_peer_context,
    prepare_active_messaging_service_overlay_text_with_route_clients, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use osl_privacy_hub::service_host::ServiceHostState;
use osl_privacy_hub::services::{save_messaging_risk_agreement, ServiceRegistryState};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3110-risk-gate-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3110-risk-gate-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3110 root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { root }
    }

    fn account_dir(&self, label: &str) -> PathBuf {
        let dir = self.root.join(label);
        fs::create_dir(&dir).expect("create isolated account dir");
        dir
    }
}

impl Drop for AccountStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn core(identity: keystore::Identity) -> HubCoreState {
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity);
    core
}

fn owner_profile_namespace(owner_osl_user_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-HUB/service-profile-owner/v1");
    hash.update(owner_osl_user_id.as_bytes());
    let digest = hash.finalize();
    let mut namespace = String::from("owner-");
    for byte in &digest[..24] {
        namespace.push_str(&format!("{byte:02x}"));
    }
    namespace
}

fn activate(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
}

fn task_3110_telegram_send_refused_after_discord_agreement() {
    let storage = AccountStorage::new();
    let alice_dir = storage.account_dir("alice");
    let bob_dir = storage.account_dir("bob");

    let alice_identity = keystore::generate_identity("task-3110-alice".to_owned());
    let alice_id = alice_identity.user_id.clone();
    let bob_identity = keystore::generate_identity("task-3110-bob".to_owned());
    let alice = core(alice_identity);
    let bob = core(bob_identity);
    let alice_security = HubSecurityState::default();
    let broker = HubBrokerState::default();
    let host = ServiceHostState::default();

    activate(&bob_dir);
    let bob_code = export_friend_code(&bob)
        .expect("export Bob friend code")
        .friend_code;

    activate(&alice_dir);
    let friend = add_friend_code(
        &alice,
        &alice_security,
        bob_code,
        Some("task 3110 Telegram peer".to_owned()),
    )
    .expect("add Telegram peer");
    verify_friend_safety_number(
        &alice,
        &alice_security,
        friend.person_id.clone(),
        friend.safety_number.clone(),
    )
    .expect("verify Telegram peer");

    let registry = ServiceRegistryState::load(alice_dir.join("service-registry.json"));
    let account = registry
        .create_for_owner(&alice_id, ServiceKind::Telegram, "Telegram".to_owned())
        .expect("create Telegram service account");
    let owner_namespace = owner_profile_namespace(&alice_id);
    let _active = host
        .begin_open(
            &owner_namespace,
            "telegram",
            &account.id,
            "web.telegram.org",
        )
        .expect("open Telegram service host");
    let binding = manual_peer_binding(&alice, friend.person_id.clone())
        .expect("build Telegram manual peer binding");
    let activated = activate_owned_manual_peer_context(
        &broker,
        &registry,
        &host,
        &alice_id,
        "telegram",
        &account.id,
        binding,
    )
    .expect("activate Telegram manual peer context");
    set_manual_peer_scope_permission(
        &alice,
        &alice_security,
        "telegram",
        &account.id,
        activated.person_id,
        activated.scope.clone(),
        true,
    )
    .expect("approve Telegram manual peer scope");
    set_scope_security(&alice_security, activated.scope, 3600, true)
        .expect("enable Telegram decrypted display");

    save_messaging_risk_agreement(&alice_id, "discord").expect("save only Discord risk agreement");
    let store_client = ipc::cipher_store_client::CipherStoreClient::new("http://127.0.0.1:9")
        .expect("construct dummy cipher-store client");
    let telegram_refusal = match prepare_active_messaging_service_overlay_text_with_route_clients(
        &alice,
        &alice_security,
        &broker,
        &osl_privacy_hub::ai_carrier::AiCarrierState::default(),
        "task 3110 Telegram protected send".to_owned(),
        false,
        &store_client,
        None,
    ) {
        Ok(_) => panic!("Telegram send must not pass with only Discord risk agreed"),
        Err(refusal) => refusal,
    };

    println!("TASK3110_DIRECT_AGREEMENT_COMMAND=save_messaging_risk_agreement");
    println!("TASK3110_DISCORD_AGREEMENT_SCOPE=discord");
    println!("TASK3110_TELEGRAM_SEND_REFUSAL={telegram_refusal}");
    assert_eq!(telegram_refusal, "you have not agreed to the Telegram risk");
}

#[test]
fn task_3110_refuse_first_service_send_until_risk_is_agreed() {
    native_discord_receive_e2e::task_3110_discord_send_is_risk_gated();
    task_3110_telegram_send_refused_after_discord_agreement();
}
