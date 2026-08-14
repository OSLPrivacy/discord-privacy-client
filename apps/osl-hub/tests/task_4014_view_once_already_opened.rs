//! TASK 4014: a native Discord view-once message stays unavailable after
//! its first reveal, including after the receiving app state is recreated.

#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_native_manual_peer_context, drain_native_discord_overlay_text,
    prepare_native_discord_overlay_text, reveal_native_discord_overlay_view_once, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_manual_peer_scope_permission,
    set_scope_security, verify_friend_safety_number, HubSecurityState,
};
use osl_privacy_hub::service_host::ServiceHostState;
use osl_privacy_hub::services::save_messaging_risk_agreement;
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

fn ai_carrier_fixture() -> osl_privacy_hub::ai_carrier::AiCarrierState {
    osl_privacy_hub::ai_carrier::AiCarrierState::default()
}

const TEST_MAIN_PASSWORD: &str = "task-4014-native-discord-password";
const MAX_DRAIN_ROWS: usize = 64;
const VIEW_ONCE_UNAVAILABLE: &str = "This view-once message is unavailable or expired";

fn fixture_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Clone)]
struct InboxRow {
    id: String,
    sender_id: String,
    recipient_id: String,
    scope_id: String,
    bundle_b64: String,
    created_at: i64,
}

/// One stored cipher-store object, modelled on the currently deployed Worker:
/// the server assigns the id, and the upload supplies one fetch token that
/// authorizes both reads and deletes.
#[derive(Clone)]
struct BlobRow {
    bytes: Vec<u8>,
    fetch_token: String,
}

/// Lowercase hex bearer-token shape used by the deployed cipher-store Worker.
fn canonical_hex_header(value: Option<&String>, length: usize) -> Option<String> {
    let value = value?;
    (value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| value.clone())
}

/// One uploaded wrapped share. The loopback relay never implemented
/// /v1/wrapped-keys at all, so every send failed with the deliberately generic
/// "OSL could not deliver the protected message" and took twelve receive-path
/// tests with it. Model the endpoint faithfully, including the two properties the
/// refusal tests depend on: a share is readable ONLY by its recipient, and a
/// single-use share is consumed by the first successful fetch.
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ControlInboxGetRecord {
    recipient_id: String,
    sender_id: Option<String>,
}

#[derive(Clone)]
enum ControlInboxGetReply {
    Honest,
    MissingEcho,
    MissingDisposition,
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
    wrapped_keys: BTreeMap<String, WrappedKeyRow>,
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

