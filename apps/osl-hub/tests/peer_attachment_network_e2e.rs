//! Headless verification of the **encrypted attachment network leg**: the
//! `/v1/attachment*` cipher-store surface that had no test, no fake and no
//! recorded run before this file existed.
//!
//! What is driven here is the real product path, in the real order that
//! `apps/osl-hub/src/native_attachment_transport.rs` drives it:
//!
//! ```text
//! broker::begin_osl_chat_attachment
//!   -> peer_attachment_io::encrypt_file            (16 KiB streaming)
//!   -> peer_attachment_io::sha256_file
//!   -> CipherStoreClient::upload_attachment_file   (direct AND multipart)
//!   -> security::record_peer_attachment_burn_capability
//!   -> broker::deliver_osl_chat_attachment         (notice -> control inbox)
//!   -> broker::list_osl_chat_attachments / take_osl_chat_attachment
//!   -> CipherStoreClient::fetch_attachment_to_writer
//!   -> size + SHA-256 verify
//!   -> peer_attachment_io::decrypt_file / decrypt_file_to_memory
//!   -> broker::commit_osl_chat_attachment_open
//! ```
//!
//! Two things are deliberately NOT driven, because they are unreachable from
//! an integration test rather than untested by choice:
//!
//! * `native_attachment_transport::select_encrypt_upload_deliver` /
//!   `open_pending_inner` are modules of `src/main.rs`, not of the library
//!   (`src/lib.rs` does not declare them), and take a `tauri::AppHandle`. The
//!   sequence above is therefore re-driven step by step against the same
//!   library functions that binary calls.
//! * `native_image_viewer::prepare` is likewise a `main.rs` module and is
//!   Windows + Tauri only. The image branch here stops at
//!   `decrypt_file_to_memory`, which is the last library-reachable step.
//!
//! ## Fidelity to the real worker
//!
//! The fixture mirrors `cipher-store-cf/src/endpoints/attachment.ts` and the
//! dispatch order of `cipher-store-cf/src/index.ts`, including the parts that
//! make it *less* convenient to test against: the capability is compared as a
//! SHA-256 digest, expiry is checked **before** the capability so a missing row
//! answers 404 and never 403, part lengths must equal the exact declared
//! length for their index, TTLs come from a four-value allowlist, and the
//! aggregate row/byte quota and per-bucket rate-limit budgets are enforced. A
//! fixture more permissive than production produces false confidence; an
//! earlier fixture in this repo returned every inbox row while the real worker
//! caps at 64, and that hid a starvation bug.
//!
//! ## Discipline
//!
//! No plaintext, draft, key material or conversation content is ever printed,
//! logged, hashed for display or persisted by this test, including on failure.
//! Content equality is asserted with `assert!(a == b, "<static message>")`
//! rather than `assert_eq!`, because `assert_eq!` prints both operands when it
//! fails, and large-file equality is streamed rather than held in memory.

#![cfg(feature = "core")]

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ATTACHMENT_MULTIPART_MAX_PARTS,
    ATTACHMENT_MULTIPART_PART_BYTES, FETCH_TOKEN_BYTES, MAX_SEALED_ATTACHMENT_BYTES,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "peer-attachment-network-fixture-password";

/// `MAX_DRAIN_ROWS` in `keyserver-cf/src/endpoints/control-inbox.ts`.
const MAX_DRAIN_ROWS: usize = 64;

// ---------------------------------------------------------------------------
// Worker limits, mirrored from cipher-store-cf/src/lib/attachment-limits.ts.
// These are duplicated on purpose: if the worker moves a bound and this file
// is not updated, the mirrored assertions below fail loudly instead of the
// fixture silently becoming more permissive than production.
// ---------------------------------------------------------------------------

const MAX_DIRECT_ATTACHMENT_BYTES: u64 = 26 * 1024 * 1024;
const WORKER_MAX_SEALED_ATTACHMENT_BYTES: u64 = 513 * 1024 * 1024;
const MAX_ATTACHMENT_PART_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ATTACHMENT_PARTS: u32 = 65;
const MAX_LIVE_ATTACHMENT_ROWS: usize = 512;
const MAX_LIVE_ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// `BUDGETS` in `cipher-store-cf/src/lib/rate-limit.ts`, per IP per hour.
const ATTACHMENT_UPLOAD_BUDGET: u32 = 140;
const ATTACHMENT_FETCH_BUDGET: u32 = 120;
const ATTACHMENT_DELETE_BUDGET: u32 = 60;

/// `keystore::set_base_dir_override`, `keystore::set_active_account_dir` and
/// the main-password file key are process-wide. Every test in this binary that
/// drives an identity must hold this for its whole critical section.
fn fixture_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Loopback cipher-store + key-server fixture state.
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ObjectState {
    Uploading,
    Ready,
}

/// Mirrors one `attachment_objects` row plus the R2 object it points at.
#[derive(Clone)]
struct AttachmentRow {
    size_bytes: u64,
    expires_at: i64,
    capability_digest_hex: String,
    state: ObjectState,
    upload_id: Option<String>,
    /// part_number -> (size_bytes, etag). `etag == None` mirrors a reserved
    /// but not-yet-uploaded part.
    parts: BTreeMap<u32, (u64, Option<String>)>,
    part_bodies: BTreeMap<u32, Vec<u8>>,
    /// The R2 object, present once a direct upload or a completion stored it.
    body: Option<Vec<u8>>,
}

/// Per-object hostile-store faults. These reproduce a store that answers but
/// answers wrongly; they are not worker behaviours and are labelled as such at
/// every use site.
#[derive(Clone, Copy, Default)]
struct ObjectFault {
    corrupt_first_byte: bool,
    truncate_served_by: u64,
}

#[derive(Default)]
struct Counts {
    direct_uploads: u32,
    sessions: u32,
    /// (part_number, byte length) in the order the fixture received them.
    parts: Vec<(u32, u64)>,
    completes: u32,
    fetches: u32,
    deletes: u32,
    rate_limited: u32,
}

#[derive(Default)]
struct RelayState {
    next_id: u64,
    inbox: Vec<InboxRow>,
    posted: Vec<InboxRow>,
    blobs: BTreeMap<String, BlobRow>,
    attachments: BTreeMap<String, AttachmentRow>,
    faults: BTreeMap<String, ObjectFault>,
    /// Global knob: every DELETE /v1/attachment/:id answers 500.
    delete_fails: bool,
    /// Test-boundary witness: once armed, the relay records whether any remote
    /// delete arrived before the client had durably committed and replay-tested
    /// the local view-once open.
    replay_commit_visible: Option<Arc<AtomicBool>>,
    delete_attempted_before_replay_commit: bool,
    /// part_number the fixture rejects with 400 bad_part_length.
    reject_part: Option<u32>,
    counts: Counts,
    rate_limit: BTreeMap<&'static str, u32>,
}

impl RelayState {
    fn next_hex(&mut self, bytes: usize) -> String {
        self.next_id += 1;
        let mut out = format!("{:0width$x}", self.next_id, width = bytes * 2);
        out.truncate(bytes * 2);
        out
    }

    /// `insertObject` in attachment.ts: one conditional write enforcing the
    /// aggregate row and byte quota.
    fn quota_allows(&self, size: u64) -> bool {
        let rows = self.attachments.len();
        let used: u64 = self.attachments.values().map(|row| row.size_bytes).sum();
        rows < MAX_LIVE_ATTACHMENT_ROWS && used <= MAX_LIVE_ATTACHMENT_BYTES.saturating_sub(size)
    }
}

struct RelayServer {
    address: String,
    state: Arc<Mutex<RelayState>>,
    stopping: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RelayServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind attachment fixture");
        listener
            .set_nonblocking(true)
            .expect("make attachment fixture nonblocking");
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

    fn address(&self) -> &str {
        &self.address
    }

    fn with_state<T>(&self, act: impl FnOnce(&mut RelayState) -> T) -> T {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        act(&mut state)
    }

    fn object_present(&self, id: &str) -> bool {
        self.with_state(|state| {
            state
                .attachments
                .get(id)
                .is_some_and(|row| row.body.is_some())
        })
    }

    fn object_len(&self, id: &str) -> Option<u64> {
        self.with_state(|state| {
            state
                .attachments
                .get(id)
                .and_then(|row| row.body.as_ref())
                .map(|body| body.len() as u64)
        })
    }

    fn row_state(&self, id: &str) -> Option<ObjectState> {
        self.with_state(|state| state.attachments.get(id).map(|row| row.state))
    }

    fn set_expiry(&self, id: &str, expires_at: i64) {
        self.with_state(|state| {
            if let Some(row) = state.attachments.get_mut(id) {
                row.expires_at = expires_at;
            }
        });
    }

    fn set_fault(&self, id: &str, fault: ObjectFault) {
        self.with_state(|state| {
            state.faults.insert(id.to_owned(), fault);
        });
    }

    fn clear_fault(&self, id: &str) {
        self.with_state(|state| {
            state.faults.remove(id);
        });
    }

    fn set_delete_fails(&self, fails: bool) {
        self.with_state(|state| state.delete_fails = fails);
    }

    fn observe_replay_commit_before_delete(&self, visible: Arc<AtomicBool>) {
        self.with_state(|state| {
            state.replay_commit_visible = Some(visible);
            state.delete_attempted_before_replay_commit = false;
        });
    }

    fn delete_attempted_before_replay_commit(&self) -> bool {
        self.with_state(|state| state.delete_attempted_before_replay_commit)
    }

    fn set_reject_part(&self, part: Option<u32>) {
        self.with_state(|state| state.reject_part = part);
    }

    fn counts<T>(&self, read: impl FnOnce(&Counts) -> T) -> T {
        self.with_state(|state| read(&state.counts))
    }

    fn reset_counts(&self) {
        self.with_state(|state| state.counts = Counts::default());
    }

