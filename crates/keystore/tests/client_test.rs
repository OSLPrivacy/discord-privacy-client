//! `KeyServerClient` tests. The HTTP wire path is exercised against
//! an in-process tiny mock server (a single-shot TCP listener) so we
//! don't depend on the Node keyserver being live during `cargo test`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use keystore::{generate_identity, Error, KeyServerClient, WrappedKeyUpload};
use sha2::Digest;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
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

    // REGISTER-FIX: registration_sig is now a real Ed25519 signature
    // over REG_MSG, verifiable against the submitted ik_ed25519_pub.
    let sig_decoded = STANDARD.decode(&req.registration_sig).unwrap();
    assert_eq!(sig_decoded.len(), 64, "Ed25519 signature is 64 bytes");
    // Derive the message from the REQUEST, so this keeps verifying whichever form
    // production signed. build_register_request now advertises a capability bitmap
    // and signs reg_msg_with_capabilities; verifying against the bitmap-less
    // reg_msg made a valid signature look forged.
    let msg = match req.rn_capabilities {
        Some(capabilities) => keystore::client::reg_msg_with_capabilities(
            &req.user_id,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
            capabilities,
        ),
        None => keystore::client::reg_msg(
            &req.user_id,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
        ),
    };
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_decoded);
    let ok = crypto::ed25519::verify(
        &id.ed25519_public,
        &msg,
        &crypto::ed25519::Signature::from_bytes(sig_arr),
    )
    .unwrap();
    assert!(ok, "registration_sig must verify against ik_ed25519_pub");
    assert!(req.rotation.is_none(), "Case A/B carries no rotation proof");
    // build_register_request advertises a capability bitmap and signs over it via
    // reg_msg_with_capabilities. The invariant is not "advertise nothing" -- it is
    // "never advertise capabilities the signature does not cover", which the
    // verification above already proves, because the message was rebuilt from
    // req.rn_capabilities itself.
    if let Some(capabilities) = req.rn_capabilities {
        assert_eq!(
            capabilities,
            keystore::client::CLIENT_RN_CAPABILITY_FLOOR,
            "an advertised bitmap must be the signed client floor"
        );
    }
}

#[test]
fn register_request_serializes_optional_rn_capabilities_bitmap() {
    let id = generate_identity("alice".to_string());
    let mut req = KeyServerClient::build_register_request(&id);

    // A bitmap the builder sets must reach the wire; an absent one must stay absent.
    let built = serde_json::to_value(&req).unwrap();
    match req.rn_capabilities {
        Some(capabilities) => assert_eq!(
            built
                .get("rn_capabilities")
                .and_then(serde_json::Value::as_u64),
            Some(u64::from(capabilities)),
            "an advertised bitmap must reach the wire"
        ),
        None => assert!(
            built.get("rn_capabilities").is_none(),
            "absent capability bitmap must stay absent on the wire"
        ),
    }

    req.rn_capabilities = Some(keystore::client::RN_CAP_WIRE_RN);
    let advertised = serde_json::to_value(&req).unwrap();
    assert_eq!(
        advertised
            .get("rn_capabilities")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(keystore::client::RN_CAP_WIRE_RN))
    );
}

/// GATE: REG_MSG byte format. This exact vector is mirrored in
/// `keyserver-cf/test/unit/signed-request.test.ts`. If these two
/// disagree by one byte, EVERY registration fails — so both sides
/// pin the identical expected string here.
#[test]
fn reg_msg_byte_format_is_pinned_and_mirrored() {
    let msg = keystore::client::reg_msg(
        "900000000000000001",
        "WdsAAA==",
        "ZWQyNTUx",
        "bWxrZW0=",
        Some("cmF0Y2g="),
    );
    let expected = "OSL-REGISTER-v1\n900000000000000001\nWdsAAA==\nZWQyNTUx\nbWxrZW0=\ncmF0Y2g=";
    assert_eq!(String::from_utf8(msg).unwrap(), expected);

    // ratchet absent → empty last component, no trailing newline.
    let msg_no_ratchet = keystore::client::reg_msg("u", "x", "e", "m", None);
    assert_eq!(
        String::from_utf8(msg_no_ratchet).unwrap(),
        "OSL-REGISTER-v1\nu\nx\ne\nm\n"
    );
}

/// GATE: a rotation request is signed correctly by BOTH keys.
#[test]
fn rotation_request_dual_signs_old_and_new() {
    let old = generate_identity("alice".to_string());
    let new = generate_identity("alice".to_string());
    let req = KeyServerClient::build_rotation_request(&old, &new);
    let rot = req.rotation.as_ref().expect("rotation proof present");

    // user_id is immutable across rotation; new keys on top level.
    assert_eq!(req.user_id, "alice");
    assert_eq!(
        STANDARD.decode(&req.ik_ed25519_pub).unwrap(),
        new.ed25519_public.as_bytes()
    );
    assert_eq!(
        STANDARD.decode(&rot.prev_ik_ed25519_pub).unwrap(),
        old.ed25519_public.as_bytes()
    );

    // registration_sig verifies under the NEW key over REG_MSG.
    // Same as the plain registration path: rebuild from the request so this verifies
    // whichever form was signed, bitmap or not.
    let reg = match req.rn_capabilities {
        Some(capabilities) => keystore::client::reg_msg_with_capabilities(
            &req.user_id,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
            capabilities,
        ),
        None => keystore::client::reg_msg(
            &req.user_id,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
        ),
    };
    let mut s = [0u8; 64];
    s.copy_from_slice(&STANDARD.decode(&req.registration_sig).unwrap());
    assert!(crypto::ed25519::verify(
        &new.ed25519_public,
        &reg,
        &crypto::ed25519::Signature::from_bytes(s)
    )
    .unwrap());

    // prev_sig verifies under the OLD key over ROT_MSG.
    // ROT_MSG has the same two forms as REG_MSG; rebuild from the request.
    let rotm = match req.rn_capabilities {
        Some(capabilities) => keystore::client::rot_msg_with_capabilities(
            &req.user_id,
            &rot.prev_ik_ed25519_pub,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
            capabilities,
        ),
        None => keystore::client::rot_msg(
            &req.user_id,
            &rot.prev_ik_ed25519_pub,
            &req.ik_x25519_pub,
            &req.ik_ed25519_pub,
            &req.ik_mlkem768_pub,
            req.ik_ratchet_initial_pub.as_deref(),
        ),
    };
    let mut p = [0u8; 64];
    p.copy_from_slice(&STANDARD.decode(&rot.prev_sig).unwrap());
    assert!(crypto::ed25519::verify(
        &old.ed25519_public,
        &rotm,
        &crypto::ed25519::Signature::from_bytes(p)
    )
    .unwrap());
}