    fn live_wrapped_keys_for(&self, sender_id: &str, recipient_id: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .wrapped_keys
            .values()
            .filter(|row| {
                row.sender_id == sender_id && row.recipient_id == recipient_id && !row.consumed
            })
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

    fn put_inbox_rows(&self, rows: impl IntoIterator<Item = InboxRow>) {
        self.state.lock().unwrap().inbox.extend(rows);
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

    fn reset_wrapped_key_consumption_for(&self, recipient_id: &str) -> usize {
        let mut reset = 0usize;
        for row in self
            .state
            .lock()
            .unwrap()
            .wrapped_keys
            .values_mut()
            .filter(|row| row.recipient_id == recipient_id && row.single_use && row.consumed)
        {
            row.consumed = false;
            reset += 1;
        }
        reset
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
            // The match binds the path with its query already stripped, so read the
            // recipient from the original request line.
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
                // A share belongs to exactly one recipient. Without this check the
                // cross-sender and mismatched-echo refusal tests could not fail.
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
        // The currently deployed cipher-store contract. Production still runs
        // the bridge protocol: the client supplies one `x-osl-fetch-token`, the
        // server assigns the blob id, and that same token gates fetch/delete.
        // The destination capability protocol (`x-osl-blob-id` plus digests)
        // must keep failing here until it is actually deployed.
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
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
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
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let mut state = state.lock().unwrap();
            let presented = headers.get("x-osl-fetch-token");
            // Production's bridge delete is idempotent for an unknown id, and
            // a known id is gated by the same token as fetch.
            let allowed = state
                .blobs
                .get(id)
                .is_none_or(|blob| presented == Some(&blob.fetch_token));
            if allowed {
                state.blobs.remove(id);
                bytes_response(204, "application/octet-stream", Vec::new())
            } else {
                json_response(403, json!({ "error": "fetch_token_mismatch" }))
            }
        }
        // The shipping receive boundary observes a signed sender-filter
        // capability floor before it will drain, exactly as
        // `keyserver-cf/src/endpoints/sender-filter-capability-floor.ts`
        // serves it. The fixture answers only for identities it was told
        // about and only for a signed request, so a drain can never reach a
        // filtered page through an unauthenticated or unknown-recipient floor.
        ("GET", route) if route.starts_with("/v1/sender-filter-capability-floor/") => {
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
            let anchor = floor_identities()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
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
            let delivery = json!({
                "live": items.len(),
                "retryable": 0,
                "quarantined": 0,
                "retired": 0,
            });
            match reply {
                ControlInboxGetReply::Honest => match sender {
                    Some(sender) => json_response(
                        200,
                        json!({
                            "items": items,
                            "filtered_sender_id": sender,
                            "filtered_sender_delivery": delivery,
                        }),
                    ),
                    None => json_response(200, json!({ "items": items })),
                },
                ControlInboxGetReply::MissingEcho => json_response(
                    200,
                    json!({ "items": items, "filtered_sender_delivery": delivery }),
                ),
                ControlInboxGetReply::MissingDisposition => {
                    json_response(200, json!({ "items": items, "filtered_sender_id": sender }))
                }
                ControlInboxGetReply::UnfilteredWithoutEcho => {
                    json_response(200, json!({ "items": items }))
                }
                ControlInboxGetReply::MismatchedEcho(echoed) => json_response(
                    200,
                    json!({
                        "items": items,
                        "filtered_sender_id": echoed,
                        "filtered_sender_delivery": delivery,
                    }),
                ),
                ControlInboxGetReply::CrossSender(_) => json_response(
                    200,
                    json!({
                        "items": items,
                        "filtered_sender_id": sender,
                        "filtered_sender_delivery": delivery,
                    }),
                ),
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

/// Public identity anchors the fixture key server needs in order to answer the
/// signed sender-filter capability-floor observation the production receive
/// boundary makes before every drain. `serve_request` is a free function and
/// every test in this binary is serialized by `fixture_lock`, so a single
/// process-wide registry is enough; `Peer::new` is the only writer.
fn floor_identities() -> &'static Mutex<BTreeMap<String, Vec<u8>>> {
    static IDENTITIES: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
    IDENTITIES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// `keystore::sender_filter_rollout::sender_filter_floor_identity_anchor_sha256`,
/// recomputed here from public inputs only so the fixture proves the anchor the
/// client independently derives rather than echoing one the client supplied.
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

struct NativePeer {
    dir: PathBuf,
    identity_id: String,
    account_id: String,
    friend_code: String,
    core: HubCoreState,
}

impl NativePeer {
    fn new(storage: &TestStorage, name: &str, relay_url: &str, account_suffix: &str) -> Self {
        let dir = storage.account(name, relay_url);
        let identity = keystore::generate_identity(format!("osl-task-4014-{name}"));
        let identity_id = identity.user_id.clone();
        floor_identities()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                identity_id.clone(),
                identity.ed25519_public.as_bytes().to_vec(),
            );
        let core = core(identity, relay_url);
        TestStorage::activate(&dir);
        let friend_code = export_friend_code(&core)
            .expect("TASK4014 exports the peer friend code")
            .friend_code;
        Self {
            dir,
            identity_id,
            account_id: format!("native-discord-{account_suffix}"),
            friend_code,
            core,
        }
    }

    fn activate_storage(&self) {
        TestStorage::activate(&self.dir);
    }

    fn add_and_verify_friend(&self, security: &HubSecurityState, friend_code: &str) -> String {
        self.activate_storage();
        let friend = add_friend_code(
            &self.core,
            security,
            friend_code.to_owned(),
            Some("TASK4014 peer".to_owned()),
        )
        .expect("TASK4014 adds the native peer");
        verify_friend_safety_number(
            &self.core,
            security,
            friend.person_id.clone(),
            friend.safety_number,
        )
        .expect("TASK4014 verifies the native peer");
        friend.person_id
    }

    fn activate_native_context(
        &self,
        security: &HubSecurityState,
        broker: &HubBrokerState,
        host: &ServiceHostState,
        person_id: &str,
    ) {
        self.activate_native_context_with_core(&self.core, security, broker, host, person_id);
    }

    fn activate_native_context_with_core(
        &self,
        core: &HubCoreState,
        security: &HubSecurityState,
        broker: &HubBrokerState,
        host: &ServiceHostState,
        person_id: &str,
    ) {
        self.activate_storage();
        let active = host
            .begin_open(
                &self.identity_id,
                "discord",
                &self.account_id,
                "discord.com",
            )
            .expect("TASK4014 begins the native Discord host generation");
        let binding = manual_peer_binding(core, person_id.to_owned())
            .expect("TASK4014 resolves the persisted verified peer");
        let activated =
            activate_owned_native_manual_peer_context(broker, &self.identity_id, &active, binding)
                .expect("TASK4014 activates the native Discord peer context");
        set_manual_peer_scope_permission(
            core,
            security,
            "discord",
            &self.account_id,
            activated.person_id,
            activated.scope.clone(),
            true,
        )
        .expect("TASK4014 persists the native peer scope grant");
        set_scope_security(security, activated.scope, 3600, true)
            .expect("TASK4014 enables protected text display");
        save_messaging_risk_agreement(&self.identity_id, "discord", &self.account_id)
            .expect("TASK4014 records the native Discord messaging agreement");
    }
}

fn refusal_text(
    result: Result<osl_privacy_hub::broker::OpenedNativeOverlayText, String>,
    attempt: &str,
) -> String {
    match result {
        Err(error) => error,
        Ok(_) => panic!("TASK4014 {attempt} unexpectedly revealed private text"),
    }
}

#[test]
fn task_4014_view_once_already_opened_is_said_on_screen_and_survives_restart() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("task-4014-view-once-already-opened");
    let relay_url = relay.base_url();

    let alice = NativePeer::new(&storage, "alice", &relay_url, "task4014-alice");
    let bob = NativePeer::new(&storage, "bob", &relay_url, "task4014-bob");

    let alice_security = HubSecurityState::default();
    let alice_broker = HubBrokerState::default();
    let alice_host = ServiceHostState::default();
    let bob_security = HubSecurityState::default();
    let bob_broker = HubBrokerState::default();
    let bob_host = ServiceHostState::default();

    let bob_person_id = alice.add_and_verify_friend(&alice_security, &bob.friend_code);
    let alice_person_id = bob.add_and_verify_friend(&bob_security, &alice.friend_code);
    alice.activate_native_context(&alice_security, &alice_broker, &alice_host, &bob_person_id);
    bob.activate_native_context(&bob_security, &bob_broker, &bob_host, &alice_person_id);

    let private_words = format!(
        "TASK4014 exact private words {}",
        uuid::Uuid::new_v4().simple()
    );
    alice.activate_storage();
    let prepared = prepare_native_discord_overlay_text(
        &alice.core,
        &alice_security,
        &alice_broker,
        &ai_carrier_fixture(),
        private_words.clone(),
        true,
    )
    .expect("TASK4014 sends one production native Discord view-once message");
    assert!(prepared.prepared.view_once, "TASK4014 send is view-once");

    bob.activate_storage();
    let pending = drain_native_discord_overlay_text(&bob.core, &bob_security, &bob_broker)
        .expect("TASK4014 recipient drains the view-once message to pending");
    assert!(
        pending.messages.is_empty(),
        "TASK4014 drain does not reveal"
    );
    assert_eq!(
        pending.pending_view_once.len(),
        1,
        "TASK4014 has exactly one pending view-once message",
    );
    assert!(
        pending.pending_view_once[0].message_id == prepared.prepared.message_id,
        "TASK4014 pending row names the sent message",
    );

    let first = reveal_native_discord_overlay_view_once(
        &bob.core,
        &bob_security,
        &bob_broker,
        &prepared.prepared.message_id,
    )
    .expect("TASK4014 first reveal opens the pending message");
    let first_private_words = first.plaintext.matches(&private_words).count();
    assert!(
        first.plaintext == private_words,
        "TASK4014 first reveal returns the exact private words",
    );
    assert_eq!(
        first_private_words, 1,
        "TASK4014 first reveal contains the exact private words once",
    );
    assert!(
        first.view_once_consumed,
        "TASK4014 first reveal reports view-once consumption",
    );

    let second_sentence = refusal_text(
        reveal_native_discord_overlay_view_once(
            &bob.core,
            &bob_security,
            &bob_broker,
            &prepared.prepared.message_id,
        ),
        "second reveal",
    );
    let second_private_words = second_sentence.matches(&private_words).count();
    assert_eq!(
        second_sentence, VIEW_ONCE_UNAVAILABLE,
        "TASK4014 second reveal returns the fixed view-once sentence",
    );
    assert_eq!(
        second_private_words, 0,
        "TASK4014 second reveal returns zero private words",
    );

    drop(bob_host);
    drop(bob_broker);
    drop(bob_security);

    let reopened_core = {
        let identity = bob
            .core
            .osl
            .identity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .expect("TASK4014 receiving identity remains available for app restart");
        let reopened = core(identity, &relay_url);
        let peer_map = ipc::peer_map::load_peer_map_from_path(&bob.dir.join("peer_map.json"))
            .expect("TASK4014 reloads the persisted peer map after app restart");
        *reopened
            .osl
            .peer_map
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = peer_map;
        reopened
    };
    let reopened_security = HubSecurityState::default();
    let reopened_broker = HubBrokerState::default();
    let reopened_host = ServiceHostState::default();
    bob.activate_native_context_with_core(
        &reopened_core,
        &reopened_security,
        &reopened_broker,
        &reopened_host,
        &alice_person_id,
    );

    let third_sentence = refusal_text(
        reveal_native_discord_overlay_view_once(
            &reopened_core,
            &reopened_security,
            &reopened_broker,
            &prepared.prepared.message_id,
        ),
        "post-restart reveal",
    );
    let third_private_words = third_sentence.matches(&private_words).count();
    assert_eq!(
        third_sentence, VIEW_ONCE_UNAVAILABLE,
        "TASK4014 post-restart reveal returns the fixed view-once sentence",
    );
    assert_eq!(
        third_private_words, 0,
        "TASK4014 post-restart reveal returns zero private words",
    );

    println!(
        "TASK4014_FIRST exact_private_words=\"{}\" private_word_count={first_private_words}",
        private_words,
    );
    println!(
        "TASK4014_SECOND sentence=\"{}\" private_word_count={second_private_words}",
        second_sentence,
    );
    println!(
        "TASK4014_AFTER_REOPEN sentence=\"{}\" private_word_count={third_private_words}",
        third_sentence,
    );
}
