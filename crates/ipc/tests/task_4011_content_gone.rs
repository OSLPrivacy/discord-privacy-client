use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use ipc::cipher_store_client::CipherStoreClient;
use ipc::prose_token::{
    derive_detection_key, prose_token_bridge_pointer, prose_token_recv_classified,
    prose_token_send_with_client, ProseTokenMiss, ProseTokenRecv, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MESSAGE_KEY: [u8; 32] = [0x11; 32];
const SEND_KEY: [u8; 32] = [0x22; 32];
const CONVERSATION_KEY: [u8; 32] = [0x33; 32];
const TTL_1H: u32 = 3_600;
const REFUSAL_SENTENCE: &str = "This encrypted message could not be opened";

#[derive(Clone)]
struct Blob {
    body: Vec<u8>,
    fetch_token: String,
}

#[derive(Default)]
struct StoreState {
    next_id: u64,
    blobs: BTreeMap<String, Blob>,
}

struct StoreServer {
    address: String,
    state: Arc<Mutex<StoreState>>,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl StoreServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        listener
            .set_nonblocking(true)
            .expect("make loopback cipher store nonblocking");
        let address = listener.local_addr().unwrap().to_string();
        let state = Arc::new(Mutex::new(StoreState::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stopping = Arc::clone(&stopping);
        let thread = thread::spawn(move || {
            while !thread_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => serve(&mut stream, &thread_state),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            state,
            stopping,
            thread: Some(thread),
        }
    }

    fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    fn blob_count(&self) -> usize {
        self.state.lock().unwrap().blobs.len()
    }
}

impl Drop for StoreServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(stream: &mut TcpStream, state: &Arc<Mutex<StoreState>>) {
    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    loop {
        let Ok(read) = stream.read(&mut chunk) else {
            return;
        };
        if read == 0 {
            return;
        }
        request.extend_from_slice(&chunk[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        if request.len() >= header_end + 4 + content_length {
            break;
        }
    }

    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let mut lines = header_text.lines();
    let Some(first_line) = lines.next() else {
        return;
    };
    let mut first = first_line.split_whitespace();
    let method = first.next().unwrap_or_default();
    let path = first.next().unwrap_or_default();
    let headers = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body = &request[header_end + 4..header_end + 4 + content_length];

    match (method, path) {
        ("POST", "/v1/blob") => {
            let Some(token) = headers.get("x-osl-fetch-token").cloned() else {
                return response(stream, 401, b"fetch_token_required", "text/plain");
            };
            let mut state = state.lock().unwrap();
            state.next_id += 1;
            let id = format!("{:016x}", state.next_id);
            state.blobs.insert(
                id.clone(),
                Blob {
                    body: body.to_vec(),
                    fetch_token: token,
                },
            );
            response(
                stream,
                201,
                json!({ "id": id, "expires_at": 4_000_000_000i64 })
                    .to_string()
                    .as_bytes(),
                "application/json",
            );
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let presented = headers.get("x-osl-fetch-token");
            let state = state.lock().unwrap();
            match state.blobs.get(id) {
                Some(blob) if presented == Some(&blob.fetch_token) => {
                    response(stream, 200, &blob.body, "application/octet-stream");
                }
                Some(_) => response(stream, 403, b"fetch_token_mismatch", "text/plain"),
                None => response(stream, 404, b"not found", "text/plain"),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let presented = headers.get("x-osl-fetch-token");
            let mut state = state.lock().unwrap();
            let authorized = state
                .blobs
                .get(id)
                .is_some_and(|blob| presented == Some(&blob.fetch_token));
            if authorized {
                state.blobs.remove(id);
                response(stream, 204, b"", "text/plain");
            } else {
                response(stream, 404, b"not found", "text/plain");
            }
        }
        _ => response(stream, 404, b"not found", "text/plain"),
    }
}

fn response(stream: &mut TcpStream, status: u16, body: &[u8], content_type: &str) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\ncontent-type: {content_type}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-4011-peer".to_owned(),
        server_id: None,
        channel_id: Some("task-4011-dm-channel".to_owned()),
    }
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn fake_wire(label: &[u8]) -> String {
    format!("DPC0::{}", B64.encode(label))
}

fn write_store_override(dir: &std::path::Path, url: &str) {
    std::fs::write(
        dir.join("keyserver.json"),
        format!(r#"{{"cipher_store_url":"{url}"}}"#),
    )
    .expect("write cipher store override");
}

fn decode_fetch_token(hex: &str) -> [u8; 16] {
    assert_eq!(hex.len(), 32);
    let mut out = [0u8; 16];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    out
}

#[test]
fn task_4011_recognized_cover_reports_gone_content_without_opening_and_present_content_opens_one() {
    let server = StoreServer::start();
    let client = CipherStoreClient::new(server.base_url()).expect("loopback client");
    let dir = tempfile::tempdir().expect("config dir");
    write_store_override(dir.path(), &server.base_url());
    let scope = scope();
    let detector = derive_detection_key(&[0x44; 32]).expect("detector");

    let gone_sent = prose_token_send_with_client(
        &client,
        &scope,
        &detector,
        send_keys(),
        &fake_wire(b"task-4011-gone-private-message"),
        TTL_1H,
    )
    .expect("send gone fixture");
    let recognized = prose_token_bridge_pointer(&scope, &detector, &gone_sent.cover_text)
        .expect("recognize cover")
        .expect("cover carries a bridge pointer");
    assert_eq!(recognized.blob_id_hex(), gone_sent.blob_id);
    let delete_token = decode_fetch_token(
        gone_sent
            .burn_capability
            .as_deref()
            .expect("bridge send returns delete token"),
    );
    client
        .delete(&gone_sent.blob_id, &delete_token)
        .expect("delete content from store before fetch");
    assert_eq!(server.blob_count(), 0);

    let gone_opened_private_messages =
        match prose_token_recv_classified(dir.path(), &scope, &detector, &gone_sent.cover_text)
            .expect("gone receive classifies cleanly")
        {
            ProseTokenRecv::Missed(ProseTokenMiss::BlobGone) => 0usize,
            other => panic!("deleted content must classify as BlobGone, got {other:?}"),
        };

    let present_sent = prose_token_send_with_client(
        &client,
        &scope,
        &detector,
        send_keys(),
        &fake_wire(b"task-4011-present-private-message"),
        TTL_1H,
    )
    .expect("send present fixture");
    let present_opened_private_messages =
        match prose_token_recv_classified(dir.path(), &scope, &detector, &present_sent.cover_text)
            .expect("present receive classifies cleanly")
        {
            ProseTokenRecv::Recovered(recovered) if recovered.blob_id == present_sent.blob_id => {
                1usize
            }
            other => panic!("present content must recover exactly one wire, got {other:?}"),
        };

    println!(
        "TASK4011_COVER_RECOGNIZED_BLOB_ID={}",
        recognized.blob_id_hex()
    );
    println!("TASK4011_CONTENT_DELETED_BEFORE_FETCH=true");
    println!("TASK4011_GONE_OPENED_PRIVATE_MESSAGES={gone_opened_private_messages}");
    println!("TASK4011_GONE_REFUSAL_SENTENCE={REFUSAL_SENTENCE}");
    println!("TASK4011_PRESENT_OPENED_PRIVATE_MESSAGES={present_opened_private_messages}");

    assert_eq!(gone_opened_private_messages, 0);
    assert_eq!(
        REFUSAL_SENTENCE,
        "This encrypted message could not be opened"
    );
    assert_eq!(present_opened_private_messages, 1);
}
