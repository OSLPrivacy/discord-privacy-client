//! Schema-v6 attachment envelope and burn-lifecycle tests.
//!
//! These tests inspect the exact `messages.sqlite` selected by production and
//! independently open its cryptographic envelope. They do not use finite
//! plaintext/encoding blacklists.

use crypto::aead;
use rusqlite::params;
use std::path::Path;
use store::{MessageStore, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[41u8; 32];
const MASTER_INFO: &[u8] = b"osl-message-store-v1";
const INDEX_INFO: &[u8] = b"osl-message-store-index-v1";
const BI_CACHE_KEY: &[u8] = b"osl-store-bi/cache_key-v1";
const BI_MESSAGE_ID: &[u8] = b"osl-store-bi/discord_message_id-v1";
const ATTACHMENT_BODY_DOMAIN: &[u8] = b"osl-store/body/v6/attachment";
const ATTACHMENT_WRAP_DOMAIN: &[u8] = b"osl-store/wrap/v6/attachment";

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawAttachment {
    ck_bi: Vec<u8>,
    mid_bi: Vec<u8>,
    sender_bi: Option<Vec<u8>>,
    meta_nonce: Vec<u8>,
    meta_ct: Vec<u8>,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    seq: i64,
    content_version: i64,
    wrapped_key_nonce: Option<Vec<u8>>,
    wrapped_key: Option<Vec<u8>>,
    burned: i64,
}

fn message(id: &str, channel: &str, body: &str) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: channel.to_string(),
        sender_discord_id: format!("sender-{id}"),
        sender_osl_user_id: format!("osl-{id}"),
        plaintext: body.to_string(),
        decrypted_at: 1,
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

fn cache_bi(message_id: &str, filename: &str) -> Vec<u8> {
    let index_key = crypto::hkdf::derive_32(&[], SECRET, INDEX_INFO).unwrap();
    let cache_key = format!("{message_id}/{filename}");
    crypto::hkdf::derive_32(&index_key, cache_key.as_bytes(), BI_CACHE_KEY)
        .unwrap()
        .to_vec()
}

fn message_bi(message_id: &str) -> Vec<u8> {
    let index_key = crypto::hkdf::derive_32(&[], SECRET, INDEX_INFO).unwrap();
    crypto::hkdf::derive_32(&index_key, message_id.as_bytes(), BI_MESSAGE_ID)
        .unwrap()
        .to_vec()
}

fn seal_master(aad: &[u8], plaintext: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let nonce = crypto::random::random_nonce();
    let ciphertext = crypto::aead::seal(&master_key(), &nonce, aad, plaintext).unwrap();
    (nonce.as_bytes().to_vec(), ciphertext)
}

fn push_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u64).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn v5_attachment_metadata(message_id: &str, filename: &str, mime: &str, len: usize) -> Vec<u8> {
    let mut out = Vec::new();
    push_string(&mut out, &format!("{message_id}/{filename}"));
    push_string(&mut out, message_id);
    push_string(&mut out, filename);
    push_string(&mut out, mime);
    out.extend_from_slice(&(len as i64).to_le_bytes());
    out.extend_from_slice(&1i64.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0]);
    out
}

