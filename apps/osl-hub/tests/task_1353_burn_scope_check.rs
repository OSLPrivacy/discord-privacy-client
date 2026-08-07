//! TASK 1353: selected burn scopes remove only selected local history.

#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{burn_scope, HubSecurityState};
use rand::{distributions::Alphanumeric, Rng};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use store::{MessageStore, StoredMessage};

const TEST_MAIN_PASSWORD: &str = "task-1353-burn-scope-check";
const TEST_FILE_KEY: [u8; 32] = [0x35; 32];

struct IsolatedStorage {
    root: PathBuf,
}

impl IsolatedStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-1353-{}-{}",
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

    fn message_store(&self, copy: &str) -> MessageStore {
        MessageStore::open(&self.root.join(format!("history-{copy}")), &TEST_FILE_KEY)
            .expect("open task 1353 message store")
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Bucket {
    SelectedThread,
    SiblingThread,
    SelectedChannel,
    OtherChannel,
}

impl Bucket {
    fn label(self) -> &'static str {
        match self {
            Bucket::SelectedThread => "selected_thread",
            Bucket::SiblingThread => "sibling_thread",
            Bucket::SelectedChannel => "selected_channel",
            Bucket::OtherChannel => "other_channel",
        }
    }
}

#[derive(Clone, Debug)]
struct MarkedMessage {
    message_id: String,
    mark: String,
}

fn random_suffix() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

fn put(store: &MessageStore, message_id: String, channel_id: &str, mark: &str, at: i64) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id,
            channel_id: channel_id.to_owned(),
            sender_discord_id: "task-1353-sender".to_owned(),
            sender_osl_user_id: "task-1353-sender".to_owned(),
            plaintext: mark.to_owned(),
            decrypted_at: at,
            burned: false,
        })
        .expect("seed marked message");
}

fn read_marks(store: &MessageStore, channel_id: &str) -> Vec<String> {
    let mut marks: Vec<String> = store
        .list_by_channel(channel_id, 20)
        .expect("read channel")
        .into_iter()
        .map(|message| message.plaintext)
        .collect();
    marks.sort();
    marks
}

fn all_counts(
    store: &MessageStore,
    channels: &BTreeMap<Bucket, String>,
) -> BTreeMap<Bucket, usize> {
    channels
        .iter()
        .map(|(bucket, channel_id)| (*bucket, read_marks(store, channel_id).len()))
        .collect()
}

fn print_counts(label: &str, copy: &str, counts: &BTreeMap<Bucket, usize>) {
    let total: usize = counts.values().sum();
    println!(
        "TASK1353_{label} copy={copy} selected_thread={} sibling_thread={} selected_channel={} other_channel={} total={}",
        counts[&Bucket::SelectedThread],
        counts[&Bucket::SiblingThread],
        counts[&Bucket::SelectedChannel],
        counts[&Bucket::OtherChannel],
        total
    );
}

fn require_exact_marks(
    store: &MessageStore,
    channels: &BTreeMap<Bucket, String>,
    expected_records: &BTreeMap<Bucket, Vec<MarkedMessage>>,
    copy: &str,
    phase: &str,
    buckets: &[Bucket],
) {
    for bucket in buckets {
        let actual = read_marks(store, &channels[bucket]);
        let expected = expected_marks(&expected_records[bucket]);
        println!(
            "TASK1353_MARKS phase={phase} copy={copy} bucket={} marks={:?}",
            bucket.label(),
            actual
        );
        assert_eq!(
            actual,
            expected,
            "exact marks not readable phase={phase} copy={copy} bucket={}",
            bucket.label()
        );
    }
}

fn fail_if_selected_still_present(
    store: &MessageStore,
    expected_records: &BTreeMap<Bucket, Vec<MarkedMessage>>,
    copy: &str,
    bucket: Bucket,
) {
    let ids: Vec<String> = expected_records[&bucket]
        .iter()
        .map(|record| record.message_id.clone())
        .collect();
    let remaining = store
        .count_message_records(&ids)
        .expect("count selected physical message records");
    if remaining != 0 {
        let marks = expected_marks(&expected_records[&bucket]);
        panic!(
            "marked selected-scope message still present: copy={copy} bucket={} remaining_records={remaining} marks={marks:?}",
            bucket.label()
        );
    }
}

fn expected_marks(records: &[MarkedMessage]) -> Vec<String> {
    let mut marks: Vec<String> = records.iter().map(|record| record.mark.clone()).collect();
    marks.sort();
    marks
}

fn seed_copy(
    storage_root: &Path,
    store: &MessageStore,
    copy: &str,
    channels: &BTreeMap<Bucket, String>,
) -> BTreeMap<Bucket, Vec<MarkedMessage>> {
    let mut expected = BTreeMap::new();
    for (bucket, channel_id) in channels {
        let mut records = Vec::new();
        for index in 0..2 {
            let message_id = format!("task-1353-{copy}-{}-{index}", bucket.label());
            let mark = format!(
                "TASK1353_SELECTED_SCOPE_MARK-{copy}-{}-{index}-{}",
                bucket.label(),
                random_suffix()
            );
            put(
                store,
                message_id.clone(),
                channel_id,
                &mark,
                1_900_000_000 + index,
            );
            records.push(MarkedMessage { message_id, mark });
        }
        records.sort_by(|left, right| left.mark.cmp(&right.mark));
        expected.insert(*bucket, records);
    }
    println!(
        "TASK1353_COPY_ROOT copy={copy} path={}",
        storage_root.display()
    );
    expected
}

