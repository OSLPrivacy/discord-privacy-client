#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, documented_message_service_send_failures, drain_osl_chat_text,
    prepare_osl_chat_text, prepare_peer_prose_text,
    retry_available_for_message_service_send_failure_before_cover_preparation, HubBrokerState,
    NativeOverlayAcknowledgmentStatus, OSL_RELAY_NOTICE_QUEUED,
    activate_owned_osl_chat_context, drain_osl_chat_text, load_osl_chat_history,
    prepare_osl_chat_text, prepare_peer_prose_text, HubBrokerState,
    NativeOverlayAcknowledgmentStatus, OpenedNativeOverlayTextBatch, OSL_RELAY_NOTICE_QUEUED,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_friend_account_reach_choice,
    set_manual_peer_scope_permission, set_scope_security, verify_friend_safety_number,
    HubSecurityState,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "sealed-relay-fixture-password";

#[derive(Clone, Debug, Eq, PartialEq)]
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
    fetch_digest: String,
    manage_digest: String,
}

fn sha256_hex(value: &str) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `x-osl-blob-id` / `x-osl-delivery-tag` shape in
/// `cipher-store-cf/src/endpoints/blob.ts`.
fn canonical_hex_header(value: Option<&String>, length: usize) -> Option<String> {
    let value = value?;
    (value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| value.clone())
}

#[derive(Default)]
struct RelayState {
    next_id: u64,
    inbox: Vec<InboxRow>,
    posted: Vec<InboxRow>,
    blobs: BTreeMap<String, BlobRow>,
    /// Ciphertext, keyed by the **caller-supplied** fetch digest rather than by
    /// blob id -- the shipping Worker's two key spaces, kept apart here as they
    /// are there (`blob_capability_index` in D1 by id, R2 by
    /// `SHA-256(fetch_cap)`). D-264 exists only because they are different key
    /// spaces: a caller holding a genuinely fresh id it owns still names any
    /// digest it likes, so winning the row is not what protects the bytes.
    /// Collapsing them into `BlobRow`, as this fixture used to, makes that
    /// defect unrepresentable and the double silently stronger than the thing
    /// it doubles.
    payloads: BTreeMap<String, Vec<u8>>,
    wrapped_keys: BTreeMap<String, WrappedKeyRow>,
    /// Public Ed25519 identity keys this relay has been told about, keyed by
    /// user id. Only these identities get a sender-filter capability-floor
    /// observation, so an unknown recipient cannot obtain one and no drain can
    /// reach a filtered page through a floor the server never issued.
    floor_identities: BTreeMap<String, Vec<u8>>,
    /// When set, `POST /v1/control-inbox` closes the connection without
    /// answering. Every other route keeps working, which is the only way to
    /// reach the half-delivered window D-223 is about: the wrapped key lands,
    /// the relay notice does not.
    control_inbox_unreachable: bool,
}

