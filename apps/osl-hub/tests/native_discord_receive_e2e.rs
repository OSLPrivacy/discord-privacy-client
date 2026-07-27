//! Headless verification of the **receive half** of the native Discord P2P
//! path: `drain_native_discord_overlay_text` -> `drain_peer_inbox_text`.
//!
//! This exercises the real broker against a loopback key server / cipher-store
//! fixture, with two (and for the misrouting case three) locally generated
//! identities. Nothing here touches Discord, the window host, the composer, or
//! keyboard focus -- the receive path depends on none of them, which is why it
//! can be proven while the send path's focus defect is still open.
//!
//! Discipline enforced by every assertion below: **no plaintext, draft, or
//! conversation content is ever printed, logged, or persisted by this test**,
//! including on failure. Content equality is asserted with `assert!(a == b,
//! "<static message>")` rather than `assert_eq!`, because `assert_eq!` prints
//! both operands when it fails.

#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_native_manual_peer_context, begin_native_overlay_attachment,
    deliver_native_overlay_attachment, drain_native_discord_overlay_text,
    list_native_overlay_attachments, prepare_native_discord_overlay_text,
    take_native_overlay_attachment, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use osl_privacy_hub::service_host::ServiceHostState;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "native-discord-receive-fixture-password";

/// `MAX_DRAIN_ROWS` in `keyserver-cf/src/endpoints/control-inbox.ts`. One
/// `GET /v1/control-inbox/<self>` returns at most this many rows, oldest
/// first, and the endpoint has no continuation cursor.
const MAX_DRAIN_ROWS: usize = 64;

/// `keystore::set_base_dir_override`, `keystore::set_active_account_dir` and
/// the main-password file key are process-wide. Every test in this binary
/// drives all three, so they must not overlap.
fn fixture_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Loopback key server / cipher-store fixture.
// ---------------------------------------------------------------------------

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControlInboxGetRecord {
    recipient_id: String,
    sender_id: Option<String>,
}

#[derive(Clone)]
enum ControlInboxGetReply {
    Honest,
    MissingEcho,
    MismatchedEcho(String),
    CrossSender(String),
    UnfilteredWithoutEcho,
}

#[derive(Default)]
struct RelayState {
    next_id: u64,
    inbox: Vec<InboxRow>,
    posted: Vec<InboxRow>,
    blobs: BTreeMap<String, BlobRow>,
    control_inbox_gets: Vec<ControlInboxGetRecord>,
    control_inbox_get_replies: VecDeque<ControlInboxGetReply>,
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

    fn pending_ids_for(&self, recipient_id: &str) -> BTreeSet<String> {
        self.state
            .lock()
            .unwrap()
            .inbox
            .iter()
            .filter(|row| row.recipient_id == recipient_id)
            .map(|row| row.id.clone())
            .collect()
    }

    fn queue_control_inbox_get_reply(&self, reply: ControlInboxGetReply) {
        self.state
            .lock()
            .unwrap()
            .control_inbox_get_replies
            .push_back(reply);
    }

    fn control_inbox_gets(&self) -> Vec<ControlInboxGetRecord> {
        self.state.lock().unwrap().control_inbox_gets.clone()
    }

    /// The first row the sender actually posted for this recipient, exactly as
    /// the key server stored it.
    fn posted_row(&self, sender_id: &str, recipient_id: &str) -> InboxRow {
        self.state
            .lock()
            .unwrap()
            .posted
            .iter()
            .find(|row| row.sender_id == sender_id && row.recipient_id == recipient_id)
            .cloned()
            .expect("the sender posted a relay notice for this recipient")
    }

    fn posted_rows(&self, sender_id: &str, recipient_id: &str) -> Vec<InboxRow> {
        self.state
            .lock()
            .unwrap()
            .posted
            .iter()
            .filter(|row| row.sender_id == sender_id && row.recipient_id == recipient_id)
            .cloned()
            .collect()
    }

    /// Put an arbitrary row into a recipient's inbox. Used to inject rows the
    /// honest sender would never produce (foreign, corrupted, misrouted).
    fn inject(
        &self,
        sender_id: &str,
        recipient_id: &str,
        scope_id: &str,
        bundle_b64: &str,
    ) -> String {
        let mut state = self.state.lock().unwrap();
        state.next_id += 1;
        let id = format!("{:032x}", state.next_id);
        let created_at = now_secs();
        state.inbox.push(InboxRow {
            id: id.clone(),
            sender_id: sender_id.to_owned(),
            recipient_id: recipient_id.to_owned(),
            scope_id: scope_id.to_owned(),
            bundle_b64: bundle_b64.to_owned(),
            created_at,
        });
        id
    }

    fn remove_inbox(&self, id: &str) {
        self.state.lock().unwrap().inbox.retain(|row| row.id != id);
    }

    /// Lift every waiting row for one recipient out of the inbox, so a test can
    /// put them back in a different order or in several batches. Models arrival
    /// order, which the receiver does not control.
    fn take_inbox_for(&self, recipient_id: &str) -> Vec<InboxRow> {
        let mut state = self.state.lock().unwrap();
        let taken = state
            .inbox
            .iter()
            .filter(|row| row.recipient_id == recipient_id)
            .cloned()
            .collect::<Vec<_>>();
        state.inbox.retain(|row| row.recipient_id != recipient_id);
        taken
    }

