use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    prose_token_recv_classified, prose_token_send_with_client, ProseTokenMiss, ProseTokenRecv,
    ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MESSAGE_KEY: [u8; 32] = [0x39; 32];
const SEND_KEY: [u8; 32] = [0x60; 32];
const CONVERSATION_KEY: [u8; 32] = [0x40; 32];
const EXACT_PRIVATE_WORDS: &str = "task 3960 exact private words";

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn scope(id: &str, channel: &str) -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: id.to_owned(),
        server_id: None,
        channel_id: Some(channel.to_owned()),
    }
}

#[derive(Default)]
struct StoreState {
    next_id: u64,
    fetches: usize,
    blobs: HashMap<String, StoredBlob>,
}

struct StoredBlob {
    fetch_token: String,
    body: Vec<u8>,
}

struct LoopbackCipherStore {
    base_url: String,
    state: Arc<Mutex<StoreState>>,
    stopping: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}

impl LoopbackCipherStore {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        listener
            .set_nonblocking(true)
            .expect("make loopback listener nonblocking");
        let address = listener.local_addr().expect("loopback address");
        let state = Arc::new(Mutex::new(StoreState::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        let server_state = Arc::clone(&state);
        let server_stopping = Arc::clone(&stopping);
        let server = thread::spawn(move || {
            while !server_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => handle_request(stream, &server_state),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            base_url: format!("http://{address}"),
            state,
            stopping,
            server: Some(server),
        }
    }

    fn fetches(&self) -> usize {
        self.state.lock().expect("store state").fetches
    }
}

impl Drop for LoopbackCipherStore {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.base_url.trim_start_matches("http://"));
        if let Some(server) = self.server.take() {
            let _ = server.join();
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

fn dpc0_wire(plaintext: &str) -> String {
    format!("DPC0::{}", STANDARD.encode(plaintext.as_bytes()))
}

fn open_row(
    config: &std::path::Path,
    this_pair_scope: &ScopeInput,
    detection_key: &[u8; 32],
    cover_text: &str,
) -> (String, Vec<String>) {
    match prose_token_recv_classified(config, this_pair_scope, detection_key, cover_text)
        .expect("cover open command returns a classified result")
    {
        ProseTokenRecv::Recovered(opened) => {
            let encoded = opened
                .wire
                .strip_prefix("DPC0::")
                .expect("opened private wire keeps DPC0 prefix");
            let plaintext = String::from_utf8(
                STANDARD
                    .decode(encoded)
                    .expect("opened private wire body is base64"),
            )
            .expect("opened private wire body is UTF-8");
            (cover_text.to_owned(), vec![plaintext])
        }
        ProseTokenRecv::Missed(ProseTokenMiss::NoToken) => (cover_text.to_owned(), Vec::new()),
        ProseTokenRecv::Missed(ProseTokenMiss::BlobGone) => {
            panic!("wrong-pair cover decoded as this pair before fetch")
        }
    }
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
            let mut store = state.lock().expect("store state");
            store.fetches += 1;
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
fn task_3960_cover_for_different_pair_is_ignored_before_store_fetch() {
    let store = LoopbackCipherStore::start();
    let config = config_dir(&store.base_url);
    let client = CipherStoreClient::new(&store.base_url).expect("loopback client");
    let detection_key =
        ipc::prose_token::derive_detection_key(&[0x96; 32]).expect("derive detection key");
    let this_pair = scope("task-3960-this-pair", "task-3960-this-dm");
    let different_pair = scope("task-3960-other-pair", "task-3960-other-dm");

    let matching_cover = prose_token_send_with_client(
        &client,
        &this_pair,
        &detection_key,
        send_keys(),
        &dpc0_wire(EXACT_PRIVATE_WORDS),
        TTL_1H,
    )
    .expect("make cover for this pair");
    let different_pair_cover = prose_token_send_with_client(
        &client,
        &different_pair,
        &detection_key,
        send_keys(),
        &dpc0_wire("task 3960 should stay hidden"),
        TTL_1H,
    )
    .expect("make cover for a different pair");

    let (matching_row_text, matching_opened) = open_row(
        config.path(),
        &this_pair,
        &detection_key,
        &matching_cover.cover_text,
    );
    let fetches_before_wrong_cover = store.fetches();
    let (wrong_row_text, wrong_opened) = open_row(
        config.path(),
        &this_pair,
        &detection_key,
        &different_pair_cover.cover_text,
    );
    let wrong_pair_fetches = store.fetches() - fetches_before_wrong_cover;

    println!(
        "TASK3960_MATCHING_OPENED_PRIVATE_MESSAGES={}",
        matching_opened.len()
    );
    println!(
        "TASK3960_MATCHING_EXACT_WORDS={}",
        matching_opened.first().map(String::as_str).unwrap_or("")
    );
    println!(
        "TASK3960_DIFFERENT_PAIR_OPENED_PRIVATE_MESSAGES={}",
        wrong_opened.len()
    );
    println!("TASK3960_DIFFERENT_PAIR_STORE_FETCHES={wrong_pair_fetches}");
    println!(
        "TASK3960_DIFFERENT_PAIR_ROW_ONLY_COVER_TEXT={}",
        wrong_row_text == different_pair_cover.cover_text && wrong_opened.is_empty()
    );

    assert_eq!(matching_opened.len(), 1);
    assert_eq!(matching_opened[0], EXACT_PRIVATE_WORDS);
    assert_eq!(matching_row_text, matching_cover.cover_text);
    assert_eq!(wrong_opened.len(), 0);
    assert_eq!(wrong_pair_fetches, 0);
    assert_eq!(wrong_row_text, different_pair_cover.cover_text);
}