/// One uploaded wrapped share. A share is readable ONLY by its recipient.
/// Transmission does not consume a single-use share: the production keyserver
/// retains it until a recipient-acknowledged receipt can authorize deletion.
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

    /// Take the relay notice lane offline without touching any other route.
    fn set_control_inbox_unreachable(&self, unreachable: bool) {
        self.state.lock().unwrap().control_inbox_unreachable = unreachable;
    }

    fn wrapped_keys_for(&self, sender_id: &str, recipient_id: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .wrapped_keys
            .values()
            .filter(|row| row.sender_id == sender_id && row.recipient_id == recipient_id)
            .count()
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

    fn posted_for(&self, sender_id: &str, recipient_id: &str) -> Vec<InboxRow> {
        self.state
            .lock()
            .unwrap()
            .posted
            .iter()
            .filter(|row| row.sender_id == sender_id && row.recipient_id == recipient_id)
            .cloned()
            .collect()
    }

    fn single_wrapped_key_id_for(&self, sender_id: &str, recipient_id: &str) -> String {
        let state = self.state.lock().unwrap();
        let ids = state
            .wrapped_keys
            .iter()
            .filter_map(|(id, row)| {
                (row.sender_id == sender_id && row.recipient_id == recipient_id).then(|| id.clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 1, "expected exactly one wrapped key row");
        ids.into_iter().next().unwrap()
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

fn attach_history_store(core: &HubCoreState, identity: &keystore::Identity, account_dir: &Path) {
    let history_dir = account_dir.join("message-store");
    fs::create_dir_all(&history_dir).expect("create isolated OSL Chat history store");
    let store = store::MessageStore::open(&history_dir, identity.x25519_secret.as_bytes())
        .expect("open isolated OSL Chat history store");
    *core.osl.message_store.lock().unwrap() = Some(store);
}

fn matching_history_plaintexts(
    core: &HubCoreState,
    broker: &HubBrokerState,
    marker_prefix: &str,
) -> Vec<String> {
    let mut rows = load_osl_chat_history(core, broker)
        .expect("fetch OSL Chat conversation history")
        .into_iter()
        .filter(|row| row.plaintext.contains(marker_prefix))
        .map(|row| row.plaintext)
        .collect::<Vec<_>>();
    // The store API is newest-first scrollback; this task compares conversation
    // order, so normalize to chronological order.
    rows.reverse();
    rows
}

fn count_mark(values: &[String], mark: &str) -> usize {
    values
        .iter()
        .filter(|value| value.matches(mark).count() == 1)
        .count()
}

fn opened_plaintexts(batch: &OpenedNativeOverlayTextBatch, marker_prefix: &str) -> Vec<String> {
    batch
        .messages
        .iter()
        .filter(|message| message.plaintext.contains(marker_prefix))
        .map(|message| message.plaintext.clone())
        .collect()
}

fn serve_request(stream: &mut TcpStream, state: &Arc<Mutex<RelayState>>) {
    let Some((method, path, headers, body)) = read_request(stream) else {
        return;
    };
    let path_without_query = path.split('?').next().unwrap_or(&path);
    // Close without answering. The client cannot tell this apart from a dropped
    // network, which is exactly the condition the queued-send proof needs.
    if method == "POST"
        && path_without_query == "/v1/control-inbox"
        && state.lock().unwrap().control_inbox_unreachable
    {
        return;
    }
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
                Some(row) => json_response(
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
                ),
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
        // D-144: production deploys the LEGACY blob protocol, and B0-01 phase 1
        // deliberately moved the client onto it. This fixture only understood the
        // capability protocol of the UNDEPLOYED worker, so a legacy upload fell
        // through to `bad_blob_metadata` and the whole suite read as a client bug.
        //
        // The shape below is not copied from the client -- it is what the live
        // store at https://ciphers.oslprivacy.com was measured doing:
        //   POST /v1/blob  + X-OSL-TTL-Seconds + X-OSL-Fetch-Token(32 hex) -> 201 {id}
        //   GET  /v1/blob/<id> + the same token                            -> 200
        //   DELETE /v1/blob/<id> + the same token                          -> 204
        //   wrong token -> 403, absent -> 401
        // Matching the deployed server is the point; matching the client would be
        // the very construction that made "12/12 green" a lie.
        ("POST", "/v1/blob")
            if headers.contains_key("x-osl-fetch-token")
                && !headers.contains_key("x-osl-blob-id") =>
        {
            let token = canonical_hex_header(headers.get("x-osl-fetch-token"), 32);
            let ttl_ok = headers
                .get("x-osl-ttl-seconds")
                .and_then(|ttl| ttl.parse::<u64>().ok())
                .is_some_and(|ttl| matches!(ttl, 3600 | 86_400 | 259_200 | 604_800));
            match (token, ttl_ok) {
                (None, _) => json_response(400, json!({ "error": "bad_fetch_token" })),
                (Some(_), false) => json_response(400, json!({ "error": "bad_ttl" })),
                (Some(token), true) => {
                    let mut state = state.lock().unwrap();
                    let id = format!("{:016x}", state.blobs.len() as u64 + 1);
                    let digest = sha256_hex(&token);
                    state.blobs.insert(
                        id.clone(),
                        BlobRow {
                            // Legacy has ONE credential: the same token authorises fetch and
                            // delete. That is D-117, and the fixture must reproduce it rather
                            // than quietly model the safer capability split.
                            fetch_digest: digest.clone(),
                            manage_digest: digest.clone(),
                        },
                    );
                    state.payloads.entry(digest).or_insert(body);
                    json_response(201, json!({ "id": id, "expires_at": now + 3600 }))
                }
            }
        }
        // D-292. This arm used to be served on `POST`, and the fixture had no
        // `PUT` arm at all -- so the double answered a request the shipping
        // Worker refuses (`POST /v1/blob` falls through to `notFound()`, pinned
        // at 404 by `cipher-store-cf/test/routes-and-healthz.test.ts`) and
        // refused the one it serves. The verb below is NOT taken on trust: it
        // is graded against `shipping_blob_upload_method`, which reads the
        // branch reaching the upload handler out of
        // `cipher-store-cf/src/index.ts`. Accepting both verbs would be the
        // same defect restated, so this arm admits one and the other falls
        // through to `not_found` exactly as it does on the Worker.
        //
        // The legacy arm above stays on `POST` deliberately: it models the
        // protocol the LIVE store was measured serving (D-144), which is an
        // older deployment of a different route, not this Worker's source.
        ("PUT", "/v1/blob") => {
            let blob_id = canonical_hex_header(headers.get("x-osl-blob-id"), 32);
            let fetch_digest = canonical_hex_header(headers.get("x-osl-fetch-digest"), 64);
            let ack_digest = canonical_hex_header(headers.get("x-osl-ack-digest"), 64);
            let manage_digest = canonical_hex_header(headers.get("x-osl-manage-digest"), 64);
            let delivery_tag = canonical_hex_header(headers.get("x-osl-delivery-tag"), 32);
            let object_class = headers
                .get("x-osl-object-class")
                .filter(|class| class.as_str() == "single-ack" || class.as_str() == "multi-fetch");
            match (
                blob_id,
                fetch_digest,
                ack_digest,
                manage_digest,
                delivery_tag,
                object_class,
            ) {
                (
                    Some(blob_id),
                    Some(fetch_digest),
                    Some(_),
                    Some(manage_digest),
                    Some(_),
                    Some(_),
                ) => {
                    if ipc::transport_padding::padded_transport_len(body.len()) != Some(body.len())
                    {
                        json_response(400, json!({ "error": "invalid_padding" }))
                    } else {
                        // D-263. This arm used to answer a taken id with
                        // `409 blob_id_collision`. D-255 removed that from the
                        // real Worker -- it was an unauthenticated existence
                        // oracle -- and the double kept it, which is the D-257
                        // pattern: a stand-in that has drifted from the thing
                        // it stands in for is eventually read as evidence
                        // about it.
                        //
                        // What the shipping route does now
                        // (`cipher-store-cf/src/endpoints/blob.ts`): admission
                        // is one `INSERT .. SELECT .. WHERE NOT EXISTS`, so a
                        // taken id changes no row, and the refusal is answered
                        // with exactly the `201 {"id", "expires_at"}` an
                        // unused id gets -- both values echoed from the
                        // request, neither read from the stored row. First
                        // writer keeps the row, and the caller cannot tell the
                        // two cases apart.
                        //
                        // `or_insert` IS the `NOT EXISTS` predicate: a second
                        // upload naming a taken id must not replace what is
                        // stored. The status below is not written down here --
                        // it is read out of the route by
                        // `shipping_upload_refusal_status`, and asserted
                        // against this fixture by
                        // `blob_upload_double_answers_a_taken_id_like_the_shipping_route`.
                        let mut state = state.lock().unwrap();
                        state.blobs.entry(blob_id.clone()).or_insert(BlobRow {
                            fetch_digest: fetch_digest.clone(),
                            manage_digest,
                        });
                        // D-264. The payload is written under the caller's own
                        // fetch digest, in a key space the id does not govern,
                        // and the write is conditional on absence -- the
                        // `onlyIf: { etagDoesNotMatch: "*" }` the shipping
                        // route puts with (`blob.ts`, "conditional on absence,
                        // like the attachment direct upload"). The refusal is
                        // SILENT: not reported and not rolled back, because
                        // either answer would be a new signal about an object
                        // this caller was never told about -- D-255 one key
                        // space over. So `or_insert`, and the response below is
                        // unchanged either way.
                        state.payloads.entry(fetch_digest).or_insert(body);
                        json_response(201, json!({ "id": blob_id, "expires_at": now + 3600 }))
                    }
                }
                _ => json_response(400, json!({ "error": "bad_blob_metadata" })),
            }
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let state = state.lock().unwrap();
            let presented = headers
                .get("x-osl-fetch-cap")
                .or_else(|| headers.get("x-osl-fetch-token"))
                .map(|cap| sha256_hex(cap));
            match state.blobs.get(id) {
                Some(blob) if presented.as_deref() == Some(blob.fetch_digest.as_str()) => {
                    // The row names the R2 key; the bytes come from the digest
                    // key space, never from the row.
                    match state.payloads.get(&blob.fetch_digest) {
                        Some(bytes) => {
                            bytes_response(200, "application/octet-stream", bytes.clone())
                        }
                        None => json_response(404, json!({ "error": "not_found" })),
                    }
                }
                Some(_) => json_response(403, json!({ "error": "fetch_cap_mismatch" })),
                None => json_response(404, json!({ "error": "not_found" })),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let mut state = state.lock().unwrap();
            let presented = headers
                .get("x-osl-manage-cap")
                .or_else(|| headers.get("x-osl-fetch-token"))
                .map(|cap| sha256_hex(cap));
            let allowed = state
                .blobs
                .get(id)
                .is_none_or(|blob| presented.as_deref() == Some(blob.manage_digest.as_str()));
            if allowed {
                // `blob.ts` burns the object by the digest recorded on the ROW
                // it just removed -- never by one the caller named, which would
                // let a fresh id delete another blob's bytes.
                if let Some(row) = state.blobs.remove(id) {
                    state.payloads.remove(&row.fetch_digest);
                }
                bytes_response(204, "application/octet-stream", Vec::new())
            } else {
                json_response(403, json!({ "error": "manage_cap_mismatch" }))
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

pub fn osl_chat_message_survives_a_lost_wrapped_key_response() {
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
    let bob = core(bob_identity.clone(), &relay_url);
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
    set_friend_account_reach_choice(
        &alice_security,
        alice_context.person_id.clone(),
        "osl-chat".to_owned(),
        "osl-main".to_owned(),
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
    set_friend_account_reach_choice(
        &bob_security,
        bob_context.person_id.clone(),
        "osl-chat".to_owned(),
        "osl-main".to_owned(),
        true,
    )
    .unwrap();
    set_scope_security(&bob_security, bob_context.scope.clone(), 3600, true).unwrap();

    let plaintext = "sealed relay fixture: alpha → beta".to_owned();
    TestStorage::activate(&alice_dir);
    // OSL chat has no Discord row, so the carrier flagtext this produces is
    // unused by this path; a default carrier state is the correct fixture.
    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();
    let prepared = prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        plaintext.clone(),
        true,
        None,
    )
    .unwrap();
    assert!(prepared.person_to_person_e2ee);
    assert!(prepared.view_once);
    assert!(prepared.delivered_to_osl_inbox);
    assert_eq!(relay.pending_for(&bob_id), 1);

    // The keyserver completed this first GET, but the response disappeared
    // before Bob could retain or decrypt it. The retry below has to use the
    // actual OSL Chat receive path, not this request result.
    TestStorage::activate(&bob_dir);
    let first_wrapped_key_id = relay.single_wrapped_key_id_for(&alice_id, &bob_id);
    keystore::KeyServerClient::new(&relay_url)
        .unwrap()
        .fetch_wrapped_key(&bob_identity, &first_wrapped_key_id)
        .unwrap();

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

#[test]
fn task_1369_live_two_way_direct_messages_read_exactly_once() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("task1369-alice-copy", &relay_url);
    let bob_dir = storage.account("task1369-bob-copy", &relay_url);

    let alice_identity = keystore::generate_identity("task1369-alice-copy".to_owned());
    let bob_identity = keystore::generate_identity("task1369-bob-copy".to_owned());
pub fn task_1303_direct_send_creates_one_protected_message_from_box_text() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice-task-1303", &relay_url);
    let bob_dir = storage.account("bob-task-1303", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-task-1303".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-task-1303".to_owned());
    let alice_id = alice_identity.user_id.clone();
    let bob_id = bob_identity.user_id.clone();
    relay.register_floor_identity(&alice_identity);
    relay.register_floor_identity(&bob_identity);
    let alice = core(alice_identity, &relay_url);
    let bob = core(bob_identity, &relay_url);
    let bob = core(bob_identity.clone(), &relay_url);
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
        Some("task1369-bob-copy".to_owned()),
        Some("Bob task 1303 fixture".to_owned()),
    )
    .unwrap();
    verify_friend_safety_number(
        &alice,
        &alice_security,
        bob_friend.person_id.clone(),
        bob_friend.safety_number.clone(),
    )
    .unwrap();
    let alice_binding = manual_peer_binding(&alice, bob_friend.person_id.clone()).unwrap();
    let alice_context =
        activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding).unwrap();
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
        Some("task1369-alice-copy".to_owned()),
        Some("Alice task 1303 fixture".to_owned()),
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

    let alice_to_bob = format!("TASK1369-Alice-to-Bob-{:032x}", rand::random::<u128>());
    let bob_to_alice = format!("TASK1369-Bob-to-Alice-{:032x}", rand::random::<u128>());
    assert_ne!(alice_to_bob, bob_to_alice);
    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();

    TestStorage::activate(&alice_dir);
    let alice_prepared = prepare_osl_chat_text(
    let box_text = "task 1303 box text direct send".to_owned();
    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();

    TestStorage::activate(&alice_dir);
    let prepared = prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        alice_to_bob.clone(),
        true,
        None,
    )
    .unwrap();
    assert!(alice_prepared.person_to_person_e2ee);
    assert!(alice_prepared.delivered_to_osl_inbox);

    TestStorage::activate(&bob_dir);
    let bob_prepared = prepare_osl_chat_text(
        &bob,
        &bob_security,
        &bob_broker,
        &ai_carrier,
        bob_to_alice.clone(),
        true,
        None,
    )
    .unwrap();
    assert!(bob_prepared.person_to_person_e2ee);
    assert!(bob_prepared.delivered_to_osl_inbox);

    TestStorage::activate(&bob_dir);
    let bob_opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    let bob_received_count = bob_opened
        .messages
        .iter()
        .filter(|message| message.plaintext == alice_to_bob)
        .count();

    TestStorage::activate(&alice_dir);
    let alice_opened = drain_osl_chat_text(&alice, &alice_security, &alice_broker, true).unwrap();
    let alice_received_count = alice_opened
        .messages
        .iter()
        .filter(|message| message.plaintext == bob_to_alice)
        .count();

    println!(
        "TASK1369 copy=task1369-bob-copy received_from=task1369-alice-copy exact_text=\"{}\" count={} total_messages={}",
        alice_to_bob,
        bob_received_count,
        bob_opened.messages.len()
    );
    println!(
        "TASK1369 copy=task1369-alice-copy received_from=task1369-bob-copy exact_text=\"{}\" count={} total_messages={}",
        bob_to_alice,
        alice_received_count,
        alice_opened.messages.len()
    );

    assert_eq!(
        bob_received_count, 1,
        "Bob copy must read Alice copy's exact marked text once"
    );
    assert_eq!(
        alice_received_count, 1,
        "Alice copy must read Bob copy's exact marked text once"
    );

    drop(relay);
        box_text.clone(),
        true,
    )
    .unwrap();
    let pending_after_send = relay.pending_for(&bob_id);
    let protected_send_count =
        usize::from(prepared.delivered_to_osl_inbox && prepared.person_to_person_e2ee);

    let first_wrapped_key_id = relay.single_wrapped_key_id_for(&alice_id, &bob_id);
    keystore::KeyServerClient::new(&relay_url)
        .unwrap()
        .fetch_wrapped_key(&bob_identity, &first_wrapped_key_id)
        .unwrap();

    TestStorage::activate(&bob_dir);
    let opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    let protected_message_count = opened
        .messages
        .iter()
        .filter(|message| {
            message.context_verified
                && message.person_to_person_e2ee
                && message.plaintext == box_text
        })
        .count();
    let opened_text = opened
        .messages
        .first()
        .map(|message| message.plaintext.as_str())
        .unwrap_or("");

    println!("TASK1303_DIRECT_SEND_ACTION=Enter");
    println!("TASK1303_PROTECTED_SEND_PATH=prepare_osl_chat_text");
    println!("TASK1303_BOX_TEXT={box_text}");
    println!("TASK1303_PROTECTED_SEND_COUNT={protected_send_count}");
    println!("TASK1303_RELAY_PENDING_AFTER_SEND={pending_after_send}");
    println!("TASK1303_OPENED_PROTECTED_MESSAGE_COUNT={protected_message_count}");
    println!("TASK1303_OPENED_TEXT={opened_text}");

    assert_eq!(protected_send_count, 1);
    assert_eq!(pending_after_send, 1);
    assert_eq!(opened.messages.len(), 1);
    assert_eq!(protected_message_count, 1);
    assert_eq!(opened_text, box_text);
}

pub fn task_0130_old_protected_message_record_survives_place_removal() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice-task-0130", &relay_url);
    let bob_dir = storage.account("bob-task-0130", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-task-0130".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-task-0130".to_owned());
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
        Some("Bob task 0130 fixture".to_owned()),
    )
    .unwrap();
    verify_friend_safety_number(
        &alice,
        &alice_security,
        bob_friend.person_id.clone(),
        bob_friend.safety_number.clone(),
    )
    .unwrap();
    let alice_binding = manual_peer_binding(&alice, bob_friend.person_id.clone()).unwrap();
    let alice_context =
        activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding).unwrap();
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
        Some("Alice task 0130 fixture".to_owned()),
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

    let old_text = "task 0130 protected fixture old text".to_owned();
    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();

    TestStorage::activate(&alice_dir);
    let prepared = prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        old_text.clone(),
        true,
    )
    .unwrap();
    let protected_send_count =
        usize::from(prepared.delivered_to_osl_inbox && prepared.person_to_person_e2ee);
    let posted_before = relay.posted_for(&alice_id, &bob_id);
    assert_eq!(
        posted_before.len(),
        1,
        "fixture must save exactly one old row"
    );
    let saved_record = posted_before[0].clone();
    let receipt_before = fs::read(alice_dir.join("hub_native_overlay_receipts.json")).unwrap();
    let wrapped_before = relay.wrapped_keys_for(&alice_id, &bob_id);
    let pending_before = relay.pending_for(&bob_id);

    let remove_result = set_manual_peer_scope_permission(
        &alice,
        &alice_security,
        "osl-chat",
        "osl-main",
        alice_context.person_id.clone(),
        alice_context.scope.clone(),
        false,
    );

    let new_send = prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        "task 0130 should be skipped after place removal".to_owned(),
        true,
    );
    let new_send_error = new_send.as_ref().err().cloned().unwrap_or_default();
    let posted_after = relay.posted_for(&alice_id, &bob_id);
    let receipt_after = fs::read(alice_dir.join("hub_native_overlay_receipts.json")).unwrap();
    let wrapped_after = relay.wrapped_keys_for(&alice_id, &bob_id);
    let pending_after = relay.pending_for(&bob_id);
    let old_record_exact = posted_after.first() == Some(&saved_record);
    let receipt_exact = receipt_after == receipt_before;
    let new_send_skipped = new_send.is_err()
        && posted_after.len() == posted_before.len()
        && wrapped_after == wrapped_before
        && pending_after == pending_before;

    println!("TASK0130_OLD_TEXT={old_text}");
    println!("TASK0130_OLD_MESSAGE_ID={}", prepared.message_id);
    println!("TASK0130_REMOVED_PLACE_SCOPE={}", saved_record.scope_id);
    println!("TASK0130_REMOVE_PLACE_RESULT={}", remove_result.is_ok());
    println!("TASK0130_PROTECTED_SEND_COUNT={protected_send_count}");
    println!(
        "TASK0130_POSTED_RECORD_COUNT_BEFORE={}",
        posted_before.len()
    );
    println!("TASK0130_POSTED_RECORD_COUNT_AFTER={}", posted_after.len());
    println!("TASK0130_WRAPPED_KEY_COUNT_BEFORE={wrapped_before}");
    println!("TASK0130_WRAPPED_KEY_COUNT_AFTER={wrapped_after}");
    println!("TASK0130_PENDING_RECORD_COUNT_BEFORE={pending_before}");
    println!("TASK0130_PENDING_RECORD_COUNT_AFTER={pending_after}");
    println!("TASK0130_SAVED_RECORD_ID={}", saved_record.id);
    println!(
        "TASK0130_SAVED_RECORD_BUNDLE_SHA256={}",
        sha256_hex(&saved_record.bundle_b64)
    );
    println!("TASK0130_OLD_RECORD_EXACT_AFTER_REMOVAL={old_record_exact}");
    println!("TASK0130_RECEIPT_FILE_EXACT_AFTER_REMOVAL={receipt_exact}");
    println!("TASK0130_NEW_SEND_SKIPPED={new_send_skipped}");
    println!("TASK0130_NEW_SEND_ERROR={new_send_error}");

    assert!(prepared.view_once);
    assert_eq!(protected_send_count, 1);
    assert_eq!(remove_result, Ok(()));
    assert_eq!(posted_after, posted_before);
    assert!(old_record_exact);
    assert!(receipt_exact);
    assert_eq!(wrapped_before, 1);
    assert_eq!(wrapped_after, 1);
    assert_eq!(pending_before, 1);
    assert_eq!(pending_after, 1);
    assert!(new_send_skipped);
    assert_eq!(
        new_send_error,
        "Approve encryption for this friend before continuing"
    );
}

