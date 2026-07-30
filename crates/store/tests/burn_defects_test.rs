//! Regression tests for the store defects raised in
//! `docs/security/osl-audit-2026-07-26-codex.md`.
//!
//! ## Why this file exists separately
//!
//! Every test here was written to **fail against the build that shipped the
//! defect**, and each one names the defect it pins. A regression test that has
//! only ever been observed passing proves nothing about the bug it claims to
//! cover — it can only re-assert what its author already believed. The failing
//! run for each test is recorded in `docs/reports/store-lane-2026-07-26.md`.
//!
//! ## No doubles
//!
//! These drive a real `MessageStore` over a real SQLite file created by the
//! real `schema::migrate` path, and assert against the bytes on disk with an
//! independent `rusqlite::Connection`. A hand-rolled DB fake would be
//! answerable only in terms of the behaviour the author encoded into it, which
//! is exactly the failure mode that let a broken attachment upload stay green.
//!
//! ## Positive path first
//!
//! Every destruction assertion is preceded by an assertion that the thing to be
//! destroyed was actually there. Without that, "nothing came back" is
//! indistinguishable from "nothing was ever stored" and the test passes
//! vacuously.

use std::path::{Path, PathBuf};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET_A: &[u8; 32] = &[1u8; 32];

fn open_a(dir: &Path) -> MessageStore {
    MessageStore::open(dir, SECRET_A).expect("open with SECRET_A should succeed")
}

fn sample(msg_id: &str, channel_id: &str, sender_did: &str, plaintext: &str) -> StoredMessage {
    StoredMessage {
        discord_message_id: msg_id.to_string(),
        channel_id: channel_id.to_string(),
        sender_discord_id: sender_did.to_string(),
        sender_osl_user_id: "alice".to_string(),
        plaintext: plaintext.to_string(),
        decrypted_at: 1,
        burned: false,
    }
}

fn all_zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|b| *b == 0)
}

/// Assert every message body on disk is shredded, bypassing every filter the
/// store applies on the read path. v4 deliberately hides message ids behind
/// private blind indexes, so these checks prove the whole table state without
/// naming a row by plaintext id.
fn assert_all_bodies_shredded(db_path: &Path) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT ciphertext, nonce, burned FROM messages")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .unwrap();

    let mut seen = 0usize;
    for row in rows {
        let (ct, nonce, burned) = row.unwrap();
        seen += 1;
        assert_eq!(burned, 1, "row {seen}: row is not marked burned");
        assert!(
            all_zero(&ct),
            "row {seen}: sealed body survives on disk ({} non-zero bytes)",
            ct.iter().filter(|b| **b != 0).count()
        );
        assert!(all_zero(&nonce), "row {seen}: AEAD nonce survives on disk");
    }
    assert!(seen > 0, "no message rows existed to verify");
}

fn count_live_bodies(db_path: &Path) -> usize {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    let mut stmt = conn.prepare("SELECT ciphertext FROM messages").unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)).unwrap();
    rows.map(|row| row.unwrap())
        .filter(|ct| ct.iter().any(|b| *b != 0))
        .count()
}

fn raw_store_artifacts(db_path: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let parent = db_path.parent().expect("database has a parent");
    let prefix = db_path
        .file_name()
        .expect("database has a filename")
        .to_string_lossy();
    let mut artifacts = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().starts_with(prefix.as_ref()))
                .unwrap_or(false)
        })
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.0.cmp(&right.0));
    artifacts
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

// ---- Defect 1: burn must be terminal ----

