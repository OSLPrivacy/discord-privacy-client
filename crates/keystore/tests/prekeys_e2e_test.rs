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

use keystore::{
    generate_identity, identity_bundle::BundleMergeError, identity_bundle::BundleVerifyPolicy,
    identity_bundle::IdentityBundle, KeyServerClient, PrekeyConfig, PrekeyState,
};
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

#[allow(clippy::zombie_processes)] // ServerHandle::drop kills and waits for the child.
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

fn identity_bundle_for(identity: &keystore::Identity, revision: u64) -> IdentityBundle {
    let mut bundle = IdentityBundle {
        ed25519_identity_pub: *identity.ed25519_public.as_bytes(),
        x25519_identity_pub: *identity.x25519_public.as_bytes(),
        mlkem768_identity_pub: identity.mlkem_public_bytes,
        capability_bundle: 1,
        revision,
        signature: [0; crypto::ed25519::SIGNATURE_SIZE],
    };
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &bundle.signed_bytes());
    bundle.signature = *signature.as_bytes();
    bundle
}

#[test]
fn prekey_round_trip_through_real_keyserver() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let client = KeyServerClient::new(format!("http://127.0.0.1:{}", server.port)).unwrap();

    // 1. Register two identities (which now include Ed25519 pub).
    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    let alice_resp = client.register(&alice).expect("register alice");
    let bob_resp = client.register(&bob).expect("register bob");
    assert_eq!(alice_resp.user_id, "alice");
    assert_eq!(bob_resp.user_id, "bob");

    // 2. Generate Bob's prekeys + replenish.
    let mut bob_state = PrekeyState::new(&bob, PrekeyConfig::default(), 1_700_000_000);
    let replenish_resp = client
        .replenish_prekeys(&bob, Some(&bob_state.current_spk), &bob_state.opk_pool)
        .expect("replenish bob");
    assert_eq!(replenish_resp.user_id, "bob");
    assert_eq!(replenish_resp.opks_added, 100);
    // The server accepted Bob's signed batch — proves Rust's
    // canonical encoding matches Node's verbatim. (If they
    // disagreed, the Ed25519 verification would have rejected with
    // 401 and `replenish_prekeys` would have returned an
    // `Error::HttpStatus` here.)

    // 3. Alice fetches Bob's bundle — server pops one OPK from Bob.
    let bundle = client
        .fetch_prekey_bundle(&alice, "bob")
        .expect("alice fetches bob bundle");
    assert_eq!(bundle.user_id, "bob");
    assert_eq!(bundle.remaining_opk_count, 99);
    let opk = bundle.opk.expect("opk should be present");
    // Server-popped OPK ID should be in the range we generated.
    assert!(opk.id < 100, "server popped an unknown OPK id: {}", opk.id);

    // 4. Fetch a few more — counts decrement, distinct IDs.
    let bundle2 = client
        .fetch_prekey_bundle(&alice, "bob")
        .expect("alice fetches bob bundle 2");
    assert_eq!(bundle2.remaining_opk_count, 98);
    let opk2 = bundle2.opk.unwrap();
    assert_ne!(opk.id, opk2.id);

    // 5. Drain Bob below threshold, then replenish through the stateful
    // path. This proves the live server's remaining count can drive
    // Bob's local replenishment without accidentally touching Alice.
    for _ in 0..78 {
        client.fetch_prekey_bundle(&alice, "bob").unwrap();
    }
    let low_bundle = client.fetch_prekey_bundle(&alice, "bob").unwrap();
    assert_eq!(low_bundle.user_id, "bob");
    assert_eq!(low_bundle.remaining_opk_count, 19);

    let top_up = client
        .replenish_using_state(&bob, &mut bob_state, 19, 1_700_000_001)
        .expect("top up bob");
    assert_eq!(top_up.user_id, "bob");
    assert_eq!(top_up.opks_added, 81);

    let replenished = client
        .fetch_prekey_bundle(&alice, "bob")
        .expect("alice fetches bob after top up");
    assert_eq!(replenished.user_id, "bob");
    assert_eq!(replenished.remaining_opk_count, 99);
}

#[test]
fn prekey_bundle_roundtrip_replenish_fetch_consumed_by_rn_handshake() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    let server = spawn_keyserver();
    let client = KeyServerClient::new(format!("http://127.0.0.1:{}", server.port)).unwrap();

    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    client.register(&alice).expect("register alice");
    client.register(&bob).expect("register bob");

    let bob_bundle = identity_bundle_for(&bob, 1);
    let policy = BundleVerifyPolicy::new();
    assert_eq!(
        policy
            .verify(&bob_bundle, &bob.ed25519_public, None)
            .expect("bob identity bundle verifies"),
        1
    );

    let bob_state = PrekeyState::new(&bob, PrekeyConfig::default(), 1_700_000_000);
    client
        .replenish_prekeys(&bob, Some(&bob_state.current_spk), &bob_state.opk_pool)
        .expect("bob replenish");

    let response = client
        .fetch_prekey_bundle(&alice, "bob")
        .expect("alice fetches bob prekey bundle");
    assert_eq!(response.user_id, "bob");
    assert_eq!(response.remaining_opk_count, 99);
    let first_opk_id = response.opk.as_ref().expect("first fetch carries OPK").id;

    let merged = bob_bundle
        .merge_prekey_bundle_response(&response, None)
        .expect("prekey response merges into pinned identity bundle");
    assert_eq!(merged.identity, bob_bundle);
    assert_eq!(merged.prekey.remaining_opk_count, 99);
    assert_eq!(merged.prekey.opk.expect("merged OPK").0, first_opk_id);
    assert_eq!(merged.prekey.spk_x25519_pub, bob_state.current_spk.public);

    let second = client
        .fetch_prekey_bundle(&alice, "bob")
        .expect("alice fetches second bob prekey bundle");
    assert_eq!(second.remaining_opk_count, 98);
    assert_ne!(
        first_opk_id,
        second.opk.as_ref().expect("second fetch carries OPK").id,
        "server must consume one OPK per authenticated prekey-bundle fetch"
    );

    let mut tampered = second;
    tampered.spk_signature.pop();
    assert!(matches!(
        bob_bundle.merge_prekey_bundle_response(&tampered, None),
        Err(BundleMergeError::MalformedSpk)
    ));
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
        client.fetch_prekey_bundle(&id, "alice").unwrap();
    }
    let bundle = client.fetch_prekey_bundle(&id, "alice").unwrap();
    assert_eq!(bundle.remaining_opk_count, 19);

    // server_remaining = 19 (below threshold). Top up to 100.
    let resp = client
        .replenish_using_state(&id, &mut state, 19, 1_700_000_001)
        .unwrap();
    // 100 - 19 = 81 OPKs added.
    assert_eq!(resp.opks_added, 81);

    // Bundle now sees a fresh pool. (Each fetch pops one, so
    // remaining is 99 after the next fetch.)
    let bundle = client.fetch_prekey_bundle(&id, "alice").unwrap();
    assert_eq!(bundle.remaining_opk_count, 99);
}