fn raw_store_bytes(dir: &Path) -> Vec<u8> {
    let mut paths = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("messages.sqlite"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .filter(|path| path.is_file())
        .flat_map(|path| std::fs::read(path).unwrap())
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn nonce(bytes: &[u8]) -> aead::Nonce {
    let array: [u8; aead::NONCE_SIZE] = bytes.try_into().expect("nonce length");
    aead::Nonce::from_bytes(array)
}

fn digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut canonical = Vec::new();
    for part in parts {
        canonical.extend_from_slice(&(part.len() as u64).to_le_bytes());
        canonical.extend_from_slice(part);
    }
    crypto::hkdf::derive_32(&[], &canonical, b"osl-store/commitment/v6/attachment").unwrap()
}

fn attachment_aad(
    domain: &[u8],
    raw: &RawAttachment,
    meta_digest: &[u8; 32],
    body_digest: Option<&[u8; 32]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    for part in [domain, raw.ck_bi.as_slice(), raw.mid_bi.as_slice()] {
        out.extend_from_slice(&(part.len() as u64).to_le_bytes());
        out.extend_from_slice(part);
    }
    out.extend_from_slice(&raw.seq.to_le_bytes());
    out.extend_from_slice(&raw.content_version.to_le_bytes());
    out.extend_from_slice(meta_digest);
    if let Some(commitment) = body_digest {
        out.extend_from_slice(commitment);
    }
    out
}

fn raw_attachment(dir: &Path, message_id: &str, filename: &str) -> RawAttachment {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    conn.query_row(
        "SELECT ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq, \
                content_version, wrapped_key_nonce, wrapped_key, burned \
           FROM attachments WHERE ck_bi = ?1",
        params![cache_bi(message_id, filename)],
        |row| {
            Ok(RawAttachment {
                ck_bi: row.get(0)?,
                mid_bi: row.get(1)?,
                sender_bi: row.get(2)?,
                meta_nonce: row.get(3)?,
                meta_ct: row.get(4)?,
                ciphertext: row.get(5)?,
                nonce: row.get(6)?,
                seq: row.get(7)?,
                content_version: row.get(8)?,
                wrapped_key_nonce: row.get(9)?,
                wrapped_key: row.get(10)?,
                burned: row.get(11)?,
            })
        },
    )
    .unwrap()
}

fn attachment_metadata(raw: &RawAttachment) -> Vec<u8> {
    let mut aad = Vec::new();
    for part in [
        b"osl-store/meta/v6/attachment".as_slice(),
        raw.ck_bi.as_slice(),
        raw.mid_bi.as_slice(),
    ] {
        aad.extend_from_slice(&(part.len() as u64).to_le_bytes());
        aad.extend_from_slice(part);
    }
    aad.extend_from_slice(&raw.seq.to_le_bytes());
    aad.extend_from_slice(&raw.content_version.to_le_bytes());
    crypto::aead::open(&master_key(), &nonce(&raw.meta_nonce), &aad, &raw.meta_ct)
        .expect("attachment metadata must authenticate")
}

fn unwrap_dek(raw: &RawAttachment, metadata: &[u8]) -> aead::Key {
    let meta_digest = digest(&[metadata]);
    let body_digest = digest(&[&raw.nonce, &raw.ciphertext]);
    let bytes = crypto::aead::open(
        &master_key(),
        &nonce(raw.wrapped_key_nonce.as_deref().expect("wrapper nonce")),
        &attachment_aad(
            ATTACHMENT_WRAP_DOMAIN,
            raw,
            &meta_digest,
            Some(&body_digest),
        ),
        raw.wrapped_key.as_deref().expect("wrapped DEK"),
    )
    .expect("master key must authenticate the attachment DEK wrapper");
    let array: [u8; aead::KEY_SIZE] = bytes.try_into().expect("DEK length");
    aead::Key::from_bytes(array)
}

fn decrypt_body(
    raw: &RawAttachment,
    metadata: &[u8],
    key: &aead::Key,
) -> Result<Vec<u8>, crypto::Error> {
    let meta_digest = digest(&[metadata]);
    crypto::aead::open(
        key,
        &nonce(&raw.nonce),
        &attachment_aad(ATTACHMENT_BODY_DOMAIN, raw, &meta_digest, None),
        &raw.ciphertext,
    )
}

#[test]
fn live_attachments_use_distinct_wrapped_deks_and_restart() {
    let tmp = TempDir::new().unwrap();
    let store = open(tmp.path());
    store
        .put_attachment(
            "attachment-a",
            "a.png",
            "image/png",
            b"first attachment",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            "attachment-b",
            "b.png",
            "image/png",
            b"second attachment",
            None,
            None,
            None,
        )
        .unwrap();
    drop(store);

    let raw_a = raw_attachment(tmp.path(), "attachment-a", "a.png");
    let raw_b = raw_attachment(tmp.path(), "attachment-b", "b.png");
    let metadata_a = attachment_metadata(&raw_a);
    let metadata_b = attachment_metadata(&raw_b);

    let dek_a = unwrap_dek(&raw_a, &metadata_a);
    let dek_b = unwrap_dek(&raw_b, &metadata_b);
    assert_ne!(dek_a.as_bytes(), dek_b.as_bytes(), "per-record DEKs");
    assert_ne!(raw_a.wrapped_key, raw_b.wrapped_key, "wrapped DEKs");
    assert_eq!(
        decrypt_body(&raw_a, &metadata_a, &dek_a).unwrap(),
        b"first attachment"
    );
    assert!(
        decrypt_body(&raw_a, &metadata_a, &master_key()).is_err(),
        "the master key must not directly decrypt attachment bytes"
    );

    let reopened = open(tmp.path());
    assert_eq!(
        reopened
            .get_attachment("attachment-a", "a.png")
            .unwrap()
            .unwrap()
            .1,
        b"first attachment"
    );
    assert_eq!(
        reopened
            .get_attachment("attachment-b", "b.png")
            .unwrap()
            .unwrap()
            .1,
        b"second attachment"
    );
}

#[test]
fn selected_burn_shreds_message_and_all_attachment_envelopes_atomically() {
    let tmp = TempDir::new().unwrap();
    let selected = message("burn-owner", "burn-channel", "burn message");
    let survivor = message("safe-owner", "safe-channel", "safe message");
    let store = open(tmp.path());
    store.put(&selected).unwrap();
    store.put(&survivor).unwrap();
    store
        .put_attachment(
            "burn-owner",
            "first.png",
            "image/png",
            b"burn first",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            "burn-owner",
            "second.bin",
            "application/octet-stream",
            b"burn second",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            "safe-owner",
            "safe.png",
            "image/png",
            b"safe exact",
            None,
            None,
            None,
        )
        .unwrap();
    let survivor_before = raw_attachment(tmp.path(), "safe-owner", "safe.png");

    store.mark_burned("burn-owner").unwrap();
    assert!(store.get("burn-owner").unwrap().is_none());
    assert!(store
        .get_attachment("burn-owner", "first.png")
        .unwrap()
        .is_none());
    assert!(store
        .get_attachment("burn-owner", "second.bin")
        .unwrap()
        .is_none());
    assert_eq!(
        store.get_attachment("safe-owner", "safe.png").unwrap(),
        Some(("image/png".to_string(), b"safe exact".to_vec()))
    );
    assert_eq!(
        raw_attachment(tmp.path(), "safe-owner", "safe.png"),
        survivor_before
    );

    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    let burned: Vec<(i64, Option<Vec<u8>>, Option<Vec<u8>>, Vec<u8>, Vec<u8>)> = conn
        .prepare(
            "SELECT burned, wrapped_key_nonce, wrapped_key, nonce, ciphertext \
               FROM attachments WHERE mid_bi = \
                    (SELECT mid_bi FROM messages WHERE burned = 1)",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(burned.len(), 2, "every attachment remains as a burn stub");
    for (flag, wrap_nonce, wrapper, body_nonce, body) in burned {
        assert_eq!(flag, 1);
        assert!(wrap_nonce.is_none());
        assert!(wrapper.is_none());
        assert!(body_nonce.iter().all(|byte| *byte == 0));
        assert!(body.iter().all(|byte| *byte == 0));
    }
    drop(conn);
    store
        .put_attachment(
            "burn-owner",
            "first.png",
            "image/png",
            b"history replay must not restore this",
            None,
            None,
            None,
        )
        .unwrap();
    assert!(store
        .get_attachment("burn-owner", "first.png")
        .unwrap()
        .is_none());
    store.trim_attachments(1).unwrap();
    store
        .put_attachment(
            "burn-owner",
            "first.png",
            "image/png",
            b"stub trim must not reopen the parent",
            None,
            None,
            None,
        )
        .unwrap();
    assert!(store
        .get_attachment("burn-owner", "first.png")
        .unwrap()
        .is_none());
    drop(store);

    let reopened = open(tmp.path());
    assert!(reopened
        .get_attachment("burn-owner", "first.png")
        .unwrap()
        .is_none());
    assert_eq!(
        reopened.get_attachment("safe-owner", "safe.png").unwrap(),
        Some(("image/png".to_string(), b"safe exact".to_vec()))
    );
}

#[test]
fn wrapper_body_and_metadata_cross_row_swaps_are_refused() {
    let tmp = TempDir::new().unwrap();
    let store = open(tmp.path());
    for (id, file, mime, body) in [
        ("swap-a", "a.png", "image/png", b"first".as_slice()),
        ("swap-b", "b.txt", "text/plain", b"second".as_slice()),
    ] {
        store
            .put_attachment(id, file, mime, body, None, None, None)
            .unwrap();
    }
    let a = raw_attachment(tmp.path(), "swap-a", "a.png");
    let b = raw_attachment(tmp.path(), "swap-b", "b.txt");
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE attachments SET ciphertext=?1, nonce=?2, wrapped_key_nonce=?3, \
                    wrapped_key=?4 WHERE ck_bi=?5",
            params![
                a.ciphertext,
                a.nonce,
                a.wrapped_key_nonce,
                a.wrapped_key,
                b.ck_bi
            ],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        store.get_attachment("swap-b", "b.txt"),
        Err(StoreError::Corrupted(_))
    ));
    assert_eq!(
        store.get_attachment("swap-a", "a.png").unwrap().unwrap().1,
        b"first"
    );
}

