use crypto::{ml_kem_768, x25519};
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    derive_detection_key, prose_token_recv_classified, prose_token_send_with_client,
    ProseTokenRecv, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use ipc::wire_v2::{decrypt_v3, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const PRIVATE_WORDS: &str = "task 3954 exact private words";
const FIXED_REFUSAL: &str = "This encrypted message could not be opened";

struct BlobServer {
    base_url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for BlobServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("blob server exits");
        }
    }
}

fn spawn_blob_server() -> BlobServer {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind blob server");
    listener
        .set_nonblocking(true)
        .expect("blob server can stop");
    let address = listener.local_addr().expect("blob server address");
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let mut next_id = 1u64;
        let mut blobs = BTreeMap::<String, (String, Vec<u8>)>::new();
        while !server_stop.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(accepted) => accepted,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept blob request: {error}"),
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("bound request read time");
            let (method, target, headers, body) = read_http_request(&mut stream);
            let response = match (method.as_str(), target.as_str()) {
                ("POST", "/v1/blob") => {
                    let token = headers
                        .get("x-osl-fetch-token")
                        .filter(|value| {
                            value.len() == 32
                                && value.bytes().all(|byte| {
                                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                                })
                        })
                        .cloned();
                    match token {
                        Some(token) if !body.is_empty() => {
                            let id = format!("{next_id:016x}");
                            next_id += 1;
                            blobs.insert(id.clone(), (token, body));
                            json_response(
                                201,
                                serde_json::json!({ "id": id, "expires_at": 2_000_000_000i64 }),
                            )
                        }
                        _ => json_response(
                            400,
                            serde_json::json!({ "error": "fetch_token_required" }),
                        ),
                    }
                }
                ("GET", target) if target.starts_with("/v1/blob/") => {
                    let id = target.trim_start_matches("/v1/blob/");
                    match (blobs.get(id), headers.get("x-osl-fetch-token")) {
                        (Some((stored_token, bytes)), Some(presented))
                            if stored_token == presented =>
                        {
                            bytes_response(200, bytes.clone())
                        }
                        (Some(_), _) => json_response(
                            403,
                            serde_json::json!({ "error": "fetch_token_mismatch" }),
                        ),
                        (None, _) => {
                            json_response(404, serde_json::json!({ "error": "not_found" }))
                        }
                    }
                }
                _ => json_response(404, serde_json::json!({ "error": "not_found" })),
            };
            stream.write_all(&response).expect("write blob response");
        }
    });
    BlobServer {
        base_url: format!("http://{address}"),
        stop,
        thread: Some(thread),
    }
}

fn read_http_request(
    stream: &mut std::net::TcpStream,
) -> (String, String, BTreeMap<String, String>, Vec<u8>) {
    let mut request = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk).expect("read request");
        assert!(read > 0, "request ended before headers");
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        assert!(request.len() <= 16 * 1024, "headers stay bounded");
    }
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("header terminator");
    let header_text = std::str::from_utf8(&request[..header_end]).expect("headers are UTF-8");
    let mut lines = header_text.lines();
    let request_line = lines.next().expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().expect("method").to_owned();
    let target = parts.next().expect("target").to_owned();
    let mut headers = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    assert!(content_length <= 64 * 1024, "body stays bounded");
    while request.len() < header_end + content_length {
        let read = stream.read(&mut chunk).expect("read body");
        assert!(read > 0, "request ended before body");
        request.extend_from_slice(&chunk[..read]);
    }
    (
        method,
        target,
        headers,
        request[header_end..header_end + content_length].to_vec(),
    )
}

fn json_response(status: u16, value: serde_json::Value) -> Vec<u8> {
    response(
        status,
        "application/json",
        serde_json::to_vec(&value).unwrap(),
    )
}

fn bytes_response(status: u16, body: Vec<u8>) -> Vec<u8> {
    response(status, "application/octet-stream", body)
}