#[test]
fn new_accepts_only_the_pinned_remote_origin() {
    KeyServerClient::new("https://keyserver.oslprivacy.com").unwrap();
    KeyServerClient::new("https://keyserver.oslprivacy.com:443").unwrap();
    assert!(KeyServerClient::new("http://keyserver.oslprivacy.com").is_err());
    assert!(KeyServerClient::new("https://keyserver.oslprivacy.com.evil.test").is_err());
    assert!(KeyServerClient::new("https://example.com").is_err());
    assert!(KeyServerClient::new("https://keyserver.oslprivacy.com/api").is_err());
    assert!(KeyServerClient::new("https://user@keyserver.oslprivacy.com").is_err());
}

#[test]
fn new_parses_host_port_and_base_path() {
    // We don't expose getters on KeyServerClient — instead exercise
    // construction across several URL shapes and confirm none error.
    KeyServerClient::new("http://127.0.0.1:3000").unwrap();
    KeyServerClient::new("https://127.0.0.1:8443").unwrap();
    KeyServerClient::new("http://[::1]:8080/api").unwrap();
    KeyServerClient::new("http://127.0.0.1:3000/").unwrap();
    assert!(KeyServerClient::new("http://localhost:8080").is_err());
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

fn multi_response_server(responses: Vec<Vec<u8>>) -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0u8; 4096];
            let mut acc = Vec::new();
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0, "request ended before headers");
                acc.extend_from_slice(&buf[..n]);
                if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let _ = tx.send(acc);
            stream.write_all(&response).unwrap();
        }
    });
    (port, rx)
}

type ResponseHandler = Box<dyn FnMut(&[u8]) -> Vec<u8> + Send>;

fn multi_response_server_from_requests(
    handlers: Vec<ResponseHandler>,
) -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for mut handler in handlers {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut buf = [0u8; 4096];
            let mut acc = Vec::new();
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0, "request ended before headers");
                acc.extend_from_slice(&buf[..n]);
                if acc.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let response = handler(&acc);
            let _ = tx.send(acc);
            stream.write_all(&response).unwrap();
        }
    });
    (port, rx)
}

fn active_account_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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

fn query_value(target: &str, key: &str) -> String {
    let url = reqwest::Url::parse(&format!("http://test{target}")).unwrap();
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| panic!("missing query key {key}"))
}

#[test]
fn ownership_challenge_round_trips_through_mock_worker() {
    let nonce = [0x5au8; 32];
    let response_body = format!(
        r#"{{"challenge_version":1,"service":"discord","nonce":"{}","service_account_id":"123456789012345678","owner_user_id":"osl1_owner","issued_at_unix_seconds":1800000000,"expires_at_unix_seconds":1800000300,"spent":false}}"#,
        STANDARD.encode(nonce)
    );
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(response_body.as_bytes());
    let (port, rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let challenge = client
        .request_ownership_challenge("123456789012345678", "osl1_owner", true)
        .unwrap();
    assert_eq!(challenge.nonce(), &nonce);
    assert!(challenge.binds("123456789012345678", "osl1_owner"));
    assert_eq!(challenge.issued_at_unix_seconds(), 1_800_000_000);
    assert_eq!(challenge.expires_at_unix_seconds(), 1_800_000_300);
    assert!(!challenge.is_spent());

    let request = String::from_utf8(rx.recv().unwrap()).unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(lower.starts_with("post /v1/account-ownership/challenge http/1.1\r\n"));
    assert!(lower.contains("content-type: application/json"));
    assert!(!lower.contains("authorization:"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["service"], "discord");
    assert_eq!(value["service_account_id"], "123456789012345678");
    assert_eq!(value["owner_user_id"], "osl1_owner");
    assert_eq!(value["consent"], true);
    assert_eq!(value.as_object().unwrap().len(), 4);
}

#[test]
fn ownership_challenge_refuses_unbound_or_malformed_responses() {
    let mismatched = format!(
        r#"{{"challenge_version":1,"service":"discord","nonce":"{}","service_account_id":"123456789012345678","owner_user_id":"other-owner","issued_at_unix_seconds":1800000000,"expires_at_unix_seconds":1800000300,"spent":false}}"#,
        STANDARD.encode([0x42u8; 32])
    );
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n\r\n", mismatched.len()).as_bytes());
    response.extend_from_slice(mismatched.as_bytes());
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.request_ownership_challenge("123456789012345678", "osl1_owner", true) {
        Err(Error::Transport(message)) => {
            assert!(message.contains("binding"));
            assert!(!message.contains("123456789012345678"));
            assert!(!message.contains("other-owner"));
            assert!(!message.contains("osl1_owner"));
        }
        other => panic!("expected categorical binding refusal, got {other:?}"),
    }

    let short_nonce = r#"{"challenge_version":1,"service":"discord","nonce":"AQ==","service_account_id":"123456789012345678","owner_user_id":"osl1_owner","issued_at_unix_seconds":1800000000,"expires_at_unix_seconds":1800000300,"spent":false}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n\r\n", short_nonce.len()).as_bytes());
    response.extend_from_slice(short_nonce.as_bytes());
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.request_ownership_challenge("123456789012345678", "osl1_owner", true) {
        Err(Error::Transport(message)) => {
            assert!(message.contains("nonce length"));
            assert!(!message.contains("AQ=="));
            assert!(!message.contains("123456789012345678"));
        }
        other => panic!("expected categorical nonce refusal, got {other:?}"),
    }
}

#[test]
fn ownership_challenge_http_status_error_redacts_response_body() {
    let response_body = br#"{"error":"account 123456789012345678 belongs to osl1_owner"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 409 Conflict\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(response_body);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.request_ownership_challenge("123456789012345678", "osl1_owner", true) {
        Err(Error::HttpStatus { status, body }) => {
            assert_eq!(status, 409);
            assert_eq!(body, "ownership challenge request refused");
            assert!(!body.contains("123456789012345678"));
            assert!(!body.contains("osl1_owner"));
        }
        other => panic!("expected redacted HTTP status error, got {other:?}"),
    }
}

#[test]
fn ownership_challenge_refuses_missing_request_binding_without_wire_call() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.request_ownership_challenge("", "osl1_owner", true) {
        Err(Error::Transport(message)) => {
            assert!(message.contains("incomplete"));
            assert!(!message.contains("osl1_owner"));
        }
        other => panic!("expected incomplete binding refusal, got {other:?}"),
    }
    match client.request_ownership_challenge("123456789012345678", "", true) {
        Err(Error::Transport(message)) => {
            assert!(message.contains("incomplete"));
            assert!(!message.contains("123456789012345678"));
        }
        other => panic!("expected incomplete binding refusal, got {other:?}"),
    }
    match client.request_ownership_challenge("123456789012345678", "osl1_owner", false) {
        Err(Error::Transport(message)) => {
            assert!(message.contains("consent"));
            assert!(!message.contains("123456789012345678"));
            assert!(!message.contains("osl1_owner"));
        }
        other => panic!("expected absent consent refusal, got {other:?}"),
    }
}

