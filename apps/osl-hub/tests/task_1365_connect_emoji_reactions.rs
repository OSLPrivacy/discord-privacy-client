#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, add_osl_chat_reaction, load_osl_chat_history,
    remove_osl_chat_reaction, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};

const ADD_ACTION: &str = "add_osl_chat_reaction";
const REMOVE_ACTION: &str = "remove_osl_chat_reaction";
const MESSAGE_ID: &str = "peer-13650000000000000000000000000000";
const EMOJI: &str = "\u{1f44d}";
const TEST_MAIN_PASSWORD: &str = "task-1365-main-password";

struct IsolatedAccount {
    _dir: tempfile::TempDir,
}

impl IsolatedAccount {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temporary account directory");
        keystore::set_base_dir_override(Some(dir.path().to_owned()));
        keystore::set_active_account_dir(Some(dir.path().to_owned()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(dir.path(), TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { _dir: dir }
    }
}

impl Drop for IsolatedAccount {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn assert_shipping_actions_registered_and_granted() {
    let main = include_str!("../src/main.rs");
    let handlers = include_str!("../src/hub_command_surface.rs");
    let permissions = include_str!("../permissions/hub.toml");
    let capability = include_str!("../capabilities/hub.json");

    for (action, permission) in [
        (ADD_ACTION, "allow-add-osl-chat-reaction"),
        (REMOVE_ACTION, "allow-remove-osl-chat-reaction"),
    ] {
        assert!(main.contains(&format!("fn {action}(")));
        assert!(handlers.contains(action));
        assert!(permissions.contains(&format!("commands.allow = [\"{action}\"]")));
        assert!(capability.contains(&format!("\"{permission}\"")));
    }
}

fn activate_verified_chat_with_history_row(
    core: &HubCoreState,
    security: &HubSecurityState,
    broker: &HubBrokerState,
    owner: &keystore::Identity,
    peer: &keystore::Identity,
    account: &IsolatedAccount,
) {
    *core.osl.identity.lock().expect("identity lock") = Some(owner.clone());
    let peer_core = HubCoreState::default();
    *peer_core.osl.identity.lock().expect("peer identity lock") = Some(peer.clone());

    let peer_code = export_friend_code(&peer_core).expect("export peer friend code");
    let friend = add_friend_code(
        core,
        security,
        peer_code.friend_code,
        Some("Task 1365 peer".to_owned()),
    )
    .expect("add peer friend code");
    verify_friend_safety_number(
        core,
        security,
        friend.person_id.clone(),
        friend.safety_number,
    )
    .expect("verify peer safety number");

    let binding = manual_peer_binding(core, friend.person_id).expect("manual peer binding");
    let chat =
        activate_owned_osl_chat_context(broker, &owner.user_id, binding).expect("activate chat");
    set_manual_peer_scope_permission(
        core,
        security,
        "osl-chat",
        "osl-main",
        chat.person_id,
        chat.scope.clone(),
        true,
    )
    .expect("approve chat scope");
    set_scope_security(security, chat.scope.clone(), 3600, true).expect("enable display");

    let history_dir = account._dir.path().join("history");
    std::fs::create_dir(&history_dir).expect("create history directory");
    let store = store::MessageStore::open(&history_dir, owner.x25519_secret.as_bytes())
        .expect("open history store");
    *core.osl.message_store.lock().expect("message store lock") = Some(store);
    let channel_id = ipc::scope::Scope::try_from(chat.scope)
        .expect("scope")
        .storage_key();
    ipc::commands::cmd_osl_persist_inbound(
        &core.osl,
        channel_id,
        MESSAGE_ID.to_owned(),
        peer.user_id.clone(),
        "message row for reaction".to_owned(),
    )
    .expect("persist history row");
}

#[test]
fn direct_actions_add_then_remove_same_reaction_from_message_row() {
    assert_shipping_actions_registered_and_granted();
    let account = IsolatedAccount::new();
    let core = HubCoreState::default();
    let security = HubSecurityState::default();
    let broker = HubBrokerState::default();
    let owner = keystore::generate_identity("task-1365-owner".to_owned());
    let peer = keystore::generate_identity("task-1365-peer".to_owned());
    activate_verified_chat_with_history_row(&core, &security, &broker, &owner, &peer, &account);

    let before = load_osl_chat_history(&core, &broker).expect("load history before reaction");
    let add = add_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), EMOJI.to_owned())
        .expect("direct add action");
    let after_add = load_osl_chat_history(&core, &broker).expect("load history after add");
    let add_row = after_add
        .iter()
        .find(|row| row.discord_message_id == MESSAGE_ID)
        .expect("message row after add");
    let added_reaction = add_row
        .reactions
        .iter()
        .find(|reaction| reaction.emoji == EMOJI)
        .expect("row reaction after add");

    let remove = remove_osl_chat_reaction(&core, &broker, MESSAGE_ID.to_owned(), EMOJI.to_owned())
        .expect("direct remove action");
    let after_remove = load_osl_chat_history(&core, &broker).expect("load history after remove");
    let remove_row = after_remove
        .iter()
        .find(|row| row.discord_message_id == MESSAGE_ID)
        .expect("message row after remove");

    println!("TASK1365_ADD_ACTION={ADD_ACTION}");
    println!("TASK1365_REMOVE_ACTION={REMOVE_ACTION}");
    println!("TASK1365_MESSAGE_ROW_ID={}", add_row.discord_message_id);
    println!("TASK1365_EMOJI={}", add.emoji);
    println!(
        "TASK1365_BEFORE_ROW_REACTIONS={}",
        before[0].reactions.len()
    );
    println!("TASK1365_ADD_ADDED={}", add.added);
    println!("TASK1365_ADD_REMOVED={}", add.removed);
    println!(
        "TASK1365_AFTER_ADD_ROW_REACTIONS={}",
        add_row.reactions.len()
    );
    println!("TASK1365_AFTER_ADD_EMOJI_COUNT={}", added_reaction.count);
    println!("TASK1365_AFTER_ADD_MINE={}", added_reaction.mine);
    println!("TASK1365_REMOVE_ADDED={}", remove.added);
    println!("TASK1365_REMOVE_REMOVED={}", remove.removed);
    println!(
        "TASK1365_AFTER_REMOVE_ROW_REACTIONS={}",
        remove_row.reactions.len()
    );
    println!("TASK1365_REMOVE_REACTION_COUNT={}", remove.reaction_count);

    assert_eq!(before.len(), 1);
    assert_eq!(before[0].reactions.len(), 0);
    assert_eq!(add.message_id, MESSAGE_ID);
    assert_eq!(add.emoji, EMOJI);
    assert_eq!(add.identity_osl_user_id, owner.user_id);
    assert!(add.added);
    assert!(!add.removed);
    assert_eq!(add.reaction_count, 1);
    assert_eq!(add_row.reactions.len(), 1);
    assert_eq!(added_reaction.count, 1);
    assert!(added_reaction.mine);
    assert_eq!(remove.message_id, MESSAGE_ID);
    assert_eq!(remove.emoji, EMOJI);
    assert!(!remove.added);
    assert!(remove.removed);
    assert_eq!(remove.reaction_count, 0);
    assert_eq!(remove_row.reactions.len(), 0);
}
