//! TASK 3643: burning the exact message that owns an in-flight multipart
//! attachment cancels that upload before it can create a completed file.

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    mpsc::{self, Receiver, Sender},
    Arc, Condvar, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::aead::Key;
use crypto::attachment::encrypt_attachment;
use ipc::cipher_store_client::{
    CipherStoreClient, ProChunkedUploadFile, ProChunkedUploadPiece, ProChunkedUploadReport,
    ATTACHMENT_MULTIPART_MAX_PARTS, ATTACHMENT_MULTIPART_PART_BYTES, TTL_7D,
};
use ipc::commands::cmd_osl_burn_sender_message_records_both_sides;
use ipc::state::AppState;
use keystore::{canonical_burn_bytes, BurnScope, KeyServerClient};
use osl_privacy_hub::attachment_limits::AttachmentAccountTier;
use osl_privacy_hub::osl_chat_attachment_download_permission::{
    RecipientAttachmentDownloadRequest, RecipientAttachmentInbox,
};
use osl_privacy_hub::osl_chat_drag_drop::OslChatAttachmentTray;
use osl_privacy_hub::osl_chat_pro_attachment_send::{
    send_registered_pro_attachment_from_osl_chat_tray, OslChatProAttachmentSendRequest,
};
use serde_json::Value;
use store::{MessageStore, StoredMessage};

const MESSAGE_ID: &str = "task3643-marked-message";
const CHANNEL_ID: &str = "task3643-channel";
const SENDER_ID: &str = "sender-3643";
const MARKED_TEXT: &str = "TASK3643-MARKED-ATTACHMENT-BURN-DURING-UPLOAD";
const UPLOAD_ID: &str = "36433643364336433643364336433643";
const CONTROL_FILE_ID: &str = "cccccccccccccccccccccccccccccccc";
const RECIPIENT_ID: &str = "recipient-3643";
const EXPIRES_AT: i64 = 1_900_003_643;
const FETCH_TOKEN: [u8; 16] = [0x36; 16];
const STORE_KEY: &[u8; 32] = &[0x43; 32];

#[derive(Default)]
struct ServerState {
    active_uploads: BTreeSet<String>,
    stored_parts: HashMap<String, BTreeSet<u32>>,
    unlock_keys: BTreeSet<String>,
    burn_request_count: usize,
    delete_cleanup_count: usize,
    scheduled_cleanup_count: usize,
    completed_target_count: usize,
}

impl ServerState {
    fn stored_part_count(&self) -> usize {
        self.stored_parts.values().map(BTreeSet::len).sum()
    }

    fn terminal_action_seen(&self) -> bool {
        self.delete_cleanup_count + self.completed_target_count != 0
    }
}

#[derive(Default)]
struct FirstPartPause {
    released: bool,
}

struct BurnDuringUploadServer {
    base_url: String,
    state: Arc<Mutex<ServerState>>,
    pause: Arc<(Mutex<FirstPartPause>, Condvar)>,
    first_part_stored: Receiver<()>,
    server: Option<JoinHandle<()>>,
}

