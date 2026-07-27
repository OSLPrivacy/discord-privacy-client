//! Schema-v5 message envelope tests.
//!
//! These inspect the exact database selected by the production path and
//! independently exercise the on-disk cryptographic construction. They do not
//! search for a finite list of plaintext encodings.

use crypto::aead;
use rusqlite::params;
use std::path::Path;
use store::{MessageStore, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[1u8; 32];
const MASTER_INFO: &[u8] = b"osl-message-store-v1";
const INDEX_INFO: &[u8] = b"osl-message-store-index-v1";
const BI_MESSAGE_ID: &[u8] = b"osl-store-bi/discord_message_id-v1";
const BI_CHANNEL_ID: &[u8] = b"osl-store-bi/channel_id-v1";
const BI_SENDER_ID: &[u8] = b"osl-store-bi/sender_discord_id-v1";
const MESSAGE_BODY_DOMAIN: &[u8] = b"osl-store/body/v5/message";
const MESSAGE_WRAP_DOMAIN: &[u8] = b"osl-store/wrap/v5/message";

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawMessage {
    mid_bi: Vec<u8>,
    meta_nonce: Vec<u8>,
    meta_ct: Vec<u8>,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    content_version: i64,
    wrapped_key_nonce: Option<Vec<u8>>,
    wrapped_key: Option<Vec<u8>>,
    burned: i64,
}

fn sample(id: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: "wrapped-channel".to_string(),
        sender_discord_id: format!("sender-{id}"),
        sender_osl_user_id: format!("osl-{id}"),
        plaintext: body.to_string(),
        decrypted_at: at,
        burned: false,
    }
}

fn open(dir: &Path) -> MessageStore {
    MessageStore::open(dir, SECRET).expect("store should open")
}

fn master_key() -> aead::Key {
    let bytes = crypto::hkdf::derive_32(&[], SECRET, MASTER_INFO).unwrap();
    aead::Key::from_bytes(bytes)
}

fn mid_bi(id: &str) -> Vec<u8> {
    let index_key = crypto::hkdf::derive_32(&[], SECRET, INDEX_INFO).unwrap();
    crypto::hkdf::derive_32(&index_key, id.as_bytes(), BI_MESSAGE_ID)
        .unwrap()
        .to_vec()
}

fn aad(domain: &[u8], selector: &[u8], version: i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + 8 + selector.len() + 8);
    out.extend_from_slice(domain);
    out.extend_from_slice(&(selector.len() as u64).to_le_bytes());
    out.extend_from_slice(selector);
    out.extend_from_slice(&version.to_le_bytes());
    out
}

fn blind_index(domain: &[u8], value: &str) -> Vec<u8> {
    let index_key = crypto::hkdf::derive_32(&[], SECRET, INDEX_INFO).unwrap();
    crypto::hkdf::derive_32(&index_key, value.as_bytes(), domain)
        .unwrap()
        .to_vec()
}

fn push_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn v4_message_metadata(message: &StoredMessage) -> Vec<u8> {
    let mut out = Vec::new();
    push_string(&mut out, &message.discord_message_id);
    push_string(&mut out, &message.channel_id);
    push_string(&mut out, &message.sender_discord_id);
    push_string(&mut out, &message.sender_osl_user_id);
    out.extend_from_slice(&message.decrypted_at.to_le_bytes());
    out
}

fn seal_master(aad: &[u8], plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let nonce = crypto::random::random_nonce();
    let ciphertext = crypto::aead::seal(&master_key(), &nonce, aad, plaintext).unwrap();
    (nonce.as_bytes().to_vec(), ciphertext)
}

fn nonce(bytes: &[u8]) -> aead::Nonce {
    let array: [u8; aead::NONCE_SIZE] = bytes.try_into().expect("nonce length");
    aead::Nonce::from_bytes(array)
}

fn raw_message(dir: &Path, id: &str) -> RawMessage {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    conn.query_row(
        "SELECT mid_bi, meta_nonce, meta_ct, ciphertext, nonce, \
                content_version, wrapped_key_nonce, wrapped_key, burned \
           FROM messages WHERE mid_bi = ?1",
        params![mid_bi(id)],
        |row| {
            Ok(RawMessage {
                mid_bi: row.get(0)?,
                meta_nonce: row.get(1)?,
                meta_ct: row.get(2)?,
                ciphertext: row.get(3)?,
                nonce: row.get(4)?,
                content_version: row.get(5)?,
                wrapped_key_nonce: row.get(6)?,
                wrapped_key: row.get(7)?,
                burned: row.get(8)?,
            })
        },
    )
    .unwrap()
}

