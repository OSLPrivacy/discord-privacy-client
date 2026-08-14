use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::osl_mail::{
    open_osl_mail_sealed_body, provision, send, OslMailSealedEnvelope, OslMailState,
};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const SENT_MESSAGE_ID: &str = "mail_task_4325c_sent_0001";
const NEVER_SENT_MESSAGE_ID: &str = "mail_task_4325c_never_sent";
const TYPED_WORDS: &str = "TASK4325c direct service fetch opens these exact words";
const SUBJECT: &str = "OSL protected message";
const RECIPIENT: &str = "receiver@oslprivacy.com";

#[test]
fn task4325c_fetches_4323_message_directly_from_mail_service() {
    let service = MailService::start();
    let storage = TestStorage::new(&service.base_url);
    let identity = keystore::generate_identity("task-4325c-sender".to_owned());
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity.clone());
    let state = OslMailState::default();

    let provisioned = provision(&core, &state, "task4325csender".to_owned()).unwrap();
    assert_eq!(
        provisioned.address.as_deref(),
        Some("task4325csender@oslprivacy.com")
    );
    let receipt = send(
        &core,
        &state,
        RECIPIENT.to_owned(),
        SUBJECT.to_owned(),
        TYPED_WORDS.to_owned(),
    )
    .unwrap();
    assert_eq!(receipt.client_message_id, SENT_MESSAGE_ID);

    let fetched = direct_service_fetch(&service.base_url, &identity, SENT_MESSAGE_ID);
    let fetch_address = fetched.address.clone();
    let fetched_message = fetched.message.expect("sent message fetch returns a row");
    let fetched_body = STANDARD
        .decode(fetched_message["ciphertext_b64"].as_str().unwrap())
        .unwrap();
    let envelope: OslMailSealedEnvelope =
        serde_json::from_str(fetched_message["envelope_json"].as_str().unwrap()).unwrap();
    let opened =
        open_osl_mail_sealed_body(&identity, RECIPIENT, SUBJECT, &envelope, &fetched_body).unwrap();

    let never_sent = direct_service_fetch(&service.base_url, &identity, NEVER_SENT_MESSAGE_ID);
    let refusal = never_sent.refusal.expect("missing message has refusal");

    println!("TASK4325c fetch_address={fetch_address}");
    println!(
        "TASK4325c sent_message_id={} opened=\"{}\" exact_match={}",
        SENT_MESSAGE_ID,
        opened,
        opened == TYPED_WORDS
    );
    println!(
        "TASK4325c never_sent_message_id={} fetch_returned_message={} refusal=\"{}\" status={}",
        NEVER_SENT_MESSAGE_ID,
        usize::from(never_sent.message.is_some()),
        refusal,
        never_sent.status
    );

    assert_eq!(fetched.status, 200);
    assert_eq!(opened, TYPED_WORDS);
    assert!(never_sent.message.is_none());
    assert_eq!(never_sent.status, 404);
    assert_eq!(refusal, "message not found");

    let state = service.state.lock().unwrap();
    assert_eq!(
        state.fetch_addresses,
        vec![fetch_address.clone(), fetch_address]
    );
    assert_eq!(state.messages.len(), 1);
    drop(storage);
}

struct TestStorage {
    root: tempfile::TempDir,
}

impl TestStorage {
    fn new(base_url: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("keyserver.json"),
            format!(r#"{{"base_url":"{base_url}"}}"#),
        )
        .unwrap();
        keystore::set_base_dir_override(Some(root.path().to_path_buf()));
        keystore::set_active_account_dir(None);
        Self { root }
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        keystore::set_base_dir_override(None);
        keystore::set_active_account_dir(None);
    }
}

struct MailService {
    base_url: String,
    state: Arc<Mutex<MailServiceState>>,
}

#[derive(Default)]
struct MailServiceState {
    messages: HashMap<String, Value>,
    fetch_addresses: Vec<String>,
}

impl MailService {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let base_url = format!("http://{address}");
        let state = Arc::new(Mutex::new(MailServiceState::default()));
        let thread_base_url = base_url.clone();
        let thread_state = Arc::clone(&state);
        std::thread::spawn(move || {
            for _ in 0..6 {
                let mut stream = listener.accept().unwrap().0;
                let request = read_http_request(&mut stream);
                serve_mail_request(&thread_base_url, &thread_state, &mut stream, &request);
            }
        });
        Self { base_url, state }
    }
}

