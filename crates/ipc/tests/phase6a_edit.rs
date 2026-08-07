//! Phase 6a: edit-side persistence tests.
//!
//! Verifies the `cmd_osl_persist_edit` IPC behaves correctly for
//! the two semantics callers care about:
//!
//! - Known id: the stored sender can upsert the row with the new
//!   plaintext, the non-plaintext metadata (channel_id, sender ids)
//!   survives, `decrypted_at` advances to the edit time, and another
//!   editor is refused.
//! - Unknown id: idempotent no-op (returns `Ok(())`). See the
//! - Known own id: row is upserted with the new plaintext, the
//!   non-plaintext metadata (channel_id, sender ids, reply parent) survives,
//!   `decrypted_at` advances to the edit time, and the edit revision advances.
//! - Unknown id: idempotent no-op (returns `Ok(None)`). See the
//!   fn-doc on `cmd_osl_persist_edit` for why we don't
//!   synthesize a row from just `(message_id, plaintext)`.
//!
//! Both tests work directly against the IPC layer (no boot.js
//! / Tauri runtime). The store is opened on a per-test
//! `TempDir` keyed off a fixed test secret so failures are
//! deterministic.

use ipc::commands::{
    cmd_osl_burn_message, cmd_osl_load_channel_history, cmd_osl_persist_edit,
    cmd_osl_persist_outbound,
};
use ipc::state::AppState;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[7u8; 32];

/// Build an `AppState` with a fresh `MessageStore` at `dir`.
/// The keystore identity / peer_map / pubkey cache are NOT
/// populated for known rows — `cmd_osl_persist_edit` only consults
/// them for unknown-row self upserts, so most tests stay store-only.
/// The peer_map / pubkey cache are NOT populated. Tests that edit known rows
/// seed only the keystore identity required to prove sender ownership.
fn fresh_state(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(dir, SECRET).expect("open store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

/// Insert a row through the raw store API so the test owns
/// `decrypted_at` (otherwise we'd be racing system time).
fn put_row(state: &AppState, msg: &StoredMessage) {
    let guard = state.message_store.lock().unwrap();
    guard.as_ref().unwrap().put(msg).expect("put");
}

fn plaintext_count(state: &AppState, channel_id: &str, plaintext: &str) -> usize {
    cmd_osl_load_channel_history(state, channel_id.to_string(), None)
        .expect("history")
        .into_iter()
        .filter(|row| row.plaintext == plaintext)
        .count()
}

#[test]
fn persist_edit_overwrites_existing_row() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());
    {
        let mut guard = state.identity.lock().unwrap();
        *guard = Some(keystore::generate_identity(
            "900000000000000003".to_string(),
        ));
    }

    // Seed a row with a fixed-old `decrypted_at` so we can
    // assert the edit path advances it (vs racing wall clock).
    let original = StoredMessage {
        discord_message_id: "msg-edit-1".to_string(),
        channel_id: "ch-edit".to_string(),
        sender_discord_id: "900000000000000003".to_string(),
        sender_osl_user_id: "liam".to_string(),
        plaintext: "before edit".to_string(),
        decrypted_at: 1_000_000_000, // 2001-09-09; well in the past
        reply_parent_id: None,
        edit_revision: 1,
        burned: false,
    };
    put_row(&state, &original);

    let edited = cmd_osl_persist_edit(
        &state,
        "msg-edit-1".to_string(),
        "after edit (fresh plaintext)".to_string(),
        None,
        "900000000000000003".to_string(),
    )
    .expect("persist_edit on known id")
    .expect("own message edit returns updated row");
    assert_eq!(edited.edit_revision, 2);

    let history =
        cmd_osl_load_channel_history(&state, "ch-edit".to_string(), None).expect("history");
    assert_eq!(history.len(), 1, "exactly one row after edit");
    let row = &history[0];
    assert_eq!(row.discord_message_id, "msg-edit-1");
    assert_eq!(row.plaintext, "after edit (fresh plaintext)");
    // Non-plaintext metadata preserved from the seed row.
    assert_eq!(row.channel_id, "ch-edit");
    assert_eq!(row.sender_discord_id, "900000000000000003");
    assert_eq!(row.sender_osl_user_id, "liam");
    assert!(!row.burned);
    // decrypted_at advanced to "now". We assert strictly
    // greater than the fixed-old seed so the test doesn't
    // depend on wall-clock granularity.
    assert!(
        row.decrypted_at > original.decrypted_at,
        "decrypted_at must advance from {} (got {})",
        original.decrypted_at,
        row.decrypted_at
    );
}

