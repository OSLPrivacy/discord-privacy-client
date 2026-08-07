use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::attachment_wire::decoy_png;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{prose_token_recv, prose_token_send_with_client, ProseTokenSendKeys};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGE_KEY: [u8; 32] = [0x62; 32];
const SEND_KEY: [u8; 32] = [0x26; 32];
const CONVERSATION_KEY: [u8; 32] = [0x20; 32];
const CHECK_MARK_SENT: &str = "task-0662-check-mark";

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-0662-recipient".to_owned(),
        server_id: None,
        channel_id: Some("task-0662-dm-channel".to_owned()),
    }
}

#[derive(Default)]
struct StoreState {
    next_id: u64,
    blobs: HashMap<String, StoredBlob>,
}

struct StoredBlob {
    fetch_token: String,
    body: Vec<u8>,
}

struct LoopbackCipherStore {
    base_url: String,
    server: thread::JoinHandle<()>,
}

impl LoopbackCipherStore {
    fn start(expected_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        let address = listener.local_addr().expect("loopback address");
        let state = Arc::new(Mutex::new(StoreState::default()));
        let server_state = Arc::clone(&state);
        let server = thread::spawn(move || {
            for _ in 0..expected_requests {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                handle_request(stream, &server_state);
            }
        });
        Self {
            base_url: format!("http://{address}"),
            server,
        }
    }
}

fn config_dir(base_url: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp config dir");
    std::fs::write(
        dir.path().join("keyserver.json"),
        format!(r#"{{"cipher_store_url":"{base_url}"}}"#),
    )
    .expect("write cipher_store_url override");
    dir
}

fn dpc0_wire(bytes: &[u8]) -> String {
    format!("DPC0::{}", STANDARD.encode(bytes))
}

fn read_request(
    stream: &mut TcpStream,
) -> Option<(String, String, HashMap<String, String>, Vec<u8>)> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).ok()?;
    let mut parts = first.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }

    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some((method, path, headers, body))
}

fn handle_request(mut stream: TcpStream, state: &Arc<Mutex<StoreState>>) {
    let Some((method, path, headers, body)) = read_request(&mut stream) else {
        return;
    };
    let response = match (method.as_str(), path.as_str()) {
        ("POST", "/v1/blob") => {
            let fetch_token = headers
                .get("x-osl-fetch-token")
                .cloned()
                .unwrap_or_default();
            let ttl_seconds = headers
                .get("x-osl-ttl-seconds")
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            let mut store = state.lock().expect("store state");
            store.next_id += 1;
            let id = format!("{:016x}", store.next_id);
            store
                .blobs
                .insert(id.clone(), StoredBlob { fetch_token, body });
            let expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64
                + ttl_seconds;
            json_response(
                201,
                &json!({ "id": id, "expires_at": expires_at }).to_string(),
            )
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let fetch_token = headers
                .get("x-osl-fetch-token")
                .cloned()
                .unwrap_or_default();
            let store = state.lock().expect("store state");
            match store.blobs.get(id) {
                Some(blob) if blob.fetch_token == fetch_token => bytes_response(&blob.body),
                Some(_) => text_response(403, "forbidden"),
                None => text_response(404, "not found"),
            }
        }
        _ => text_response(404, "not found"),
    };
    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

fn json_response(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn text_response(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} X\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn bytes_response(bytes: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    )
    .into_bytes();
    response.extend_from_slice(bytes);
    response
}

#[test]
fn fixture_image_pointer_reads_back_through_recipient_command() {
    let store = LoopbackCipherStore::start(2);
    let config = config_dir(&store.base_url);
    let client = CipherStoreClient::new(&store.base_url).expect("loopback client");
    let detection_key =
        ipc::prose_token::derive_detection_key(&[0x66; 32]).expect("derive detection key");

    let sent_payload = json!({
        "imageB64": STANDARD.encode(decoy_png()),
        "checkMark": CHECK_MARK_SENT,
    });
    let sent_wire = dpc0_wire(sent_payload.to_string().as_bytes());
    let sent = prose_token_send_with_client(
        &client,
        &scope(),
        &detection_key,
        send_keys(),
        &sent_wire,
        TTL_1H,
    )
    .expect("encode fixture image pointer");

    let returned = prose_token_recv(config.path(), &scope(), &detection_key, &sent.cover_text)
        .expect("recipient command succeeds")
        .expect("recipient command returns a pointer-backed wire");
    let returned_payload_b64 = returned
        .wire
        .strip_prefix("DPC0::")
        .expect("recipient wire keeps DPC0 prefix");
    let returned_payload_json = STANDARD
        .decode(returned_payload_b64)
        .expect("recipient payload is base64");
    let returned_payload: serde_json::Value =
        serde_json::from_slice(&returned_payload_json).expect("recipient payload is JSON");
    let returned_check_mark = returned_payload["checkMark"]
        .as_str()
        .expect("checkMark is a string");
    let returned_image = STANDARD
        .decode(
            returned_payload["imageB64"]
                .as_str()
                .expect("imageB64 is a string"),
        )
        .expect("returned image is base64");

    println!("TASK0662_SENT_POINTER={}", sent.blob_id);
    println!("TASK0662_RETURNED_POINTER={}", returned.blob_id);
    println!("TASK0662_SENT_CHECK_MARK={CHECK_MARK_SENT}");
    println!("TASK0662_RETURNED_CHECK_MARK={returned_check_mark}");
    println!("TASK0662_RETURNED_IMAGE_BYTES={}", returned_image.len());

    assert_eq!(returned.blob_id, sent.blob_id);
    assert_eq!(returned_check_mark, CHECK_MARK_SENT);
    assert_eq!(returned_image, decoy_png());

    store
        .server
        .join()
        .expect("loopback store handled requests");
}
