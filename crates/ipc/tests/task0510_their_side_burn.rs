use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::cmd_osl_burn_their_side_message;
use ipc::AppState;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
struct WrappedCopy {
    content_id: String,
    content_type: String,
    sender_id: String,
    recipient_id: String,
    session_version: u32,
    share_index: u32,
    wrapped_share_blob: String,
    blob_version: u32,
    single_use: bool,
    expires_at: String,
}

struct Task0510Server {
    base_url: String,
    server: JoinHandle<()>,
}

impl Task0510Server {
    fn start(expected_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind task0510 server");
        let address = listener.local_addr().expect("read task0510 server address");
        let copies = Arc::new(Mutex::new(HashMap::<String, WrappedCopy>::new()));
        let server_copies = Arc::clone(&copies);
        let server = thread::spawn(move || {
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept task0510 request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set task0510 read timeout");
                handle_request(&mut stream, &server_copies);
            }
        });
        Self {
            base_url: format!("http://{address}"),
            server,
        }
    }

    fn join(self) {
        self.server.join().expect("task0510 server exits");
    }
}

fn handle_request(stream: &mut TcpStream, copies: &Arc<Mutex<HashMap<String, WrappedCopy>>>) {
    let request = read_request(stream);
    let request_text = String::from_utf8_lossy(&request);
    let (head, body) = request_text
        .split_once("\r\n\r\n")
        .expect("request has headers and body");
    let request_line = head.lines().next().expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let path_without_query = path.split_once('?').map(|(path, _)| path).unwrap_or(path);
    match (method, path_without_query) {
        ("POST", "/v1/wrapped-keys") => {
            let parsed = serde_json::from_str::<Value>(body).expect("wrapped-key post JSON");
            let copy = WrappedCopy {
                content_id: str_field(&parsed, "content_id"),
                content_type: str_field(&parsed, "content_type"),
                sender_id: str_field(&parsed, "sender_id"),
                recipient_id: str_field(&parsed, "recipient_id"),
                session_version: parsed["session_version"].as_u64().unwrap_or(0) as u32,
                share_index: parsed["share_index"].as_u64().unwrap_or(0) as u32,
                wrapped_share_blob: str_field(&parsed, "wrapped_share_blob"),
                blob_version: parsed["blob_version"].as_u64().unwrap_or(0) as u32,
                single_use: parsed["single_use"].as_bool().unwrap_or(false),
                expires_at: str_field(&parsed, "expires_at"),
            };
            copies
                .lock()
                .expect("lock task0510 copies")
                .insert(copy.content_id.clone(), copy.clone());
            write_json(stream, 201, json!({ "content_id": copy.content_id }));
        }
        ("GET", route) if route.starts_with("/v1/wrapped-keys/") => {
            let content_id = route.trim_start_matches("/v1/wrapped-keys/");
            let requester = query_param(path, "recipient_id").unwrap_or_default();
            let copy = copies
                .lock()
                .expect("lock task0510 copies")
                .get(content_id)
                .cloned();
            match copy {
                None => write_json(
                    stream,
                    404,
                    json!({ "error": "unknown or burned content_id" }),
                ),
                Some(copy) if copy.recipient_id != requester => {
                    write_json(stream, 403, json!({ "error": "recipient mismatch" }))
                }
                Some(copy) => write_json(
                    stream,
                    200,
                    json!({
                        "content_id": copy.content_id,
                        "content_type": copy.content_type,
                        "system_message_kind": null,
                        "sender_id": copy.sender_id,
                        "recipient_id": copy.recipient_id,
                        "session_version": copy.session_version,
                        "share_index": copy.share_index,
                        "wrapped_share_blob": copy.wrapped_share_blob,
                        "blob_version": copy.blob_version,
                        "single_use": copy.single_use,
                        "display_duration_seconds": null,
                        "expires_at": copy.expires_at,
                        "created_at": "2026-08-06T00:00:00.000Z",
                    }),
                ),
            }
        }
        ("DELETE", "/v1/wrapped-keys") => {
            let burn = serde_json::from_str::<Value>(body).expect("burn request JSON");
            assert_eq!(burn["scope"], "single");
            let user_id = str_field(&burn, "user_id");
            let target_content_id = str_field(&burn, "target_content_id");
            let removed = {
                let mut copies = copies.lock().expect("lock task0510 copies");
                match copies.get(&target_content_id) {
                    Some(copy) if copy.sender_id == user_id => {
                        copies.remove(&target_content_id);
                        1_u32
                    }
                    _ => 0_u32,
                }
            };
            write_json(
                stream,
                200,
                json!({ "scope": "single", "deleted_count": removed }),
            );
        }
        _ => write_json(stream, 404, json!({ "error": "not found" })),
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read task0510 request");
        assert_ne!(read, 0, "request ended before headers arrived");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("content length numeric")
                })
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

fn str_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.to_owned())
    })
}

fn write_json(stream: &mut TcpStream, status: u16, body: Value) {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        201 => "Created",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
    .expect("write task0510 response");
}