impl BurnDuringUploadServer {
    fn start(sealed_len: u64, user_id: String, public_key: crypto::ed25519::PublicKey) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind task 3643 server");
        listener
            .set_nonblocking(true)
            .expect("make task 3643 accept loop bounded");
        let address = listener.local_addr().expect("task 3643 server address");
        let state = Arc::new(Mutex::new(ServerState {
            unlock_keys: BTreeSet::from([MESSAGE_ID.to_owned()]),
            ..ServerState::default()
        }));
        let pause = Arc::new((Mutex::new(FirstPartPause::default()), Condvar::new()));
        let (stored_tx, stored_rx) = mpsc::channel();
        let server_state = Arc::clone(&state);
        let server_pause = Arc::clone(&pause);
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut handlers = Vec::new();
            loop {
                if Instant::now() >= deadline {
                    panic!(
                        "task 3643 server timed out waiting for burn and terminal upload action"
                    );
                }
                match listener.accept() {
                    Ok((stream, _)) => {
                        let request_state = Arc::clone(&server_state);
                        let request_pause = Arc::clone(&server_pause);
                        let request_stored_tx = stored_tx.clone();
                        let request_user_id = user_id.clone();
                        let request_public_key = public_key;
                        handlers.push(thread::spawn(move || {
                            handle_request(
                                stream,
                                sealed_len,
                                &request_user_id,
                                request_public_key,
                                request_state,
                                request_pause,
                                request_stored_tx,
                            );
                        }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        let finished = {
                            let state = server_state.lock().expect("lock task 3643 server state");
                            state.burn_request_count == 1 && state.terminal_action_seen()
                        };
                        if finished {
                            break;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("task 3643 accept failed: {error}"),
                }
            }
            for handler in handlers {
                handler.join().expect("task 3643 request handler completes");
            }
        });
        Self {
            base_url: format!("http://{address}"),
            state,
            pause,
            first_part_stored: stored_rx,
            server: Some(server),
        }
    }

    fn wait_for_first_part(&self) {
        self.first_part_stored
            .recv_timeout(Duration::from_secs(20))
            .expect("first upload part must be stored before bounded wait expires");
    }

    fn release_first_part(&self) {
        let (lock, ready) = &*self.pause;
        let mut pause = lock.lock().expect("lock first-part pause");
        pause.released = true;
        ready.notify_all();
    }

    fn counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        let state = self.state.lock().expect("lock task 3643 counts");
        (
            state.active_uploads.len(),
            state.stored_part_count(),
            state.unlock_keys.len(),
            state.completed_target_count,
            state.delete_cleanup_count,
            state.scheduled_cleanup_count,
        )
    }

    fn join(&mut self) {
        self.server
            .take()
            .expect("task 3643 server join is single-use")
            .join()
            .expect("task 3643 server exits cleanly");
    }

    /// Model the store's expiry sweep after the immediate cancellation delete.
    /// There must be nothing left for this later cleanup path to claim.
    fn run_scheduled_cleanup(&self) {
        let mut state = self.state.lock().expect("lock scheduled cleanup");
        let lingering = state.active_uploads.len();
        state.scheduled_cleanup_count += lingering;
        state.active_uploads.clear();
        state.stored_parts.clear();
    }
}