fn serve_mail_request(
    base_url: &str,
    state: &Arc<Mutex<MailServiceState>>,
    stream: &mut TcpStream,
    request: &str,
) {
    if request.starts_with("GET /v1/mail/capabilities ") {
        write_json(
            stream,
            200,
            r#"{"version":1,"addressDomain":"oslprivacy.com","oslToOslE2ee":true}"#,
        );
        return;
    }
    if request.starts_with("POST /v1/mail/address ") {
        write_json(
            stream,
            200,
            r#"{"address":"task4325csender@oslprivacy.com","username":"task4325csender","user_id":"task-4325c-sender","state":"active"}"#,
        );
        return;
    }
    if request.starts_with("POST /v1/mail/send/osl ") {
        let upload: Value = serde_json::from_str(request_body(request)).unwrap();
        assert_eq!(upload["ciphertext_b64"].as_str().unwrap().is_empty(), false);
        assert_eq!(upload["envelope"]["version"], 2);
        let fetched = serde_json::json!({
            "message_id": SENT_MESSAGE_ID,
            "kind": "osl_e2ee",
            "sender_user_id": upload["user_id"].as_str().unwrap(),
            "opaque_thread_token": upload["opaque_thread_token"].as_str().unwrap(),
            "ciphertext_b64": upload["ciphertext_b64"].as_str().unwrap(),
            "envelope_json": upload["envelope"].to_string(),
            "recipient_key_fingerprint": upload["recipient_key_fingerprint"].as_str().unwrap(),
            "received_at": 4325,
            "expires_at": 4325 + 604800,
            "byte_length": STANDARD.decode(upload["ciphertext_b64"].as_str().unwrap()).unwrap().len(),
        });
        state
            .lock()
            .unwrap()
            .messages
            .insert(SENT_MESSAGE_ID.to_owned(), fetched);
        write_json(
            stream,
            200,
            r#"{"message_id":"mail_task_4325c_sent_0001","accepted":true}"#,
        );
        return;
    }
    if request.starts_with("POST /v1/mail/fetch ") {
        let body: Value = serde_json::from_str(request_body(request)).unwrap();
        assert_eq!(body["user_id"].as_str().unwrap(), "task-4325c-sender");
        assert_eq!(body["signature_b64"].as_str().unwrap().is_empty(), false);
        let message_id = body["message_id"].as_str().unwrap();
        let address = format!("{base_url}/v1/mail/fetch");
        let mut guard = state.lock().unwrap();
        guard.fetch_addresses.push(address);
        if let Some(message) = guard.messages.get(message_id) {
            write_json(stream, 200, &message.to_string());
        } else {
            write_json(stream, 404, r#"{"error":"message not found"}"#);
        }
        return;
    }
    write_json(stream, 404, r#"{"error":"not found"}"#);
}

struct DirectFetchResult {
    address: String,
    status: u16,
    message: Option<Value>,
    refusal: Option<String>,
}

fn direct_service_fetch(
    base_url: &str,
    identity: &keystore::Identity,
    message_id: &str,
) -> DirectFetchResult {
    let address = format!("{base_url}/v1/mail/fetch");
    let mut body = signed_mail_body(identity, "FETCH", message_id);
    let response = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .post(&address)
        .json(&body)
        .send()
        .unwrap();
    let status = response.status().as_u16();
    let value: Value = response.json().unwrap();
    body.clear();
    DirectFetchResult {
        address,
        status,
        message: (status == 200).then_some(value.clone()),
        refusal: value["error"].as_str().map(str::to_owned),
    }
}

fn signed_mail_body(
    identity: &keystore::Identity,
    operation: &str,
    message_id: &str,
) -> Map<String, Value> {
    let mut unsigned = Map::new();
    unsigned.insert(
        "message_id".to_owned(),
        Value::String(message_id.to_owned()),
    );
    unsigned.insert("timestamp_ms".to_owned(), Value::from(now_millis()));
    unsigned.insert("request_id".to_owned(), Value::String(request_id()));
    unsigned.insert(
        "user_id".to_owned(),
        Value::String(identity.user_id.clone()),
    );
    let message = signed_message(operation, &unsigned);
    unsigned.insert(
        "signature_b64".to_owned(),
        Value::String(
            STANDARD.encode(crypto::ed25519::sign(&identity.ed25519_secret, &message).as_bytes()),
        ),
    );
    unsigned
}

fn now_millis() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

fn request_id() -> String {
    URL_SAFE_NO_PAD.encode(crypto::random::random_bytes(32))
}

fn signed_message(operation: &str, body: &Map<String, Value>) -> Vec<u8> {
    let mut unsigned = body.clone();
    unsigned.remove("signature_b64");
    format!(
        "OSL-MAIL-{operation}-v1\n{}\n",
        canonical_json(&Value::Object(unsigned))
    )
    .into_bytes()
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => serde_json::to_string(value).unwrap(),
        Value::Number(number) if number.as_i64().is_some() || number.as_u64().is_some() => {
            number.to_string()
        }
        Value::Number(_) => panic!("canonical JSON only accepts safe integers"),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(values) => {
            let mut fields = values.iter().collect::<Vec<_>>();
            fields.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
            format!(
                "{{{}}}",
                fields
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        canonical_json(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = [0u8; 8192];
    let mut bytes = Vec::new();
    loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "connection closed before request completed");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_len = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .or_else(|| {
                    headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_len {
                return String::from_utf8_lossy(&bytes).to_string();
            }
        }
    }
}

fn request_body(request: &str) -> &str {
    request.split("\r\n\r\n").nth(1).unwrap_or_default()
}

fn write_json(stream: &mut TcpStream, status: u16, body: &str) {
    write!(
        stream,
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.as_bytes().len(),
        body
    )
    .unwrap();
}
