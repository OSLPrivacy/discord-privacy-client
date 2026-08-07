use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::cmd_osl_burn_both_sides_server_copies;
use ipc::AppState;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug)]
enum CrashActor {
    Sender,
    Receiver,
    Service,
}

impl CrashActor {
    fn label(self) -> &'static str {
        match self {
            CrashActor::Sender => "sender",
            CrashActor::Receiver => "receiver",
            CrashActor::Service => "service",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CrashPhase {
    Before,
    After,
}

impl CrashPhase {
    fn label(self) -> &'static str {
        match self {
            CrashPhase::Before => "before",
            CrashPhase::After => "after",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ServiceCrash {
    BeforeRemoval,
    AfterRemoval,
}

#[derive(Clone, Debug)]
struct WrappedCopy {
    content_id: String,
    content_type: String,
    sender_id: String,
    recipient_id: String,
    session_version: u32,
    share_index: u32,
    wrapped_share_blob: String,
    blob_version: u32,
    single_use: bool,
    expires_at: String,
}

#[derive(Clone, Default)]
struct Task3608Service {
    copies: Arc<Mutex<HashMap<String, WrappedCopy>>>,
}

struct RunningService {
    base_url: String,
    server: JoinHandle<()>,
}

impl Task3608Service {
    fn start(&self, expected_requests: usize, crash: Option<ServiceCrash>) -> RunningService {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind task3608 server");
        let address = listener.local_addr().expect("read task3608 server address");
        let copies = Arc::clone(&self.copies);
        let server = thread::spawn(move || {
            let mut crash = crash;
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept task3608 request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("set task3608 read timeout");
                if !handle_request(&mut stream, &copies, &mut crash) {
                    break;
                }
            }
        });
        RunningService {
            base_url: format!("http://{address}"),
            server,
        }
    }
}

impl RunningService {
    fn client(&self) -> keystore::KeyServerClient {
        keystore::KeyServerClient::new(&self.base_url).expect("build task3608 client")
    }

    fn join(self) {
        self.server.join().expect("task3608 server exits");
    }
}

fn handle_request(
    stream: &mut TcpStream,
    copies: &Arc<Mutex<HashMap<String, WrappedCopy>>>,
    crash: &mut Option<ServiceCrash>,
) -> bool {
    let request = read_request(stream);
    let request_text = String::from_utf8_lossy(&request);
    let (head, body) = request_text
        .split_once("\r\n\r\n")
        .expect("request has headers and body");
    let request_line = head.lines().next().expect("request line");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    let path_without_query = path.split_once('?').map(|(path, _)| path).unwrap_or(path);
    match (method, path_without_query) {
        ("POST", "/v1/wrapped-keys") => {
            let parsed = serde_json::from_str::<Value>(body).expect("wrapped-key post JSON");
            let copy = WrappedCopy {
                content_id: str_field(&parsed, "content_id"),
                content_type: str_field(&parsed, "content_type"),
                sender_id: str_field(&parsed, "sender_id"),
                recipient_id: str_field(&parsed, "recipient_id"),
                session_version: parsed["session_version"].as_u64().unwrap_or(0) as u32,
                share_index: parsed["share_index"].as_u64().unwrap_or(0) as u32,
                wrapped_share_blob: str_field(&parsed, "wrapped_share_blob"),
                blob_version: parsed["blob_version"].as_u64().unwrap_or(0) as u32,
                single_use: parsed["single_use"].as_bool().unwrap_or(false),
                expires_at: str_field(&parsed, "expires_at"),
            };
            copies
                .lock()
                .expect("lock task3608 copies")
                .insert(copy.content_id.clone(), copy.clone());
            write_json(stream, 201, json!({ "content_id": copy.content_id }));
            true
        }
        ("GET", route) if route.starts_with("/v1/wrapped-keys/") => {
            let content_id = route.trim_start_matches("/v1/wrapped-keys/");
            let requester = query_param(path, "recipient_id").unwrap_or_default();
            let copy = copies
                .lock()
                .expect("lock task3608 copies")
                .get(content_id)
                .cloned();
            match copy {
                None => write_json(
                    stream,
                    404,
                    json!({ "error": "unknown or burned content_id" }),
                ),
                Some(copy) if copy.recipient_id != requester => {
                    write_json(stream, 403, json!({ "error": "recipient mismatch" }))
                }
                Some(copy) => write_json(
                    stream,
                    200,
                    json!({
                        "content_id": copy.content_id,
                        "content_type": copy.content_type,
                        "system_message_kind": null,
                        "sender_id": copy.sender_id,
                        "recipient_id": copy.recipient_id,
                        "session_version": copy.session_version,
                        "share_index": copy.share_index,
                        "wrapped_share_blob": copy.wrapped_share_blob,
                        "blob_version": copy.blob_version,
                        "single_use": copy.single_use,
                        "display_duration_seconds": null,
                        "expires_at": copy.expires_at,
                        "created_at": "2026-08-06T00:00:00.000Z",
                    }),
                ),
            }
            true
        }
        ("DELETE", "/v1/wrapped-keys") => {
            let burn = serde_json::from_str::<Value>(body).expect("burn request JSON");
            assert_eq!(burn["scope"], "all");
            let user_id = str_field(&burn, "user_id");
            if matches!(crash, Some(ServiceCrash::BeforeRemoval)) {
                *crash = None;
                return false;
            }
            let removed = {
                let mut copies = copies.lock().expect("lock task3608 copies");
                let ids: Vec<String> = copies
                    .iter()
                    .filter_map(|(id, copy)| (copy.sender_id == user_id).then(|| id.clone()))
                    .collect();
                let removed = ids.len() as u32;
                for id in ids {
                    copies.remove(&id);
                }
                removed
            };
            if matches!(crash, Some(ServiceCrash::AfterRemoval)) {
                *crash = None;
                return false;
            }
            write_json(
                stream,
                200,
                json!({ "scope": "all", "deleted_count": removed }),
            );
            true
        }
        _ => {
            write_json(stream, 404, json!({ "error": "not found" }));
            true
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).expect("read task3608 request");
        assert_ne!(read, 0, "request ended before headers arrived");
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
                        .expect("content length numeric")
                })
            })
            .unwrap_or(0);
        if request.len() >= headers_end + 4 + content_length {
            return request;
        }
    }
}