/// D-223, both halves, end to end.
///
/// The relay notice lane is taken offline *after* the wrapped key has landed —
/// the one window where an OSL Chat send is neither delivered nor lost. The
/// send must refuse honestly and durably queue the notice; a later receive poll
/// must finish the delivery by itself, and the peer must read the original
/// plaintext.
///
/// This executes the shipping path. Deleting the enqueue in
/// `prepare_peer_inbox_text_with_route_clients`, or the
/// `drain_osl_chat_send_queue` call in `drain_osl_chat_text`, makes it fail.
pub fn osl_chat_queues_a_relay_notice_the_key_server_never_accepted() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice", &relay_url);
    let bob_dir = storage.account("bob", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-queued-e2e".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-queued-e2e".to_owned());
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
        bob_friend.safety_number.clone(),
    )
    .unwrap();
    let alice_binding = manual_peer_binding(&alice, bob_friend.person_id.clone()).unwrap();
    let alice_context =
        activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding).unwrap();
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
    set_friend_account_reach_choice(
        &alice_security,
        alice_context.person_id.clone(),
        "osl-chat".to_owned(),
        "osl-main".to_owned(),
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
    set_friend_account_reach_choice(
        &bob_security,
        bob_context.person_id.clone(),
        "osl-chat".to_owned(),
        "osl-main".to_owned(),
        true,
    )
    .unwrap();
    set_scope_security(&bob_security, bob_context.scope.clone(), 3600, true).unwrap();

    let plaintext = "queued while the relay notice lane was down".to_owned();
    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();

    // ---- the outage ---------------------------------------------------
    TestStorage::activate(&alice_dir);
    relay.set_control_inbox_unreachable(true);
    // `PreparedNativeOverlayText` deliberately has no `Debug`, so this cannot
    // use `expect_err`.
    let refusal = match prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        plaintext.clone(),
        true,
        None,
    ) {
        Ok(_) => panic!("an unreachable relay notice lane cannot report a delivered send"),
        Err(refusal) => refusal,
    };
    assert_eq!(
        refusal, OSL_RELAY_NOTICE_QUEUED,
        "an unreachable key server must be reported as queued, not as lost"
    );

    // Half delivered, exactly: the key is on the server, the notice is not.
    assert_eq!(
        relay.wrapped_keys_for(&alice_id, &bob_id),
        1,
        "the wrapped key must already have landed for this to be the queued window"
    );
    assert_eq!(
        relay.pending_for(&bob_id),
        0,
        "no relay notice may exist while the notice lane is unreachable"
    );

    // The promise is durable, not a flag in memory.
    let queued = osl_privacy_hub::osl_chat_queue::osl_chat_send_queue_at_config_dir()
        .expect("open the durable send queue")
        .pending()
        .expect("read the durable send queue");
    assert_eq!(
        queued.len(),
        1,
        "the undelivered notice must be persisted, not dropped"
    );
    assert!(
        !queued[0].encrypted_envelope.is_empty(),
        "a queued record must carry the sealed notice"
    );

    // ---- reconnect ----------------------------------------------------
    relay.set_control_inbox_unreachable(false);
    TestStorage::activate(&alice_dir);
    // The ordinary receive poll. Nothing here asks for a resend.
    drain_osl_chat_text(&alice, &alice_security, &alice_broker, true)
        .expect("Alice's own receive poll");
    assert_eq!(
        relay.pending_for(&bob_id),
        1,
        "reconnecting must finish the delivery the outage stranded"
    );
    assert!(
        osl_privacy_hub::osl_chat_queue::osl_chat_send_queue_at_config_dir()
            .expect("reopen the durable send queue")
            .pending()
            .expect("read the durable send queue")
            .is_empty(),
        "a delivered record must leave the queue"
    );

    // ---- and it is the real message -----------------------------------
    TestStorage::activate(&bob_dir);
    let opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    assert_eq!(opened.messages.len(), 1, "Bob receives exactly one message");
    assert_eq!(
        opened.messages[0].plaintext, plaintext,
        "the queued message must arrive byte-identical"
    );
    assert!(opened.messages[0].person_to_person_e2ee);

    drop(relay);
}

