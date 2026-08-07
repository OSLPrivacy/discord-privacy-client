#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_native_manual_peer_context, drain_native_discord_overlay_text,
    prepare_native_discord_overlay_text_with_route_clients,
    reveal_native_discord_overlay_view_once, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use osl_privacy_hub::service_host::ServiceHostState;
use serde_json::{json, Value};
use sha2::Digest;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-0471-view-once-fixture-password";
const MAX_DRAIN_ROWS: usize = 64;

#[derive(Clone)]
struct InboxRow {
    id: String,
    sender_id: String,
    recipient_id: String,
    scope_id: String,
    bundle_b64: String,
    created_at: i64,
}

struct BlobRow {
    bytes: Vec<u8>,
    fetch_token: String,
}

struct WrappedKeyRow {
    sender_id: String,
    recipient_id: String,
    content_type: String,
    system_message_kind: Option<String>,
    session_version: u64,
    share_index: u64,
    wrapped_share_blob: String,
    blob_version: u64,
    single_use: bool,
    display_duration_seconds: Option<u64>,
    expires_at: String,
    created_at: String,
    consumed: bool,
}

#[derive(Default)]
struct RelayState {
    next_id: u64,
    inbox: Vec<InboxRow>,
    blobs: BTreeMap<String, BlobRow>,
    wrapped_keys: BTreeMap<String, WrappedKeyRow>,
}

