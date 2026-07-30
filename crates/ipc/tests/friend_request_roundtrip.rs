//! Unit a77: end-to-end friend-request round trip.

use ipc::commands::{cmd_osl_accept_friend_request, cmd_osl_send_friend_request};
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::Scope;
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use std::path::Path;
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const PEER_DID: &str = "900000000000007700";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ConfigDirGuard
}

fn trusted_peer_bundle(label: &str) -> KeyBundle {
    KeyBundle {
        ed25519_pub: format!("{label}-ed25519"),
        x25519_pub: format!("{label}-x25519"),
        mlkem768_pub: format!("{label}-mlkem768"),
        ratchet_initial_pub: Some(format!("{label}-ratchet")),
    }
}

#[test]
fn friend_request_round_trip() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());

    let state = AppState::new();
    state.install_identity(keystore::generate_identity("a77-requester-osl".to_string()));
    let scope = Scope::dm(PEER_DID);

    state.peer_map.lock().unwrap().insert(
        PEER_DID.to_string(),
        PeerEntry {
            discord_id: Some(PEER_DID.to_string()),
            tofu_key_bundle: Some(trusted_peer_bundle("a77-peer")),
            ..PeerEntry::default()
        },
    );
    assert!(state
        .peer_map
        .lock()
        .unwrap()
        .get(PEER_DID)
        .unwrap()
        .outgoing_whitelists
        .is_empty());
    assert!(state.whitelist_state.lock().unwrap().is_empty());

    let sent = cmd_osl_send_friend_request(&state, PEER_DID.to_string(), (&scope).into()).unwrap();

    assert!(sent.request.grants_scope(&scope));
    assert_eq!(sent.pending.peer_discord_id, PEER_DID);
    assert_eq!(sent.pending.scope_storage_key, scope.storage_key());
    assert!(dir.path().join("pending_friend_requests.json").exists());

    cmd_osl_accept_friend_request(&state, PEER_DID.to_string(), sent.request).unwrap();

    let peer_map = state.peer_map.lock().unwrap();
    let peer = peer_map.get(PEER_DID).unwrap();
    assert_eq!(peer.discord_id.as_deref(), Some(PEER_DID));
    assert_eq!(peer.outgoing_whitelists.len(), 1);
    assert!(matches!(
        peer.outgoing_whitelists.as_slice(),
        [WhitelistEntry::Dm {
            broadened: false,
            enabled_at: Some(_),
        }]
    ));
    drop(peer_map);

    let whitelist_state = state.whitelist_state.lock().unwrap();
    let adopted = whitelist_state.get(&scope.storage_key()).unwrap();
    assert!(adopted.encrypt_toggle);
    assert!(adopted.auto_enabled);
}
