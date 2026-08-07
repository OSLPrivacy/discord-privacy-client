use rusqlite::params;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x49; 32];
const UNABLE_STATE: &str = "unable_destructive_remote_burn";
const SEPARATE_GRANT_STATE: &str = "has_separate_remote_burn_grant";
const LEGACY_UNABLE_COUNT_KEY: &str = "remote_burn_legacy_unable_count";

fn message(id: &str, seq: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: "task-0409-channel".to_string(),
        sender_discord_id: format!("task-0409-sender-{seq}"),
        sender_osl_user_id: format!("sender-{seq}"),
        plaintext: format!("old stored message {seq}"),
        decrypted_at: 1_786_000_000 + seq,
        burned: false,
    }
}

fn schema_version(conn: &rusqlite::Connection) -> u32 {
    let bytes: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    u32::from_le_bytes(bytes.try_into().unwrap())
}

#[test]
fn task_0409_real_store_migration_reports_old_messages_and_no_guessed_grants() {
    let tmp = TempDir::new().unwrap();
    {
        let store = MessageStore::open(tmp.path(), SECRET).unwrap();
        for seq in 1..=4 {
            store
                .put(&message(&format!("task-0409-old-{seq}"), seq))
                .unwrap();
        }
    }

    let db_path = tmp.path().join("messages.sqlite");
    let old_messages = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        assert_eq!(schema_version(&conn), 10);
        let old_messages: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        conn.execute_batch(
            "ALTER TABLE messages DROP COLUMN destructive_remote_burn_grant;
             ALTER TABLE messages DROP COLUMN destructive_remote_burn_state;",
        )
        .unwrap();
        conn.execute(
            "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
            params![9u32.to_le_bytes().to_vec()],
        )
        .unwrap();
        assert_eq!(schema_version(&conn), 9);
        old_messages
    };

    let store = MessageStore::open(tmp.path(), SECRET).unwrap();
    assert!(store.get("task-0409-old-1").unwrap().is_some());
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let reported_bytes: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = ?1",
            params![LEGACY_UNABLE_COUNT_KEY],
            |row| row.get(0),
        )
        .unwrap();
    let mut reported = [0u8; 8];
    reported.copy_from_slice(&reported_bytes);
    let reported_unable = u64::from_le_bytes(reported);
    let unable_messages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages
              WHERE destructive_remote_burn_state = ?1
                AND destructive_remote_burn_grant IS NULL",
            params![UNABLE_STATE],
            |row| row.get(0),
        )
        .unwrap();
    let guessed_grants: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages
              WHERE destructive_remote_burn_state = ?1
                 OR destructive_remote_burn_grant IS NOT NULL",
            params![SEPARATE_GRANT_STATE],
            |row| row.get(0),
        )
        .unwrap();

    println!(
        "task_0409 remote burn migration: old_messages={old_messages} reported_unable={reported_unable} {UNABLE_STATE}={unable_messages} guessed_grants={guessed_grants}"
    );
    assert_eq!(reported_unable, old_messages as u64);
    assert_eq!(unable_messages, old_messages);
    assert_eq!(guessed_grants, 0);
    assert_eq!(schema_version(&conn), 10);
}
