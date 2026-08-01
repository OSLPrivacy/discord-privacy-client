//! REGISTER-FIX: prove the post-unlock / post-snowflake keyserver
//! registration hook runs the keyserver-client install that the
//! boot-time path skipped.
//!
//! Root cause being fixed: `bootstrap::run_autostart` runs at cold
//! boot. On a V2 clean install there is no `identity.json` then, so
//! the only boot-time caller of `client.register` + the only
//! installer of `state.keyserver` (`init_keyserver_and_register`) is
//! skipped, and nothing ever retried it — the machine never reached
//! `POST /v1/register`. The fix routes both the boot path and the
//! runtime paths through the shared
//! [`ipc::commands::ensure_keyserver_registered`].
//!
//! These tests stay on loopback: failure-path tests POST to an
//! unreachable port so the network call fails fast, while success-path
//! tests use a tiny local mock server to capture the register and prekey
//! replenish requests. Per the helper's non-fatal contract a `register`
//! failure is swallowed and the client is still installed (so later
//! `fetch_pubkeys` can work) — exactly the boot path's behaviour.

use ipc::commands::ensure_keyserver_registered;
use ipc::state::CloudRegistrationState;
use ipc::AppState;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use tempfile::TempDir;

/// Unreachable on purpose: TCP port 1 refuses immediately, so
/// `register` returns a transport error in milliseconds instead of
/// waiting on the 30s client timeout.
const UNREACHABLE: &str = "http://127.0.0.1:1";

static KEYSTORE_TEST_LOCK: Mutex<()> = Mutex::new(());

struct IsolatedKeystore {
    _guard: MutexGuard<'static, ()>,
    dir: TempDir,
}

impl IsolatedKeystore {
    fn new() -> Self {
        let guard = KEYSTORE_TEST_LOCK.lock().expect("keystore test lock");
        let dir = TempDir::new().expect("isolated keystore dir");
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(Some(dir.path().to_path_buf()));
        Self { _guard: guard, dir }
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

impl Drop for IsolatedKeystore {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("set timeout");
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).expect("read request");
        if read == 0 {
            break request.len();
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(position) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            break position;
        }
    };
    let header_text = std::str::from_utf8(&request[..header_end]).expect("headers are utf8");
    let content_length = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then_some(value.trim())
        })
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body_read = request[header_end + 4..].len();
    while body_read < content_length {
        let read = stream.read(&mut chunk).expect("read request body");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        body_read += read;
    }
    request
}

fn successful_register_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("local address");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept register request");
        drop(listener);
        let request = read_http_request(&mut stream);
        assert!(String::from_utf8_lossy(&request)
            .to_ascii_lowercase()
            .starts_with("post /v1/register http/1.1"));
        let body = br#"{"user_id":"123456789012345678","registered_at":"2026-07-17T00:00:00Z"}"#;
        write!(
            stream,
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write response headers");
        stream.write_all(body).expect("write response body");
    });
    format!("http://{address}")
}

fn successful_register_and_replenish_server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let address = listener.local_addr().expect("local address");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut register_stream, _) = listener.accept().expect("accept register request");
        let register_request = read_http_request(&mut register_stream);
        assert!(String::from_utf8_lossy(&register_request)
            .to_ascii_lowercase()
            .starts_with("post /v1/register http/1.1"));
        let register_body =
            br#"{"user_id":"123456789012345678","registered_at":"2026-07-17T00:00:00Z"}"#;
        write!(
            register_stream,
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            register_body.len()
        )
        .expect("write register headers");
        register_stream
            .write_all(register_body)
            .expect("write register body");

        let (mut replenish_stream, _) = listener.accept().expect("accept replenish request");
        let replenish_request = read_http_request(&mut replenish_stream);
        let replenish_text = String::from_utf8(replenish_request).expect("request utf8");
        assert!(replenish_text
            .to_ascii_lowercase()
            .starts_with("post /v1/prekey-bundle/replenish http/1.1"));
        tx.send(replenish_text).expect("send replenish request");
        let replenish_body = br#"{"user_id":"123456789012345678","opks_added":100}"#;
        write!(
            replenish_stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            replenish_body.len()
        )
        .expect("write replenish headers");
        replenish_stream
            .write_all(replenish_body)
            .expect("write replenish body");
    });
    (format!("http://{address}"), rx)
}