fn unwrap_dek(raw: &RawMessage) -> aead::Key {
    let wrap_nonce = raw
        .wrapped_key_nonce
        .as_deref()
        .expect("live row must carry wrapper nonce");
    let wrapped = raw
        .wrapped_key
        .as_deref()
        .expect("live row must carry wrapped DEK");
    let bytes = crypto::aead::open(
        &master_key(),
        &nonce(wrap_nonce),
        &aad(MESSAGE_WRAP_DOMAIN, &raw.mid_bi, raw.content_version),
        wrapped,
    )
    .expect("master must authenticate and unwrap the selected DEK");
    let array: [u8; aead::KEY_SIZE] = bytes.try_into().expect("wrapped DEK length");
    aead::Key::from_bytes(array)
}

fn decrypt_body(raw: &RawMessage, key: &aead::Key) -> Result<Vec<u8>, crypto::Error> {
    crypto::aead::open(
        key,
        &nonce(&raw.nonce),
        &aad(MESSAGE_BODY_DOMAIN, &raw.mid_bi, raw.content_version),
        &raw.ciphertext,
    )
}

#[test]
fn live_messages_use_distinct_wrapped_deks_and_restart() {
    let tmp = TempDir::new().unwrap();
    let first = sample("wrapped-a", "first protected body", 1);
    let second = sample("wrapped-b", "second protected body", 2);
    {
        let store = open(tmp.path());
        store.put(&first).unwrap();
        store.put(&second).unwrap();
        assert_eq!(store.get("wrapped-a").unwrap(), Some(first.clone()));
        assert_eq!(store.get("wrapped-b").unwrap(), Some(second.clone()));
    }

    let raw_a = raw_message(tmp.path(), "wrapped-a");
    let raw_b = raw_message(tmp.path(), "wrapped-b");
    let dek_a = unwrap_dek(&raw_a);
    let dek_b = unwrap_dek(&raw_b);
    assert_ne!(
        dek_a.as_bytes(),
        dek_b.as_bytes(),
        "each live row needs a unique DEK"
    );
    assert_ne!(
        raw_a.wrapped_key, raw_b.wrapped_key,
        "two live rows must not share one wrapped key"
    );
    assert_ne!(
        raw_a.nonce,
        raw_a.wrapped_key_nonce.clone().unwrap(),
        "body and wrapper nonces must be independently generated"
    );
    assert_eq!(
        decrypt_body(&raw_a, &dek_a).unwrap(),
        first.plaintext.as_bytes()
    );
    assert_eq!(
        decrypt_body(&raw_b, &dek_b).unwrap(),
        second.plaintext.as_bytes()
    );
    assert!(
        decrypt_body(&raw_a, &master_key()).is_err(),
        "the store master key must not directly decrypt a v5 body"
    );

    let reopened = open(tmp.path());
    assert_eq!(reopened.get("wrapped-a").unwrap(), Some(first));
    assert_eq!(reopened.get("wrapped-b").unwrap(), Some(second));
}

#[test]
fn master_encrypted_body_substitution_is_refused() {
    let tmp = TempDir::new().unwrap();
    let target = sample("master-substitution", "body under a DEK", 1);
    let survivor = sample("master-survivor", "survivor exact", 2);
    let store = open(tmp.path());
    store.put(&target).unwrap();
    store.put(&survivor).unwrap();
    assert_eq!(
        store.get(&target.discord_message_id).unwrap(),
        Some(target.clone())
    );
    assert_eq!(
        store.get(&survivor.discord_message_id).unwrap(),
        Some(survivor.clone())
    );

    let raw = raw_message(tmp.path(), &target.discord_message_id);
    let bad_nonce = crypto::random::random_nonce();
    let bad_ct = crypto::aead::seal(
        &master_key(),
        &bad_nonce,
        &aad(MESSAGE_BODY_DOMAIN, &raw.mid_bi, raw.content_version),
        b"master-key substitution",
    )
    .unwrap();
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    let changed = conn
        .execute(
            "UPDATE messages SET ciphertext = ?1, nonce = ?2 WHERE mid_bi = ?3",
            params![bad_ct, bad_nonce.as_bytes().to_vec(), raw.mid_bi],
        )
        .unwrap();
    assert_eq!(changed, 1);

    assert!(
        matches!(
            store.get(&target.discord_message_id),
            Err(StoreError::Corrupted(_))
        ),
        "a body authenticated by the master instead of its wrapped DEK must be refused"
    );
    assert_eq!(
        store.get(&survivor.discord_message_id).unwrap(),
        Some(survivor.clone())
    );
}