    /// Whether one specific injected row is still waiting. Row-level truth,
    /// so a consumption assertion names the row it means instead of leaning
    /// on a total that several different behaviours could produce.
    fn still_pending(&self, id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .inbox
            .iter()
            .any(|row| row.id == id)
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
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-native-discord-receive-{label}-{}-{}",
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
            let mut state = state.lock().unwrap();
            state.control_inbox_gets.push(ControlInboxGetRecord {
                recipient_id: recipient.clone(),
                sender_id: sender.clone(),
            });
            let reply = state
                .control_inbox_get_replies
                .pop_front()
                .unwrap_or(ControlInboxGetReply::Honest);
            let use_filter = !matches!(&reply, ControlInboxGetReply::UnfilteredWithoutEcho);
            let mut rows = state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient)
                .filter(|row| {
                    !use_filter
                        || sender
                            .as_deref()
                            .is_none_or(|requested| row.sender_id == requested)
                })
                // The real worker applies the sender predicate before
                // `ORDER BY created_at ASC LIMIT MAX_DRAIN_ROWS`.
                .take(MAX_DRAIN_ROWS)
                .cloned()
                .collect::<Vec<_>>();
            if let ControlInboxGetReply::CrossSender(ref other_sender) = reply {
                if let Some(row) = state
                    .inbox
                    .iter()
                    .find(|row| row.recipient_id == recipient && row.sender_id == *other_sender)
                    .cloned()
                {
                    rows.push(row);
                }
            }
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
            match reply {
                ControlInboxGetReply::Honest => match sender {
                    Some(sender) => {
                        json_response(200, json!({ "items": items, "filtered_sender_id": sender }))
                    }
                    None => json_response(200, json!({ "items": items })),
                },
                ControlInboxGetReply::MissingEcho | ControlInboxGetReply::UnfilteredWithoutEcho => {
                    json_response(200, json!({ "items": items }))
                }
                ControlInboxGetReply::MismatchedEcho(echoed) => {
                    json_response(200, json!({ "items": items, "filtered_sender_id": echoed }))
                }
                ControlInboxGetReply::CrossSender(_) => {
                    json_response(200, json!({ "items": items, "filtered_sender_id": sender }))
                }
            }
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

// ---------------------------------------------------------------------------
// Native Discord peer setup.
// ---------------------------------------------------------------------------

/// One side of a native Discord protected conversation: its own identity, its
/// own account directory, and the state triple the broker commands take.
struct Peer {
    dir: PathBuf,
    identity_id: String,
    core: HubCoreState,
    security: HubSecurityState,
    broker: HubBrokerState,
    host: ServiceHostState,
    account_id: String,
    friend_code: String,
    safety_number: String,
    /// The scope of the most recently activated context, so a test can flip a
    /// per-conversation security setting without re-deriving it.
    scope: Mutex<Option<ipc::scope::ScopeInput>>,
}

impl Peer {
    fn new(storage: &TestStorage, name: &str, relay_url: &str, account_suffix: &str) -> Self {
        let dir = storage.account(name, relay_url);
        let identity = keystore::generate_identity(format!("osl-{name}-native-receive"));
        let identity_id = identity.user_id.clone();
        let core = core(identity, relay_url);
        TestStorage::activate(&dir);
        let exported = export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            identity_id,
            core,
            security: HubSecurityState::default(),
            broker: HubBrokerState::default(),
            host: ServiceHostState::default(),
            // `activate_owned_native_manual_peer_context` accepts only a
            // synthetic native Discord account id; this is the exact shape
            // `NativeWindowHostState::current_discord_service_host` produces.
            account_id: format!("native-discord-{account_suffix}"),
            friend_code: exported.friend_code,
            safety_number: exported.safety_number,
            scope: Mutex::new(None),
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    /// Add + verify `other` as a friend and open a native Discord protected
    /// context against them, with decrypted display enabled.
    fn open_native_context_to(&self, other_code: &str, other_safety: &str) -> String {
        self.activate();
        let friend = add_friend_code(
            &self.core,
            &self.security,
            other_code.to_owned(),
            Some("fixture peer".to_owned()),
        )
        .expect("add friend code");
        verify_friend_safety_number(
            &self.core,
            &self.security,
            friend.person_id.clone(),
            other_safety.to_owned(),
        )
        .expect("verify safety number");
        self.reopen_native_context(&friend.person_id)
    }

    /// Re-activate an already verified friend. Every activation takes a fresh
    /// native host generation, exactly like a real Discord window re-attach.
    fn reopen_native_context(&self, person_id: &str) -> String {
        self.activate();
        let active = self
            .host
            .begin_open("owner", "discord", &self.account_id, "discord.com")
            .expect("begin native Discord host generation");
        let binding =
            manual_peer_binding(&self.core, person_id.to_owned()).expect("manual peer binding");
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
            activated.person_id.clone(),
            activated.scope.clone(),
            true,
        )
        .expect("approve manual peer scope");
        set_scope_security(&self.security, activated.scope.clone(), 3600, true)
            .expect("enable decrypted display for this scope");
        *self.scope.lock().unwrap_or_else(|error| error.into_inner()) =
            Some(activated.scope.clone());
        activated.person_id
    }

