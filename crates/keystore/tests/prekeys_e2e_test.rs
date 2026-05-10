//! End-to-end integration: spawn the Node keyserver as a subprocess
//! and exercise the prekey flow via the real `KeyServerClient`.
//!
//! This is the load-bearing cross-language test: it confirms the
//! Rust `canonical_replenish_bytes` produces the exact bytes the
//! Node `canonicalReplenishBytes` reconstructs (otherwise the
//! Ed25519 batch signature would fail on the server). Without this
//! test, the two sides could silently diverge on a punctuation
//! detail.
//!
//! Skipped automatically if `node` isn't on PATH or `npm install`
//! hasn't been run for the keyserver.

use keystore::{generate_identity, KeyServerClient, PrekeyConfig, PrekeyState};
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn keyserver_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/keystore. Step up two
    // levels to repo root, then into keyserver/.
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
            "skipping prekeys e2e: {} has no node_modules — run `npm install` \
             in the keyserver directory to enable this test",
            dir.display()
        );
        return true;
    }
    if which("node").is_none() {
        eprintln!("skipping prekeys e2e: `node` not on PATH");
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
    // Pick a random ephemeral port: bind to 0, take whatever the OS
    // gives, drop the listener (race), spawn the keyserver on that
    // port. Acceptable risk for tests.
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

    // Wait up to 5 s for the server to accept connections on `port`.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() >= deadline {
            // Best-effort: dump child stderr to ease debugging.
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

#[test]
fn prekey_round_trip_through_real_keyserver() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let client = KeyServerClient::new(format!("http://127.0.0.1:{}", server.port)).unwrap();

    // 1. Register identity (which now includes Ed25519 pub).
    let id = generate_identity("alice".to_string());
    let resp = client.register(&id).expect("register");
    assert_eq!(resp.user_id, "alice");

    // 2. Generate prekeys + replenish.
    let mut state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    let _replenish_resp = client
        .replenish_prekeys(&id, Some(&state.current_spk), &state.opk_pool)
        .expect("replenish");
    // The server accepted our signed batch — proves Rust's
    // canonical encoding matches Node's verbatim. (If they
    // disagreed, the Ed25519 verification would have rejected with
    // 401 and `replenish_prekeys` would have returned an
    // `Error::HttpStatus` here.)
    let _ = &mut state; // mark used

    // 3. Fetch the bundle — server pops one OPK.
    let bundle = client.fetch_prekey_bundle("alice").expect("fetch bundle");
    assert_eq!(bundle.user_id, "alice");
    assert_eq!(bundle.remaining_opk_count, 99);
    let opk = bundle.opk.expect("opk should be present");
    // Server-popped OPK ID should be in the range we generated.
    assert!(opk.id < 100, "server popped an unknown OPK id: {}", opk.id);

    // 4. Fetch a few more — counts decrement, distinct IDs.
    let bundle2 = client.fetch_prekey_bundle("alice").expect("fetch 2");
    assert_eq!(bundle2.remaining_opk_count, 98);
    let opk2 = bundle2.opk.unwrap();
    assert_ne!(opk.id, opk2.id);
}

#[test]
fn replenish_using_state_tops_up_to_target() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let client = KeyServerClient::new(format!("http://127.0.0.1:{}", server.port)).unwrap();

    let id = generate_identity("alice".to_string());
    client.register(&id).unwrap();

    // Initial pool of 100.
    let mut state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    client
        .replenish_prekeys(&id, Some(&state.current_spk), &state.opk_pool)
        .unwrap();

    // Burn through 80 OPKs so server has 20 remaining (below the
    // default replenish threshold of 25). Then call
    // `replenish_using_state` and confirm it tops the server back up.
    for _ in 0..80 {
        client.fetch_prekey_bundle("alice").unwrap();
    }
    let bundle = client.fetch_prekey_bundle("alice").unwrap();
    assert_eq!(bundle.remaining_opk_count, 19);

    // server_remaining = 19 (below threshold). Top up to 100.
    let resp = client
        .replenish_using_state(&id, &mut state, 19, 1_700_000_001)
        .unwrap();
    // 100 - 19 = 81 OPKs added.
    assert_eq!(resp.opks_added, 81);

    // Bundle now sees a fresh pool. (Each fetch pops one, so
    // remaining is 99 after the next fetch.)
    let bundle = client.fetch_prekey_bundle("alice").unwrap();
    assert_eq!(bundle.remaining_opk_count, 99);
}