#[test]
fn wrapper_and_body_cross_row_swap_is_refused() {
    let tmp = TempDir::new().unwrap();
    let source = sample("swap-source", "source body", 1);
    let target = sample("swap-target", "target body", 2);
    let store = open(tmp.path());
    store.put(&source).unwrap();
    store.put(&target).unwrap();
    let source_raw = raw_message(tmp.path(), &source.discord_message_id);
    let target_before = raw_message(tmp.path(), &target.discord_message_id);

    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    let changed = conn
        .execute(
            "UPDATE messages \
                SET ciphertext = ?1, nonce = ?2, wrapped_key_nonce = ?3, wrapped_key = ?4 \
              WHERE mid_bi = ?5",
            params![
                source_raw.ciphertext,
                source_raw.nonce,
                source_raw.wrapped_key_nonce,
                source_raw.wrapped_key,
                target_before.mid_bi,
            ],
        )
        .unwrap();
    assert_eq!(changed, 1);

    assert!(
        matches!(
            store.get(&target.discord_message_id),
            Err(StoreError::Corrupted(_))
        ),
        "message selector/type AAD must reject a cross-row envelope pair"
    );
    assert_eq!(store.get(&source.discord_message_id).unwrap(), Some(source));
}

#[test]
fn stale_same_row_version_replay_is_refused() {
    let tmp = TempDir::new().unwrap();
    let first = sample("versioned-row", "version one", 1);
    let second = sample("versioned-row", "version two", 2);
    let store = open(tmp.path());
    store.put(&first).unwrap();
    let stale = raw_message(tmp.path(), &first.discord_message_id);
    store.put(&second).unwrap();
    let current = raw_message(tmp.path(), &second.discord_message_id);
    assert!(current.content_version > stale.content_version);
    assert_eq!(store.get(&second.discord_message_id).unwrap(), Some(second));

    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    let changed = conn
        .execute(
            "UPDATE messages \
                SET ciphertext = ?1, nonce = ?2, \
                    wrapped_key_nonce = ?3, wrapped_key = ?4 \
              WHERE mid_bi = ?5",
            params![
                &stale.ciphertext,
                &stale.nonce,
                &stale.wrapped_key_nonce,
                &stale.wrapped_key,
                &current.mid_bi,
            ],
        )
        .unwrap();
    assert_eq!(changed, 1);

    assert!(
        matches!(store.get("versioned-row"), Err(StoreError::Corrupted(_))),
        "a stale envelope must not authenticate under the current row version"
    );

    assert_eq!(
        conn.execute(
            "UPDATE messages \
                SET ciphertext = ?1, nonce = ?2, content_version = ?3, \
                    wrapped_key_nonce = ?4, wrapped_key = ?5 \
              WHERE mid_bi = ?6",
            params![
                &stale.ciphertext,
                &stale.nonce,
                stale.content_version,
                &stale.wrapped_key_nonce,
                &stale.wrapped_key,
                &current.mid_bi,
            ],
        )
        .unwrap(),
        1
    );
    assert!(
        matches!(store.get("versioned-row"), Err(StoreError::Corrupted(_))),
        "rolling the version back without its version-bound metadata must fail"
    );
}

#[test]
fn selected_burn_destroys_only_its_wrapper_and_preserves_survivor() {
    let tmp = TempDir::new().unwrap();
    let selected = sample("burn-selected", "destroy this DEK", 1);
    let survivor = sample("burn-survivor", "preserve this exactly", 2);
    let store = open(tmp.path());
    store.put(&selected).unwrap();
    store.put(&survivor).unwrap();
    let selected_before = raw_message(tmp.path(), &selected.discord_message_id);
    let survivor_before = raw_message(tmp.path(), &survivor.discord_message_id);
    assert!(selected_before.wrapped_key.is_some());
    assert!(survivor_before.wrapped_key.is_some());

    store.mark_burned(&selected.discord_message_id).unwrap();
    assert!(store.get(&selected.discord_message_id).unwrap().is_none());
    assert_eq!(
        store.get(&survivor.discord_message_id).unwrap(),
        Some(survivor.clone())
    );

    let selected_after = raw_message(tmp.path(), &selected.discord_message_id);
    let survivor_after = raw_message(tmp.path(), &survivor.discord_message_id);
    assert_eq!(selected_after.burned, 1);
    assert!(selected_after.wrapped_key.is_none());
    assert!(selected_after.wrapped_key_nonce.is_none());
    assert!(selected_after.ciphertext.iter().all(|byte| *byte == 0));
    assert!(selected_after.nonce.iter().all(|byte| *byte == 0));
    assert_eq!(
        survivor_after, survivor_before,
        "burn must not rewrite the untouched survivor envelope"
    );
}