    /// Flip decrypted display for the currently open conversation, leaving the
    /// scope approval and every other gate exactly as it was.
    fn set_decrypt_display(&self, enabled: bool) {
        self.activate();
        let scope = self
            .scope
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .expect("a context has been opened for this peer");
        set_scope_security(&self.security, scope, 3600, enabled)
            .expect("set decrypted display for this scope");
    }
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

/// The whole inbound contract for one single-chunk protected message:
///
/// * an empty inbox drains to an explicit empty batch, not an error;
/// * a well-formed inbound message decrypts to exactly the sender's plaintext;
/// * a row carrying a wire addressed to a **different recipient** is refused
///   even when its key-server routing (sender id + scope id) is correct;
/// * a truncated bundle, a non-base64 bundle, and a correctly encrypted row
///   copied under the wrong relay scope are all refused;
/// * none of those refusals prevent the honest message in the same page from
///   opening;
/// * replaying the exact authenticated row does not produce a second open.
#[test]
fn native_discord_inbound_opens_once_and_refuses_foreign_malformed_and_replayed_rows() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("single");
    let relay_url = relay.base_url();

    let alice = Peer::new(&storage, "alice", &relay_url, "aaaa1111");
    let bob = Peer::new(&storage, "bob", &relay_url, "bbbb2222");
    // Charlie exists only so a *legitimately encrypted* message addressed to
    // somebody else can be offered to Bob. No account is registered anywhere;
    // this is a locally generated identity inside the temporary test root.
    let charlie = Peer::new(&storage, "charlie", &relay_url, "cccc3333");

    // Alice -> Charlie first, so its bundle exists before Alice's live
    // conversation with Bob is opened. Activating a new manual peer context
    // invalidates the previous lease, which is exactly the ordering a real
    // operator produces by switching conversations.
    alice.open_native_context_to(&charlie.friend_code, &charlie.safety_number);
    charlie.open_native_context_to(&alice.friend_code, &alice.safety_number);
    alice.activate();
    prepare_native_discord_overlay_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        "fixture text addressed to a third identity".to_owned(),
        false,
    )
    .expect("prepare the misaddressed fixture message");
    let misaddressed = relay.posted_row(&alice.identity_id, &charlie.identity_id);

    // The live conversation.
    alice.open_native_context_to(&bob.friend_code, &bob.safety_number);
    bob.open_native_context_to(&alice.friend_code, &alice.safety_number);

    // 1. Empty inbox. This must be an explicit empty batch, never an error and
    //    never a refusal that reads like "nothing to do".
    bob.activate();
    let empty = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("an empty inbox drains successfully");
    assert!(empty.messages.is_empty(), "empty inbox yields no messages");
    assert!(
        empty.pending_view_once.is_empty(),
        "empty inbox yields no pending view-once entries"
    );
    assert!(
        empty.acknowledgments.is_empty(),
        "empty inbox yields no receipts"
    );
    assert_eq!(empty.fetched, 0, "empty inbox reports zero fetched");
    //    An empty batch now states *why* it is empty. With the eye on and the
    //    store reachable, "nothing for you" is the only reading left.
    assert!(
        empty.decrypt_display_enabled,
        "an empty batch reports that opening is switched on"
    );
    assert_eq!(
        empty.deferred_rows, 0,
        "an empty inbox owes no deferred rows"
    );