fn handle_request(
    mut stream: TcpStream,
    sealed_len: u64,
    user_id: &str,
    public_key: crypto::ed25519::PublicKey,
    state: Arc<Mutex<ServerState>>,
    pause: Arc<(Mutex<FirstPartPause>, Condvar)>,
    stored_tx: Sender<()>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("bound task 3643 request read");
    stream
        .set_write_timeout(Some(Duration::from_secs(20)))
        .expect("bound task 3643 response write");
    let request = read_request(&mut stream);
    let (method, path, headers, body) = split_request(&request);

    match (method.as_str(), path.as_str()) {
        ("POST", "/v1/attachment/session") => {
            assert_eq!(
                header(&headers, "x-osl-size-bytes"),
                Some(sealed_len.to_string()).as_deref()
            );
            let mut state = state.lock().expect("lock session state");
            assert!(state.active_uploads.insert(UPLOAD_ID.to_owned()));
            state
                .stored_parts
                .insert(UPLOAD_ID.to_owned(), BTreeSet::new());
            drop(state);
            respond_json(
                &mut stream,
                &format!(
                    r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{sealed_len},"max_part_bytes":{ATTACHMENT_MULTIPART_PART_BYTES},"max_parts":{ATTACHMENT_MULTIPART_MAX_PARTS}}}"#
                ),
            );
        }
        ("PUT", path) if path == format!("/v1/attachment/{UPLOAD_ID}/part/1") => {
            assert_eq!(body.len() as u64, ATTACHMENT_MULTIPART_PART_BYTES);
            {
                let mut state = state.lock().expect("lock first-part state");
                assert!(state.active_uploads.contains(UPLOAD_ID));
                assert!(state
                    .stored_parts
                    .get_mut(UPLOAD_ID)
                    .expect("session has a part set")
                    .insert(1));
            }
            stored_tx.send(()).expect("announce stored first part");
            let (lock, ready) = &*pause;
            let pause = lock.lock().expect("lock first-part response pause");
            let (pause, timeout) = ready
                .wait_timeout_while(pause, Duration::from_secs(20), |pause| !pause.released)
                .expect("wait on first-part response pause");
            assert!(
                !timeout.timed_out() && pause.released,
                "first-part response was never released"
            );
            respond_json(
                &mut stream,
                &format!(r#"{{"part_number":1,"size_bytes":{ATTACHMENT_MULTIPART_PART_BYTES}}}"#),
            );
        }
        ("DELETE", "/v1/wrapped-keys") => {
            let burn = serde_json::from_slice::<Value>(body).expect("burn request is JSON");
            assert_eq!(burn["scope"], "single");
            assert_eq!(burn["user_id"], user_id);
            assert!(burn["target_user_id"].is_null());
            let content_id = burn["target_content_id"]
                .as_str()
                .expect("burn names exact content id");
            assert_eq!(content_id, MESSAGE_ID);
            let timestamp_ms = burn["timestamp_ms"].as_i64().expect("burn timestamp");
            let request_id = burn["request_id"].as_str().expect("burn request id");
            assert!(!request_id.is_empty());
            let signature = STANDARD
                .decode(burn["burn_signature_b64"].as_str().expect("burn signature"))
                .expect("burn signature is base64");
            let signature = crypto::ed25519::Signature::from_bytes(
                signature.try_into().expect("burn signature is 64 bytes"),
            );
            let canonical = canonical_burn_bytes(
                user_id,
                timestamp_ms,
                request_id,
                &BurnScope::Single {
                    content_id: content_id.to_owned(),
                },
            );
            assert!(crypto::ed25519::verify(&public_key, &canonical, &signature)
                .expect("verify exact-message burn signature"));
            let deleted = {
                let mut state = state.lock().expect("lock unlock-key state");
                state.burn_request_count += 1;
                state.unlock_keys.remove(content_id)
            };
            assert!(deleted, "exact marked message must own one unlock key");
            respond_json(
                &mut stream,
                &format!(
                    r#"{{"scope":"single","deleted_count":{}}}"#,
                    usize::from(deleted)
                ),
            );
        }
        ("DELETE", path) if path == format!("/v1/attachment/{UPLOAD_ID}") => {
            let mut state = state.lock().expect("lock cancellation cleanup state");
            assert!(state.active_uploads.remove(UPLOAD_ID));
            let removed_parts = state
                .stored_parts
                .remove(UPLOAD_ID)
                .expect("cancelled upload has stored parts")
                .len();
            assert_eq!(removed_parts, 1);
            state.delete_cleanup_count += 1;
            drop(state);
            respond_json(&mut stream, r#"{"deleted":true}"#);
        }
        ("POST", path) if path == format!("/v1/attachment/{UPLOAD_ID}/complete") => {
            let mut state = state.lock().expect("lock false-success completion state");
            state.completed_target_count += 1;
            state.active_uploads.remove(UPLOAD_ID);
            state.stored_parts.remove(UPLOAD_ID);
            drop(state);
            respond_json(
                &mut stream,
                &format!(
                    r#"{{"id":"{UPLOAD_ID}","expires_at":{EXPIRES_AT},"size_bytes":{sealed_len}}}"#
                ),
            );
        }
        _ => panic!("unexpected task 3643 request: {method} {path}"),
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 32 * 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read task 3643 request");
        assert_ne!(read, 0, "task 3643 request ended before its body");
        request.extend_from_slice(&chunk[..read]);
        let Some(headers_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length").then(|| {
                    value
                        .trim()
                        .parse::<usize>()
                        .expect("numeric content length")
                })
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            request.truncate(headers_end + 4 + content_length);
            return request;
        }
    }
}

