//! Phase 5b2: persistence side-effect tests for the decrypt path.
//!
//! Mirrors the harness in `phase5_production_flow.rs` (warm pubkey
//! cache, fixed peer_map, two-peer encrypt/decrypt) and adds an
//! at-rest [`store::MessageStore`] under a per-test [`tempfile::TempDir`].
//! The store is opened with the receiver's identity secret bytes —
//! same key derivation `bootstrap.rs` uses on Windows — so a write
//! by the decrypt path round-trips through `cmd_osl_load_channel_history`
//! exactly as it would for the user.
//!
//! What we're trying to catch:
//!
//! - The `_with_id` variant accepts `Some(id)` and persists.
//! - The same variant accepts `None` and does NOT persist (Phase 5b3
//!   wires boot.js to start sending the id; until then the existing
//!   call shape stays a no-op against the store).
//! - `cmd_osl_load_channel_history` returns an empty list when the
//!   store is `None` (open failed at bootstrap) instead of erroring —
//!   the JS side renders that as "no history" rather than a toast.
//! - `cmd_osl_burn_message` removes a row from subsequent history
//!   reads. (The crypto-shred property is the store crate's concern;
//!   here we only verify the read-side filter.)

use ipc::commands::{
    cmd_osl_burn_message, cmd_osl_decrypt_message_with_id, cmd_osl_load_channel_history,
    encrypt_osl_phase4_to_pubkeys,
};
use ipc::state::AppState;
use keystore::{generate_identity, Identity};
use store::MessageStore;
use tempfile::TempDir;

const LIAM_DISCORD_ID: &str = "900000000000000003";
const HENRY_DISCORD_ID: &str = "900000000000000001";

/// Pre-warm both peers' pubkeys so cmd_osl_decrypt_message_with_id
/// hits the cache (no keyserver round-trip needed for these tests).
fn warm_cache(state: &AppState, a: &Identity, b: &Identity) {
    state
        .sender_pubkey_cache
        .insert(a.user_id.clone(), a.x25519_public);
    state
        .sender_pubkey_cache
        .insert(b.user_id.clone(), b.x25519_public);
}

/// Install the canonical liam/henry peer_map both peers carry on
/// disk in production.
fn install_peer_map(state: &AppState) {
    let mut pm = state.peer_map.lock().unwrap();
    pm.insert(
        LIAM_DISCORD_ID.to_string(),
        ipc::peer_map::legacy_entry("liam"),
    );
    pm.insert(
        HENRY_DISCORD_ID.to_string(),
        ipc::peer_map::legacy_entry("henry"),
    );
}