    // 2. One well-formed inbound message.
    const FIXTURE: &str = "native discord receive fixture: line one\n\nline three";
    alice.activate();
    let prepared = prepare_native_discord_overlay_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        FIXTURE.to_owned(),
        false,
    )
    .expect("prepare the protected message");
    assert!(
        prepared.prepared.person_to_person_e2ee,
        "the prepared message is peer end-to-end encrypted"
    );
    assert!(
        prepared.prepared.delivered_to_osl_inbox,
        "the prepared message reached the OSL inbox"
    );
    assert!(
        prepared.flagtext.is_some(),
        "a single-chunk message carries exactly one Discord carrier row"
    );
    let honest = relay.posted_row(&alice.identity_id, &bob.identity_id);
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        1,
        "exactly one relay notice is waiting for the recipient"
    );

    // 3. Rows the honest sender would never produce, all injected under Bob's
    //    real routing key so only the cryptographic bindings can reject them.
    let truncated = {
        let mut bytes = base64_decode(&honest.bundle_b64);
        bytes.truncate(bytes.len() / 2);
        base64_encode(&bytes)
    };
    let foreign_row = relay.inject(
        &alice.identity_id,
        &bob.identity_id,
        &honest.scope_id,
        &misaddressed.bundle_b64,
    );
    let truncated_row = relay.inject(
        &alice.identity_id,
        &bob.identity_id,
        &honest.scope_id,
        &truncated,
    );
    let undecodable_row = relay.inject(
        &alice.identity_id,
        &bob.identity_id,
        &honest.scope_id,
        "not base64 at all!!",
    );
    let wrong_scope_row = relay.inject(
        &alice.identity_id,
        &bob.identity_id,
        "native-overlay:some-other-conversation",
        &honest.bundle_b64,
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        5,
        "four hostile rows sit alongside the honest one"
    );

    // 4. The drain opens exactly the honest message.
    bob.activate();
    let opened = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the protected inbox");
    assert_eq!(
        opened.messages.len(),
        1,
        "exactly one message is opened out of five candidate rows"
    );
    assert!(
        opened.messages[0].plaintext == FIXTURE,
        "the opened plaintext is byte-identical to what the sender encrypted"
    );
    assert!(
        opened.messages[0].context_verified,
        "the opened message is context verified"
    );
    assert!(
        opened.messages[0].person_to_person_e2ee,
        "the opened message is peer end-to-end encrypted"
    );
    assert!(
        !opened.messages[0].view_once_consumed,
        "an ordinary message is not marked view-once consumed"
    );
    assert!(
        opened.messages[0].expires_at > now_secs(),
        "the opened message carries a live expiry"
    );
    assert!(
        opened.pending_view_once.is_empty(),
        "an ordinary message produces no pending view-once entry"
    );
    assert_eq!(opened.fetched, 1, "the batch reports one fetched message");

    //    The correlation handle, end to end. A received message has to be
    //    attributable to the Discord row it belongs to, because the eye's whole
    //    contract is to paint decrypted text over that row in place. Before this,
    //    the batch carried neither an id nor a cover pointer, the drain resolved
    //    the cover only to fetch the ciphertext and then discarded it, and the
    //    only ordering available to the renderer was the key server's inbox
    //    order.
    assert!(
        opened.messages[0].message_id == prepared.prepared.message_id,
        "the opened message names the exact message the sender prepared"
    );
    //    Every message on this path is chunked, so an ordinary one-row message is
    //    a group of exactly one chunk -- which does have a single Discord carrier
    //    row, and must therefore carry its cover.
    assert!(
        opened.messages[0].cover_pointer.is_some(),
        "an ordinary one-row message carries the cover of the row it belongs over"
    );
    assert!(
        opened.messages[0].cover_pointer.as_deref() == prepared.flagtext.as_deref(),
        "the opened message names the exact public Discord carrier row it belongs over"
    );
    assert!(
        opened.decrypt_display_enabled,
        "a batch that opened a message reports opening as switched on"
    );
    assert_eq!(
        opened.deferred_rows, 0,
        "nothing was deferred: every refused row here is a verdict, not a retry"
    );

    // 5. Row-level consumption truth, asserted per row rather than as a total.
    //
    //    The honest row is consumed. The three rows that can never authenticate
    //    are all retained: an unauthenticated row is never deleted, which is
    //    deliberate (a transient decrypt failure must not destroy a real
    //    message) but is also the head-of-line hazard proved separately by
    //    `unauthenticatable_rows_are_retained_and_head_of_line_block_the_drain`.
    assert!(
        !relay.still_pending(&honest.id),
        "the authenticated row is consumed"
    );
    for (row, label) in [
        (&foreign_row, "a wire addressed to a third identity"),
        (&truncated_row, "a truncated bundle"),
        (&undecodable_row, "a non-base64 bundle"),
    ] {
        assert!(
            relay.still_pending(row),
            "{label} is refused without being consumed"
        );
    }

    //    `wrong_scope_row` is the honest bundle re-posted under a different
    //    key-server routing label, and it is retained too. The drain filters on
    //    both halves of the routing key -- sender *and* `scope_id` -- so a row
    //    labelled for another conversation is left for that conversation's own
    //    drain instead of being pulled into this one, recognised as a duplicate
    //    of an already-consumed message, and deleted. Display safety never
    //    depended on this (the conversation binding inside the signed notice is
    //    what gates display), but the routing check is the layer that keeps a
    //    foreign-labelled row out of this reassembly group in the first place.
    assert!(
        relay.still_pending(&wrong_scope_row),
        "a row labelled for another conversation's relay scope is left alone"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        4,
        "three unauthenticatable rows and one foreign-scope row remain after the drain"
    );
    for id in [
        &foreign_row,
        &truncated_row,
        &undecodable_row,
        &wrong_scope_row,
    ] {
        relay.remove_inbox(id);
    }

    // 6. Alice picks up the "opened" receipt for her own sent message.
    alice.activate();
    let receipt = drain_native_discord_overlay_text(&alice.core, &alice.security, &alice.broker)
        .expect("drain the sender's receipt");
    assert_eq!(
        receipt.acknowledgments.len(),
        1,
        "the sender receives exactly one opened receipt"
    );
    assert_eq!(
        receipt.acknowledgments[0].message_id, prepared.prepared.message_id,
        "the receipt is correlated to the logical message id"
    );
    assert!(
        receipt.messages.is_empty(),
        "a receipt drain yields no displayable messages"
    );

    // 7. Replay of the exact authenticated row: no second open.
    relay.inject(
        &alice.identity_id,
        &bob.identity_id,
        &honest.scope_id,
        &honest.bundle_b64,
    );
    bob.activate();
    let replay = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the replayed row");
    assert!(
        replay.messages.is_empty(),
        "a replayed message is never displayed twice"
    );
    assert!(
        replay.pending_view_once.is_empty(),
        "a replayed message produces no pending view-once entry"
    );
    assert_eq!(
        replay.fetched, 0,
        "the replay batch reports nothing fetched"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        0,
        "the replayed row is consumed rather than left behind"
    );

    // 8. And the inbox is empty again.
    let drained = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the now-empty inbox");
    assert!(
        drained.messages.is_empty() && drained.acknowledgments.is_empty(),
        "a fully drained inbox reports nothing"
    );

    // 8b. With decrypted display switched off, the drain still succeeds and still
    //     opens nothing -- but it no longer looks like an empty inbox. That
    //     collapse mattered: a caller could not tell "there is nothing for you"
    //     from "you turned this conversation's decrypted text off", and so
    //     reported the first for the second.
    bob.set_decrypt_display(false);
    let sealed = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("a drain with decrypted display off still succeeds");
    assert!(
        sealed.messages.is_empty(),
        "nothing is opened while decrypted display is off"
    );
    assert!(
        !sealed.decrypt_display_enabled,
        "the batch reports that opening is switched off rather than that the inbox was empty"
    );
    assert!(
        drained.decrypt_display_enabled,
        "the same empty result with the eye on is a different, distinguishable state"
    );
    bob.set_decrypt_display(true);

    // 9. Nothing anywhere under either account root holds the plaintext.
    //    Checked as a byte-window search so a failure reports only the
    //    offending path, never the content.
    if let Some(leaked) = file_containing(&storage.root, FIXTURE.as_bytes()) {
        panic!("message plaintext reached persistent storage at {leaked:?}");
    }
    // The sender's receipt ledger is the one file that names the message at
    // all, and it must be encrypted at rest.
    let receipt_bytes = fs::read(alice.dir.join("hub_native_overlay_receipts.json"))
        .expect("the sender wrote an encrypted receipt ledger");
    assert!(
        ipc::main_password::has_enc_magic(&receipt_bytes),
        "the receipt ledger is encrypted at rest"
    );

    drop(alice);
    drop(bob);
    drop(charlie);
    drop(storage);
}