pub fn task_3572_every_send_failure_is_safe_and_retries_once() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice-task-3572", &relay_url);
    let bob_dir = storage.account("bob-task-3572", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-task-3572".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-task-3572".to_owned());
#[test]
fn task3784_racing_two_osl_chat_sends_keeps_order_and_private_cover_pairs() {
    let relay = RelayServer::start();
    let storage = TestStorage::new();
    let relay_url = relay.base_url();
    let alice_dir = storage.account("alice-race", &relay_url);
    let bob_dir = storage.account("bob-race", &relay_url);

    let alice_identity = keystore::generate_identity("osl-alice-race-two-sends".to_owned());
    let bob_identity = keystore::generate_identity("osl-bob-race-two-sends".to_owned());
    let alice_id = alice_identity.user_id.clone();
    let bob_id = bob_identity.user_id.clone();
    relay.register_floor_identity(&alice_identity);
    relay.register_floor_identity(&bob_identity);
    let alice = core(alice_identity, &relay_url);
    let bob = core(bob_identity, &relay_url);
    let alice_security = HubSecurityState::default();
    let bob_security = HubSecurityState::default();
    let alice_broker = HubBrokerState::default();
    let alice = Arc::new(core(alice_identity.clone(), &relay_url));
    let bob = core(bob_identity.clone(), &relay_url);
    attach_history_store(&alice, &alice_identity, &alice_dir);
    attach_history_store(&bob, &bob_identity, &bob_dir);
    let alice_security = Arc::new(HubSecurityState::default());
    let bob_security = HubSecurityState::default();
    let alice_broker = Arc::new(HubBrokerState::default());
    let bob_broker = HubBrokerState::default();

    let alice_code = export_friend_code(&alice).unwrap();
    let bob_code = export_friend_code(&bob).unwrap();

    TestStorage::activate(&alice_dir);
    let bob_friend = add_friend_code(
        &alice,
        &alice_security,
        bob_code.friend_code,
        Some("Bob task 3572 fixture".to_owned()),
        Some("Bob race fixture".to_owned()),
    )
    .unwrap();
    verify_friend_safety_number(
        &alice,
        &alice_security,
        bob_friend.person_id.clone(),
        bob_friend.safety_number.clone(),
    )
    .unwrap();
    let alice_binding = manual_peer_binding(&alice, bob_friend.person_id.clone()).unwrap();
    let alice_context =
        activate_owned_osl_chat_context(&alice_broker, &alice_id, alice_binding).unwrap();
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
        Some("Alice task 3572 fixture".to_owned()),
        Some("Alice race fixture".to_owned()),
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

    let ai_carrier = osl_privacy_hub::ai_carrier::AiCarrierState::default();
    let mut marked_count = 0usize;
    let control_mark = "TASK3572_MARK_CONTROL";

    println!("TASK3572_MARKED_COUNT_INITIAL={marked_count}");
    assert_eq!(marked_count, 0);

    TestStorage::activate(&alice_dir);
    prepare_osl_chat_text(
        &alice,
        &alice_security,
        &alice_broker,
        &ai_carrier,
        control_mark.to_owned(),
        true,
    )
    .expect("control mark send succeeds");
    TestStorage::activate(&bob_dir);
    let control_opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    let control_delta = control_opened
        .messages
        .iter()
        .filter(|message| message.plaintext == control_mark)
        .count();
    assert_eq!(
        control_delta, 1,
        "receiver must open exactly one control mark"
    );
    marked_count += control_delta;
    println!(
        "TASK3572_CONTROL_MARK text={control_mark} marked_before=0 marked_after={marked_count} delta={control_delta}"
    );
    assert_eq!(marked_count, 1);

    let failures = documented_message_service_send_failures();
    let mut retry_count = 0usize;
    let mut unchanged_failed_drafts = 0usize;
    let mut unchanged_failed_cover_counts = 0usize;
    let mut retry_marks = Vec::new();

    for (index, name) in failures.iter().copied().enumerate() {
        let marked_before_failure = marked_count;
        assert_eq!(
            marked_before_failure, 1,
            "{name} failure must run before any retry is delivered"
        );
        let failed_draft = format!("TASK3572_FAILED_DRAFT_{index}_{name}");
        let failed_cover_count_before = relay.posted_for(&alice_id, &bob_id).len();
        let failure = retry_available_for_message_service_send_failure_before_cover_preparation(
            name,
            &failed_draft,
            failed_cover_count_before,
        )
        .unwrap_or_else(|error| panic!("listed failure {name} must be forceable: {error}"));
        assert_eq!(
            failure.name, name,
            "{name} must identify the forced failure"
        );
        assert_eq!(
            failure.local_result,
            osl_privacy_hub::broker::MESSAGE_SERVICE_SEND_RETRYABLE_LOCAL_RESULT,
            "{name} must remain failed/retryable before any retry"
        );
        assert!(failure.retryable, "{name} must be retryable");
        assert!(
            failure.private_draft_unchanged,
            "{name} must keep the failed draft exact"
        );
        let failed_draft_after = failed_draft.clone();
        let failed_cover_count_after = relay.posted_for(&alice_id, &bob_id).len();
        let marked_after_failure = marked_count;
        assert_eq!(
            failed_draft_after, failed_draft,
            "{name} must not rewrite the failed draft"
        );
        assert_eq!(
            failed_cover_count_after, failed_cover_count_before,
            "{name} must not add a receiver-visible cover while failed"
        );
        assert_eq!(
            marked_after_failure, marked_before_failure,
            "{name} must not deliver a mark while failed"
        );
        assert_eq!(
            marked_after_failure, 1,
            "{name} must leave the receiver at the control-only mark count"
        );
        unchanged_failed_drafts += usize::from(failed_draft_after == failed_draft);
        unchanged_failed_cover_counts +=
            usize::from(failed_cover_count_after == failed_cover_count_before);
        println!(
            "TASK3572_FAILURE name={name} marked_before={marked_before_failure} marked_after={marked_after_failure} failed_draft_before=\"{failed_draft}\" failed_draft_after=\"{failed_draft_after}\" failed_draft_exact={} failed_cover_count_before={failed_cover_count_before} failed_cover_count_after={failed_cover_count_after}",
            failed_draft_after == failed_draft
        );

        retry_marks.push((name, format!("TASK3572_MARK_RETRY_{index}_{name}")));
    }

    for (name, retry_mark) in retry_marks {
        let marked_before_retry = marked_count;
        TestStorage::activate(&alice_dir);
        prepare_osl_chat_text(
            &alice,
            &alice_security,
            &alice_broker,
            &ai_carrier,
            retry_mark.clone(),
            true,
        )
        .unwrap_or_else(|error| panic!("retry after cleared failure {name} must send: {error}"));
        TestStorage::activate(&bob_dir);
        let retry_opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true)
            .unwrap_or_else(|error| panic!("receiver drain after retry {name} must open: {error}"));
        let retry_delta = retry_opened
            .messages
            .iter()
            .filter(|message| message.plaintext == retry_mark)
            .count();
        assert_eq!(
            retry_delta, 1,
            "{name} retry must deliver exactly one marked message"
        );
        marked_count += retry_delta;
        retry_count += 1;
        println!(
            "TASK3572_RETRY name={name} retry_text={retry_mark} marked_before={marked_before_retry} marked_after={marked_count} delta={retry_delta}"
        );
        assert_eq!(
            marked_count,
            marked_before_retry + 1,
            "{name} retry must raise the receiver mark count by exactly one"
        );
    }

    println!("TASK3572_LISTED_FAILURE_COUNT={}", failures.len());
    println!("TASK3572_RETRY_COUNT={retry_count}");
    println!("TASK3572_FAILED_DRAFT_EXACT_COUNT={unchanged_failed_drafts}");
    println!("TASK3572_FAILED_COVER_COUNT_UNCHANGED_COUNT={unchanged_failed_cover_counts}");
    println!("TASK3572_FINAL_MARKED_COUNT={marked_count}");
    println!(
        "TASK3572_EXPECTED_FINAL_MARKED_COUNT={}",
        failures.len() + 1
    );
    println!("TASK3572_FAILURE_NAMES={}", failures.join(","));

    assert_eq!(failures.len(), 20);
    assert_eq!(retry_count, failures.len());
    assert_eq!(unchanged_failed_drafts, failures.len());
    assert_eq!(unchanged_failed_cover_counts, failures.len());
    assert_eq!(marked_count, failures.len() + 1);
    const MARK_PREFIX: &str = "task3784-race";
    const MARK_A: &str = "task3784-race-A";
    const MARK_B: &str = "task3784-race-B";
    let private_a = format!("{MARK_A}: private alpha text");
    let private_b = format!("{MARK_B}: private beta text");
    let ai_carrier = Arc::new(osl_privacy_hub::ai_carrier::AiCarrierState::default());

    TestStorage::activate(&alice_dir);
    let alice_before = matching_history_plaintexts(&alice, &alice_broker, MARK_PREFIX);
    TestStorage::activate(&bob_dir);
    let bob_before = matching_history_plaintexts(&bob, &bob_broker, MARK_PREFIX);
    println!(
        "TASK3784 before alice_matching={} bob_matching={}",
        alice_before.len(),
        bob_before.len()
    );
    assert_eq!(alice_before.len(), 0);
    assert_eq!(bob_before.len(), 0);

    let start = Arc::new(Barrier::new(3));
    let spawn_send = |label: &'static str, plaintext: String| {
        let alice = Arc::clone(&alice);
        let alice_security = Arc::clone(&alice_security);
        let alice_broker = Arc::clone(&alice_broker);
        let ai_carrier = Arc::clone(&ai_carrier);
        let alice_dir = alice_dir.clone();
        let start = Arc::clone(&start);
        thread::spawn(move || {
            TestStorage::activate(&alice_dir);
            start.wait();
            prepare_osl_chat_text(
                &alice,
                &alice_security,
                &alice_broker,
                &ai_carrier,
                plaintext,
                false,
            )
            .map(|prepared| (label, prepared.message_id))
        })
    };
    let send_a = spawn_send("A", private_a.clone());
    let send_b = spawn_send("B", private_b.clone());
    println!("TASK3784 release=two_sends_at_one_barrier");
    start.wait();
    let sent_a = send_a.join().expect("send A thread did not panic").unwrap();
    let sent_b = send_b.join().expect("send B thread did not panic").unwrap();
    println!(
        "TASK3784 sent {}={} {}={}",
        sent_a.0, sent_a.1, sent_b.0, sent_b.1
    );
    assert_ne!(
        sent_a.1, sent_b.1,
        "the two raced sends must save as distinct messages"
    );
    assert_eq!(relay.pending_for(&bob_id), 2, "both relay notices landed");

    TestStorage::activate(&bob_dir);
    let opened = drain_osl_chat_text(&bob, &bob_security, &bob_broker, true).unwrap();
    let opened_order = opened_plaintexts(&opened, MARK_PREFIX);
    let opened_covers = opened
        .messages
        .iter()
        .filter(|message| message.plaintext.contains(MARK_PREFIX))
        .map(|message| message.cover_pointer.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    println!("TASK3784 opened_order={opened_order:?}");
    println!("TASK3784 opened_cover_count={}", opened_covers.len());
    assert_eq!(opened_order.len(), 2);
    assert_eq!(count_mark(&opened_order, MARK_A), 1);
    assert_eq!(count_mark(&opened_order, MARK_B), 1);
    assert_eq!(opened_covers.len(), 2);
    assert!(
        opened_covers.iter().all(|cover| !cover.is_empty()),
        "every opened message must report its own cover message"
    );
    assert_ne!(
        opened_covers[0], opened_covers[1],
        "the two raced sends must not reuse one cover message"
    );
    for (plaintext, cover) in opened_order.iter().zip(opened_covers.iter()) {
        assert!(
            !cover.contains(MARK_PREFIX),
            "cover text must not contain private task marks"
        );
        if plaintext.contains(MARK_A) {
            assert!(!plaintext.contains(MARK_B));
        } else if plaintext.contains(MARK_B) {
            assert!(!plaintext.contains(MARK_A));
        } else {
            panic!("opened message lost both task marks");
        }
    }

    TestStorage::activate(&alice_dir);
    let alice_after = matching_history_plaintexts(&alice, &alice_broker, MARK_PREFIX);
    TestStorage::activate(&bob_dir);
    let bob_after = matching_history_plaintexts(&bob, &bob_broker, MARK_PREFIX);
    println!("TASK3784 alice_history_order={alice_after:?}");
    println!("TASK3784 bob_history_order={bob_after:?}");
    assert_eq!(alice_after.len(), 2);
    assert_eq!(bob_after.len(), 2);
    assert_eq!(count_mark(&alice_after, MARK_A), 1);
    assert_eq!(count_mark(&alice_after, MARK_B), 1);
    assert_eq!(count_mark(&bob_after, MARK_A), 1);
    assert_eq!(count_mark(&bob_after, MARK_B), 1);
    assert_eq!(
        alice_after, bob_after,
        "both conversation copies must report the same exact order"
    );
    assert_eq!(
        bob_after, opened_order,
        "the saved receiver copy must keep the receive batch order"
    );
    for plaintext in alice_after.iter().chain(bob_after.iter()) {
        if plaintext.contains(MARK_A) {
            assert_eq!(plaintext, &private_a);
        } else if plaintext.contains(MARK_B) {
            assert_eq!(plaintext, &private_b);
        } else {
            panic!("saved message lost its task mark");
        }
    }
    println!(
        "TASK3784 after alice_matching={} bob_matching={} mark_a_alice={} mark_b_alice={} mark_a_bob={} mark_b_bob={} same_order={}",
        alice_after.len(),
        bob_after.len(),
        count_mark(&alice_after, MARK_A),
        count_mark(&alice_after, MARK_B),
        count_mark(&bob_after, MARK_A),
        count_mark(&bob_after, MARK_B),
        alice_after == bob_after
    );

    drop(relay);
}