#[test]
fn master_encrypted_body_and_partial_wrapper_substitutions_are_refused() {
    let tmp = TempDir::new().unwrap();
    let store = open(tmp.path());
    store
        .put_attachment(
            "master-target",
            "body.bin",
            "application/octet-stream",
            b"real DEK body",
            None,
            None,
            None,
        )
        .unwrap();
    let raw = raw_attachment(tmp.path(), "master-target", "body.bin");
    let metadata = attachment_metadata(&raw);
    let bad_nonce = crypto::random::random_nonce();
    let bad_ct = crypto::aead::seal(
        &master_key(),
        &bad_nonce,
        &attachment_aad(ATTACHMENT_BODY_DOMAIN, &raw, &digest(&[&metadata]), None),
        b"direct master substitution",
    )
    .unwrap();
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE attachments SET ciphertext=?1, nonce=?2 WHERE ck_bi=?3",
            params![bad_ct, bad_nonce.as_bytes().to_vec(), raw.ck_bi],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        store.get_attachment("master-target", "body.bin"),
        Err(StoreError::Corrupted(_))
    ));

    assert_eq!(
        conn.execute(
            "UPDATE attachments SET ciphertext=?1, nonce=?2, wrapped_key_nonce=NULL \
              WHERE ck_bi=?3",
            params![raw.ciphertext, raw.nonce, raw.ck_bi],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        store.get_attachment("master-target", "body.bin"),
        Err(StoreError::Corrupted(_))
    ));
}

