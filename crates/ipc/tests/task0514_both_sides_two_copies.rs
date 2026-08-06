use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
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

const COPY_A_NAME: &str = "OSL Copy A";
const COPY_B_NAME: &str = "OSL Copy B";
const COPY_A_MESSAGE_ID: &str = "task0514-copy-a-marked-message";
const COPY_B_MESSAGE_ID: &str = "task0514-copy-b-marked-message";
const CHANNEL: &str = "task0514-two-copy-channel";
const STORE_KEY_A: &[u8; 32] = &[0x14; 32];
const STORE_KEY_B: &[u8; 32] = &[0x15; 32];

#[derive(Clone)]
struct ExpectedBurn {
    content_id: String,
    user_id: String,
    public_key: crypto::ed25519::PublicKey,
}

struct SignedBurnServer {
    base_url: String,
    removed: Arc<Mutex<BTreeSet<String>>>,
    requests: Arc<Mutex<Vec<String>>>,
    server: JoinHandle<()>,
}

impl SignedBurnServer {
    fn start(expected_burns: Vec<ExpectedBurn>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback burn server");
        let address = listener.local_addr().expect("read loopback burn address");
        let expected: BTreeMap<String, ExpectedBurn> = expected_burns
            .into_iter()
            .map(|burn| (burn.content_id.clone(), burn))
            .collect();
        let removed = Arc::new(Mutex::new(
            expected.keys().cloned().collect::<BTreeSet<_>>(),
        ));
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
                assert!(burn["target_user_id"].is_null());
                let content_id = burn["target_content_id"]
                    .as_str()
                    .expect("single-scope burn names a content id");
                let expected_burn = expected
                    .get(content_id)
                    .expect("remote burn content id was seeded in a named copy");
                assert_eq!(burn["user_id"], expected_burn.user_id);
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
                    &expected_burn.user_id,
                    timestamp_ms,
                    request_id,
                    &BurnScope::Single {
                        content_id: content_id.to_owned(),
                    },
                );
                assert!(
                    crypto::ed25519::verify(&expected_burn.public_key, &canonical, &signature)
                        .unwrap(),
                    "remote burn request signature must verify for the named copy"
                );
                requests_for_server
                    .lock()
                    .expect("lock requests")
                    .push(content_id.to_owned());
                let deleted = removed_for_server
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

struct LocalCopy {
    name: &'static str,
    message_id: &'static str,
    _dir: tempfile::TempDir,
    state: AppState,
}

fn make_copy(
    name: &'static str,
    message_id: &'static str,
    store_key: &[u8; 32],
    identity_entropy: [u8; 16],
    keyserver_url: &str,
    mark: &str,
) -> (LocalCopy, ExpectedBurn) {
    let dir = tempfile::tempdir().expect("local copy store dir");
    let state = AppState::new();
    let identity = keystore::identity_from_entropy(
        identity_entropy,
        name.replace(' ', "-").to_ascii_lowercase(),
    );
    let expected = ExpectedBurn {
        content_id: message_id.to_owned(),
        user_id: identity.user_id.clone(),
        public_key: identity.ed25519_public,
    };
    state.install_identity(identity);
    *state.keyserver.lock().expect("keyserver mutex poisoned") =
        Some(KeyServerClient::new(keyserver_url).expect("install loopback keyserver"));
    let store = MessageStore::open(dir.path(), store_key).expect("open message store");
    store
        .put(&StoredMessage {
            discord_message_id: message_id.to_owned(),
            channel_id: CHANNEL.to_owned(),
            sender_discord_id: expected.user_id.clone(),
            sender_osl_user_id: expected.user_id.clone(),
            plaintext: mark.to_owned(),
            decrypted_at: 1_900_514_000,
            burned: false,
        })
        .expect("seed marked message in named copy");
    *state
        .message_store
        .lock()
        .expect("message store mutex poisoned") = Some(store);
    (
        LocalCopy {
            name,
            message_id,
            _dir: dir,
            state,
        },
        expected,
    )
}

fn exact_mark_count(copy: &LocalCopy, mark: &str) -> (usize, bool, Option<String>) {
    let rows =
        cmd_osl_load_channel_history(&copy.state, CHANNEL.to_owned(), Some(10)).expect("history");
    let count = rows.iter().filter(|row| row.plaintext == mark).count();
    let exact_text = rows
        .iter()
        .find(|row| row.discord_message_id == copy.message_id)
        .map(|row| row.plaintext.clone());
    (count, exact_text.as_deref() == Some(mark), exact_text)
}

#[test]
fn both_sides_burn_removes_one_marked_message_from_both_named_copies() {
    let mark: String = format!(
        "TASK0514 marked {}",
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect::<String>()
    );
    let expected = vec![
        ExpectedBurn {
            content_id: COPY_A_MESSAGE_ID.to_owned(),
            user_id: COPY_A_NAME.replace(' ', "-").to_ascii_lowercase(),
            public_key: keystore::identity_from_entropy(
                [0x14; 16],
                COPY_A_NAME.replace(' ', "-").to_ascii_lowercase(),
            )
            .ed25519_public,
        },
        ExpectedBurn {
            content_id: COPY_B_MESSAGE_ID.to_owned(),
            user_id: COPY_B_NAME.replace(' ', "-").to_ascii_lowercase(),
            public_key: keystore::identity_from_entropy(
                [0x15; 16],
                COPY_B_NAME.replace(' ', "-").to_ascii_lowercase(),
            )
            .ed25519_public,
        },
    ];
    let server = SignedBurnServer::start(expected);
    let (copy_a, expected_a) = make_copy(
        COPY_A_NAME,
        COPY_A_MESSAGE_ID,
        STORE_KEY_A,
        [0x14; 16],
        &server.base_url,
        &mark,
    );
    let (copy_b, expected_b) = make_copy(
        COPY_B_NAME,
        COPY_B_MESSAGE_ID,
        STORE_KEY_B,
        [0x15; 16],
        &server.base_url,
        &mark,
    );
    assert_eq!(expected_a.content_id, COPY_A_MESSAGE_ID);
    assert_eq!(expected_b.content_id, COPY_B_MESSAGE_ID);

    for copy in [&copy_a, &copy_b] {
        let (count, present, exact_text) = exact_mark_count(copy, &mark);
        println!(
            "TASK0514 before copy=\"{}\" exact_text=\"{}\" count={} present={}",
            copy.name,
            exact_text.as_deref().unwrap_or("<absent>"),
            count,
            present
        );
        assert_eq!(exact_text.as_deref(), Some(mark.as_str()));
        assert_eq!(count, 1);
        assert!(present);
    }

    for copy in [&copy_a, &copy_b] {
        let result = cmd_osl_burn_sender_message_records_choice(
            &copy.state,
            "both-sides",
            vec![copy.message_id.to_owned()],
        )
        .expect("burn Both Sides once for named copy");
        println!(
            "TASK0514 burn copy=\"{}\" scope_choice=both-sides requested_count={} local_removal_count={} remote_removal_count={} equal_removal_counts={}",
            copy.name,
            result.requested_count,
            result.local_removal_count,
            result.remote_removal_count,
            result.equal_removal_counts
        );
        assert_eq!(result.requested_count, 1);
        assert_eq!(result.local_removal_count, 1);
        assert_eq!(result.remote_removal_count, 1);
        assert!(result.equal_removal_counts);
    }
    let (remote_requests, remote_remaining) = server.join();
    assert_eq!(
        remote_requests,
        vec![COPY_A_MESSAGE_ID.to_owned(), COPY_B_MESSAGE_ID.to_owned()]
    );
    assert_eq!(remote_remaining, 0);

    for copy in [&copy_a, &copy_b] {
        let (count, present, exact_text) = exact_mark_count(copy, &mark);
        println!(
            "TASK0514 after copy=\"{}\" exact_text_present={} count={} present={}",
            copy.name,
            exact_text.is_some(),
            count,
            present
        );
        assert_eq!(count, 0);
        assert!(!present);
        assert!(exact_text.is_none());
    }
    println!(
        "TASK0514 remote_requests={} remote_remaining={} marked=\"{}\"",
        remote_requests.join(","),
        remote_remaining,
        mark
    );
}