// =========================================================================
// D-263 — keeping the blob-upload double honest about the route it doubles.
//
// The mock above answered a taken blob id with `409 blob_id_collision` long
// after D-255 removed that response from the real Worker for being an
// unauthenticated existence oracle. Nothing asserted on it, which is why the
// drift survived: an unasserted double is a claim nobody ever grades.
//
// So it is graded here, and the expectation is DERIVED rather than written
// down a second time. A second hand-written copy of the route's behaviour is
// the drift this repository keeps paying for (D-241, D-266, the duplicate
// cover-history), and it is what this defect IS. The two halves below:
//
//   1. `shipping_upload_refusal_status` reads the refusal branch out of
//      `cipher-store-cf/src/endpoints/blob.ts` -- the file that ships -- and
//      returns the status it answers with. Nothing in this file states what
//      that status is.
//   2. the executing test drives the fixture through a real socket and
//      asserts it answers a taken id with THAT status, byte-identically to an
//      unused id, and leaves the stored row alone.
//
// Change the real Worker's answer and (1) changes with it, so (2) goes red
// against the fixture. That is the coupling; a cross-language shared
// definition the Worker itself reads was not available -- `cipher-store-cf/**`
// is another lane's file this session, and a "shared" definition only one side
// reads is a third copy, not a contract.
//
// This scans the WORKER's source, never this file's own text (D-285): the
// strings being searched for are declared here, and a checker shown its own
// declaration always finds what it is looking for.