/// A message larger than one carrier chunk is split by the sender into several
/// independently encrypted rows and must be reassembled by the receiver into
/// exactly the original text -- once. This is the `v = 4` chunk path, which had
/// never been exercised end to end for the native Discord carrier.
#[test]
fn native_discord_multi_chunk_message_reassembles_exactly_once() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("chunked");
    let relay_url = relay.base_url();

    let alice = Peer::new(&storage, "alice", &relay_url, "dddd4444");
    let bob = Peer::new(&storage, "bob", &relay_url, "eeee5555");
    alice.open_native_context_to(&bob.friend_code, &bob.safety_number);
    bob.open_native_context_to(&alice.friend_code, &alice.safety_number);

    // Just over two 40 KiB carrier chunks, with a multi-byte character placed
    // where a naive byte split would cut it in half.
    let fixture = format!("{}\u{1f642}{}", "a".repeat(40_960), "b".repeat(40_960));

    alice.activate();
    let prepared = prepare_native_discord_overlay_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        fixture.clone(),
        false,
    )
    .expect("prepare the chunked protected message");
    assert!(
        prepared.flagtext.is_none(),
        "a multi-chunk message deliberately has no single Discord carrier row"
    );
    let posted = relay.posted_rows(&alice.identity_id, &bob.identity_id);
    assert!(
        posted.len() >= 3,
        "the sender posted one relay notice per carrier chunk"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        posted.len(),
        "every chunk notice is waiting for the recipient"
    );

    bob.activate();
    let opened = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the chunked message");
    assert_eq!(
        opened.messages.len(),
        1,
        "the chunk group reassembles into exactly one message"
    );
    assert!(
        opened.messages[0].plaintext == fixture,
        "the reassembled plaintext is byte-identical to the original"
    );
    assert_eq!(
        opened.fetched, 1,
        "a reassembled chunk group counts as one fetched message"
    );
    // The other half of the correlation handle: a multi-row message deliberately
    // has no single Discord carrier row, so there is no cover to name and OSL
    // reports none rather than naming one of its several rows.
    assert!(
        opened.messages[0].cover_pointer.is_none(),
        "a multi-row message names no single Discord carrier row"
    );
    assert!(
        opened.messages[0].message_id == prepared.prepared.message_id,
        "the reassembled message is still named by the id the sender prepared"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        0,
        "every chunk row is consumed once the group is opened"
    );

    // Replaying the whole group must not re-display it.
    for row in &posted {
        relay.inject(
            &alice.identity_id,
            &bob.identity_id,
            &row.scope_id,
            &row.bundle_b64,
        );
    }
    let replay = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the replayed chunk group");
    assert!(
        replay.messages.is_empty(),
        "a replayed chunk group is never displayed twice"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        0,
        "the replayed chunk rows are consumed rather than left behind"
    );

    drop(alice);
    drop(bob);
    drop(storage);
}

