use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{cmd_osl_create_friend_request_by_osl_name, CreateFriendRequestByOslNameInput};
use ipc::state::AppState;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;

const KNOWN_NAME: &str = "VIOLET-0212";
const UNKNOWN_NAME: &str = "VIOLET-X";
const REQUEST_ID: &str = "REQ-0212";

struct ActiveAccountDirReset;

impl Drop for ActiveAccountDirReset {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

#[test]
fn task_0212_unknown_osl_name_is_refused_without_changing_pending_request() {
    let _reset = ActiveAccountDirReset;
    let dir = tempfile::tempdir().expect("tempdir");
    keystore::set_active_account_dir(Some(dir.path().to_path_buf()));
    ipc::main_password::set_file_storage_key(None);

    let local = keystore::generate_identity("task_0212_local".to_string());
    let peer = keystore::generate_identity("task_0212_peer".to_string());
    let (base_url, requests, server) = directory_fixture(KNOWN_NAME, UNKNOWN_NAME, &peer);

    let state = AppState::new();
    state.install_identity(local);
    *state.keyserver_slot() = Some(keystore::KeyServerClient::new(base_url).expect("keyserver"));

    let known_readable = bucket_contains_name(KNOWN_NAME, &peer);
    let before_count = pending_count(dir.path());
    println!("TASK_0212_READABLE_NAME={KNOWN_NAME} readable={known_readable}");
    println!("TASK_0212_PENDING_COUNT_BEFORE={before_count}");
    assert!(
        known_readable,
        "{KNOWN_NAME} must be readable in the fixture"
    );
    assert_eq!(before_count, 0);

    let good = CreateFriendRequestByOslNameInput {
        recipient_name: KNOWN_NAME.to_string(),
        request_id: REQUEST_ID.to_string(),
    };
    let sent = cmd_osl_create_friend_request_by_osl_name(&state, good.clone())
        .expect("known OSL name should create a pending request");
    let after_good_count = pending_count(dir.path());
    let fingerprint_after_good = pending_fingerprint(dir.path());
    println!(
        "TASK_0212_GOOD_REQUEST request_id={} recipient_name={} pending_count={} fingerprint={}",
        sent.request_id, sent.recipient_name, after_good_count, fingerprint_after_good
    );
    assert_eq!(sent.request_id, REQUEST_ID);
    assert_eq!(sent.recipient_name, KNOWN_NAME);
    assert_eq!(after_good_count, 1);

    let mut unknown = good;
    unknown.recipient_name = UNKNOWN_NAME.to_string();
    let refused = cmd_osl_create_friend_request_by_osl_name(&state, unknown)
        .expect_err("unknown OSL name must be refused");
    let after_unknown_count = pending_count(dir.path());
    let fingerprint_after_unknown = pending_fingerprint(dir.path());
    println!("TASK_0212_UNKNOWN_NAME={UNKNOWN_NAME} refused={refused}");
    println!("TASK_0212_PENDING_COUNT_AFTER_UNKNOWN={after_unknown_count}");
    println!(
        "TASK_0212_FINGERPRINT request_id={REQUEST_ID} before={} after={}",
        fingerprint_after_good, fingerprint_after_unknown
    );
    assert_eq!(refused, "OSL: unknown OSL name");
    assert_eq!(after_unknown_count, 1);
    assert_eq!(fingerprint_after_unknown, fingerprint_after_good);

    let seen = requests
        .try_iter()
        .map(|request| request_target(&request))
        .collect::<Vec<_>>();
    assert_eq!(seen.len(), 3);
    assert!(seen[0].starts_with("/v1/username-bucket/"));
    assert_eq!(seen[1], "/v1/pubkeys/task_0212_peer");
    assert!(seen[2].starts_with("/v1/username-bucket/"));
    server.join().expect("fixture server joined");
}

fn directory_fixture(
    known_name: &'static str,
    unknown_name: &'static str,
    peer: &keystore::Identity,
) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let (tx, rx) = mpsc::channel();
    let peer_pubkeys = pubkeys_response_json(peer);
    let known_bucket = username_bucket(known_name, Some(peer));
    let unknown_bucket = username_bucket(unknown_name, None);
    let known_bucket_path = format!("/v1/username-bucket/{}", bucket_prefix(known_name));
    let unknown_bucket_path = format!("/v1/username-bucket/{}", bucket_prefix(unknown_name));
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept");
            let request = read_request(&mut stream);
            let target = request_target(&request);
            tx.send(request).expect("request sent");
            let response = if target == "/v1/pubkeys/task_0212_peer" {
                http_response("200 OK", "application/json", peer_pubkeys.as_bytes())
            } else if target == known_bucket_path {
                http_response("200 OK", "text/plain", known_bucket.as_bytes())
            } else if target == unknown_bucket_path {
                http_response("200 OK", "text/plain", unknown_bucket.as_bytes())
            } else {
                http_response(
                    "404 Not Found",
                    "application/json",
                    br#"{"error":"not_found"}"#,
                )
            };
            stream.write_all(&response).expect("write response");
        }
    });
    (base_url, rx, server)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("timeout");
    let mut buffer = [0u8; 4096];
    let mut bytes = Vec::new();
    let header_end = loop {
        let read = stream.read(&mut buffer).expect("read request");
        assert!(read > 0, "request ended before headers");
        bytes.extend_from_slice(&buffer[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("headers utf8");
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    while bytes[header_end + 4..].len() < content_length {
        let read = stream.read(&mut buffer).expect("read body");
        assert!(read > 0, "request body ended early");
        bytes.extend_from_slice(&buffer[..read]);
    }
    bytes
}

fn request_target(request: &[u8]) -> String {
    String::from_utf8_lossy(request)
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .expect("request target")
        .to_string()
}

fn http_response(status: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = Vec::new();
    response.extend_from_slice(format!("HTTP/1.1 {status}\r\n").as_bytes());
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    response.extend_from_slice(body);
    response
}

fn pubkeys_response_json(identity: &keystore::Identity) -> String {
    let ik_x25519_pub = STANDARD.encode(identity.x25519_public.as_bytes());
    let ik_ed25519_pub = STANDARD.encode(identity.ed25519_public.as_bytes());
    let ik_mlkem768_pub = STANDARD.encode(identity.mlkem_public_bytes);
    let ratchet = identity
        .ratchet_initial_pub
        .as_ref()
        .map(|key| STANDARD.encode(key.as_bytes()));
    let capabilities = keystore::client::CLIENT_RN_CAPABILITY_FLOOR;
    let signed = keystore::client::reg_msg_with_capabilities(
        &identity.user_id,
        &ik_x25519_pub,
        &ik_ed25519_pub,
        &ik_mlkem768_pub,
        ratchet.as_deref(),
        capabilities,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &signed);
    serde_json::json!({
        "user_id": identity.user_id,
        "ik_x25519_pub": ik_x25519_pub,
        "ik_ed25519_pub": ik_ed25519_pub,
        "ik_mlkem768_pub": ik_mlkem768_pub,
        "registered_at": "2026-08-06T00:00:00.000Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": ratchet,
        "rn_capabilities": capabilities,
        "registration_sig": STANDARD.encode(signature.as_bytes())
    })
    .to_string()
}

fn bucket_contains_name(name: &str, peer: &keystore::Identity) -> bool {
    let bucket = username_bucket(name, Some(peer));
    let suffix = &username_digest_hex(name)[4..];
    bucket.lines().any(|line| line.starts_with(suffix))
}

fn username_bucket(name: &str, peer: Option<&keystore::Identity>) -> String {
    let target_suffix = peer.map(|_| username_digest_hex(name)[4..].to_string());
    let mut suffixes = (0..1024)
        .map(|index| format!("{index:060x}"))
        .filter(|suffix| Some(suffix) != target_suffix.as_ref())
        .take(1023)
        .map(|suffix| {
            (
                suffix,
                "padding-user".to_string(),
                STANDARD.encode([0u8; 32]),
            )
        })
        .collect::<Vec<_>>();
    if let Some(peer) = peer {
        suffixes.push((
            target_suffix.expect("target suffix"),
            peer.user_id.clone(),
            STANDARD.encode(peer.ed25519_public.as_bytes()),
        ));
    } else {
        suffixes.push((
            format!("{:060x}", 1024),
            "padding-user".to_string(),
            STANDARD.encode([1u8; 32]),
        ));
    }
    suffixes.sort_by(|left, right| left.0.cmp(&right.0));
    suffixes
        .into_iter()
        .map(|(suffix, user_id, ed)| format!("{suffix}:{user_id}:{ed}\n"))
        .collect()
}

fn bucket_prefix(name: &str) -> String {
    username_digest_hex(name)[..4].to_string()
}

fn username_digest_hex(name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"OSL-USERNAME-BUCKET-v1");
    hasher.update(name.as_bytes());
    hex(&hasher.finalize())
}

fn pending_count(dir: &std::path::Path) -> usize {
    let path = dir.join("pending_friend_requests.json");
    if !path.exists() {
        return 0;
    }
    let bytes = std::fs::read(path).expect("read pending file");
    let plain = ipc::main_password::maybe_decrypt(&bytes).expect("decrypt pending file");
    let value: Value = serde_json::from_slice(&plain).expect("pending JSON");
    value.as_array().expect("pending array").len()
}

fn pending_fingerprint(dir: &std::path::Path) -> String {
    let bytes = std::fs::read(dir.join("pending_friend_requests.json")).expect("read pending file");
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