/// The shipping blob route, comments stripped.
///
/// Stripping matters: `blob.ts` explains the D-255 removal in a comment that
/// names `409 blob_id_collision`, so a raw scan would find the very string
/// whose absence is the property.
fn shipping_blob_route_source() -> String {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cipher-store-cf/src/endpoints/blob.ts");
    let source = fs::read_to_string(&path).unwrap_or_else(|why| {
        panic!("the shipping blob route must be readable at {path:?}: {why}")
    });
    assert!(
        source.contains("async function insertBlobRow(") && source.len() > 2_000,
        "read {} bytes from {path:?} -- this is not the shipping blob route",
        source.len()
    );
    strip_ts_comments(&source)
}

/// Remove `//` line comments and `/* */` block comments, respecting string
/// literals so a `//` inside a SQL string is not treated as a comment.
fn strip_ts_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) => {
                out.push(byte as char);
                if byte == b'\\' && index + 1 < bytes.len() {
                    out.push(bytes[index + 1] as char);
                    index += 2;
                    continue;
                }
                if byte == open {
                    quote = None;
                }
                index += 1;
            }
            None if byte == b'"' || byte == b'\'' || byte == b'`' => {
                quote = Some(byte);
                out.push(byte as char);
                index += 1;
            }
            None if byte == b'/' && bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            None if byte == b'/' && bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            None => {
                out.push(byte as char);
                index += 1;
            }
        }
    }
    out
}

/// The status the shipping route answers an upload whose id was already taken
/// with, read out of the route's refusal branch.
///
/// The branch is `if (!admitted) { .. }`: nothing was written, and the route
/// decides what to say without consulting the id. Its only other exit is the
/// capacity refusal, which is a fact about the store rather than about this
/// blob, so it is identified by its own error code and excluded.
fn shipping_upload_refusal_status() -> u16 {
    let source = shipping_blob_route_source();
    let opener = "if (!admitted) {";
    let start = source
        .find(opener)
        .expect("the shipping route must decide what to answer an unadmitted upload")
        + opener.len();
    let mut depth = 1usize;
    let mut end = start;
    for (offset, byte) in source[start..].bytes().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + offset;
                    break;
                }
            }
            _ => {}
        }
    }
    assert!(end > start, "the refusal branch is unterminated");
    let branch = &source[start..end];

    let mut statuses = Vec::new();
    for line in branch.lines() {
        let line = line.trim();
        if !line.starts_with("return ") {
            continue;
        }
        if line.contains("storage_capacity") {
            // The store is full: true of every upload whatever id it names.
            continue;
        }
        // `json(body, status)` and `error(status, code, message)` are the two
        // shapes this route answers in; read whichever it used rather than
        // assuming the one it happens to use today.
        let status = if let Some(rest) = line.strip_prefix("return error(") {
            rest.split(',')
                .next()
                .and_then(|head| head.trim().parse::<u16>().ok())
        } else {
            line.rsplit(',')
                .next()
                .and_then(|tail| tail.trim().trim_end_matches([')', ';']).parse::<u16>().ok())
        }
        .unwrap_or_else(|| panic!("cannot read the status this refusal answers with: {line}"));
        statuses.push((status, line.to_owned()));
    }
    assert_eq!(
        statuses.len(),
        1,
        "the refusal branch must have exactly one id-independent answer, found {statuses:?}"
    );
    let (status, line) = statuses.into_iter().next().unwrap();
    assert!(
        line.contains("headers.blobId") && line.contains("expiresAt"),
        "the refusal must echo the caller's own id and window, not the stored row: {line}"
    );
    status
}

// ---------------------------------------------------------------------------
// D-292: the METHOD the double serves capability upload on.
//
// Same drift class as D-263, one field over. The double served the upload on
// `POST`; the shipping Worker routes it on `PUT` and nothing else, and the
// double had no `PUT` arm at all -- so it answered a request the Worker
// refuses and refused the one the Worker serves. `cipher_store_client.rs:628`
// records the same mistake being made in the shipping client, and
// `cipher-store-cf/test/routes-and-healthz.test.ts` pins `POST /v1/blob` at
// `404` against the real Worker.
//
// The method is DERIVED, on the D-263 pattern: `shipping_blob_upload_method`
// reads `cipher-store-cf/src/index.ts`, finds the branch that reaches the
// upload handler, and returns the verb that branch admits. **Nothing in this
// file states what that verb is** -- the fixture's own match arm below is the
// double's implementation, which is the thing being graded, not the oracle
// grading it. A double that read the verb out of the Worker at request time
// would not be a double: it could never disagree with the Worker, so it could
// never show that it had drifted. Drift needs two independent statements and a
// comparison between them; this is the comparison.
//
// Every scan here reads the WORKER's source, never this file's own text
// (D-285), and that is asserted rather than asserted-by-comment: this file
// declares synthetic routers of its own, so a reader shown its own text finds
// routes -- and the shipping derivation must not find those.
// ---------------------------------------------------------------------------

/// The shipping Worker's routing table, comments stripped.
///
/// Stripping is load-bearing twice over. The entry point opens with a prose
/// restatement of its own routes (`///   <VERB>    /v1/blob ...`), which is a
/// second hand-written copy inside the Worker and can go stale exactly as this
/// fixture did; and a route that has been commented out is not a route. Both
/// are asserted in `the_upload_method_derivation_reads_the_worker_not_itself`.
fn shipping_router_source() -> String {
    strip_ts_comments(&shipping_router_raw())
}

fn shipping_router_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cipher-store-cf/src/index.ts")
}

fn shipping_router_raw() -> String {
    let path = shipping_router_path();
    let source = fs::read_to_string(&path).unwrap_or_else(|why| {
        panic!("the shipping Worker entry point must be readable at {path:?}: {why}")
    });
    assert!(
        source.contains("export default {")
            && source.contains("return notFound();")
            && source.len() > 8_000,
        "read {} bytes from {path:?} -- this is not the shipping Worker entry point",
        source.len()
    );
    source
}

/// Index of the delimiter that closes the one already open at `start`.
fn matching_delimiter(source: &str, start: usize, open: u8, close: u8) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, byte) in source[start..].bytes().enumerate() {
        if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Some(start + offset);
            }
        }
    }
    None
}

/// Every string literal that follows `marker` in `haystack`.
fn quoted_after(haystack: &str, marker: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut cursor = 0usize;
    while let Some(hit) = haystack[cursor..].find(marker) {
        let from = cursor + hit + marker.len();
        cursor = from;
        match haystack[from..].find('"') {
            Some(end) => found.push(haystack[from..from + end].to_owned()),
            None => break,
        }
    }
    found
}

