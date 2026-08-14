use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    derive_detection_key, prose_token_recv, prose_token_send_with_client_and_writer,
    ProseTokenCoverWriter, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGE_KEY: [u8; 32] = [0x35; 32];
const SEND_KEY: [u8; 32] = [0x20; 32];
const CONVERSATION_KEY: [u8; 32] = [0x18; 32];

fn exact_200_character_private_message() -> String {
    let source = "Meet me beside the old library after sunset. Bring the blue notebook and keep this private between us; I will explain everything when we are safely inside. Please reply only after you arrive alone. ";
    source.chars().cycle().take(200).collect()
}

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-3520-friend".to_owned(),
        server_id: None,
        channel_id: Some("task-3520-private-channel".to_owned()),
    }
}

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
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

struct LoopbackPointerStore {
    base_url: String,
    server: thread::JoinHandle<()>,
}

impl LoopbackPointerStore {
    fn start(expected_requests: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind pointer store");
        let address = listener.local_addr().expect("pointer store address");
        let state = Arc::new(Mutex::new(StoreState::default()));
        let server_state = Arc::clone(&state);
        let server = thread::spawn(move || {
            for _ in 0..expected_requests {
                let (stream, _) = listener.accept().expect("accept pointer-store request");
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
    .expect("write pointer-store override");
    dir
}

fn read_request(stream: &mut TcpStream) -> (String, String, HashMap<String, String>, Vec<u8>) {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).expect("read request line");
    let mut parts = first.split_whitespace();
    let method = parts.next().expect("request method").to_owned();
    let path = parts.next().expect("request path").to_owned();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("read request header");
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("read request body");
    (method, path, headers, body)
}

fn handle_request(mut stream: TcpStream, state: &Arc<Mutex<StoreState>>) {
    let (method, path, headers, body) = read_request(&mut stream);
    let response = if method == "POST" && path == "/v1/blob" {
        let mut state = state.lock().expect("pointer-store state");
        state.next_id += 1;
        let id = format!("{:016x}", state.next_id);
        state.blobs.insert(
            id.clone(),
            StoredBlob {
                fetch_token: headers
                    .get("x-osl-fetch-token")
                    .cloned()
                    .unwrap_or_default(),
                body,
            },
        );
        let ttl = headers
            .get("x-osl-ttl-seconds")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or_default();
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_secs() as i64
            + ttl;
        response(
            201,
            "application/json",
            json!({"id": id, "expires_at": expires_at})
                .to_string()
                .as_bytes(),
        )
    } else if method == "GET" && path.starts_with("/v1/blob/") {
        let id = path.trim_start_matches("/v1/blob/");
        let state = state.lock().expect("pointer-store state");
        match state.blobs.get(id) {
            Some(blob) if headers.get("x-osl-fetch-token") == Some(&blob.fetch_token) => {
                response(200, "application/octet-stream", &blob.body)
            }
            Some(_) => response(403, "text/plain", b"forbidden"),
            None => response(404, "text/plain", b"not found"),
        }
    } else {
        response(404, "text/plain", b"not found")
    };
    stream.write_all(&response).expect("write response");
}

fn response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 {status} result\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn private_message_from_wire(wire: &str) -> String {
    let encoded = wire.strip_prefix("DPC0::").expect("DPC0 wire prefix");
    String::from_utf8(STANDARD.decode(encoded).expect("base64 wire"))
        .expect("private message is UTF-8")
}

#[test]
fn task_3520_covertext_button_drives_existing_layered_wordbank() {
    let store = LoopbackPointerStore::start(4);
    let config = config_dir(&store.base_url);
    let client = CipherStoreClient::new(&store.base_url).expect("loopback client");
    let detection_key = derive_detection_key(&[0x52; 32]).expect("derive detection key");
    let original = exact_200_character_private_message();
    assert_eq!(original.chars().count(), 200);
    let wire = format!("DPC0::{}", STANDARD.encode(original.as_bytes()));

    let baseline = prose_token_send_with_client_and_writer(
        &client,
        &scope(),
        &detection_key,
        send_keys(),
        &wire,
        TTL_1H,
        ProseTokenCoverWriter::Baseline,
    )
    .expect("send without Covertext pressed");
    let covertext = prose_token_send_with_client_and_writer(
        &client,
        &scope(),
        &detection_key,
        send_keys(),
        &wire,
        TTL_1H,
        ProseTokenCoverWriter::Covertext,
    )
    .expect("send with Covertext pressed");

    let baseline_readback = prose_token_recv(
        config.path(),
        &scope(),
        &detection_key,
        &baseline.cover_text,
    )
    .expect("baseline readback request")
    .expect("baseline cover holds a pointer");
    let covertext_readback = prose_token_recv(
        config.path(),
        &scope(),
        &detection_key,
        &covertext.cover_text,
    )
    .expect("Covertext readback request")
    .expect("layered wordbank cover holds a pointer");

    let baseline_words = baseline.cover_text.split_ascii_whitespace().count();
    let covertext_words = covertext.cover_text.split_ascii_whitespace().count();
    let word_count_delta = baseline_words.abs_diff(covertext_words);
    let baseline_original = private_message_from_wire(&baseline_readback.wire);
    let covertext_original = private_message_from_wire(&covertext_readback.wire);

    println!("TASK3520 original_characters={}", original.chars().count());
    println!("TASK3520 original_words={original}");
    println!("TASK3520 baseline_cover={}", baseline.cover_text);
    println!("TASK3520 covertext_cover={}", covertext.cover_text);
    println!("TASK3520 baseline_words={baseline_words}");
    println!("TASK3520 covertext_words={covertext_words}");
    println!("TASK3520 word_count_delta={word_count_delta}");
    println!(
        "TASK3520 covers_differ={}",
        baseline.cover_text != covertext.cover_text
    );
    println!("TASK3520 baseline_exact={}", baseline_original == original);
    println!(
        "TASK3520 covertext_exact={}",
        covertext_original == original
    );

    assert_ne!(baseline.cover_text, covertext.cover_text);
    assert_ne!(baseline_words, covertext_words);
    assert!(word_count_delta > 0);
    assert_eq!(baseline_original, original);
    assert_eq!(covertext_original, original);
    store
        .server
        .join()
        .expect("pointer store served both modes");
}