fn response(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

fn open_cover(
    config_dir: &std::path::Path,
    scope: &ScopeInput,
    detection_key: &[u8; 32],
    cover_text: &str,
    recipient_secret: &x25519::SecretKey,
    recipient_mlkem_secret: &ml_kem_768::DecapsulationKey,
) -> Result<String, String> {
    match prose_token_recv_classified(config_dir, scope, detection_key, cover_text) {
        Ok(ProseTokenRecv::Recovered(recovered)) => {
            decrypt_v3(&recovered.wire, recipient_secret, recipient_mlkem_secret)
                .map_err(|_| FIXED_REFUSAL.to_owned())
                .and_then(|opened| {
                    String::from_utf8(opened.plaintext).map_err(|_| FIXED_REFUSAL.to_owned())
                })
                .map_err(|_| FIXED_REFUSAL.to_owned())
        }
        Ok(ProseTokenRecv::Missed(_)) | Err(_) => Err(FIXED_REFUSAL.to_owned()),
    }
}

#[test]
fn task_3954_reply_and_forward_wrapped_cover_never_show_silent_empty() {
    let server = spawn_blob_server();
    let config_dir = tempfile::tempdir().expect("config dir");
    std::fs::write(
        config_dir.path().join("keyserver.json"),
        format!(r#"{{"cipher_store_url":"{}"}}"#, server.base_url),
    )
    .expect("write local cipher-store config");

    let (sender_secret, sender_public) = x25519::generate_keypair();
    let (recipient_secret, recipient_public) = x25519::generate_keypair();
    let (recipient_mlkem_secret, recipient_mlkem_public) = ml_kem_768::generate_keypair();
    let scope = ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-3954-sender-local-peer".to_owned(),
        server_id: None,
        channel_id: Some("task-3954-same-conversation".to_owned()),
    };
    let detection_key = derive_detection_key(b"task-3954 same conversation secret").unwrap();
    let wire = encrypt_v3(
        &sender_secret,
        &sender_public,
        &[RecipientV3 {
            x25519_pub: recipient_public,
            mlkem_pub: recipient_mlkem_public,
        }],
        MSG_TYPE_CONTENT,
        PRIVATE_WORDS.as_bytes(),
    )
    .expect("encrypted cover wire");
    let client = CipherStoreClient::new(&server.base_url).expect("loopback client");
    let sent = prose_token_send_with_client(
        &client,
        &scope,
        &detection_key,
        ProseTokenSendKeys {
            message_key: b"task-3954-message-key",
            send_key: b"task-3954-send-key",
            conversation_key: b"task-3954-conversation-key",
        },
        &wire,
        TTL_1H,
    )
    .expect("send cover through loopback blob store");

    let original = open_cover(
        config_dir.path(),
        &scope,
        &detection_key,
        &sent.cover_text,
        &recipient_secret,
        &recipient_mlkem_secret,
    )
    .expect("the unmodified cover opens");
    assert_eq!(original, PRIVATE_WORDS);
    println!("TASK3954 original_cover_outcome=opened value={original:?}");
    println!("TASK3954 same_conversation=task-3954-same-conversation");

    let cases = [
        (
            "reply",
            format!(
                "Replying with a note before the original.\n\n> {}\n\nExtra reply text after it.",
                sent.cover_text
            ),
        ),
        (
            "forward",
            format!(
                "Forwarding this to the same conversation.\n\n{}\n\nExtra forward text after it.",
                sent.cover_text
            ),
        ),
    ];
    let silently_show_nothing_count = 0usize;
    for (case, visible_text) in cases {
        match open_cover(
            config_dir.path(),
            &scope,
            &detection_key,
            &visible_text,
            &recipient_secret,
            &recipient_mlkem_secret,
        ) {
            Ok(plaintext) if plaintext == PRIVATE_WORDS => {
                println!("TASK3954 case={case} outcome=opened value={plaintext:?}");
            }
            Ok(plaintext) if plaintext.is_empty() => {
                panic!("TASK3954 case={case} silently showed nothing and said nothing");
            }
            Ok(plaintext) => {
                panic!("TASK3954 case={case} opened unexpected text: {plaintext:?}");
            }
            Err(refusal) if refusal == FIXED_REFUSAL => {
                println!("TASK3954 case={case} outcome=refused value={refusal:?}");
            }
            Err(refusal) => panic!("TASK3954 case={case} used wrong refusal: {refusal:?}"),
        }
    }
    println!("TASK3954 silently_show_nothing_count={silently_show_nothing_count}");
    assert_eq!(silently_show_nothing_count, 0);
}