#[test]
fn persist_edit_refuses_editor_who_is_not_sender() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());

    let original = StoredMessage {
        discord_message_id: "msg-1361".to_string(),
        channel_id: "ch-1361".to_string(),
        sender_discord_id: "Ava".to_string(),
        sender_osl_user_id: "Ava".to_string(),
        plaintext: "MAPLE-4172".to_string(),
        decrypted_at: 1_000_000_000,
        burned: false,
    };
    put_row(&state, &original);

    let before_edit_count = plaintext_count(&state, "ch-1361", "PINE-4172");
    println!("before_edit_count={before_edit_count}");
    assert_eq!(before_edit_count, 0, "PINE-4172 count before edit");

    cmd_osl_persist_edit(
        &state,
        "msg-1361".to_string(),
        "PINE-4172".to_string(),
        None,
        "Ava".to_string(),
    )
    .expect("Ava is the sender and can edit");

    let after_ava_history =
        cmd_osl_load_channel_history(&state, "ch-1361".to_string(), None).expect("history");
    assert_eq!(after_ava_history.len(), 1, "one row after Ava edit");
    println!("after_ava_plaintext={}", after_ava_history[0].plaintext);
    assert_eq!(after_ava_history[0].plaintext, "PINE-4172");
    let after_ava_edit_count = plaintext_count(&state, "ch-1361", "PINE-4172");
    println!("after_ava_edit_count={after_ava_edit_count}");
    assert_eq!(after_ava_edit_count, 1, "PINE-4172 count after Ava edit");

    let ben_err = cmd_osl_persist_edit(
        &state,
        "msg-1361".to_string(),
        "CEDAR-4172".to_string(),
        None,
        "Ben".to_string(),
    )
    .expect_err("Ben is not the sender and must be refused");
    assert_eq!(
        ben_err,
        "OSL: persist_edit refused: only the sender can edit"
    );
    println!("ben_refusal={ben_err}");

    let after_ben_history =
        cmd_osl_load_channel_history(&state, "ch-1361".to_string(), None).expect("history");
    assert_eq!(after_ben_history.len(), 1, "one row after Ben refusal");
    println!("after_ben_plaintext={}", after_ben_history[0].plaintext);
    assert_eq!(after_ben_history[0].plaintext, "PINE-4172");
    let after_ben_edit_count = plaintext_count(&state, "ch-1361", "PINE-4172");
    println!("after_ben_edit_count={after_ben_edit_count}");
    assert_eq!(
        after_ben_edit_count, 1,
        "PINE-4172 count stays 1 after Ben refusal"
    );