/// A burned row must not come back to life because the receive observer
/// re-decrypted the same Discord message and wrote it again.
///
/// `THREAT_MODEL.md:160-162` states that what burn achieves locally is that
/// "the local cached plaintext of those messages is gone". A `put` that
/// resurrects the row makes that claim false.
#[test]
fn put_cannot_resurrect_a_burned_message() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m1", "chan", "sender1", "top secret"))
        .unwrap();
    assert!(
        store.get("m1").unwrap().is_some(),
        "positive path: the message must be readable before it is burned"
    );

    store.mark_burned("m1").unwrap();
    assert!(store.get("m1").unwrap().is_none());

    // The receive observer re-decrypts the same snowflake and re-puts it.
    // This is ordinary shipping behaviour, not an attack.
    let mut again = sample("m1", "chan", "sender1", "top secret");
    again.decrypted_at = 999;
    store.put(&again).unwrap();

    assert!(
        store.get("m1").unwrap().is_none(),
        "defect 1: burn is not terminal — a plain put un-burned the row"
    );
    assert_all_bodies_shredded(&db_path);
}

/// An unknown burn is a refusal, not a successful no-op and not permission to
/// touch an arbitrary row.  The fixture is deliberately nonempty across two
/// channels and includes attachments: an implementation that maps a missing
/// blind index to a default row, deletes everything, or always returns the
/// same error cannot satisfy the exact postcondition.
#[test]
fn unknown_burn_refuses_without_mutating_a_nonempty_store() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());
    let first = sample("known-burn-a", "burn-channel-a", "sender-a", "first survives");
    let second = sample("known-burn-b", "burn-channel-b", "sender-b", "second survives");
    store.put(&first).unwrap();
    store.put(&second).unwrap();
    store
        .put_attachment("known-burn-a", "a.png", "image/png", b"A-PIXELS", None, None, None)
        .unwrap();
    store
        .put_attachment("known-burn-b", "b.png", "image/png", b"B-PIXELS", None, None, None)
        .unwrap();
    assert_eq!(store.get("known-burn-a").unwrap(), Some(first.clone()));
    assert_eq!(store.get("known-burn-b").unwrap(), Some(second.clone()));

    match store.mark_burned("missing-burn-id") {
        Err(store::StoreError::NotFound(id)) => assert_eq!(id, "missing-burn-id"),
        Err(other) => panic!("unknown burn returned the wrong error: {other}"),
        Ok(()) => panic!("unknown burn reported a successful no-op"),
    }

    assert_eq!(store.get("known-burn-a").unwrap(), Some(first));
    assert_eq!(store.get("known-burn-b").unwrap(), Some(second));
    assert_eq!(
        store.get_attachment("known-burn-a", "a.png").unwrap(),
        Some(("image/png".to_string(), b"A-PIXELS".to_vec()))
    );
    assert_eq!(
        store.get_attachment("known-burn-b", "b.png").unwrap(),
        Some(("image/png".to_string(), b"B-PIXELS".to_vec()))
    );
}

/// The same resurrection seen through the other public reader.
#[test]
fn list_by_channel_cannot_resurrect_a_burned_message() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    let burned = sample("m2", "chan", "sender1", "top secret");
    let mut survivor = sample("m2-survivor", "chan", "sender2", "must remain listed");
    survivor.decrypted_at = 2;
    store.put(&burned).unwrap();
    store.put(&survivor).unwrap();
    assert_eq!(
        store.list_by_channel("chan", 10).unwrap(),
        vec![survivor.clone(), burned.clone()],
        "positive path: both selected-channel rows must be listed before burn"
    );

    store.mark_burned("m2").unwrap();
    store
        .put(&sample("m2", "chan", "sender1", "top secret"))
        .unwrap();

    assert_eq!(
        store.list_by_channel("chan", 10).unwrap(),
        vec![survivor],
        // This rejects a list implementation that hides every row, or a burn
        // predicate that removes the whole channel instead of only m2.
        "defect 1: re-put must not resurrect m2 or delete its live sibling"
    );
}

