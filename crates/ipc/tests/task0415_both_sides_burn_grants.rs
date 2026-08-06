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

const SECRET: &[u8; 32] = &[0x15; 32];
const CHANNEL: &str = "task0415-channel";
const USER_ID: &str = "burner-0415";

#[derive(Clone, Debug, PartialEq, Eq)]
struct InspectedDeletion {
    content_id: String,
    request_id: String,
    authorized: bool,
    deleted: bool,
}

struct InspectingBurnServer {
    base_url: String,
    remaining_remote_copies: Arc<Mutex<BTreeSet<String>>>,
    inspections: Arc<Mutex<Vec<InspectedDeletion>>>,
    server: JoinHandle<()>,
}

impl InspectingBurnServer {
    fn start(
        expected_content_ids: Vec<String>,
        user_id: String,
        public_key: crypto::ed25519::PublicKey,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let expected_count = expected_content_ids.len();
        let remaining_remote_copies = Arc::new(Mutex::new(
            expected_content_ids.into_iter().collect::<BTreeSet<_>>(),
        ));
        let inspections = Arc::new(Mutex::new(Vec::new()));
        let remaining_for_server = Arc::clone(&remaining_remote_copies);
        let inspections_for_server = Arc::clone(&inspections);
        let server = thread::spawn(move || {
            for _ in 0..expected_count {
                let (mut stream, _) = listener.accept().expect("accept burn request");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                assert!(
                    head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
                    "both-sides burn must request wrapped-key deletion"
                );

                let burn = serde_json::from_str::<Value>(body).expect("burn body is JSON");
                assert_eq!(burn["scope"], "single");
                assert_eq!(burn["user_id"], user_id);
                assert!(burn["target_user_id"].is_null());
                let content_id = burn["target_content_id"]
                    .as_str()
                    .expect("single burn names target_content_id")
                    .to_owned();
                let timestamp_ms = burn["timestamp_ms"]
                    .as_i64()
                    .expect("burn carries timestamp_ms");
                let request_id = burn["request_id"]
                    .as_str()
                    .expect("burn carries request_id")
                    .to_owned();
                assert!(!request_id.is_empty());
                let signature_bytes = STANDARD
                    .decode(
                        burn["burn_signature_b64"]
                            .as_str()
                            .expect("burn carries burn_signature_b64"),
                    )
                    .expect("burn signature is base64");
                let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] = signature_bytes
                    .try_into()
                    .expect("burn signature is 64 bytes");
                let signature = crypto::ed25519::Signature::from_bytes(signature_array);
                let canonical = canonical_burn_bytes(
                    &user_id,
                    timestamp_ms,
                    &request_id,
                    &BurnScope::Single {
                        content_id: content_id.clone(),
                    },
                );
                let authorized = crypto::ed25519::verify(&public_key, &canonical, &signature)
                    .expect("signature verification runs");
                assert!(authorized, "burn deletion request must be authorized");

                let deleted = remaining_for_server
                    .lock()
                    .expect("lock remote copies")
                    .remove(&content_id);
                inspections_for_server
                    .lock()
                    .expect("lock inspections")
                    .push(InspectedDeletion {
                        content_id,
                        request_id,
                        authorized,
                        deleted,
                    });
                let response_body = format!(
                    r#"{{"scope":"single","deleted_count":{}}}"#,
                    u32::from(deleted)
                );
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                )
                .expect("respond to burn request");
            }
        });

        Self {
            base_url: format!("http://{address}"),
            remaining_remote_copies,
            inspections,
            server,
        }
    }

    fn join(self) -> (Vec<InspectedDeletion>, usize) {
        self.server.join().expect("burn server exits cleanly");
        let inspections = self.inspections.lock().expect("lock inspections").clone();
        let remaining = self
            .remaining_remote_copies
            .lock()
            .expect("lock remaining remote copies")
            .len();
        (inspections, remaining)
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

fn message(id: &str, body: &str, at: i64) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_owned(),
        channel_id: CHANNEL.to_owned(),
        sender_discord_id: USER_ID.to_owned(),
        sender_osl_user_id: USER_ID.to_owned(),
        plaintext: body.to_owned(),
        decrypted_at: at,
        burned: false,
    }
}

fn selected_remaining(state: &AppState, selected: &[String]) -> usize {
    cmd_osl_load_channel_history(state, CHANNEL.to_owned(), Some(10))
        .expect("load channel history")
        .iter()
        .filter(|row| selected.contains(&row.discord_message_id))
        .count()
}

#[test]
fn task_0415_both_sides_burn_grants_delete_two_copies() {
    let tmp = TempDir::new().unwrap();
    let selected = vec!["task0415-copy-a".to_owned(), "task0415-copy-b".to_owned()];
    let identity = keystore::identity_from_entropy([0x41; 16], USER_ID.to_owned());
    let server = InspectingBurnServer::start(
        selected.clone(),
        USER_ID.to_owned(),
        identity.ed25519_public,
    );

    let state = AppState::new();
    state.install_identity(identity);
    *state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&server.base_url).expect("install loopback keyserver"));
    *state.message_store.lock().unwrap() =
        Some(MessageStore::open(tmp.path(), SECRET).expect("open message store"));
    {
        let guard = state.message_store.lock().unwrap();
        let store = guard.as_ref().expect("message store installed");
        store
            .put(&message(
                &selected[0],
                "TASK0415 first local protected copy",
                1_900_415_001,
            ))
            .unwrap();
        store
            .put(&message(
                &selected[1],
                "TASK0415 second local protected copy",
                1_900_415_002,
            ))
            .unwrap();
        store
            .put(&message(
                "task0415-survivor",
                "TASK0415 unrelated copy must remain",
                1_900_415_003,
            ))
            .unwrap();
    }

    let result = cmd_osl_burn_sender_message_records_both_sides(&state, selected.clone()).unwrap();
    let local_remaining = selected_remaining(&state, &selected);
    let survivor_remaining = selected_remaining(&state, &["task0415-survivor".to_owned()]);
    let (inspections, remote_remaining) = server.join();
    let authorized_deletions = inspections
        .iter()
        .filter(|request| request.authorized && request.deleted)
        .count();
    let inspected_ids = inspections
        .iter()
        .map(|request| request.content_id.clone())
        .collect::<Vec<_>>();

    assert_eq!(result.requested_count, 2);
    assert_eq!(result.local_removal_count, 2);
    assert_eq!(result.remote_removal_count, 2);
    assert_eq!(result.remaining_local_count, 0);
    assert!(result.equal_removal_counts);
    assert_eq!(inspections.len(), 2);
    assert_eq!(inspected_ids, selected);
    assert_eq!(authorized_deletions, 2);
    assert_eq!(local_remaining, 0);
    assert_eq!(remote_remaining, 0);
    assert_eq!(survivor_remaining, 1);

    println!("TASK0415_REQUESTED_COPIES={}", result.requested_count);
    println!("TASK0415_INSPECTED_DELETION_REQUESTS={}", inspections.len());
    println!("TASK0415_AUTHORIZED_DELETIONS={authorized_deletions}");
    println!(
        "TASK0415_DELETION_REQUEST_IDS={}",
        inspections
            .iter()
            .map(|request| format!(
                "{}:authorized={}:deleted={}",
                request.content_id, request.authorized, request.deleted
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("TASK0415_LOCAL_SELECTED_REMAINING={local_remaining}");
    println!("TASK0415_REMOTE_SELECTED_REMAINING={remote_remaining}");
    println!("TASK0415_SURVIVOR_REMAINING={survivor_remaining}");
}
