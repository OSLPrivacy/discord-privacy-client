//! Phase 7a fresh-start integration test.
//!
//! Spec: `docs/phase-7-design.md` §1, §3.6.
//!
//! Sets up dummy state in every file the fresh-start path touches,
//! invokes `cmd_osl_fresh_start`, then asserts:
//!
//! - All on-disk files exist with the expected empty/fresh shape.
//! - The new `identity.json` carries a different keypair from the
//!   pre-wipe state (proves we generated fresh, not just left it
//!   in place).
//! - The SQLite store and its WAL/SHM siblings are wiped.

use ipc::fresh_start::cmd_osl_fresh_start;
use ipc::peer_map::load_peer_map_from_path;
use ipc::whitelist_state::load_whitelist_state_from_path;
use keystore::{load_identity, save_identity, select_best_sealer};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_fresh_start_wipes_and_regenerates() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    // ---- Set up "before" state ----

    // Pre-existing identity (we'll compare keypairs against the
    // fresh one below to prove regeneration happened).
    let sealer = select_best_sealer();
    let prior_identity = keystore::generate_identity("prior-user".to_string());
    let prior_pub = *prior_identity.x25519_public.as_bytes();
    save_identity(&dir.join("identity.json"), &prior_identity, sealer.as_ref()).unwrap();

    // peer_map.json with one entry.
    fs::write(
        dir.join("peer_map.json"),
        r#"{"900000000000000003":"liam"}"#,
    )
    .unwrap();

    // channels.json with one channel.
    fs::write(
        dir.join("channels.json"),
        r#"{"channels":{"123":{"recipients":["liam"]}}}"#,
    )
    .unwrap();

    // whitelist_state.json with one scope.
    fs::write(
        dir.join("whitelist_state.json"),
        r#"{"dm:900000000000000003":{"encrypt_toggle":true,"auto_enabled":true}}"#,
    )
    .unwrap();

    // pending_invitations.json with one entry.
    fs::write(
        dir.join("pending_invitations.json"),
        r#"{"from_liam_dm":{"from":"liam","scope":"dm","received_at":"2026-05-09T12:00:00Z","status":"pending"}}"#,
    )
    .unwrap();

    // store dir + sqlite + WAL/SHM placeholder files. The real
    // store would create them via MessageStore::open, but for the
    // wipe test we just need files at the expected paths.
    fs::create_dir_all(dir.join("store")).unwrap();
    fs::write(dir.join("store/messages.sqlite"), b"fake db bytes").unwrap();
    fs::write(dir.join("store/messages.sqlite-wal"), b"fake wal").unwrap();
    fs::write(dir.join("store/messages.sqlite-shm"), b"fake shm").unwrap();

    // ---- Invoke fresh_start ----

    let new_identity = cmd_osl_fresh_start(dir, "fresh-user".to_string()).expect("fresh_start");

    // ---- Post-wipe assertions ----

    // SQLite + WAL/SHM all gone.
    assert!(
        !dir.join("store/messages.sqlite").exists(),
        "messages.sqlite must be wiped"
    );
    assert!(
        !dir.join("store/messages.sqlite-wal").exists(),
        "wal must be wiped"
    );
    assert!(
        !dir.join("store/messages.sqlite-shm").exists(),
        "shm must be wiped"
    );

    // Fresh schemas present + empty.
    let peer_map = load_peer_map_from_path(&dir.join("peer_map.json")).expect("peer_map");
    assert!(
        peer_map.is_empty(),
        "peer_map.json must be empty after wipe"
    );

    let channels_body = fs::read_to_string(dir.join("channels.json")).unwrap();
    assert!(
        channels_body.contains("\"channels\"") && !channels_body.contains("liam"),
        "channels.json must be reset to empty map (got {channels_body})"
    );

    let whitelist =
        load_whitelist_state_from_path(&dir.join("whitelist_state.json")).expect("whitelist_state");
    assert!(whitelist.is_empty(), "whitelist_state must be empty");

    // 9-C1: pending_invitations.json is unconditionally deleted by
    // bootstrap. Fresh-start's own remove-if-present pass also nukes
    // it, and we no longer write a stub.
    assert!(
        !dir.join("pending_invitations.json").exists(),
        "pending_invitations.json must be absent after C1 fresh_start"
    );

    // identity.json present, new keypair, new user_id.
    let identity_on_disk =
        load_identity(&dir.join("identity.json"), sealer.as_ref()).expect("identity loads");
    assert_eq!(identity_on_disk.user_id, "fresh-user");
    let new_pub = *identity_on_disk.x25519_public.as_bytes();
    assert_ne!(
        new_pub, prior_pub,
        "fresh identity keypair must differ from the prior one"
    );
    assert_eq!(
        new_pub,
        *new_identity.x25519_public.as_bytes(),
        "returned identity must match the one written to disk"
    );
}

#[test]
fn test_fresh_start_works_on_empty_directory() {
    // Idempotency: running fresh_start against a directory with
    // nothing in it produces the same end state as running it
    // against a fully-populated one. No files exist to wipe → all
    // the remove_if_present calls hit NotFound (Ok), then the
    // write steps run.
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let _ = cmd_osl_fresh_start(dir, "first-launch".to_string()).expect("fresh_start on empty dir");

    assert!(dir.join("identity.json").exists());
    assert!(dir.join("peer_map.json").exists());
    assert!(dir.join("channels.json").exists());
    assert!(dir.join("whitelist_state.json").exists());
    // 9-C1: pending_invitations.json no longer written by fresh_start.
}
