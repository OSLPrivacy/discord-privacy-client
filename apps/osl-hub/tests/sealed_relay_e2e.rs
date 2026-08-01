#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, drain_osl_chat_text, prepare_osl_chat_text,
    prepare_peer_prose_text, HubBrokerState, NativeOverlayAcknowledgmentStatus,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "sealed-relay-fixture-password";

#[derive(Clone)]
struct InboxRow {
    id: String,
    sender_id: String,
    recipient_id: String,
    scope_id: String,
    bundle_b64: String,
    created_at: i64,
}

#[derive(Clone)]
struct BlobRow {
    bytes: Vec<u8>,
    fetch_token: String,
}

#[derive(Default)]
struct RelayState {
    next_id: u64,
    inbox: Vec<InboxRow>,
    posted: Vec<InboxRow>,
    blobs: BTreeMap<String, BlobRow>,
    wrapped_keys: BTreeMap<String, WrappedKeyRow>,
    /// Public Ed25519 identity keys this relay has been told about, keyed by
    /// user id. Only these identities get a sender-filter capability-floor
    /// observation, so an unknown recipient cannot obtain one and no drain can
    /// reach a filtered page through a floor the server never issued.
    floor_identities: BTreeMap<String, Vec<u8>>,
}

/// One uploaded wrapped share. A share is readable ONLY by its recipient, and a
/// single-use share is consumed by its first successful fetch -- both properties
/// the replay-rejection and view-once assertions depend on.
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

    fn replay_first_message(
        &self,
        sender_id: &str,
        recipient_id: &str,
        scope_override: Option<&str>,
    ) -> String {
        let mut state = self.state.lock().unwrap();
        let mut row = state
            .posted
            .iter()
            .find(|row| row.sender_id == sender_id && row.recipient_id == recipient_id)
            .cloned()
            .expect("original relay notice was posted");
        state.next_id += 1;
        row.id = format!("{:032x}", state.next_id);
        if let Some(scope_id) = scope_override {
            row.scope_id = scope_id.to_owned();
        }
        let id = row.id.clone();
        state.inbox.push(row);
        id
    }

    fn remove_inbox(&self, id: &str) {
        self.state.lock().unwrap().inbox.retain(|row| row.id != id);
    }

    /// Publish one identity's PUBLIC signing key so this relay can answer the
    /// signed sender-filter capability-floor observation the receive boundary
    /// makes before every drain. Nothing secret crosses this boundary: the
    /// anchor is recomputed server-side from the user id and this public key.
    fn register_floor_identity(&self, identity: &keystore::Identity) {
        self.state.lock().unwrap().floor_identities.insert(
            identity.user_id.clone(),
            identity.ed25519_public.as_bytes().to_vec(),
        );
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
            "osl-sealed-relay-e2e-{}-{}",
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

fn core(identity: keystore::Identity, relay_url: &str) -> HubCoreState {
    let core = HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity);
    *core.osl.keyserver.lock().unwrap() = Some(keystore::KeyServerClient::new(relay_url).unwrap());
    core
}

