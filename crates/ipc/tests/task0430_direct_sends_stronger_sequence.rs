use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::cmd_osl_encrypt_message_v2;
use ipc::peer_map::{load_peer_map_from_path, PeerEntry, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use ipc::whitelist_state::ScopeState;
use keystore::client::{reg_msg_with_capabilities, KeyServerClient, CLIENT_RN_CAPABILITY_FLOOR};
use keystore::{generate_identity, Identity, PrekeyConfig, PrekeyState};

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const ALICE_DID: &str = "900000000000000431";
const BOB_DID: &str = "900000000000000432";

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

fn signed_live_rn_pubkeys(identity: &Identity) -> serde_json::Value {
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
    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": x25519,
        "ik_ed25519_pub": ed25519,
        "ik_mlkem768_pub": mlkem,
        "registered_at": "2026-08-06T00:00:00Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": null,
        "rn_capabilities": rn_capabilities,
        "registration_sig": STANDARD.encode(signature.as_bytes()),
        "identity_scheme": null,
        "identity_bundle_version": null,
        "identity_revision": null,
        "ik_root_ed25519_pub": null,
        "identity_bundle_proof_sig": null,
    })
}

fn prekey_bundle_json(identity: &Identity, prekeys: &PrekeyState) -> serde_json::Value {
    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": STANDARD.encode(identity.x25519_public.as_bytes()),
        "ik_ed25519_pub": STANDARD.encode(identity.ed25519_public.as_bytes()),
        "ik_mlkem768_pub": STANDARD.encode(identity.mlkem_public_bytes),
        "spk_pub": STANDARD.encode(prekeys.current_spk.public),
        "spk_signature": STANDARD.encode(prekeys.current_spk.signature),
        "spk_rotated_at": "2026-08-06T00:00:01Z",
        "opk": null,
        "remaining_opk_count": 7,
        "ik_ratchet_initial_pub": null,
    })
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
    install_dm_whitelist(state);
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

fn install_dm_whitelist(state: &AppState) {
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
}

#[test]
fn direct_send_output_names_stronger_sequence_and_no_basic_path() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("test profile directory");
    let _config = use_temp_config_dir(dir.path());

    let alice = generate_identity("task0430-alice".to_owned());
    let bob = generate_identity("task0430-bob".to_owned());
    let bob_prekeys = PrekeyState::new(&bob, PrekeyConfig::default(), 1_786_080_000);
    let port = start_keyserver(vec![
        ("GET /v1/pubkeys/", signed_live_rn_pubkeys(&bob)),
        (
            "GET /v1/prekey-bundle/",
            prekey_bundle_json(&bob, &bob_prekeys),
        ),
    ]);

    let first_state = AppState::new();
    first_state.set_rn_wire_in_enabled(true);
    first_state.install_identity(alice.clone());
    *first_state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{port}"))
            .expect("loopback keyserver client"),
    );
    install_dm_peer(&first_state, &bob);

    let first_output = cmd_osl_encrypt_message_v2(
        &first_state,
        "TASK0430 first eligible direct send".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect("first eligible direct send should use OSL-RN");

    let restarted_state = AppState::new();
    restarted_state.set_rn_wire_in_enabled(true);
    restarted_state.install_identity(alice);
    install_dm_whitelist(&restarted_state);
    *restarted_state.peer_map.lock().expect("peer map lock") =
        load_peer_map_from_path(&dir.path().join("peer_map.json"))
            .expect("restart loads persisted direct-chat security state");

    let direct_output = cmd_osl_encrypt_message_v2(
        &restarted_state,
        "TASK0430 restarted eligible direct send".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect("restarted eligible direct send should use persisted OSL-RN state");
    let output_json = serde_json::to_value(&direct_output).expect("output serializes");

    let outputs = [&first_output, &direct_output];
    let stronger_sequence_outputs = outputs
        .iter()
        .filter(|output| output.key_sequence == "stronger-osl-rn")
        .count();
    let basic_path_outputs = outputs
        .iter()
        .filter(|output| output.basic_path_used)
        .count();
    let rn_wire_outputs = outputs
        .iter()
        .filter(|output| {
            output.messages.len() == 1
                && osl_ratchet_next::peek_wire_version(&output.messages[0])
                    == Some(osl_ratchet_next::WIRE_VERSION_RN)
        })
        .count();
    let basic_wire_outputs = outputs
        .iter()
        .filter(|output| {
            output.messages.len() == 1
                && osl_ratchet_next::peek_wire_version(&output.messages[0])
                    == Some(ipc::wire_v2::WIRE_VERSION_V3)
        })
        .count();

    assert_eq!(output_json["key_sequence"], "stronger-osl-rn");
    assert_eq!(output_json["basic_path_used"], false);
    assert_eq!(stronger_sequence_outputs, 2);
    assert_eq!(basic_path_outputs, 0);
    assert_eq!(rn_wire_outputs, 2);
    assert_eq!(basic_wire_outputs, 0);

    println!(
        "TASK0430 direct_send_command=osl_encrypt_message_v2 eligible_direct_sends={} direct_send_output_key_sequence={} stronger_sequence_outputs={} direct_send_output_basic_path_used={} basic_path_outputs={} rn_wire_outputs={} basic_wire_outputs={}",
        outputs.len(),
        output_json["key_sequence"].as_str().unwrap_or("<missing>"),
        stronger_sequence_outputs,
        output_json["basic_path_used"].as_bool().unwrap_or(true),
        basic_path_outputs,
        rn_wire_outputs,
        basic_wire_outputs,
    );
}
