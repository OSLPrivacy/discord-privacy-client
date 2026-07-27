//! Regression tests for the six store defects raised in
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

use std::path::Path;
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

/// Read `(ciphertext, nonce, burned)` straight off disk, bypassing every
/// filter the store applies on the read path. A burn that only hides a row
/// from `get` is not a burn.
fn raw_row(db_path: &Path, msg_id: &str) -> (Vec<u8>, Vec<u8>, i64) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.query_row(
        "SELECT ciphertext, nonce, burned FROM messages WHERE discord_message_id = ?1",
        rusqlite::params![msg_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .unwrap()
}

fn assert_shredded(db_path: &Path, msg_id: &str, context: &str) {
    let (ct, nonce, burned) = raw_row(db_path, msg_id);
    assert_eq!(burned, 1, "{context}: row is not marked burned");
    assert!(
        ct.iter().all(|b| *b == 0),
        "{context}: sealed body survives on disk ({} non-zero bytes)",
        ct.iter().filter(|b| **b != 0).count()
    );
    assert!(
        nonce.iter().all(|b| *b == 0),
        "{context}: AEAD nonce survives on disk"
    );
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
    assert_shredded(&db_path, "m1", "defect 1: after re-put");
}

/// The same resurrection seen through the other public reader.
#[test]
fn list_by_channel_cannot_resurrect_a_burned_message() {
    let tmp = TempDir::new().unwrap();
    let store = open_a(tmp.path());

    store
        .put(&sample("m2", "chan", "sender1", "top secret"))
        .unwrap();
    assert_eq!(
        store.list_by_channel("chan", 10).unwrap().len(),
        1,
        "positive path: the message must be listed before it is burned"
    );

    store.mark_burned("m2").unwrap();
    store
        .put(&sample("m2", "chan", "sender1", "top secret"))
        .unwrap();

    assert!(
        store.list_by_channel("chan", 10).unwrap().is_empty(),
        "defect 1: a burned message reappeared in list_by_channel after a re-put"
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
    store.mark_burned("m3").unwrap();

    let mut unburn = sample("m3", "chan", "sender1", "secret");
    unburn.burned = false;
    store.put(&unburn).unwrap();

    let (_, _, burned) = raw_row(&db_path, "m3");
    assert_eq!(
        burned, 1,
        "defect 1: caller-supplied burned=false cleared a burn"
    );
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

    assert_shredded(
        &db_path,
        "m4",
        "defect 2: put wrote a live sealed body under burned = 1",
    );
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
    let (ct_before, _, _) = raw_row(&db_path, "m4b");
    assert!(
        ct_before.iter().any(|b| *b != 0),
        "positive path: a sealed body must exist on disk before the burn"
    );
    drop(store);

    // Reproduce the pre-fix state: flag set, body untouched.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE messages SET burned = 1 WHERE discord_message_id = 'm4b'",
            [],
        )
        .unwrap();
    }

    let store = open_a(tmp.path());
    store.mark_burned("m4b").unwrap();

    assert_shredded(
        &db_path,
        "m4b",
        "defect 2: mark_burned returned Ok while the sealed body survived",
    );
}

/// Re-burning must remain safe and must not restamp `burned_at`, or an old
/// destruction starts looking fresh in the audit trail.
#[test]
fn repeat_mark_burned_is_safe_and_keeps_the_original_burn_time() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m5", "chan", "sender1", "secret"))
        .unwrap();
    assert!(store.get("m5").unwrap().is_some(), "positive path");
    store.mark_burned("m5").unwrap();

    let first_burn_at: Option<i64> = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT burned_at FROM messages WHERE discord_message_id = 'm5'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert!(
        first_burn_at.is_some(),
        "positive path: burned_at was stamped"
    );

    std::thread::sleep(std::time::Duration::from_millis(1100));
    store.mark_burned("m5").unwrap();

    let second_burn_at: Option<i64> = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.query_row(
            "SELECT burned_at FROM messages WHERE discord_message_id = 'm5'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        first_burn_at, second_burn_at,
        "a second burn restamped burned_at and made an old destruction look fresh"
    );
    assert_shredded(&db_path, "m5", "after repeat burn");
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

// ---- Upgrade / downgrade behaviour for the metadata authenticator ----

/// Turn a store written by this build back into the shape a pre-fix build
/// left behind: rows with no authenticator, and no strict marker.
///
/// This is the state sitting in every already-installed database, and it is
/// the state an upgrade must not destroy.
fn degrade_to_legacy(db_path: &Path) {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute("UPDATE messages SET meta_tag = NULL", [])
        .unwrap();
    conn.execute("DELETE FROM _meta WHERE key = 'meta_auth_strict'", [])
        .unwrap();
}

/// An installed user's history predates the authenticator. Upgrading must keep
/// every one of those messages readable — a privacy tool that eats the user's
/// own history on update has done more damage than the defect it fixed.
#[test]
fn legacy_untagged_rows_survive_the_upgrade_and_stay_readable() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");

    let store = open_a(tmp.path());
    store
        .put(&sample("old1", "chan", "sender1", "history worth keeping"))
        .unwrap();
    store
        .put(&sample("old2", "chan", "sender1", "more history"))
        .unwrap();
    assert_eq!(store.list_by_channel("chan", 10).unwrap().len(), 2);
    drop(store);

    degrade_to_legacy(&db_path);

    // Re-open: this is the upgrade.
    let store = open_a(tmp.path());
    assert_eq!(
        store.get("old1").unwrap().unwrap().plaintext,
        "history worth keeping",
        "upgrade orphaned a legacy row"
    );
    assert_eq!(
        store.list_by_channel("chan", 10).unwrap().len(),
        2,
        "upgrade orphaned legacy history from the channel view"
    );
}

