//! T19-T32: a pairwise OSL-RN wire must never fan out to a group scope.
//!
//! The fixture gives each peer the signed live-RN capability.  That makes a
//! mistaken per-recipient RN selection observable: the command must reach
//! the multi-recipient guard, rather than emit a `0x10` wire for one member.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::cmd_osl_encrypt_message_v2_wire;
use ipc::peer_map::{PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use ipc::whitelist_state::ScopeState;
use keystore::client::{
    reg_msg_with_capabilities, KeyServerClient, CLIENT_RN_CAPABILITY_FLOOR, RN_CAP_WIRE_RN_LIVE,
};
use keystore::{generate_identity, Identity};

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const ALICE_DID: &str = "900000000000000301";
const BOB_DID: &str = "900000000000000302";
const CAROL_DID: &str = "900000000000000303";
const GROUP_ID: &str = "900000000000000399";

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

fn signed_live_rn_pubkeys_response(identity: &Identity) -> serde_json::Value {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
    let capabilities = CLIENT_RN_CAPABILITY_FLOOR;
    assert_ne!(
        capabilities & RN_CAP_WIRE_RN_LIVE,
        0,
        "T19-T32 needs peers that advertise the live-RN bit"
    );
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
        "registered_at": "2026-08-01T00:00:00Z",
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
                "the send command must verify each peer's live RN capability"
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

fn add_group_peer(state: &AppState, did: &str, identity: &Identity) {
    state.peer_map.lock().expect("peer map lock").insert(
        did.to_owned(),
        PeerEntry {
            osl_user_id: Some(identity.user_id.clone()),
            discord_id: Some(did.to_owned()),
            pubkey: Some(STANDARD.encode(identity.x25519_public.as_bytes())),
            ik_mlkem768_pub: Some(STANDARD.encode(identity.mlkem_public_bytes)),
            tofu_key_bundle: Some(KeyBundle {
                ed25519_pub: STANDARD.encode(identity.ed25519_public.as_bytes()),
                x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
                mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
                ratchet_initial_pub: None,
            }),
            outgoing_whitelists: vec![WhitelistEntry::Gc {
                id: GROUP_ID.to_owned(),
                user_specific: false,
            }],
            ..PeerEntry::default()
        },
    );
}

#[test]
fn three_recipient_scope_refuses_to_fan_out_an_rn_wire() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("isolated config directory");
    let _config_dir = use_temp_config_dir(dir.path());

    let alice = generate_identity("t19-t32-alice".to_owned());
    let bob = generate_identity("t19-t32-bob".to_owned());
    let carol = generate_identity("t19-t32-carol".to_owned());
    let port = start_pubkeys_server(vec![
        signed_live_rn_pubkeys_response(&bob),
        signed_live_rn_pubkeys_response(&carol),
    ]);

    let state = AppState::new();
    state.install_identity(alice);
    *state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{port}"))
            .expect("loopback keyserver client"),
    );
    state
        .whitelist_state
        .lock()
        .expect("whitelist lock")
        .insert(
            Scope::gc(GROUP_ID).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    add_group_peer(&state, BOB_DID, &bob);
    add_group_peer(&state, CAROL_DID, &carol);

    let error = cmd_osl_encrypt_message_v2_wire(
        &state,
        "T19-T32 pairwise-only RN".to_owned(),
        ScopeInput::from(&Scope::gc(GROUP_ID)),
        vec![BOB_DID.to_owned(), CAROL_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect_err("a two-peer scope must not emit a one-peer RN wire");

    assert!(
        error.contains("refusing to fan out a one-peer ratchet message"),
        "the multi-recipient guard, not an unrelated send failure, must reject RN fan-out: {error}"
    );
}
