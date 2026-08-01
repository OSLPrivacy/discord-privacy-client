//! T1-42: legacy Mode 1 preferences must not create multiple carrier covers.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::x25519;
use ipc::app_preferences::StegoMode;
use ipc::commands::cmd_osl_encrypt_message_v2;
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::generate_identity;

const SELF_DID: &str = "900000000000000003";
const PEER_DID: &str = "900000000000000001";

fn state_with_dm_peer() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("self".into()));
    let mut peers = state.peer_map.lock().unwrap();
    let self_peer = peers.entry(SELF_DID.to_string()).or_default();
    self_peer.is_self = Some(true);
    self_peer.discord_id = Some(SELF_DID.to_string());

    let (_peer_secret, peer_public) = x25519::generate_keypair();
    let (_mlkem_secret, mlkem_public) = crypto::ml_kem_768::generate_keypair();
    let (_ratchet_secret, ratchet_public) = x25519::generate_keypair();
    let peer = peers.entry(PEER_DID.to_string()).or_default();
    peer.pubkey = Some(STANDARD.encode(peer_public.as_bytes()));
    peer.ik_mlkem768_pub = Some(STANDARD.encode(mlkem_public.to_bytes()));
    peer.ik_ratchet_initial_pub = Some(STANDARD.encode(ratchet_public.as_bytes()));
    peer.discord_id = Some(PEER_DID.to_string());
    peer.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
    drop(peers);

    state.whitelist_state.lock().unwrap().insert(
        Scope::dm(PEER_DID).storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
    state
}

#[test]
fn legacy_mode1_preference_emits_one_non_chunked_message() {
    let state = state_with_dm_peer();
    state.app_preferences.lock().unwrap().stego_mode = StegoMode::Mode1;

    let output = cmd_osl_encrypt_message_v2(
        &state,
        "a message that formerly would have been chunked".into(),
        ScopeInput::from(&Scope::dm(PEER_DID)),
        vec![PEER_DID.into()],
        SELF_DID.into(),
    )
    .expect("encryption succeeds");

    assert_eq!(output.messages.len(), 1);
    assert!(output.messages[0].starts_with("DPC0::"));
    assert!(output.session_id.is_none());
}
