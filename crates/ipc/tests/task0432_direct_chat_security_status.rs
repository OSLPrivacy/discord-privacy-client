use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_direct_chat_security_status, DirectChatSecurityState};
use ipc::peer_map::WhitelistEntry;
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use keystore::client::{
    reg_msg_with_capabilities, KeyServerClient, RN_CAP_WIRE_RN, RN_CAP_WIRE_RN_LIVE,
};
use keystore::{generate_identity, Identity};

const STRONGER_DID: &str = "900000000000004321";
const PENDING_DID: &str = "900000000000004322";
const REFUSED_DID: &str = "900000000000004323";

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

struct ConfigDirGuard {
    previous_active_account_dir: Option<PathBuf>,
}

impl ConfigDirGuard {
    fn install(dir: &Path) -> Self {
        let previous_active_account_dir = keystore::active_account_dir();
        keystore::set_active_account_dir(Some(dir.to_path_buf()));
        Self {
            previous_active_account_dir,
        }
    }
}

impl Drop for ConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(self.previous_active_account_dir.clone());
    }
}

fn signed_pubkeys_response(identity: &Identity, capabilities: u32) -> serde_json::Value {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
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

fn start_pubkeys_server(responses: Vec<serde_json::Value>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind pubkeys fixture");
    let port = listener.local_addr().expect("fixture address").port();
    thread::spawn(move || {
        for body in responses {
            let (mut stream, _) = listener.accept().expect("accept pubkeys request");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read pubkeys request");
            assert!(
                std::str::from_utf8(&request[..read])
                    .expect("request is UTF-8")
                    .starts_with("GET /v1/pubkeys/"),
                "status command must verify peer capability through pubkeys"
            );
            let encoded = serde_json::to_vec(&body).expect("serialize fixture response");
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

fn install_peer(state: &AppState, did: &str, identity: &Identity) {
    let mut peers = state.peer_map.lock().expect("peer map lock");
    let entry = peers.entry(did.to_owned()).or_default();
    entry.osl_user_id = Some(identity.user_id.clone());
    entry.discord_id = Some(did.to_owned());
    entry.pubkey = Some(STANDARD.encode(identity.x25519_public.as_bytes()));
    entry.ik_mlkem768_pub = Some(STANDARD.encode(identity.mlkem_public_bytes));
    entry.tofu_key_bundle = Some(KeyBundle {
        ed25519_pub: STANDARD.encode(identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
        ratchet_initial_pub: identity
            .ratchet_initial_pub
            .map(|key| STANDARD.encode(key.as_bytes())),
    });
    entry.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn assert_no_key_bytes(json: &str, identities: &[&Identity]) -> usize {
    let forbidden_terms = [
        "x25519",
        "ed25519",
        "mlkem",
        "pubkey",
        "public",
        "secret",
        "signature",
        "capabilities",
        "DPC0::",
    ];
    let mut leaks = 0usize;
    for term in forbidden_terms {
        if json.contains(term) {
            leaks += 1;
        }
    }
    for identity in identities {
        let encoded_keys = [
            STANDARD.encode(identity.x25519_public.as_bytes()),
            STANDARD.encode(identity.ed25519_public.as_bytes()),
            STANDARD.encode(identity.mlkem_public_bytes),
            STANDARD.encode(identity.x25519_secret.as_bytes()),
            STANDARD.encode(identity.ed25519_secret.as_bytes()),
        ];
        for encoded in encoded_keys {
            if json.contains(&encoded) {
                leaks += 1;
            }
        }
    }
    assert_eq!(leaks, 0, "direct-chat status leaked key material: {json}");
    leaks
}

#[test]
fn three_fixture_direct_chats_return_safe_security_states_without_key_bytes() {
    let _serial = CONFIG_DIR_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let config_dir = tempfile::TempDir::new().expect("config dir");
    let _config_guard = ConfigDirGuard::install(config_dir.path());

    let stronger = generate_identity("task0432-stronger".to_owned());
    let pending = generate_identity("task0432-pending".to_owned());
    let refused = generate_identity("task0432-refused".to_owned());
    let local = generate_identity("task0432-local".to_owned());
    let server = start_pubkeys_server(vec![
        signed_pubkeys_response(&stronger, RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE),
        signed_pubkeys_response(&pending, 0),
        signed_pubkeys_response(&refused, 0),
    ]);

    let state = AppState::new();
    state.install_identity(local.clone());
    *state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{server}"))
            .expect("loopback keyserver client"),
    );
    install_peer(&state, STRONGER_DID, &stronger);
    install_peer(&state, PENDING_DID, &pending);
    install_peer(&state, REFUSED_DID, &refused);
    ipc::wire_rn::RnSessionStore::for_config_dir(config_dir.path())
        .expect("RN store")
        .raise_pair_pin_to_rn(
            local.x25519_public.as_bytes(),
            refused.x25519_public.as_bytes(),
        )
        .expect("seed refused downgrade floor");

    let statuses = vec![
        cmd_osl_direct_chat_security_status(&state, STRONGER_DID.to_owned())
            .expect("stronger fixture returns a status"),
        cmd_osl_direct_chat_security_status(&state, PENDING_DID.to_owned())
            .expect("pending fixture returns a status"),
        cmd_osl_direct_chat_security_status(&state, REFUSED_DID.to_owned())
            .expect("refused fixture returns a status"),
    ];
    let states: Vec<&str> = statuses
        .iter()
        .map(|status| status.state.as_str())
        .collect();
    assert_eq!(states, ["stronger", "pending", "refused"]);
    assert_eq!(statuses[0].state, DirectChatSecurityState::Stronger);
    assert_eq!(statuses[1].state, DirectChatSecurityState::Pending);
    assert_eq!(statuses[2].state, DirectChatSecurityState::Refused);

    let json = serde_json::to_string(&statuses).expect("serialize safe status list");
    let key_byte_leaks = assert_no_key_bytes(&json, &[&stronger, &pending, &refused]);
    println!(
        "TASK0432 fixture_conversations={} states={} key_byte_leaks={} json={}",
        statuses.len(),
        states.join(","),
        key_byte_leaks,
        json
    );
}
