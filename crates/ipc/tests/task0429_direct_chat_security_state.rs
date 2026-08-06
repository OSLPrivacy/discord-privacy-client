use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::cmd_osl_encrypt_message_v2_wire;
use ipc::peer_map::{load_peer_map_from_path, PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use ipc::whitelist_state::ScopeState;
use keystore::client::{reg_msg_with_capabilities, KeyServerClient, CLIENT_RN_CAPABILITY_FLOOR};
use keystore::{generate_identity, Identity, PrekeyConfig, PrekeyState};

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const ALICE_DID: &str = "900000000000000429";
const BOB_DID: &str = "900000000000000430";

struct ConfigDirGuard;

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

fn use_temp_config_dir(dir: &Path) -> ConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    ConfigDirGuard
}

struct SignedPubkeys {
    body: serde_json::Value,
    registration_sig: String,
    rn_capabilities: u32,
}

fn signed_live_rn_pubkeys(identity: &Identity) -> SignedPubkeys {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
    let rn_capabilities = CLIENT_RN_CAPABILITY_FLOOR;
    let message = reg_msg_with_capabilities(
        &identity.user_id,
        &x25519,
        &ed25519,
        &mlkem,
        None,
        rn_capabilities,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);
    let registration_sig = STANDARD.encode(signature.as_bytes());
    SignedPubkeys {
        body: serde_json::json!({
            "user_id": identity.user_id,
            "ik_x25519_pub": x25519,
            "ik_ed25519_pub": ed25519,
            "ik_mlkem768_pub": mlkem,
            "registered_at": "2026-08-06T00:00:00Z",
            "last_rotated_at": null,
            "ik_ratchet_initial_pub": null,
            "rn_capabilities": rn_capabilities,
            "registration_sig": registration_sig,
            "identity_scheme": null,
            "identity_bundle_version": null,
            "identity_revision": null,
            "ik_root_ed25519_pub": null,
            "identity_bundle_proof_sig": null,
        }),
        registration_sig,
        rn_capabilities,
    }
}

fn prekey_bundle_json(identity: &Identity, prekeys: &PrekeyState) -> (serde_json::Value, String) {
    let spk_signature = STANDARD.encode(prekeys.current_spk.signature);
    (
        serde_json::json!({
            "user_id": identity.user_id,
            "ik_x25519_pub": STANDARD.encode(identity.x25519_public.as_bytes()),
            "ik_ed25519_pub": STANDARD.encode(identity.ed25519_public.as_bytes()),
            "ik_mlkem768_pub": STANDARD.encode(identity.mlkem_public_bytes),
            "spk_pub": STANDARD.encode(prekeys.current_spk.public),
            "spk_signature": spk_signature,
            "spk_rotated_at": "2026-08-06T00:00:01Z",
            "opk": null,
            "remaining_opk_count": 7,
            "ik_ratchet_initial_pub": null,
        }),
        spk_signature,
    )
}

