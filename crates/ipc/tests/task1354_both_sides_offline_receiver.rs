use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_burn_sender_message_records_choice, cmd_osl_load_channel_history};
use ipc::state::AppState;
use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde_json::Value;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const CHANNEL: &str = "task1354-offline-receiver-channel";
const MESSAGE_ID: &str = "task1354-both-sides-offline-message";
const SENDER_NAME: &str = "sender copy";
const RECEIVER_NAME: &str = "offline receiver copy";
const SENDER_KEY: &[u8; 32] = &[0x54; 32];
const RECEIVER_KEY: &[u8; 32] = &[0x55; 32];

struct OfflineReceiverBurnServer {
    base_url: String,
    requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl OfflineReceiverBurnServer {
    fn start(
        expected_content_id: String,
        sender_user_id: String,
        sender_public_key: crypto::ed25519::PublicKey,
        receiver_store_dir: PathBuf,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_for_server = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept burn request");
            let request = read_request(&mut stream);
            let request_text = String::from_utf8_lossy(&request);
            let (head, body) = request_text
                .split_once("\r\n\r\n")
                .expect("request has headers and body");
            assert!(
                head.starts_with("DELETE /v1/wrapped-keys HTTP/1.1"),
                "Both Sides must call the wrapped-key delete endpoint"
            );
            let burn = serde_json::from_str::<Value>(body).expect("burn request is JSON");
            assert_eq!(burn["scope"], "single");
            assert_eq!(burn["user_id"], sender_user_id);
            assert!(burn["target_user_id"].is_null());
            let content_id = burn["target_content_id"]
                .as_str()
                .expect("single-scope burn names a content id");
            assert_eq!(content_id, expected_content_id);
            let timestamp_ms = burn["timestamp_ms"]
                .as_i64()
                .expect("signed burn carries a timestamp");
            let request_id = burn["request_id"]
                .as_str()
                .expect("signed burn carries a request id");
            let signature_b64 = burn["burn_signature_b64"]
                .as_str()
                .expect("signed burn carries a signature");
            let signature_bytes = STANDARD.decode(signature_b64).expect("signature is base64");
            let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] =
                signature_bytes.try_into().expect("signature is 64 bytes");
            let signature = crypto::ed25519::Signature::from_bytes(signature_array);
            let canonical = canonical_burn_bytes(
                &sender_user_id,
                timestamp_ms,
                request_id,
                &BurnScope::Single {
                    content_id: content_id.to_owned(),
                },
            );
            assert!(
                crypto::ed25519::verify(&sender_public_key, &canonical, &signature).unwrap(),
                "remote burn request signature must verify for the sender copy"
            );

            let receiver_store = MessageStore::open(&receiver_store_dir, RECEIVER_KEY)
                .expect("reopen closed receiver store inside remote burn handler");
            let receiver_delete = receiver_store
                .delete_message_records(&[content_id.to_owned()])
                .expect("delete marked receiver row while receiver copy is closed");
            assert_eq!(receiver_delete.removed_count, 1);
            requests_for_server
                .lock()
                .expect("lock requests")
                .push(content_id.to_owned());
            let body = r#"{"scope":"single","deleted_count":1}"#;
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
            requests,
            server,
        }
    }

    fn join(self) -> Vec<String> {
        self.server.join().expect("burn server exits cleanly");
        self.requests.lock().expect("lock requests").clone()
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

struct LocalCopy {
    name: &'static str,
    dir: TempDir,
    state: AppState,
}

fn make_copy(
    name: &'static str,
    store_key: &[u8; 32],
    sender_user_id: &str,
    mark: &str,
) -> LocalCopy {
    let dir = tempfile::tempdir().expect("local copy store dir");
    let state = AppState::new();
    let store = MessageStore::open(dir.path(), store_key).expect("open message store");
    store
        .put(&StoredMessage {
            discord_message_id: MESSAGE_ID.to_owned(),
            channel_id: CHANNEL.to_owned(),
            sender_discord_id: sender_user_id.to_owned(),
            sender_osl_user_id: sender_user_id.to_owned(),
            plaintext: mark.to_owned(),
            decrypted_at: 1_901_354_000,
            burned: false,
            reply_parent_id: None,
            edit_revision: 1,
        })
        .expect("seed marked message in named copy");
    *state
        .message_store
        .lock()
        .expect("message store mutex poisoned") = Some(store);
    LocalCopy { name, dir, state }
}

fn exact_mark_count(state: &AppState, mark: &str) -> (usize, bool) {
    let rows = cmd_osl_load_channel_history(state, CHANNEL.to_owned(), Some(10)).expect("history");
    (
        rows.iter().filter(|row| row.plaintext == mark).count(),
        rows.iter()
            .any(|row| row.discord_message_id == MESSAGE_ID && row.plaintext == mark),
    )
}

#[test]
fn task1354_both_sides_burn_reaches_closed_receiver_copy_on_reopen() {
    let mark = format!(
        "TASK1354 marked {}",
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect::<String>()
    );
    let sender_identity = keystore::identity_from_entropy([0x54; 16], "task1354-sender".to_owned());
    let sender = make_copy(SENDER_NAME, SENDER_KEY, &sender_identity.user_id, &mark);
    let receiver = make_copy(RECEIVER_NAME, RECEIVER_KEY, &sender_identity.user_id, &mark);

    for copy in [&sender, &receiver] {
        let (count, present) = exact_mark_count(&copy.state, &mark);
        println!(
            "TASK1354 before copy=\"{}\" marked=\"{}\" count={} present={}",
            copy.name, mark, count, present
        );
        assert_eq!(count, 1);
        assert!(present);
    }

    *receiver
        .state
        .message_store
        .lock()
        .expect("receiver store mutex poisoned") = None;
    let receiver_closed_count = exact_mark_count(&receiver.state, &mark).0;
    assert_eq!(receiver_closed_count, 0);

    sender.state.install_identity(sender_identity.clone());
    let server = OfflineReceiverBurnServer::start(
        MESSAGE_ID.to_owned(),
        sender_identity.user_id.clone(),
        sender_identity.ed25519_public,
        receiver.dir.path().to_path_buf(),
    );
    *sender
        .state
        .keyserver
        .lock()
        .expect("keyserver mutex poisoned") =
        Some(KeyServerClient::new(&server.base_url).expect("install loopback keyserver"));

    let result = cmd_osl_burn_sender_message_records_choice(
        &sender.state,
        "both-sides",
        vec![MESSAGE_ID.to_owned()],
    )
    .expect("burn Both Sides once while receiver copy is closed");
    let remote_requests = server.join();
    println!(
        "TASK1354 burn scope_choice=both-sides requested_count={} local_removal_count={} remote_removal_count={} equal_removal_counts={} remote_requests={}",
        result.requested_count,
        result.local_removal_count,
        result.remote_removal_count,
        result.equal_removal_counts,
        remote_requests.join(",")
    );
    assert_eq!(result.requested_count, 1);
    assert_eq!(result.local_removal_count, 1);
    assert_eq!(result.remote_removal_count, 1);
    assert!(result.equal_removal_counts);
    assert_eq!(remote_requests, vec![MESSAGE_ID.to_owned()]);

    let reopened_receiver =
        MessageStore::open(receiver.dir.path(), RECEIVER_KEY).expect("reopen receiver store");
    *receiver
        .state
        .message_store
        .lock()
        .expect("receiver store mutex poisoned") = Some(reopened_receiver);

    for copy in [&sender, &receiver] {
        let (count, present) = exact_mark_count(&copy.state, &mark);
        println!(
            "TASK1354 after_reopen copy=\"{}\" marked=\"{}\" count={} present={}",
            copy.name, mark, count, present
        );
        assert_eq!(count, 0);
        assert!(!present);
    }
}