/// A burned row must stay burned even if the writer explicitly asks for
/// `burned: false`. The struct field is caller-supplied data, not authority.
#[test]
fn put_with_burned_false_cannot_clear_the_burn_flag() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m3", "chan", "sender1", "secret"))
        .unwrap();
    assert!(store.get("m3").unwrap().is_some(), "positive path");
    let survivor = sample("m3-survivor", "other-chan", "sender2", "must remain live");
    store.put(&survivor).unwrap();
    store.mark_burned("m3").unwrap();

    let mut unburn = sample("m3", "chan", "sender1", "secret");
    unburn.burned = false;
    store.put(&unburn).unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let (burned, zeroed): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), SUM(ciphertext = zeroblob(length(ciphertext)) AND nonce = zeroblob(length(nonce))) FROM messages WHERE burned = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((burned, zeroed), (1, 1));
    // This rejects a delete-all/broad-update repair that makes a one-row
    // terminal test green without preserving unrelated live data.
    assert_eq!(store.get("m3-survivor").unwrap(), Some(survivor));
}

// ---- Defect 2: mark_burned must not report success without shredding ----

/// `put` used to accept `burned: true` and write a **live sealed body** under a
/// row that already claimed to be burned. That is the state `mark_burned` then
/// mistook for "already destroyed".
///
/// The writer must never create it: a burned flag and an intact secret must not
/// coexist in a row this store wrote.
#[test]
fn put_with_burned_true_never_writes_a_live_body() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    let mut pre_flagged = sample("m4", "chan", "sender1", "must never reach disk");
    pre_flagged.burned = true;
    store.put(&pre_flagged).unwrap();

    assert_all_bodies_shredded(&db_path);
}

/// The rows the shipped build already created: `burned = 1` over an intact
/// sealed body. `mark_burned` saw the flag, returned `Ok(())`, and destroyed
/// nothing — a destructive call reporting success over a secret it never
/// touched.
///
/// The precondition is written with raw SQL **on purpose**. It is the exact
/// on-disk state a pre-fix binary produced, and after the `put` fix above it is
/// no longer reachable through this crate's API. Constructing it any other way
/// would be testing a state that cannot occur; this one is sitting in installed
/// databases right now.
#[test]
fn mark_burned_shreds_a_row_left_live_by_an_older_build() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m4b", "chan", "sender1", "still on disk"))
        .unwrap();
    let survivor = sample("m4b-survivor", "other-chan", "sender2", "still readable");
    store.put(&survivor).unwrap();
    assert_eq!(
        count_live_bodies(&db_path),
        2,
        "positive path: target and unaffected sealed bodies must exist before burn"
    );
    drop(store);

    // Reproduce the pre-fix state: flag set, body untouched. Retain the exact
    // old body bytes so the final assertion can inspect the database, active
    // WAL/SHM, and every other same-prefix sidecar rather than trusting only
    // the logical row returned by SQLite.
    let (_target_rowid, old_ciphertext, old_nonce): (i64, Vec<u8>, Vec<u8>) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let old = conn
            .query_row("SELECT rowid, ciphertext, nonce FROM messages ORDER BY rowid LIMIT 1", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        // m4b was inserted first. Fail if that fixture assumption changes,
        // rather than manufacturing the legacy state on its live sibling.
        assert_eq!(old.0, 1, "legacy target must be m4b's row");
        assert_eq!(
            conn.execute("UPDATE messages SET burned = 1 WHERE rowid = ?1", [old.0])
                .unwrap(),
            1,
            "only the legacy target may be pre-flagged"
        );
        old
    };
    assert!(old_ciphertext.iter().any(|byte| *byte != 0));
    assert!(old_nonce.iter().any(|byte| *byte != 0));

    let store = open_a(tmp.path());
    store.mark_burned("m4b").unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let burned_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages WHERE burned = 1", [], |row| row.get(0))
        .unwrap();
    assert_eq!(burned_rows, 1, "only the pre-fix target may become terminal");
    drop(conn);
    // This rejects a repair that reports shredding the legacy target by
    // zeroing or deleting every live row in the database.
    assert_eq!(store.get("m4b-survivor").unwrap(), Some(survivor));
    let artifacts = raw_store_artifacts(&db_path);
    assert!(!artifacts.is_empty(), "store artifacts must exist");
    for (path, bytes) in &artifacts {
        assert!(
            !contains(bytes, &old_ciphertext),
            "burned sealed body survives in {}",
            path.display()
        );
        assert!(
            !contains(bytes, &old_nonce),
            "burned body nonce survives in {}",
            path.display()
        );
    }
}

