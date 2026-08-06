use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::Mutex;
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

const ALICE_DID: &str = "900000000000003758";
const BOB_DID: &str = "900000000000003759";

static IO_LOCK: Mutex<()> = Mutex::new(());

struct AuditedMessage {
    name: &'static str,
    label: &'static str,
    material_present: bool,
}

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
        "registered_at": "2026-08-06T00:00:00Z",
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

fn signed_prekey_bundle_response(identity: &Identity) -> serde_json::Value {
    let prekeys =
        keystore::PrekeyState::new(identity, keystore::PrekeyConfig::default(), 1_780_000_000);
    let opk = prekeys.opk_pool.first().expect("fixture OPK exists");

    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": STANDARD.encode(identity.x25519_public.as_bytes()),
        "ik_ed25519_pub": STANDARD.encode(identity.ed25519_public.as_bytes()),
        "ik_mlkem768_pub": STANDARD.encode(identity.mlkem_public_bytes),
        "spk_pub": STANDARD.encode(prekeys.current_spk.public),
        "spk_signature": STANDARD.encode(prekeys.current_spk.signature),
        "spk_rotated_at": "2026-08-06T00:00:00Z",
        "opk": {
            "id": opk.id,
            "pub_b64": STANDARD.encode(opk.public),
        },
        "remaining_opk_count": 1,
        "ik_ratchet_initial_pub": null,
    })
}

fn start_keyserver(peer: &Identity) -> u16 {
    let pubkeys = serde_json::to_vec(&signed_pubkeys_response(peer)).expect("pubkeys JSON");
    let prekey = serde_json::to_vec(&signed_prekey_bundle_response(peer)).expect("prekey JSON");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().expect("loopback address").port();
    thread::spawn(move || {
        for expected in ["GET /v1/pubkeys/", "GET /v1/prekey-bundle/"] {
            let (mut stream, _) = listener.accept().expect("keyserver request");
            let mut request = [0_u8; 8192];
            let read = stream.read(&mut request).expect("read request");
            let request = std::str::from_utf8(&request[..read]).expect("request UTF-8");
            assert!(
                request.starts_with(expected),
                "unexpected keyserver request for stronger-protection proof: {request}"
            );
            let body = if expected.contains("pubkeys") {
                &pubkeys
            } else {
                &prekey
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .expect("write response headers");
            stream.write_all(body).expect("write response body");
        }
    });
    port
}

fn state_with_peer(
    self_identity: Identity,
    peer_identity: &Identity,
    peer_did: &str,
    keyserver_port: Option<u16>,
    peer_has_rn_label: bool,
) -> AppState {
    let state = AppState::new();
    state.install_identity(self_identity);
    if let Some(port) = keyserver_port {
        *state.keyserver.lock().expect("keyserver lock") = Some(
            KeyServerClient::new(format!("http://127.0.0.1:{port}"))
                .expect("loopback keyserver client"),
        );
    }
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
    entry.osl_user_id = peer_has_rn_label.then(|| peer_identity.user_id.clone());
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
    state
}

fn raw_wire(wire: &str) -> Vec<u8> {
    let b64 = wire.strip_prefix("DPC0::").expect("DPC0 prefix");
    STANDARD.decode(b64).expect("wire base64")
}

fn rn_bootstrap_mlkem_material_present(wire: &str, sender: &Identity) -> bool {
    let raw = raw_wire(wire);
    raw.first() == Some(&osl_ratchet_next::WIRE_VERSION_RN)
        && raw
            .get(1)
            .map(|flags| flags & 0x01 == 0x01)
            .unwrap_or(false)
        && raw.len() >= 2 + 32 + 32 + 1 + osl_ratchet_next::MLKEM_CT
        && osl_ratchet_next::peek_bootstrap_initiator_identity(wire)
            .ok()
            .flatten()
            .map(|identity| identity.as_bytes() == sender.x25519_public.as_bytes())
            .unwrap_or(false)
}

fn legacy_v3_without_rn_material(wire: &str) -> bool {
    let raw = raw_wire(wire);
    raw.first() == Some(&ipc::wire_v2::WIRE_VERSION_V3)
        && osl_ratchet_next::peek_wire_version(wire) != Some(osl_ratchet_next::WIRE_VERSION_RN)
        && osl_ratchet_next::peek_bootstrap_initiator_identity(wire).is_err()
}

#[test]
fn task_3758_prepared_messages_prove_stronger_material_not_just_label() {
    let _lock = IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let rn_dir = tempfile::tempdir().expect("RN temp config dir");
    let _config = use_temp_config_dir(rn_dir.path());

    let alice = generate_identity("task-3758-alice".to_owned());
    let bob = generate_identity("task-3758-bob".to_owned());

    let rn_port = start_keyserver(&bob);
    let rn_state = state_with_peer(alice.clone(), &bob, BOB_DID, Some(rn_port), true);
    rn_state.set_rn_wire_in_enabled(true);
    let rn_wire = cmd_osl_encrypt_message_v2_wire(
        &rn_state,
        "task 3758 rn material proof".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect("RN-on prepared message")
    .content;
    let rn_material_present = rn_bootstrap_mlkem_material_present(&rn_wire, &alice);

    let legacy_dir = tempfile::tempdir().expect("legacy temp config dir");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(legacy_dir.path().to_path_buf()));
    ipc::main_password::set_file_storage_key(None);
    let legacy_state = state_with_peer(alice, &bob, BOB_DID, None, false);
    legacy_state.set_rn_wire_in_enabled(false);
    let legacy_wire = cmd_osl_encrypt_message_v2_wire(
        &legacy_state,
        "task 3758 legacy material absence proof".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect("RN-off prepared message")
    .content;
    let legacy_material_absent = legacy_v3_without_rn_material(&legacy_wire);

    let rn_label = "on";
    let legacy_label = "off";
    let audited_messages = [
        AuditedMessage {
            name: "task-3758-rn-message",
            label: rn_label,
            material_present: rn_material_present,
        },
        AuditedMessage {
            name: "task-3758-legacy-message",
            label: legacy_label,
            material_present: !legacy_material_absent,
        },
    ];
    let label_on_material_absent: Vec<&AuditedMessage> = audited_messages
        .iter()
        .filter(|message| message.label == "on" && !message.material_present)
        .collect();

    println!("TASK3758 on_label={rn_label}");
    println!(
        "TASK3758 on_wire_version={:?}",
        osl_ratchet_next::peek_wire_version(&rn_wire)
    );
    println!("TASK3758 on_mlkem_bootstrap_material_present={rn_material_present}");
    println!("TASK3758 off_label={legacy_label}");
    println!("TASK3758 off_wire_version={}", raw_wire(&legacy_wire)[0]);
    println!(
        "TASK3758 off_mlkem_bootstrap_material_present={}",
        !legacy_material_absent
    );
    println!(
        "TASK3758 label_on_material_absent_count={}",
        label_on_material_absent.len()
    );

    assert!(rn_material_present);
    assert!(legacy_material_absent);
    assert!(
        label_on_material_absent.is_empty(),
        "TASK3758 label lied for message {}",
        label_on_material_absent
            .iter()
            .map(|message| message.name)
            .collect::<Vec<_>>()
            .join(",")
    );
}
