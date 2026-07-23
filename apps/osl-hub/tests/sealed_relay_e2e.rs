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
        ("GET", path) if path.starts_with("/v1/control-inbox/") => {
            let recipient = path.trim_start_matches("/v1/control-inbox/");
            let state = state.lock().unwrap();
            let items = state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient)
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
            json_response(200, json!({ "items": items }))
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
        bob_code.safety_number,
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
        alice_code.safety_number,
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
    assert!(matches!(
        receipt.acknowledgments[0].status,
        NativeOverlayAcknowledgmentStatus::Opened
    ));
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
        NativeOverlayAcknowledgmentStatus::Opened
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