#[test]
fn successful_plain_registration_leaves_authoritative_ready_state() {
    let isolated = IsolatedKeystore::new();
    let state = AppState::new();
    *state.identity.lock().unwrap() =
        Some(keystore::generate_identity("123456789012345678".into()));

    ensure_keyserver_registered(&state, &successful_register_server(), None);

    assert_eq!(
        state.cloud_registration_state(),
        CloudRegistrationState::Registered,
        "a successful plain register must not remain stuck in pending"
    );
    assert!(
        isolated.path().join("prekeys.json").is_file(),
        "successful registration should create sealed prekey state"
    );
}

#[test]
fn successful_registration_publishes_initial_prekeys() {
    let isolated = IsolatedKeystore::new();
    let state = AppState::new();
    *state.identity.lock().unwrap() =
        Some(keystore::generate_identity("123456789012345678".into()));
    let (server, replenish_rx) = successful_register_and_replenish_server();

    ensure_keyserver_registered(&state, &server, None);

    let sealer = keystore::select_best_sealer();
    let loaded =
        keystore::load_prekey_state(&isolated.path().join("prekeys.json"), sealer.as_ref())
            .expect("sealed prekey state loads");
    assert_eq!(loaded.opk_pool.len(), 100);
    assert!(loaded.previous_spk.is_none());

    let request = replenish_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("replenish request captured");
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let body = request
        .split("\r\n\r\n")
        .nth(1)
        .expect("request body present");
    let json: serde_json::Value = serde_json::from_str(body).expect("replenish body json");
    assert_eq!(json["user_id"].as_str(), Some("123456789012345678"));
    assert!(json["batch_signature_b64"].as_str().is_some());
    assert!(json["spk"].is_object());
    assert_eq!(json["opks"].as_array().map(Vec::len), Some(100));
}

#[test]
fn existing_prekeys_for_different_identity_are_not_overwritten() {
    let isolated = IsolatedKeystore::new();
    let sealer = keystore::select_best_sealer();
    let old_identity = keystore::generate_identity("123456789012345678".into());
    let old_state = keystore::PrekeyState::new(
        &old_identity,
        keystore::PrekeyConfig::default(),
        1_700_000_000,
    );
    keystore::save_prekey_state(
        &isolated.path().join("prekeys.json"),
        &old_state,
        sealer.as_ref(),
    )
    .expect("seed old prekeys");

    let state = AppState::new();
    let current_identity = keystore::generate_identity("123456789012345678".into());
    *state.identity.lock().unwrap() = Some(current_identity.clone());

    ensure_keyserver_registered(&state, &successful_register_server(), None);

    let loaded =
        keystore::load_prekey_state(&isolated.path().join("prekeys.json"), sealer.as_ref())
            .expect("prekeys remain loadable");
    assert_eq!(loaded.current_spk.public, old_state.current_spk.public);
    assert!(
        crypto::ed25519::verify(
            &old_identity.ed25519_public,
            &loaded.current_spk.public,
            &crypto::ed25519::Signature::from_bytes(loaded.current_spk.signature),
        )
        .unwrap(),
        "seeded prekeys still belong to the old identity"
    );
    assert!(
        !crypto::ed25519::verify(
            &current_identity.ed25519_public,
            &loaded.current_spk.public,
            &crypto::ed25519::Signature::from_bytes(loaded.current_spk.signature),
        )
        .unwrap(),
        "mismatched prekeys must not be treated as current authority"
    );
}

