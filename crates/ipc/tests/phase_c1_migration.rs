//! Phase 9-C1 Stage 2: legacy whitelist_state → envelope migration.
//!
//! Drives `ipc::migration::migrate_whitelist_state_in_place` against
//! a temp dir holding a hand-crafted legacy `whitelist_state.json`.
//! Verifies:
//!
//! 1. DM scope migrates to a `WhitelistEntry::Dm` on the peer.
//! 2. GC full whitelist migrates each member to a `WhitelistEntry::Gc`.
//! 3. ServerChannel per-user whitelist migrates each whitelisted_user
//!    to a `WhitelistEntry::ServerChannel`.
//! 4. Idempotency — running the migration twice doesn't re-project
//!    the same entries, and the `migrated_c1` marker is preserved.
//! 5. Missing whitelist_state.json returns Ok(None).
//! 6. A GC with empty members (full_whitelist=true, members=[])
//!    yields zero peer projections.

use ipc::migration::migrate_whitelist_state_in_place;
use ipc::peer_map::WhitelistEntry;
use ipc::state::AppState;
use tempfile::tempdir;

const HENRY_DID: &str = "1502770642930634812";
const ALICE_DID: &str = "1602770642930634812";
const GC_ID: &str = "1234567890";
const SERVER_ID: &str = "9876";
const CHANNEL_ID: &str = "5432";

fn write_legacy(dir: &std::path::Path, body: &str) {
    std::fs::write(dir.join("whitelist_state.json"), body).unwrap();
}

#[test]
fn dm_legacy_migrates_to_peer_outgoing_whitelists() {
    let dir = tempdir().unwrap();
    write_legacy(
        dir.path(),
        &format!(
            r#"{{
              "dm:{HENRY_DID}": {{ "encrypt_toggle": true, "auto_enabled": true }}
            }}"#
        ),
    );
    let state = AppState::new();
    let report = migrate_whitelist_state_in_place(&state, dir.path())
        .expect("migrate ok")
        .expect("file present");
    assert!(!report.was_already_migrated);
    assert_eq!(report.scope_entries_loaded, 1);
    assert_eq!(report.legacy_scope_entries_migrated, 1);
    assert_eq!(report.peer_links_added, 1);

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).expect("henry peer entry created");
    assert!(matches!(
        henry.outgoing_whitelists.as_slice(),
        [WhitelistEntry::Dm {
            broadened: false,
            ..
        }]
    ));
    drop(pm);

    // Marker stamped on disk.
    let reread =
        ipc::whitelist_state::load_whitelist_state_file(&dir.path().join("whitelist_state.json"))
            .unwrap();
    assert!(
        reread.migrated_c1,
        "envelope marker must be true post-migrate"
    );
}

#[test]
fn gc_full_whitelist_migrates_each_member() {
    let dir = tempdir().unwrap();
    write_legacy(
        dir.path(),
        &format!(
            r#"{{
              "gc:{GC_ID}": {{
                "encrypt_toggle": true,
                "auto_enabled": false,
                "full_whitelist": true,
                "members": ["{HENRY_DID}", "{ALICE_DID}"]
              }}
            }}"#
        ),
    );
    let state = AppState::new();
    let report = migrate_whitelist_state_in_place(&state, dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(report.peer_links_added, 2);

    let pm = state.peer_map.lock().unwrap();
    for did in [HENRY_DID, ALICE_DID] {
        let pe = pm.get(did).expect("peer entry created");
        assert!(
            pe.outgoing_whitelists.iter().any(|w| matches!(
                w,
                WhitelistEntry::Gc { id, user_specific: false } if id == GC_ID
            )),
            "missing Gc whitelist entry on peer {did}: {:?}",
            pe.outgoing_whitelists
        );
    }
}

#[test]
fn server_channel_per_user_migrates_whitelisted_users() {
    let dir = tempdir().unwrap();
    write_legacy(
        dir.path(),
        &format!(
            r#"{{
              "server_channel:{SERVER_ID}:{CHANNEL_ID}": {{
                "encrypt_toggle": true,
                "full_whitelist": false,
                "whitelisted_users": ["{HENRY_DID}"]
              }}
            }}"#
        ),
    );
    let state = AppState::new();
    let report = migrate_whitelist_state_in_place(&state, dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(report.peer_links_added, 1);

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    assert!(henry.outgoing_whitelists.iter().any(|w| matches!(
        w,
        WhitelistEntry::ServerChannel { server_id, channel_id, user_specific: true }
            if server_id == SERVER_ID && channel_id == CHANNEL_ID
    )));
}

#[test]
fn idempotent_second_run_does_not_duplicate_peer_links() {
    let dir = tempdir().unwrap();
    write_legacy(
        dir.path(),
        &format!(
            r#"{{
              "dm:{HENRY_DID}": {{ "encrypt_toggle": true }},
              "gc:{GC_ID}": {{
                "encrypt_toggle": false,
                "full_whitelist": true,
                "members": ["{HENRY_DID}"]
              }}
            }}"#
        ),
    );
    let state = AppState::new();
    let r1 = migrate_whitelist_state_in_place(&state, dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(r1.peer_links_added, 2);

    // Second run: marker present, no further peer mutations.
    let r2 = migrate_whitelist_state_in_place(&state, dir.path())
        .unwrap()
        .unwrap();
    assert!(r2.was_already_migrated, "second run sees marker");
    assert_eq!(r2.peer_links_added, 0);
    assert_eq!(r2.legacy_scope_entries_migrated, 0);

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    // Exactly one Dm + one Gc — no duplicates.
    let dm_count = henry
        .outgoing_whitelists
        .iter()
        .filter(|w| matches!(w, WhitelistEntry::Dm { .. }))
        .count();
    let gc_count = henry
        .outgoing_whitelists
        .iter()
        .filter(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID))
        .count();
    assert_eq!(dm_count, 1);
    assert_eq!(gc_count, 1);
}

#[test]
fn missing_whitelist_state_returns_none() {
    let dir = tempdir().unwrap();
    let state = AppState::new();
    let result = migrate_whitelist_state_in_place(&state, dir.path()).unwrap();
    assert!(result.is_none(), "missing file → Ok(None)");
}

#[test]
fn empty_members_yields_no_peer_projections() {
    let dir = tempdir().unwrap();
    write_legacy(
        dir.path(),
        &format!(
            r#"{{
              "gc:{GC_ID}": {{
                "encrypt_toggle": true,
                "full_whitelist": true,
                "members": []
              }}
            }}"#
        ),
    );
    let state = AppState::new();
    let report = migrate_whitelist_state_in_place(&state, dir.path())
        .unwrap()
        .unwrap();
    assert_eq!(report.peer_links_added, 0);
    let pm = state.peer_map.lock().unwrap();
    assert!(pm.is_empty(), "no peer entries should be created");
}