    fn pending_for(&self, recipient_id: &str) -> usize {
        self.with_state(|state| {
            state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient_id)
                .count()
        })
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

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for chunk in text.as_bytes().chunks_exact(2) {
        let pair = std::str::from_utf8(chunk).ok()?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(out)
}

fn is_lower_hex(text: &str, length: usize) -> bool {
    text.len() == length
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `capabilityDigestHex` in attachment.ts: SHA-256 over the *decoded* 16
/// capability bytes, not over the hex text.
fn capability_digest_hex(capability_hex: &str) -> Option<String> {
    let bytes = hex_decode(capability_hex)?;
    Some(hex_lower(&Sha256::digest(&bytes)))
}

fn allowed_ttl(raw: Option<&String>) -> Option<u32> {
    match raw.map(String::as_str) {
        Some("3600") => Some(3600),
        Some("86400") => Some(86400),
        Some("259200") => Some(259200),
        Some("604800") => Some(604800),
        _ => None,
    }
}

/// `readCapability`: header must be present and match `^[0-9a-f]{32}$`.
fn read_capability(headers: &BTreeMap<String, String>) -> Option<String> {
    let raw = headers.get("x-osl-fetch-token")?.trim().to_ascii_lowercase();
    is_lower_hex(&raw, 32).then_some(raw)
}

// ---------------------------------------------------------------------------
// HTTP plumbing.
// ---------------------------------------------------------------------------

fn read_request(
    stream: &mut TcpStream,
) -> Option<(String, String, BTreeMap<String, String>, Vec<u8>)> {
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok()?;
    let mut request = Vec::with_capacity(16 * 1024);
    let mut buffer = vec![0u8; 64 * 1024];
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
    let total = header_end.saturating_add(content_length);
    request.reserve(total.saturating_sub(request.len()));
    while request.len() < total {
        let read = stream.read(&mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&buffer[..read]);
    }
    let body = request[header_end..total].to_vec();
    Some((method, path, headers, body))
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

/// Build a response. `declared_length` overrides the `content-length` header so
/// a hostile-store fault can declare one length and deliver another.
fn raw_response(
    status: u16,
    content_type: &str,
    body: Vec<u8>,
    declared_length: Option<u64>,
    no_store: bool,
) -> Vec<u8> {
    let length = declared_length.unwrap_or(body.len() as u64);
    let mut response = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    response.push_str(&format!("content-type: {content_type}\r\n"));
    response.push_str(&format!("content-length: {length}\r\n"));
    if no_store {
        response.push_str("cache-control: no-store\r\n");
    }
    response.push_str("connection: close\r\n\r\n");
    let mut out = response.into_bytes();
    out.extend_from_slice(&body);
    out
}

fn json_response(status: u16, body: Value) -> Vec<u8> {
    raw_response(
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&body).unwrap(),
        None,
        true,
    )
}

fn bytes_response(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    raw_response(status, content_type, body, None, false)
}

/// `error(status, code, message)` in cipher-store-cf/src/lib/http.ts.
fn error_response(status: u16, code: &str, message: &str) -> Vec<u8> {
    json_response(status, json!({ "error": code, "message": message }))
}

fn not_found() -> Vec<u8> {
    error_response(404, "not_found", "no such route or blob")
}

/// One raw HTTP request, for the request shapes `CipherStoreClient` cannot
/// produce (no capability header, malformed capability, bad TTL).
fn raw_request(
    address: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(address).expect("connect to attachment fixture");
    let mut request = format!("{method} {path} HTTP/1.1\r\nhost: fixture\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("content-length: {}\r\n", body.len()));
    request.push_str("connection: close\r\n\r\n");
    let mut wire = request.into_bytes();
    wire.extend_from_slice(body);
    stream.write_all(&wire).expect("write raw fixture request");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("set raw read timeout");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let status = std::str::from_utf8(&response)
        .ok()
        .and_then(|text| text.split_whitespace().nth(1)?.parse::<u16>().ok())
        .unwrap_or(0);
    let start = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .unwrap_or(response.len());
    (status, response[start..].to_vec())
}

// ---------------------------------------------------------------------------
// Attachment endpoints, mirroring cipher-store-cf/src/endpoints/attachment.ts.
// ---------------------------------------------------------------------------

/// `unsignedLength`. `Ok(None)` mirrors an absent header, which the worker
/// treats as "not declared" rather than an error for direct uploads.
fn unsigned_length(raw: Option<&String>, max: u64) -> Result<Option<u64>, Vec<u8>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error_response(
            400,
            "bad_content_length",
            "length must be an unsigned integer",
        ));
    }
    let Ok(length) = raw.parse::<u64>() else {
        return Err(error_response(
            413,
            "too_large",
            &format!("attachment data exceeds {max} bytes"),
        ));
    };
    if length > max {
        return Err(error_response(
            413,
            "too_large",
            &format!("attachment data exceeds {max} bytes"),
        ));
    }
    if length == 0 {
        return Err(error_response(
            400,
            "empty_body",
            "attachment data required",
        ));
    }
    Ok(Some(length))
}

/// `authorizedRow`. Ordering is load-bearing and matches the worker exactly:
/// id shape, then row existence + expiry (404), then capability presence
/// (401), then capability digest (403). A missing row therefore never answers
/// 403, so 403 is only ever reachable for an object that really exists.
fn authorized_row(
    state: &RelayState,
    id: &str,
    headers: &BTreeMap<String, String>,
    now: i64,
) -> Result<AttachmentRow, Vec<u8>> {
    if !is_lower_hex(id, 32) {
        return Err(error_response(
            400,
            "bad_id",
            "id must be 32 lowercase hex chars",
        ));
    }
    let Some(row) = state.attachments.get(id) else {
        return Err(not_found());
    };
    if row.expires_at <= now {
        return Err(not_found());
    }
    let Some(presented) = read_capability(headers) else {
        return Err(error_response(
            401,
            "fetch_token_required",
            "X-OSL-Fetch-Token header required",
        ));
    };
    let presented_digest = capability_digest_hex(&presented).unwrap_or_default();
    if presented_digest != row.capability_digest_hex {
        return Err(error_response(
            403,
            "fetch_token_mismatch",
            "fetch token does not match",
        ));
    }
    Ok(row.clone())
}

/// `handleAttachmentUpload` — POST /v1/attachment.
fn handle_direct_upload(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    body: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    let declared = match unsigned_length(headers.get("content-length"), MAX_DIRECT_ATTACHMENT_BYTES)
    {
        Ok(value) => value,
        Err(response) => return response,
    };
    let Some(ttl) = allowed_ttl(headers.get("x-osl-ttl-seconds")) else {
        return error_response(400, "bad_ttl", "unsupported attachment TTL");
    };
    let Some(capability) = read_capability(headers) else {
        return error_response(400, "bad_fetch_token", "invalid fetch token");
    };
    let size = body.len() as u64;
    if size == 0 {
        return error_response(400, "empty_body", "attachment body required");
    }
    if size > MAX_DIRECT_ATTACHMENT_BYTES {
        return error_response(
            413,
            "too_large",
            &format!("attachment exceeds {MAX_DIRECT_ATTACHMENT_BYTES} bytes"),
        );
    }
    if declared.is_some_and(|declared| declared != size) {
        return error_response(400, "content_length_mismatch", "invalid attachment length");
    }
    if !state.quota_allows(size) {
        return error_response(
            503,
            "storage_capacity",
            "attachment storage is temporarily at capacity",
        );
    }
    let id = state.next_hex(16);
    state.attachments.insert(
        id.clone(),
        AttachmentRow {
            size_bytes: size,
            expires_at: now + i64::from(ttl),
            capability_digest_hex: capability_digest_hex(&capability).unwrap_or_default(),
            state: ObjectState::Ready,
            upload_id: None,
            parts: BTreeMap::new(),
            part_bodies: BTreeMap::new(),
            body: Some(body),
        },
    );
    state.counts.direct_uploads += 1;
    json_response(
        201,
        json!({ "id": id, "expires_at": now + i64::from(ttl), "size_bytes": size }),
    )
}

/// `handleAttachmentSessionCreate` — POST /v1/attachment/session.
fn handle_session_create(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    now: i64,
) -> Vec<u8> {
    let Some(ttl) = allowed_ttl(headers.get("x-osl-ttl-seconds")) else {
        return error_response(400, "bad_ttl", "unsupported attachment TTL");
    };
    let Some(capability) = read_capability(headers) else {
        return error_response(400, "bad_fetch_token", "invalid fetch token");
    };
    let declared = match unsigned_length(
        headers.get("x-osl-size-bytes"),
        WORKER_MAX_SEALED_ATTACHMENT_BYTES,
    ) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error_response(400, "size_required", "X-OSL-Size-Bytes header required")
        }
        Err(response) => return response,
    };
    if !state.quota_allows(declared) {
        return error_response(
            503,
            "storage_capacity",
            "attachment storage is temporarily at capacity",
        );
    }
    let id = state.next_hex(16);
    let upload_id = format!("upload-{id}");
    state.attachments.insert(
        id.clone(),
        AttachmentRow {
            size_bytes: declared,
            expires_at: now + i64::from(ttl),
            capability_digest_hex: capability_digest_hex(&capability).unwrap_or_default(),
            state: ObjectState::Uploading,
            upload_id: Some(upload_id),
            parts: BTreeMap::new(),
            part_bodies: BTreeMap::new(),
            body: None,
        },
    );
    state.counts.sessions += 1;
    json_response(
        201,
        json!({
            "id": id,
            "expires_at": now + i64::from(ttl),
            "size_bytes": declared,
            "max_part_bytes": MAX_ATTACHMENT_PART_BYTES,
            "max_parts": MAX_ATTACHMENT_PARTS,
        }),
    )
}

/// `handleAttachmentPartUpload` — PUT /v1/attachment/:id/part/:n.
fn handle_part_upload(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    id: &str,
    part_number: u32,
    body: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    if part_number < 1 || part_number > MAX_ATTACHMENT_PARTS {
        return error_response(
            400,
            "bad_part",
            &format!("part number must be between 1 and {MAX_ATTACHMENT_PARTS}"),
        );
    }
    let row = match authorized_row(state, id, headers, now) {
        Ok(row) => row,
        Err(response) => return response,
    };
    if row.state != ObjectState::Uploading || row.upload_id.is_none() {
        return error_response(409, "upload_not_open", "attachment upload is not open");
    }
    let expected_parts = row.size_bytes.div_ceil(MAX_ATTACHMENT_PART_BYTES) as u32;
    if part_number > expected_parts {
        return error_response(
            400,
            "bad_part",
            "part number exceeds the declared attachment size",
        );
    }
    let declared = match unsigned_length(headers.get("content-length"), MAX_ATTACHMENT_PART_BYTES) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return error_response(
                411,
                "content_length_required",
                "Content-Length is required for attachment parts",
            )
        }
        Err(response) => return response,
    };
    let expected_length = if part_number < expected_parts {
        MAX_ATTACHMENT_PART_BYTES
    } else {
        row.size_bytes - MAX_ATTACHMENT_PART_BYTES * u64::from(expected_parts - 1)
    };
    if declared != expected_length || state.reject_part == Some(part_number) {
        return error_response(
            400,
            "bad_part_length",
            "part length does not match the declared attachment size",
        );
    }
    // Mirrors the conditional UPSERT: this part's own prior reservation is
    // excluded from the aggregate so a retry replaces rather than accumulates.
    let others: u64 = row
        .parts
        .iter()
        .filter(|(number, _)| **number != part_number)
        .map(|(_, (size, _))| *size)
        .sum();
    if declared + others > row.size_bytes {
        return error_response(
            409,
            "part_exceeds_declared_size",
            "attachment parts exceed the declared size",
        );
    }
    let actual = body.len() as u64;
    if actual == 0 || actual != declared {
        state.attachments.remove(id);
        return error_response(
            400,
            if actual == 0 {
                "empty_body"
            } else {
                "content_length_mismatch"
            },
            "invalid attachment part length",
        );
    }
    if let Some(row) = state.attachments.get_mut(id) {
        row.parts
            .insert(part_number, (actual, Some(format!("etag-{part_number}"))));
        row.part_bodies.insert(part_number, body);
    }
    state.counts.parts.push((part_number, actual));
    json_response(
        201,
        json!({ "part_number": part_number, "size_bytes": actual }),
    )
}

