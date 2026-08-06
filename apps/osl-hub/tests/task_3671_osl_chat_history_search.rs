//! TASK 3671: OSL Chats history search returns stable results and opens one at
//! its exact conversation position.

#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, open_osl_chat_history_result, search_osl_chat_history,
    HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_friend_account_reach_choice,
    set_manual_peer_scope_permission, set_scope_security, verify_friend_safety_number,
    HubSecurityState,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3671-history-search-password";
const EXACT_QUERY: &str = "TASK_3671_EXACT_SEARCH_MESSAGE_73";

struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "task-3671-osl-chat-history-search-{}-{}",
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
fn exact_search_opens_message_73_with_72_and_74_beside_it() {
    let storage = LocalStorage::new();
    let alice_identity = keystore::generate_identity("task-3671-alice".to_owned());
    let bob_identity = keystore::generate_identity("task-3671-bob".to_owned());

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
        Some("Alice task 3671".to_owned()),
    )
    .expect("add local friend");
    verify_friend_safety_number(
        &bob_core,
        &bob_security,
        alice_friend.person_id.clone(),
        alice_friend.safety_number,
    )
    .expect("verify local friend");
    set_friend_account_reach_choice(
        &bob_security,
        alice_friend.person_id.clone(),
        "osl-chat".to_owned(),
        "osl-main".to_owned(),
        true,
    )
    .expect("allow this friend on the first-party OSL Chat account");

    let bob_binding =
        manual_peer_binding(&bob_core, alice_friend.person_id.clone()).expect("peer binding");
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

    let history_dir = storage.root.join("history");
    fs::create_dir(&history_dir).expect("create history directory");
    let message_store =
        store::MessageStore::open(&history_dir, bob_identity.x25519_secret.as_bytes())
            .expect("open local encrypted history store");
    let history_channel_id = ipc::scope::Scope::try_from(chat.scope.clone())
        .expect("OSL Chat scope")
        .storage_key();

    for number in 1..=100 {
        let plaintext = if number == 73 {
            EXACT_QUERY.to_owned()
        } else {
            format!("TASK_3671 ordered message {number:03}")
        };
        message_store
            .put(&store::StoredMessage {
                discord_message_id: format!("task-3671-message-{number:03}"),
                channel_id: history_channel_id.clone(),
                sender_discord_id: alice_identity.user_id.clone(),
                sender_osl_user_id: alice_identity.user_id.clone(),
                plaintext,
                decrypted_at: 1_800_000_000 + i64::from(number),
                burned: false,
            })
            .expect("persist ordered message");
    }
    *bob_core
        .osl
        .message_store
        .lock()
        .expect("message store lock") = Some(message_store);

    let results = search_osl_chat_history(&bob_core, &bob_broker, EXACT_QUERY.to_owned())
        .expect("search OSL Chat history");
    assert_eq!(results.len(), 1, "exact search returns message 73 once");
    assert_eq!(results[0].position, 73, "search result position is 73");
    assert_eq!(
        results[0].message.discord_message_id, "task-3671-message-073",
        "search result is the stable id for message 73"
    );

    let opened = open_osl_chat_history_result(&bob_core, &bob_broker, results[0].result_id.clone())
        .expect("open stable search result");
    let previous = opened.previous.as_ref().expect("message 72 is beside 73");
    let next = opened.next.as_ref().expect("message 74 is beside 73");

    assert_eq!(
        opened.total_messages, 100,
        "opened view counts all ordered messages"
    );
    assert_eq!(opened.position, 73, "opened position is 73");
    assert_eq!(opened.message.discord_message_id, "task-3671-message-073");
    assert_eq!(previous.discord_message_id, "task-3671-message-072");
    assert_eq!(next.discord_message_id, "task-3671-message-074");
    assert_eq!(previous.plaintext, "TASK_3671 ordered message 072");
    assert_eq!(next.plaintext, "TASK_3671 ordered message 074");

    println!(
        "TASK_3671 ordered_messages=100 exact_query=\"{}\" exact_result_count={} result_message_id={} search_position={} opened_position={} opened_total={} previous_message_id={} previous_text=\"{}\" opened_message_id={} opened_text=\"{}\" next_message_id={} next_text=\"{}\"",
        EXACT_QUERY,
        results.len(),
        results[0].message.discord_message_id,
        results[0].position,
        opened.position,
        opened.total_messages,
        previous.discord_message_id,
        previous.plaintext,
        opened.message.discord_message_id,
        opened.message.plaintext,
        next.discord_message_id,
        next.plaintext
    );

    *bob_core
        .osl
        .message_store
        .lock()
        .expect("message store lock") = None;
}
