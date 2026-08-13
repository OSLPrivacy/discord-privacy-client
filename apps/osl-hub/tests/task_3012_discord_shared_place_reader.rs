#![cfg(feature = "core")]

use osl_privacy_hub::services::{
    read_discord_shared_places, save_messaging_risk_agreement, ConversationPlaceCandidate,
    ConversationPlaceKind, ConversationPlaceParent, DiscordReleaseChannel, DiscordReleaseStore,
    DiscordReleaseStores,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-3012-discord-shared-place-reader-password";

struct AccountStorage {
    root: PathBuf,
}

impl AccountStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3012-discord-reader-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time after unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated task 3012 root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { root }
    }

    fn account_dir(&self) -> PathBuf {
        let dir = self.root.join("owner");
        fs::create_dir(&dir).expect("create isolated owner directory");
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

fn discord_release_stores() -> DiscordReleaseStores {
    let server = ConversationPlaceParent::new("discord-server-scrub-d", "SCRUB-D server");
    let channel = ConversationPlaceParent::new("discord-channel-scrub-d", "SCRUB-D channel");
    DiscordReleaseStores::new(
        DiscordReleaseStore::official(
            DiscordReleaseChannel::Stable,
            "discord-stable-store-3012",
            [ConversationPlaceCandidate::direct_message(
                "discord-dm-scrub-d",
                "SCRUB-D",
            )],
        ),
        DiscordReleaseStore::official(
            DiscordReleaseChannel::Ptb,
            "discord-ptb-store-3012",
            [ConversationPlaceCandidate::group(
                "discord-group-scrub-d",
                "SCRUB-D group",
            )],
        ),
        DiscordReleaseStore::official(
            DiscordReleaseChannel::Canary,
            "discord-canary-store-3012",
            [
                ConversationPlaceCandidate::channel(
                    channel.id.clone(),
                    channel.label.clone(),
                    server.clone(),
                ),
                ConversationPlaceCandidate::thread(
                    "discord-thread-scrub-d",
                    "SCRUB-D thread",
                    Some(server),
                    channel,
                ),
            ],
        ),
    )
    .expect("three independent official Discord release stores")
}

#[test]
fn task_3012_discord_release_stores_return_seeded_places_only_for_ticked_account() {
    let storage = AccountStorage::new();
    let owner_dir = storage.account_dir();
    keystore::set_active_account_dir(Some(owner_dir));

    let owner = keystore::generate_identity("task-3012-owner".to_owned())
        .user_id
        .clone();
    let approved_account = "discord-approved-3012";
    let unticked_account = "discord-unticked-3012";
    save_messaging_risk_agreement(&owner, "discord", approved_account)
        .expect("tick the approved Discord account for Scrub");

    let stores = discord_release_stores();
    let approved_places = read_discord_shared_places(&owner, approved_account, &stores)
        .expect("read the three official Discord release stores");
    let unticked_places = read_discord_shared_places(&owner, unticked_account, &stores)
        .expect("unticked account must be silent");

    println!("TASK3012_READER=read_discord_shared_places");
    println!("TASK3012_RELEASE_STORE_COUNT={}", stores.stores().len());
    for store in stores.stores() {
        println!(
            "TASK3012_RELEASE channel={} store={} executable={} arguments={:?}",
            store.channel.as_str(),
            store.store_id,
            store.launch.executable_name,
            store.launch.arguments,
        );
    }
    println!("TASK3012_APPROVED_ACCOUNT={approved_account}");
    println!("TASK3012_APPROVED_PLACE_COUNT={}", approved_places.len());
    for place in &approved_places {
        println!(
            "TASK3012_PLACE id={} name={} kind={} account={} server={} channel={}",
            place.place_id,
            place.label,
            place.place_kind.as_str(),
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
    println!("TASK3012_UNTICKED_ACCOUNT={unticked_account}");
    println!("TASK3012_UNTICKED_PLACE_COUNT={}", unticked_places.len());

    assert_eq!(approved_places.len(), 4);
    assert_eq!(
        approved_places
            .iter()
            .map(|place| (place.label.as_str(), place.place_kind))
            .collect::<Vec<_>>(),
        vec![
            ("SCRUB-D", ConversationPlaceKind::DirectMessage),
            ("SCRUB-D group", ConversationPlaceKind::Group),
            ("SCRUB-D channel", ConversationPlaceKind::Channel),
            ("SCRUB-D thread", ConversationPlaceKind::Thread),
        ]
    );
    assert!(approved_places
        .iter()
        .all(|place| place.account_id == approved_account));
    assert!(unticked_places.is_empty());
}

#[test]
fn task_3012_refuses_shared_release_store_and_user_data_dir_launch() {
    let stores = discord_release_stores();
    let duplicate_store = DiscordReleaseStores::new(
        stores.stable.clone(),
        DiscordReleaseStore::official(
            DiscordReleaseChannel::Ptb,
            stores.stable.store_id.clone(),
            std::iter::empty(),
        ),
        stores.canary.clone(),
    )
    .expect_err("two Discord releases resolving to one store must exit 1");

    let mut user_data_dir_store = stores.ptb.clone();
    user_data_dir_store
        .launch
        .arguments
        .push("--user-data-dir=C:\\temp\\not-a-discord-store".to_owned());
    let user_data_dir = DiscordReleaseStores::new(
        stores.stable.clone(),
        user_data_dir_store,
        stores.canary.clone(),
    )
    .expect_err("a Discord --user-data-dir launch must exit 1");

    println!("TASK3012_DUPLICATE_STORE_EXIT=1");
    println!("TASK3012_DUPLICATE_STORE_ERROR={duplicate_store}");
    println!("TASK3012_USER_DATA_DIR_EXIT=1");
    println!("TASK3012_USER_DATA_DIR_ERROR={user_data_dir}");
    assert!(duplicate_store.contains("must be independent"));
    assert!(user_data_dir.contains("must not use --user-data-dir"));
}