fn split_request(request: &[u8]) -> (String, String, String, &[u8]) {
    let headers_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .expect("task 3643 header end");
    let headers = String::from_utf8_lossy(&request[..headers_end]).into_owned();
    let mut request_line = headers
        .lines()
        .next()
        .expect("task 3643 request line")
        .split_whitespace();
    (
        request_line.next().expect("request method").to_owned(),
        request_line.next().expect("request path").to_owned(),
        headers,
        &request[headers_end + 4..],
    )
}

fn header<'a>(headers: &'a str, wanted: &str) -> Option<&'a str> {
    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(wanted).then(|| value.trim())
    })
}

fn respond_json(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write task 3643 response");
}

fn stored_message() -> StoredMessage {
    StoredMessage {
        discord_message_id: MESSAGE_ID.to_owned(),
        channel_id: CHANNEL_ID.to_owned(),
        sender_discord_id: SENDER_ID.to_owned(),
        sender_osl_user_id: SENDER_ID.to_owned(),
        plaintext: MARKED_TEXT.to_owned(),
        decrypted_at: 1_900_003_643,
        reply_parent_id: None,
        edit_revision: 0,
        burned: false,
    }
}

fn completed_control_report() -> ProChunkedUploadReport {
    ProChunkedUploadReport {
        finished_pieces: vec![ProChunkedUploadPiece {
            upload_id: CONTROL_FILE_ID.to_owned(),
            piece_number: 1,
            size_bytes: 17,
        }],
        completed_file: ProChunkedUploadFile {
            file_id: CONTROL_FILE_ID.to_owned(),
            total_size_bytes: 17,
            piece_count: 1,
            expires_at: EXPIRES_AT,
        },
    }
}