fn serve_request(stream: &mut TcpStream, state: &Arc<Mutex<RelayState>>) {
    let Some((method, path, headers, body)) = read_request(stream) else {
        return;
    };
    let path_without_query = path.split('?').next().unwrap_or(&path);
    let now = now_secs();
    let response = match (method.as_str(), path_without_query) {
        ("GET", "/v1/healthz") => json_response(
            200,
            json!({
                "ok": true,
                "capabilities": {
                    "control_inbox_sender_disposition": 1,
                    "control_inbox_eviction_signal": 1,
                },
            }),
        ),
        ("POST", "/v1/wrapped-keys") => match serde_json::from_slice::<Value>(&body) {
            Err(_) => json_response(400, json!({ "error": "bad json" })),
            Ok(parsed) => {
                let field = |name: &str| -> String {
                    parsed
                        .get(name)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let content_id = field("content_id");
                if content_id.is_empty() {
                    json_response(400, json!({ "error": "content_id required" }))
                } else {
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
                None => json_response(404, json!({ "error": "not found" })),
                Some(row) if row.recipient_id != requester => {
                    json_response(403, json!({ "error": "recipient mismatch" }))
                }
                Some(row) if row.single_use && row.consumed => {
                    json_response(410, json!({ "error": "already consumed" }))
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
        ("DELETE", route) if route.starts_with("/v1/wrapped-keys") => {
            let content_id = route
                .trim_start_matches("/v1/wrapped-keys")
                .trim_start_matches('/')
                .to_owned();
            let mut state = state.lock().unwrap();
            let removed = state.wrapped_keys.remove(&content_id).is_some();
            json_response(200, json!({ "burned": removed }))
        }
        ("POST", "/v1/blob") => {
            let mut state = state.lock().unwrap();
            state.next_id += 1;
            let id = format!("{:016x}", state.next_id);
            let fetch_token = headers
                .get("x-osl-fetch-token")
                .cloned()
                .unwrap_or_default();
            state.blobs.insert(
                id.clone(),
                BlobRow {
                    bytes: body,
                    fetch_token,
                },
            );
            json_response(200, json!({ "id": id, "expires_at": now + 3600 }))
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let state = state.lock().unwrap();
            match state.blobs.get(id) {
                Some(blob) if headers.get("x-osl-fetch-token") == Some(&blob.fetch_token) => {
                    bytes_response(200, "application/octet-stream", blob.bytes.clone())
                }
                Some(_) => json_response(403, json!({ "error": "fetch_token_mismatch" })),
                None => json_response(404, json!({ "error": "not_found" })),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let mut state = state.lock().unwrap();
            let allowed = state
                .blobs
                .get(id)
                .is_none_or(|blob| headers.get("x-osl-fetch-token") == Some(&blob.fetch_token));
            if allowed {
                state.blobs.remove(id);
                bytes_response(204, "application/octet-stream", Vec::new())
            } else {
                json_response(403, json!({ "error": "fetch_token_mismatch" }))
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
            state.posted.push(row.clone());
            state.inbox.push(row.clone());
            json_response(200, json!({ "id": row.id, "expires_at": now + 3600 }))
        }
        ("GET", route) if route.starts_with("/v1/control-inbox/") => {
            let target = url::Url::parse(&format!("http://relay.invalid{path}"))
                .expect("parse control-inbox request target");
            let recipient = target.path().trim_start_matches("/v1/control-inbox/");
            let sender = target
                .query_pairs()
                .find_map(|(key, value)| (key == "sender").then(|| value.into_owned()));
            let state = state.lock().unwrap();
            let items = state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient)
                .filter(|row| {
                    sender
                        .as_deref()
                        .is_none_or(|requested| row.sender_id == requested)
                })
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
            match sender {
                Some(sender) => json_response(
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
                ),
                None => json_response(200, json!({ "items": items })),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/control-inbox/") => {
            let id = path.trim_start_matches("/v1/control-inbox/");
            state.lock().unwrap().inbox.retain(|row| row.id != id);
            bytes_response(204, "application/json", Vec::new())
        }
        // The shipping receive boundary observes a signed sender-filter
        // capability floor before it will drain a filtered page, exactly as
        // `keyserver-cf/src/endpoints/sender-filter-capability-floor.ts` serves
        // it. This fixture answers only for an identity it was told about and
        // only for a signed, timestamped request, and it recomputes the anchor
        // from public inputs rather than echoing anything the caller sent -- so
        // the floor stays a real authority check rather than a rubber stamp.
        ("GET", route) if route.starts_with("/v1/sender-filter-capability-floor/") => {
            sender_filter_capability_floor_response(&path, state)
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

/// `keystore::sender_filter_rollout::sender_filter_floor_identity_anchor_sha256`,
/// recomputed here from public inputs only, so the fixture proves the anchor the
/// client independently derives instead of echoing one the client supplied.
fn floor_identity_anchor_sha256(user_id: &str, ed25519_public: &[u8]) -> String {
    fn length_prefixed(output: &mut Vec<u8>, value: &[u8]) {
        output.extend_from_slice(&(value.len() as u32).to_be_bytes());
        output.extend_from_slice(value);
    }
    let mut canonical = Vec::new();
    length_prefixed(&mut canonical, b"OSL-SENDER-FILTER-FLOOR-IDENTITY-v1\0");
    length_prefixed(&mut canonical, user_id.as_bytes());
    length_prefixed(&mut canonical, ed25519_public);
    let digest = <sha2::Sha256 as sha2::Digest>::digest(canonical);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sender_filter_capability_floor_response(path: &str, state: &Arc<Mutex<RelayState>>) -> Vec<u8> {
    let target = url::Url::parse(&format!("http://relay.invalid{path}"))
        .expect("parse sender-filter floor request target");
    let recipient = target
        .path()
        .trim_start_matches("/v1/sender-filter-capability-floor/")
        .to_owned();
    let query = |name: &str| {
        target
            .query_pairs()
            .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
            .unwrap_or_default()
    };
    let timestamp_ms = query("ts").parse::<i64>().unwrap_or_default();
    let request_id = query("request_id");
    let anchor = state
        .lock()
        .unwrap()
        .floor_identities
        .get(&recipient)
        .map(|public| floor_identity_anchor_sha256(&recipient, public));
    match anchor {
        Some(anchor) if timestamp_ms > 0 && !query("sig").is_empty() => json_response(
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
        204 => "No Content",
        403 => "Forbidden",
        404 => "Not Found",
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
fn sealed_relay_post_run_reset_cleanup() {
    let relay = RelayServer::start();
    let relay_address = relay.address.clone();
    let storage = TestStorage::new();
    let root = storage.root.clone();
    let account_dir = storage.account("cleanup-account", &relay.base_url());
    TestStorage::activate(&account_dir);

    assert_eq!(keystore::active_account_dir(), Some(account_dir.clone()));
    assert!(ipc::main_password::get_file_storage_key().is_some());
    assert!(root.exists());
    assert!(account_dir.exists());

    drop(storage);
    drop(relay);

    assert!(keystore::active_account_dir().is_none());
    assert!(ipc::main_password::get_file_storage_key().is_none());
    assert!(
        keystore::osl_base_dir()
            .map(|base| base != root)
            .unwrap_or(true),
        "base-dir override must be cleared after the sealed relay fixture"
    );
    assert!(
        !root.exists(),
        "isolated sealed relay fixture root must be removed after the run"
    );
    assert!(
        TcpStream::connect(&relay_address).is_err(),
        "sealed relay fixture listener must be stopped after the run"
    );
}

#[test]
fn two_verified_identities_complete_sealed_relay_open_ack_and_replay_rejection() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice", &relay_url);
    let bob_dir = storage.account("bob", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-sealed-e2e".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-sealed-e2e".to_owned());
    let alice_id = alice_identity.user_id.clone();
    let bob_id = bob_identity.user_id.clone();
    relay.register_floor_identity(&alice_identity);
    relay.register_floor_identity(&bob_identity);
    let alice = core(alice_identity, &relay_url);
    let bob = core(bob_identity, &relay_url);
    let alice_security = HubSecurityState::default();
    let bob_security = HubSecurityState::default();
    let alice_broker = HubBrokerState::default();
    let bob_broker = HubBrokerState::default();

    let alice_code = export_friend_code(&alice).unwrap();
    let bob_code = export_friend_code(&bob).unwrap();

    TestStorage::activate(&alice_dir);
    let bob_friend = add_friend_code(
        &alice,
        &alice_security,
        bob_code.friend_code,
        Some("Bob fixture".to_owned()),
    )
    .unwrap();
    verify_friend_safety_number(
        &alice,
        &alice_security,
        bob_friend.person_id.clone(),
        // Pair-derived, so this is the same value Bob's device displays.
        bob_friend.safety_number.clone(),
    )
    .unwrap();
    let alice_binding = manual_peer_binding(&alice, bob_friend.person_id.clone()).unwrap();
    let alice_context =
        activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding.clone()).unwrap();
    set_manual_peer_scope_permission(
        &alice,
        &alice_security,
        "osl-chat",
        "osl-main",
        alice_context.person_id.clone(),
        alice_context.scope.clone(),
        true,
    )
    .unwrap();
    set_scope_security(&alice_security, alice_context.scope.clone(), 3600, true).unwrap();

    TestStorage::activate(&bob_dir);
    let alice_friend = add_friend_code(
        &bob,
        &bob_security,
        alice_code.friend_code,
        Some("Alice fixture".to_owned()),
    )
    .unwrap();
    verify_friend_safety_number(
        &bob,
        &bob_security,
        alice_friend.person_id.clone(),
        alice_friend.safety_number.clone(),
    )
    .unwrap();
    let bob_binding = manual_peer_binding(&bob, alice_friend.person_id.clone()).unwrap();
    let bob_context = activate_owned_osl_chat_context(&bob_broker, &bob_id, bob_binding).unwrap();
    set_manual_peer_scope_permission(
        &bob,
        &bob_security,
        "osl-chat",
        "osl-main",
        bob_context.person_id.clone(),
        bob_context.scope.clone(),
        true,
    )
    .unwrap();
    set_scope_security(&bob_security, bob_context.scope.clone(), 3600, true).unwrap();

    let plaintext = "sealed relay fixture: alpha → beta".to_owned();
    TestStorage::activate(&alice_dir);
    let prepared = prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        plaintext.clone(),
        true,
    )
    .unwrap();
    assert!(prepared.person_to_person_e2ee);
    assert!(prepared.view_once);
    assert!(prepared.delivered_to_osl_inbox);
    assert_eq!(relay.pending_for(&bob_id), 1);

    // A valid encrypted notice copied under a different relay scope is not
    // consumed by this conversation. The original correctly-bound row still
    // opens, proving the routing check does not broaden to all recipient rows.
    let wrong_scope = relay.replay_first_message(&alice_id, &bob_id, Some("wrong-scope"));
    TestStorage::activate(&bob_dir);
    let opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    assert_eq!(opened.messages.len(), 1);
    assert_eq!(opened.messages[0].plaintext, plaintext);
    assert!(opened.messages[0].context_verified);
    assert!(opened.messages[0].person_to_person_e2ee);
    assert!(opened.messages[0].view_once_consumed);
    assert_eq!(relay.pending_for(&bob_id), 1);
    relay.remove_inbox(&wrong_scope);

    TestStorage::activate(&alice_dir);
    let receipt = drain_osl_chat_text(&alice, &alice_security, &alice_broker, true).unwrap();
    assert_eq!(receipt.acknowledgments.len(), 1);
    assert_eq!(receipt.acknowledgments[0].message_id, prepared.message_id);
    // Received, NOT Opened. An Opened receipt discloses that the recipient actually
    // read the message, so production admits it only under durable signed mutual
    // consent -- a feature that is not built yet.
    // broker::native_overlay_acknowledgment_is_admissible_without_mutual_consent
    // permits Received alone, both call sites refuse Opened, and
    // "an authenticated Opened frame remains inadmissible without mutual consent"
    // is asserted in broker.rs. This test previously expected Opened, contradicting
    // all of that; asserting the shipped contract keeps it a real gate.
    assert!(matches!(
        receipt.acknowledgments[0].status,
        NativeOverlayAcknowledgmentStatus::Received
    ));
    assert!(
        !matches!(
            receipt.acknowledgments[0].status,
            NativeOverlayAcknowledgmentStatus::Opened
        ),
        "an Opened receipt must not be emitted without durable mutual consent"
    );
    let receipt_bytes = fs::read(alice_dir.join("hub_native_overlay_receipts.json")).unwrap();
    assert!(ipc::main_password::has_enc_magic(&receipt_bytes));
    assert!(!receipt_bytes
        .windows(plaintext.len())
        .any(|window| window == plaintext.as_bytes()));

    // Replaying the exact authenticated notice can retry its ACK, but durable
    // per-scope consumption prevents a second plaintext or view-once open.
    relay.replay_first_message(&alice_id, &bob_id, None);
    TestStorage::activate(&bob_dir);
    let replay = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    assert!(replay.messages.is_empty());
    assert!(replay.pending_view_once.is_empty());
    assert_eq!(relay.pending_for(&bob_id), 0);

    TestStorage::activate(&alice_dir);
    let replay_receipt = drain_osl_chat_text(&alice, &alice_security, &alice_broker, true).unwrap();
    assert_eq!(replay_receipt.acknowledgments.len(), 1);
    assert_eq!(
        replay_receipt.acknowledgments[0].message_id,
        prepared.message_id
    );
    assert!(matches!(
        replay_receipt.acknowledgments[0].status,
        NativeOverlayAcknowledgmentStatus::Received
    ));

    // A later broker activation invalidates the old lease before encryption,
    // preserving the account/context epoch boundary without a relay request.
    let old_token = alice_context.lease.context_token;
    activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding).unwrap();
    assert!(prepare_peer_prose_text(
        &alice,
        &alice_security,
        &alice_broker,
        &old_token,
        "stale epoch must fail".to_owned(),
        false,
    )
    .is_err());
}