#[test]
fn attachment_context_mutations_and_metadata_swap_are_refused() {
    let tmp = TempDir::new().unwrap();
    let store = open(tmp.path());
    store
        .put_attachment(
            "context-a",
            "a.png",
            "image/png",
            b"context a",
            None,
            None,
            Some("sender-a"),
        )
        .unwrap();
    store
        .put_attachment(
            "context-b",
            "b.txt",
            "text/plain",
            b"context b",
            None,
            None,
            Some("sender-b"),
        )
        .unwrap();
    let a = raw_attachment(tmp.path(), "context-a", "a.png");
    let b = raw_attachment(tmp.path(), "context-b", "b.txt");
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();

    assert_eq!(
        conn.execute(
            "UPDATE attachments SET mid_bi=?1 WHERE ck_bi=?2",
            params![&b.mid_bi, &a.ck_bi],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        store.get_attachment("context-a", "a.png"),
        Err(StoreError::Corrupted(_))
    ));
    conn.execute(
        "UPDATE attachments SET mid_bi=?1 WHERE ck_bi=?2",
        params![&a.mid_bi, &a.ck_bi],
    )
    .unwrap();

    assert_eq!(
        conn.execute(
            "UPDATE attachments SET sender_bi=?1 WHERE ck_bi=?2",
            params![&b.sender_bi, &a.ck_bi],
        )
        .unwrap(),
        1
    );
    assert!(matches!(
        store.get_attachment("context-a", "a.png"),
        Err(StoreError::Corrupted(_))
    ));
    conn.execute(
        "UPDATE attachments SET sender_bi=?1 WHERE ck_bi=?2",
        params![&a.sender_bi, &a.ck_bi],
    )
    .unwrap();
    let current = raw_attachment(tmp.path(), "context-a", "a.png");
    for (column, value) in [
        ("seq", current.seq + 1),
        ("content_version", current.content_version + 1),
    ] {
        conn.execute(
            &format!("UPDATE attachments SET {column}=?1 WHERE ck_bi=?2"),
            params![value, &current.ck_bi],
        )
        .unwrap();
        assert!(matches!(
            store.get_attachment("context-a", "a.png"),
            Err(StoreError::Corrupted(_))
        ));
        conn.execute(
            &format!("UPDATE attachments SET {column}=?1 WHERE ck_bi=?2"),
            params![
                if column == "seq" {
                    current.seq
                } else {
                    current.content_version
                },
                &current.ck_bi
            ],
        )
        .unwrap();
    }
    conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
    let temporary = vec![0x55u8; 32];
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&temporary, &a.ck_bi],
    )
    .unwrap();
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&a.ck_bi, &b.ck_bi],
    )
    .unwrap();
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&b.ck_bi, &temporary],
    )
    .unwrap();
    conn.execute_batch("COMMIT;").unwrap();
    assert!(matches!(
        store.get_attachment("context-a", "a.png"),
        Err(StoreError::Corrupted(_))
    ));
    conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&temporary, &a.ck_bi],
    )
    .unwrap();
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&a.ck_bi, &b.ck_bi],
    )
    .unwrap();
    conn.execute(
        "UPDATE attachments SET ck_bi=?1 WHERE ck_bi=?2",
        params![&b.ck_bi, &temporary],
    )
    .unwrap();
    conn.execute_batch("COMMIT;").unwrap();

    conn.execute(
        "UPDATE attachments SET meta_nonce=?1, meta_ct=?2 WHERE ck_bi=?3",
        params![b.meta_nonce, b.meta_ct, current.ck_bi],
    )
    .unwrap();
    assert!(matches!(
        store.get_attachment("context-a", "a.png"),
        Err(StoreError::Corrupted(_))
    ));
}

