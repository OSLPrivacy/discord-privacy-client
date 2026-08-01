//! Phase 9-C3: server defaults command tests.
//!
//! Covers `cmd_osl_set_server_default`, `cmd_osl_get_server_defaults`,
//! and `cmd_osl_apply_server_default_to_existing_channels`.

use ipc::commands::{
    cmd_osl_apply_server_default_to_existing_channels, cmd_osl_get_server_defaults,
    cmd_osl_set_server_default, GuildDto,
};
use ipc::scope::Scope;
use ipc::state::AppState;
use std::sync::Mutex;

// The disk-persistence side of these commands writes through
// `persist_whitelist_state_now`, which resolves the config dir via
// `keystore::osl_config_dir()` — that's a process-wide singleton.
// Tests that flip server_defaults serialize against this mutex so
// the persisted file isn't shared concurrently between tests.
static IO_LOCK: Mutex<()> = Mutex::new(());

const SERVER_A: &str = "9876543210000001";
const SERVER_B: &str = "9876543210000002";
const CHAN_1: &str = "5555000000000001";
const CHAN_2: &str = "5555000000000002";
const CHAN_3: &str = "5555000000000003";

fn install_guild(state: &AppState, server_id: &str, channel_ids: Vec<String>) {
    let mut gl = state.guild_list.lock().unwrap();
    gl.push(GuildDto {
        id: server_id.to_string(),
        name: "test-guild".to_string(),
        member_ids: vec![],
        channel_ids,
    });
}

#[test]
fn get_returns_empty_when_no_defaults_set() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    let r = cmd_osl_get_server_defaults(&state).unwrap();
    assert!(r.is_empty());
}

#[test]
fn set_and_get_roundtrip() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    cmd_osl_set_server_default(&state, SERVER_A.into(), true).unwrap();
    cmd_osl_set_server_default(&state, SERVER_B.into(), false).unwrap();
    let r = cmd_osl_get_server_defaults(&state).unwrap();
    assert_eq!(r.len(), 2);
    // Sorted by server_id deterministically.
    assert_eq!(r[0].server_id, SERVER_A);
    assert!(r[0].encrypt_by_default);
    assert_eq!(r[1].server_id, SERVER_B);
    assert!(!r[1].encrypt_by_default);
}

#[test]
fn get_sorts_by_server_id_deterministic() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    // Insert in reverse-lexical order to verify sort.
    cmd_osl_set_server_default(&state, SERVER_B.into(), true).unwrap();
    cmd_osl_set_server_default(&state, SERVER_A.into(), true).unwrap();
    let r = cmd_osl_get_server_defaults(&state).unwrap();
    assert_eq!(r[0].server_id, SERVER_A);
    assert_eq!(r[1].server_id, SERVER_B);
}

#[test]
fn apply_to_existing_iterates_guild_channels_and_sets_states() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    install_guild(
        &state,
        SERVER_A,
        vec![CHAN_1.into(), CHAN_2.into(), CHAN_3.into()],
    );
    let n = cmd_osl_apply_server_default_to_existing_channels(&state, SERVER_A.into()).unwrap();
    assert_eq!(n, 3);
    let ws = state.whitelist_state.lock().unwrap();
    for ch in [CHAN_1, CHAN_2, CHAN_3] {
        let key = Scope::server_channel(SERVER_A, ch).storage_key();
        let entry = ws.get(&key).expect("ScopeState created");
        assert!(entry.encrypt_toggle);
        assert!(entry.auto_enabled);
    }
}

#[test]
fn apply_to_existing_returns_zero_if_no_channels_known() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    // No guild registered.
    let n = cmd_osl_apply_server_default_to_existing_channels(&state, SERVER_A.into()).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn apply_to_existing_idempotent_on_already_enabled_channels() {
    let _g = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let state = AppState::new();
    install_guild(&state, SERVER_A, vec![CHAN_1.into(), CHAN_2.into()]);
    let n1 = cmd_osl_apply_server_default_to_existing_channels(&state, SERVER_A.into()).unwrap();
    assert_eq!(n1, 2);
    let n2 = cmd_osl_apply_server_default_to_existing_channels(&state, SERVER_A.into()).unwrap();
    assert_eq!(n2, 0, "second run should be a no-op (already encrypted)");
}

#[test]
fn set_server_default_rejects_empty_id() {
    let state = AppState::new();
    let r = cmd_osl_set_server_default(&state, String::new(), true);
    assert!(r.is_err());
}
