use ipc::allowed_places::{add_allowed_place_record, allowed_place_summary, AllowedPlaceRecord};
use ipc::app_preferences::{load_app_preferences, UpdateChannel, APP_PREFERENCES_VERSION};
use ipc::commands::{
    cmd_osl_check_for_updates, cmd_osl_get_friend_ids, cmd_osl_set_friend_ids,
    cmd_osl_set_update_channel, UpdateCheckResult, UpdateInfo,
};
use ipc::state::AppState;
use keystore::NoOpSealer;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const IDENTITY_NAME: &str = "task-3179-UPD-KEEP";
const CHANNEL_ID: &str = "task-3179-channel";
const MESSAGE_MARKER: &str = "UPD-KEEP";
const CURRENT_VERSION: &str = "3179.0.0";
const NEXT_VERSION: &str = "3179.0.1";

struct FileStorageKeyReset;

impl Drop for FileStorageKeyReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreservationCounts {
    identities: usize,
    friends: usize,
    places: usize,
    messages: usize,
    identity_name: String,
    version: String,
}

fn counts(
    state: &AppState,
    dir: &std::path::Path,
    sealer: &dyn keystore::Sealer,
    message_ids: &[String],
    version: impl Into<String>,
) -> PreservationCounts {
    let loaded_identity = keystore::load_identity(&dir.join("identity.json"), sealer).ok();
    let identities = usize::from(loaded_identity.is_some());
    let identity_name = loaded_identity
        .as_ref()
        .map(|identity| identity.user_id.clone())
        .unwrap_or_default();
    let friends = cmd_osl_get_friend_ids(state).expect("friend ids").len();
    let places = allowed_place_summary(dir).expect("allowed places").places;
    let messages = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
        .expect("message store open")
        .count_message_records(message_ids)
        .expect("message records");

    PreservationCounts {
        identities,
        friends,
        places,
        messages,
        identity_name,
        version: version.into(),
    }
}

#[test]
fn task3179_good_update_keeps_identity_friends_places_and_upd_keep_history() {
    let _file_key_reset = FileStorageKeyReset;
    ipc::main_password::set_file_storage_key(Some([0x79; 32]));

    let dir = TempDir::new().expect("tempdir");
    let state = AppState::new();
    let identity = keystore::generate_identity(IDENTITY_NAME.to_owned());
    let secret = *identity.x25519_secret.as_bytes();
    let sealer = NoOpSealer::new();
    state.install_identity(identity.clone());
    keystore::save_identity(&dir.path().join("identity.json"), &identity, &sealer)
        .expect("persist identity");

    cmd_osl_set_friend_ids(
        &state,
        vec![
            "task-3179-friend-1".to_owned(),
            "task-3179-friend-2".to_owned(),
            "task-3179-friend-3".to_owned(),
        ],
    )
    .expect("set friends");

    add_allowed_place_record(
        dir.path(),
        AllowedPlaceRecord::discord_direct_message(IDENTITY_NAME, "task-3179-place-1"),
    )
    .expect("place 1");
    add_allowed_place_record(
        dir.path(),
        AllowedPlaceRecord::discord_direct_message(IDENTITY_NAME, "task-3179-place-2"),
    )
    .expect("place 2");

    let store = MessageStore::open(&dir.path().join("store"), &secret).expect("open store");
    let message_ids: Vec<String> = (0..10)
        .map(|index| format!("task-3179-message-{index}"))
        .collect();
    for (index, message_id) in message_ids.iter().enumerate() {
        store
            .put(&StoredMessage {
                discord_message_id: message_id.clone(),
                channel_id: CHANNEL_ID.to_owned(),
                sender_discord_id: format!("task-3179-friend-{}", (index % 3) + 1),
                sender_osl_user_id: format!("task-3179-friend-{}", (index % 3) + 1),
                plaintext: format!("{MESSAGE_MARKER} message {index}"),
                decrypted_at: 3179 + index as i64,
                burned: false,
                reply_parent_id: None,
                edit_revision: 1,
            })
            .expect("put UPD-KEEP message");
    }
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") = Some(store);

    let before = counts(&state, dir.path(), &sealer, &message_ids, CURRENT_VERSION);
    let prefs_version_before = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .version;
    assert_eq!(
        keystore::load_identity(&dir.path().join("identity.json"), &sealer)
            .expect("load persisted identity")
            .user_id,
        IDENTITY_NAME
    );
    assert_eq!(before.identities, 1);
    assert_eq!(before.friends, 3);
    assert_eq!(before.places, 2);
    assert_eq!(before.messages, 10);
    assert_eq!(before.identity_name, IDENTITY_NAME);

    let update = cmd_osl_check_for_updates(
        CURRENT_VERSION.to_owned(),
        Ok(Some(UpdateInfo {
            version: NEXT_VERSION.to_owned(),
            notes: Some("task 3179 good update".to_owned()),
            url: "https://updates.example.test/task-3179".to_owned(),
        })),
    );
    assert_eq!(
        update,
        UpdateCheckResult::UpdateAvailable {
            current: CURRENT_VERSION.to_owned(),
            next: NEXT_VERSION.to_owned(),
            notes: "task 3179 good update".to_owned(),
            url: "https://updates.example.test/task-3179".to_owned(),
        }
    );
    cmd_osl_set_update_channel(&state, UpdateChannel::Beta, Some(dir.path().to_path_buf()))
        .expect("good update-channel write");
    let persisted_preferences = load_app_preferences(&dir.path().join("app_preferences.json"));
    assert_eq!(persisted_preferences.update_channel, UpdateChannel::Beta);
    assert_eq!(persisted_preferences.version, APP_PREFERENCES_VERSION);

    let after = counts(&state, dir.path(), &sealer, &message_ids, NEXT_VERSION);
    let prefs_version_after = persisted_preferences.version;
    let history = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
        .expect("message store open")
        .list_by_channel(CHANNEL_ID, 10)
        .expect("history");
    assert_eq!(history.len(), 10);
    assert!(
        history
            .iter()
            .all(|message| message.plaintext.contains(MESSAGE_MARKER)),
        "all retained messages must be marked {MESSAGE_MARKER}"
    );
    assert_eq!(after.identities, before.identities);
    assert_eq!(after.friends, before.friends);
    assert_eq!(after.places, before.places);
    assert_eq!(after.messages, before.messages);
    assert_eq!(after.identity_name, IDENTITY_NAME);
    assert_ne!(after.version, before.version);
    assert_ne!(prefs_version_after, prefs_version_before);

    println!(
        "TASK3179 before identities={} friends={} places={} messages={} identity_name={} version={} prefs_version={}",
        before.identities,
        before.friends,
        before.places,
        before.messages,
        before.identity_name,
        before.version,
        prefs_version_before
    );
    println!(
        "TASK3179 after identities={} friends={} places={} messages={} identity_name={} version={} prefs_version={}",
        after.identities,
        after.friends,
        after.places,
        after.messages,
        after.identity_name,
        after.version,
        prefs_version_after
    );
    println!(
        "TASK3179 marker={MESSAGE_MARKER} retained_messages={}",
        history.len()
    );
}
