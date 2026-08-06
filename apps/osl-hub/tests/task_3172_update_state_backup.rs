use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use tempfile::TempDir;

use osl_privacy_hub::update_state_backup::{
    copy_identity_and_history_before_update, read_update_state_copy_record,
};

const FILE_KEY: [u8; 32] = [0x31; 32];

struct FileKeyGuard(Option<[u8; 32]>);

impl FileKeyGuard {
    fn set(key: [u8; 32]) -> Self {
        let previous = ipc::main_password::get_file_storage_key();
        ipc::main_password::set_file_storage_key(Some(key));
        Self(previous)
    }
}

impl Drop for FileKeyGuard {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(self.0);
    }
}

#[test]
fn task_3172_copies_identity_friends_allowed_places_and_messages_before_update() {
    let _file_key = FileKeyGuard::set(FILE_KEY);
    let root = TempDir::new().expect("temp app config");
    let core = root.path().join("osl-core");
    fs::create_dir_all(&core).expect("create core dir");

    write_identity_files(&core);
    write_people_file(&core, 2);
    write_allowed_places(&core, 4);
    write_message_history(&core.join("store"), 5);

    let source = fs::read_to_string("src/main.rs").expect("read main source");
    let backup_call = source
        .find("copy_identity_and_history_before_update")
        .expect("update backup call is wired");
    let replace_call = source
        .find("download_and_install")
        .expect("update replacement call is wired");
    assert!(
        backup_call < replace_call,
        "state copy must happen before updater replacement"
    );

    let record = copy_identity_and_history_before_update(root.path(), "0.2.0")
        .expect("copy update state backup");
    let persisted_record = read_update_state_copy_record(
        &Path::new(&record.backup_dir).join("update-state-copy-record.json"),
    )
    .expect("read persisted update state copy record");
    assert_eq!(persisted_record, record);
    assert_eq!(record.live_counts, record.copied_counts);
    assert_eq!(record.live_counts.identity_count, 3);
    assert_eq!(record.live_counts.friend_count, 2);
    assert_eq!(record.live_counts.allowed_place_count, 4);
    assert_eq!(record.live_counts.message_count, 5);

    let record_json =
        fs::read_to_string(Path::new(&record.backup_dir).join("update-state-copy-record.json"))
            .expect("record json exists");
    for name in [
        "identity_count",
        "friend_count",
        "allowed_place_count",
        "message_count",
    ] {
        assert!(record_json.contains(name), "record must name {name}");
    }

    println!("TASK3172_BACKUP_BEFORE_REPLACE=true");
    println!(
        "TASK3172_LIVE_IDENTITY_COUNT={}",
        record.live_counts.identity_count
    );
    println!(
        "TASK3172_COPY_IDENTITY_COUNT={}",
        record.copied_counts.identity_count
    );
    println!(
        "TASK3172_LIVE_FRIEND_COUNT={}",
        record.live_counts.friend_count
    );
    println!(
        "TASK3172_COPY_FRIEND_COUNT={}",
        record.copied_counts.friend_count
    );
    println!(
        "TASK3172_LIVE_ALLOWED_PLACE_COUNT={}",
        record.live_counts.allowed_place_count
    );
    println!(
        "TASK3172_COPY_ALLOWED_PLACE_COUNT={}",
        record.copied_counts.allowed_place_count
    );
    println!(
        "TASK3172_LIVE_MESSAGE_COUNT={}",
        record.live_counts.message_count
    );
    println!(
        "TASK3172_COPY_MESSAGE_COUNT={}",
        record.copied_counts.message_count
    );
    println!(
        "TASK3172_RECORD_COUNT_NAMES=identity_count,friend_count,allowed_place_count,message_count"
    );
}

fn write_identity_files(core: &Path) {
    fs::write(core.join("identity.json"), b"sealed identity 0").expect("write flat identity");
    for slot in ["slot-a", "slot-b"] {
        let slot_dir = core.join("hub-identities").join(slot);
        fs::create_dir_all(&slot_dir).expect("create slot");
        fs::write(
            slot_dir.join("identity.json"),
            format!("sealed identity {slot}"),
        )
        .expect("write slot identity");
    }
}

fn write_people_file(core: &Path, count: usize) {
    let people = serde_json::json!({
        "version": 3,
        "people": (0..count)
            .map(|index| {
                (
                    format!("person-{index}"),
                    serde_json::json!({
                        "osl_user_id": format!("osl_friend_{index}"),
                        "ed25519_public": format!("ed25519-{index}"),
                        "safety_number_verified": true
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>()
    });
    let plain = serde_json::to_vec(&people).expect("encode people");
    let sealed = ipc::main_password::encrypt_at_rest(&plain, &FILE_KEY).expect("seal people");
    fs::write(core.join("hub_people.json"), sealed).expect("write people");
}

fn write_allowed_places(core: &Path, count: usize) {
    let conn = Connection::open(core.join("allowed_places.sqlite")).expect("open allowed db");
    conn.execute_batch(
        "CREATE TABLE allowed_places (
            stable_id TEXT PRIMARY KEY,
            app TEXT NOT NULL,
            account TEXT NOT NULL,
            kind TEXT NOT NULL
        );",
    )
    .expect("schema allowed places");
    for index in 0..count {
        conn.execute(
            "INSERT INTO allowed_places (stable_id, app, account, kind) VALUES (?1, ?2, ?3, ?4)",
            params![
                format!("discord:account:direct_message:friend-{index}"),
                "discord",
                "account",
                "direct_message"
            ],
        )
        .expect("insert allowed place");
    }
}

fn write_message_history(store_dir: &Path, count: usize) {
    fs::create_dir_all(store_dir).expect("create store dir");
    let conn = Connection::open(store_dir.join("messages.sqlite")).expect("open messages db");
    conn.execute_batch("CREATE TABLE messages (mid_bi BLOB PRIMARY KEY);")
        .expect("schema messages");
    for index in 0..count {
        conn.execute(
            "INSERT INTO messages (mid_bi) VALUES (?1)",
            params![format!("message-{index}")],
        )
        .expect("insert message");
    }
}