/// Arrival order belongs to the key server, not the sender. A chunk group must
/// reassemble no matter what order its rows come back in, and a group that is
/// still only partly there must survive to a later drain instead of being
/// consumed or corrupted.
#[test]
fn native_discord_chunk_group_survives_reversed_and_split_arrival() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("ordering");
    let relay_url = relay.base_url();

    let alice = Peer::new(&storage, "alice", &relay_url, "ffff6666");
    let bob = Peer::new(&storage, "bob", &relay_url, "00007777");
    alice.open_native_context_to(&bob.friend_code, &bob.safety_number);
    bob.open_native_context_to(&alice.friend_code, &alice.safety_number);

    let fixture = format!("{}\u{1f642}{}", "q".repeat(40_960), "z".repeat(40_960));
    alice.activate();
    prepare_native_discord_overlay_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        fixture.clone(),
        false,
    )
    .expect("prepare the chunked protected message");

    // Lift the whole group out and hand it back worst case first: reversed, and
    // one chunk short. The chunk held back is the one the sender posted first,
    // so the reassembly template cannot come from chunk 0 either.
    let mut rows = relay.take_inbox_for(&bob.identity_id);
    assert!(
        rows.len() >= 3,
        "the sender posted one relay notice per carrier chunk"
    );
    rows.reverse();
    let held_back = rows.pop().expect("hold the first-posted chunk back");
    let partial_ids = rows
        .iter()
        .map(|row| {
            relay.inject(
                &row.sender_id,
                &bob.identity_id,
                &row.scope_id,
                &row.bundle_b64,
            )
        })
        .collect::<Vec<_>>();

    // An incomplete group displays nothing, and reports itself exactly like an
    // empty inbox: same `Ok`, same empty vectors, same `fetched == 0`. Nothing
    // in the batch distinguishes "still arriving" from "nothing for you".
    bob.activate();
    let partial = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("an incomplete chunk group drains successfully");
    assert!(
        partial.messages.is_empty(),
        "an incomplete chunk group displays nothing"
    );
    assert!(
        partial.pending_view_once.is_empty(),
        "an incomplete chunk group produces no pending view-once entry"
    );
    assert_eq!(
        partial.fetched, 0,
        "an incomplete chunk group is indistinguishable from an empty inbox"
    );
    // The load-bearing half: the chunks that did arrive are not consumed, so
    // the message is recoverable rather than silently destroyed.
    for id in &partial_ids {
        assert!(
            relay.still_pending(id),
            "an incomplete group's chunks are retained for a later drain"
        );
    }

    // The missing chunk lands last of all; the group completes fully reversed.
    relay.inject(
        &held_back.sender_id,
        &bob.identity_id,
        &held_back.scope_id,
        &held_back.bundle_b64,
    );
    let opened = drain_native_discord_overlay_text(&bob.core, &bob.security, &bob.broker)
        .expect("drain the completed chunk group");
    assert_eq!(
        opened.messages.len(),
        1,
        "the out-of-order group reassembles into exactly one message"
    );
    assert!(
        opened.messages[0].plaintext == fixture,
        "chunk arrival order does not affect the reassembled plaintext"
    );
    assert_eq!(
        opened.fetched, 1,
        "the completed group counts as one fetched message"
    );
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        0,
        "every chunk row is consumed once the group opens"
    );

    if let Some(leaked) = file_containing(&storage.root, fixture.as_bytes()) {
        panic!("message plaintext reached persistent storage at {leaked:?}");
    }

    drop(alice);
    drop(bob);
    drop(storage);
}

struct FixtureAttachment {
    attachment_id: String,
    original_filename: String,
    plaintext_size: u64,
    expires_at: i64,
    view_once: bool,
    sealed_size: u64,
    ciphertext_sha256: String,
    object_id: String,
    fetch_token: String,
}

fn deliver_fixture_attachment(sender: &Peer) -> FixtureAttachment {
    const PLAINTEXT_SIZE: u64 = 128;
    const SEALED_SIZE: u64 = 256;
    sender.activate();
    let plan = begin_native_overlay_attachment(
        &sender.core,
        &sender.broker,
        "fixture.png".to_owned(),
        PLAINTEXT_SIZE,
        false,
    )
    .expect("begin the framed fixture attachment");
    let expected = FixtureAttachment {
        attachment_id: plan.attachment_id.clone(),
        original_filename: plan.original_filename.clone(),
        plaintext_size: plan.plaintext_size,
        expires_at: plan.expires_at,
        view_once: plan.view_once,
        sealed_size: SEALED_SIZE,
        ciphertext_sha256: "a".repeat(64),
        object_id: "b".repeat(32),
        fetch_token: "c".repeat(32),
    };
    let delivered = deliver_native_overlay_attachment(
        &sender.core,
        &sender.broker,
        plan,
        expected.sealed_size,
        expected.ciphertext_sha256.clone(),
        expected.object_id.clone(),
        expected.fetch_token.clone(),
    )
    .expect("deliver the framed fixture attachment");
    assert!(
        delivered.attachment_id == expected.attachment_id
            && delivered.original_filename == expected.original_filename
            && delivered.plaintext_size == expected.plaintext_size
            && delivered.expires_at == expected.expires_at
            && delivered.view_once == expected.view_once
            && delivered.delivered_to_osl_inbox,
        "the attachment notice preserves the production seal plan"
    );
    expected
}