fn str_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.to_owned())
    })
}

fn write_json(stream: &mut TcpStream, status: u16, body: Value) {
    let text = body.to_string();
    let reason = match status {
        200 => "OK",
        201 => "Created",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    )
    .expect("write task3608 response");
}

fn random_hex(bytes: usize) -> String {
    crypto::random::random_bytes(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn upload_copy(
    client: &keystore::KeyServerClient,
    sender: &keystore::Identity,
    content_id: &str,
    recipient_id: &str,
    mark: &str,
) {
    client
        .post_wrapped_key(
            sender,
            &keystore::wrapped_key::WrappedKeyUpload {
                content_id: content_id.to_owned(),
                content_type: "text".to_owned(),
                system_message_kind: None,
                recipient_id: recipient_id.to_owned(),
                session_version: 1,
                share_index: 0,
                wrapped_share_blob: STANDARD.encode(mark.as_bytes()),
                blob_version: 1,
                single_use: false,
                display_duration_seconds: None,
                expires_at: "2099-01-01T00:00:00.000Z".to_owned(),
            },
        )
        .expect("upload marked wrapped-key copy");
}

fn fetch_mark_count(
    client: &keystore::KeyServerClient,
    recipient: &keystore::Identity,
    content_id: &str,
    expected_mark: &str,
) -> (usize, bool) {
    match client.fetch_wrapped_key(recipient, content_id) {
        Ok(row) => {
            let text = String::from_utf8(
                STANDARD
                    .decode(row.wrapped_share_blob.as_bytes())
                    .expect("decode wrapped mark"),
            )
            .expect("wrapped mark is UTF-8");
            assert_eq!(text, expected_mark);
            (1, true)
        }
        Err(keystore::Error::HttpStatus { status: 404, .. }) => (0, false),
        Err(error) => panic!("unexpected wrapped-key fetch error: {error}"),
    }
}

fn sender_state(sender: keystore::Identity, client: keystore::KeyServerClient) -> AppState {
    let state = AppState::new();
    state.install_identity(sender);
    *state.keyserver.lock().expect("keyserver lock") = Some(client);
    state
}

fn assert_not_partial(sender_count: usize, receiver_count: usize, context: &str) {
    assert!(
        !matches!((sender_count, receiver_count), (1, 0) | (0, 1)),
        "{context}: forbidden partial both-sides burn state sender={sender_count} receiver={receiver_count}"
    );
}

fn run_crash_scenario(actor: CrashActor, phase: CrashPhase) {
    let service = Task3608Service::default();
    let sender = keystore::generate_identity(format!(
        "task3608-sender-{}-{}-{}",
        actor.label(),
        phase.label(),
        random_hex(4)
    ));
    let receiver = keystore::generate_identity(format!(
        "task3608-receiver-{}-{}-{}",
        actor.label(),
        phase.label(),
        random_hex(4)
    ));
    let mark = format!(
        "TASK3608 BOTH-SIDES MARK actor={} phase={} nonce={}",
        actor.label(),
        phase.label(),
        random_hex(16)
    );
    let sender_content_id = format!(
        "task3608-sender-copy-{}-{}-{}",
        actor.label(),
        phase.label(),
        random_hex(8)
    );
    let receiver_content_id = format!(
        "task3608-receiver-copy-{}-{}-{}",
        actor.label(),
        phase.label(),
        random_hex(8)
    );

    let setup = service.start(4, None);
    let setup_client = setup.client();
    upload_copy(
        &setup_client,
        &sender,
        &sender_content_id,
        &sender.user_id,
        &mark,
    );
    upload_copy(
        &setup_client,
        &sender,
        &receiver_content_id,
        &receiver.user_id,
        &mark,
    );
    let (sender_before_count, sender_before_mark) =
        fetch_mark_count(&setup_client, &sender, &sender_content_id, &mark);
    let (receiver_before_count, receiver_before_mark) =
        fetch_mark_count(&setup_client, &receiver, &receiver_content_id, &mark);
    println!(
        "TASK3608 before actor={} phase={} removal_step=atomic_service_all mark=\"{}\" sender_count={} receiver_count={} sender_mark_present={} receiver_mark_present={}",
        actor.label(),
        phase.label(),
        mark,
        sender_before_count,
        receiver_before_count,
        sender_before_mark,
        receiver_before_mark
    );
    assert_eq!((sender_before_count, receiver_before_count), (1, 1));
    assert!(sender_before_mark && receiver_before_mark);
    setup.join();

    match (actor, phase) {
        (CrashActor::Sender | CrashActor::Receiver, CrashPhase::Before) => {
            println!(
                "TASK3608 crash actor={} phase={} removal_step=atomic_service_all simulated_before_request=true",
                actor.label(),
                phase.label()
            );
        }
        (CrashActor::Sender | CrashActor::Receiver, CrashPhase::After) => {
            let run = service.start(1, None);
            let state = sender_state(sender.clone(), run.client());
            let result = cmd_osl_burn_both_sides_server_copies(&state)
                .expect("both-sides burn before client crash");
            println!(
                "TASK3608 crash actor={} phase={} removal_step=atomic_service_all discarded_success choice=\"{}\" remote_removal_count={}",
                actor.label(),
                phase.label(),
                result.choice,
                result.remote_removal_count
            );
            assert_eq!(result.choice, "Both Sides");
            assert_eq!(result.remote_removal_count, 2);
            run.join();
        }
        (CrashActor::Service, CrashPhase::Before) => {
            let run = service.start(1, Some(ServiceCrash::BeforeRemoval));
            let state = sender_state(sender.clone(), run.client());
            let err = cmd_osl_burn_both_sides_server_copies(&state)
                .expect_err("service crash before removal must not report success");
            println!(
                "TASK3608 crash actor=service phase=before removal_step=atomic_service_all error=\"{}\"",
                err
            );
            run.join();
        }
        (CrashActor::Service, CrashPhase::After) => {
            let run = service.start(1, Some(ServiceCrash::AfterRemoval));
            let state = sender_state(sender.clone(), run.client());
            let err = cmd_osl_burn_both_sides_server_copies(&state)
                .expect_err("service crash after removal must not report success");
            println!(
                "TASK3608 crash actor=service phase=after removal_step=atomic_service_all error=\"{}\"",
                err
            );
            run.join();
        }
    }

    let retry = service.start(5, None);
    let retry_client = retry.client();
    let (sender_after_crash_count, sender_after_crash_mark) =
        fetch_mark_count(&retry_client, &sender, &sender_content_id, &mark);
    let (receiver_after_crash_count, receiver_after_crash_mark) =
        fetch_mark_count(&retry_client, &receiver, &receiver_content_id, &mark);
    println!(
        "TASK3608 after_crash actor={} phase={} removal_step=atomic_service_all sender_count={} receiver_count={} sender_mark_present={} receiver_mark_present={}",
        actor.label(),
        phase.label(),
        sender_after_crash_count,
        receiver_after_crash_count,
        sender_after_crash_mark,
        receiver_after_crash_mark
    );
    assert_not_partial(
        sender_after_crash_count,
        receiver_after_crash_count,
        "after crash",
    );

    let state = sender_state(sender.clone(), retry_client.clone());
    let retry_result =
        cmd_osl_burn_both_sides_server_copies(&state).expect("restart and retry both-sides burn");
    assert_eq!(retry_result.choice, "Both Sides");
    let (sender_after_count, sender_after_mark) =
        fetch_mark_count(&retry_client, &sender, &sender_content_id, &mark);
    let (receiver_after_count, receiver_after_mark) =
        fetch_mark_count(&retry_client, &receiver, &receiver_content_id, &mark);
    println!(
        "TASK3608 success actor={} phase={} removal_step=atomic_service_all choice=\"{}\" remote_removal_count={} sender_count={} receiver_count={} sender_mark_present={} receiver_mark_present={}",
        actor.label(),
        phase.label(),
        retry_result.choice,
        retry_result.remote_removal_count,
        sender_after_count,
        receiver_after_count,
        sender_after_mark,
        receiver_after_mark
    );
    assert_eq!((sender_after_count, receiver_after_count), (0, 0));
    assert!(!sender_after_mark && !receiver_after_mark);
    retry.join();
}

#[test]
fn both_sides_burn_restarts_after_every_crash_without_partial_success() {
    for actor in [
        CrashActor::Sender,
        CrashActor::Receiver,
        CrashActor::Service,
    ] {
        for phase in [CrashPhase::Before, CrashPhase::After] {
            run_crash_scenario(actor, phase);
        }
    }
}