#[test]
fn post_unlock_registers_when_boot_skipped_it() {
    let _isolated = IsolatedKeystore::new();
    let state = AppState::new();

    // Precondition: this is exactly the state bootstrap leaves behind
    // when it skipped init_keyserver_and_register (no identity at
    // boot) — keyserver client absent.
    assert!(
        state.keyserver.lock().unwrap().is_none(),
        "precondition: keyserver client not installed (boot skipped it)"
    );

    // Identity now exists in state (loaded on relaunch by bootstrap,
    // or just generated by the Discord-snowflake path).
    *state.identity.lock().unwrap() =
        Some(keystore::generate_identity("123456789012345678".into()));

    ensure_keyserver_registered(&state, UNREACHABLE, None);

    assert_eq!(
        state.cloud_registration_state(),
        CloudRegistrationState::Offline,
        "a constructed client must not be mistaken for confirmed remote registration"
    );

    // The register POST failed (offline) but, exactly like the boot
    // path, the client is installed regardless so fetch_pubkeys can
    // still work. This install is the work boot skipped — proving the
    // post-unlock hook closes the gap.
    assert!(
        state.keyserver.lock().unwrap().is_some(),
        "post-unlock hook must install the keyserver client that boot skipped"
    );
}

#[test]
fn no_identity_is_a_safe_noop() {
    let _isolated = IsolatedKeystore::new();
    let state = AppState::new();
    assert!(state.identity.lock().unwrap().is_none());

    // An unlock that happens before the Discord-snowflake identity
    // exists (first install: gate verifies before Discord opens).
    // Must not panic and must not install a client.
    ensure_keyserver_registered(&state, UNREACHABLE, None);

    assert_eq!(
        state.cloud_registration_state(),
        CloudRegistrationState::NotAttempted
    );

    assert!(
        state.keyserver.lock().unwrap().is_none(),
        "no identity → no-op: nothing to register, nothing installed"
    );
}

#[test]
fn idempotent_does_not_double_install_or_panic() {
    let _isolated = IsolatedKeystore::new();
    let state = AppState::new();
    *state.identity.lock().unwrap() =
        Some(keystore::generate_identity("123456789012345678".into()));

    // First call installs the client (boot-skipped recovery).
    ensure_keyserver_registered(&state, UNREACHABLE, None);
    assert!(state.keyserver.lock().unwrap().is_some());

    // This helper runs on EVERY unlock. Re-running must not panic and
    // must not stomp the already-installed client. (register itself
    // is a server-side upsert keyed by user_id with stable keys, so
    // the repeated POST is a no-op beyond a metadata timestamp.)
    ensure_keyserver_registered(&state, UNREACHABLE, None);
    ensure_keyserver_registered(&state, UNREACHABLE, None);
    assert!(
        state.keyserver.lock().unwrap().is_some(),
        "repeated unlocks keep exactly one installed client"
    );
}

#[test]
fn pre_installed_client_is_preserved() {
    let _isolated = IsolatedKeystore::new();
    // Simulates: bootstrap DID load an identity at boot and already
    // installed + registered (the dev-keyserver.json case requirement
    // #4 must not regress). A later password-gate unlock then fires
    // the same shared helper.
    let state = AppState::new();
    *state.identity.lock().unwrap() =
        Some(keystore::generate_identity("123456789012345678".into()));

    // Pretend boot installed it.
    *state.keyserver.lock().unwrap() =
        Some(keystore::KeyServerClient::new(UNREACHABLE).expect("client builds"));
    assert!(state.keyserver.lock().unwrap().is_some());

    // Post-unlock helper runs; must leave the boot-installed client
    // in place (no double-install / no overwrite).
    ensure_keyserver_registered(&state, UNREACHABLE, None);

    assert!(
        state.keyserver.lock().unwrap().is_some(),
        "an already-installed (boot) client must survive the post-unlock helper"
    );
}