fn task_1359_direct_commands_return_reply_parent_id_and_sender_edit_revision() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());
    {
        let mut guard = state.identity.lock().unwrap();
        *guard = Some(keystore::generate_identity("sender-1359".to_string()));
    }

    let written = ipc::commands::cmd_osl_persist_outbound(
        &state,
        "channel-1359".to_string(),
        "message-1359".to_string(),
        "reply plaintext".to_string(),
        Some("parent-1359".to_string()),
    )
    .expect("reply command succeeds")
    .expect("reply command returns stored row");

    let reply_parent_id = written.reply_parent_id.as_deref().unwrap_or("");
    assert_eq!(reply_parent_id, "parent-1359");
    println!("task_1359_reply_parent_id_count=1");
    println!("task_1359_reply_parent_id={reply_parent_id}");

    let edited = cmd_osl_persist_edit(
        &state,
        "message-1359".to_string(),
        "sender edit plaintext".to_string(),
        None,
    )
    .expect("sender edit command succeeds")
    .expect("sender edit command returns stored row");

    assert_eq!(edited.sender_discord_id, "sender-1359");
    assert_eq!(edited.edit_revision, 2);
    println!("task_1359_sender_edit_revision_count=1");
    println!("task_1359_sender_edit_revision={}", edited.edit_revision);
}

#[test]
fn task_1360_direct_actions_create_one_reply_and_one_edit_in_each_available_conversation_kind() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());
    {
        let mut guard = state.identity.lock().unwrap();
        *guard = Some(keystore::generate_identity("sender-1360".to_string()));
    }

    let conversation_kinds = [
        ("direct", "direct-1360-channel"),
        ("group", "group-1360-channel"),
        ("channel", "channel-1360-channel"),
        // Discord threads arrive at this backend path as their selected
        // message channel id, so the direct persistence action covers them by
        // writing the thread channel id independently from a server channel.
        ("thread", "thread-1360-channel"),
    ];

    let mut reply_count = 0usize;
    let mut edit_count = 0usize;
    let mut covered_kinds = Vec::new();

    for (kind, channel_id) in conversation_kinds {
        let message_id = format!("{kind}-1360-message");
        let reply_parent_id = format!("{kind}-1360-parent");
        let reply_plaintext = format!("{kind} reply plaintext");
        let edit_plaintext = format!("{kind} edit plaintext");

        let written = cmd_osl_persist_outbound(
            &state,
            channel_id.to_string(),
            message_id.clone(),
            reply_plaintext,
            Some(reply_parent_id.clone()),
        )
        .expect("reply action succeeds")
        .expect("reply action returns stored row");
        assert_eq!(written.channel_id, channel_id);
        assert_eq!(
            written.reply_parent_id.as_deref(),
            Some(reply_parent_id.as_str())
        );
        assert_eq!(written.edit_revision, 1);
        reply_count += 1;

        let edited = cmd_osl_persist_edit(&state, message_id.clone(), edit_plaintext.clone(), None)
            .expect("edit action succeeds")
            .expect("edit action returns stored row");
        assert_eq!(edited.channel_id, channel_id);
        assert_eq!(edited.sender_discord_id, "sender-1360");
        assert_eq!(edited.plaintext, edit_plaintext);
        assert_eq!(
            edited.reply_parent_id.as_deref(),
            Some(reply_parent_id.as_str())
        );
        assert_eq!(edited.edit_revision, 2);
        edit_count += 1;

        let history = cmd_osl_load_channel_history(&state, channel_id.to_string(), Some(10))
            .expect("conversation history loads");
        assert_eq!(
            history.len(),
            1,
            "one persisted message for {kind} conversation"
        );
        assert_eq!(history[0].discord_message_id, message_id);
        assert_eq!(
            history[0].reply_parent_id.as_deref(),
            Some(reply_parent_id.as_str())
        );
        assert_eq!(history[0].edit_revision, 2);
        covered_kinds.push(kind);
    }

    assert_eq!(reply_count, 4);
    assert_eq!(edit_count, 4);
    assert_eq!(covered_kinds, ["direct", "group", "channel", "thread"]);
    println!(
        "task_1360_available_conversation_kinds={}",
        covered_kinds.join(",")
    );
    println!("task_1360_reply_action_count={reply_count}");
    println!("task_1360_edit_action_count={edit_count}");
}