/// The authenticator column is additive and the schema version must stay put,
/// because bumping it is what makes an older binary refuse to open the file.
/// This is the mechanical basis of the documented downgrade behaviour.
#[test]
fn adding_the_authenticator_does_not_bump_the_schema_version() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());
    store.put(&sample("v", "chan", "sender1", "body")).unwrap();
    drop(store);

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    let version: Vec<u8> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(version.try_into().unwrap()),
        3,
        "the metadata authenticator bumped the schema version, which makes an \
         older binary refuse to open an upgraded database"
    );

    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(messages)").unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert!(
        cols.iter().any(|c| c == "meta_tag"),
        "positive path: the authenticator column must actually exist"
    );
}

/// Once a store has no legacy rows left, it latches strict and an untagged row
/// is refused. Without this, stripping the authenticator would be a trivial
/// bypass of the whole defect-5 fix.
#[test]
fn a_store_with_no_legacy_rows_refuses_an_untagged_row() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");

    let store = open_a(tmp.path());
    store.put(&sample("s1", "chan", "sender1", "body")).unwrap();
    assert!(
        store.get("s1").unwrap().is_some(),
        "positive path: the row reads back before the tag is stripped"
    );
    drop(store);

    // Strip only the authenticator, leaving the strict marker in place — this
    // is the downgrade an offline editor would attempt.
    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE messages SET meta_tag = NULL WHERE discord_message_id = 's1'",
            [],
        )
        .unwrap();
    }

    let store = open_a(tmp.path());
    assert!(
        store.get("s1").is_err(),
        "a strict store served a row whose metadata authenticator had been stripped"
    );
}

/// A database that still holds legacy rows must stay lenient, or the fix above
/// would refuse the very history it is meant to preserve.
#[test]
fn a_store_with_legacy_rows_stays_lenient_until_they_drain() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");

    let store = open_a(tmp.path());
    store
        .put(&sample("leg", "chan", "sender1", "legacy body"))
        .unwrap();
    drop(store);
    degrade_to_legacy(&db_path);

    let store = open_a(tmp.path());
    assert!(
        store.get("leg").unwrap().is_some(),
        "legacy row must still read"
    );
    // Drain the legacy row, then re-open: the store should now latch strict.
    store.mark_burned("leg").unwrap();
    drop(store);

    let store = open_a(tmp.path());
    store
        .put(&sample("new", "chan", "sender1", "new body"))
        .unwrap();
    assert!(store.get("new").unwrap().is_some());
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let marker: Option<Vec<u8>> = conn
            .query_row(
                "SELECT value FROM _meta WHERE key = 'meta_auth_strict'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert!(
            marker.is_some(),
            "the store never latched strict after its legacy rows drained"
        );
    }
}

// ---- Defect 5: security-relevant metadata sits outside AEAD ----

/// The AAD is only `discord_message_id`, so `sender_osl_user_id`,
/// `sender_discord_id`, `channel_id` and `decrypted_at` are unauthenticated.
/// Someone with write access to the file can re-attribute a message to another
/// person and the AEAD tag still verifies.
#[test]
fn tampered_sender_attribution_is_rejected_not_returned() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m13", "chan", "sender1", "I never said this"))
        .unwrap();
    assert_eq!(
        store.get("m13").unwrap().unwrap().sender_osl_user_id,
        "alice",
        "positive path: the honest attribution must read back first"
    );
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE messages SET sender_osl_user_id = 'mallory' \
             WHERE discord_message_id = 'm13'",
            [],
        )
        .unwrap();
    }

    let store = open_a(tmp.path());
    match store.get("m13") {
        Err(_) => {}
        Ok(None) => {}
        Ok(Some(msg)) => panic!(
            "defect 5: forged attribution accepted — store returned sender_osl_user_id={:?} \
             for a row an offline editor rewrote",
            msg.sender_osl_user_id
        ),
    }
}

/// Moving a row into another channel is the same forgery through a different
/// column, and it is the one that changes who sees the message.
#[test]
fn tampered_channel_id_is_rejected_not_returned() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("messages.sqlite");
    let store = open_a(tmp.path());

    store
        .put(&sample("m14", "private", "sender1", "private business"))
        .unwrap();
    assert_eq!(
        store.list_by_channel("private", 10).unwrap().len(),
        1,
        "positive path"
    );
    drop(store);

    {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE messages SET channel_id = 'public' WHERE discord_message_id = 'm14'",
            [],
        )
        .unwrap();
    }

    let store = open_a(tmp.path());
    let listed = store.list_by_channel("public", 10);
    match listed {
        Err(_) => {}
        Ok(rows) => assert!(
            rows.is_empty(),
            "defect 5: a row relocated by an offline editor was served as \
             belonging to the attacker's channel"
        ),
    }
}