/// `handleAttachmentComplete` — POST /v1/attachment/:id/complete.
fn handle_complete(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    id: &str,
    now: i64,
) -> Vec<u8> {
    let row = match authorized_row(state, id, headers, now) {
        Ok(row) => row,
        Err(response) => return response,
    };
    if row.state == ObjectState::Ready {
        return json_response(
            200,
            json!({ "id": id, "expires_at": row.expires_at, "size_bytes": row.size_bytes }),
        );
    }
    if row.upload_id.is_none() {
        return error_response(409, "upload_not_open", "attachment upload is not open");
    }
    let uploaded: Vec<(u32, u64)> = row
        .parts
        .iter()
        .filter(|(_, (_, etag))| etag.is_some())
        .map(|(number, (size, _))| (*number, *size))
        .collect();
    let total: u64 = uploaded.iter().map(|(_, size)| *size).sum();
    let contiguous = uploaded
        .iter()
        .enumerate()
        .all(|(index, (number, _))| *number == index as u32 + 1);
    if uploaded.is_empty() || total != row.size_bytes || !contiguous {
        return error_response(409, "parts_incomplete", "attachment parts are incomplete");
    }
    let mut assembled = Vec::with_capacity(total as usize);
    for (number, _) in &uploaded {
        if let Some(part) = row.part_bodies.get(number) {
            assembled.extend_from_slice(part);
        }
    }
    if assembled.len() as u64 != row.size_bytes {
        state.attachments.remove(id);
        return error_response(
            500,
            "completed_size_mismatch",
            "completed attachment size did not match",
        );
    }
    if let Some(row) = state.attachments.get_mut(id) {
        row.state = ObjectState::Ready;
        row.upload_id = None;
        row.body = Some(assembled);
        row.parts.clear();
        row.part_bodies.clear();
    }
    state.counts.completes += 1;
    json_response(
        201,
        json!({ "id": id, "expires_at": row.expires_at, "size_bytes": row.size_bytes }),
    )
}

/// `handleAttachmentFetch` — GET /v1/attachment/:id.
fn handle_attachment_fetch(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    id: &str,
    now: i64,
) -> Vec<u8> {
    let row = match authorized_row(state, id, headers, now) {
        Ok(row) => row,
        Err(response) => return response,
    };
    if row.state != ObjectState::Ready {
        return not_found();
    }
    let Some(body) = row.body.clone() else {
        return not_found();
    };
    if body.len() as u64 != row.size_bytes {
        return not_found();
    }
    state.counts.fetches += 1;
    // The worker sets content-length from the D1 row, not from the object.
    let fault = state.faults.get(id).copied().unwrap_or_default();
    let mut served = body;
    if fault.corrupt_first_byte {
        if let Some(byte) = served.first_mut() {
            *byte ^= 0xff;
        }
    }
    if fault.truncate_served_by > 0 {
        let keep = served
            .len()
            .saturating_sub(fault.truncate_served_by as usize);
        served.truncate(keep);
    }
    raw_response(
        200,
        "application/octet-stream",
        served,
        Some(row.size_bytes),
        true,
    )
}

/// `handleAttachmentDelete` — DELETE /v1/attachment/:id. A 404 from
/// `authorizedRow` becomes 204 when the id is well formed (idempotent burn).
fn handle_attachment_delete(
    state: &mut RelayState,
    headers: &BTreeMap<String, String>,
    id: &str,
    now: i64,
) -> Vec<u8> {
    if state
        .replay_commit_visible
        .as_ref()
        .is_some_and(|visible| !visible.load(Ordering::Acquire))
    {
        state.delete_attempted_before_replay_commit = true;
    }
    if state.delete_fails {
        return error_response(500, "internal_error", "server failure");
    }
    match authorized_row(state, id, headers, now) {
        Ok(_) => {
            state.attachments.remove(id);
            state.counts.deletes += 1;
            raw_response(204, "application/octet-stream", Vec::new(), None, false)
        }
        Err(response) => {
            let status = std::str::from_utf8(&response)
                .ok()
                .and_then(|text| text.split_whitespace().nth(1)?.parse::<u16>().ok())
                .unwrap_or(0);
            if status == 404 && is_lower_hex(id, 32) {
                state.counts.deletes += 1;
                return raw_response(204, "application/octet-stream", Vec::new(), None, false);
            }
            response
        }
    }
}

/// `rateLimit`. One loopback client is one IP, so a single counter per bucket
/// is the faithful shape.
fn rate_limit(state: &mut RelayState, bucket: &'static str, budget: u32) -> bool {
    let used = state.rate_limit.entry(bucket).or_insert(0);
    if *used >= budget {
        state.counts.rate_limited += 1;
        return false;
    }
    *used += 1;
    true
}

// ---------------------------------------------------------------------------
// Dispatch, in the same order as cipher-store-cf/src/index.ts.
// ---------------------------------------------------------------------------

fn serve_request(stream: &mut TcpStream, shared: &Arc<Mutex<RelayState>>) {
    let Some((method, target, headers, body)) = read_request(stream) else {
        return;
    };
    let path = target.split('?').next().unwrap_or(&target).to_owned();
    let now = now_secs();
    let mut state = shared.lock().unwrap_or_else(|error| error.into_inner());
    let state = &mut *state;

    let response = if path == "/v1/attachment" && method == "POST" {
        if rate_limit(state, "attachment-upload", ATTACHMENT_UPLOAD_BUDGET) {
            handle_direct_upload(state, &headers, body, now)
        } else {
            error_response(429, "rate_limited", "upload rate limit hit")
        }
    } else if path == "/v1/attachment/session" && method == "POST" {
        if rate_limit(state, "attachment-upload", ATTACHMENT_UPLOAD_BUDGET) {
            handle_session_create(state, &headers, now)
        } else {
            error_response(429, "rate_limited", "upload rate limit hit")
        }
    } else if let Some((id, part_number)) = parse_part_route(&path).filter(|_| method == "PUT") {
        if rate_limit(state, "attachment-upload", ATTACHMENT_UPLOAD_BUDGET) {
            handle_part_upload(state, &headers, &id, part_number, body, now)
        } else {
            error_response(429, "rate_limited", "upload rate limit hit")
        }
    } else if let Some(id) = parse_complete_route(&path).filter(|_| method == "POST") {
        if rate_limit(state, "attachment-upload", ATTACHMENT_UPLOAD_BUDGET) {
            handle_complete(state, &headers, &id, now)
        } else {
            error_response(429, "rate_limited", "upload rate limit hit")
        }
    } else if let Some(id) = parse_attachment_route(&path) {
        match method.as_str() {
            "GET" => {
                if rate_limit(state, "attachment-fetch", ATTACHMENT_FETCH_BUDGET) {
                    handle_attachment_fetch(state, &headers, &id, now)
                } else {
                    error_response(429, "rate_limited", "fetch rate limit hit")
                }
            }
            "DELETE" => {
                if rate_limit(state, "attachment-delete", ATTACHMENT_DELETE_BUDGET) {
                    handle_attachment_delete(state, &headers, &id, now)
                } else {
                    error_response(429, "rate_limited", "delete rate limit hit")
                }
            }
            _ => not_found(),
        }
    } else {
        serve_legacy_request(state, &method, &path, &target, &headers, body, now)
    };
    let _ = stream.write_all(&response);
}

/// `/^\/v1\/attachment\/([0-9a-f]{32})\/part\/(\d+)$/`
fn parse_part_route(path: &str) -> Option<(String, u32)> {
    let rest = path.strip_prefix("/v1/attachment/")?;
    let (id, number) = rest.split_once("/part/")?;
    if !is_lower_hex(id, 32) || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((id.to_owned(), number.parse().ok()?))
}

/// `/^\/v1\/attachment\/([0-9a-f]{32})\/complete$/`
fn parse_complete_route(path: &str) -> Option<String> {
    let rest = path.strip_prefix("/v1/attachment/")?;
    let id = rest.strip_suffix("/complete")?;
    is_lower_hex(id, 32).then(|| id.to_owned())
}

/// `/^\/v1\/attachment\/([0-9a-f]+)$/` — note the worker matches any hex
/// length here and lets `authorizedRow` answer 400 for the wrong length.
fn parse_attachment_route(path: &str) -> Option<String> {
    let id = path.strip_prefix("/v1/attachment/")?;
    if id.is_empty()
        || id.contains('/')
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(id.to_owned())
}

