use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keystore::canonical_wrapped_key_open_claim_bytes;
use serde_json::Value;

const VIEW_ONCE_CLAIM: &str = env!("CARGO_BIN_EXE_view-once-claim");
const CONTENT_ID: &str = "task0557-protected-record";
const RECIPIENT_ID: &str = "task0557-recipient";
const ENTROPY_HEX: &str = "57575757575757575757575757575757";

struct OpenClaimServer {
    base_url: String,
    opened: Arc<Mutex<bool>>,
    server: JoinHandle<()>,
}

impl OpenClaimServer {
    fn start(public_key: crypto::ed25519::PublicKey) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback open-claim server");
        let address = listener.local_addr().expect("read loopback address");
        let opened = Arc::new(Mutex::new(false));
        let server_opened = Arc::clone(&opened);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept open claim");
                let request = read_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                let (head, body) = request_text
                    .split_once("\r\n\r\n")
                    .expect("request has headers and body");
                assert!(
                    head.starts_with(
                        "POST /v1/wrapped-keys/task0557-protected-record/opened HTTP/1.1"
                    ),
                    "view-once command must call the opened claim endpoint"
                );
                let claim = serde_json::from_str::<Value>(body).expect("claim request is JSON");
                assert_eq!(claim["recipient_id"], RECIPIENT_ID);
                let timestamp_ms = claim["timestamp_ms"]
                    .as_i64()
                    .expect("claim carries timestamp");
                let request_id = claim["request_id"]
                    .as_str()
                    .expect("claim carries request_id");
                let signature_b64 = claim["open_signature_b64"]
                    .as_str()
                    .expect("claim carries signature");
                let signature_bytes = STANDARD.decode(signature_b64).expect("signature is base64");
                let signature_array: [u8; crypto::ed25519::SIGNATURE_SIZE] =
                    signature_bytes.try_into().expect("signature is 64 bytes");
                let signature = crypto::ed25519::Signature::from_bytes(signature_array);
                let canonical = canonical_wrapped_key_open_claim_bytes(
                    RECIPIENT_ID,
                    CONTENT_ID,
                    timestamp_ms,
                    request_id,
                );
                assert!(
                    crypto::ed25519::verify(&public_key, &canonical, &signature).unwrap(),
                    "view-once opened claim signature must verify"
                );

                let mut opened = server_opened.lock().expect("lock opened state");
                if !*opened {
                    *opened = true;
                    write_response(
                        &mut stream,
                        200,
                        r#"{"content_id":"task0557-protected-record","opened":true}"#,
                    );
                } else {
                    write_response(
                        &mut stream,
                        409,
                        r#"{"error":"single-use protected record already opened"}"#,
                    );
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            opened,
            server,
        }
    }

    fn join(self) -> bool {
        self.server.join().expect("open claim server exits cleanly");
        *self.opened.lock().expect("lock opened state")
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read open claim");
        assert_ne!(read, 0, "open claim ended before its body arrived");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .expect("claim has content length")
            .parse::<usize>()
            .expect("content length is numeric");
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        409 => "Conflict",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write open claim response");
}

#[test]
fn first_claim_succeeds_and_second_claim_for_same_record_exits_1() {
    let identity = keystore::identity_from_entropy([0x57; 16], RECIPIENT_ID.to_owned());
    let server = OpenClaimServer::start(identity.ed25519_public);

    let first = Command::new(VIEW_ONCE_CLAIM)
        .arg("--keyserver-url")
        .arg(&server.base_url)
        .arg("--recipient-id")
        .arg(RECIPIENT_ID)
        .arg("--content-id")
        .arg(CONTENT_ID)
        .arg("--identity-entropy-hex")
        .arg(ENTROPY_HEX)
        .output()
        .expect("run first view-once claim");
    let first_stdout = String::from_utf8(first.stdout).expect("first stdout UTF-8");
    let first_stderr = String::from_utf8(first.stderr).expect("first stderr UTF-8");
    assert!(
        first.status.success(),
        "first claim must succeed, stdout={first_stdout}, stderr={first_stderr}"
    );
    assert!(first_stdout.contains("opened=true"), "{first_stdout}");

    let second = Command::new(VIEW_ONCE_CLAIM)
        .arg("--keyserver-url")
        .arg(&server.base_url)
        .arg("--recipient-id")
        .arg(RECIPIENT_ID)
        .arg("--content-id")
        .arg(CONTENT_ID)
        .arg("--identity-entropy-hex")
        .arg(ENTROPY_HEX)
        .output()
        .expect("run second view-once claim");
    let second_stderr = String::from_utf8(second.stderr).expect("second stderr UTF-8");
    assert_eq!(second.status.code(), Some(1));
    assert!(
        second_stderr.contains("view-once claim failed"),
        "{second_stderr}"
    );
    assert!(server.join());

    println!(
        "TASK0557 first_claim=opened first_exit={} second_claim=same-record second_exit=1 protected_record_opened=true",
        first.status.code().unwrap_or(0)
    );
}
