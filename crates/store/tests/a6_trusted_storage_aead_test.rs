//! Narrow A6 canaries, not a finite proof of confidentiality.
//!
//! This test binds itself to the exact `messages.sqlite` root selected by the
//! public Store API and independently opens the persisted AEAD envelopes.  It
//! intentionally does not claim to enumerate backups, external staging, OS
//! root access, or every reversible plaintext representation.

use crypto::{aead, hkdf};
use rusqlite::{params, Connection};
use std::fs;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = b"a6-trusted-root-secret-32-bytes!";
const MASTER_INFO: &[u8] = b"osl-message-store-v1";
const INDEX_INFO: &[u8] = b"osl-message-store-index-v1";
const BI_MESSAGE: &[u8] = b"osl-store-bi/discord_message_id-v1";
const BI_CACHE: &[u8] = b"osl-store-bi/cache_key-v1";
const MESSAGE_BODY: &[u8] = b"osl-store/body/v5/message";
const MESSAGE_WRAP: &[u8] = b"osl-store/wrap/v5/message";
const ATTACHMENT_BODY: &[u8] = b"osl-store/body/v6/attachment";
const ATTACHMENT_WRAP: &[u8] = b"osl-store/wrap/v6/attachment";
const ATTACHMENT_META: &[u8] = b"osl-store/meta/v6/attachment";
const ATTACHMENT_COMMITMENT: &[u8] = b"osl-store/commitment/v6/attachment";

fn message(id: &str, body: &str) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: "a6-channel".to_string(),
        sender_discord_id: "a6-sender".to_string(),
        sender_osl_user_id: "a6-service".to_string(),
        plaintext: body.to_string(),
        decrypted_at: 77,
        burned: false,
    }
}

fn key(info: &[u8]) -> [u8; 32] {
    hkdf::derive_32(&[], SECRET, info).unwrap()
}

fn blind(domain: &[u8], text: &str) -> Vec<u8> {
    hkdf::derive_32(&key(INDEX_INFO), text.as_bytes(), domain)
        .unwrap()
        .to_vec()
}

fn nonce(bytes: &[u8]) -> aead::Nonce {
    aead::Nonce::from_bytes(bytes.try_into().expect("stored nonce length"))
}

fn versioned_aad(domain: &[u8], selector: &[u8], version: i64) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(domain);
    out.extend_from_slice(&(selector.len() as u64).to_le_bytes());
    out.extend_from_slice(selector);
    out.extend_from_slice(&version.to_le_bytes());
    out
}

fn push_part(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u64).to_le_bytes());
    out.extend_from_slice(part);
}

fn commitment(parts: &[&[u8]]) -> [u8; 32] {
    let mut canonical = Vec::new();
    for part in parts {
        push_part(&mut canonical, part);
    }
    hkdf::derive_32(&[], &canonical, ATTACHMENT_COMMITMENT).unwrap()
}

fn attachment_aad(
    domain: &[u8],
    ck_bi: &[u8],
    mid_bi: &[u8],
    seq: i64,
    version: i64,
    meta: Option<&[u8; 32]>,
    body: Option<&[u8; 32]>,
) -> Vec<u8> {
    let mut out = Vec::new();
    push_part(&mut out, domain);
    push_part(&mut out, ck_bi);
    push_part(&mut out, mid_bi);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    if let Some(meta) = meta {
        out.extend_from_slice(meta);
    }
    if let Some(body) = body {
        out.extend_from_slice(body);
    }
    out
}