/// The `/v1/blob*` and `/v1/control-inbox*` surface the existing loopback
/// fixtures already implement. The control-inbox cap and ordering are what the
/// notice-delivery half of this test depends on.
fn serve_legacy_request(
    state: &mut RelayState,
    method: &str,
    path: &str,
    target: &str,
    headers: &BTreeMap<String, String>,
    body: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    match (method, path) {
        ("POST", "/v1/blob") => {
            let id = state.next_hex(8);
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
            match state.blobs.get(id) {
                Some(blob) if headers.get("x-osl-fetch-token") == Some(&blob.fetch_token) => {
                    bytes_response(200, "application/octet-stream", blob.bytes.clone())
                }
                Some(_) => error_response(403, "fetch_token_mismatch", "fetch token does not match"),
                None => not_found(),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id = path.trim_start_matches("/v1/blob/");
            let allowed = state
                .blobs
                .get(id)
                .is_none_or(|blob| headers.get("x-osl-fetch-token") == Some(&blob.fetch_token));
            if allowed {
                state.blobs.remove(id);
                raw_response(204, "application/octet-stream", Vec::new(), None, false)
            } else {
                error_response(403, "fetch_token_mismatch", "fetch token does not match")
            }
        }
        ("POST", "/v1/control-inbox") => {
            let value: Value = serde_json::from_slice(&body).expect("valid control-inbox post");
            let id = state.next_hex(16);
            let row = InboxRow {
                id,
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
            let target = url::Url::parse(&format!("http://relay.invalid{target}"))
                .expect("parse control-inbox request target");
            let sender = target
                .query_pairs()
                .find_map(|(key, value)| (key == "sender").then(|| value.into_owned()));
            let items = state
                .inbox
                .iter()
                .filter(|row| row.recipient_id == recipient)
                .filter(|row| {
                    sender
                        .as_deref()
                        .is_none_or(|requested| row.sender_id == requested)
                })
                // `ORDER BY created_at ASC LIMIT MAX_DRAIN_ROWS`, no cursor.
                .take(MAX_DRAIN_ROWS)
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
                        "filtered_sender_id": sender,
                        "filtered_sender_delivery": {
                            "live": items.len(),
                            "retryable": 0,
                            "quarantined": 0,
                            "retired": 0,
                        },
                        "items": items,
                    }),
                ),
                None => json_response(200, json!({ "items": items })),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/control-inbox/") => {
            let id = path.trim_start_matches("/v1/control-inbox/");
            state.inbox.retain(|row| row.id != id);
            raw_response(204, "application/json", Vec::new(), None, false)
        }
        _ => not_found(),
    }
}

// ---------------------------------------------------------------------------
// Isolated storage + two locally generated identities.
// ---------------------------------------------------------------------------

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-peer-attachment-net-{label}-{}-{}",
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

    /// One app-local-data root per side, holding that side's
    /// `peer-attachment-staging` directory.
    fn local_root(&self, name: &str) -> PathBuf {
        let dir = self.root.join(format!("{name}-local"));
        fs::create_dir(&dir).expect("create isolated OSL local data root");
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

fn core(identity: keystore::Identity, relay_url: &str) -> osl_privacy_hub::core_bridge::HubCoreState {
    let core = osl_privacy_hub::core_bridge::HubCoreState::default();
    *core.osl.identity.lock().unwrap() = Some(identity);
    *core.osl.keyserver.lock().unwrap() = Some(keystore::KeyServerClient::new(relay_url).unwrap());
    core
}

/// One side of a protected OSL chat conversation. Keys are generated fresh in
/// the temp root; nothing is registered anywhere.
struct Peer {
    dir: PathBuf,
    local_root: PathBuf,
    identity_id: String,
    core: osl_privacy_hub::core_bridge::HubCoreState,
    security: osl_privacy_hub::security::HubSecurityState,
    broker: osl_privacy_hub::broker::HubBrokerState,
    friend_code: String,
    safety_number: String,
}

impl Peer {
    fn new(storage: &TestStorage, name: &str, relay_url: &str) -> Self {
        let dir = storage.account(name, relay_url);
        let local_root = storage.local_root(name);
        let identity = keystore::generate_identity(format!("osl-{name}-attachment-net"));
        let identity_id = identity.user_id.clone();
        let core = core(identity, relay_url);
        TestStorage::activate(&dir);
        let exported =
            osl_privacy_hub::security::export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            local_root,
            identity_id,
            core,
            security: osl_privacy_hub::security::HubSecurityState::default(),
            broker: osl_privacy_hub::broker::HubBrokerState::default(),
            friend_code: exported.friend_code,
            safety_number: exported.safety_number,
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    /// Add + verify the other side and open an OSL chat context against them,
    /// with decrypted display enabled and a 1 hour scope TTL (3600 is one of
    /// the four TTLs the cipher-store allowlist accepts).
    fn open_context_to(&self, other_code: &str, other_safety: &str) {
        self.activate();
        let friend = osl_privacy_hub::security::add_friend_code(
            &self.core,
            &self.security,
            other_code.to_owned(),
            Some("fixture peer".to_owned()),
        )
        .expect("add friend code");
        osl_privacy_hub::security::verify_friend_safety_number(
            &self.core,
            &self.security,
            friend.person_id.clone(),
            other_safety.to_owned(),
        )
        .expect("verify safety number");
        let binding = osl_privacy_hub::security::manual_peer_binding(&self.core, friend.person_id)
            .expect("manual peer binding");
        let activated = osl_privacy_hub::broker::activate_owned_osl_chat_context(
            &self.broker,
            &self.identity_id,
            binding,
        )
        .expect("activate OSL chat context");
        osl_privacy_hub::security::set_manual_peer_scope_permission(
            &self.core,
            &self.security,
            "osl-chat",
            "osl-main",
            activated.person_id.clone(),
            activated.scope.clone(),
            true,
        )
        .expect("approve manual peer scope");
        osl_privacy_hub::security::set_scope_security(&self.security, activated.scope, 3600, true)
            .expect("enable decrypted display for this scope");
    }
}

/// Both sides, mutually verified, sharing one loopback backend.
fn verified_pair(storage: &TestStorage, relay_url: &str) -> (Peer, Peer) {
    let alice = Peer::new(storage, "alice", relay_url);
    let bob = Peer::new(storage, "bob", relay_url);
    alice.open_context_to(&bob.friend_code, &bob.safety_number);
    bob.open_context_to(&alice.friend_code, &alice.safety_number);
    (alice, bob)
}

// ---------------------------------------------------------------------------
// File helpers. Nothing here prints, hashes for display, or retains content.
// ---------------------------------------------------------------------------

/// A marker embedded in generated plaintext so the filesystem sweep can look
/// for surviving plaintext without ever surfacing file content.
const PLAINTEXT_MARKER: &[u8] = b"OSL-FIXTURE-PLAINTEXT-MARKER-b7e1c40f";

/// Write `len` bytes of deterministic non-uniform filler with the marker
/// embedded near the start, and return the path.
fn write_plaintext_source(path: &Path, len: usize) -> PathBuf {
    assert!(
        len > PLAINTEXT_MARKER.len() * 2,
        "fixture plaintext must be long enough to carry its marker"
    );
    let mut file = BufWriter::new(File::create(path).expect("create fixture plaintext"));
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut written = 0usize;
    let mut block = vec![0u8; 64 * 1024];
    while written < len {
        let take = block.len().min(len - written);
        for slot in block[..take].iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *slot = (state >> 33) as u8;
        }
        if written == 0 {
            let marker_at = 128;
            block[marker_at..marker_at + PLAINTEXT_MARKER.len()]
                .copy_from_slice(PLAINTEXT_MARKER);
        }
        file.write_all(&block[..take])
            .expect("write fixture plaintext");
        written += take;
    }
    file.flush().expect("flush fixture plaintext");
    path.to_owned()
}

/// Write `len` bytes of opaque filler, standing in for already-sealed
/// ciphertext when a specific sealed length is required.
fn write_opaque_file(path: &Path, len: u64) {
    let mut file = BufWriter::new(File::create(path).expect("create opaque fixture file"));
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut written = 0u64;
    let mut block = vec![0u8; 256 * 1024];
    while written < len {
        let take = (block.len() as u64).min(len - written) as usize;
        for slot in block[..take].iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *slot = (state >> 33) as u8;
        }
        file.write_all(&block[..take])
            .expect("write opaque fixture file");
        written += take as u64;
    }
    file.flush().expect("flush opaque fixture file");
}

/// Stream-compare two files. Returns equality only — never the differing
/// bytes, their offset, or any content.
fn files_identical(left: &Path, right: &Path) -> bool {
    let (Ok(left_meta), Ok(right_meta)) = (fs::metadata(left), fs::metadata(right)) else {
        return false;
    };
    if left_meta.len() != right_meta.len() {
        return false;
    }
    let (Ok(mut left_file), Ok(mut right_file)) = (File::open(left), File::open(right)) else {
        return false;
    };
    let mut left_block = vec![0u8; 64 * 1024];
    let mut right_block = vec![0u8; 64 * 1024];
    loop {
        let left_read = match read_full(&mut left_file, &mut left_block) {
            Ok(read) => read,
            Err(()) => return false,
        };
        let right_read = match read_full(&mut right_file, &mut right_block) {
            Ok(read) => read,
            Err(()) => return false,
        };
        if left_read != right_read {
            return false;
        }
        if left_read == 0 {
            return true;
        }
        if left_block[..left_read] != right_block[..right_read] {
            return false;
        }
    }
}

fn read_full(file: &mut File, buffer: &mut [u8]) -> Result<usize, ()> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(_) => return Err(()),
        }
    }
    Ok(filled)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|window| window == needle)
}

/// Walk every regular file under `root` and return the first whose bytes
/// contain `needle`. Only the path is ever surfaced, never the content. Same
/// technique as the sweep in `native_discord_receive_e2e.rs`.
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

/// Every directory OSL itself owns: both account directories and both
/// app-local-data roots. The sender's own input file lives outside these on
/// purpose, so a sweep of them is a statement about OSL's storage only.
fn osl_roots(alice: &Peer, bob: &Peer) -> Vec<PathBuf> {
    vec![
        alice.dir.clone(),
        bob.dir.clone(),
        alice.local_root.clone(),
        bob.local_root.clone(),
    ]
}

/// First OSL-owned file containing the marker, if any. Path only, never
/// content.
fn surviving_plaintext(alice: &Peer, bob: &Peer) -> Option<PathBuf> {
    osl_roots(alice, bob)
        .into_iter()
        .find_map(|root| file_containing(&root, PLAINTEXT_MARKER))
}

/// Count the staging files of one kind that currently exist for a side.
fn staging_files(local_root: &Path, prefix: &str) -> Vec<PathBuf> {
    let staging = local_root.join("peer-attachment-staging");
    let Ok(entries) = fs::read_dir(&staging) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect();
    out.sort();
    out
}

fn fresh_fetch_token() -> [u8; FETCH_TOKEN_BYTES] {
    let mut token = [0u8; FETCH_TOKEN_BYTES];
    token.copy_from_slice(&crypto::random::random_bytes(FETCH_TOKEN_BYTES));
    token
}