#[test]
fn v4_to_v5_mid_row_failure_rolls_back_and_reopens_after_repair() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("messages.sqlite");
    let first = sample("migration-first", "first migrates", 1);
    let second = sample("migration-second", "second repaired", 2);
    let (canary_nonce, canary_ct) =
        seal_master(b"osl-message-store/canary", b"osl-message-store-canary-v1");
    let mut repaired_second: Option<(Vec<u8>, Vec<u8>, Vec<u8>)> = None;
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
             CREATE TABLE messages (
                mid_bi BLOB PRIMARY KEY,
                chan_bi BLOB NOT NULL,
                sender_bi BLOB NOT NULL,
                meta_nonce BLOB NOT NULL,
                meta_ct BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                nonce BLOB NOT NULL,
                seq INTEGER NOT NULL,
                burned INTEGER NOT NULL DEFAULT 0,
                burned_at INTEGER,
                wrapped_key BLOB,
                discord_message_id TEXT,
                channel_id TEXT,
                sender_discord_id TEXT,
                sender_osl_user_id TEXT,
                decrypted_at INTEGER,
                scope_type TEXT,
                scope_id TEXT,
                meta_tag BLOB
             );
             CREATE TABLE attachments (
                ck_bi BLOB PRIMARY KEY,
                mid_bi BLOB NOT NULL,
                sender_bi BLOB,
                meta_nonce BLOB NOT NULL,
                meta_ct BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                nonce BLOB NOT NULL,
                seq INTEGER NOT NULL,
                cache_key TEXT,
                discord_message_id TEXT,
                random_filename TEXT,
                mime TEXT,
                byte_len INTEGER,
                created_at INTEGER,
                scope_type TEXT,
                scope_id TEXT,
                sender_discord_id TEXT
             );",
        )
        .unwrap();
        for (key, value) in [
            ("schema_version", 4u32.to_le_bytes().to_vec()),
            ("canary_nonce", canary_nonce),
            ("canary_ct", canary_ct),
        ] {
            conn.execute(
                "INSERT INTO _meta(key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .unwrap();
        }
        for (index, message) in [&first, &second].into_iter().enumerate() {
            let mid = blind_index(BI_MESSAGE_ID, &message.discord_message_id);
            let chan = blind_index(BI_CHANNEL_ID, &message.channel_id);
            let sender = blind_index(BI_SENDER_ID, &message.sender_discord_id);
            let (meta_nonce, meta_ct) = seal_master(&mid, &v4_message_metadata(message));
            let (body_nonce, body_ct) = seal_master(
                message.discord_message_id.as_bytes(),
                message.plaintext.as_bytes(),
            );
            let stored_ct = if index == 0 {
                body_ct.clone()
            } else {
                let mut corrupted = body_ct.clone();
                corrupted[0] ^= 0x80;
                repaired_second = Some((mid.clone(), body_nonce.clone(), body_ct.clone()));
                corrupted
            };
            conn.execute(
                "INSERT INTO messages
                    (mid_bi, chan_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce,
                     seq, burned, burned_at, wrapped_key)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, NULL, NULL)",
                params![
                    mid,
                    chan,
                    sender,
                    meta_nonce,
                    meta_ct,
                    stored_ct,
                    body_nonce,
                    index as i64 + 1
                ],
            )
            .unwrap();
        }
    }

    assert!(matches!(
        MessageStore::open(tmp.path(), SECRET),
        Err(StoreError::Corrupted(_))
    ));
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT value FROM _meta WHERE key='schema_version'",
                [],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
            4u32.to_le_bytes()
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='messages_v5'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "the first converted row must roll back with the second-row failure"
        );
        let (mid, nonce, ciphertext) = repaired_second.unwrap();
        assert_eq!(
            conn.execute(
                "UPDATE messages SET nonce=?1, ciphertext=?2 WHERE mid_bi=?3",
                params![nonce, ciphertext, mid],
            )
            .unwrap(),
            1
        );
    }

    let reopened = open(tmp.path());
    assert_eq!(reopened.get("migration-first").unwrap(), Some(first));
    assert_eq!(reopened.get("migration-second").unwrap(), Some(second));
    assert_eq!(
        rusqlite::Connection::open(&db)
            .unwrap()
            .query_row(
                "SELECT value FROM _meta WHERE key='schema_version'",
                [],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        7u32.to_le_bytes()
    );
}