/// Every routing branch that reaches the blob upload handler, as
/// `(path it matches, methods it admits)`.
///
/// Anchored on the **handler**, not on a path spelling: a branch is a candidate
/// because its body calls the upload handler, and only then is its condition
/// read for the path and the verbs. Pure in its input, so it can be starved on
/// synthetic routers -- it must return what it was shown and never a verb it
/// knows.
fn blob_upload_routes_in(router: &str) -> Vec<(String, Vec<String>)> {
    const OPENER: &str = "if (";
    const HANDLER: &str = "handleUpload(";
    let mut routes = Vec::new();
    let mut cursor = 0usize;
    while let Some(hit) = router[cursor..].find(OPENER) {
        let condition_start = cursor + hit + OPENER.len();
        cursor = condition_start;
        let Some(condition_end) = matching_delimiter(router, condition_start, b'(', b')') else {
            continue;
        };
        let condition = &router[condition_start..condition_end];
        if !condition.contains("path === \"") {
            continue;
        }
        // The branch must open a block: `) {`. Anything else (`) return x;`)
        // is a guard clause, not a route.
        let Some(gap) = router[condition_end + 1..].find('{') else {
            continue;
        };
        if !router[condition_end + 1..condition_end + 1 + gap]
            .trim()
            .is_empty()
        {
            continue;
        }
        let body_start = condition_end + gap + 2;
        let Some(body_end) = matching_delimiter(router, body_start, b'{', b'}') else {
            continue;
        };
        if !router[body_start..body_end].contains(HANDLER) {
            continue;
        }
        let mut paths = quoted_after(condition, "path === \"");
        assert_eq!(
            paths.len(),
            1,
            "an upload branch must constrain exactly one path: {condition}"
        );
        routes.push((
            paths.remove(0),
            quoted_after(condition, "request.method === \""),
        ));
    }
    routes
}

/// The one HTTP method the shipping router admits on the capability upload.
fn shipping_blob_upload_method() -> String {
    let branches = blob_upload_routes_in(&shipping_router_source());
    assert_eq!(
        branches.len(),
        1,
        "the shipping router must reach the upload handler from exactly one branch, found {branches:?}"
    );
    let (path, mut methods) = branches.into_iter().next().unwrap();
    assert_eq!(
        path, "/v1/blob",
        "the capability upload branch must be the one on /v1/blob"
    );
    assert_eq!(
        methods.len(),
        1,
        "the upload branch must admit exactly one method, found {methods:?} -- \
         a branch admitting none accepts every verb and a double cannot mirror that"
    );
    methods.remove(0)
}

/// The expression the shipping upload route keys the payload object under,
/// read out of the route's own `putByDigest` call.
///
/// This is what makes D-264 a defect at all: the key is a header the CALLER
/// supplies and is not the blob id, so winning the row is not what protects
/// the bytes. Derived rather than assumed, so that a Worker which re-keyed R2
/// by the id would make the test below stop claiming to model something the
/// route still permits.
fn shipping_payload_key_expression() -> String {
    let source = shipping_blob_route_source();
    const CALL: &str = "putByDigest(";
    // The route explains this write in prose that also names the call, so the
    // stripped source is the only one worth counting.
    assert_eq!(
        source.matches(CALL).count(),
        1,
        "the shipping upload route must have exactly one payload write"
    );
    let start = source.find(CALL).expect("the route writes the payload") + CALL.len();
    let end = source[start..]
        .find(',')
        .expect("putByDigest takes a key and the bytes");
    source[start..start + end].trim().to_owned()
}

