#![cfg(feature = "core")]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use message_lifecycle::AcceptedPart;
use osl_privacy_hub::message_expiry::{
    note_delivered_at_path, record_first_open, relative_release, run_pass,
};
use store::{MessageStore, StoredMessage};

const KEY: [u8; 32] = [0x32; 32];
const STORE_SECRET: [u8; 32] = [0x13; 32];
const SENT_AT: i64 = 2_100_000;
const OPENED_AT: i64 = SENT_AT + 5;
const EXPIRED_AT: i64 = OPENED_AT + 3_600;

#[derive(Clone, Copy)]
enum ExpiryGap {
    HiddenScrolled,
    Closed,
    Asleep,
}

impl ExpiryGap {
    fn label(self) -> &'static str {
        match self {
            Self::HiddenScrolled => "hidden-scrolled-out-of-sight",
            Self::Closed => "app-closed",
            Self::Asleep => "machine-asleep",
        }
    }
}

struct IsolatedAccount {
    root: PathBuf,
    backup: PathBuf,
}

impl IsolatedAccount {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "osl-task-3213-{label}-{}-{nonce}",
            std::process::id()
        ));
        let backup = std::env::temp_dir().join(format!(
            "osl-task-3213-{label}-backup-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated account root");
        keystore::set_base_dir_override(Some(root.clone()));
        keystore::set_active_account_dir(Some(root.clone()));
        ipc::main_password::set_file_storage_key(Some(KEY));
        Self { root, backup }
    }

    fn ledger_path(&self) -> PathBuf {
        self.root.join("message_open_clock.json")
    }

    fn store_dir(&self) -> PathBuf {
        self.root.join("history")
    }

    fn backup_store_dir(&self) -> PathBuf {
        self.backup.join("history")
    }
}

impl Drop for IsolatedAccount {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_dir_all(&self.backup);
    }
}

#[derive(Clone)]
struct Notice {
    message_id: String,
    preview: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Counts {
    message_row: usize,
    message_search: usize,
    notice_history: usize,
    live_store: usize,
    restored_backup: usize,
}

impl Counts {
    fn all_one(&self) -> bool {
        self.message_row == 1
            && self.message_search == 1
            && self.notice_history == 1
            && self.live_store == 1
            && self.restored_backup == 1
    }

    fn held_places(&self) -> Vec<&'static str> {
        let mut places = Vec::new();
        if self.message_row != 0 {
            places.push("message row");
        }
        if self.message_search != 0 {
            places.push("message search");
        }
        if self.notice_history != 0 {
            places.push("notice history");
        }
        if self.live_store != 0 {
            places.push("live store");
        }
        if self.restored_backup != 0 {
            places.push("restored backup");
        }
        places
    }
}

fn parts() -> Vec<AcceptedPart> {
    vec![AcceptedPart {
        index: 0,
        sealed_bytes: 512,
        digest: [0x24; 32],
    }]
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create backup directory");
    for entry in fs::read_dir(source).expect("read backup source") {
        let entry = entry.expect("read backup entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().expect("read file type").is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("copy backup file");
        }
    }
}

fn replace_tree(source: &Path, destination: &Path) {
    let _ = fs::remove_dir_all(destination);
    copy_tree(source, destination);
}

fn put_timed_message(
    account: &IsolatedAccount,
    store: &MessageStore,
    scope: &str,
    message_id: &str,
    mark: &str,
) {
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: scope.to_owned(),
            sender_discord_id: "task3213-sender".to_owned(),
            sender_osl_user_id: "task3213-osl-user".to_owned(),
            plaintext: mark.to_owned(),
            decrypted_at: SENT_AT,
            burned: false,
        })
        .expect("write timed message to encrypted store");
    note_delivered_at_path(
        &account.ledger_path(),
        &KEY,
        scope,
        message_id,
        relative_release(SENT_AT, ipc::cipher_store_client::TTL_1H).expect("one-hour open clock"),
        parts(),
        [0x45; 32],
        Some(message_id.to_owned()),
        SENT_AT,
    )
    .expect("record timed message in sealed expiry ledger");
    assert!(record_first_open(scope, message_id, [0x46; 32], OPENED_AT).is_readable());
}

fn count_message_row(store: &MessageStore, scope: &str, mark: &str) -> usize {
    store
        .list_by_channel(scope, 20)
        .expect("list message row")
        .iter()
        .filter(|message| message.plaintext == mark)
        .count()
}

fn count_message_search(store: &MessageStore, scope: &str, mark: &str) -> usize {
    store
        .list_by_channel(scope, 20)
        .expect("search message rows")
        .iter()
        .filter(|message| message.plaintext.contains(mark))
        .count()
}

fn count_live_store(store: &MessageStore, message_id: &str, mark: &str) -> usize {
    usize::from(
        store
            .get(message_id)
            .expect("read live store row")
            .is_some_and(|message| message.plaintext == mark),
    )
}

fn count_notice_history(notices: &[Notice], mark: &str) -> usize {
    notices
        .iter()
        .filter(|notice| notice.preview == mark)
        .count()
}

fn counts(
    store: &MessageStore,
    restored_backup: &MessageStore,
    notices: &[Notice],
    scope: &str,
    message_id: &str,
    mark: &str,
) -> Counts {
    Counts {
        message_row: count_message_row(store, scope, mark),
        message_search: count_message_search(store, scope, mark),
        notice_history: count_notice_history(notices, mark),
        live_store: count_live_store(store, message_id, mark),
        restored_backup: count_live_store(restored_backup, message_id, mark),
    }
}