/// Build a proof for `service_account_id` owned by `identity`, from a
/// challenge shaped exactly like the one the keyserver issues.
fn ownership_proof_for(
    identity: &keystore::Identity,
    service_account_id: &str,
) -> (keystore::AccountOwnershipProof, [u8; 32]) {
    let nonce = [0x3cu8; 32];
    let mut challenge = keystore::ProofChallenge::new(
        nonce,
        service_account_id,
        &identity.user_id,
        1_800_000_000,
        1_800_000_300,
    )
    .expect("challenge lifetime is valid");
    let proof =
        keystore::AccountOwnershipProof::from_challenge(identity, &mut challenge, 1_800_000_001)
            .expect("owner can answer its own challenge");
    (proof, nonce)
}

#[test]
fn ownership_proof_submission_round_trips_through_mock_worker() {
    let identity = generate_identity("osl1_owner".to_string());
    let (proof, nonce) = ownership_proof_for(&identity, "123456789012345678");

    let response_body = br#"{"result":"account_ownership_proof_recorded","service":"discord","owner_user_id":"osl1_owner","verified_at_unix_seconds":1800000005}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let verified_at = client
        .submit_ownership_proof("123456789012345678", "osl1_owner", &proof)
        .expect("mock worker accepts the proof");
    assert_eq!(verified_at, 1_800_000_005);

    let request = String::from_utf8(rx.recv().unwrap()).unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(lower.starts_with("post /v1/account-ownership/proof http/1.1\r\n"));
    assert!(lower.contains("content-type: application/json"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 4);
    assert_eq!(value["service"], "discord");
    assert_eq!(value["service_account_id"], "123456789012345678");
    assert_eq!(value["owner_user_id"], "osl1_owner");
    assert_eq!(value["proof"]["platform_id"], "123456789012345678");
    assert_eq!(
        value["proof"]["proof_type"],
        "ed25519_identity_challenge_v1"
    );
    assert_eq!(value["proof"]["e"]["owner_user_id"], "osl1_owner");
    assert_eq!(value["proof"]["e"]["nonce_b64"], STANDARD.encode(nonce));
    assert_eq!(value["proof"]["e"]["issued_at_unix_seconds"], 1_800_000_000u64);
    assert_eq!(
        value["proof"]["e"]["expires_at_unix_seconds"],
        1_800_000_300u64
    );
    let signature = value["proof"]["e"]["signature_b64"].as_str().unwrap();
    assert_eq!(STANDARD.decode(signature).unwrap().len(), 64);
}

#[test]
fn ownership_proof_submission_surfaces_a_server_refusal() {
    let identity = generate_identity("osl1_owner".to_string());
    let (proof, _) = ownership_proof_for(&identity, "123456789012345678");

    let response_body =
        br#"{"error":"account ownership proof rejected: proof_replayed 123456789012345678"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 409 Conflict\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(response_body);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.submit_ownership_proof("123456789012345678", "osl1_owner", &proof) {
        Err(Error::HttpStatus { status, body }) => {
            assert_eq!(status, 409);
            assert_eq!(body, "ownership proof submission refused");
            assert!(!body.contains("123456789012345678"));
        }
        other => panic!("expected a surfaced server refusal, got {other:?}"),
    }
}

#[test]
fn ownership_proof_submission_refuses_unbound_material_without_wire_call() {
    let identity = generate_identity("osl1_owner".to_string());
    let (proof, _) = ownership_proof_for(&identity, "123456789012345678");

    // Nothing is listening: any refusal that arrives is a local one.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();

    match client.submit_ownership_proof("999999999999999999", "osl1_owner", &proof) {
        Err(Error::Transport(message)) => assert!(message.contains("different account")),
        other => panic!("expected a different-account refusal, got {other:?}"),
    }
    match client.submit_ownership_proof("123456789012345678", "osl1_other", &proof) {
        Err(Error::Transport(message)) => assert!(message.contains("different owner")),
        other => panic!("expected a different-owner refusal, got {other:?}"),
    }
    match client.submit_ownership_proof("", "osl1_owner", &proof) {
        Err(Error::Transport(message)) => assert!(message.contains("incomplete")),
        other => panic!("expected an incomplete-binding refusal, got {other:?}"),
    }
}

#[test]
fn prekey_fetch_carries_registered_identity_signature() {
    let response_body = br#"{"user_id":"bob","ik_x25519_pub":"AA","ik_ed25519_pub":"CC","ik_mlkem768_pub":"BB","ik_ratchet_initial_pub":null,"spk_pub":"DD","spk_signature":"EE","spk_rotated_at":"2026-05-08T11:00:00Z","opk":null,"remaining_opk_count":0}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let requester = generate_identity("alice".to_string());
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}"))
        .unwrap()
        .with_client_token(Some("distributed-client-token".to_string()));
    client.fetch_prekey_bundle(&requester, "bob").unwrap();

    let request = String::from_utf8(rx.recv().unwrap()).unwrap();
    let target = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    assert!(target.starts_with("/v1/prekey-bundle/bob?"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    assert_eq!(query_value(target, "requester_id"), "alice");
    assert_eq!(query_value(target, "recipient_id"), "bob");
    let ts = query_value(target, "ts").parse::<i64>().unwrap();
    let sig = STANDARD.decode(query_value(target, "sig")).unwrap();
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&sig);
    let message = keystore::canonical_prekey_bundle_get_bytes("alice", "bob", ts);
    assert!(crypto::ed25519::verify(
        &requester.ed25519_public,
        &message,
        &crypto::ed25519::Signature::from_bytes(sig_bytes),
    )
    .unwrap());
}