#[test]
fn persist_edit_for_unknown_id_without_channel_is_idempotent_no_op() {
    // When `channel_id` is None we cannot construct a complete row
    // (sender metadata is unrecoverable for arbitrary message ids),
    // so the conservative no-op is preserved — matches the historical
    // 2-arg behaviour exactly.
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());

    cmd_osl_persist_edit(
        &state,
        "never-seen-this-id".to_string(),
        "some plaintext".to_string(),
        None,
        "editor".to_string(),
    )
    .expect("persist_edit on unknown id (no channel) is Ok");

    // No row should have been synthesised.
    let history =
        cmd_osl_load_channel_history(&state, "any-channel".to_string(), None).expect("history");
    assert!(
        history.is_empty(),
        "unknown-id persist_edit (no channel) must not create rows (got {history:?})"
    );

    // Repeat the same call: still Ok (idempotent).
    cmd_osl_persist_edit(
        &state,
        "never-seen-this-id".to_string(),
        "different plaintext".to_string(),
        None,
        "editor".to_string(),
    )
    .expect("persist_edit unknown id second call still Ok");
}

#[test]
fn persist_edit_for_unknown_id_with_channel_upserts_as_self() {
    // Probe-2 fix: when boot.js passes channel_id (the common case
    // after the outbound-persistence rollout), persist_edit upserts
    // a fresh self-sender row so editing an old pre-fix message that
    // was never persisted creates the row instead of silently dropping.
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());
    // Seed an identity so the self-upsert has a user_id to write.
    {
        let mut guard = state.identity.lock().unwrap();
        *guard = Some(keystore::generate_identity("1111".to_string()));
    }

    cmd_osl_persist_edit(
        &state,
        "fresh-id-after-edit".to_string(),
        "the typed plaintext".to_string(),
        Some("ch-upsert".to_string()),
        "1111".to_string(),
    )
    .expect("persist_edit upsert with channel_id is Ok");

    let history =
        cmd_osl_load_channel_history(&state, "ch-upsert".to_string(), None).expect("history");
    assert_eq!(history.len(), 1, "exactly one row after upsert");
    let row = &history[0];
    assert_eq!(row.discord_message_id, "fresh-id-after-edit");
    assert_eq!(row.plaintext, "the typed plaintext");
    assert_eq!(row.channel_id, "ch-upsert");
    assert_eq!(row.sender_discord_id, "1111");
    assert_eq!(row.sender_osl_user_id, "1111");
    assert!(!row.burned);
}

#[test]
fn persist_edit_with_store_disabled_is_ok() {
    // Match the load-history / burn convention: a disabled
    // store (open failed at bootstrap) doesn't surface as an
    // error — boot.js would otherwise show a toast on every
    // edit, which would be a worse UX than silently skipping
    // persistence.
    let state = AppState::new();
    cmd_osl_persist_edit(
        &state,
        "any-id".to_string(),
        "any plaintext".to_string(),
        None,
        "any-editor".to_string(),
    )
    .expect("persist_edit on disabled store is Ok no-op");
}

#[test]
fn persist_edit_after_burn_is_no_op() {
    // Burned rows are filtered from `store.get`, so the persist
    // path takes the unknown-id branch and no-ops. Subsequent
    // history reads continue to exclude the burned id.
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());
    put_row(
        &state,
        &StoredMessage {
            discord_message_id: "burn-then-edit".to_string(),
            channel_id: "ch-z".to_string(),
            sender_discord_id: "did".to_string(),
            sender_osl_user_id: "uid".to_string(),
            plaintext: "before burn".to_string(),
            decrypted_at: 1,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        },
    );
    cmd_osl_burn_message(&state, "burn-then-edit".to_string()).unwrap();

    cmd_osl_persist_edit(
        &state,
        "burn-then-edit".to_string(),
        "tried to edit after burn".to_string(),
        None,
        "did".to_string(),
    )
    .expect("persist_edit on burned row is Ok no-op");

    let history = cmd_osl_load_channel_history(&state, "ch-z".to_string(), None).unwrap();
    assert!(history.is_empty(), "burned row must not resurface");
}