fn print_counts(scenario: &str, phase: &str, counts: &Counts) {
    let held = counts.held_places();
    println!(
        "TASK3213 scenario={scenario} phase={phase} message_row={} message_search={} notice_history={} live_store={} restored_backup={} held={}",
        counts.message_row,
        counts.message_search,
        counts.notice_history,
        counts.live_store,
        counts.restored_backup,
        if held.is_empty() { "none".to_owned() } else { held.join(",") }
    );
}

fn run_scenario(gap: ExpiryGap) {
    let account = IsolatedAccount::new(gap.label());
    let scenario = gap.label();
    let scope = format!("dm:task3213:{scenario}");
    let message_id = format!("task3213-{scenario}-message");
    let mark = format!("TASK3213-READABLE-COPY-{scenario}");
    let mut notices = vec![Notice {
        message_id: message_id.clone(),
        preview: mark.clone(),
    }];

    let store = MessageStore::open(&account.store_dir(), &STORE_SECRET).expect("open live store");
    put_timed_message(&account, &store, &scope, &message_id, &mark);
    copy_tree(&account.root, &account.backup);
    let backup_before = MessageStore::open(&account.backup_store_dir(), &STORE_SECRET)
        .expect("open backup before expiry");
    let before = counts(&store, &backup_before, &notices, &scope, &message_id, &mark);
    print_counts(scenario, "before-expiry", &before);
    assert!(
        before.all_one(),
        "before-expiry counts must all be one: {before:?}"
    );
    drop(backup_before);

    let mut live_store = Some(store);
    println!("TASK3213 scenario={scenario} expiry_gap={}", gap.label());
    match gap {
        ExpiryGap::HiddenScrolled => {
            println!(
                "TASK3213 scenario={scenario} row_visibility_during_expiry=scrolled-out-of-sight"
            );
            let pass = run_pass(&account.root, live_store.as_ref(), EXPIRED_AT);
            println!(
                "TASK3213 scenario={scenario} live_pass ran={} expired_messages={} shredded_cache_rows={} degraded={}",
                pass.ran, pass.expired_messages, pass.shredded_cache_rows, pass.degraded
            );
            assert!(pass.ran);
            assert_eq!(pass.expired_messages, 1);
            assert_eq!(pass.shredded_cache_rows, 1);
            assert!(!pass.degraded);
        }
        ExpiryGap::Closed => {
            drop(live_store.take());
            ipc::main_password::set_file_storage_key(None);
            println!("TASK3213 scenario={scenario} app_state=closed-before-expiry");
            ipc::main_password::set_file_storage_key(Some(KEY));
            let restarted =
                MessageStore::open(&account.store_dir(), &STORE_SECRET).expect("restart store");
            let pass = run_pass(&account.root, Some(&restarted), EXPIRED_AT);
            println!(
                "TASK3213 scenario={scenario} restart_pass ran={} expired_messages={} shredded_cache_rows={} degraded={}",
                pass.ran, pass.expired_messages, pass.shredded_cache_rows, pass.degraded
            );
            assert!(pass.ran);
            assert_eq!(pass.expired_messages, 1);
            assert_eq!(pass.shredded_cache_rows, 1);
            assert!(!pass.degraded);
            drop(restarted);
        }
        ExpiryGap::Asleep => {
            println!("TASK3213 scenario={scenario} machine_state=asleep-no-sweep-until-wake");
            let pass = run_pass(&account.root, live_store.as_ref(), EXPIRED_AT);
            println!(
                "TASK3213 scenario={scenario} wake_pass ran={} expired_messages={} shredded_cache_rows={} degraded={}",
                pass.ran, pass.expired_messages, pass.shredded_cache_rows, pass.degraded
            );
            assert!(pass.ran);
            assert_eq!(pass.expired_messages, 1);
            assert_eq!(pass.shredded_cache_rows, 1);
            assert!(!pass.degraded);
        }
    }
    drop(live_store.take());

    let live_after = MessageStore::open(&account.store_dir(), &STORE_SECRET)
        .expect("open live store after expiry");
    notices.retain(|notice| {
        notice.message_id != message_id || count_live_store(&live_after, &message_id, &mark) != 0
    });
    replace_tree(&account.backup, &account.root);
    keystore::set_base_dir_override(Some(account.root.clone()));
    keystore::set_active_account_dir(Some(account.root.clone()));
    ipc::main_password::set_file_storage_key(Some(KEY));
    let restored =
        MessageStore::open(&account.store_dir(), &STORE_SECRET).expect("open restored backup");
    let restore_pass = run_pass(&account.root, Some(&restored), EXPIRED_AT);
    println!(
        "TASK3213 scenario={scenario} restored_backup_pass ran={} expired_messages={} shredded_cache_rows={} degraded={}",
        restore_pass.ran,
        restore_pass.expired_messages,
        restore_pass.shredded_cache_rows,
        restore_pass.degraded
    );
    assert!(restore_pass.ran);
    assert_eq!(restore_pass.expired_messages, 1);
    assert_eq!(restore_pass.shredded_cache_rows, 1);
    assert!(!restore_pass.degraded);

    let after = counts(&live_after, &restored, &notices, &scope, &message_id, &mark);
    print_counts(scenario, "after-expiry", &after);
    let held = after.held_places();
    assert!(
        held.is_empty(),
        "nonzero readable copy count held in: {held:?}; counts={after:?}"
    );
}

#[test]
fn task_3213_expiry_removes_hidden_closed_asleep_readable_copies() {
    run_scenario(ExpiryGap::HiddenScrolled);
    run_scenario(ExpiryGap::Closed);
    run_scenario(ExpiryGap::Asleep);
}