#[test]
fn stale_same_row_attachment_replay_is_refused() {
    let tmp = TempDir::new().unwrap();
    let store = open(tmp.path());
    store
        .put_attachment(
            "version-owner",
            "version.bin",
            "application/octet-stream",
            b"version one",
            None,
            None,
            None,
        )
        .unwrap();
    let stale = raw_attachment(tmp.path(), "version-owner", "version.bin");
    store
        .put_attachment(
            "version-owner",
            "version.bin",
            "application/octet-stream",
            b"version two",
            None,
            None,
            None,
        )
        .unwrap();
    let current = raw_attachment(tmp.path(), "version-owner", "version.bin");
    assert!(current.content_version > stale.content_version);
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    conn.execute(
        "UPDATE attachments SET ciphertext=?1, nonce=?2, wrapped_key_nonce=?3, \
                wrapped_key=?4 WHERE ck_bi=?5",
        params![
            stale.ciphertext,
            stale.nonce,
            stale.wrapped_key_nonce,
            stale.wrapped_key,
            current.ck_bi
        ],
    )
    .unwrap();
    assert!(matches!(
        store.get_attachment("version-owner", "version.bin"),
        Err(StoreError::Corrupted(_))
    ));
    conn.execute(
        "UPDATE attachments SET ciphertext=?1, nonce=?2, wrapped_key_nonce=?3, \
                wrapped_key=?4, content_version=?5 WHERE ck_bi=?6",
        params![
            current.ciphertext,
            current.nonce,
            current.wrapped_key_nonce,
            current.wrapped_key,
            stale.content_version,
            current.ck_bi
        ],
    )
    .unwrap();
    assert!(matches!(
        store.get_attachment("version-owner", "version.bin"),
        Err(StoreError::Corrupted(_))
    ));
}

#[test]
fn a_forced_attachment_shred_failure_rolls_back_the_whole_message_burn() {
    let tmp = TempDir::new().unwrap();
    let target = message("rollback-target", "rollback-channel", "target body");
    let survivor = message("rollback-survivor", "other-channel", "survivor body");
    let store = open(tmp.path());
    store.put(&target).unwrap();
    store.put(&survivor).unwrap();
    store
        .put_attachment(
            "rollback-target",
            "target.bin",
            "application/octet-stream",
            b"target attachment",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            "rollback-survivor",
            "survivor.bin",
            "application/octet-stream",
            b"survivor attachment",
            None,
            None,
            None,
        )
        .unwrap();
    let target_before = raw_attachment(tmp.path(), "rollback-target", "target.bin");
    let survivor_before = raw_attachment(tmp.path(), "rollback-survivor", "survivor.bin");
    let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER force_attachment_burn_failure
         BEFORE UPDATE OF wrapped_key ON attachments
         BEGIN SELECT RAISE(ABORT, 'forced attachment shred failure'); END;",
    )
    .unwrap();

    assert!(matches!(
        store.mark_burned("rollback-target"),
        Err(StoreError::Sqlite(_))
    ));
    assert_eq!(store.get("rollback-target").unwrap(), Some(target));
    assert_eq!(
        store
            .get_attachment("rollback-target", "target.bin")
            .unwrap()
            .unwrap()
            .1,
        b"target attachment"
    );
    assert_eq!(
        raw_attachment(tmp.path(), "rollback-target", "target.bin"),
        target_before
    );
    assert_eq!(
        raw_attachment(tmp.path(), "rollback-survivor", "survivor.bin"),
        survivor_before
    );
    conn.execute_batch("DROP TRIGGER force_attachment_burn_failure;")
        .unwrap();
    store.mark_burned("rollback-target").unwrap();
    assert!(store.get("rollback-target").unwrap().is_none());
}