fn attachment_meta_aad(ck_bi: &[u8], mid_bi: &[u8], seq: i64, version: i64) -> Vec<u8> {
    let mut out = Vec::new();
    push_part(&mut out, ATTACHMENT_META);
    push_part(&mut out, ck_bi);
    push_part(&mut out, mid_bi);
    out.extend_from_slice(&seq.to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out
}

#[test]
fn trusted_root_and_independent_aead_envelope_controls_are_nonvacuous() {
    let tmp = TempDir::new().unwrap();
    let decoy = TempDir::new().unwrap();
    let first = message("a6-message-one", "a6 known plaintext one");
    let second = message("a6-message-two", "a6 known plaintext two");
    // `0xfb, 0xff` encode to Base64 `+/8=`: both alphabet positions 62/63
    // are exercised as a canary only, never as an exhaustive encoding proof.
    let attachment_a = b"a6 attachment one \xfb\xff";
    let attachment_b = b"a6 attachment two \xfb\xff";
    assert_eq!(base64_two_byte_canary([0xfb, 0xff]), b"+/8=");

    // Fixed inputs make this a known-answer regression vector for the pinned
    // public AEAD API.  It is deliberately separate from Store's private
    // sealing helpers, so an identity/no-op replacement cannot satisfy the
    // envelope checks below by changing only those helpers.
    let kat_key = aead::Key::from_bytes([0x42; 32]);
    let kat_nonce = aead::Nonce::from_bytes([
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    ]);
    let kat = aead::seal(
        &kat_key,
        &kat_nonce,
        b"a6-known-answer-aad",
        b"a6-known-answer-plaintext",
    )
    .unwrap();
    assert_eq!(
        kat,
        [
            0xed, 0xca, 0xa9, 0x8c, 0xa9, 0xce, 0xb0, 0xae, 0xdf, 0x07, 0xca, 0x35, 0x75, 0x5d,
            0x8c, 0x93, 0xa1, 0xb5, 0x6a, 0xd3, 0xed, 0xdb, 0x2f, 0xd5, 0xdc, 0xfe, 0x05, 0x5e,
            0x1e, 0x0d, 0x51, 0x12, 0x3c, 0x4c, 0x1d, 0xce, 0x1e, 0x9c, 0xf3, 0x73, 0xbf,
        ]
    );
    assert_eq!(
        aead::open(&kat_key, &kat_nonce, b"a6-known-answer-aad", &kat).unwrap(),
        b"a6-known-answer-plaintext"
    );

    let store = MessageStore::open(tmp.path(), SECRET).unwrap();
    store.put(&first).unwrap();
    store.put(&second).unwrap();
    store
        .put_attachment(
            &first.discord_message_id,
            "first.bin",
            "application/octet-stream",
            attachment_a,
            None,
            None,
            None,
        )
        .unwrap();
    store
        .put_attachment(
            &first.discord_message_id,
            "second.bin",
            "application/octet-stream",
            attachment_b,
            None,
            None,
            None,
        )
        .unwrap();

    // Positive controls establish that this is a live, nonempty Store before
    // inspecting raw bytes.  A no-op writer cannot pass by storing nothing.
    assert_eq!(
        store.get(&first.discord_message_id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        store.get(&second.discord_message_id).unwrap(),
        Some(second.clone())
    );
    assert_eq!(
        store
            .get_attachment(&first.discord_message_id, "first.bin")
            .unwrap(),
        Some((
            "application/octet-stream".to_string(),
            attachment_a.to_vec()
        ))
    );
    assert_eq!(
        store
            .get_attachment(&first.discord_message_id, "second.bin")
            .unwrap(),
        Some((
            "application/octet-stream".to_string(),
            attachment_b.to_vec()
        ))
    );

    // Enumeration is bound to the Store-owned connection, not an inspection
    // connection opened by this test. These decoys have plausible names and
    // nonempty bytes but cannot enter the owned three-file result.
    fs::write(tmp.path().join("payload.sqlite"), b"nonempty decoy").unwrap();
    let decoy_store = MessageStore::open(decoy.path(), SECRET).unwrap();
    decoy_store
        .put(&message("a6-decoy", "nonempty wrong-root decoy"))
        .unwrap();
    let decoy_artifacts = decoy_store.live_storage_artifacts().unwrap();
    let artifacts = store.live_storage_artifacts().unwrap();
    assert_eq!(
        artifacts
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        vec![
            "messages.sqlite",
            "messages.sqlite-wal",
            "messages.sqlite-shm"
        ],
        "artifact names must be the exact bound main/WAL/SHM trio, not a prefix scan"
    );
    assert!(
        artifacts
            .iter()
            .all(|path| fs::metadata(path).unwrap().len() > 0),
        "the live connection must have attributable nonempty DB/WAL/SHM files"
    );
    assert!(
        artifacts
            .iter()
            .all(|path| path.parent() == Some(tmp.path())),
        "wrong-root files must not be attributed to this Store"
    );
    assert!(
        artifacts
            .iter()
            .zip(decoy_artifacts.iter())
            .all(|(actual, wrong_root)| actual != wrong_root),
        "a different live Store root must not be attributed to this Store"
    );
    assert!(artifacts
        .iter()
        .all(|path| path.file_name().unwrap() != "payload.sqlite"));

    // This independent handle only reads rows after Store-owned artifact
    // binding has already succeeded; it is never used to select the root.
    let conn = Connection::open(&artifacts[0]).unwrap();

    let master = aead::Key::from_bytes(key(MASTER_INFO));
    let mut message_deks = Vec::new();
    let mut message_envelopes = Vec::new();
    for (id, plaintext) in [
        (
            first.discord_message_id.as_str(),
            first.plaintext.as_bytes(),
        ),
        (
            second.discord_message_id.as_str(),
            second.plaintext.as_bytes(),
        ),
    ] {
        let mid = blind(BI_MESSAGE, id);
        let (ct, body_nonce, version, wrap_nonce, wrapped): (Vec<u8>, Vec<u8>, i64, Vec<u8>, Vec<u8>) = conn
            .query_row(
                "SELECT ciphertext, nonce, content_version, wrapped_key_nonce, wrapped_key FROM messages WHERE mid_bi=?1",
                params![mid.clone()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .unwrap();
        assert_ne!(ct, plaintext, "identity body persistence is forbidden");
        let dek = aead::open(
            &master,
            &nonce(&wrap_nonce),
            &versioned_aad(MESSAGE_WRAP, &mid, version),
            &wrapped,
        )
        .unwrap();
        let dek: [u8; 32] = dek.try_into().unwrap();
        message_deks.push(dek);
        let body_aad = versioned_aad(MESSAGE_BODY, &mid, version);
        assert_eq!(
            aead::open(
                &aead::Key::from_bytes(dek),
                &nonce(&body_nonce),
                &body_aad,
                &ct
            )
            .unwrap(),
            plaintext
        );
        let mut tampered = ct.clone();
        tampered[0] ^= 1;
        assert!(aead::open(
            &aead::Key::from_bytes(dek),
            &nonce(&body_nonce),
            &body_aad,
            &tampered
        )
        .is_err());
        assert!(aead::open(
            &aead::Key::from_bytes([9; 32]),
            &nonce(&body_nonce),
            &body_aad,
            &ct
        )
        .is_err());
        message_envelopes.push((dek, body_nonce, body_aad, ct));
    }

    assert_ne!(
        message_deks[0], message_deks[1],
        "fixed shared message DEK mutation must fail"
    );
    assert!(
        aead::open(
            &aead::Key::from_bytes(message_envelopes[0].0),
            &nonce(&message_envelopes[1].1),
            &message_envelopes[1].2,
            &message_envelopes[1].3,
        )
        .is_err(),
        "a message DEK must not open a different message record"
    );
    let mut attachment_deks = Vec::new();
    let mut attachment_envelopes = Vec::new();

    for (filename, plaintext) in [
        ("first.bin", attachment_a.as_slice()),
        ("second.bin", attachment_b.as_slice()),
    ] {
        let cache_key = format!("{}/{}", first.discord_message_id, filename);
        let ck = blind(BI_CACHE, &cache_key);
        let mid = blind(BI_MESSAGE, &first.discord_message_id);
        let (ct, body_nonce, seq, version, meta_nonce, meta_ct, wrap_nonce, wrapped): (Vec<u8>, Vec<u8>, i64, i64, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) = conn
            .query_row(
                "SELECT ciphertext, nonce, seq, content_version, meta_nonce, meta_ct, wrapped_key_nonce, wrapped_key FROM attachments WHERE ck_bi=?1",
                params![ck.clone()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
            )
            .unwrap();
        assert_ne!(
            ct, plaintext,
            "identity attachment persistence is forbidden"
        );
        let metadata = aead::open(
            &master,
            &nonce(&meta_nonce),
            &attachment_meta_aad(&ck, &mid, seq, version),
            &meta_ct,
        )
        .unwrap();
        let meta = commitment(&[&metadata]);
        let body = commitment(&[&body_nonce, &ct]);
        let aad = attachment_aad(
            ATTACHMENT_WRAP,
            &ck,
            &mid,
            seq,
            version,
            Some(&meta),
            Some(&body),
        );
        let dek = aead::open(&master, &nonce(&wrap_nonce), &aad, &wrapped).unwrap();
        let dek: [u8; 32] = dek.try_into().unwrap();
        let body_aad = attachment_aad(ATTACHMENT_BODY, &ck, &mid, seq, version, Some(&meta), None);
        // A message DEK must not decrypt an attachment body. This catches a
        // fixed shared DEK across record types, not merely ciphertext identity.
        for message_dek in &message_deks {
            assert!(aead::open(
                &aead::Key::from_bytes(*message_dek),
                &nonce(&body_nonce),
                &body_aad,
                &ct
            )
            .is_err());
        }
        attachment_deks.push(dek);
        assert_eq!(
            aead::open(
                &aead::Key::from_bytes(dek),
                &nonce(&body_nonce),
                &body_aad,
                &ct
            )
            .unwrap(),
            plaintext
        );
        let mut tampered = ct.clone();
        tampered[0] ^= 1;
        assert!(aead::open(
            &aead::Key::from_bytes(dek),
            &nonce(&body_nonce),
            &body_aad,
            &tampered
        )
        .is_err());
        assert!(aead::open(
            &aead::Key::from_bytes([7; 32]),
            &nonce(&body_nonce),
            &body_aad,
            &ct
        )
        .is_err());
        attachment_envelopes.push((dek, body_nonce, body_aad, ct));
    }
    assert_ne!(
        attachment_deks[0], attachment_deks[1],
        "fixed shared attachment DEK mutation must fail"
    );
    assert_ne!(
        message_deks[0], attachment_deks[0],
        "message and attachment DEKs must be type-separated"
    );
    assert!(
        aead::open(
            &aead::Key::from_bytes(attachment_envelopes[0].0),
            &nonce(&message_envelopes[0].1),
            &message_envelopes[0].2,
            &message_envelopes[0].3,
        )
        .is_err(),
        "an attachment DEK must not open a message record"
    );
    drop(conn);
    drop(store);
    let reopened = MessageStore::open(tmp.path(), SECRET).unwrap();
    assert_eq!(
        reopened.get(&first.discord_message_id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        reopened.get(&second.discord_message_id).unwrap(),
        Some(second)
    );
    assert_eq!(
        reopened
            .get_attachment(&first.discord_message_id, "first.bin")
            .unwrap(),
        Some((
            "application/octet-stream".to_string(),
            attachment_a.to_vec()
        ))
    );
    assert_eq!(
        reopened
            .get_attachment(&first.discord_message_id, "second.bin")
            .unwrap(),
        Some((
            "application/octet-stream".to_string(),
            attachment_b.to_vec()
        ))
    );
}

fn base64_two_byte_canary(input: [u8; 2]) -> Vec<u8> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    vec![
        ALPHABET[(input[0] >> 2) as usize],
        ALPHABET[(((input[0] & 0x03) << 4) | (input[1] >> 4)) as usize],
        ALPHABET[((input[1] & 0x0f) << 2) as usize],
        b'=',
    ]
}