/// Re-burning must remain safe and keep the already-shredded row byte-exact.
/// Schema v8 deliberately stores no wall-clock burn timestamp.
#[test]
fn repeat_mark_burned_is_safe_and_keeps_the_terminal_row_exact() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m5", "chan", "sender1", "secret"))
        .unwrap();
    let survivor = sample("m5-survivor", "other-chan", "sender2", "must stay live");
    store.put(&survivor).unwrap();
    assert!(store.get("m5").unwrap().is_some(), "positive path");
    store.mark_burned("m5").unwrap();

    let first_stub: (Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM pragma_table_info('messages') \
                  WHERE name='burned_at'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "schema v8 must not retain an exact burn timestamp column"
        );
        conn.query_row(
            "SELECT ciphertext, nonce, wrapped_key_nonce, wrapped_key, burned \
               FROM messages",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap()
    };

    store.mark_burned("m5").unwrap();

    let second_stub: (Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64) = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT ciphertext, nonce, wrapped_key_nonce, wrapped_key, burned \
               FROM messages",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap()
    };
    assert_eq!(
        first_stub, second_stub,
        "a second burn rewrote the already-terminal row"
    );
    // This rejects an idempotency branch that preserves the target stub only
    // by applying its terminal update to every row on the retry.
    assert_eq!(store.get("m5-survivor").unwrap(), Some(survivor));
}

// ---- Defect 4: legacy / unscoped attachments survive scope burns ----

/// `put_attachment` tolerates `None` scope for legacy and unknown-context
/// callers. Those rows match no scope predicate, so a scope burn leaves the
/// decrypted picture sitting in the cache, still served by `get_attachment`.
#[test]
fn scope_burn_wipes_legacy_unscoped_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("m6", "chan9", "sender1", "with a picture"))
        .unwrap();
    store
        .put_attachment("m6", "rnd.png", "image/png", b"PIXELS", None, None, None)
        .unwrap();
    store
        .put(&sample("m6-survivor", "other-chan", "sender2", "other attachment"))
        .unwrap();
    store
        .put_attachment(
            "m6-survivor",
            "rnd.png",
            "image/png",
            b"OTHER-PIXELS",
            None,
            None,
            None,
        )
        .unwrap();
    assert!(
        store.get_attachment("m6", "rnd.png").unwrap().is_some(),
        "positive path: the attachment must be cached before the burn"
    );

    store
        .wipe_wrapped_keys_in_scope("dm", "chan9", None)
        .unwrap();
    store
        .wipe_attachments_in_scope("dm", "chan9", None)
        .unwrap();

    assert!(
        store.get_attachment("m6", "rnd.png").unwrap().is_none(),
        "defect 4: a legacy unscoped attachment survived a full-scope burn"
    );
    // This rejects a wipe implementation that empties the attachment cache
    // rather than selecting the requested channel's legacy row.
    assert_eq!(
        store.get_attachment("m6-survivor", "rnd.png").unwrap(),
        Some(("image/png".to_string(), b"OTHER-PIXELS".to_vec()))
    );
}