#[test]
fn active_reader_observes_preburn_snapshot_but_burn_commits_as_one_state() {
    let tmp = TempDir::new().unwrap();
    let target = message("reader-target", "reader-channel", "target body");
    let survivor = message("reader-survivor", "other-channel", "survivor body");
    let store = open(tmp.path());
    store.put(&target).unwrap();
    store.put(&survivor).unwrap();
    store
        .put_attachment(
            "reader-target",
            "target.bin",
            "application/octet-stream",
            b"reader target",
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            "reader-survivor",
            "survivor.bin",
            "application/octet-stream",
            b"reader survivor",
            None,
            None,
            None,
        )
        .unwrap();
    let survivor_before = raw_attachment(tmp.path(), "reader-survivor", "survivor.bin");
    let reader = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
    reader.execute_batch("BEGIN;").unwrap();
    let old_wrapper_count: i64 = reader
        .query_row(
            "SELECT COUNT(*) FROM attachments WHERE wrapped_key IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_wrapper_count, 2);

    assert!(matches!(
        store.mark_burned("reader-target"),
        Err(StoreError::Sealer(message))
            if message.contains("reader is active")
    ));
    assert_eq!(
        rusqlite::Connection::open(tmp.path().join("messages.sqlite"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM _meta WHERE key='shred_checkpoint_pending'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1,
        "a checkpoint blocked by a reader must leave a durable recovery marker"
    );
    assert_eq!(
        reader
            .query_row(
                "SELECT COUNT(*) FROM attachments WHERE wrapped_key IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        2,
        "the already-open SQLite reader keeps its pre-burn snapshot"
    );
    assert!(store.get("reader-target").unwrap().is_none());
    assert!(store
        .get_attachment("reader-target", "target.bin")
        .unwrap()
        .is_none());
    assert_eq!(
        raw_attachment(tmp.path(), "reader-survivor", "survivor.bin"),
        survivor_before
    );
    reader.execute_batch("ROLLBACK;").unwrap();
    drop(reader);

    // Repeating the terminal burn completes the pending WAL checkpoint.
    store.mark_burned("reader-target").unwrap();
    drop(store);
    let reopened = open(tmp.path());
    assert!(reopened.get("reader-target").unwrap().is_none());
    assert_eq!(
        reopened
            .get_attachment("reader-survivor", "survivor.bin")
            .unwrap()
            .unwrap()
            .1,
        b"reader survivor"
    );
    assert_eq!(
        rusqlite::Connection::open(tmp.path().join("messages.sqlite"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM _meta WHERE key='shred_checkpoint_pending'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "successful retry/reopen must retire the checkpoint marker"
    );
}

#[test]
fn ambiguous_legacy_attachment_wrapper_columns_refuse_without_mutation() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("messages.sqlite");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
             INSERT INTO _meta VALUES ('schema_version', X'05000000');
             CREATE TABLE attachments (
                ck_bi BLOB,
                content_version INTEGER,
                wrapped_key BLOB
             );
             INSERT INTO attachments VALUES (X'01', 1, X'02');",
        )
        .unwrap();
    }
    let before = std::fs::read(&db).unwrap();
    assert!(matches!(
        MessageStore::open(tmp.path(), SECRET),
        Err(StoreError::Schema(message))
            if message == "live legacy attachment has ambiguous wrapped-key columns; refusing to guess their format"
    ));
    assert_eq!(
        std::fs::read(&db).unwrap(),
        before,
        "preflight refusal must precede journal/schema mutation"
    );
}

#[test]
fn duplicate_v5_attachment_selector_aborts_migration_without_partial_v6_state() {
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join("messages.sqlite");
    let message_id = "duplicate-owner";
    let filename = "duplicate.bin";
    let mime = "application/octet-stream";
    let body = b"legacy direct-master body";
    let cache_key = format!("{message_id}/{filename}");
    let ck_bi = cache_bi(message_id, filename);
    let mid_bi = message_bi(message_id);
    let metadata = v5_attachment_metadata(message_id, filename, mime, body.len());
    let (meta_nonce, meta_ct) = seal_master(&ck_bi, &metadata);
    let (body_nonce, ciphertext) = seal_master(cache_key.as_bytes(), body);
    let (canary_nonce, canary_ct) =
        seal_master(b"osl-message-store/canary", b"osl-message-store-canary-v1");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value BLOB);
             CREATE TABLE attachments (
                ck_bi BLOB,
                mid_bi BLOB NOT NULL,
                sender_bi BLOB,
                meta_nonce BLOB NOT NULL,
                meta_ct BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                nonce BLOB NOT NULL,
                seq INTEGER NOT NULL
             );",
        )
        .unwrap();
        for (key, value) in [
            ("schema_version", 5u32.to_le_bytes().to_vec()),
            ("canary_nonce", canary_nonce),
            ("canary_ct", canary_ct),
        ] {
            conn.execute(
                "INSERT INTO _meta(key, value) VALUES (?1, ?2)",
                params![key, value],
            )
            .unwrap();
        }
        for seq in [1i64, 2] {
            conn.execute(
                "INSERT INTO attachments
                    (ck_bi, mid_bi, sender_bi, meta_nonce, meta_ct, ciphertext, nonce, seq)
                 VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &ck_bi,
                    &mid_bi,
                    &meta_nonce,
                    &meta_ct,
                    &ciphertext,
                    &body_nonce,
                    seq
                ],
            )
            .unwrap();
        }
    }

    assert!(matches!(
        MessageStore::open(tmp.path(), SECRET),
        Err(StoreError::Sqlite(_))
    ));
    let conn = rusqlite::Connection::open(&db).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT value FROM _meta WHERE key='schema_version'",
            [],
            |row| row.get::<_, Vec<u8>>(0)
        )
        .unwrap(),
        5u32.to_le_bytes()
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM attachments", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        2,
        "both legacy rows must survive the failed migration"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='attachments_v6'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "the failed transaction must not strand a partial v6 table"
    );
}