fn start_keyserver(responses: Vec<(&'static str, serde_json::Value)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().expect("loopback address").port();
    thread::spawn(move || {
        for (expected_prefix, body) in responses {
            let (mut stream, _) = listener.accept().expect("keyserver request");
            let mut request = [0_u8; 8192];
            let read = stream.read(&mut request).expect("read request");
            let request = std::str::from_utf8(&request[..read]).expect("request UTF-8");
            assert!(
                request.starts_with(expected_prefix),
                "expected request prefix {expected_prefix}, got {request}"
            );
            let encoded = serde_json::to_vec(&body).expect("serialize response");
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

fn install_dm_peer(state: &AppState, peer: &Identity) {
    state
        .whitelist_state
        .lock()
        .expect("whitelist lock")
        .insert(
            Scope::dm(BOB_DID).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    state.peer_map.lock().expect("peer map lock").insert(
        BOB_DID.to_owned(),
        PeerEntry {
            osl_user_id: Some(peer.user_id.clone()),
            discord_id: Some(BOB_DID.to_owned()),
            pubkey: Some(STANDARD.encode(peer.x25519_public.as_bytes())),
            ik_mlkem768_pub: Some(STANDARD.encode(peer.mlkem_public_bytes)),
            tofu_key_bundle: Some(KeyBundle {
                ed25519_pub: STANDARD.encode(peer.ed25519_public.as_bytes()),
                x25519_pub: STANDARD.encode(peer.x25519_public.as_bytes()),
                mlkem768_pub: STANDARD.encode(peer.mlkem_public_bytes),
                ratchet_initial_pub: None,
            }),
            outgoing_whitelists: vec![WhitelistEntry::Dm {
                broadened: false,
                enabled_at: None,
            }],
            ..PeerEntry::default()
        },
    );
}

#[test]
fn restarting_test_profile_retains_agreed_state_and_peer_proof() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("test profile directory");
    let _config = use_temp_config_dir(dir.path());

    let alice = generate_identity("task0429-alice".to_owned());
    let bob = generate_identity("task0429-bob".to_owned());
    let bob_prekeys = PrekeyState::new(&bob, PrekeyConfig::default(), 1_786_080_000);
    let signed_pubkeys = signed_live_rn_pubkeys(&bob);
    let (prekey_body, expected_spk_signature) = prekey_bundle_json(&bob, &bob_prekeys);
    let port = start_keyserver(vec![
        ("GET /v1/pubkeys/", signed_pubkeys.body.clone()),
        ("GET /v1/prekey-bundle/", prekey_body),
    ]);

    let state = AppState::new();
    state.set_rn_wire_in_enabled(true);
    state.install_identity(alice);
    *state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{port}"))
            .expect("loopback keyserver client"),
    );
    install_dm_peer(&state, &bob);

    let wire = cmd_osl_encrypt_message_v2_wire(
        &state,
        "TASK0429 direct-chat restart proof".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect("eligible direct chat should send on RN")
    .content;
    assert_eq!(
        osl_ratchet_next::peek_wire_version(&wire),
        Some(osl_ratchet_next::WIRE_VERSION_RN)
    );

    let restarted_state = AppState::new();
    let loaded_peer_map =
        load_peer_map_from_path(&dir.path().join("peer_map.json")).expect("restart loads peer map");
    let reloaded_peer = loaded_peer_map.get(BOB_DID).expect("reloaded DM peer");
    let security = reloaded_peer
        .direct_chat_security
        .as_ref()
        .expect("reloaded direct-chat security state");
    let security = security.clone();
    *restarted_state.peer_map.lock().expect("peer map lock") = loaded_peer_map;

    let reloaded_store =
        ipc::wire_rn::RnSessionStore::for_config_dir(dir.path()).expect("restart opens RN store");
    let reloaded_pin = reloaded_store
        .load_pin(bob.x25519_public.as_bytes())
        .expect("restart loads RN pin");
    let reloaded_session = reloaded_store
        .load_session(bob.x25519_public.as_bytes())
        .expect("restart loads RN session");

    let registration_sig_retained =
        security.peer_proof.registration_sig == signed_pubkeys.registration_sig;
    let spk_signature_retained = security.peer_proof.spk_signature == expected_spk_signature;
    assert_eq!(
        security.agreed_wire_version,
        osl_ratchet_next::WIRE_VERSION_RN
    );
    assert!(reloaded_pin.is_pinned_to_rn());
    assert!(reloaded_session.is_some());
    assert_eq!(
        security.peer_proof.rn_capabilities,
        signed_pubkeys.rn_capabilities
    );
    assert!(registration_sig_retained);
    assert!(spk_signature_retained);
    assert_eq!(
        restarted_state
            .peer_map
            .lock()
            .expect("peer map lock")
            .get(BOB_DID)
            .and_then(|entry| entry.direct_chat_security.as_ref())
            .map(|state| state.agreed_wire_version),
        Some(osl_ratchet_next::WIRE_VERSION_RN)
    );

    println!(
        "TASK 0429 restarted_profile=1 agreed_wire_version=0x{version:02x} rn_pin_retained={pin} rn_session_retained={session} peer_proof_retained={proof} registration_sig_retained={registration} spk_signature_retained={spk} peer_proof_capabilities={caps}",
        version = security.agreed_wire_version,
        pin = u8::from(reloaded_pin.is_pinned_to_rn()),
        session = u8::from(reloaded_session.is_some()),
        proof = u8::from(registration_sig_retained && spk_signature_retained),
        registration = u8::from(registration_sig_retained),
        spk = u8::from(spk_signature_retained),
        caps = security.peer_proof.rn_capabilities,
    );
}