/// Build an `AppState` with `loaded` as the loaded identity, the
/// peer_map populated, the sender pubkey cache pre-warmed for both
/// peers, and a fresh [`MessageStore`] under `dir` keyed off the
/// loaded identity's x25519 secret bytes (same derivation
/// `bootstrap::open_message_store` uses).
fn fresh_state_with_store(
    dir: &std::path::Path,
    loaded: Identity,
    counterpart: &Identity,
) -> AppState {
    let state = AppState::new();
    warm_cache(&state, &loaded, counterpart);
    install_peer_map(&state);
    let secret_bytes: [u8; 32] = *loaded.x25519_secret.as_bytes();
    *state.identity.lock().unwrap() = Some(loaded);
    let store = MessageStore::open(dir, &secret_bytes).expect("open store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

/// Same shape, but leaves `state.message_store` as `None` to
/// exercise the persistence-disabled paths.
fn fresh_state_without_store(loaded: Identity, counterpart: &Identity) -> AppState {
    let state = AppState::new();
    warm_cache(&state, &loaded, counterpart);
    install_peer_map(&state);
    *state.identity.lock().unwrap() = Some(loaded);
    state
}

/// Sender encrypts to a single counterpart; returns the
/// `DPC0::<base64>` cover string the JS hook would put on the wire.
fn encrypt_one_to_one(sender: &Identity, recipient: &Identity, plaintext: &str) -> String {
    encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &[recipient.x25519_public], plaintext)
        .expect("encrypt should succeed for valid inputs")
}

// ---- decrypt-then-load-history ----

#[test]
fn decrypt_with_id_persists_and_history_returns_it() {
    let tmp = TempDir::new().unwrap();
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    // Henry sends to Liam twice; build covers up-front before
    // moving `liam` into state (Identity is not Clone).
    let cover_with_id = encrypt_one_to_one(&henry, &liam, "from henry");
    let cover_no_id = encrypt_one_to_one(&henry, &liam, "second");

    let state = fresh_state_with_store(tmp.path(), liam, &henry);

    let plaintext = cmd_osl_decrypt_message_with_id(
        &state,
        Some("msg-1".to_string()),
        "ch-test".to_string(),
        HENRY_DISCORD_ID.to_string(),
        cover_with_id,
    )
    .expect("liam should decrypt henry's message");
    assert_eq!(plaintext, "from henry");

    // History should now contain exactly that message.
    let history =
        cmd_osl_load_channel_history(&state, "ch-test".to_string(), None).expect("history read");
    assert_eq!(history.len(), 1);
    let row = &history[0];
    assert_eq!(row.discord_message_id, "msg-1");
    assert_eq!(row.channel_id, "ch-test");
    assert_eq!(row.sender_discord_id, HENRY_DISCORD_ID);
    assert_eq!(row.sender_osl_user_id, "henry");
    assert_eq!(row.plaintext, "from henry");
    assert!(!row.burned);

    // A different channel returns empty (channel scope filter).
    let other =
        cmd_osl_load_channel_history(&state, "ch-other".to_string(), None).expect("history read");
    assert!(other.is_empty(), "wrong channel must not see msg-1");

    // None message_id is the Phase 5b3-pre call shape — must NOT
    // add a second row.
    let pt2 = cmd_osl_decrypt_message_with_id(
        &state,
        None,
        "ch-test".to_string(),
        HENRY_DISCORD_ID.to_string(),
        cover_no_id,
    )
    .expect("decrypt without id");
    assert_eq!(pt2, "second");
    let after =
        cmd_osl_load_channel_history(&state, "ch-test".to_string(), None).expect("history reread");
    assert_eq!(
        after.len(),
        1,
        "None message_id must not have produced a row (got {after:?})",
    );
}

// ---- decrypt-with-store-disabled ----

#[test]
fn decrypt_succeeds_and_history_empty_when_store_disabled() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    let cover = encrypt_one_to_one(&henry, &liam, "no store");
    let state = fresh_state_without_store(liam, &henry);

    // Even with Some(id), an absent store must not regress decrypt.
    let plaintext = cmd_osl_decrypt_message_with_id(
        &state,
        Some("msg-disabled".to_string()),
        "ch".to_string(),
        HENRY_DISCORD_ID.to_string(),
        cover,
    )
    .expect("decrypt must succeed regardless of store presence");
    assert_eq!(plaintext, "no store");

    // History must be Ok(empty) — not an error — so the JS hook
    // renders "no history" instead of a toast.
    let history = cmd_osl_load_channel_history(&state, "ch".to_string(), None)
        .expect("history read on store-disabled returns Ok");
    assert!(history.is_empty(), "store-disabled history must be empty");

    // Burn against a disabled store is a no-op (Ok).
    cmd_osl_burn_message(&state, "any-id".to_string()).expect("burn on store-disabled is Ok no-op");
}

// ---- burn-then-load ----

#[test]
fn burn_removes_row_from_subsequent_history_reads() {
    let tmp = TempDir::new().unwrap();
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    // Two messages from henry to liam in the same channel.
    let cover_a = encrypt_one_to_one(&henry, &liam, "first");
    let cover_b = encrypt_one_to_one(&henry, &liam, "second");
    let state = fresh_state_with_store(tmp.path(), liam, &henry);

    cmd_osl_decrypt_message_with_id(
        &state,
        Some("a".to_string()),
        "ch-burn".to_string(),
        HENRY_DISCORD_ID.to_string(),
        cover_a,
    )
    .unwrap();
    cmd_osl_decrypt_message_with_id(
        &state,
        Some("b".to_string()),
        "ch-burn".to_string(),
        HENRY_DISCORD_ID.to_string(),
        cover_b,
    )
    .unwrap();

    let before = cmd_osl_load_channel_history(&state, "ch-burn".to_string(), None).unwrap();
    assert_eq!(before.len(), 2);

    // Burn the first message.
    cmd_osl_burn_message(&state, "a".to_string()).expect("burn known id");

    let after = cmd_osl_load_channel_history(&state, "ch-burn".to_string(), None).unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].discord_message_id, "b");
    assert_eq!(after[0].plaintext, "second");

    // Idempotent: re-burning the same id is Ok.
    cmd_osl_burn_message(&state, "a".to_string()).expect("re-burn is Ok");

    // Burning an unknown id is also Ok (NotFound → Ok by spec).
    cmd_osl_burn_message(&state, "never-existed".to_string()).expect("unknown-id burn is Ok no-op");
}