#[test]
fn task_3643_burn_exact_attachment_during_first_upload_part() {
    let temp = tempfile::tempdir().expect("task 3643 tempdir");
    let plaintext_path = temp.path().join("task3643-marked-plaintext.bin");
    let plaintext = vec![0x43; ATTACHMENT_MULTIPART_PART_BYTES as usize];
    std::fs::write(&plaintext_path, &plaintext).expect("write exact 8 MiB fixture");
    let sealed = encrypt_attachment(
        Key::from_bytes([0x36; 32]),
        &plaintext,
        b"task-3643-marked-attachment".to_vec(),
        0,
    )
    .expect("seal task 3643 fixture");
    assert!(sealed.len() as u64 > ATTACHMENT_MULTIPART_PART_BYTES);
    let sealed_path = temp.path().join("task3643-marked-sealed.bin");
    std::fs::write(&sealed_path, &sealed).expect("write sealed task 3643 fixture");

    let state = Arc::new(AppState::new());
    let message_store = MessageStore::open(&temp.path().join("message-store"), STORE_KEY)
        .expect("open task 3643 message store");
    message_store
        .put(&stored_message())
        .expect("persist exact marked attachment message");
    assert_eq!(
        message_store
            .get(MESSAGE_ID)
            .expect("read marked message")
            .expect("marked message exists")
            .plaintext,
        MARKED_TEXT
    );
    *state.message_store.lock().expect("install message store") = Some(message_store);

    let identity = keystore::identity_from_entropy([0x36; 16], "burner-3643".to_owned());
    let public_key = identity.ed25519_public;
    state.install_identity(identity);
    let mut server =
        BurnDuringUploadServer::start(sealed.len() as u64, "burner-3643".to_owned(), public_key);
    *state.keyserver.lock().expect("install task 3643 keyserver") =
        Some(KeyServerClient::new(&server.base_url).expect("task 3643 loopback keyserver"));

    let mut tray = OslChatAttachmentTray::default();
    tray.accept_dropped_files([plaintext_path.as_path()])
        .expect("admit task 3643 tray fixture");
    let attachment = tray.attachments().first().expect("marked tray attachment");
    let request = OslChatProAttachmentSendRequest {
        tray_id: attachment.tray_id.clone(),
        tray_path: attachment.path.clone(),
        tray_size_bytes: attachment.size_bytes,
        sealed_path,
        ttl_seconds: TTL_7D,
        fetch_token: FETCH_TOKEN,
    };
    let client = CipherStoreClient::new(&server.base_url).expect("task 3643 upload client");

    let mut receiver = RecipientAttachmentInbox::new();
    receiver
        .record_completed(&completed_control_report(), RECIPIENT_ID, [0xcc; 16])
        .expect("seed receiver control completion");
    let receiver_control_count = receiver.completed_file_count();
    assert_eq!(receiver_control_count, 1);

    let upload_state = Arc::clone(&state);
    let upload = thread::spawn(move || {
        send_registered_pro_attachment_from_osl_chat_tray(
            upload_state.as_ref(),
            MESSAGE_ID,
            &tray,
            &request,
            &client,
        )
    });
    server.wait_for_first_part();

    let active_before = state
        .active_attachment_upload_count()
        .expect("count active upload before burn");
    let (server_active_before, stored_before, unlock_before, _, _, _) = server.counts();
    assert_eq!(active_before, 1);
    assert_eq!(server_active_before, 1);
    assert_eq!(stored_before, 1);
    assert_eq!(unlock_before, 1);
    println!(
        "TASK3643 before_burn active_upload_count={active_before} stored_part_count={stored_before} unlock_key_count={unlock_before} receiver_completed_file_count={receiver_control_count}"
    );

    let burn =
        cmd_osl_burn_sender_message_records_both_sides(state.as_ref(), vec![MESSAGE_ID.to_owned()])
            .expect("burn exact marked attachment message");
    assert_eq!(burn.requested_count, 1);
    assert_eq!(burn.cancelled_upload_count, 1);
    assert_eq!(burn.local_removal_count, 1);
    assert_eq!(burn.remote_removal_count, 1);
    assert_eq!(burn.remaining_local_count, 0);
    assert!(burn.equal_removal_counts);

    server.release_first_part();
    let upload_error = upload
        .join()
        .expect("task 3643 uploader thread completes")
        .expect_err("burned attachment upload must not report success");
    assert_eq!(upload_error, "OSL Chat Pro attachment upload cancelled");
    server.join();
    server.run_scheduled_cleanup();

    let active_after = state
        .active_attachment_upload_count()
        .expect("count active upload after burn");
    let (
        server_active_after,
        stored_after,
        unlock_after,
        completed_target,
        delete_cleanup,
        scheduled_cleanup,
    ) = server.counts();
    assert_eq!(active_after, 0);
    assert_eq!(server_active_after, 0);
    assert_eq!(stored_after, 0);
    assert_eq!(unlock_after, 0);
    assert_eq!(completed_target, 0);
    assert_eq!(delete_cleanup, 1);
    assert_eq!(scheduled_cleanup, 0);
    assert_eq!(receiver.completed_file_count(), receiver_control_count);

    let target_request = RecipientAttachmentDownloadRequest {
        recipient_osl_user_id: RECIPIENT_ID.to_owned(),
        account_tier: AttachmentAccountTier::Free,
    };
    let target_client =
        CipherStoreClient::new(&server.base_url).expect("receiver target open client");
    let mut opened = Vec::new();
    let refusal = receiver
        .open(UPLOAD_ID, &target_request, &target_client, &mut opened)
        .expect_err("incomplete burned target must not open");
    assert_eq!(refusal.name(), "completed_file_unavailable");
    assert!(opened.is_empty());

    println!(
        "TASK3643 after_burn active_upload_count={active_after} stored_part_count={stored_after} unlock_key_count={unlock_after} server_completed_target_count={completed_target} receiver_completed_file_count={} upload_result={upload_error:?}",
        receiver.completed_file_count(),
    );
    println!(
        "TASK3643 open_target=false refusal_name={} burn_local_action_count={} burn_remote_action_count={} cancelled_upload_count={} delete_cleanup_count={delete_cleanup} scheduled_cleanup_count={scheduled_cleanup}",
        refusal.name(),
        burn.local_removal_count,
        burn.remote_removal_count,
        burn.cancelled_upload_count,
    );
}
