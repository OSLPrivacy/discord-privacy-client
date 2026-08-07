use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keystore::{canonical_burn_bytes, BurnScope};
use serde_json::Value;

const BURN_CHOICE: &str = env!("CARGO_BIN_EXE_burn-choice");

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
fn chat_burn_result_names_open_chat_id_and_selected_scope() {
    let open_chat_id = "task0525-open-chat";
    let selected_scope = "both-sides";
    let selected = vec![
        "333333333333333332".to_owned(),
        "333333333333333331".to_owned(),
    ];
    let identity = keystore::identity_from_entropy([0x25; 16], "burner-0525".to_owned());
    let server = SignedBurnServer::start(
        selected.clone(),
        "burner-0525".to_owned(),
        identity.ed25519_public,
    );

    let output = Command::new(BURN_CHOICE)
        .arg("chat-burn")
        .arg("--open-chat-id")
        .arg(open_chat_id)
        .arg("--scope")
        .arg(selected_scope)
        .arg("--keyserver-url")
        .arg(&server.base_url)
        .output()
        .expect("run burn-choice chat-burn");
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(
        output.status.success(),
        "chat-burn should succeed, stdout={stdout}, stderr={stderr}"
    );
    let (requests, remaining) = server.join();
    assert_eq!(requests, selected);
    assert_eq!(remaining, 0);
    assert!(stdout.contains("TASK0525 chat_burn_result"), "{stdout}");
    assert!(
        stdout.contains("open_chat_id=task0525-open-chat"),
        "{stdout}"
    );
    assert!(stdout.contains("selected_scope=both-sides"), "{stdout}");
    assert!(
        stdout.contains("selected_message_ids=333333333333333332,333333333333333331"),
        "{stdout}"
    );
    assert!(!stdout.contains("444444444444444441"), "{stdout}");
    assert!(!stdout.contains("555555555555555551"), "{stdout}");

    println!("{}", stdout.trim());
    println!(
        "TASK0525 open_chat_id={open_chat_id} selected_scope={selected_scope} selected_message_ids={} remote_requests={} remote_remaining={remaining}",
        selected.join(","),
        requests.join(",")
    );
}
