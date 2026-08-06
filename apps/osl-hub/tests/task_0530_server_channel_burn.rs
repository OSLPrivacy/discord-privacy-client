//! TASK 0530: a server-channel burn deletes only the chosen channel.

#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{burn_scope, HubSecurityState};
use rand::{distributions::Alphanumeric, Rng};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use store::{MessageStore, StoredMessage};

const TEST_MAIN_PASSWORD: &str = "task-0530-server-channel-burn";
const SERVER_ID: &str = "task-0530-server";
const CHOSEN_CHANNEL: &str = "task-0530-chosen-channel";
const OTHER_CHANNEL: &str = "task-0530-other-channel";

struct IsolatedStorage {
    root: PathBuf,
}

impl IsolatedStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-0530-{}-{}",
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

    fn message_store(&self) -> MessageStore {
        MessageStore::open(&self.root.join("history"), &[0x53; 32])
            .expect("open task 0530 message store")
    }
}

impl Drop for IsolatedStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn random_mark(label: &str) -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();
    format!("TASK0530-{label}-{suffix}")
}

fn put(store: &MessageStore, message_id: &str, channel_id: &str, mark: &str, at: i64) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: channel_id.to_owned(),
            sender_discord_id: "task-0530-sender".to_owned(),
            sender_osl_user_id: "task-0530-sender".to_owned(),
            plaintext: mark.to_owned(),
            decrypted_at: at,
            burned: false,
        })
        .expect("seed marked message");
}

fn read_marks(store: &MessageStore, channel_id: &str) -> Vec<String> {
    let mut marks: Vec<String> = store
        .list_by_channel(channel_id, 10)
        .expect("read channel")
        .into_iter()
        .map(|message| message.plaintext)
        .collect();
    marks.sort();
    marks
}

#[test]
fn task_0530_server_channel_burn_removes_only_the_chosen_channel() {
    let storage = IsolatedStorage::new();
    let core = HubCoreState::default();
    let security = HubSecurityState::default();
    let store = storage.message_store();

    let chosen_mark = random_mark("chosen");
    let other_mark_a = random_mark("other-a");
    let other_mark_b = random_mark("other-b");
    put(
        &store,
        "task-0530-chosen-1",
        CHOSEN_CHANNEL,
        &chosen_mark,
        1,
    );
    put(&store, "task-0530-other-1", OTHER_CHANNEL, &other_mark_a, 2);
    put(&store, "task-0530-other-2", OTHER_CHANNEL, &other_mark_b, 3);

    let before_chosen = read_marks(&store, CHOSEN_CHANNEL);
    let before_other = read_marks(&store, OTHER_CHANNEL);
    println!(
        "TASK0530_BEFORE chosen_count={} other_count={} chosen_marks={:?} other_marks={:?}",
        before_chosen.len(),
        before_other.len(),
        before_chosen,
        before_other
    );
    assert_eq!(before_chosen, vec![chosen_mark.clone()]);
    assert_eq!(before_other, {
        let mut expected = vec![other_mark_a.clone(), other_mark_b.clone()];
        expected.sort();
        expected
    });

    *core.osl.message_store.lock().expect("message store lock") = Some(store);

    let chosen_scope = ipc::scope::Scope::server_channel(SERVER_ID, CHOSEN_CHANNEL);
    let burn = burn_scope(
        &core,
        &security,
        ipc::scope::ScopeInput::from(&chosen_scope),
        Vec::new(),
        true,
        vec!["task-0530-chosen-1".to_owned()],
    )
    .expect("burn chosen server channel once");
    println!(
        "TASK0530_BURN rows_destroyed={} channels_destroyed={} storage_key={}",
        burn.rows_destroyed, burn.channels_destroyed, burn.storage_key
    );
    assert_eq!(burn.rows_destroyed, 1);
    assert_eq!(burn.channels_destroyed, 1);
    assert_eq!(burn.storage_key, chosen_scope.storage_key());

    let store_guard = core.osl.message_store.lock().expect("message store lock");
    let store = store_guard.as_ref().expect("message store still installed");
    let after_chosen = read_marks(store, CHOSEN_CHANNEL);
    let after_other = read_marks(store, OTHER_CHANNEL);
    let chosen_mark_readable_after = after_chosen
        .iter()
        .chain(after_other.iter())
        .any(|mark| mark == &chosen_mark);
    let chosen_absent = !chosen_mark_readable_after;
    let other_a_readable = after_other.iter().any(|mark| mark == &other_mark_a);
    let other_b_readable = after_other.iter().any(|mark| mark == &other_mark_b);
    println!(
        "TASK0530_AFTER chosen_count={} other_count={} chosen_mark_absent={} chosen_mark_readable_after={} other_a_readable={} other_b_readable={} chosen_mark={} other_mark_a={} other_mark_b={} chosen_marks={:?} other_marks={:?}",
        after_chosen.len(),
        after_other.len(),
        chosen_absent,
        chosen_mark_readable_after,
        other_a_readable,
        other_b_readable,
        chosen_mark,
        other_mark_a,
        other_mark_b,
        after_chosen,
        after_other
    );
    assert_eq!(after_chosen.len(), 0);
    assert_eq!(after_other.len(), 2);
    assert!(chosen_absent);
    assert!(!chosen_mark_readable_after);
    assert!(other_a_readable);
    assert!(other_b_readable);
}