fn random_hex(bytes: usize) -> String {
    crypto::random::random_bytes(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn upload_copy(
    client: &keystore::KeyServerClient,
    sender: &keystore::Identity,
    content_id: &str,
    recipient_id: &str,
    mark: &str,
) {
    client
        .post_wrapped_key(
            sender,
            &keystore::wrapped_key::WrappedKeyUpload {
                content_id: content_id.to_owned(),
                content_type: "text".to_owned(),
                system_message_kind: None,
                recipient_id: recipient_id.to_owned(),
                session_version: 1,
                share_index: 0,
                wrapped_share_blob: STANDARD.encode(mark.as_bytes()),
                blob_version: 1,
                single_use: false,
                display_duration_seconds: None,
                expires_at: "2099-01-01T00:00:00.000Z".to_owned(),
            },
        )
        .expect("upload marked wrapped-key copy");
}

fn fetch_mark_count(
    client: &keystore::KeyServerClient,
    recipient: &keystore::Identity,
    content_id: &str,
    expected_mark: &str,
) -> (usize, Option<String>) {
    match client.fetch_wrapped_key(recipient, content_id) {
        Ok(row) => {
            let text = String::from_utf8(
                STANDARD
                    .decode(row.wrapped_share_blob.as_bytes())
                    .expect("decode wrapped mark"),
            )
            .expect("wrapped mark is UTF-8");
            assert_eq!(text, expected_mark);
            (1, Some(text))
        }
        Err(keystore::Error::HttpStatus { status: 404, .. }) => (0, None),
        Err(error) => panic!("unexpected wrapped-key fetch error: {error}"),
    }
}

#[test]
fn their_side_burn_removes_only_the_recipient_named_copy() {
    let relay = Task0510Server::start(7);
    let client = keystore::KeyServerClient::new(&relay.base_url).expect("build task0510 client");
    let sender = keystore::generate_identity(format!("task0510-sender-{}", random_hex(4)));
    let recipient = keystore::generate_identity(format!("task0510-recipient-{}", random_hex(4)));
    let mark = format!("TASK0510 marked message {}", random_hex(16));
    let sender_content_id = format!("task0510-sender-copy-{}", random_hex(8));
    let recipient_content_id = format!("task0510-recipient-copy-{}", random_hex(8));

    upload_copy(&client, &sender, &sender_content_id, &sender.user_id, &mark);
    upload_copy(
        &client,
        &sender,
        &recipient_content_id,
        &recipient.user_id,
        &mark,
    );

    let (sender_before_count, sender_before_text) =
        fetch_mark_count(&client, &sender, &sender_content_id, &mark);
    let (recipient_before_count, recipient_before_text) =
        fetch_mark_count(&client, &recipient, &recipient_content_id, &mark);
    println!(
        "TASK0510 initial copy=sender count={sender_before_count} text=\"{}\"",
        sender_before_text.as_deref().unwrap_or("")
    );
    println!(
        "TASK0510 initial copy=recipient count={recipient_before_count} text=\"{}\"",
        recipient_before_text.as_deref().unwrap_or("")
    );
    assert_eq!(sender_before_count, 1);
    assert_eq!(recipient_before_count, 1);
    assert_eq!(sender_before_text.as_deref(), Some(mark.as_str()));
    assert_eq!(recipient_before_text.as_deref(), Some(mark.as_str()));

    let state = AppState::new();
    state.install_identity(sender.clone());
    *state.keyserver.lock().expect("keyserver lock") = Some(client.clone());

    let burn = cmd_osl_burn_their_side_message(&state, recipient_content_id.clone())
        .expect("burn Their Side once");
    println!(
        "TASK0510 burn choice=\"{}\" target_content_id={} remote_removal_count={}",
        burn.choice, burn.target_content_id, burn.remote_removal_count
    );
    assert_eq!(burn.choice, "Their Side");
    assert_eq!(burn.target_content_id, recipient_content_id);
    assert_eq!(burn.remote_removal_count, 1);

    let (sender_after_count, sender_after_text) =
        fetch_mark_count(&client, &sender, &sender_content_id, &mark);
    let (recipient_after_count, recipient_after_text) =
        fetch_mark_count(&client, &recipient, &recipient_content_id, &mark);
    let sender_mark_absent = sender_after_text.as_deref() != Some(mark.as_str());
    let recipient_mark_absent = recipient_after_text.as_deref() != Some(mark.as_str());
    println!(
        "TASK0510 after_server_fetch copy=sender count={sender_after_count} mark_present={}",
        !sender_mark_absent
    );
    println!(
        "TASK0510 after_server_fetch copy=recipient count={recipient_after_count} mark_present={}",
        !recipient_mark_absent
    );
    println!(
        "TASK0510 absent_marks sender_absent={sender_mark_absent} recipient_absent={recipient_mark_absent}"
    );
    assert_eq!(sender_after_count, 1);
    assert_eq!(recipient_after_count, 0);
    assert!(!sender_mark_absent);
    assert!(recipient_mark_absent);

    relay.join();
}