fn token_from_hex(text: &str) -> [u8; FETCH_TOKEN_BYTES] {
    let bytes = hex_decode(text).expect("capability is canonical hex");
    let mut token = [0u8; FETCH_TOKEN_BYTES];
    token.copy_from_slice(&bytes);
    token
}

// ---------------------------------------------------------------------------
// The product send / receive sequence, step for step.
// ---------------------------------------------------------------------------

/// What the sender ends up holding after a successful send.
struct SentAttachment {
    object_id: String,
    token: [u8; FETCH_TOKEN_BYTES],
    sealed_size: u64,
    digest_hex: String,
}

/// Mirrors `native_attachment_transport::select_encrypt_upload_deliver_inner`
/// exactly, minus the Tauri file dialog and the surface/tier re-checks that
/// need an `AppHandle`.
fn send_attachment(
    sender: &Peer,
    client: &CipherStoreClient,
    source_path: &Path,
    filename: &str,
    view_once: bool,
) -> SentAttachment {
    sender.activate();
    let mut source = File::open(source_path).expect("open fixture source");
    let plaintext_size = source.metadata().expect("source metadata").len();

    let plan = osl_privacy_hub::broker::begin_osl_chat_attachment(
        &sender.core,
        &sender.broker,
        filename.to_owned(),
        plaintext_size,
        view_once,
    )
    .expect("begin OSL chat attachment");

    let attachment_key = plan.attachment_key;
    let content_id = plan.content_id;
    let burn_scope = plan.burn_scope.clone();
    let ttl = u32::try_from(plan.expires_at.saturating_sub(plan.created_at))
        .expect("scope TTL fits in u32");

    let staged = osl_privacy_hub::peer_attachment_io::encrypt_file(
        &sender.local_root,
        &mut source,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(attachment_key),
        content_id.to_vec(),
        0,
    )
    .expect("stream-encrypt the attachment");

    let (digest, sealed_size) =
        osl_privacy_hub::peer_attachment_io::sha256_file(staged.path()).expect("digest the sealed file");

    let token = fresh_fetch_token();
    let sealed_file = File::open(staged.path()).expect("reopen the sealed file");
    let upload = client
        .upload_attachment_file(sealed_file, ttl, &token)
        .expect("upload the sealed attachment");

    osl_privacy_hub::peer_attachment_io::remove_staged_file(staged)
        .expect("clear sealed staging after upload");

    osl_privacy_hub::security::record_peer_attachment_burn_capability(
        &sender.security,
        burn_scope,
        upload.id_hex.clone(),
        hex_lower(&token),
        upload.expires_at,
    )
    .expect("retain the burn capability");

    let digest_hex = hex_lower(&digest);
    osl_privacy_hub::broker::deliver_osl_chat_attachment(
        &sender.core,
        &sender.broker,
        plan,
        sealed_size,
        digest_hex.clone(),
        upload.id_hex.clone(),
        hex_lower(&token),
    )
    .expect("deliver the attachment notice");

    SentAttachment {
        object_id: upload.id_hex,
        token,
        sealed_size,
        digest_hex,
    }
}

/// Mirrors `open_pending_inner` up to (not including) decryption: stream the
/// ciphertext into an OSL staging file, then check the declared size and the
/// pre-decrypt SHA-256. `Err` is the product's refusal, carrying only the
/// static user-facing string the product would surface.
fn fetch_and_verify(
    receiver: &Peer,
    client: &CipherStoreClient,
    plan: &osl_privacy_hub::broker::NativeOverlayAttachmentOpenPlan,
) -> Result<PathBuf, String> {
    let token = token_from_hex(&plan.fetch_token);
    let (download_path, mut download) =
        osl_privacy_hub::peer_attachment_io::create_download_file(&receiver.local_root)?;
    let fetched = match client.fetch_attachment_to_writer(&plan.object_id, &token, &mut download) {
        Ok(size) => size,
        Err(_) => {
            drop(download);
            let _ = osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path);
            return Err("This private attachment is unavailable or expired".to_owned());
        }
    };
    if fetched != plan.sealed_size || download.sync_all().is_err() {
        drop(download);
        let _ = osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path);
        return Err("This private attachment has an invalid size".to_owned());
    }
    drop(download);
    let (digest, size) = osl_privacy_hub::peer_attachment_io::sha256_file(&download_path)?;
    if size != plan.sealed_size || hex_lower(&digest) != plan.ciphertext_sha256 {
        let _ = osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path);
        return Err("This private attachment failed authentication".to_owned());
    }
    Ok(download_path)
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

/// The whole happy path for a non-image attachment small enough to take the
/// **direct** upload route:
///
/// * the sender's real seal/upload/notice sequence completes;
/// * the fixture recorded a direct upload and no multipart session;
/// * the receiver lists exactly one pending attachment with the sender's
///   metadata, and takes an open plan whose object id, sealed size and digest
///   match what the sender uploaded;
/// * the fetched ciphertext passes the size and SHA-256 checks;
/// * decryption recovers **byte-identical** plaintext;
/// * the uploaded ciphertext never contained the plaintext marker;
/// * a plaintext file does exist at rest after decryption (by design — this is
///   `opened-*.oslatt`), and once the product removes it, a full-tree sweep
///   finds no surviving plaintext anywhere.
#[test]
fn direct_upload_round_trip_recovers_byte_identical_plaintext_and_leaves_no_plaintext_at_rest() {
    let _serial = fixture_lock().lock().unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("direct");
    let relay_url = relay.base_url();
    let (alice, bob) = verified_pair(&storage, &relay_url);
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    let source = write_plaintext_source(&storage.root.join("source.txt"), 200 * 1024);
    let sent = send_attachment(&alice, &client, &source, "fixture-note.txt", false);

    assert!(
        is_lower_hex(&sent.object_id, 32),
        "the cipher-store must assign a 128-bit lowercase-hex attachment id"
    );
    relay.counts(|counts| {
        assert_eq!(counts.direct_uploads, 1);
        assert_eq!(counts.sessions, 0);
        assert!(
            counts.parts.is_empty(),
            "a 200 KiB attachment must not take the multipart route"
        );
    });
    assert!(
        sent.sealed_size <= MAX_DIRECT_ATTACHMENT_BYTES,
        "this case is only meaningful while the sealed size stays under the direct-upload bound"
    );
    assert_eq!(relay.object_len(&sent.object_id), Some(sent.sealed_size));
    assert_eq!(relay.pending_for(&bob.identity_id), 1);

    // The stored ciphertext must not contain the plaintext marker.
    let ciphertext_leaks = relay.with_state(|state| {
        state
            .attachments
            .get(&sent.object_id)
            .and_then(|row| row.body.as_ref())
            .is_some_and(|body| contains_bytes(body, PLAINTEXT_MARKER))
    });
    assert!(
        !ciphertext_leaks,
        "the uploaded attachment body must be opaque ciphertext"
    );

    bob.activate();
    let pending = osl_privacy_hub::broker::list_osl_chat_attachments(&bob.core, &bob.security, &bob.broker)
        .expect("list pending attachments");
    assert_eq!(pending.len(), 1);
    assert!(
        pending[0].original_filename == "fixture-note.txt",
        "the receiver must see the sender's filename"
    );
    assert!(
        pending[0].mime_type == "text/plain",
        "the receiver must see the MIME type derived from the filename"
    );
    assert_eq!(pending[0].plaintext_size, 200 * 1024);
    assert!(!pending[0].view_once);

    let plan = osl_privacy_hub::broker::take_osl_chat_attachment(
        &bob.core,
        &bob.security,
        &bob.broker,
        &pending[0].attachment_id,
    )
    .expect("take the attachment open plan");
    assert!(
        plan.object_id == sent.object_id,
        "the notice must carry the object id the sender uploaded"
    );
    assert_eq!(plan.sealed_size, sent.sealed_size);
    assert!(
        plan.ciphertext_sha256 == sent.digest_hex,
        "the notice must carry the sender's pre-upload ciphertext digest"
    );

    let download_path = fetch_and_verify(&bob, &client, &plan).expect("fetch and verify ciphertext");
    let mut sealed = File::open(&download_path).expect("open the verified ciphertext");
    let opened = osl_privacy_hub::peer_attachment_io::decrypt_file(
        &bob.local_root,
        &mut sealed,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
    )
    .expect("decrypt the attachment");
    drop(sealed);
    osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path)
        .expect("clear the download staging file");
    assert_eq!(opened.plaintext_len(), plan.plaintext_size);

    let opened_path = opened
        .path()
        .expect("the decrypted staging file is owned until it is removed")
        .to_owned();
    assert!(
        files_identical(&source, &opened_path),
        "the recovered plaintext must be byte-identical to the sender's file"
    );

    osl_privacy_hub::broker::commit_osl_chat_attachment_open(
        &bob.core,
        &bob.security,
        &bob.broker,
        &plan,
    )
    .expect("commit the attachment open");
    assert_eq!(relay.pending_for(&bob.identity_id), 0);

    // Documented plaintext-at-rest window: the non-image path decrypts to a
    // real file, and that file is the only plaintext OSL writes.
    let staged_plaintext = staging_files(&bob.local_root, "opened-");
    assert_eq!(staged_plaintext.len(), 1);
    assert!(
        file_containing(&bob.local_root, PLAINTEXT_MARKER).is_some(),
        "the decrypted staging file is expected to exist before the product removes it"
    );

    let unremoved_before = osl_privacy_hub::peer_attachment_io::unremoved_plaintext_files();
    opened
        .remove_now()
        .expect("remove the decrypted staging file");
    assert!(
        surviving_plaintext(&alice, &bob).is_none(),
        "no plaintext may survive anywhere under the OSL roots after cleanup"
    );
    assert!(
        source.is_file(),
        "the sweep must not have disturbed the sender's own input file"
    );
    assert_eq!(
        osl_privacy_hub::peer_attachment_io::unremoved_plaintext_files(),
        unremoved_before,
        "a successful removal must not be counted as a surviving plaintext file"
    );
    assert!(staging_files(&bob.local_root, "download-").is_empty());
    assert!(staging_files(&bob.local_root, "sealed-").is_empty());
    assert!(staging_files(&alice.local_root, "sealed-").is_empty());
}

