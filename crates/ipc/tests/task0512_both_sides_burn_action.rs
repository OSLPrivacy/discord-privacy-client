use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_burn_sender_message_records_both_sides, cmd_osl_load_channel_history};
use ipc::state::AppState;
use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
use serde_json::Value;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = &[0x51; 32];
const CHANNEL: &str = "task0512-channel";
const SENDER: &str = "sender-0512";

fn sample(id: &str, sender: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: CHANNEL.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: sender.to_string(),
        plaintext: body.to_string(),
        decrypted_at: at,
        burned: false,
    }
}

fn state_with_store(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let store = MessageStore::open(dir, SECRET).expect("open message store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

struct SignedBurnServer {
    base_url: String,
    removed: Arc<Mutex<BTreeSet<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl SignedBurnServer {
    fn start(
        expected_content_ids: Vec<String>,
        user_id: String,
        public_key: crypto::ed25519::PublicKey,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let expected: BTreeSet<_> = expected_content_ids.into_iter().collect();
        let removed = Arc::new(Mutex::new(expected.clone()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let removed_for_server = Arc::clone(&removed);
        let requests_for_server = Arc::clone(&requests);
        let server = thread::spawn(move || {
            for _ in 0..expected.len() {
                let (mut stream, _) = listener.accept().expect("accept burn request");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                assert!(
                    head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
                    "remote burn must call the wrapped-key delete endpoint"
                );
                let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
                assert_eq!(burn["scope"], "single");
                assert_eq!(burn["user_id"], user_id);
                assert!(burn["target_user_id"].is_null());
                let content_id = burn["target_content_id"]
                    .as_str()
                    .expect("single-scope burn names a content id");
                let timestamp_ms = burn["timestamp_ms"]
                    .as_i64()
                    .expect("signed burn carries a timestamp");
                let request_id = burn["request_id"]
                    .as_str()
                    .expect("signed burn carries a request id");
                assert!(!request_id.is_empty());
                let signature_b64 = burn["burn_signature_b64"]
                    .as_str()
                    .expect("signed burn carries a signature");
                let signature_bytes = STANDARD.decode(signature_b64).expect("signature is base64");
                let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] =
                    signature_bytes.try_into().expect("signature is 64 bytes");
                let signature = crypto::ed25519::Signature::from_bytes(signature_array);
                let canonical = canonical_burn_bytes(
                    &user_id,
                    timestamp_ms,
                    request_id,
                    &BurnScope::Single {
                        content_id: content_id.to_owned(),
                    },
                );
                assert!(
                    crypto::ed25519::verify(&public_key, &canonical, &signature).unwrap(),
                    "remote burn request signature must verify"
                );
                requests_for_server
                    .lock()
                    .expect("lock requests")
                    .push(content_id.to_owned());
                let deleted = expected.contains(content_id)
                    && removed_for_server
                        .lock()
                        .expect("lock removed set")
                        .remove(content_id);
                let body = format!(
                    r#"{{"scope":"single","deleted_count":{}}}"#,
                    u32::from(deleted)
                );
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("respond to burn request");
            }
        });
        Self {
            base_url: format!("http://{address}"),
            removed,
            requests,
            server,
        }
    }

    fn join(self) -> (Vec<String>, usize) {
        self.server.join().expect("burn server exits cleanly");
        let requests = self.requests.lock().expect("lock requests").clone();
        let remaining = self.removed.lock().expect("lock removed set").len();
        (requests, remaining)
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
fn task0512_action_burns_the_same_selected_records_locally_and_remotely() {
    let tmp = TempDir::new().unwrap();
    let state = state_with_store(tmp.path());
    let identity = keystore::identity_from_entropy([0x12; 16], "burner-0512".to_owned());
    let public_key = identity.ed25519_public;
    state.install_identity(identity);
    let selected = vec![
        "task0512-message-1".to_string(),
        "task0512-message-2".to_string(),
        "task0512-message-3".to_string(),
    ];
    let server = SignedBurnServer::start(selected.clone(), "burner-0512".to_owned(), public_key);
    *state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&server.base_url).expect("install loopback keyserver"));

    {
        let guard = state.message_store.lock().unwrap();
        let store = guard.as_ref().expect("message store installed");
        for (idx, id) in selected.iter().enumerate() {
            store
                .put(&sample(
                    id,
                    SENDER,
                    &format!("TASK0512 selected sender record {}", idx + 1),
                    1_900_000_000 + idx as i64,
                ))
                .unwrap();
        }
        store
            .put(&sample(
                "task0512-survivor",
                "sender-0512-survivor",
                "TASK0512 survivor record",
                1_900_000_100,
            ))
            .unwrap();
    }

    let result = cmd_osl_burn_sender_message_records_both_sides(&state, selected.clone()).unwrap();
    let (remote_requests, remote_remaining) = server.join();

    assert_eq!(result.requested_count, 3);
    assert_eq!(result.local_removal_count, 3);
    assert_eq!(result.remote_removal_count, 3);
    assert_eq!(result.remaining_local_count, 0);
    assert!(result.equal_removal_counts);
    assert_eq!(remote_requests, selected);
    assert_eq!(remote_remaining, 0);

    let after = cmd_osl_load_channel_history(&state, CHANNEL.to_string(), Some(10)).unwrap();
    let selected_after = after
        .iter()
        .filter(|row| {
            [
                "task0512-message-1",
                "task0512-message-2",
                "task0512-message-3",
            ]
            .contains(&row.discord_message_id.as_str())
        })
        .count();
    let survivor_count = after
        .iter()
        .filter(|row| row.discord_message_id == "task0512-survivor")
        .count();
    assert_eq!(selected_after, 0);
    assert_eq!(survivor_count, 1);

    println!(
        "TASK0512 action=cmd_osl_burn_sender_message_records_both_sides selected_records={} local_removal_count={} remote_removal_count={} equal_removal_counts={} remaining_local_count={} survivor_count={}",
        result.requested_count,
        result.local_removal_count,
        result.remote_removal_count,
        result.equal_removal_counts,
        result.remaining_local_count,
        survivor_count
    );
}
