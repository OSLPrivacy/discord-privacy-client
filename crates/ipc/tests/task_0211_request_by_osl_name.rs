//! Task 0211: create requests by exact public OSL name.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{cmd_osl_create_friend_request_by_osl_name, cmd_osl_list_friend_requests};
use ipc::scope::Scope;
use ipc::state::AppState;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{mpsc, Mutex};

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const PUBLIC_OSL_NAME: &str = "maple_0211";
const PEER_OSL_USER_ID: &str = "task_0211_peer";

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
    ipc::main_password::set_file_storage_key(Some([0x21; 32]));
    ConfigDirGuard
}

fn username_digest_hex(name: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hasher = Sha256::new();
    hasher.update(b"OSL-USERNAME-BUCKET-v1");
    hasher.update(name.as_bytes());
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn bucket_for_name(name: &str, user_id: &str, ed25519_b64: &str) -> (String, String) {
    let digest_hex = username_digest_hex(name);
    let prefix = digest_hex[..4].to_owned();
    let suffix = &digest_hex[4..];
    let mut rows = vec![format!("{suffix}:{user_id}:{ed25519_b64}")];
    let mut counter = 0u64;
    while rows.len() < 1024 {
        let candidate_suffix = format!("{counter:060x}");
        counter += 1;
        if candidate_suffix == suffix {
            continue;
        }
        rows.push(format!("{candidate_suffix}:dummy_{counter}:{ed25519_b64}"));
    }
    rows.sort();
    (prefix, format!("{}\n", rows.join("\n")))
}

fn signed_pubkeys_json(identity: &keystore::Identity) -> String {
    let ik_x25519_pub = STANDARD.encode(identity.x25519_public.as_bytes());
    let ik_ed25519_pub = STANDARD.encode(identity.ed25519_public.as_bytes());
    let ik_mlkem768_pub = STANDARD.encode(identity.mlkem_public_bytes);
    let ik_ratchet_initial_pub = identity
        .ratchet_initial_pub
        .as_ref()
        .map(|key| STANDARD.encode(key.as_bytes()));
    let rn_capabilities = keystore::client::CLIENT_RN_CAPABILITY_FLOOR;
    let signed = keystore::client::reg_msg_with_capabilities(
        &identity.user_id,
        &ik_x25519_pub,
        &ik_ed25519_pub,
        &ik_mlkem768_pub,
        ik_ratchet_initial_pub.as_deref(),
        rn_capabilities,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &signed);

    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": ik_x25519_pub,
        "ik_ed25519_pub": ik_ed25519_pub,
        "ik_mlkem768_pub": ik_mlkem768_pub,
        "registered_at": "2026-08-06T00:00:00.000Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": ik_ratchet_initial_pub,
        "rn_capabilities": rn_capabilities,
        "registration_sig": STANDARD.encode(signature.as_bytes())
    })
    .to_string()
}

fn http_response(body: impl AsRef<[u8]>, content_type: &str) -> Vec<u8> {
    let body = body.as_ref();
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    response.extend_from_slice(b"Connection: close\r\n\r\n");
    response.extend_from_slice(body);
    response
}

fn scripted_server(responses: Vec<Vec<u8>>) -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buf = [0u8; 8192];
            let mut request = Vec::new();
            loop {
                let n = stream.read(&mut buf).expect("read request");
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            tx.send(String::from_utf8_lossy(&request).into_owned())
                .expect("send captured request");
            stream.write_all(&response).expect("write response");
        }
    });
    (port, rx)
}

#[test]
fn direct_command_creates_one_pending_request_for_exact_known_public_name() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let _config_dir = use_temp_config_dir(dir.path());

    let state = AppState::new();
    let mut requester = keystore::generate_identity("task_0211_requester".to_string());
    requester.discord_snowflake = Some("900000000000002111".to_string());
    state.install_identity(requester);

    let peer = keystore::generate_identity(PEER_OSL_USER_ID.to_string());
    let peer_ed25519_b64 = STANDARD.encode(peer.ed25519_public.as_bytes());
    let (prefix, bucket) = bucket_for_name(PUBLIC_OSL_NAME, PEER_OSL_USER_ID, &peer_ed25519_b64);
    let pubkeys = signed_pubkeys_json(&peer);
    let (port, requests) = scripted_server(vec![
        http_response(bucket, "text/plain"),
        http_response(pubkeys, "application/json"),
    ]);
    *state.keyserver_slot() =
        Some(keystore::KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap());

    let created = cmd_osl_create_friend_request_by_osl_name(&state, PUBLIC_OSL_NAME.to_string())
        .expect("known exact public OSL name should create a pending request");
    let listed = cmd_osl_list_friend_requests(&state).unwrap();
    let scope = Scope::dm(PEER_OSL_USER_ID);

    let bucket_request = requests.recv().unwrap();
    let pubkeys_request = requests.recv().unwrap();
    assert!(
        bucket_request.starts_with(&format!("GET /v1/username-bucket/{prefix} HTTP/1.1")),
        "{bucket_request}"
    );
    assert!(
        pubkeys_request.starts_with(&format!("GET /v1/pubkeys/{PEER_OSL_USER_ID} HTTP/1.1")),
        "{pubkeys_request}"
    );

    println!(
        "TASK_0211_DIRECT_OSL_NAME_REQUEST name={} pending_count={} peer={} scope={}",
        PUBLIC_OSL_NAME,
        listed.len(),
        listed[0].peer_discord_id,
        listed[0].scope_storage_key
    );

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], created.pending);
    assert_eq!(listed[0].peer_discord_id, PEER_OSL_USER_ID);
    assert_eq!(listed[0].scope_storage_key, scope.storage_key());
}