struct RelayServer {
    address: String,
    state: Arc<Mutex<RelayState>>,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RelayServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind relay fixture");
        listener
            .set_nonblocking(true)
            .expect("make relay fixture nonblocking");
        let address = listener.local_addr().unwrap().to_string();
        let state = Arc::new(Mutex::new(RelayState::default()));
        let stopping = Arc::new(AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stopping = Arc::clone(&stopping);
        let thread = thread::spawn(move || {
            while !thread_stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => serve_request(&mut stream, &thread_state),
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

    fn pending_for(&self, recipient_id: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .inbox
            .iter()
            .filter(|row| row.recipient_id == recipient_id)
            .count()
    }
}

impl Drop for RelayServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        let _ = TcpStream::connect(&self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-0471-view-once-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated OSL test root");
        keystore::set_base_dir_override(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated OSL main password");
        Self { root }
    }

    fn account(&self, name: &str, relay_url: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir(&dir).expect("create isolated OSL account dir");
        fs::write(
            dir.join("keyserver.json"),
            serde_json::to_vec(&json!({ "cipher_store_url": relay_url })).unwrap(),
        )
        .expect("write isolated cipher-store configuration");
        dir
    }

    fn activate(dir: &Path) {
        keystore::set_active_account_dir(Some(dir.to_owned()));
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Peer {
    dir: PathBuf,
    identity_id: String,
    core: HubCoreState,
    security: HubSecurityState,
    broker: HubBrokerState,
    host: ServiceHostState,
    account_id: String,
    friend_code: String,
}

impl Peer {
    fn new(storage: &TestStorage, name: &str, relay_url: &str, account_suffix: &str) -> Self {
        let dir = storage.account(name, relay_url);
        let identity = keystore::generate_identity(format!("osl-task-0471-{name}"));
        let identity_id = identity.user_id.clone();
        floor_identities()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                identity_id.clone(),
                identity.ed25519_public.as_bytes().to_vec(),
            );
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(identity);
        *core.osl.keyserver.lock().unwrap() =
            Some(keystore::KeyServerClient::new(relay_url).unwrap());
        TestStorage::activate(&dir);
        let exported = export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            identity_id,
            core,
            security: HubSecurityState::default(),
            broker: HubBrokerState::default(),
            host: ServiceHostState::default(),
            account_id: format!("native-discord-{account_suffix}"),
            friend_code: exported.friend_code,
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    fn open_native_context_to(&self, other_code: &str) {
        self.activate();
        let friend = add_friend_code(
            &self.core,
            &self.security,
            other_code.to_owned(),
            Some("task 0471 fixture peer".to_owned()),
        )
        .expect("add friend code");
        verify_friend_safety_number(
            &self.core,
            &self.security,
            friend.person_id.clone(),
            friend.safety_number.clone(),
        )
        .expect("verify safety number");
        let active = self
            .host
            .begin_open("owner", "discord", &self.account_id, "discord.com")
            .expect("begin native Discord host generation");
        let binding =
            manual_peer_binding(&self.core, friend.person_id.clone()).expect("manual peer binding");
        let activated = activate_owned_native_manual_peer_context(
            &self.broker,
            &self.identity_id,
            &active,
            binding,
        )
        .expect("activate native Discord manual peer context");
        set_manual_peer_scope_permission(
            &self.core,
            &self.security,
            "discord",
            &self.account_id,
            activated.person_id,
            activated.scope.clone(),
            true,
        )
        .expect("approve manual peer scope");
        set_scope_security(&self.security, activated.scope, 3600, true)
            .expect("enable decrypted display");
    }
}

fn serve_request(stream: &mut TcpStream, state: &Arc<Mutex<RelayState>>) {
    let Some((method, path, headers, body)) = read_request(stream) else {
        return;
    };
    let route = path.split('?').next().unwrap_or(&path);
    let now = now_secs();
    let response = match (method.as_str(), route) {
        ("GET", "/v1/healthz") => json_response(
            200,
            json!({
                "ok": true,
                "capabilities": {
                    "control_inbox_sender_disposition": 1,
                },
            }),
        ),
        ("GET", route) if route.starts_with("/v1/sender-filter-capability-floor/") => {
            let target = url::Url::parse(&format!("http://relay.invalid{path}"))
                .expect("parse sender-filter floor request target");
            let recipient = target
                .path()
                .trim_start_matches("/v1/sender-filter-capability-floor/")
                .to_owned();
            let timestamp_ms = target
                .query_pairs()
                .find_map(|(key, value)| (key == "ts").then(|| value.into_owned()))
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default();
            let request_id = target
                .query_pairs()
                .find_map(|(key, value)| (key == "request_id").then(|| value.into_owned()))
                .unwrap_or_default();
            let sig_present = target
                .query_pairs()
                .any(|(key, value)| key == "sig" && !value.is_empty());
            let anchor = floor_identities()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&recipient)
                .map(|public| floor_identity_anchor_sha256(&recipient, public));
            match anchor {
                Some(anchor) if timestamp_ms > 0 && sig_present => json_response(
                    200,
                    json!({
                        "format": "osl.keyserver.sender-filter-capability-floor.v3",
                        "recipient_user_id": recipient,
                        "identity_anchor_sha256": anchor,
                        "capability_version": 1,
                        "monotonic_version": 1,
                        "first_observed_at_ms": timestamp_ms,
                        "request_timestamp_ms": timestamp_ms,
                        "request_id": request_id,
                    }),
                ),
                _ => json_response(404, json!({ "error": "not_found" })),
            }
        }
        ("POST", "/v1/blob") => match canonical_hex_header(headers.get("x-osl-fetch-token"), 32) {
            Some(fetch_token) => {
                let mut state = state.lock().unwrap();
                state.next_id += 1;
                let blob_id = format!("{:016x}", state.next_id);
                state.blobs.insert(
                    blob_id.clone(),
                    BlobRow {
                        bytes: body,
                        fetch_token,
                    },
                );
                json_response(201, json!({ "id": blob_id, "expires_at": now + 3600 }))
            }
            None => json_response(400, json!({ "error": "fetch_token_required" })),
        },
        ("GET", route) if route.starts_with("/v1/blob/") => {
            let id = route.trim_start_matches("/v1/blob/");
            let state = state.lock().unwrap();
            let presented = headers.get("x-osl-fetch-token");
            match state.blobs.get(id) {
                Some(blob) if presented == Some(&blob.fetch_token) => {
                    bytes_response(200, "application/octet-stream", blob.bytes.clone())
                }
                Some(_) => json_response(403, json!({ "error": "fetch_token_mismatch" })),
                None => json_response(404, json!({ "error": "not_found" })),
            }
        }
        ("POST", "/v1/wrapped-keys") => match serde_json::from_slice::<Value>(&body) {
            Err(_) => json_response(400, json!({ "error": "bad_json" })),
            Ok(parsed) => {
                let field = |name: &str| -> String {
                    parsed
                        .get(name)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let content_id = field("content_id");
                let mut state = state.lock().unwrap();
                state.wrapped_keys.insert(
                    content_id.clone(),
                    WrappedKeyRow {
                        sender_id: field("sender_id"),
                        recipient_id: field("recipient_id"),
                        content_type: field("content_type"),
                        system_message_kind: parsed
                            .get("system_message_kind")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        session_version: parsed
                            .get("session_version")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        share_index: parsed
                            .get("share_index")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        wrapped_share_blob: field("wrapped_share_blob"),
                        blob_version: parsed
                            .get("blob_version")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        single_use: parsed
                            .get("single_use")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        display_duration_seconds: parsed
                            .get("display_duration_seconds")
                            .and_then(Value::as_u64),
                        expires_at: field("expires_at"),
                        created_at: format!("{now}"),
                        consumed: false,
                    },
                );
                json_response(200, json!({ "content_id": content_id }))
            }
        },
        ("GET", route) if route.starts_with("/v1/wrapped-keys/") => {
            let content_id = route.trim_start_matches("/v1/wrapped-keys/").to_owned();
            let requester = path
                .split_once('?')
                .map(|(_, query)| query)
                .unwrap_or_default()
                .split('&')
                .find_map(|pair| pair.strip_prefix("recipient_id="))
                .unwrap_or_default()
                .replace("%3A", ":");
            let mut state = state.lock().unwrap();
            match state.wrapped_keys.get_mut(&content_id) {
                None => json_response(404, json!({ "error": "not_found" })),
                Some(row) if row.recipient_id != requester => {
                    json_response(403, json!({ "error": "recipient_mismatch" }))
                }
                Some(row) if row.single_use && row.consumed => {
                    json_response(410, json!({ "error": "already_consumed" }))
                }
                Some(row) => {
                    if row.single_use {
                        row.consumed = true;
                    }
                    json_response(
                        200,
                        json!({
                            "content_id": content_id,
                            "content_type": row.content_type,
                            "system_message_kind": row.system_message_kind,
                            "sender_id": row.sender_id,
                            "recipient_id": row.recipient_id,
                            "session_version": row.session_version,
                            "share_index": row.share_index,
                            "wrapped_share_blob": row.wrapped_share_blob,
                            "blob_version": row.blob_version,
                            "single_use": row.single_use,
                            "display_duration_seconds": row.display_duration_seconds,
                            "expires_at": row.expires_at,
                            "created_at": row.created_at,
                        }),
                    )
                }
            }
        }
        ("POST", "/v1/control-inbox") => {
            let value: Value = serde_json::from_slice(&body).expect("valid control-inbox post");
            let mut state = state.lock().unwrap();
            state.next_id += 1;
            let row = InboxRow {
                id: format!("{:032x}", state.next_id),
                sender_id: value["sender_id"].as_str().unwrap().to_owned(),
                recipient_id: value["recipient_id"].as_str().unwrap().to_owned(),
                scope_id: value["scope_id"].as_str().unwrap().to_owned(),
                bundle_b64: value["bundle_b64"].as_str().unwrap().to_owned(),
                created_at: now,
            };
            state.inbox.push(row.clone());
            json_response(200, json!({ "id": row.id, "expires_at": now + 3600 }))
        }
        ("GET", route) if route.starts_with("/v1/control-inbox/") => {
            let target = url::Url::parse(&format!("http://relay.invalid{path}"))
                .expect("parse control-inbox request target");
            let recipient = target
                .path()
                .trim_start_matches("/v1/control-inbox/")
                .to_owned();
            let sender = target
                .query_pairs()
                .find_map(|(key, value)| (key == "sender").then(|| value.into_owned()));
            let state = state.lock().unwrap();
            let rows = state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient)
                .filter(|row| {
                    sender
                        .as_deref()
                        .is_none_or(|requested| row.sender_id == requested)
                })
                .take(MAX_DRAIN_ROWS)
                .cloned()
                .collect::<Vec<_>>();
            let items = rows
                .iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "sender_id": row.sender_id,
                        "scope_id": row.scope_id,
                        "bundle_b64": row.bundle_b64,
                        "created_at": row.created_at,
                    })
                })
                .collect::<Vec<_>>();
            json_response(
                200,
                json!({
                    "items": items,
                    "filtered_sender_id": sender,
                    "filtered_sender_delivery": {
                        "live": items.len(),
                        "retryable": 0,
                        "quarantined": 0,
                        "retired": 0,
                    },
                }),
            )
        }
        ("DELETE", path) if path.starts_with("/v1/control-inbox/") => {
            let id = path.trim_start_matches("/v1/control-inbox/");
            state.lock().unwrap().inbox.retain(|row| row.id != id);
            bytes_response(204, "application/json", Vec::new())
        }
        _ => json_response(404, json!({ "error": "not_found" })),
    };
    let _ = stream.write_all(&response);
}

