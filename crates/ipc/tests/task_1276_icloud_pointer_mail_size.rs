use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::cipher_store_client::{CipherStoreClient, TTL_1H};
use ipc::prose_token::{
    prose_token_recover_pointer, prose_token_send_with_client, ProseTokenSendKeys,
};
use ipc::scope::{ScopeInput, ScopeKind};
use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const MESSAGE_KEY: [u8; 32] = [0x12; 32];
const SEND_KEY: [u8; 32] = [0x76; 32];
const CONVERSATION_KEY: [u8; 32] = [0x14; 32];
const ICLOUD_MAIL_SIZE_LIMIT_BYTES: usize = 14 * 1024 * 1024 + 512 * 1024;
const FILE_RECORD_SIZE_BYTES: usize = 16 * 1024 * 1024;

fn send_keys() -> ProseTokenSendKeys<'static> {
    ProseTokenSendKeys {
        message_key: &MESSAGE_KEY,
        send_key: &SEND_KEY,
        conversation_key: &CONVERSATION_KEY,
    }
}

fn icloud_mail_scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-1276-icloud-mail-recipient".to_owned(),
        server_id: Some("icloud-mail".to_owned()),
        channel_id: Some("task-1276-icloud-fixture-thread".to_owned()),
    }
}

#[derive(Default)]
struct StoreState {
    next_id: u64,
    stored_body_bytes: Option<usize>,
}

struct LoopbackCipherStore {
    base_url: String,
    state: Arc<Mutex<StoreState>>,
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
            state,
            server,
        }
    }
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
            assert!(headers.contains_key("x-osl-fetch-token"));
            assert_eq!(
                headers.get("x-osl-ttl-seconds").map(String::as_str),
                Some("3600")
            );
            let mut store = state.lock().expect("store state");
            store.next_id += 1;
            store.stored_body_bytes = Some(body.len());
            let id = format!("{:016x}", store.next_id);
            let expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64
                + TTL_1H as i64;
            json_response(
                201,
                &json!({ "id": id, "expires_at": expires_at }).to_string(),
            )
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

#[test]
fn task_1276_icloud_pointer_file_record_ignores_mail_size_limit() {
    let store = LoopbackCipherStore::start(1);
    let client = CipherStoreClient::new(&store.base_url).expect("loopback client");
    let scope = icloud_mail_scope();
    let detection_key =
        ipc::prose_token::derive_detection_key(&[0x7d; 32]).expect("derive detection key");
    let file_record = json!({
        "provider": "iCloud Mail",
        "storage": "protected_pointer",
        "displayName": "task-1276-icloud-16mib.bin",
        "sizeBytes": FILE_RECORD_SIZE_BYTES,
        "contentDigestSha256": "1276127612761276127612761276127612761276127612761276127612761276",
    });
    let sent = prose_token_send_with_client(
        &client,
        &scope,
        &detection_key,
        send_keys(),
        &dpc0_wire(file_record.to_string().as_bytes()),
        TTL_1H,
    )
    .expect("send protected iCloud pointer through loopback fixture");
    let recovered = prose_token_recover_pointer(&scope, &detection_key, &sent.cover_text)
        .expect("cover decodes")
        .expect("cover carries a protected pointer");
    let cover_draft_bytes = sent.cover_text.len();
    let file_record_size_bytes = file_record["sizeBytes"]
        .as_u64()
        .expect("fixture file record has a sizeBytes field")
        as usize;
    let stored_body_bytes = store
        .state
        .lock()
        .expect("store state")
        .stored_body_bytes
        .expect("fixture stored one protected object");

    println!(
        "TASK1276_ICLOUD_POINTER_MAIL_SIZE limit_bytes={} cover_draft_bytes={} file_record_bytes={} stored_object_bytes={} pointer_blob_id={} recovered_blob_id={}",
        ICLOUD_MAIL_SIZE_LIMIT_BYTES,
        cover_draft_bytes,
        file_record_size_bytes,
        stored_body_bytes,
        sent.blob_id,
        recovered.blob_id,
    );

    assert!(cover_draft_bytes < ICLOUD_MAIL_SIZE_LIMIT_BYTES);
    assert!(file_record_size_bytes > ICLOUD_MAIL_SIZE_LIMIT_BYTES);
    assert_eq!(recovered.blob_id, sent.blob_id);

    store.server.join().expect("loopback store handled request");
}
