//! T19-T07: two current-build peers advertise signed RN capability bit 0.
//!
//! The production command must refuse each DM while the RN wire-in fuse is
//! closed.  It must not silently fall back to a legacy v=3 send.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::cmd_osl_encrypt_message_v2_wire;
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use ipc::whitelist_state::ScopeState;
use keystore::client::{reg_msg_with_capabilities, KeyServerClient, CLIENT_RN_CAPABILITY_FLOOR};
use keystore::{generate_identity, Identity};

const ALICE_DID: &str = "900000000000000101";
const BOB_DID: &str = "900000000000000102";

fn signed_pubkeys_response(identity: &Identity) -> serde_json::Value {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
    let capabilities = CLIENT_RN_CAPABILITY_FLOOR;
    let message = reg_msg_with_capabilities(
        &identity.user_id,
        &x25519,
        &ed25519,
        &mlkem,
        None,
        capabilities,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);

    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": x25519,
        "ik_ed25519_pub": ed25519,
        "ik_mlkem768_pub": mlkem,
        "registered_at": "2026-07-31T00:00:00Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": null,
        "rn_capabilities": capabilities,
        "registration_sig": STANDARD.encode(signature.as_bytes()),
        "identity_scheme": null,
        "identity_bundle_version": null,
        "identity_revision": null,
        "ik_root_ed25519_pub": null,
        "identity_bundle_proof_sig": null,
    })
}

fn start_pubkeys_server(responses: Vec<serde_json::Value>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().expect("loopback address").port();
    thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("keyserver request");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read request");
            assert!(
                std::str::from_utf8(&request[..read])
                    .expect("request UTF-8")
                    .starts_with("GET /v1/pubkeys/"),
                "the send command must verify the peer capability through pubkeys"
            );
            let encoded = serde_json::to_vec(&body).expect("serialize pubkeys response");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                encoded.len()
            )
            .expect("write response headers");
            stream.write_all(&encoded).expect("write response body");
        }
    });
    port
}

fn state_with_current_build_peer(
    self_identity: Identity,
    self_did: &str,
    peer_identity: &Identity,
    peer_did: &str,
    keyserver_port: u16,
) -> AppState {
    let state = AppState::new();
    *state.identity.lock().expect("identity lock") = Some(self_identity);
    *state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{keyserver_port}"))
            .expect("loopback keyserver client"),
    );
    state
        .whitelist_state
        .lock()
        .expect("whitelist lock")
        .insert(
            Scope::dm(peer_did).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    let mut peers = state.peer_map.lock().expect("peer map lock");
    let entry = peers.entry(peer_did.to_owned()).or_default();
    entry.osl_user_id = Some(peer_identity.user_id.clone());
    entry.discord_id = Some(peer_did.to_owned());
    entry.pubkey = Some(STANDARD.encode(peer_identity.x25519_public.as_bytes()));
    entry.ik_mlkem768_pub = Some(STANDARD.encode(peer_identity.mlkem_public_bytes));
    entry.tofu_key_bundle = Some(KeyBundle {
        ed25519_pub: STANDARD.encode(peer_identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(peer_identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(peer_identity.mlkem_public_bytes),
        ratchet_initial_pub: None,
    });
    entry.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
    drop(peers);

    // Keep the fixture honest: command callers identify the local peer too.
    assert_ne!(self_did, peer_did);
    state
}

fn send_dm(state: &AppState, sender_did: &str, recipient_did: &str) -> Result<String, String> {
    cmd_osl_encrypt_message_v2_wire(
        state,
        "T19-T07 capability refusal".to_owned(),
        ScopeInput::from(&Scope::dm(recipient_did)),
        vec![recipient_did.to_owned()],
        sender_did.to_owned(),
    )
    .map(|wire| wire.content)
}

#[test]
fn two_current_build_peers_refuse_to_send_while_rn_wire_in_is_disabled() {
    let alice = generate_identity("t19-a7-alice".to_owned());
    let bob = generate_identity("t19-a7-bob".to_owned());
    let port = start_pubkeys_server(vec![
        signed_pubkeys_response(&bob),
        signed_pubkeys_response(&alice),
    ]);
    let alice_state = state_with_current_build_peer(alice.clone(), ALICE_DID, &bob, BOB_DID, port);
    let bob_state = state_with_current_build_peer(bob, BOB_DID, &alice, ALICE_DID, port);
    // Disable the gate explicitly. This test previously relied on the AppState
    // DEFAULT being false, and an agent flipped that default to make it pass -
    // which broke four other tests (including a signoff) that require the
    // shipping default to be true. A test about the disabled path must set the
    // flag it is testing.
    alice_state.set_rn_wire_in_enabled(false);
    bob_state.set_rn_wire_in_enabled(false);

    let alice_error = send_dm(&alice_state, ALICE_DID, BOB_DID)
        .expect_err("a current-build peer must refuse rather than downgrade its DM");
    let bob_error = send_dm(&bob_state, BOB_DID, ALICE_DID)
        .expect_err("the second current-build peer must refuse too");

    assert!(
        alice_error.contains("wire-in is disabled"),
        "Alice verdict: {alice_error}"
    );
    assert!(
        bob_error.contains("wire-in is disabled"),
        "Bob verdict: {bob_error}"
    );
}