#[test]
fn wrapped_key_fetch_binds_recipient_and_content_id() {
    let response_body = br#"{"content_id":"msg-1","content_type":"text","system_message_kind":null,"sender_id":"alice","recipient_id":"bob","session_version":1,"share_index":0,"wrapped_share_blob":"AQ==","blob_version":1,"single_use":true,"display_duration_seconds":10,"expires_at":"2026-05-08T11:05:00Z","created_at":"2026-05-08T11:00:00Z"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let recipient = generate_identity("bob".to_string());
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let wrapped = client.fetch_wrapped_key(&recipient, "msg-1").unwrap();
    assert_eq!(wrapped.recipient_id, "bob");
    assert!(wrapped.single_use);

    let request = String::from_utf8(rx.recv().unwrap()).unwrap();
    let target = request
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap();
    assert!(target.starts_with("/v1/wrapped-keys/msg-1?"));
    let ts = query_value(target, "ts").parse::<i64>().unwrap();
    let sig = STANDARD.decode(query_value(target, "sig")).unwrap();
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&sig);
    let message = keystore::canonical_wrapped_key_get_bytes("bob", "bob", "msg-1", ts);
    assert!(crypto::ed25519::verify(
        &recipient.ed25519_public,
        &message,
        &crypto::ed25519::Signature::from_bytes(sig_bytes),
    )
    .unwrap());
}