/// The signed sender filter is applied before the 64-row page bound for both
/// production receive consumers. A's text and attachment remain reachable
/// behind one full older page, every foreign row survives A's work, and B's
/// authenticated row remains independently drainable afterward.
#[test]
fn sender_filtered_text_and_attachment_drains_bypass_64_foreign_rows_and_preserve_other_peers() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("sender-filtered");
    let relay_url = relay.base_url();

    let sender_a = Peer::new(&storage, "sender-a", &relay_url, "11118888");
    let receiver = Peer::new(&storage, "receiver", &relay_url, "22229999");
    let sender_b = Peer::new(&storage, "sender-b", &relay_url, "33330000");
    sender_a.open_native_context_to(&receiver.friend_code, &receiver.safety_number);
    sender_b.open_native_context_to(&receiver.friend_code, &receiver.safety_number);
    let receiver_b_person =
        receiver.open_native_context_to(&sender_b.friend_code, &sender_b.safety_number);
    receiver.open_native_context_to(&sender_a.friend_code, &sender_a.safety_number);

    const B_FIXTURE: &str = "independently drainable sender B fixture";
    sender_b.activate();
    prepare_native_discord_overlay_text(
        &sender_b.core,
        &sender_b.security,
        &sender_b.broker,
        B_FIXTURE.to_owned(),
        false,
    )
    .expect("prepare B's protected message");
    let b_honest = relay.posted_row(&sender_b.identity_id, &receiver.identity_id);

    // B's authenticated row plus 31 B fillers and 32 C fillers make exactly one
    // older page while respecting the real 32-row per-sender admission ceiling.
    let filler = base64_encode(&[0x7fu8; 96]);
    let mut foreign_ids = BTreeSet::from([b_honest.id.clone()]);
    for _ in 0..31 {
        foreign_ids.insert(relay.inject(
            &sender_b.identity_id,
            &receiver.identity_id,
            "native-overlay:some-unopened-conversation",
            &filler,
        ));
    }
    let sender_c = "osl-unopened-conversation-c";
    for _ in 0..32 {
        foreign_ids.insert(relay.inject(
            sender_c,
            &receiver.identity_id,
            "native-overlay:some-other-unopened-conversation",
            &filler,
        ));
    }
    assert_eq!(foreign_ids.len(), MAX_DRAIN_ROWS);

    const A_FIXTURE: &str = "active sender A fixture behind a full foreign page";
    sender_a.activate();
    let a_prepared = prepare_native_discord_overlay_text(
        &sender_a.core,
        &sender_a.security,
        &sender_a.broker,
        A_FIXTURE.to_owned(),
        false,
    )
    .expect("prepare A's protected message");
    let expected_attachment = deliver_fixture_attachment(&sender_a);
    let a_rows = relay.posted_rows(&sender_a.identity_id, &receiver.identity_id);
    assert_eq!(
        a_rows.len(),
        2,
        "A posts one framed text row and one framed attachment row"
    );
    let a_text_row = a_rows
        .iter()
        .find(|row| ipc::wire_v2::is_native_overlay_relay_bundle(&base64_decode(&row.bundle_b64)))
        .expect("identify A's framed text row");
    let a_attachment_row = a_rows
        .iter()
        .find(|row| ipc::wire_v2::is_attachment_bundle(&base64_decode(&row.bundle_b64)))
        .expect("identify A's framed attachment row");
    assert_eq!(
        relay.pending_for(&receiver.identity_id),
        MAX_DRAIN_ROWS + 2,
        "64 older foreign rows precede A's text and attachment"
    );

    receiver.activate();
    let opened =
        drain_native_discord_overlay_text(&receiver.core, &receiver.security, &receiver.broker)
            .expect("drain A through the sender-filtered production text path");
    assert_eq!(opened.messages.len(), 1, "exactly A's text opens");
    assert!(
        opened.messages[0].plaintext == A_FIXTURE,
        "A's opened plaintext is byte-identical"
    );
    assert!(
        opened.messages[0].message_id == a_prepared.prepared.message_id
            && opened.messages[0].cover_pointer.as_deref() == a_prepared.flagtext.as_deref(),
        "A's opened text retains its production message and cover correlation"
    );
    assert!(
        !relay.still_pending(&a_text_row.id),
        "A's authenticated text row is consumed"
    );
    assert!(
        relay.still_pending(&a_attachment_row.id),
        "the text drain leaves A's attachment for the attachment consumer"
    );
    for id in &foreign_ids {
        assert!(
            relay.still_pending(id),
            "A's text drain preserves every exact foreign row ID"
        );
    }

    let listed =
        list_native_overlay_attachments(&receiver.core, &receiver.security, &receiver.broker)
            .expect("list A through the sender-filtered attachment path");
    assert_eq!(listed.len(), 1, "exactly A's attachment is listed");
    assert!(
        listed[0].attachment_id == expected_attachment.attachment_id
            && listed[0].original_filename == expected_attachment.original_filename
            && listed[0].plaintext_size == expected_attachment.plaintext_size
            && listed[0].expires_at == expected_attachment.expires_at
            && listed[0].view_once == expected_attachment.view_once,
        "the attachment list preserves A's authenticated notice metadata"
    );
    let plan = take_native_overlay_attachment(
        &receiver.core,
        &receiver.security,
        &receiver.broker,
        &listed[0].attachment_id,
    )
    .expect("take A's attachment open plan");
    assert!(
        plan.attachment_id == expected_attachment.attachment_id
            && plan.original_filename == expected_attachment.original_filename
            && plan.plaintext_size == expected_attachment.plaintext_size
            && plan.view_once == expected_attachment.view_once
            && plan.sealed_size == expected_attachment.sealed_size
            && plan.ciphertext_sha256 == expected_attachment.ciphertext_sha256
            && plan.object_id == expected_attachment.object_id
            && plan.fetch_token == expected_attachment.fetch_token,
        "the taken attachment plan is the exact one A framed"
    );
    assert!(
        relay.still_pending(&a_attachment_row.id),
        "listing and taking a plan are non-consuming"
    );
    for id in &foreign_ids {
        assert!(
            relay.still_pending(id),
            "A's attachment consumers preserve every exact foreign row ID"
        );
    }

    let gets = relay.control_inbox_gets();
    assert_eq!(gets.len(), 3, "text, list and take each issue one GET");
    assert!(
        gets.iter().all(|get| {
            get.recipient_id == receiver.identity_id
                && get.sender_id.as_deref() == Some(sender_a.identity_id.as_str())
        }),
        "every A consumer requests the exact active sender"
    );

    receiver.reopen_native_context(&receiver_b_person);
    let b_opened =
        drain_native_discord_overlay_text(&receiver.core, &receiver.security, &receiver.broker)
            .expect("drain B independently after A");
    assert_eq!(b_opened.messages.len(), 1, "exactly B's valid text opens");
    assert!(
        b_opened.messages[0].plaintext == B_FIXTURE,
        "B's independently opened plaintext is byte-identical"
    );
    assert!(
        !relay.still_pending(&b_honest.id),
        "B's authenticated row is consumed only by B's drain"
    );
    for id in foreign_ids
        .iter()
        .filter(|id| id.as_str() != b_honest.id.as_str())
    {
        assert!(
            relay.still_pending(id),
            "B's drain preserves all remaining foreign filler IDs"
        );
    }
    assert!(
        relay.still_pending(&a_attachment_row.id),
        "B's drain leaves A's attachment independently available"
    );
    let gets = relay.control_inbox_gets();
    assert_eq!(gets.len(), 4, "B adds exactly one independent GET");
    assert!(
        gets[3].recipient_id == receiver.identity_id
            && gets[3].sender_id.as_deref() == Some(sender_b.identity_id.as_str()),
        "the final GET requests B rather than widening the page"
    );

    if let Some(leaked) = file_containing(&storage.root, A_FIXTURE.as_bytes()) {
        panic!("A plaintext reached persistent storage at {leaked:?}");
    }
    if let Some(leaked) = file_containing(&storage.root, B_FIXTURE.as_bytes()) {
        panic!("B plaintext reached persistent storage at {leaked:?}");
    }

    drop(sender_a);
    drop(sender_b);
    drop(receiver);
    drop(storage);
}