/// Same defect, sender-scoped: burning your own messages must take your own
/// cached attachments with them even when the rows predate the scope columns.
#[test]
fn sender_scoped_burn_wipes_that_senders_legacy_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store.put(&sample("m7", "chan9", "burner", "mine")).unwrap();
    store
        .put_attachment("m7", "mine.png", "image/png", b"MINE", None, None, None)
        .unwrap();
    store.put(&sample("m7-other", "chan9", "other", "theirs")).unwrap();
    store
        .put_attachment(
            "m7-other",
            "mine.png",
            "image/png",
            b"THEIRS",
            None,
            None,
            None,
        )
        .unwrap();
    assert!(
        store.get_attachment("m7", "mine.png").unwrap().is_some(),
        "positive path"
    );

    store
        .wipe_attachments_in_scope("dm", "chan9", Some("burner"))
        .unwrap();

    assert!(
        store.get_attachment("m7", "mine.png").unwrap().is_none(),
        "defect 4: the burner's own legacy attachment survived a sender-scoped burn"
    );
    // This rejects a sender-scoped fallback that widens to every legacy row in
    // the channel when the row has no explicit sender scope.
    assert_eq!(
        store.get_attachment("m7-other", "mine.png").unwrap(),
        Some(("image/png".to_string(), b"THEIRS".to_vec()))
    );
}

/// The other direction, which matters just as much: a sender-scoped burn must
/// NOT evict somebody else's cached attachment. A fix that deletes everything
/// in the channel would pass the two tests above and silently destroy other
/// participants' data.
#[test]
fn sender_scoped_burn_spares_other_senders_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store.put(&sample("m8", "chan9", "burner", "mine")).unwrap();
    store
        .put(&sample("m9", "chan9", "someone_else", "theirs"))
        .unwrap();
    store
        .put_attachment("m8", "mine.png", "image/png", b"MINE", None, None, None)
        .unwrap();
    store
        .put_attachment("m9", "theirs.png", "image/png", b"THEIRS", None, None, None)
        .unwrap();
    assert!(store.get_attachment("m9", "theirs.png").unwrap().is_some());

    store
        .wipe_attachments_in_scope("dm", "chan9", Some("burner"))
        .unwrap();

    assert!(
        store.get_attachment("m8", "mine.png").unwrap().is_none(),
        "the burner's own attachment should be gone"
    );
    assert!(
        store.get_attachment("m9", "theirs.png").unwrap().is_some(),
        "a sender-scoped burn destroyed another participant's cached attachment"
    );
}

/// `delete_messages_in_channel` is documented as "full data destruction for a
/// channel", but it only deletes `messages` rows. The decrypted attachment
/// bytes stay in the cache and `get_attachment` still serves them, so the
/// channel's pictures survive the destruction of its text.
#[test]
fn delete_messages_in_channel_also_drops_cached_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("m10", "doomed", "sender1", "text"))
        .unwrap();
    store
        .put_attachment("m10", "pic.png", "image/png", b"PIXELS", None, None, None)
        .unwrap();
    assert!(
        store.get_attachment("m10", "pic.png").unwrap().is_some(),
        "positive path: the attachment must be cached before the delete"
    );

    let deleted = store.delete_messages_in_channel("doomed").unwrap();
    assert_eq!(
        deleted, 1,
        "positive path: the message row must have been deleted"
    );

    assert!(
        store.get_attachment("m10", "pic.png").unwrap().is_none(),
        "defect 4: channel destruction left the decrypted attachment retrievable"
    );
}

/// A channel delete must not reach into another channel's cache.
#[test]
fn delete_messages_in_channel_spares_other_channels_attachments() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("m11", "doomed", "sender1", "text"))
        .unwrap();
    store
        .put(&sample("m12", "kept", "sender1", "text"))
        .unwrap();
    store
        .put_attachment("m11", "a.png", "image/png", b"A", None, None, None)
        .unwrap();
    store
        .put_attachment("m12", "b.png", "image/png", b"B", None, None, None)
        .unwrap();
    assert!(store.get_attachment("m12", "b.png").unwrap().is_some());

    store.delete_messages_in_channel("doomed").unwrap();

    assert!(store.get_attachment("m11", "a.png").unwrap().is_none());
    assert!(
        store.get_attachment("m12", "b.png").unwrap().is_some(),
        "a channel delete evicted an unrelated channel's attachment"
    );
}

// ---- Defect 5: security-relevant metadata sits outside AEAD ----

