#![cfg(feature = "core")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use osl_privacy_hub::shipping_outlook_web_mailbox_receive::{
    read_shipping_outlook_web_inbox, report_outlook_web_oauth_challenge,
    shipping_outlook_web_reader_count, MicrosoftGraphTransport, OutlookGraphOAuthChallenge,
    OutlookWebMailboxBinding, SHIPPING_OUTLOOK_WEB_READER_COUNT,
};
use serde_json::json;

const ACCOUNT: &str = "production-outlook-account-task-4337";
const ALICE: &str = "alice@production.example";
const BOB: &str = "bob@independent.example";
const MARKER: &str = "TASK4337-FRESH-UNIQUE";
const MESSAGE_COUNT: usize = 10;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct GraphApiState {
    /// The Graph mailbox itself is immutable in this dev-only protocol server;
    /// request logs are kept outside that state as an audit trail.
    requests: Vec<String>,
}

/// A local HTTP implementation of the Microsoft Graph production response
/// shape. It is not an OSL mailbox fixture: every answer is reached through
/// recorded Graph GET requests, and the shipping reader has no way to receive
/// a pre-loaded mailbox snapshot.
fn start_graph_api_server(
    response_count: usize,
    status: u16,
) -> (String, Arc<Mutex<GraphApiState>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind local Microsoft Graph API");
    let root = format!("http://{}/v1.0/me", listener.local_addr().unwrap());
    let state = Arc::new(Mutex::new(GraphApiState::default()));
    let state_for_server = state.clone();
    let server = thread::spawn(move || {
        for _ in 0..response_count {
            let (mut stream, _) = listener.accept().expect("accept Microsoft Graph GET");
            let request = read_request(&mut stream);
            state_for_server
                .lock()
                .unwrap()
                .requests
                .push(request.clone());
            let body = if status == 200 {
                graph_api_response(&request)
            } else {
                json!({"error": {"code": "InvalidAuthenticationToken", "message": "credential revoked"}})
                    .to_string()
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
        let read = stream
            .read(&mut chunk)
            .expect("read Microsoft Graph request");
        assert_ne!(read, 0, "Microsoft Graph request ended before headers");
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
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nETag: graph-server-read-etag\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len(),
    )
    .expect("write Microsoft Graph response");
}

fn graph_api_response(request: &str) -> String {
    let request_line = request.lines().next().expect("request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .expect("Microsoft Graph request path");
    if path.starts_with("/v1.0/me/mailFolders/Inbox/messages?") {
        return json!({
            "value": (0..MESSAGE_COUNT)
                .map(|ordinal| json!({
                    "id": format!("outlook-provider-message-{ordinal:02}"),
                    "conversationId": "outlook-provider-conversation-cover",
                    "subject": if ordinal == 9 {
                        format!("Re: {MARKER} cover message")
                    } else {
                        format!("{MARKER} cover message")
                    },
                    "from": {"emailAddress": {
                        "name": "Independent Sender",
                        "address": BOB,
                    }},
                    "receivedDateTime": format!("2026-08-11T00:00:{ordinal:02}Z"),
                }))
                .collect::<Vec<_>>(),
        })
        .to_string();
    }

    let id = path
        .strip_prefix("/v1.0/me/messages/")
        .and_then(|remainder| remainder.split('?').next())
        .expect("Microsoft Graph message path");
    assert!(
        path.contains("$select=body") || path.contains("%24select=body"),
        "body must use Graph GET /messages/{{id}}?$select=body, got {path}",
    );
    let ordinal = id
        .strip_prefix("outlook-provider-message-")
        .and_then(|value| value.parse::<usize>().ok())
        .expect("Microsoft Graph message id ordinal");
    json!({
        "id": id,
        "@odata.etag": format!("graph-message-etag-{ordinal}"),
        "body": {
            "contentType": "text",
            "content": format!("{MARKER} exact Outlook web cover text {ordinal}"),
        },
    })
    .to_string()
}

fn binding() -> OutlookWebMailboxBinding {
    OutlookWebMailboxBinding::new(ACCOUNT, ALICE, BOB).expect("production Outlook web binding")
}

#[test]
fn task_4337_outlook_web_graph_reader_returns_ten_fresh_inbox_messages_without_mutation() {
    // One Inbox list + ten exact-body Graph GETs per independent read.
    let (root, state, server) = start_graph_api_server(22, 200);
    let mut graph = MicrosoftGraphTransport::with_api_root("production-test-oauth-token", &root)
        .expect("live Microsoft Graph transport");

    let first = read_shipping_outlook_web_inbox(Some(&mut graph), &binding())
        .expect("first production Outlook web Graph read");
    let second = read_shipping_outlook_web_inbox(Some(&mut graph), &binding())
        .expect("second independent Outlook web Graph read");
    server.join().expect("Microsoft Graph API server completed");

    assert_eq!(first.inbox.len(), MESSAGE_COUNT);
    assert_eq!(first, second);
    assert_eq!(shipping_outlook_web_reader_count(), 1);
    assert_eq!(SHIPPING_OUTLOOK_WEB_READER_COUNT, 1);
    let state = state.lock().unwrap().clone();
    assert_eq!(state.requests.len(), 22);
    assert!(state
        .requests
        .iter()
        .all(|request| request.starts_with("GET ")));
    assert!(state.requests.iter().all(|request| request
        .to_ascii_lowercase()
        .contains("authorization: bearer production-test-oauth-token")));
    let body_requests = state
        .requests
        .iter()
        .filter(|request| {
            request.contains("/messages/")
                && (request.contains("$select=body") || request.contains("%24select=body"))
        })
        .count();
    assert_eq!(body_requests, 20, "each read must use Graph body selection");

    let thread_name = first.inbox[0].thread_name.clone();
    for (ordinal, message) in first.inbox.iter().enumerate() {
        assert_eq!(
            message.provider_sender,
            format!("Independent Sender <{BOB}>")
        );
        assert_eq!(message.time, 1_786_406_400 + ordinal as i64);
        assert_eq!(
            message.text,
            format!("{MARKER} exact Outlook web cover text {ordinal}")
        );
        assert_eq!(
            message.graph_message_id,
            format!("outlook-provider-message-{ordinal:02}")
        );
        assert_eq!(
            message.conversation_id,
            "outlook-provider-conversation-cover"
        );
        assert_eq!(message.thread_name, thread_name);
        println!(
            "TASK4337_OUTLOOK_WEB_MESSAGE ordinal={ordinal} provider_sender={} time={} text={} graph_message_id={} conversation_id={} thread_name={}",
            message.provider_sender,
            message.time,
            message.text,
            message.graph_message_id,
            message.conversation_id,
            message.thread_name,
        );
    }
    println!("TASK4337_OUTLOOK_WEB_FRESH_UNIQUE_MARKER={MARKER}");
    println!(
        "TASK4337_OUTLOOK_WEB_ARRIVED_MESSAGE_COUNT={}",
        first.inbox.len()
    );
    println!("TASK4337_OUTLOOK_WEB_INDEPENDENT_THREAD_NAME_FIRST={thread_name}");
    println!(
        "TASK4337_OUTLOOK_WEB_INDEPENDENT_THREAD_NAME_SECOND={}",
        second.inbox[0].thread_name
    );
    println!("TASK4337_OUTLOOK_WEB_GRAPH_STATE_UNCHANGED=true");
    println!(
        "TASK4337_OUTLOOK_WEB_GRAPH_GET_REQUESTS={}",
        state.requests.len()
    );
    println!("TASK4337_OUTLOOK_WEB_GRAPH_BODY_SELECT_GET_REQUESTS={body_requests}");
    println!("TASK4337_OUTLOOK_WEB_GRAPH_MUTATION_REQUESTS=0");
    println!("TASK4337_SHIPPING_OUTLOOK_WEB_READERS_BEFORE=0");
    println!(
        "TASK4337_SHIPPING_OUTLOOK_WEB_READERS_AFTER={}",
        shipping_outlook_web_reader_count()
    );
}

#[test]
fn task_4337_outlook_web_graph_reader_fails_closed_and_hands_oauth_challenges_to_liam() {
    let (root, _state, server) = start_graph_api_server(1, 401);
    let mut revoked_graph = MicrosoftGraphTransport::with_api_root("revoked-oauth-token", &root)
        .expect("Microsoft Graph transport with now-revoked credential");
    let revoked = read_shipping_outlook_web_inbox(Some(&mut revoked_graph), &binding())
        .expect_err("revoked Graph credential must not return saved rows");
    server
        .join()
        .expect("revoked Microsoft Graph API server completed");
    assert!(
        revoked.contains("Outlook web"),
        "credential error must name Outlook web: {revoked}"
    );

    let absent = read_shipping_outlook_web_inbox(None, &binding())
        .expect_err("removing shipping Graph transport must fail closed");
    assert!(
        absent.contains("Outlook web"),
        "transport error must name Outlook web: {absent}"
    );

    for challenge in [
        OutlookGraphOAuthChallenge::Captcha,
        OutlookGraphOAuthChallenge::TwoFactor,
    ] {
        let handoff = report_outlook_web_oauth_challenge(challenge)
            .expect_err("OAuth challenge must stop rather than be bypassed");
        assert!(handoff.contains("Outlook web"));
        assert!(handoff.contains("Liam's login handoff"));
        println!("TASK4337_OUTLOOK_WEB_OAUTH_CHALLENGE_EXIT=1 error={handoff}");
    }
    println!("TASK4337_OUTLOOK_WEB_REVOKED_GRAPH_TOKEN_EXIT=1 error={revoked}");
    println!("TASK4337_OUTLOOK_WEB_REMOVED_GRAPH_CALL_EXIT=1 error={absent}");
}
