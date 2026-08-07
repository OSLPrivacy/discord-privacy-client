#![cfg(feature = "core")]

use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::services::{
    read_shared_conversation_places, save_messaging_risk_agreement, ConversationPlaceCandidate,
    ConversationPlaceKind, ConversationPlaceParent, ServiceRegistryState,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3002-shared-conversation-list-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3002-shared-conversation-list-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3002 root");
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

fn activate(dir: &Path) {
    keystore::set_active_account_dir(Some(dir.to_owned()));
}

fn test_service_places() -> Vec<ConversationPlaceCandidate> {
    let server = ConversationPlaceParent::new("srv-task-3002", "Task 3002 Server");
    let channel = ConversationPlaceParent::new("chan-task-3002", "Task 3002 Channel");
    vec![
        ConversationPlaceCandidate::direct_message("dm-task-3002-alice", "Alice DM"),
        ConversationPlaceCandidate::direct_message("dm-task-3002-bob", "Bob DM"),
        ConversationPlaceCandidate::group("group-task-3002", "Task 3002 Group"),
        ConversationPlaceCandidate::channel(
            channel.id.clone(),
            channel.label.clone(),
            server.clone(),
        ),
        ConversationPlaceCandidate::thread(
            "thread-task-3002",
            "Task 3002 Thread",
            Some(server),
            channel,
        ),
    ]
}

#[test]
fn task_3002_shared_conversation_reader_returns_approved_places_only() {
    let storage = AccountStorage::new();
    let owner_dir = storage.account_dir("owner");
    activate(&owner_dir);

    let identity = keystore::generate_identity("task-3002-owner".to_owned());
    let owner = identity.user_id.clone();
    let registry = ServiceRegistryState::load(owner_dir.join("service-registry.json"));
    let approved_account = registry
        .create_for_owner(&owner, ServiceKind::Discord, "Discord approved".to_owned())
        .expect("create approved Discord account");
    let never_approved_account = registry
        .create_for_owner(
            &owner,
            ServiceKind::Discord,
            "Discord never approved".to_owned(),
        )
        .expect("create never-approved Discord account");

    save_messaging_risk_agreement(&owner, "discord", &approved_account.id)
        .expect("approve first Discord account");

    let approved_places = read_shared_conversation_places(
        &owner,
        "discord",
        &approved_account.id,
        test_service_places(),
    )
    .expect("read approved shared conversation places");
    let never_approved_places = read_shared_conversation_places(
        &owner,
        "discord",
        &never_approved_account.id,
        test_service_places(),
    )
    .expect("read never-approved shared conversation places");

    let kinds = approved_places
        .iter()
        .map(|place| place.place_kind.as_str())
        .collect::<BTreeSet<_>>();
    let direct_messages = approved_places
        .iter()
        .filter(|place| place.place_kind == ConversationPlaceKind::DirectMessage)
        .count();
    let groups = approved_places
        .iter()
        .filter(|place| place.place_kind == ConversationPlaceKind::Group)
        .count();
    let channels = approved_places
        .iter()
        .filter(|place| place.place_kind == ConversationPlaceKind::Channel)
        .count();
    let threads = approved_places
        .iter()
        .filter(|place| place.place_kind == ConversationPlaceKind::Thread)
        .count();
    let servers = approved_places
        .iter()
        .filter_map(|place| place.server.as_ref().map(|server| server.id.as_str()))
        .collect::<BTreeSet<_>>();

    println!("TASK3002_DIRECT_READER=read_shared_conversation_places");
    println!("TASK3002_APPROVED_ACCOUNT={}", approved_account.id);
    println!(
        "TASK3002_NEVER_APPROVED_ACCOUNT={}",
        never_approved_account.id
    );
    println!("TASK3002_APPROVED_PLACE_COUNT={}", approved_places.len());
    println!("TASK3002_APPROVED_PLACE_KIND_COUNT={}", kinds.len());
    println!("TASK3002_RETURNED_DIRECT_MESSAGES={direct_messages}");
    println!("TASK3002_RETURNED_GROUPS={groups}");
    println!("TASK3002_RETURNED_SERVERS={}", servers.len());
    println!("TASK3002_RETURNED_CHANNELS={channels}");
    println!("TASK3002_RETURNED_THREADS={threads}");
    println!(
        "TASK3002_NEVER_APPROVED_PLACE_COUNT={}",
        never_approved_places.len()
    );
    for place in &approved_places {
        println!(
            "TASK3002_PLACE id={} kind={} service={} account={} server={} channel={}",
            place.place_id,
            place.place_kind.as_str(),
            place.service_id,
            place.account_id,
            place
                .server
                .as_ref()
                .map(|server| server.id.as_str())
                .unwrap_or("none"),
            place
                .channel
                .as_ref()
                .map(|channel| channel.id.as_str())
                .unwrap_or("none"),
        );
    }

    assert_eq!(approved_places.len(), 5);
    assert_eq!(kinds.len(), 4);
    assert_eq!(direct_messages, 2);
    assert_eq!(groups, 1);
    assert_eq!(servers.len(), 1);
    assert_eq!(channels, 1);
    assert_eq!(threads, 1);
    assert!(approved_places
        .iter()
        .all(|place| place.service_id == "discord" && place.account_id == approved_account.id));
    assert!(never_approved_places.is_empty());
}