fn run_copy(copy: &str, store: MessageStore, core: &HubCoreState, security: &HubSecurityState) {
    let suffix = random_suffix();
    let server_id = format!("task1353-server-{suffix}");
    let selected_thread = format!("task1353-selected-thread-{suffix}");
    let sibling_thread = format!("task1353-sibling-thread-{suffix}");
    let selected_channel = format!("task1353-selected-channel-{suffix}");
    let other_channel = format!("task1353-other-channel-{suffix}");
    let channels = BTreeMap::from([
        (Bucket::SelectedThread, selected_thread.clone()),
        (Bucket::SiblingThread, sibling_thread),
        (Bucket::SelectedChannel, selected_channel.clone()),
        (Bucket::OtherChannel, other_channel),
    ]);
    let expected_records = seed_copy(Path::new("."), &store, copy, &channels);

    require_exact_marks(
        &store,
        &channels,
        &expected_records,
        copy,
        "before_thread_burn",
        &[
            Bucket::SelectedThread,
            Bucket::SiblingThread,
            Bucket::SelectedChannel,
            Bucket::OtherChannel,
        ],
    );
    print_counts(
        "BEFORE_THREAD_BURN_COUNTS",
        copy,
        &all_counts(&store, &channels),
    );

    *core.osl.message_store.lock().expect("message store lock") = Some(store);
    let thread_scope = ipc::scope::Scope::gc(selected_thread);
    let thread_result = burn_scope(
        core,
        security,
        ipc::scope::ScopeInput::from(&thread_scope),
        Vec::new(),
        true,
        Vec::new(),
    )
    .expect("burn selected thread");
    let store_guard = core.osl.message_store.lock().expect("message store lock");
    let store = store_guard
        .as_ref()
        .expect("message store remains installed");
    let after_thread_counts = all_counts(store, &channels);
    print_counts("AFTER_THREAD_BURN_COUNTS", copy, &after_thread_counts);
    fail_if_selected_still_present(store, &expected_records, copy, Bucket::SelectedThread);
    println!(
        "TASK1353_THREAD_BURN_RESULT copy={copy} storage_key={} channels_destroyed={} rows_destroyed={}",
        thread_result.storage_key, thread_result.channels_destroyed, thread_result.rows_destroyed
    );
    assert_eq!(thread_result.channels_destroyed, 1);
    assert_eq!(thread_result.rows_destroyed, 2);
    assert_eq!(after_thread_counts[&Bucket::SelectedThread], 0);
    assert_eq!(after_thread_counts[&Bucket::SiblingThread], 2);
    assert_eq!(after_thread_counts[&Bucket::SelectedChannel], 2);
    assert_eq!(after_thread_counts[&Bucket::OtherChannel], 2);

    require_exact_marks(
        store,
        &channels,
        &expected_records,
        copy,
        "before_channel_burn",
        &[
            Bucket::SiblingThread,
            Bucket::SelectedChannel,
            Bucket::OtherChannel,
        ],
    );

    drop(store_guard);
    let channel_scope = ipc::scope::Scope::server_channel(server_id, selected_channel);
    let channel_result = burn_scope(
        core,
        security,
        ipc::scope::ScopeInput::from(&channel_scope),
        Vec::new(),
        true,
        Vec::new(),
    )
    .expect("burn selected server channel");
    let store_guard = core.osl.message_store.lock().expect("message store lock");
    let store = store_guard
        .as_ref()
        .expect("message store remains installed");
    let after_channel_counts = all_counts(store, &channels);
    print_counts("AFTER_CHANNEL_BURN_COUNTS", copy, &after_channel_counts);
    fail_if_selected_still_present(store, &expected_records, copy, Bucket::SelectedChannel);
    println!(
        "TASK1353_CHANNEL_BURN_RESULT copy={copy} storage_key={} channels_destroyed={} rows_destroyed={}",
        channel_result.storage_key, channel_result.channels_destroyed, channel_result.rows_destroyed
    );
    assert_eq!(channel_result.channels_destroyed, 1);
    assert_eq!(channel_result.rows_destroyed, 2);
    assert_eq!(after_channel_counts[&Bucket::SelectedThread], 0);
    assert_eq!(after_channel_counts[&Bucket::SiblingThread], 2);
    assert_eq!(after_channel_counts[&Bucket::SelectedChannel], 0);
    assert_eq!(after_channel_counts[&Bucket::OtherChannel], 2);
    require_exact_marks(
        store,
        &channels,
        &expected_records,
        copy,
        "after_both_burns",
        &[Bucket::SiblingThread, Bucket::OtherChannel],
    );
}

#[test]
fn task_1353_burn_scopes_remove_selected_and_preserve_other_copies() {
    let storage = IsolatedStorage::new();
    let core = HubCoreState::default();
    let security = HubSecurityState::default();

    let copy_a = storage.message_store("copy-a");
    run_copy("copy_a", copy_a, &core, &security);
    let copy_b = storage.message_store("copy-b");
    run_copy("copy_b", copy_b, &core, &security);
}