/// The sealed v4 metadata blob authenticates attribution. An offline editor
/// who flips a byte inside `meta_ct` must get a tag failure, not a readable
/// row with forged sender or channel fields.
#[test]
fn tampered_metadata_ciphertext_is_rejected_not_returned() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m13", "chan", "sender1", "I never said this"))
        .unwrap();
    let unaffected = sample("m13-sibling", "other-chan", "sender2", "still authentic");
    store.put(&unaffected).unwrap();
    store
        .put_attachment(
            "m13-sibling",
            "sibling.png",
            "image/png",
            b"SIBLING-PIXELS",
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store.get("m13").unwrap().unwrap().sender_osl_user_id,
        "alice",
        "positive path: the honest attribution must read back first"
    );
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let (target_mid_bi, mut meta_ct): (Vec<u8>, Vec<u8>) = conn
            .query_row(
                "SELECT mid_bi, meta_ct FROM messages ORDER BY seq ASC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(
            !meta_ct.is_empty(),
            "positive path: sealed metadata must exist before tampering"
        );
        meta_ct[0] ^= 0x01;
        assert_eq!(
            conn.execute(
                "UPDATE messages SET meta_ct = ?1 WHERE mid_bi = ?2",
                rusqlite::params![meta_ct, target_mid_bi],
            )
            .unwrap(),
            1,
            "the tamper must alter exactly the target row"
        );
    }

    let store = open_a(tmp.path());
    assert!(
        matches!(store.get("m13"), Err(store::StoreError::Corrupted(_))),
        "tampered metadata must produce Corrupted, not an unrelated error or \
         a hidden row"
    );
    // A constant Corrupted response or delete-all recovery is not a valid
    // tamper defense.  The unrelated row and attachment must remain exact.
    assert_eq!(store.get("m13-sibling").unwrap(), Some(unaffected));
    assert_eq!(
        store.get_attachment("m13-sibling", "sibling.png").unwrap(),
        Some(("image/png".to_string(), b"SIBLING-PIXELS".to_vec()))
    );
}

/// Swapping sealed metadata blobs between two rows must not let an offline
/// editor re-attribute either row. The blob is authenticated with the row's
/// own blind index as AAD, so a swap must fail or hide the row, never return
/// another row's sender/channel as if it belonged here.
#[test]
fn swapped_metadata_blobs_cannot_forge_attribution() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m14a", "private", "sender1", "private business"))
        .unwrap();
    store
        .put(&sample("m14b", "public", "sender2", "public business"))
        .unwrap();
    assert_eq!(
        store.get("m14a").unwrap().unwrap().channel_id,
        "private",
        "positive path: first row must read with its honest channel"
    );
    assert_eq!(
        store.get("m14b").unwrap().unwrap().sender_discord_id,
        "sender2",
        "positive path: second row must read with its honest sender"
    );
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let rows: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = {
            let mut stmt = conn
                .prepare("SELECT mid_bi, meta_nonce, meta_ct FROM messages ORDER BY seq ASC")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(
            rows.len(),
            2,
            "positive path: the metadata swap test needs exactly two rows"
        );
        let (first_mid_bi, first_meta_nonce, first_meta_ct) = rows[0].clone();
        let (second_mid_bi, second_meta_nonce, second_meta_ct) = rows[1].clone();
        conn.execute(
            "UPDATE messages SET meta_nonce = ?1, meta_ct = ?2 WHERE mid_bi = ?3",
            rusqlite::params![second_meta_nonce, second_meta_ct, first_mid_bi],
        )
        .unwrap();
        conn.execute(
            "UPDATE messages SET meta_nonce = ?1, meta_ct = ?2 WHERE mid_bi = ?3",
            rusqlite::params![first_meta_nonce, first_meta_ct, second_mid_bi],
        )
        .unwrap();
    }

    let store = open_a(tmp.path());
    for id in ["m14a", "m14b"] {
        assert!(
            matches!(store.get(id), Err(store::StoreError::Corrupted(_))),
            "swapped metadata for {id} must produce Corrupted, not an \
             unrelated error or a hidden row"
        );
    }
}
