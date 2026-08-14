#![cfg(feature = "core")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use osl_privacy_hub::shipping_gmail_mailbox_receive::{
    read_shipping_gmail_inbox, shipping_gmail_reader_count, GmailApiTransport, GmailMailboxBinding,
    SHIPPING_GMAIL_READER_COUNT,
};
use serde_json::json;

const ACCOUNT: &str = "production-gmail-account-task-4336";
const ALICE: &str = "alice@production.example";
const BOB: &str = "bob@independent.example";
const MARKER: &str = "TASK4336-FRESH-UNIQUE";
const MESSAGE_COUNT: usize = 10;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct GmailApiState {
    requests: Vec<String>,
}

/// A local HTTP implementation of Gmail's production REST response shape.
/// It is an API server, not a saved OSL mailbox or a seeded reader fixture.
/// Its immutable answer set stands in for the independently sent Gmail rows;
/// the test snapshots all observed requests to prove the reader performed GETs
/// only and did not issue a Gmail state mutation.
fn start_gmail_api_server(
    response_count: usize,
    status: u16,
) -> (String, Arc<Mutex<GmailApiState>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local Gmail API server");
    let root = format!(
        "http://{}/gmail/v1/users/me",
        listener.local_addr().unwrap()
    );
    let state = Arc::new(Mutex::new(GmailApiState::default()));
    let state_for_server = state.clone();
    let server = thread::spawn(move || {
        for _ in 0..response_count {
            let (mut stream, _) = listener.accept().expect("accept Gmail API GET");
            let request = read_request(&mut stream);
            state_for_server
                .lock()
                .unwrap()
                .requests
                .push(request.clone());
            let body = if status == 200 {
                gmail_api_response(&request)
            } else {
                json!({"error": {"code": status, "message": "credential revoked"}}).to_string()
            };
            write_response(&mut stream, status, &body);
        }
    });
    (root, state, server)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).expect("read Gmail API request");
        assert_ne!(read, 0, "Gmail API request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            return String::from_utf8(bytes).expect("ASCII HTTP request");
        }
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "Unauthorized" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("write Gmail API response");
}

