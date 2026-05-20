//! Phase 6a: edit-side persistence tests.
//!
//! Verifies the `cmd_osl_persist_edit` IPC behaves correctly for
//! the two semantics callers care about:
//!
//! - Known id: row is upserted with the new plaintext, the
//!   non-plaintext metadata (channel_id, sender ids) survives,
//!   `decrypted_at` advances to the edit time.
//! - Unknown id: idempotent no-op (returns `Ok(())`). See the
//!   fn-doc on `cmd_osl_persist_edit` for why we don't
//!   synthesize a row from just `(message_id, plaintext)`.
//!
//! Both tests work directly against the IPC layer (no boot.js
//! / Tauri runtime). The store is opened on a per-test
//! `TempDir` keyed off a fixed test secret so failures are
//! deterministic.

use ipc::commands::{cmd_osl_burn_message, cmd_osl_load_channel_history, cmd_osl_persist_edit};
use ipc::state::AppState;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[7u8; 32];

/// Build an `AppState` with a fresh `MessageStore` at `dir`.
/// The keystore identity / peer_map / pubkey cache are NOT
/// populated — `cmd_osl_persist_edit` does not consult them
/// (it only goes through the store), so the test stays a
/// store-only integration test.
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

#[test]
fn persist_edit_overwrites_existing_row() {
    let tmp = TempDir::new().unwrap();
    let state = fresh_state(tmp.path());

    // Seed a row with a fixed-old `decrypted_at` so we can
    // assert the edit path advances it (vs racing wall clock).
    let original = StoredMessage {
        discord_message_id: "msg-edit-1".to_string(),
        channel_id: "ch-edit".to_string(),
        sender_discord_id: "1477008451799482419".to_string(),
        sender_osl_user_id: "liam".to_string(),
        plaintext: "before edit".to_string(),
        decrypted_at: 1_000_000_000, // 2001-09-09; well in the past
        burned: false,
    };
    put_row(&state, &original);

    cmd_osl_persist_edit(
        &state,
        "msg-edit-1".to_string(),
        "after edit (fresh plaintext)".to_string(),
        None,
    )
    .expect("persist_edit on known id");

    let history =
        cmd_osl_load_channel_history(&state, "ch-edit".to_string(), None).expect("history");
    assert_eq!(history.len(), 1, "exactly one row after edit");
    let row = &history[0];
    assert_eq!(row.discord_message_id, "msg-edit-1");
    assert_eq!(row.plaintext, "after edit (fresh plaintext)");
    // Non-plaintext metadata preserved from the seed row.
    assert_eq!(row.channel_id, "ch-edit");
    assert_eq!(row.sender_discord_id, "1477008451799482419");
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
            burned: false,
        },
    );
    cmd_osl_burn_message(&state, "burn-then-edit".to_string()).unwrap();

    cmd_osl_persist_edit(
        &state,
        "burn-then-edit".to_string(),
        "tried to edit after burn".to_string(),
        None,
    )
    .expect("persist_edit on burned row is Ok no-op");

    let history = cmd_osl_load_channel_history(&state, "ch-z".to_string(), None).unwrap();
    assert!(history.is_empty(), "burned row must not resurface");
}