fn assert_text_and_attachment_refuse_reply(reply: ControlInboxGetReply) {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("sender-filter-refusal");
    let relay_url = relay.base_url();
    let sender = Peer::new(&storage, "refusal-sender", &relay_url, "44441111");
    let receiver = Peer::new(&storage, "refusal-receiver", &relay_url, "55552222");
    sender.open_native_context_to(&receiver.friend_code, &receiver.safety_number);
    receiver.open_native_context_to(&sender.friend_code, &sender.safety_number);

    sender.activate();
    prepare_native_discord_overlay_text(
        &sender.core,
        &sender.security,
        &sender.broker,
        "closed-path text fixture".to_owned(),
        false,
    )
    .expect("prepare the refusal text row");
    deliver_fixture_attachment(&sender);
    relay.inject(
        "osl-refusal-other-sender",
        &receiver.identity_id,
        "native-overlay:other-scope",
        &base64_encode(&[0x7fu8; 96]),
    );
    let before = relay.pending_ids_for(&receiver.identity_id);

    receiver.activate();
    relay.queue_control_inbox_get_reply(reply.clone());
    let text_error = match drain_native_discord_overlay_text(
        &receiver.core,
        &receiver.security,
        &receiver.broker,
    ) {
        Ok(_) => panic!("the text drain accepted an unconfirmed or widened page"),
        Err(error) => error,
    };
    assert!(
        text_error == "OSL could not receive protected messages",
        "the text drain fails through its closed production verdict"
    );

    relay.queue_control_inbox_get_reply(reply);
    let attachment_error =
        match list_native_overlay_attachments(&receiver.core, &receiver.security, &receiver.broker)
        {
            Ok(_) => panic!("the attachment drain accepted an unconfirmed or widened page"),
            Err(error) => error,
        };
    assert!(
        attachment_error == "OSL could not receive private attachments",
        "the attachment drain fails through its closed production verdict"
    );
    assert!(
        relay.pending_ids_for(&receiver.identity_id) == before,
        "a refused page consumes no inbox row"
    );
    let gets = relay.control_inbox_gets();
    assert_eq!(gets.len(), 2, "each consumer issues exactly one request");
    assert!(
        gets.iter().all(|get| {
            get.recipient_id == receiver.identity_id
                && get.sender_id.as_deref() == Some(sender.identity_id.as_str())
        }),
        "both refused requests remain signed-sender-shaped without fallback"
    );

    drop(sender);
    drop(receiver);
    drop(storage);
}

#[test]
fn native_discord_text_and_attachment_drains_refuse_missing_sender_echo() {
    assert_text_and_attachment_refuse_reply(ControlInboxGetReply::MissingEcho);
}

#[test]
fn native_discord_text_and_attachment_drains_refuse_mismatched_sender_echo() {
    assert_text_and_attachment_refuse_reply(ControlInboxGetReply::MismatchedEcho(
        "osl-refusal-other-sender".to_owned(),
    ));
}

#[test]
fn native_discord_text_and_attachment_drains_refuse_cross_sender_rows_under_matching_echo() {
    assert_text_and_attachment_refuse_reply(ControlInboxGetReply::CrossSender(
        "osl-refusal-other-sender".to_owned(),
    ));
}

#[test]
fn native_discord_text_and_attachment_drains_refuse_unfiltered_fallback_without_echo() {
    assert_text_and_attachment_refuse_reply(ControlInboxGetReply::UnfilteredWithoutEcho);
}

// ---------------------------------------------------------------------------
// Small helpers. Deliberately local so the test binary needs no extra crate.
// ---------------------------------------------------------------------------

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(triple >> 18) as usize & 63] as char);
        out.push(BASE64_ALPHABET[(triple >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(triple >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[triple as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes().filter(|byte| *byte != b'=') {
        let value = BASE64_ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .expect("the fixture bundle is standard base64") as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    out
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Walk every regular file under `root` and return the first one whose bytes
/// contain `needle`. Only the path is ever surfaced, never the content.
fn file_containing(root: &Path, needle: &[u8]) -> Option<PathBuf> {
    let mut stack = vec![root.to_owned()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(entry_path),
                Ok(kind) if kind.is_file() => {
                    if fs::read(&entry_path).is_ok_and(|bytes| contains_bytes(&bytes, needle)) {
                        return Some(entry_path);
                    }
                }
                _ => {}
            }
        }
    }
    None
}