fn read_request(
    stream: &mut TcpStream,
) -> Option<(String, String, BTreeMap<String, String>, Vec<u8>)> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        if request.len() > 128 * 1024 {
            return None;
        }
    };
    let header = String::from_utf8(request[..header_end].to_vec()).ok()?;
    let mut lines = header.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let mut headers = BTreeMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while request.len() < header_end.saturating_add(content_length) {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    Some((
        method,
        path,
        headers,
        request[header_end..header_end + content_length].to_vec(),
    ))
}

fn canonical_hex_header(value: Option<&String>, length: usize) -> Option<String> {
    let value = value?;
    (value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| value.clone())
}

fn floor_identities() -> &'static Mutex<BTreeMap<String, Vec<u8>>> {
    static IDENTITIES: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
    IDENTITIES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn floor_identity_anchor_sha256(user_id: &str, ed25519_public: &[u8]) -> String {
    fn length_prefixed(output: &mut Vec<u8>, value: &[u8]) {
        output.extend_from_slice(&(value.len() as u32).to_be_bytes());
        output.extend_from_slice(value);
    }
    let mut canonical = Vec::new();
    length_prefixed(&mut canonical, b"OSL-SENDER-FILTER-FLOOR-IDENTITY-v1\0");
    length_prefixed(&mut canonical, user_id.as_bytes());
    length_prefixed(&mut canonical, ed25519_public);
    let digest = <sha2::Sha256 as Digest>::digest(canonical);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn json_response(status: u16, body: Value) -> Vec<u8> {
    bytes_response(
        status,
        "application/json",
        serde_json::to_vec(&body).unwrap(),
    )
}

fn bytes_response(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        410 => "Gone",
        _ => "Error",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn task_0471_server_open_refuses_second_view_once_open() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice = Peer::new(&storage, "alice", &relay_url, "0471a");
    let bob = Peer::new(&storage, "bob", &relay_url, "0471b");
    alice.open_native_context_to(&bob.friend_code);
    bob.open_native_context_to(&alice.friend_code);

    const FIXTURE: &str = "TASK0471 view-once server fixture";
    alice.activate();
    let store_client =
        ipc::cipher_store_client::CipherStoreClient::new(&relay_url).expect("cipher-store client");
    let prepared = prepare_native_discord_overlay_text_with_route_clients(
        &alice.core,
        &alice.security,
        &alice.broker,
        &osl_privacy_hub::ai_carrier::AiCarrierState::default(),
        FIXTURE.to_owned(),
        true,
        &store_client,
        alice.core.osl.keyserver.lock().unwrap().as_ref(),
    )
    .expect("prepare and deliver one view-once fixture");
    assert!(prepared.prepared.view_once);
    assert_eq!(relay.pending_for(&bob.identity_id), 1);

    bob.activate();
    let listed = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("list the view-once fixture before opening");
    assert_eq!(listed.messages.len(), 0);
    assert_eq!(listed.pending_view_once.len(), 1);
    let message_id = listed.pending_view_once[0].message_id.clone();
    assert_eq!(message_id, prepared.prepared.message_id);

    let first =
        reveal_native_discord_overlay_view_once(&bob.core, &bob.security, &bob.broker, &message_id)
            .expect("first open returns content");
    assert!(first.plaintext == FIXTURE);
    assert!(first.view_once_consumed);
    println!(
        "TASK0471_FIRST_OPEN content_returned={} fixture_match={} consumed={} message_id={}",
        !first.plaintext.is_empty(),
        first.plaintext == FIXTURE,
        first.view_once_consumed,
        first.message_id
    );

    let second =
        reveal_native_discord_overlay_view_once(&bob.core, &bob.security, &bob.broker, &message_id);
    let second_error = match second {
        Ok(_) => panic!("second open must be refused"),
        Err(error) => error,
    };
    assert!(second_error == "This view-once message is unavailable or expired");
    println!("TASK0471_SECOND_OPEN refused=true error={}", second_error);

    drop(alice);
    drop(bob);
    drop(storage);
}