#[test]
fn pending_vacuum_marker_is_recovered_before_normal_use() {
    let tmp = TempDir::new().unwrap();
    {
        let store = open(tmp.path());
        store
            .put_attachment(
                "vacuum-owner",
                "kept.bin",
                "application/octet-stream",
                b"kept through recovery",
                None,
                None,
                None,
            )
            .unwrap();
    }
    let sentinel = b"VACUUM-RECOVERY-RAW-PAGE-SENTINEL-7d4bdfe8";
    {
        let conn = rusqlite::Connection::open(tmp.path().join("messages.sqlite")).unwrap();
        conn.pragma_update(None, "secure_delete", "OFF").unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO _meta(key, value) VALUES ('vacuum_pending', X'01')",
            [],
        )
        .unwrap();
        conn.execute_batch("CREATE TABLE vacuum_recovery_garbage(value BLOB);")
            .unwrap();
        let residue = sentinel.repeat(2048);
        conn.execute(
            "INSERT INTO vacuum_recovery_garbage(value) VALUES (?1)",
            params![residue],
        )
        .unwrap();
        conn.execute_batch(
            "DROP TABLE vacuum_recovery_garbage;
             PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .unwrap();
    }
    assert!(
        contains(&raw_store_bytes(tmp.path()), sentinel),
        "fixture must leave the dropped-table sentinel in a raw SQLite page"
    );
    let reopened = open(tmp.path());
    assert_eq!(
        reopened
            .get_attachment("vacuum-owner", "kept.bin")
            .unwrap()
            .unwrap()
            .1,
        b"kept through recovery"
    );
    drop(reopened);
    assert!(
        !contains(&raw_store_bytes(tmp.path()), sentinel),
        "pending VACUUM recovery must remove the raw free-page residue"
    );
    assert_eq!(
        rusqlite::Connection::open(tmp.path().join("messages.sqlite"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM _meta WHERE key='vacuum_pending'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