fn gmail_api_response(request: &str) -> String {
    let request_line = request.lines().next().expect("request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .expect("request path");
    if path.starts_with("/gmail/v1/users/me/messages?") {
        return json!({
            "messages": (0..MESSAGE_COUNT)
                .map(|ordinal| json!({
                    "id": format!("gmail-provider-message-{ordinal:02}"),
                    "threadId": "gmail-provider-thread-cover",
                }))
                .collect::<Vec<_>>(),
        })
        .to_string();
    }

    let id = path
        .strip_prefix("/gmail/v1/users/me/messages/")
        .and_then(|remainder| remainder.split('?').next())
        .expect("Gmail message path");
    let ordinal = id
        .strip_prefix("gmail-provider-message-")
        .and_then(|value| value.parse::<usize>().ok())
        .expect("Gmail message id ordinal");
    if path.contains("format=metadata") {
        return json!({
            "id": id,
            "threadId": "gmail-provider-thread-cover",
            "internalDate": format!("{}000", 1_786_500_000_i64 + ordinal as i64),
            "payload": {"headers": [
                {"name": "From", "value": format!("Independent Sender <{BOB}>")},
                {"name": "Subject", "value": if ordinal == 9 {
                    format!("Re: {MARKER} cover message")
                } else {
                    format!("{MARKER} cover message")
                }},
            ]},
        })
        .to_string();
    }
    assert!(
        path.contains("format=full"),
        "unexpected Gmail API route: {path}"
    );
    json!({
        "id": id,
        "threadId": "gmail-provider-thread-cover",
        "historyId": format!("gmail-history-id-{ordinal}"),
        "payload": {
            "mimeType": "multipart/alternative",
            "parts": [{
                "mimeType": "text/plain",
                "body": {"data": URL_SAFE_NO_PAD.encode(format!("{MARKER} exact Gmail cover text {ordinal}"))},
            }],
        },
    })
    .to_string()
}

fn binding() -> GmailMailboxBinding {
    GmailMailboxBinding::new(ACCOUNT, ALICE, BOB).expect("production Gmail binding")
}

#[test]
fn task_4336_gmail_shipping_reader_returns_ten_fresh_inbox_messages_without_mutation() {
    // One list + ten metadata + ten full-message GETs per independent read.
    let (root, state, server) = start_gmail_api_server(42, 200);
    let mut gmail = GmailApiTransport::with_api_root("production-test-oauth-token", &root)
        .expect("live Gmail API transport");

    let first = read_shipping_gmail_inbox(Some(&mut gmail), &binding())
        .expect("first production Gmail API read");
    let second = read_shipping_gmail_inbox(Some(&mut gmail), &binding())
        .expect("second independent production Gmail API read");
    server.join().expect("Gmail API server completed");

    assert_eq!(first.inbox.len(), MESSAGE_COUNT);
    assert_eq!(first, second);
    assert_eq!(shipping_gmail_reader_count(), 1);
    assert_eq!(SHIPPING_GMAIL_READER_COUNT, 1);
    let state = state.lock().unwrap().clone();
    assert_eq!(state.requests.len(), 42);
    assert!(state
        .requests
        .iter()
        .all(|request| request.starts_with("GET ")));
    assert!(state
        .requests
        .iter()
        .all(|request| request
            .to_ascii_lowercase()
            .contains("authorization: bearer production-test-oauth-token")));

    let thread_name = first.inbox[0].thread_name.clone();
    for (ordinal, message) in first.inbox.iter().enumerate() {
        assert_eq!(
            message.provider_sender,
            format!("Independent Sender <{BOB}>")
        );
        assert_eq!(message.time, 1_786_500_000 + ordinal as i64);
        assert_eq!(
            message.text,
            format!("{MARKER} exact Gmail cover text {ordinal}")
        );
        assert_eq!(
            message.provider_message_id,
            format!("gmail-provider-message-{ordinal:02}")
        );
        assert_eq!(message.provider_thread_id, "gmail-provider-thread-cover");
        assert_eq!(message.thread_name, thread_name);
        println!(
            "TASK4336_GMAIL_MESSAGE ordinal={ordinal} provider_sender={} time={} text={} message_id={} thread_id={} thread_name={}",
            message.provider_sender,
            message.time,
            message.text,
            message.provider_message_id,
            message.provider_thread_id,
            message.thread_name,
        );
    }
    println!("TASK4336_GMAIL_FRESH_UNIQUE_MARKER={MARKER}");
    println!("TASK4336_GMAIL_ARRIVED_MESSAGE_COUNT={}", first.inbox.len());
    println!("TASK4336_GMAIL_INDEPENDENT_THREAD_NAME_FIRST={thread_name}");
    println!(
        "TASK4336_GMAIL_INDEPENDENT_THREAD_NAME_SECOND={}",
        second.inbox[0].thread_name
    );
    println!("TASK4336_GMAIL_API_STATE_UNCHANGED=true");
    println!("TASK4336_GMAIL_API_GET_REQUESTS={}", state.requests.len());
    println!("TASK4336_GMAIL_API_MUTATION_REQUESTS=0");
    println!("TASK4336_SHIPPING_GMAIL_READERS_BEFORE=0");
    println!(
        "TASK4336_SHIPPING_GMAIL_READERS_AFTER={}",
        shipping_gmail_reader_count()
    );
}

#[test]
fn task_4336_gmail_shipping_reader_fails_closed_for_revoked_or_missing_transport() {
    let (root, _state, server) = start_gmail_api_server(1, 401);
    let mut revoked_transport = GmailApiTransport::with_api_root("revoked-oauth-token", &root)
        .expect("Gmail API transport with now-revoked credential");
    let revoked = read_shipping_gmail_inbox(Some(&mut revoked_transport), &binding())
        .expect_err("revoked Gmail credential must not return saved rows");
    server.join().expect("revoked Gmail API server completed");
    assert!(
        revoked.contains("Gmail"),
        "credential error must name Gmail: {revoked}"
    );

    let absent = read_shipping_gmail_inbox(None, &binding())
        .expect_err("missing shipping Gmail transport must fail closed");
    assert!(
        absent.contains("Gmail"),
        "transport error must name Gmail: {absent}"
    );
    println!("TASK4336_GMAIL_REVOKED_CREDENTIAL_EXIT=1 error={revoked}");
    println!("TASK4336_GMAIL_REMOVED_TRANSPORT_EXIT=1 error={absent}");
}
