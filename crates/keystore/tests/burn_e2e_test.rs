//! End-to-end burn flow: spawn the real Node keyserver, register a
//! Rust-side identity, upload a wrapped key, then issue burns through
//! the Rust `KeyServerClient` and confirm the server actually deletes.
//!
//! Locks the cross-language byte agreement for `canonical_burn_bytes`:
//! the Ed25519 signature must verify on the Node side, otherwise the
//! server returns 401 and the burn no-ops.
//!
//! Skipped automatically if `node` isn't on PATH or `npm install`
//! hasn't been run for the keyserver.

use keystore::{generate_identity, BurnScope, KeyServerClient};
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn keyserver_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("keyserver");
    p
}

fn skip_if_keyserver_unavailable() -> bool {
    let dir = keyserver_dir();
    if !dir.join("node_modules").exists() {
        eprintln!(
            "skipping burn e2e: {} has no node_modules — run `npm install` \
             in the keyserver directory to enable this test",
            dir.display()
        );
        return true;
    }
    if which("node").is_none() {
        eprintln!("skipping burn e2e: `node` not on PATH");
        return true;
    }
    false
}

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let p = entry.join(prog);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

struct ServerHandle {
    child: Child,
    port: u16,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_keyserver() -> ServerHandle {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut cmd = Command::new("node");
    cmd.arg("src/server.js")
        .current_dir(keyserver_dir())
        .env("PORT", port.to_string())
        .env("KEYSERVER_DB", ":memory:")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn keyserver");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() >= deadline {
            if let Some(stderr) = child.stderr.take() {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                while reader.read_line(&mut buf).unwrap_or(0) > 0 {
                    if buf.len() > 4096 {
                        break;
                    }
                }
                eprintln!("keyserver stderr: {buf}");
            }
            let _ = child.kill();
            panic!("keyserver did not become ready within 5s on port {port}");
        }
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(50),
        )
        .is_ok()
        {
            return ServerHandle { child, port };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Use a separate HTTP request to upload a wrapped key. Goes through
/// the existing keyserver POST /v1/wrapped-keys path. Returns nothing
/// on success.
fn upload_wrapped_key(base_url: &str, content_id: &str, sender: &str, recipient: &str) {
    use std::io::{Read, Write};
    let parts: Vec<&str> = base_url
        .strip_prefix("http://")
        .unwrap()
        .split(':')
        .collect();
    let host = parts[0];
    let port: u16 = parts[1].parse().unwrap();

    let body = format!(
        r#"{{"content_id":"{content_id}","content_type":"text","sender_id":"{sender}",
            "recipient_id":"{recipient}","session_version":1,"share_index":0,
            "wrapped_share_blob":"YQ==","blob_version":1,"single_use":false,
            "expires_at":"2099-01-01T00:00:00.000Z"}}"#
    );
    let req = format!(
        "POST /v1/wrapped-keys HTTP/1.1\r\nHost: {host}:{port}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let mut s = TcpStream::connect((host, port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).unwrap();
    let resp_text = String::from_utf8_lossy(&resp);
    assert!(
        resp_text.contains("201") || resp_text.contains("200"),
        "upload failed: {resp_text}"
    );
}

/// Confirm GET /v1/wrapped-keys/:id returns the given status code.
fn assert_wrapped_status(base_url: &str, content_id: &str, expected: u16) {
    use std::io::{Read, Write};
    let parts: Vec<&str> = base_url
        .strip_prefix("http://")
        .unwrap()
        .split(':')
        .collect();
    let host = parts[0];
    let port: u16 = parts[1].parse().unwrap();

    let req = format!(
        "GET /v1/wrapped-keys/{content_id} HTTP/1.1\r\nHost: {host}:{port}\r\n\
         Connection: close\r\n\r\n"
    );
    let mut s = TcpStream::connect((host, port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).unwrap();
    let resp_text = String::from_utf8_lossy(&resp);
    let status_line = resp_text.lines().next().unwrap_or("");
    assert!(
        status_line.contains(&expected.to_string()),
        "expected status {expected}, got: {status_line}"
    );
}

#[test]
fn burn_round_trip_through_real_keyserver() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let base_url = format!("http://127.0.0.1:{}", server.port);
    let client = KeyServerClient::new(&base_url).unwrap();

    // Two registered users.
    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    client.register(&alice).expect("register alice");
    client.register(&bob).expect("register bob");

    // Alice uploads two messages; bob uploads one.
    upload_wrapped_key(&base_url, "alice-1", "alice", "bob");
    upload_wrapped_key(&base_url, "alice-2", "alice", "bob");
    upload_wrapped_key(&base_url, "bob-1", "bob", "alice");

    // Burn just alice-1 — the Rust client signs canonical_burn_bytes
    // and the Node server verifies with Alice's stored Ed25519 pub.
    let resp = client
        .burn(
            &alice,
            &BurnScope::Single {
                content_id: "alice-1".into(),
            },
        )
        .expect("burn single");
    assert_eq!(resp.scope, "single");
    assert_eq!(resp.deleted_count, 1);

    assert_wrapped_status(&base_url, "alice-1", 404);
    assert_wrapped_status(&base_url, "alice-2", 200);
    assert_wrapped_status(&base_url, "bob-1", 200);

    // Burn-all on alice — bob's row stays.
    let resp = client.burn(&alice, &BurnScope::All).expect("burn all");
    assert_eq!(resp.scope, "all");
    assert_eq!(resp.deleted_count, 1);
    assert_wrapped_status(&base_url, "alice-2", 404);
    assert_wrapped_status(&base_url, "bob-1", 200);
}

#[test]
fn burn_to_user_round_trip_through_real_keyserver() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let base_url = format!("http://127.0.0.1:{}", server.port);
    let client = KeyServerClient::new(&base_url).unwrap();

    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    let carol = generate_identity("carol".to_string());
    client.register(&alice).unwrap();
    client.register(&bob).unwrap();
    client.register(&carol).unwrap();

    upload_wrapped_key(&base_url, "a-to-b-1", "alice", "bob");
    upload_wrapped_key(&base_url, "a-to-b-2", "alice", "bob");
    upload_wrapped_key(&base_url, "a-to-c-1", "alice", "carol");

    let resp = client
        .burn(
            &alice,
            &BurnScope::ToUser {
                user_id: "bob".into(),
            },
        )
        .expect("burn to_user");
    assert_eq!(resp.scope, "to_user");
    assert_eq!(resp.deleted_count, 2);

    assert_wrapped_status(&base_url, "a-to-b-1", 404);
    assert_wrapped_status(&base_url, "a-to-b-2", 404);
    assert_wrapped_status(&base_url, "a-to-c-1", 200);
}
