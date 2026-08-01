//! Proves that an already-delivered OSL Chat message is read from the local
//! encrypted store even when this process has no keyserver client.

#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, load_osl_chat_history, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "offline-history-fixture-password";

struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-chat-offline-history-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated storage root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("unlock isolated storage");
        Self { root }
    }
}

impl Drop for LocalStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn already_delivered_osl_chat_history_reads_with_no_keyserver_client() {
    let _storage = LocalStorage::new();
    let alice_identity = keystore::generate_identity("osl-offline-history-alice".to_owned());
    let bob_identity = keystore::generate_identity("osl-offline-history-bob".to_owned());

    let alice_core = HubCoreState::default();
    *alice_core.osl.identity.lock().expect("identity lock") = Some(alice_identity.clone());
    let bob_core = HubCoreState::default();
    *bob_core.osl.identity.lock().expect("identity lock") = Some(bob_identity.clone());
    let bob_security = HubSecurityState::default();
    let bob_broker = HubBrokerState::default();

    let alice_code = export_friend_code(&alice_core).expect("export local friend code");
    let alice_friend = add_friend_code(
        &bob_core,
        &bob_security,
        alice_code.friend_code,
        Some("Alice offline fixture".to_owned()),
    )
    .expect("add local friend");
    verify_friend_safety_number(
        &bob_core,
        &bob_security,
        alice_friend.person_id.clone(),
        alice_friend.safety_number,
    )
    .expect("verify local friend");

    let bob_binding = manual_peer_binding(&bob_core, alice_friend.person_id).expect("peer binding");
    let chat = activate_owned_osl_chat_context(&bob_broker, &bob_identity.user_id, bob_binding)
        .expect("activate OSL Chat");
    set_manual_peer_scope_permission(
        &bob_core,
        &bob_security,
        "osl-chat",
        "osl-main",
        chat.person_id,
        chat.scope.clone(),
        true,
    )
    .expect("approve OSL Chat peer scope");
    set_scope_security(&bob_security, chat.scope.clone(), 3600, true)
        .expect("enable local decrypted display");

    let history_dir = _storage.root.join("history");
    fs::create_dir(&history_dir).expect("create history directory");
    let message_store =
        store::MessageStore::open(&history_dir, bob_identity.x25519_secret.as_bytes())
            .expect("open local encrypted history store");
    *bob_core
        .osl
        .message_store
        .lock()
        .expect("message store lock") = Some(message_store);
    ipc::commands::cmd_osl_persist_inbound(
        &bob_core.osl,
        chat.scope.channel_id.expect("OSL Chat channel"),
        "peer-offline-history-0000000000000001".to_owned(),
        alice_identity.user_id.clone(),
        "already delivered while online".to_owned(),
    )
    .expect("persist delivered message locally");

    // No keyserver client is configured. A history load must therefore remain
    // entirely local; a network fallback would fail this test rather than pass.
    *bob_core.osl.keyserver.lock().expect("keyserver lock") = None;
    let history = load_osl_chat_history(&bob_core, &bob_broker)
        .expect("read local history while network is unavailable");

    assert!(history.len() == 1, "one locally stored message is readable");
    assert!(
        history[0].plaintext == "already delivered while online",
        "the durable local row is returned"
    );
    assert!(
        history[0].sender_osl_user_id == alice_identity.user_id,
        "the durable row retains its authenticated sender"
    );
    *bob_core
        .osl
        .message_store
        .lock()
        .expect("message store lock") = None;
}