/// The **multipart** upload route, which only engages above 26 MiB.
///
/// This drives `CipherStoreClient::upload_attachment_file` — the real product
/// function — against an opaque file sized to cross the part boundary, because
/// the sealed size is what selects the route and the client treats the body as
/// opaque either way. Covered here:
///
/// * a rejected part aborts the upload and the client's rollback removes the
///   session, so nothing is orphaned when the rollback succeeds;
/// * when the rollback delete *also* fails, the session row survives — the
///   orphan this test exists to characterise;
/// * the happy path sends exactly the expected part numbers and lengths, with
///   three full 8 MiB parts and a short final part;
/// * completion is accepted and the object fetches back byte-identical, which
///   is what proves the parts were assembled in the right order.
#[test]
fn multipart_upload_crosses_the_part_boundary_and_rolls_back_a_rejected_part() {
    let relay = RelayServer::start();
    let relay_url = relay.base_url();
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    let scratch = std::env::temp_dir().join(format!(
        "osl-attachment-multipart-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&scratch).expect("create multipart scratch dir");

    // 27 MiB: above the 26 MiB direct-upload bound, so the client must switch
    // to a session, and short enough that the final part is a partial one.
    let sealed_len: u64 = 27 * 1024 * 1024;
    let opaque = scratch.join("sealed.bin");
    write_opaque_file(&opaque, sealed_len);
    assert!(
        sealed_len > MAX_DIRECT_ATTACHMENT_BYTES,
        "this case is only meaningful above the direct-upload bound"
    );
    let expected_parts: Vec<(u32, u64)> = vec![
        (1, MAX_ATTACHMENT_PART_BYTES),
        (2, MAX_ATTACHMENT_PART_BYTES),
        (3, MAX_ATTACHMENT_PART_BYTES),
        (4, sealed_len - 3 * MAX_ATTACHMENT_PART_BYTES),
    ];

    // --- a rejected part rolls the whole session back -----------------------
    relay.set_reject_part(Some(1));
    let token = fresh_fetch_token();
    let refused = client.upload_attachment_file(
        File::open(&opaque).expect("open opaque fixture"),
        3600,
        &token,
    );
    assert!(
        refused.is_err(),
        "a part the store rejects must fail the whole upload"
    );
    relay.counts(|counts| {
        assert_eq!(counts.sessions, 1);
        assert_eq!(counts.completes, 0);
        assert_eq!(counts.direct_uploads, 0);
    });
    assert!(
        relay.with_state(|state| state.attachments.is_empty()),
        "the client's rollback must remove the session it opened"
    );

    // --- the same failure with a failing rollback leaves an orphan ----------
    relay.reset_counts();
    relay.set_delete_fails(true);
    let orphan_token = fresh_fetch_token();
    let orphaned = client.upload_attachment_file(
        File::open(&opaque).expect("open opaque fixture"),
        3600,
        &orphan_token,
    );
    assert!(orphaned.is_err());
    let orphan_rows = relay.with_state(|state| {
        state
            .attachments
            .iter()
            .map(|(id, row)| (id.clone(), row.state))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        orphan_rows.len(),
        1,
        "a failed rollback leaves the multipart session behind"
    );
    assert_eq!(orphan_rows[0].1, ObjectState::Uploading);
    // Characterised, not asserted as correct: the client discards this delete
    // failure (`let _ = self.delete_attachment(...)` in
    // crates/ipc/src/cipher_store_client.rs), so nothing retries it. The
    // durable outbox in peer_attachment_io is the mechanism that fixes this,
    // and it is exercised by the view-once test below.
    relay.with_state(|state| state.attachments.clear());
    relay.set_delete_fails(false);
    relay.set_reject_part(None);

    // --- happy path ---------------------------------------------------------
    relay.reset_counts();
    let token = fresh_fetch_token();
    let upload = client
        .upload_attachment_file(
            File::open(&opaque).expect("open opaque fixture"),
            3600,
            &token,
        )
        .expect("multipart upload succeeds");
    assert!(is_lower_hex(&upload.id_hex, 32));

    relay.counts(|counts| {
        assert_eq!(counts.sessions, 1);
        assert_eq!(counts.completes, 1);
        assert_eq!(
            counts.direct_uploads, 0,
            "a 27 MiB body must never take the direct route"
        );
        assert_eq!(
            counts.parts, expected_parts,
            "the client must send contiguous parts of exactly the declared lengths"
        );
        assert!(counts.parts.len() as u32 <= MAX_ATTACHMENT_PARTS);
        assert_eq!(counts.rate_limited, 0);
    });
    assert_eq!(relay.row_state(&upload.id_hex), Some(ObjectState::Ready));
    assert_eq!(relay.object_len(&upload.id_hex), Some(sealed_len));

    // Assembled in order: byte-identical means no part was transposed.
    let fetched_path = scratch.join("fetched.bin");
    let mut fetched = File::create(&fetched_path).expect("create fetch sink");
    let copied = client
        .fetch_attachment_to_writer(&upload.id_hex, &token, &mut fetched)
        .expect("fetch the multipart object");
    fetched.sync_all().expect("flush fetch sink");
    drop(fetched);
    assert_eq!(copied, sealed_len);
    assert!(
        files_identical(&opaque, &fetched_path),
        "the reassembled multipart object must be byte-identical to what was uploaded"
    );

    client
        .delete_attachment(&upload.id_hex, &token)
        .expect("burn the multipart object");
    assert!(!relay.object_present(&upload.id_hex));

    let _ = fs::remove_dir_all(&scratch);
}

/// Every refusal the receive half owes, against one honestly uploaded object.
///
/// The point of gathering them in one test is that they must be *distinct*: a
/// rejected capability, a burned object, an expired object, a tampered body and
/// a lying declared size are different facts, and the audit found the last
/// user-visible step flattening the first two into one "expired" string.
#[test]
fn tampered_expired_deleted_and_capability_rejected_fetches_are_each_refused_distinctly() {
    let _serial = fixture_lock().lock().unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("refusals");
    let relay_url = relay.base_url();
    let (alice, bob) = verified_pair(&storage, &relay_url);
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    let source = write_plaintext_source(&storage.root.join("source.txt"), 64 * 1024);
    let sent = send_attachment(&alice, &client, &source, "fixture-note.txt", false);

    bob.activate();
    let pending = osl_privacy_hub::broker::list_osl_chat_attachments(&bob.core, &bob.security, &bob.broker)
        .expect("list pending attachments");
    assert_eq!(pending.len(), 1);
    let mut plan = osl_privacy_hub::broker::take_osl_chat_attachment(
        &bob.core,
        &bob.security,
        &bob.broker,
        &pending[0].attachment_id,
    )
    .expect("take the attachment open plan");
    let token = token_from_hex(&plan.fetch_token);

    use osl_privacy_hub::peer_attachment_io::{
        classify_cipher_store_error, describe_transport_outcome, TransportOutcome, TransportPhase,
    };

    // --- an id that was never issued is Gone -------------------------------
    let missing_id = "0".repeat(32);
    let mut sink = Vec::new();
    let missing = client
        .fetch_attachment_to_writer(&missing_id, &token, &mut sink)
        .expect_err("an unknown object must not be served");
    assert!(matches!(missing, CipherStoreError::NotFound));
    assert_eq!(
        classify_cipher_store_error(&missing),
        TransportOutcome::Gone
    );

    // --- a malformed id is refused locally, with no request at all ---------
    let fetches_before = relay.counts(|counts| counts.fetches);
    let mut sink = Vec::new();
    let malformed = client
        .fetch_attachment_to_writer("not-a-canonical-id", &token, &mut sink)
        .expect_err("a malformed id must be refused before the network");
    assert!(matches!(malformed, CipherStoreError::ParseError(_)));
    assert_eq!(
        relay.counts(|counts| counts.fetches),
        fetches_before,
        "a malformed id must not reach the store"
    );

    // --- a rejected capability is NOT the same fact as an expired object ---
    let mut wrong_token = token;
    wrong_token[0] ^= 0xff;
    let mut sink = Vec::new();
    let rejected = client
        .fetch_attachment_to_writer(&sent.object_id, &wrong_token, &mut sink)
        .expect_err("a mismatched capability must be refused");
    assert!(
        matches!(rejected, CipherStoreError::Status { status: 403, .. }),
        "an existing object with the wrong capability must answer 403"
    );
    let rejected_outcome = classify_cipher_store_error(&rejected);
    assert_eq!(rejected_outcome, TransportOutcome::CapabilityRejected);
    assert_ne!(
        rejected_outcome,
        classify_cipher_store_error(&missing),
        "403 capability mismatch must not classify as 404 expiry"
    );
    assert_ne!(
        describe_transport_outcome(rejected_outcome, TransportPhase::Fetch),
        describe_transport_outcome(TransportOutcome::Gone, TransportPhase::Fetch),
        "the two outcomes must read differently to the user"
    );

    // Existence is checked before the capability, so a wrong capability on an
    // id that does not exist still answers 404 and leaks nothing.
    let mut sink = Vec::new();
    let missing_and_wrong = client
        .fetch_attachment_to_writer(&missing_id, &wrong_token, &mut sink)
        .expect_err("an unknown object must not be served");
    assert!(matches!(missing_and_wrong, CipherStoreError::NotFound));

    // --- a tampered body fails the pre-decrypt digest ----------------------
    relay.set_fault(
        &sent.object_id,
        ObjectFault {
            corrupt_first_byte: true,
            truncate_served_by: 0,
        },
    );
    let tampered = fetch_and_verify(&bob, &client, &plan)
        .expect_err("a tampered body must not reach the decryptor");
    assert!(
        tampered == "This private attachment failed authentication",
        "a tampered body must be refused as an authentication failure"
    );
    assert!(
        staging_files(&bob.local_root, "download-").is_empty(),
        "a refused fetch must not leave a download staging file behind"
    );

    // --- a short body is refused cleanly, without hanging -----------------
    relay.set_fault(
        &sent.object_id,
        ObjectFault {
            corrupt_first_byte: false,
            truncate_served_by: 4096,
        },
    );
    let short = fetch_and_verify(&bob, &client, &plan)
        .expect_err("a body shorter than its declared length must be refused");
    assert!(
        short.starts_with("This private attachment"),
        "a short body must surface a product refusal, not a panic"
    );
    assert!(staging_files(&bob.local_root, "download-").is_empty());
    relay.clear_fault(&sent.object_id);

    // --- an honest fetch still works, so the refusals above were specific --
    let good = fetch_and_verify(&bob, &client, &plan).expect("the honest object still verifies");
    osl_privacy_hub::peer_attachment_io::remove_staging_path(&good)
        .expect("clear the download staging file");

    // --- a notice that lies about the sealed size is refused --------------
    let honest_size = plan.sealed_size;
    plan.sealed_size = honest_size + 1;
    let lied = fetch_and_verify(&bob, &client, &plan)
        .expect_err("a declared size that does not match the object must be refused");
    assert!(
        lied == "This private attachment has an invalid size",
        "a mismatched sealed size must be refused before decryption"
    );
    plan.sealed_size = honest_size;

    // --- an expired object is Gone ----------------------------------------
    relay.set_expiry(&sent.object_id, now_secs() - 1);
    let expired = fetch_and_verify(&bob, &client, &plan)
        .expect_err("an expired object must not be served");
    assert!(expired == "This private attachment is unavailable or expired");
    let mut sink = Vec::new();
    let expired_raw = client
        .fetch_attachment_to_writer(&sent.object_id, &token, &mut sink)
        .expect_err("an expired object must not be served");
    assert_eq!(
        classify_cipher_store_error(&expired_raw),
        TransportOutcome::Gone
    );
    relay.set_expiry(&sent.object_id, now_secs() + 3600);

    // --- a burned object is Gone, and the burn is idempotent --------------
    client
        .delete_attachment(&sent.object_id, &token)
        .expect("burn the object");
    assert!(!relay.object_present(&sent.object_id));
    let deleted = fetch_and_verify(&bob, &client, &plan)
        .expect_err("a burned object must not be served");
    assert!(deleted == "This private attachment is unavailable or expired");
    client
        .delete_attachment(&sent.object_id, &token)
        .expect("a second burn of the same id is idempotent");

    assert!(
        surviving_plaintext(&alice, &bob).is_none(),
        "no refusal path may leave plaintext on disk"
    );
    assert!(staging_files(&bob.local_root, "download-").is_empty());
}

/// View-once, the image branch, and what actually happens when the burn fails.
///
/// * an image attachment decrypts **into memory only**, so no plaintext file is
///   ever created for it;
/// * the recovered image bytes are byte-identical to the sender's file;
/// * committing the open deletes the inbox capability and makes a second open
///   impossible, so replay safety does not depend on the remote burn;
/// * when the remote burn fails, the ciphertext **survives** — the orphan the
///   audit predicted, reproduced here rather than assumed;
/// * the durable deletion outbox now in `peer_attachment_io` takes ownership of
///   that failed burn and completes it on a later drain, which is what turns a
///   full-TTL orphan into a retry.
#[test]
fn view_once_open_is_replay_safe_and_a_failed_burn_is_recovered_by_the_deletion_outbox() {
    let _serial = fixture_lock().lock().unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("viewonce");
    let relay_url = relay.base_url();
    let (alice, bob) = verified_pair(&storage, &relay_url);
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    let source = write_plaintext_source(&storage.root.join("source.png"), 96 * 1024);
    let sent = send_attachment(&alice, &client, &source, "fixture-pixels.png", true);

    bob.activate();
    let pending = osl_privacy_hub::broker::list_osl_chat_attachments(&bob.core, &bob.security, &bob.broker)
        .expect("list pending attachments");
    assert_eq!(pending.len(), 1);
    assert!(pending[0].view_once, "the notice must carry the view-once flag");
    assert!(pending[0].mime_type == "image/png");
    assert!(osl_privacy_hub::peer_attachment_io::supported_protected_image_mime(
        &pending[0].mime_type
    ));
    // The open plan keeps its expiry private, so take it from the listing.
    let expires_at = pending[0].expires_at;

    let plan = osl_privacy_hub::broker::take_osl_chat_attachment(
        &bob.core,
        &bob.security,
        &bob.broker,
        &pending[0].attachment_id,
    )
    .expect("take the attachment open plan");
    let token = token_from_hex(&plan.fetch_token);
    assert!(
        token == sent.token && plan.object_id == sent.object_id,
        "the notice must carry the exact object id and capability the sender uploaded with"
    );
    let download_path = fetch_and_verify(&bob, &client, &plan).expect("fetch and verify ciphertext");

    let mut sealed = File::open(&download_path).expect("open the verified ciphertext");
    let opened = osl_privacy_hub::peer_attachment_io::decrypt_file_to_memory(
        &mut sealed,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
    )
    .expect("decrypt the image into memory");
    drop(sealed);
    osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path)
        .expect("clear the download staging file");

    assert_eq!(opened.len() as u64, plan.plaintext_size);
    let expected = fs::read(&source).expect("read the sender's file");
    assert!(
        opened.as_slice() == expected.as_slice(),
        "the recovered image must be byte-identical to the sender's file"
    );
    drop(expected);
    drop(opened);

    // The image branch never writes plaintext, so nothing should be found even
    // before any cleanup step runs.
    assert!(
        staging_files(&bob.local_root, "opened-").is_empty(),
        "the in-memory image path must not create a plaintext staging file"
    );
    assert!(
        surviving_plaintext(&alice, &bob).is_none(),
        "the in-memory image path must leave no plaintext on disk"
    );

    let replay_commit_visible = Arc::new(AtomicBool::new(false));
    relay.observe_replay_commit_before_delete(Arc::clone(&replay_commit_visible));

    // --- commit: replay safety is durable before any remote burn ------------
    osl_privacy_hub::broker::commit_osl_chat_attachment_open(
        &bob.core,
        &bob.security,
        &bob.broker,
        &plan,
    )
    .expect("commit the view-once open");
    assert_eq!(relay.pending_for(&bob.identity_id), 0);
    assert!(
        osl_privacy_hub::broker::take_osl_chat_attachment(
            &bob.core,
            &bob.security,
            &bob.broker,
            &plan.attachment_id,
        )
        .is_err(),
        "a committed view-once attachment must never open a second time"
    );
    replay_commit_visible.store(true, Ordering::Release);

    // --- the burn fails: ciphertext survives -------------------------------
    relay.set_delete_fails(true);
    let burn = client.delete_attachment(&plan.object_id, &token);
    assert!(
        !relay.delete_attempted_before_replay_commit(),
        "the relay observed a remote delete attempt before the durable replay commit and replay check"
    );
    assert!(burn.is_err(), "the fixture is refusing deletes for this step");
    assert!(
        relay.object_present(&plan.object_id),
        "a failed view-once burn leaves the ciphertext in remote storage"
    );
    assert_eq!(
        osl_privacy_hub::peer_attachment_io::classify_cipher_store_error(
            burn.as_ref().expect_err("delete failed")
        ),
        osl_privacy_hub::peer_attachment_io::TransportOutcome::ServerFault,
        "a 5xx burn failure must be reported as a server fault, not as success"
    );

    // --- the durable outbox takes ownership of the owed burn ---------------
    osl_privacy_hub::peer_attachment_io::enqueue_attachment_deletion(
        &plan.object_id,
        &plan.fetch_token,
        expires_at,
        true,
    )
    .expect("record the owed deletion durably");

    relay.set_delete_fails(false);
    let mut attempted = 0u32;
    let report = osl_privacy_hub::peer_attachment_io::drain_attachment_deletions(|pending| {
        attempted += 1;
        match client.delete_attachment(&pending.object_id, &pending.fetch_token) {
            Ok(()) => osl_privacy_hub::peer_attachment_io::DeletionAttempt::Deleted,
            Err(CipherStoreError::NotFound) => {
                osl_privacy_hub::peer_attachment_io::DeletionAttempt::AlreadyGone
            }
            Err(_) => osl_privacy_hub::peer_attachment_io::DeletionAttempt::Retry,
        }
    })
    .expect("drain the deletion outbox");
    assert_eq!(attempted, 1);
    assert_eq!(report.deleted, 1);
    assert_eq!(report.retained, 0);
    assert!(
        !relay.object_present(&plan.object_id),
        "the outbox drain must complete the burn the inline delete lost"
    );
    assert!(
        surviving_plaintext(&alice, &bob).is_none(),
        "no plaintext may survive the view-once path"
    );
}

/// Fidelity checks for request shapes `CipherStoreClient` cannot produce.
///
/// A fixture is only useful if it refuses what production refuses. These go
/// through a raw socket to reach the 401 / 400 / 413 / 411 branches the Rust
/// client never exercises, because it always sends a well-formed capability, a
/// TTL from the allowlist and a canonical id.
#[test]
fn fixture_refuses_the_request_shapes_the_rust_client_cannot_produce() {
    let relay = RelayServer::start();
    let relay_url = relay.base_url();
    let address = relay.address().to_owned();
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    // The client's mirrored bounds must equal the worker's, or every assertion
    // in this file is measuring the wrong wall.
    assert_eq!(MAX_SEALED_ATTACHMENT_BYTES, WORKER_MAX_SEALED_ATTACHMENT_BYTES);
    assert_eq!(ATTACHMENT_MULTIPART_PART_BYTES, MAX_ATTACHMENT_PART_BYTES);
    assert_eq!(ATTACHMENT_MULTIPART_MAX_PARTS, MAX_ATTACHMENT_PARTS);

    let token_hex = hex_lower(&fresh_fetch_token());
    let good_token = token_from_hex(&token_hex);

    // One honest object to authorise against.
    let scratch = std::env::temp_dir().join(format!(
        "osl-attachment-raw-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&scratch).expect("create raw scratch dir");
    let body = scratch.join("sealed.bin");
    write_opaque_file(&body, 4096);
    let upload = client
        .upload_attachment_file(File::open(&body).expect("open body"), 3600, &good_token)
        .expect("seed one honest object");

    let code = |response: &[u8]| -> String {
        serde_json::from_slice::<Value>(response)
            .ok()
            .and_then(|value| value["error"].as_str().map(str::to_owned))
            .unwrap_or_default()
    };

    // GET with no capability header at all -> 401, never 403 or 404.
    let (status, response) = raw_request(
        &address,
        "GET",
        &format!("/v1/attachment/{}", upload.id_hex),
        &[],
        b"",
    );
    assert_eq!(status, 401);
    assert!(code(&response) == "fetch_token_required");

    // GET with a capability that is not 32 lowercase hex -> 401.
    let (status, response) = raw_request(
        &address,
        "GET",
        &format!("/v1/attachment/{}", upload.id_hex),
        &[("x-osl-fetch-token", "NOTHEX")],
        b"",
    );
    assert_eq!(status, 401);
    assert!(code(&response) == "fetch_token_required");

    // An id of the wrong length -> 400 bad_id, distinct from 404.
    let (status, response) = raw_request(
        &address,
        "GET",
        "/v1/attachment/0123456789abcdef",
        &[("x-osl-fetch-token", &token_hex)],
        b"",
    );
    assert_eq!(status, 400);
    assert!(code(&response) == "bad_id");

    // A well-formed but unknown id -> 404 on GET, 204 on DELETE (idempotent).
    let unknown = "1".repeat(32);
    let (status, _) = raw_request(
        &address,
        "GET",
        &format!("/v1/attachment/{unknown}"),
        &[("x-osl-fetch-token", &token_hex)],
        b"",
    );
    assert_eq!(status, 404);
    let (status, _) = raw_request(
        &address,
        "DELETE",
        &format!("/v1/attachment/{unknown}"),
        &[("x-osl-fetch-token", &token_hex)],
        b"",
    );
    assert_eq!(status, 204);

    // Direct upload with a TTL outside the allowlist -> 400 bad_ttl.
    let (status, response) = raw_request(
        &address,
        "POST",
        "/v1/attachment",
        &[
            ("x-osl-ttl-seconds", "7200"),
            ("x-osl-fetch-token", &token_hex),
            ("content-type", "application/octet-stream"),
        ],
        b"payload",
    );
    assert_eq!(status, 400);
    assert!(code(&response) == "bad_ttl");

    // Direct upload with no capability -> 400 bad_fetch_token.
    let (status, response) = raw_request(
        &address,
        "POST",
        "/v1/attachment",
        &[
            ("x-osl-ttl-seconds", "3600"),
            ("content-type", "application/octet-stream"),
        ],
        b"payload",
    );
    assert_eq!(status, 400);
    assert!(code(&response) == "bad_fetch_token");

    // Session with no declared size -> 400 size_required.
    let (status, response) = raw_request(
        &address,
        "POST",
        "/v1/attachment/session",
        &[
            ("x-osl-ttl-seconds", "3600"),
            ("x-osl-fetch-token", &token_hex),
        ],
        b"",
    );
    assert_eq!(status, 400);
    assert!(code(&response) == "size_required");

    // Session larger than the sealed ceiling -> 413 too_large.
    let (status, response) = raw_request(
        &address,
        "POST",
        "/v1/attachment/session",
        &[
            ("x-osl-ttl-seconds", "3600"),
            ("x-osl-fetch-token", &token_hex),
            (
                "x-osl-size-bytes",
                &(WORKER_MAX_SEALED_ATTACHMENT_BYTES + 1).to_string(),
            ),
        ],
        b"",
    );
    assert_eq!(status, 413);
    assert!(code(&response) == "too_large");

    // A part for a session that does not exist -> 404, before any body work.
    let (status, _) = raw_request(
        &address,
        "PUT",
        &format!("/v1/attachment/{unknown}/part/1"),
        &[("x-osl-fetch-token", &token_hex)],
        b"part",
    );
    assert_eq!(status, 404);

    // A part number beyond the hard ceiling -> 400 bad_part.
    let (status, response) = raw_request(
        &address,
        "PUT",
        &format!("/v1/attachment/{unknown}/part/999"),
        &[("x-osl-fetch-token", &token_hex)],
        b"part",
    );
    assert_eq!(status, 400);
    assert!(code(&response) == "bad_part");

    client
        .delete_attachment(&upload.id_hex, &good_token)
        .expect("burn the seeded object");
    let _ = fs::remove_dir_all(&scratch);
}

/// What the padding-bucket table and the rate-limit budgets jointly imply about
/// the multipart route in production. No network, no crypto — this exists so a
/// change to either table fails a test instead of silently changing the shape
/// of every real upload.
#[test]
fn padding_buckets_and_rate_limit_budgets_bound_the_real_multipart_shape() {
    use crypto::attachment::{ATTACHMENT_BUCKETS, ATTACHMENT_CHUNK_SIZE};

    // Sealed length for a bucket: every chunk is a full chunk plus its tag.
    let sealed_for = |bucket: u64| -> u64 {
        let chunks = bucket / ATTACHMENT_CHUNK_SIZE as u64;
        bucket + chunks * 16
    };
    let parts_for = |sealed: u64| -> u64 { sealed.div_ceil(MAX_ATTACHMENT_PART_BYTES) };

    // Every bucket must still fit the sealed ceiling the store enforces.
    for bucket in ATTACHMENT_BUCKETS {
        assert!(
            sealed_for(*bucket) <= WORKER_MAX_SEALED_ATTACHMENT_BYTES,
            "every padding bucket must seal to something the cipher-store accepts"
        );
        assert!(
            parts_for(sealed_for(*bucket)) <= u64::from(MAX_ATTACHMENT_PARTS),
            "every padding bucket must fit within the multipart part ceiling"
        );
    }

    // Because plaintext is padded to a bucket, sealed sizes are quantised: no
    // real attachment can land between the 25 MiB and 50 MiB buckets. So the
    // direct route covers every bucket up to 25 MiB, and the first multipart
    // upload in production is the 50 MiB bucket at seven parts -- there is no
    // real two-, three- or four-part attachment.
    let twenty_five = sealed_for(25 * 1024 * 1024);
    let fifty = sealed_for(50 * 1024 * 1024);
    assert!(
        twenty_five <= MAX_DIRECT_ATTACHMENT_BYTES,
        "the 25 MiB bucket must still take the direct route"
    );
    assert!(
        fifty > MAX_DIRECT_ATTACHMENT_BYTES,
        "the 50 MiB bucket must take the multipart route"
    );
    assert_eq!(
        parts_for(fifty), 7,
        "the smallest multipart upload production can produce is seven parts"
    );

    // The largest bucket must land exactly on the part ceiling.
    let largest = sealed_for(*ATTACHMENT_BUCKETS.last().expect("buckets are non-empty"));
    assert_eq!(parts_for(largest), u64::from(MAX_ATTACHMENT_PARTS));

    // Request budget: one session + N parts + one completion.
    let requests_for_largest = 1 + parts_for(largest) + 1;
    assert_eq!(requests_for_largest, 67);
    let attempts = u64::from(ATTACHMENT_UPLOAD_BUDGET) / requests_for_largest;
    assert_eq!(
        attempts, 2,
        "the attachment-upload budget permits only two maximum-size attempts per hour"
    );
    // A fetch of the same object is a single request, so the fetch and delete
    // budgets are per-object rather than per-part.
    assert!(ATTACHMENT_FETCH_BUDGET >= 120);
    assert!(ATTACHMENT_DELETE_BUDGET >= 60);
}

/// The full product path over the **smallest multipart shape production can
/// actually produce**: a plaintext just past the 25 MiB bucket, which pads to
/// the 50 MiB bucket and seals to roughly 50 MiB across seven parts.
///
/// Ignored by default because it moves ~50 MiB of ciphertext through the
/// loopback fixture and hashes it three times in an unoptimised test build.
/// Run it deliberately:
///
/// ```text
/// cargo test --features core --test peer_attachment_network_e2e -- --ignored
/// ```
#[test]
#[ignore = "moves ~50 MiB through the loopback fixture; run deliberately"]
fn full_crypto_multipart_round_trip_at_the_fifty_mebibyte_bucket() {
    let _serial = fixture_lock().lock().unwrap_or_else(|error| error.into_inner());
    let relay = RelayServer::start();
    let storage = TestStorage::new("bigmultipart");
    let relay_url = relay.base_url();
    let (alice, bob) = verified_pair(&storage, &relay_url);
    let client = CipherStoreClient::new(&relay_url).expect("build cipher-store client");

    // Just past the 25 MiB bucket, so padding selects the 50 MiB bucket.
    let plaintext_len = 25 * 1024 * 1024 + 1;
    let source = write_plaintext_source(&storage.root.join("source.bin"), plaintext_len);
    let sent = send_attachment(&alice, &client, &source, "fixture-archive.zip", false);

    assert!(
        sent.sealed_size > MAX_DIRECT_ATTACHMENT_BYTES,
        "this case must exercise the multipart route"
    );
    relay.counts(|counts| {
        assert_eq!(counts.direct_uploads, 0);
        assert_eq!(counts.sessions, 1);
        assert_eq!(counts.completes, 1);
        assert_eq!(
            counts.parts.len(),
            7,
            "the 50 MiB bucket must upload in seven parts"
        );
        // Every part but the last is exactly the maximum part size, and the
        // sizes must sum to the sealed length.
        for (index, (number, size)) in counts.parts.iter().enumerate() {
            assert_eq!(*number, index as u32 + 1);
            if index + 1 < counts.parts.len() {
                assert_eq!(*size, MAX_ATTACHMENT_PART_BYTES);
            } else {
                assert!(*size > 0 && *size <= MAX_ATTACHMENT_PART_BYTES);
            }
        }
        assert_eq!(
            counts.parts.iter().map(|(_, size)| *size).sum::<u64>(),
            sent.sealed_size
        );
    });

    bob.activate();
    let pending = osl_privacy_hub::broker::list_osl_chat_attachments(&bob.core, &bob.security, &bob.broker)
        .expect("list pending attachments");
    assert_eq!(pending.len(), 1);
    let plan = osl_privacy_hub::broker::take_osl_chat_attachment(
        &bob.core,
        &bob.security,
        &bob.broker,
        &pending[0].attachment_id,
    )
    .expect("take the attachment open plan");
    assert_eq!(plan.sealed_size, sent.sealed_size);

    let download_path = fetch_and_verify(&bob, &client, &plan).expect("fetch and verify ciphertext");
    let mut sealed = File::open(&download_path).expect("open the verified ciphertext");
    let opened = osl_privacy_hub::peer_attachment_io::decrypt_file(
        &bob.local_root,
        &mut sealed,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
    )
    .expect("decrypt the multipart attachment");
    drop(sealed);
    osl_privacy_hub::peer_attachment_io::remove_staging_path(&download_path)
        .expect("clear the download staging file");

    let opened_path = opened
        .path()
        .expect("the decrypted staging file is owned until it is removed")
        .to_owned();
    assert_eq!(opened.plaintext_len(), plaintext_len as u64);
    assert!(
        files_identical(&source, &opened_path),
        "a multipart round trip must recover byte-identical plaintext"
    );

    osl_privacy_hub::broker::commit_osl_chat_attachment_open(
        &bob.core,
        &bob.security,
        &bob.broker,
        &plan,
    )
    .expect("commit the attachment open");
    opened
        .remove_now()
        .expect("remove the decrypted staging file");
    assert!(
        surviving_plaintext(&alice, &bob).is_none(),
        "no plaintext may survive a multipart round trip"
    );
}
