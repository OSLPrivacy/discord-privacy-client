//! `KeyServerClient` tests. The HTTP wire path is exercised against
//! an in-process tiny mock server (a single-shot TCP listener) so we
//! don't depend on the Node keyserver being live during `cargo test`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keystore::{generate_identity, Error, KeyServerClient};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

#[test]
fn register_request_carries_correct_base64_keys() {
    let id = generate_identity("alice".to_string());
    let req = KeyServerClient::build_register_request(&id);
    assert_eq!(req.user_id, "alice");

    let x25519_decoded = STANDARD.decode(&req.ik_x25519_pub).unwrap();
    assert_eq!(x25519_decoded, id.x25519_public.as_bytes());

    let ed25519_decoded = STANDARD.decode(&req.ik_ed25519_pub).unwrap();
    assert_eq!(ed25519_decoded, id.ed25519_public.as_bytes());

    let mlkem_decoded = STANDARD.decode(&req.ik_mlkem768_pub).unwrap();
    assert_eq!(mlkem_decoded.as_slice(), &id.mlkem_public_bytes[..]);

    let sig_decoded = STANDARD.decode(&req.ik_x25519_signature).unwrap();
    assert_eq!(sig_decoded, b"PROTOTYPE_NO_SIG");
}

#[test]
fn new_accepts_https_url() {
    // Phase B follow-up: HTTPS is required because Railway force-
    // redirects HTTP→HTTPS at the edge. The pre-Phase-B prototype
    // rejected `https://` because it had no TLS stack; reqwest +
    // rustls now handles it.
    KeyServerClient::new("https://example.com").unwrap();
    KeyServerClient::new("https://keyserver.example.com:8443/api").unwrap();
}

#[test]
fn new_parses_host_port_and_base_path() {
    // We don't expose getters on KeyServerClient — instead exercise
    // construction across several URL shapes and confirm none error.
    KeyServerClient::new("http://127.0.0.1:3000").unwrap();
    KeyServerClient::new("http://localhost").unwrap();
    KeyServerClient::new("http://localhost:8080/api").unwrap();
    KeyServerClient::new("http://127.0.0.1:3000/").unwrap();
    // Wrong scheme rejected.
    assert!(KeyServerClient::new("ftp://x").is_err());
    // Malformed URL rejected at construction (defensive parse).
    assert!(KeyServerClient::new("http://").is_err());
}

/// One-shot mock HTTP server: accepts one connection, reads the
/// request, sends back a fixed response. Returns the captured request
/// bytes via the channel.
fn one_shot_server(response: Vec<u8>) -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        // Read until "\r\n\r\n", then if there's a content-length
        // greater than zero read that much body.
        let mut buf = [0u8; 4096];
        let mut acc = Vec::new();
        let header_end = loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break acc.len();
            }
            acc.extend_from_slice(&buf[..n]);
            if let Some(p) = acc.windows(4).position(|w| w == b"\r\n\r\n") {
                break p;
            }
        };
        let header_text = std::str::from_utf8(&acc[..header_end]).unwrap();
        let cl = header_text
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body_so_far = acc[header_end + 4..].len();
        while body_so_far < cl {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            acc.extend_from_slice(&buf[..n]);
            body_so_far += n;
        }
        let _ = tx.send(acc);
        stream.write_all(&response).unwrap();
        // Close the stream by dropping it.
    });
    (port, rx)
}

#[test]
fn register_round_trips_through_mock_server() {
    let response_body = br#"{"user_id":"alice","registered_at":"2026-05-08T10:00:00Z"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let id = generate_identity("alice".to_string());
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client.register(&id).unwrap();
    assert_eq!(resp.user_id, "alice");
    assert_eq!(resp.registered_at.as_deref(), Some("2026-05-08T10:00:00Z"));

    // Confirm what the client sent on the wire. reqwest 0.12 emits
    // lowercase header names per HTTP/1.1 normalization (the wire
    // bytes hyper produces match the on-the-wire form HTTP/2 uses
    // even on HTTP/1.1 connections); assertions are
    // case-insensitive so this stays robust to reqwest's casing
    // choices.
    let req_bytes = rx.recv().unwrap();
    let req_text = std::str::from_utf8(&req_bytes).unwrap();
    let lower = req_text.to_ascii_lowercase();
    assert!(lower.starts_with("post /v1/register http/1.1\r\n"));
    assert!(lower.contains("host: 127.0.0.1:"));
    assert!(lower.contains("content-type: application/json"));
    // The JSON body should mention the user_id.
    assert!(req_text.contains("alice"));
}