/// One raw HTTP exchange against the fixture, so the double is graded through
/// the same socket the client uses rather than by calling its handler.
fn fixture_request(
    base_url: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>) {
    let address = base_url.trim_start_matches("http://");
    let mut stream = TcpStream::connect(address).expect("connect to the relay fixture");
    let mut request = format!("{method} {path} HTTP/1.1\r\nhost: {address}\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("content-length: {}\r\n\r\n", body.len()));
    let mut wire = request.into_bytes();
    wire.extend_from_slice(body);
    stream.write_all(&wire).expect("write the fixture request");
    stream.flush().expect("flush the fixture request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("read the fixture response");
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("the fixture answers with framed headers");
    let head = String::from_utf8_lossy(&response[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .expect("the fixture answers with a status line");
    (status, response[split + 4..].to_vec())
}

fn upload_headers(blob_id: &str, fetch_cap: &str) -> Vec<(String, String)> {
    vec![
        ("x-osl-blob-id".to_owned(), blob_id.to_owned()),
        ("x-osl-fetch-digest".to_owned(), sha256_hex(fetch_cap)),
        ("x-osl-ack-digest".to_owned(), sha256_hex("ack-cap")),
        ("x-osl-manage-digest".to_owned(), sha256_hex("manage-cap")),
        ("x-osl-delivery-tag".to_owned(), "9".repeat(32)),
        ("x-osl-object-class".to_owned(), "single-ack".to_owned()),
        ("x-osl-ttl-seconds".to_owned(), "3600".to_owned()),
    ]
}

#[test]
fn blob_upload_double_answers_a_taken_id_like_the_shipping_route() {
    // Derived from the route, not restated here. If the Worker starts
    // answering a taken id differently, this value moves and the fixture --
    // which does not know about it -- goes red.
    let refusal_status = shipping_upload_refusal_status();
    // D-292: and on the method the shipping router admits, not one this file
    // chose. Grading the double on a verb the Worker refuses would make every
    // conclusion below a conclusion about a Worker that does not exist.
    let method = shipping_blob_upload_method();

    let relay = RelayServer::start();
    let base = relay.base_url();

    let taken_id = "a".repeat(32);
    let unused_id = "b".repeat(32);
    let first_body =
        ipc::transport_padding::pad_transport_object(b"first-writer-payload".to_vec()).unwrap();
    let second_body =
        ipc::transport_padding::pad_transport_object(b"second-writer-payload".to_vec()).unwrap();

    let upload = |id: &str, cap: &str, body: &[u8]| {
        let owned = upload_headers(id, cap);
        let borrowed: Vec<(&str, &str)> = owned
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        fixture_request(&base, &method, "/v1/blob", &borrowed, body)
    };

    // The gate must be reachable: an unused id is admitted, so nothing below
    // can pass for the trivial reason that every upload was refused.
    let (first_status, first_response) = upload(&taken_id, "fetch-cap-one", &first_body);
    assert_eq!(first_status, 201, "an unused id must be admitted");

    // The same id again, with different bytes and a different fetch
    // capability: the answer may not differ from the one an unused id gets.
    let (second_status, second_response) = upload(&taken_id, "fetch-cap-two", &second_body);
    let (control_status, control_response) = upload(&unused_id, "fetch-cap-three", &second_body);

    assert_eq!(
        second_status, refusal_status,
        "a taken id must be answered with the status the shipping route answers"
    );
    assert_eq!(
        second_status, control_status,
        "a taken id and an unused id must be indistinguishable by status"
    );

    let mask = |bytes: &[u8], id: &str| {
        String::from_utf8_lossy(bytes)
            .replace(id, "<caller-supplied-id>")
            .replace(&sha256_hex("fetch-cap-two"), "<caller-supplied-digest>")
            .replace(&sha256_hex("fetch-cap-three"), "<caller-supplied-digest>")
    };
    assert_eq!(
        mask(&second_response, &taken_id),
        mask(&control_response, &unused_id),
        "the two answers must differ only in what the caller itself supplied"
    );
    assert!(
        mask(&first_response, &taken_id).contains("expires_at"),
        "the admitted answer carries the caller's own window"
    );

    // First writer keeps the row: the second upload replaced neither the
    // stored bytes nor the capability digests it would be fetched under.
    let fetch = |id: &str, cap: &str| {
        fixture_request(
            &base,
            "GET",
            &format!("/v1/blob/{id}"),
            &[("x-osl-fetch-cap", cap)],
            b"",
        )
    };
    let (kept_status, kept_bytes) = fetch(&taken_id, "fetch-cap-one");
    assert_eq!(
        kept_status, 200,
        "the first writer's capability still fetches"
    );
    assert_eq!(
        kept_bytes, first_body,
        "the second upload must not replace the stored payload"
    );
    let (usurper_status, _) = fetch(&taken_id, "fetch-cap-two");
    assert_eq!(
        usurper_status, 403,
        "the second upload must not take over the row's fetch capability"
    );

    drop(relay);
}

#[test]
fn the_shipping_upload_route_has_no_existence_oracle_for_the_double_to_copy() {
    let source = shipping_blob_route_source();

    // Comment-stripping is doing real work here, and it is asserted rather
    // than assumed: the route's prose DOES name the removed response.
    let raw = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cipher-store-cf/src/endpoints/blob.ts"),
    )
    .expect("the shipping blob route is readable");
    assert!(
        raw.contains("blob_id_collision") && !source.contains("blob_id_collision"),
        "the scan must read the route's code, not its explanation of a removal"
    );

    assert!(
        !source.contains("409"),
        "the shipping upload route answers no 409, so the double must not either"
    );
    assert_eq!(
        shipping_upload_refusal_status(),
        201,
        "a taken id is answered exactly as an unused one is"
    );
}

/// D-292. The double must serve the capability upload on the verb the shipping
/// router admits, and must refuse the verbs it does not.
///
/// Both halves matter. Serving only the right verb is not enough if every
/// other verb is served too -- that is the drift restated, since the double
/// would still answer a request the Worker refuses.
#[test]
fn the_double_serves_capability_upload_on_the_method_the_shipping_router_admits() {
    let admitted = shipping_blob_upload_method();

    let relay = RelayServer::start();
    let base = relay.base_url();
    let body = ipc::transport_padding::pad_transport_object(b"upload-body".to_vec()).unwrap();

    // The fixture's own answer for a route it does not have, read off the
    // fixture instead of written down here.
    let (not_found, _) = fixture_request(&base, "GET", "/v1/no-such-route", &[], b"");

    let upload = |method: &str, id: &str| {
        let owned = upload_headers(id, "fetch-cap");
        let borrowed: Vec<(&str, &str)> = owned
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        fixture_request(&base, method, "/v1/blob", &borrowed, &body).0
    };

    // Positive control first: the admitted verb is genuinely served, so
    // nothing below can pass because the double refuses everything.
    assert_eq!(
        upload(&admitted, &"a".repeat(32)),
        201,
        "the double must serve the capability upload on the verb the shipping router admits"
    );

    // `HEAD` is left out on purpose: this fixture answers it with a body, and
    // that is a separate defect from the one being fixed here.
    for refused in ["POST", "PATCH", "GET", "DELETE", "OPTIONS", "BREW"] {
        if refused == admitted {
            continue;
        }
        let status = upload(refused, &sha256_hex(refused)[..32]);
        assert_ne!(
            status, 201,
            "{refused} /v1/blob is not routed by the shipping Worker, so the double must not \
             answer it like an upload"
        );
        assert_eq!(
            status, not_found,
            "{refused} /v1/blob must fall through to the same answer as any unrouted request"
        );
    }

    // ... and it really did store something under the admitted verb, so the
    // positive control above is not a bare status either.
    let (fetched, bytes) = fixture_request(
        &base,
        "GET",
        &format!("/v1/blob/{}", "a".repeat(32)),
        &[("x-osl-fetch-cap", "fetch-cap")],
        b"",
    );
    assert_eq!(fetched, 200, "the admitted upload stored a fetchable row");
    assert_eq!(bytes, body, "and stored the bytes it was given");

    drop(relay);
}

/// D-264, the half the previous pass could not express.
///
/// `or_insert` on the row already reproduced "a taken id leaves the row it
/// collided with as it found it". The other half is a caller with a genuinely
/// **fresh** id naming someone else's fetch digest -- reachable only because
/// the payload key space is not the id space, which is why the fixture now has
/// two of them.
#[test]
fn a_fresh_id_naming_another_blobs_fetch_digest_cannot_replace_its_payload() {
    let method = shipping_blob_upload_method();
    let key = shipping_payload_key_expression();
    assert!(
        key.starts_with("headers."),
        "the payload key must come from the caller's own headers for this attack to exist: {key}"
    );
    assert_ne!(
        key, "headers.blobId",
        "if the route keyed the payload by the id it just won, there would be nothing to model"
    );

    let relay = RelayServer::start();
    let base = relay.base_url();
    let victim_body =
        ipc::transport_padding::pad_transport_object(b"victim-ciphertext".to_vec()).unwrap();
    let attacker_body =
        ipc::transport_padding::pad_transport_object(b"attacker-ciphertext".to_vec()).unwrap();
    assert_ne!(
        victim_body, attacker_body,
        "the two payloads must be distinguishable or nothing below means anything"
    );

    let upload = |id: &str, cap: &str, body: &[u8]| {
        let owned = upload_headers(id, cap);
        let borrowed: Vec<(&str, &str)> = owned
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        fixture_request(&base, &method, "/v1/blob", &borrowed, body).0
    };
    let fetch = |id: &str, cap: &str| {
        fixture_request(
            &base,
            "GET",
            &format!("/v1/blob/{id}"),
            &[("x-osl-fetch-cap", cap)],
            b"",
        )
    };

    let victim_id = "c".repeat(32);
    let attacker_id = "d".repeat(32);
    let free_id = "e".repeat(32);

    assert_eq!(upload(&victim_id, "victim-fetch-cap", &victim_body), 201);

    // A genuinely unused id -- so D1 admission is demonstrably not what stops
    // this -- naming the victim's fetch digest. The row IS written and the
    // answer IS the ordinary admitted one: the refusal is silent.
    assert_eq!(
        upload(&attacker_id, "victim-fetch-cap", &attacker_body),
        201,
        "the attacker's fresh id is admitted, so winning the row is not the guard"
    );

    assert_eq!(
        fetch(&victim_id, "victim-fetch-cap"),
        (200, victim_body.clone()),
        "the victim's ciphertext must survive an upload under a fresh id naming its digest"
    );
    assert_eq!(
        fetch(&attacker_id, "victim-fetch-cap"),
        (200, victim_body.clone()),
        "the attacker's own row resolves to the bytes already under that key, not to its own"
    );

    // Starve the gate: a free key still receives the caller's bytes, so
    // "nothing was overwritten" cannot pass by refusing every write.
    assert_eq!(upload(&free_id, "free-fetch-cap", &attacker_body), 201);
    assert_eq!(
        fetch(&free_id, "free-fetch-cap"),
        (200, attacker_body.clone()),
        "an unused payload key must accept the caller's bytes"
    );

    drop(relay);
}

/// D-285 closure for the method derivation: a checker that reads the file
/// declaring what it checks always finds what it is looking for.
///
/// This file declares synthetic routers a few lines below. They exist so the
/// reader can be starved and inverted in-suite rather than only under a
/// mutation run -- and their presence is what makes the last assertion here a
/// real one: a derivation pointed at this file WOULD find routes.
#[test]
fn the_upload_method_derivation_reads_the_worker_not_itself() {
    // 1. The reader returns what it was shown, not a verb it knows.
    const DECOY: &str = r#"if (path === "/v1/blob" && request.method === "TRAP-VERB") { return handleUpload(request, env); }"#;
    assert_eq!(
        blob_upload_routes_in(DECOY),
        vec![("/v1/blob".to_owned(), vec!["TRAP-VERB".to_owned()])],
        "the reader must report the verb in front of it"
    );

    // 2. It cannot manufacture a route that is not there: a router that never
    //    reaches the upload handler yields nothing, so an empty result is a
    //    failure of the derivation rather than a passing answer.
    assert!(
        blob_upload_routes_in(r#"if (path === "/v1/healthz") { return handleHealthz(env); }"#)
            .is_empty(),
        "a router with no upload branch must yield no upload route"
    );
    assert!(
        blob_upload_routes_in("").is_empty(),
        "an empty router must yield no upload route"
    );

    // 3. A guard clause is not a route: `if (...) return x;` opens no block,
    //    and the next block along must not be read as its body.
    assert!(
        blob_upload_routes_in(
            "if (path === \"/v1/blob\") return notFound();\nif (ok) { return handleUpload(r, e); }"
        )
        .is_empty(),
        "a conditional with no block must not adopt the following block"
    );

    // 4. A commented-out route is not a route -- and this is what stripping
    //    has to remove, demonstrated in both directions rather than assumed.
    const COMMENTED: &str = concat!(
        "// if (path === \"/v1/blob\" && request.method === \"BREW\") { return handleUpload(r, e); }\n",
        "if (path === \"/v1/blob\" && request.method === \"TRAP-VERB\") { return handleUpload(r, e); }\n",
    );
    assert!(
        blob_upload_routes_in(COMMENTED)
            .iter()
            .any(|(_, methods)| methods.contains(&"BREW".to_owned())),
        "unstripped, the commented-out route IS visible -- which is the reason for stripping"
    );
    assert_eq!(
        blob_upload_routes_in(&strip_ts_comments(COMMENTED)),
        vec![("/v1/blob".to_owned(), vec!["TRAP-VERB".to_owned()])],
        "stripped, only the live route survives"
    );

    // 5. The Worker's own prose routing table is a second hand-written copy
    //    inside the Worker, and is stripped before anything is read. Asserted
    //    without naming a verb, so this assertion cannot become the answer.
    let raw = shipping_router_raw();
    let prose = raw
        .lines()
        .find(|line| line.trim_start().starts_with("///") && line.contains("/v1/blob"))
        .expect("the entry point restates its routes in prose")
        .trim()
        .to_owned();
    assert!(
        !shipping_router_source().contains(&prose),
        "the prose routing table must be stripped before the router is read: {prose}"
    );

    // 6. D-285 itself. Point the reader at this very file and it finds the
    //    synthetic routes declared above -- so the derivation is only worth
    //    anything because it is pointed somewhere else, and what it finds
    //    there is not among them.
    let own =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sealed_relay_e2e.rs"))
            .expect("this test file must be readable");
    let self_read: Vec<String> = blob_upload_routes_in(&own)
        .into_iter()
        .flat_map(|(_, methods)| methods)
        .collect();
    assert!(
        self_read.contains(&"TRAP-VERB".to_owned()),
        "a reader satisfied by this file's own declarations would find TRAP-VERB"
    );
    let shipping = shipping_blob_upload_method();
    assert!(
        !self_read.contains(&shipping),
        "this file must not state the verb the derivation is supposed to discover, \
         and the derivation must not be reading this file: found {self_read:?}"
    );
}