#[test]
fn wrapped_key_post_carries_sender_identity_signature_without_bearer() {
    let response_body = br#"{"content_id":"msg-1"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 201 Created\r\n");
    response
        .extend_from_slice(format!("Content-Length: {}\r\n\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let sender = generate_identity("alice".to_string());
    let upload = WrappedKeyUpload {
        content_id: "msg-1".into(),
        content_type: "text".into(),
        system_message_kind: None,
        recipient_id: "bob".into(),
        session_version: 1,
        share_index: 0,
        wrapped_share_blob: "AQIDBA==".into(),
        blob_version: 1,
        single_use: false,
        display_duration_seconds: None,
        expires_at: "2026-07-18T00:00:00.000Z".into(),
    };
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let result = client.post_wrapped_key(&sender, &upload).unwrap();
    assert_eq!(result.content_id, "msg-1");

    let request = String::from_utf8(rx.recv().unwrap()).unwrap();
    assert!(request
        .to_ascii_lowercase()
        .starts_with("post /v1/wrapped-keys http/1.1\r\n"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let value: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(value["sender_id"], "alice");
    let timestamp_ms = value["timestamp_ms"].as_i64().unwrap();
    let signature = STANDARD
        .decode(value["sender_signature_b64"].as_str().unwrap())
        .unwrap();
    let mut signature_bytes = [0u8; 64];
    signature_bytes.copy_from_slice(&signature);
    let canonical = keystore::canonical_wrapped_key_post_bytes("alice", &upload, timestamp_ms);
    assert!(crypto::ed25519::verify(
        &sender.ed25519_public,
        &canonical,
        &crypto::ed25519::Signature::from_bytes(signature_bytes),
    )
    .unwrap());
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

// ---- F2.1: license/validate ----

/// All seven statuses the keyserver may surface must deserialise
/// cleanly. The type is intentionally a free `String` so the
/// keystore crate doesn't have to ship a new variant every time
/// the keyserver's state machine grows; consuming layers
/// (F2.2 `LicenseState`) do the mapping.
#[test]
fn license_validate_response_deserialises_all_status_values() {
    use keystore::LicenseValidateResponse;
    for status in [
        "ACTIVE",
        "GRACE",
        "CANCELLED",
        "EXPIRED",
        "REVOKED",
        "UNKNOWN",
        "PENDING",
    ] {
        let body = format!(
            r#"{{"status":"{status}","current_period_end":1800000000,"checksum_ok":true}}"#
        );
        let resp: LicenseValidateResponse =
            serde_json::from_str(&body).unwrap_or_else(|e| panic!("deserialise {status}: {e}"));
        assert_eq!(resp.status, status);
        assert_eq!(resp.current_period_end, Some(1_800_000_000));
        assert!(resp.checksum_ok);
    }
}

#[test]
fn license_validate_response_handles_null_current_period_end() {
    use keystore::LicenseValidateResponse;
    let body = r#"{"status":"PENDING","current_period_end":null,"checksum_ok":true}"#;
    let resp: LicenseValidateResponse = serde_json::from_str(body).unwrap();
    assert_eq!(resp.status, "PENDING");
    assert_eq!(resp.current_period_end, None);
    assert!(resp.checksum_ok);
}

#[test]
fn license_validate_response_handles_missing_current_period_end() {
    // `#[serde(default)]` on current_period_end lets older keyserver
    // responses that omit the field entirely still deserialise.
    use keystore::LicenseValidateResponse;
    let body = r#"{"status":"UNKNOWN","checksum_ok":false}"#;
    let resp: LicenseValidateResponse = serde_json::from_str(body).unwrap();
    assert_eq!(resp.status, "UNKNOWN");
    assert_eq!(resp.current_period_end, None);
    assert!(!resp.checksum_ok);
}

#[test]
fn validate_license_round_trips_through_mock_server() {
    let response_body =
        br#"{"status":"ACTIVE","current_period_end":1800000000,"checksum_ok":true}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(response_body);
    let (port, rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let resp = client
        .validate_license("OSL-2222-3333-4444-5555")
        .expect("validate_license should succeed against mock");
    assert_eq!(resp.status, "ACTIVE");
    assert_eq!(resp.current_period_end, Some(1_800_000_000));
    assert!(resp.checksum_ok);

    // Wire check: POST to /v1/license/validate with the JSON body
    // shape the keyserver expects.
    let req_bytes = rx.recv().unwrap();
    let req_text = std::str::from_utf8(&req_bytes).unwrap();
    let lower = req_text.to_ascii_lowercase();
    assert!(lower.starts_with("post /v1/license/validate http/1.1\r\n"));
    assert!(lower.contains("content-type: application/json"));
    assert!(req_text.contains(r#""license_key":"OSL-2222-3333-4444-5555""#));
}

#[test]
fn validate_license_propagates_keyserver_unreachable_as_transport_error() {
    // Bind a port, then close immediately so a connect attempt
    // either races or hits a closed port. Either way, the client
    // surfaces the error as Error::Transport (NOT HttpStatus) —
    // F2.4's offline-grace logic depends on telling these apart.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.validate_license("OSL-X") {
        Err(Error::Transport(_)) => {
            // Expected — caller can match on this variant to
            // honour the cached state during offline grace.
        }
        other => panic!("expected Error::Transport, got {other:?}"),
    }
}

#[test]
fn validate_license_propagates_non_2xx_as_http_status_error() {
    // Rate-limit / server-side rejection path. F2.4 treats this
    // as "keyserver answered, cached state is stale" — distinct
    // from the unreachable case.
    let response_body = br#"{"error":"rate_limited"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 429 Too Many Requests\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", response_body.len()).as_bytes());
    response.extend_from_slice(b"Retry-After: 30\r\n\r\n");
    response.extend_from_slice(response_body);
    let (port, _rx) = one_shot_server(response);

    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    match client.validate_license("OSL-X") {
        Err(Error::HttpStatus { status, body }) => {
            assert_eq!(status, 429);
            assert!(body.contains("rate_limited"));
        }
        other => panic!("expected Error::HttpStatus, got {other:?}"),
    }
}

// ---------------------------------------------------------------
// Signed protocol-capability advertisement (keyserver 0026) — the
// client half of layer L1 in
// `crates/osl-ratchet-next/src/negotiate.rs`.
// ---------------------------------------------------------------

/// GATE: the extended REG_MSG byte format. Mirrored in
/// `keyserver-cf/test/unit/signed-request.test.ts`. A one-byte
/// disagreement makes every capability-advertising registration fail.
#[test]
fn reg_msg_with_capabilities_byte_format_is_pinned_and_mirrored() {
    let msg = keystore::client::reg_msg_with_capabilities(
        "900000000000000001",
        "WdsAAA==",
        "ZWQyNTUx",
        "bWxrZW0=",
        Some("cmF0Y2g="),
        1,
    );
    let expected = "OSL-REGISTER-v1\n900000000000000001\nWdsAAA==\nZWQyNTUx\nbWxrZW0=\ncmF0Y2g=\n1";
    assert_eq!(String::from_utf8(msg).unwrap(), expected);

    // The legacy form is a strict prefix of the extended one, and the
    // two are never equal: that is what makes a stripped bitmap a
    // signature failure rather than a silent downgrade.
    let legacy = keystore::client::reg_msg("u", "x", "e", "m", None);
    let extended = keystore::client::reg_msg_with_capabilities("u", "x", "e", "m", None, 0);
    assert_ne!(legacy, extended);
    assert_eq!(
        String::from_utf8(extended).unwrap(),
        "OSL-REGISTER-v1\nu\nx\ne\nm\n\n0"
    );
}

/// Build a `PubkeysResponse` for `caps`, signed by `id`.
fn signed_pubkeys_response(
    id: &keystore::Identity,
    caps: Option<u32>,
) -> keystore::client::PubkeysResponse {
    let x = STANDARD.encode(id.x25519_public.as_bytes());
    let e = STANDARD.encode(id.ed25519_public.as_bytes());
    let m = STANDARD.encode(&id.mlkem_public_bytes[..]);
    let msg = match caps {
        Some(c) => keystore::client::reg_msg_with_capabilities(&id.user_id, &x, &e, &m, None, c),
        None => keystore::client::reg_msg(&id.user_id, &x, &e, &m, None),
    };
    let sig = crypto::ed25519::sign(&id.ed25519_secret, &msg);
    keystore::client::PubkeysResponse {
        user_id: id.user_id.clone(),
        ik_x25519_pub: x,
        ik_ed25519_pub: e,
        ik_mlkem768_pub: m,
        registered_at: "2026-01-01T00:00:00.000Z".into(),
        last_rotated_at: None,
        ik_ratchet_initial_pub: None,
        rn_capabilities: caps,
        registration_sig: caps
            .filter(|c| *c != 0)
            .map(|_| STANDARD.encode(sig.as_bytes())),
        identity_scheme: None,
        identity_bundle_version: None,
        identity_revision: None,
        ik_root_ed25519_pub: None,
        identity_bundle_proof_sig: None,
    }
}

#[test]
fn a_verified_bitmap_is_accepted() {
    use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
    let id = generate_identity("peer".to_string());
    let resp = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    let caps = keystore::client::verify_peer_capabilities(&resp);
    assert_eq!(caps, PeerCapabilities::Verified(RN_CAP_WIRE_RN));
    assert!(caps.supports_rn());
}

/// The three shapes that must all mean "no OSL-RN capability", and
/// none of which may mean "assume capable".
#[test]
fn an_absent_or_zero_bitmap_fails_closed() {
    use keystore::client::PeerCapabilities;
    let id = generate_identity("peer".to_string());

    // A key server that predates the advertisement: no field at all.
    let legacy = signed_pubkeys_response(&id, None);
    assert_eq!(
        keystore::client::verify_peer_capabilities(&legacy),
        PeerCapabilities::Absent
    );
    assert!(!keystore::client::verify_peer_capabilities(&legacy).supports_rn());

    // A peer that has the field but advertises nothing.
    let zero = signed_pubkeys_response(&id, Some(0));
    assert_eq!(
        keystore::client::verify_peer_capabilities(&zero),
        PeerCapabilities::Absent
    );
}

/// A bitmap served without the signature that covers it must never be
/// believed — otherwise a dishonest key server could invent capability.
#[test]
fn a_bitmap_without_a_signature_is_unverified() {
    use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
    let id = generate_identity("peer".to_string());
    let mut resp = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    resp.registration_sig = None;
    let caps = keystore::client::verify_peer_capabilities(&resp);
    assert_eq!(caps, PeerCapabilities::Unverified);
    assert!(!caps.supports_rn(), "unverified must not read as capable");
    assert_eq!(caps.bitmap(), 0);
}

/// Read-path tampering: every mutation of a served record makes the
/// bitmap unverifiable rather than quietly lowering it.
#[test]
fn tampering_with_a_served_record_is_detected() {
    use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
    let id = generate_identity("peer".to_string());
    let other = generate_identity("peer".to_string());

    // Lowered bitmap, signature untouched.
    let mut lowered = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN | 2));
    lowered.rn_capabilities = Some(RN_CAP_WIRE_RN);
    assert_eq!(
        keystore::client::verify_peer_capabilities(&lowered),
        PeerCapabilities::Unverified
    );

    // Raised bitmap: a server cannot inflate capability either.
    let mut raised = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    raised.rn_capabilities = Some(RN_CAP_WIRE_RN | 8);
    assert_eq!(
        keystore::client::verify_peer_capabilities(&raised),
        PeerCapabilities::Unverified
    );

    // Out-of-range bitmap.
    let mut huge = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    huge.rn_capabilities = Some(keystore::client::RN_CAP_MAX + 1);
    assert_eq!(
        keystore::client::verify_peer_capabilities(&huge),
        PeerCapabilities::Unverified
    );

    // Signature from a different identity.
    let mut swapped = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    swapped.registration_sig =
        signed_pubkeys_response(&other, Some(RN_CAP_WIRE_RN)).registration_sig;
    assert_eq!(
        keystore::client::verify_peer_capabilities(&swapped),
        PeerCapabilities::Unverified
    );

    // A mutated key field also breaks it: the signature covers the
    // whole record, not just the bitmap.
    let mut rekeyed = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    rekeyed.ik_x25519_pub = STANDARD.encode([0x55u8; 32]);
    assert_eq!(
        keystore::client::verify_peer_capabilities(&rekeyed),
        PeerCapabilities::Unverified
    );

    // Malformed base64 in either slot must not panic.
    let mut junk = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    junk.registration_sig = Some("!!!not base64!!!".into());
    assert_eq!(
        keystore::client::verify_peer_capabilities(&junk),
        PeerCapabilities::Unverified
    );
    let mut junk_key = signed_pubkeys_response(&id, Some(RN_CAP_WIRE_RN));
    junk_key.ik_ed25519_pub = "!!!".into();
    assert_eq!(
        keystore::client::verify_peer_capabilities(&junk_key),
        PeerCapabilities::Unverified
    );
}

fn control_inbox_response(filtered_sender: Option<&str>, senders: &[&str]) -> Vec<u8> {
    let items: Vec<serde_json::Value> = senders
        .iter()
        .enumerate()
        .map(|(index, sender)| {
            serde_json::json!({
                "id": format!("inbox-{index:02}"),
                "sender_id": sender,
                "scope_id": "scope-a",
                "bundle_b64": "AQ==",
                "created_at": 1_700_000_000 + index as i64,
                "kind": "",
            })
        })
        .collect();
    let mut body = serde_json::json!({
        "items": items,
        "filtered_sender_delivery": {
            "live": senders.len(),
            "retryable": 0,
            "quarantined": 0,
            "retired": 0,
        },
    });
    if let Some(sender) = filtered_sender {
        body["filtered_sender_id"] = serde_json::json!(sender);
    }
    control_inbox_json_response(body)
}

fn legacy_control_inbox_response(senders: &[&str]) -> Vec<u8> {
    let items: Vec<serde_json::Value> = senders
        .iter()
        .enumerate()
        .map(|(index, sender)| {
            serde_json::json!({
                "id": format!("inbox-{index:02}"),
                "sender_id": sender,
                "scope_id": "scope-a",
                "bundle_b64": "AQ==",
                "created_at": 1_700_000_000 + index as i64,
                "kind": "",
            })
        })
        .collect();
    control_inbox_json_response(serde_json::json!({ "items": items }))
}

fn health_response(value: serde_json::Value) -> Vec<u8> {
    control_inbox_json_response(value)
}

fn write_lp(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn sender_filter_floor_identity_anchor_sha256(user_id: &str, ed25519_public: &[u8]) -> String {
    let mut canonical = Vec::new();
    write_lp(&mut canonical, b"OSL-SENDER-FILTER-FLOOR-IDENTITY-v1\0");
    write_lp(&mut canonical, user_id.as_bytes());
    write_lp(&mut canonical, ed25519_public);
    let digest = sha2::Sha256::digest(canonical);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sender_filter_floor_response(user_id: &str, ed25519_public: &[u8], target: &str) -> Vec<u8> {
    assert!(target.starts_with("/v1/sender-filter-capability-floor/"));
    let timestamp_ms = query_value(target, "ts").parse::<i64>().unwrap();
    let request_id = query_value(target, "request_id");
    assert_eq!(request_id.len(), 43);
    assert!(!query_value(target, "sig").is_empty());
    control_inbox_json_response(serde_json::json!({
        "format": "osl.keyserver.sender-filter-capability-floor.v3",
        "recipient_user_id": user_id,
        "identity_anchor_sha256": sender_filter_floor_identity_anchor_sha256(
            user_id,
            ed25519_public,
        ),
        "capability_version": 1,
        "monotonic_version": 1,
        "first_observed_at_ms": timestamp_ms,
        "request_timestamp_ms": timestamp_ms,
        "request_id": request_id,
    }))
}

fn control_inbox_json_response(body: serde_json::Value) -> Vec<u8> {
    let body = serde_json::to_vec(&body).unwrap();
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes());
    response.extend_from_slice(&body);
    response
}

fn request_target(request: &[u8]) -> &str {
    std::str::from_utf8(request)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
}

#[test]
fn compatible_control_inbox_uses_signed_sender_filter_after_capability_probe() {
    let _serial = active_account_test_lock().lock().unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "osl-keystore-sender-filter-legacy-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    keystore::set_active_account_dir(Some(dir.clone()));

    let identity = generate_identity("recipient".to_owned());
    let floor_user_id = identity.user_id.clone();
    let floor_ed25519_public = identity.ed25519_public.as_bytes().to_vec();
    let (port, requests) = multi_response_server_from_requests(vec![
        Box::new(|_| {
            health_response(serde_json::json!({
                "ok": true,
                "capabilities": {
                    "control_inbox_sender_disposition": 1,
                    "control_inbox_eviction_signal": 1,
                },
            }))
        }),
        Box::new(move |request| {
            sender_filter_floor_response(
                &floor_user_id,
                &floor_ed25519_public,
                request_target(request),
            )
        }),
        Box::new(|_| control_inbox_response(Some("peer-a"), &["peer-a"])),
    ]);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();

    let page = client
        .get_control_inbox_compatible_from(&identity, "peer-a")
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].sender_id, "peer-a");
    assert_eq!(page.delivery.live, 1);
    assert_eq!(request_target(&requests.recv().unwrap()), "/v1/healthz");
    let floor_get = request_target(&requests.recv().unwrap()).to_owned();
    assert!(floor_get.starts_with(&format!(
        "/v1/sender-filter-capability-floor/{}?",
        identity.user_id
    )));
    assert_eq!(query_value(&floor_get, "request_id").len(), 43);
    let filtered_get = request_target(&requests.recv().unwrap()).to_owned();
    assert!(filtered_get.starts_with(&format!("/v1/control-inbox/{}?", identity.user_id)));
    assert_eq!(query_value(&filtered_get, "sender"), "peer-a");

    keystore::set_active_account_dir(None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn compatible_control_inbox_refuses_wrong_sender_filter_capability_version_before_get() {
    let identity = generate_identity("recipient".to_owned());
    let (port, requests) = multi_response_server(vec![health_response(serde_json::json!({
        "ok": true,
        "capabilities": {
            "control_inbox_sender_disposition": 0,
            "control_inbox_eviction_signal": 1,
        },
    }))]);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let error = client
        .get_control_inbox_compatible_from(&identity, "peer-a")
        .expect_err("a wrong sender-filter capability version must be refused");
    assert!(matches!(
        error,
        Error::Transport(message)
            if message.contains("sender-filter capability is unavailable, malformed, or transitional")
    ));
    assert_eq!(request_target(&requests.recv().unwrap()), "/v1/healthz");
    assert!(
        requests.try_recv().is_err(),
        "a refused capability probe must not fall through to a filtered GET"
    );
}

#[test]
fn compatible_control_inbox_refuses_legacy_shape_after_capability_probe() {
    let _serial = active_account_test_lock().lock().unwrap();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "osl-keystore-sender-filter-downgrade-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    keystore::set_active_account_dir(Some(dir.clone()));

    let identity = generate_identity("recipient".to_owned());
    let floor_user_id = identity.user_id.clone();
    let floor_ed25519_public = identity.ed25519_public.as_bytes().to_vec();
    let (port, requests) = multi_response_server_from_requests(vec![
        Box::new(|_| {
            health_response(serde_json::json!({
                "ok": true,
                "capabilities": {
                    "control_inbox_sender_disposition": 1,
                },
            }))
        }),
        Box::new(move |request| {
            sender_filter_floor_response(
                &floor_user_id,
                &floor_ed25519_public,
                request_target(request),
            )
        }),
        Box::new(|_| legacy_control_inbox_response(&["peer-b", "peer-a"])),
    ]);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    assert!(matches!(
        client.get_control_inbox_compatible_from(&identity, "peer-a"),
        Err(Error::Transport(message))
            if message.contains("did not confirm the sender filter")
    ));
    assert_eq!(request_target(&requests.recv().unwrap()), "/v1/healthz");
    let floor_get = request_target(&requests.recv().unwrap()).to_owned();
    assert!(floor_get.starts_with(&format!(
        "/v1/sender-filter-capability-floor/{}?",
        identity.user_id
    )));
    assert!(request_target(&requests.recv().unwrap()).contains("&sender=peer-a"));

    keystore::set_active_account_dir(None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn filtered_control_inbox_accepts_positive_rows_from_two_senders_independently() {
    for sender in ["peer-a", "peer-b"] {
        let response = control_inbox_response(Some(sender), &[sender]);
        let (port, request) = one_shot_server(response);
        let identity = generate_identity("recipient".to_owned());
        let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();

        let page = client.get_control_inbox_from(&identity, sender).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].sender_id, sender);
        assert_eq!(page.delivery.live, 1);
        assert_eq!(page.delivery.retained_disabled(), 0);

        let request = request.recv().unwrap();
        let target = request_target(&request);
        assert!(
            target.starts_with(&format!("/v1/control-inbox/{}?", identity.user_id)),
            "the authenticated identity, not response JSON, fixes the recipient path"
        );
        assert_eq!(query_value(target, "sender"), sender);
        let timestamp_ms = query_value(target, "ts").parse::<i64>().unwrap();
        let signature = STANDARD.decode(query_value(target, "sig")).unwrap();
        let signature: [u8; 64] = signature.try_into().unwrap();
        let canonical = keystore::control_inbox::canonical_control_inbox_get_bytes_filtered(
            &identity.user_id,
            timestamp_ms,
            Some(sender),
        );
        assert!(
            crypto::ed25519::verify(
                &identity.ed25519_public,
                &canonical,
                &crypto::ed25519::Signature::from_bytes(signature),
            )
            .unwrap(),
            "the sender filter must be covered by the recipient signature"
        );
    }
}

#[test]
fn filtered_control_inbox_refuses_missing_invalid_and_cross_sender_values() {
    let identity = generate_identity("recipient".to_owned());
    let unreachable = KeyServerClient::new("http://127.0.0.1:9").unwrap();
    for invalid in ["".to_owned(), "bad\nsender".to_owned(), "x".repeat(257)] {
        assert!(matches!(
            unreachable.get_control_inbox_from(&identity, &invalid),
            Err(Error::Transport(message))
                if message == "control-inbox sender filter is invalid"
        ));
    }

    let response = control_inbox_response(Some("peer-a"), &["peer-b"]);
    let (port, _) = one_shot_server(response);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    assert!(matches!(
        client.get_control_inbox_from(&identity, "peer-a"),
        Err(Error::Transport(message))
            if message == "control-inbox drain returned a row outside its sender filter"
    ));
}

#[test]
fn unfiltered_or_wrong_filter_echo_cannot_fall_back_to_a_wider_page() {
    let identity = generate_identity("recipient".to_owned());

    // This is the starvation shape: a complete unfiltered page from another
    // sender, followed by the active peer's row. Even though the desired row is
    // present in this mock response, absence of the server's filter echo makes
    // the whole answer unusable.
    let mut unfiltered_senders = vec!["peer-b"; 64];
    unfiltered_senders.push("peer-a");
    let response = control_inbox_response(None, &unfiltered_senders);
    let (port, _) = one_shot_server(response);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    assert!(matches!(
        client.get_control_inbox_from(&identity, "peer-a"),
        Err(Error::Transport(message))
            if message.contains("did not confirm the sender filter")
    ));

    let response = control_inbox_response(Some("peer-b"), &["peer-b"]);
    let (port, _) = one_shot_server(response);
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    assert!(matches!(
        client.get_control_inbox_from(&identity, "peer-a"),
        Err(Error::Transport(message))
            if message.contains("did not confirm the sender filter")
    ));
}

#[test]
fn filtered_control_inbox_requires_exact_typed_disposition_and_recipient_binding() {
    let identity = generate_identity("recipient-a".to_owned());
    let sender = "peer-a";
    let valid = || {
        serde_json::json!({
            "items": [],
            "filtered_sender_id": sender,
            "filtered_sender_delivery": {
                "live": 0,
                "retryable": 1,
                "quarantined": 0,
                "retired": 0,
            },
        })
    };

    let mut cases = Vec::new();
    let mut missing_object = valid();
    missing_object
        .as_object_mut()
        .unwrap()
        .remove("filtered_sender_delivery");
    cases.push(("missing disposition object", missing_object));

    let mut missing_member = valid();
    missing_member["filtered_sender_delivery"]
        .as_object_mut()
        .unwrap()
        .remove("retired");
    cases.push(("missing disposition member", missing_member));

    let mut unknown_member = valid();
    unknown_member["filtered_sender_delivery"]["held"] = serde_json::json!(1);
    cases.push(("unknown disposition member", unknown_member));

    let mut unknown_top_level = valid();
    unknown_top_level["recipient_id"] = serde_json::json!("recipient-b");
    cases.push(("spoofed recipient field", unknown_top_level));

    let mut negative_count = valid();
    negative_count["filtered_sender_delivery"]["retryable"] = serde_json::json!(-1);
    cases.push(("negative disposition count", negative_count));

    let mut wrong_count_type = valid();
    wrong_count_type["filtered_sender_delivery"]["retryable"] = serde_json::json!("1");
    cases.push(("wrong disposition count type", wrong_count_type));

    for (label, body) in cases {
        let (port, _) = one_shot_server(control_inbox_json_response(body));
        let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
        assert!(
            client.get_control_inbox_from(&identity, sender).is_err(),
            "{label} must be refused"
        );
    }

    let unreachable = KeyServerClient::new("http://127.0.0.1:9").unwrap();
    let invalid_recipient = generate_identity("bad\nrecipient".to_owned());
    assert!(matches!(
        unreachable.get_control_inbox_from(&invalid_recipient, sender),
        Err(Error::Transport(message))
            if message == "control-inbox recipient identity is invalid"
    ));
}

#[test]
fn filtered_control_inbox_retention_cleanup_boundary_is_not_an_empty_inbox() {
    let identity = generate_identity("recipient".to_owned());
    let sender = "peer-a";
    let response = |retryable| {
        control_inbox_json_response(serde_json::json!({
            "items": [],
            "filtered_sender_id": sender,
            "filtered_sender_delivery": {
                "live": 0,
                "retryable": retryable,
                "quarantined": 0,
                "retired": 0,
            },
        }))
    };

    // The Worker reports non-live rows until its bounded cleanup actually
    // removes them. The client receives no timestamp, so this test pins the two
    // observable sides of that server-owned cleanup boundary: retained before
    // removal, genuinely empty afterward.
    let (port, _) = one_shot_server(response(1));
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let at_boundary = client.get_control_inbox_from(&identity, sender).unwrap();
    assert!(at_boundary.items.is_empty());
    assert_eq!(at_boundary.delivery.retryable, 1);
    assert_eq!(at_boundary.delivery.retained_disabled(), 1);

    let (port, _) = one_shot_server(response(0));
    let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
    let after_boundary = client.get_control_inbox_from(&identity, sender).unwrap();
    assert!(after_boundary.items.is_empty());
    assert_eq!(after_boundary.delivery.retained_disabled(), 0);
}

#[test]
fn retained_or_spoofed_rows_cannot_be_relabelled_as_live_delete_candidates() {
    let identity = generate_identity("recipient".to_owned());
    let sender = "peer-a";
    let item = serde_json::json!({
        "id": "inbox-retained",
        "sender_id": sender,
        "scope_id": "scope-a",
        "bundle_b64": "AQ==",
        "created_at": 1_700_000_000,
        "kind": "",
    });

    for (label, body) in [
        (
            "retained payload exposed as an item",
            serde_json::json!({
                "items": [item.clone()],
                "filtered_sender_id": sender,
                "filtered_sender_delivery": {
                    "live": 0,
                    "retryable": 1,
                    "quarantined": 0,
                    "retired": 0,
                },
            }),
        ),
        (
            "spoofed sender row",
            serde_json::json!({
                "items": [{
                    "id": "inbox-spoofed",
                    "sender_id": "peer-b",
                    "scope_id": "scope-a",
                    "bundle_b64": "AQ==",
                    "created_at": 1_700_000_000,
                    "kind": "",
                }],
                "filtered_sender_id": sender,
                "filtered_sender_delivery": {
                    "live": 1,
                    "retryable": 0,
                    "quarantined": 0,
                    "retired": 0,
                },
            }),
        ),
    ] {
        let (port, _) = one_shot_server(control_inbox_json_response(body));
        let client = KeyServerClient::new(format!("http://127.0.0.1:{port}")).unwrap();
        assert!(
            client.get_control_inbox_from(&identity, sender).is_err(),
            "{label} must not reach the broker"
        );
    }
}
