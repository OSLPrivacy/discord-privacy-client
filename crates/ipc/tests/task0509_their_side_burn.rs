use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ipc::commands::{
    cmd_osl_burn_their_side_message, cmd_osl_load_channel_history, TheirSideBurnDto,
};
use ipc::state::AppState;
use keystore::{identity_from_entropy, BurnScope, KeyServerClient};
use serde_json::Value;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const CONTENT_ID: &str = "task0509-message";
const CHANNEL_ID: &str = "task0509-channel";

#[derive(Clone)]
struct RemoteRecord {
    sender_id: String,
}

struct SignedBurnRelay {
    base_url: String,
    rows: Arc<Mutex<BTreeMap<String, RemoteRecord>>>,
    server: JoinHandle<()>,
}

impl SignedBurnRelay {
    fn start(identity: &keystore::Identity) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback relay");
        let address = listener.local_addr().expect("read relay address");
        let rows = Arc::new(Mutex::new(BTreeMap::from([(
            CONTENT_ID.to_string(),
            RemoteRecord {
                sender_id: identity.user_id.clone(),
            },
        )])));
        let relay_rows = Arc::clone(&rows);
        let expected_user_id = identity.user_id.clone();
        let expected_public_key = identity.ed25519_public;
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept burn request");
            let request = read_request(&mut stream);
            let request_text = String::from_utf8_lossy(&request);
            let (head, body) = request_text
                .split_once("\r\n\r\n")
                .expect("request has headers and body");
            let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
            let timestamp_ms = burn["timestamp_ms"]
                .as_i64()
                .expect("burn request has timestamp_ms");
            let request_id = burn["request_id"]
                .as_str()
                .expect("burn request has request_id");
            let signature_b64 = burn["burn_signature_b64"]
                .as_str()
                .expect("burn request has burn_signature_b64");
            let signature_bytes: [u8; crypto::ed25519::SIGNATURE_SIZE] = STANDARD
                .decode(signature_b64)
                .expect("burn signature is base64")
                .try_into()
                .expect("burn signature is 64 bytes");
            let signature = crypto::ed25519::Signature::from_bytes(signature_bytes);
            let canonical = keystore::canonical_burn_bytes(
                &expected_user_id,
                timestamp_ms,
                request_id,
                &BurnScope::Single {
                    content_id: CONTENT_ID.to_string(),
                },
            );
            let signature_valid =
                crypto::ed25519::verify(&expected_public_key, &canonical, &signature)
                    .expect("burn signature verification runs");
            let request_valid = head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1")
                && burn["scope"] == "single"
                && burn["user_id"] == expected_user_id
                && burn["target_content_id"] == CONTENT_ID
                && burn["target_user_id"].is_null()
                && signature_valid;
            let removed = if request_valid {
                let mut rows = relay_rows.lock().expect("lock relay rows");
                match rows.get(CONTENT_ID) {
                    Some(record) if record.sender_id == expected_user_id => {
                        rows.remove(CONTENT_ID);
                        1
                    }
                    _ => 0,
                }
            } else {
                0
            };
            let body = format!(r#"{{"scope":"single","deleted_count":{removed}}}"#);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("respond to burn request");
        });
        Self {
            base_url: format!("http://{address}"),
            rows,
            server,
        }
    }

    fn remote_count(&self) -> usize {
        self.rows.lock().expect("lock relay rows").len()
    }

    fn join(self) {
        self.server.join().expect("relay exits cleanly");
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read burn request");
        assert_ne!(read, 0, "burn request ended before its body arrived");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .expect("burn request has content length")
            .parse::<usize>()
            .expect("content length is numeric");
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

#[test]
fn their_side_burn_removes_remote_wrapped_key_and_keeps_local_record() {
    let dir = TempDir::new().unwrap();
    let state = AppState::new();
    let identity = identity_from_entropy([50; 16], "task0509-sender".to_string());
    let secret: [u8; 32] = *identity.x25519_secret.as_bytes();
    let relay = SignedBurnRelay::start(&identity);
    state.install_identity(identity);
    *state.keyserver.lock().unwrap() = Some(KeyServerClient::new(&relay.base_url).unwrap());
    let store = MessageStore::open(dir.path(), &secret).expect("open message store");
    store
        .put(&StoredMessage {
            discord_message_id: CONTENT_ID.to_string(),
            channel_id: CHANNEL_ID.to_string(),
            sender_discord_id: "task0509-self-discord".to_string(),
            sender_osl_user_id: "task0509-sender".to_string(),
            plaintext: "local copy must remain".to_string(),
            decrypted_at: 1_800_000_509,
            burned: false,
        })
        .expect("seed local message");
    *state.message_store.lock().unwrap() = Some(store);

    let result: TheirSideBurnDto = cmd_osl_burn_their_side_message(&state, CONTENT_ID.to_string())
        .expect("their-side command signs and sends remote burn");

    assert_eq!(result.remote_removal_count, 1);
    assert_eq!(relay.remote_count(), 0);
    let local_count = cmd_osl_load_channel_history(&state, CHANNEL_ID.to_string(), None)
        .expect("local history remains readable")
        .len();
    assert_eq!(local_count, 1);
    println!("TASK0509 local_count_after={local_count}");
    println!(
        "TASK0509 remote_removal_count={}",
        result.remote_removal_count
    );
    relay.join();
}