#[test]
fn fetch_pubkeys_round_trips_through_mock_server() {
    let response_body = br#"{"user_id":"bob","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","registered_at":"2026-05-08T11:00:00Z","last_rotated_at":null}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client.fetch_pubkeys("bob").unwrap();
    assert_eq!(resp.user_id, "bob");
    assert_eq!(resp.last_rotated_at, None);

    let req_bytes = rx.recv().unwrap();
    let req_text = std::str::from_utf8(&req_bytes).unwrap();
    assert!(req_text
        .to_ascii_lowercase()
        .starts_with("get /v1/pubkeys/bob http/1.1\r\n"));
}

#[test]
fn fetch_pubkeys_url_encodes_special_chars() {
    let response_body = br#"{"user_id":"liam@discord","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","registered_at":"2026-05-08T11:00:00Z","last_rotated_at":null}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let _ = client.fetch_pubkeys("liam@discord").unwrap();

    let req_bytes = rx.recv().unwrap();
    let req_text = std::str::from_utf8(&req_bytes).unwrap();
    assert!(req_text.contains("/v1/pubkeys/liam%40discord"));
}

#[test]
fn http_status_error_propagates_body() {
    let response_body = br#"{"error":"unknown user_id"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 404 Not Found\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(response_body);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.fetch_pubkeys("nobody") {
        Err(Error::HttpStatus { status, body }) => {
            assert_eq!(status, 404);
            assert!(body.contains("unknown user_id"));
        }
        other => panic!("expected HttpStatus error, got {other:?}"),
    }
}

// ---- Phase 9-A2: ratchet bootstrap column ----

#[test]
fn publish_with_ratchet_pub_then_fetch_returns_it() {
    let id = generate_identity("alice".to_string());
    let req = KeyServerClient::build_register_request(&id);
    let ratchet_b64 = req
        .ik_ratchet_initial_pub
        .clone()
        .expect("fresh identity carries ratchet bootstrap pub");
    let ratchet_decoded = STANDARD.decode(&ratchet_b64).unwrap();
    assert_eq!(
        ratchet_decoded.len(),
        32,
        "ratchet bootstrap pub must be 32 bytes (X25519)"
    );

    // Round-trip: server echoes back the column we registered with.
    let echo = format!(
        r#"{{"user_id":"alice","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","ik_ratchet_initial_pub":"{ratchet_b64}","registered_at":"2026-05-08T11:00:00Z","last_rotated_at":null}}"#
    );
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", echo.len()).as_bytes());
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(echo.as_bytes());
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client.fetch_pubkeys("alice").unwrap();
    assert_eq!(
        resp.ik_ratchet_initial_pub.as_deref(),
        Some(ratchet_b64.as_str())
    );
}

#[test]
fn publish_without_ratchet_pub_then_fetch_returns_none() {
    // Older clients may register without the field, or the server
    // may have a NULL column post-migration. Either way the
    // response carries ik_ratchet_initial_pub: null.
    let echo = br#"{"user_id":"bob","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","ik_ratchet_initial_pub":null,"registered_at":"2026-05-08T11:00:00Z","last_rotated_at":null}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", echo.len()).as_bytes());
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(echo);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client.fetch_pubkeys("bob").unwrap();
    assert!(resp.ik_ratchet_initial_pub.is_none());
}

#[test]
fn fetch_against_legacy_response_without_field_parses_as_none() {
    // Pre-A2 server response: the field is absent entirely (not
    // even `null`). serde(default) must let this parse cleanly with
    // None.
    let legacy = br#"{"user_id":"charlie","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","registered_at":"2026-05-08T11:00:00Z","last_rotated_at":null}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", legacy.len()).as_bytes());
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(legacy);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client.fetch_pubkeys("charlie").unwrap();
    assert!(resp.ik_ratchet_initial_pub.is_none());
    assert_eq!(resp.user_id, "charlie");
}
