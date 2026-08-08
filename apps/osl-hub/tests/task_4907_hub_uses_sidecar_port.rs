//! TASK 4907: the hub uses the sidecar's owned port.
//!
//! Proves the finish line with real hub starts against the real sidecar
//! binary (direct dial mode, loopback fixtures only):
//!   - 20 hub starts hold 20 sidecar-reported ports, 0 of them 9150;
//!   - a fake listener bound on 127.0.0.1:9150 records 0 SOCKS greetings
//!     while an OSL store send goes through the hub's own reported port;
//!   - the Tor route's shipping source contains 0 live uses of 9150.
//!
//! Run with `--features core --test-threads=1`.

use std::collections::HashSet;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;
use std::time::Duration;

use osl_privacy_hub::tor_pref::{NetworkAuthorization, TorPreference, TorPreferenceState};
use transport::tor::ArtiProxyConfig;

const TOR_BROWSER_SOCKS_PORT: u16 = 9150;

fn workspace_root() -> PathBuf {
    // `apps/osl-hub` -> workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root is two levels above this crate")
        .to_path_buf()
}

/// Build the sidecar once and hand back its binary path. The sidecar is a
/// standalone package, so the hub's own build never compiles it for us.
fn sidecar_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let root = workspace_root();
        let manifest = root.join("apps/osl-tor-sidecar/Cargo.toml");
        let status = Command::new("cargo")
            .args(["build", "--locked", "--manifest-path"])
            .arg(&manifest)
            .status()
            .expect("run cargo build for the sidecar");
        assert!(status.success(), "sidecar build failed: {status}");
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("apps/osl-tor-sidecar/target"));
        let binary = target.join("debug").join("osl-tor-sidecar");
        assert!(
            binary.is_file(),
            "built sidecar binary missing at {}",
            binary.display()
        );
        binary
    })
}

fn sidecar_config() -> ArtiProxyConfig {
    let mut config = ArtiProxyConfig::new(sidecar_binary());
    // Direct dial mode: loopback-only fixture dialing, no Tor bootstrap, so
    // the port-ownership contract is provable hermetically.
    config.args = vec!["--dial-mode".to_string(), "direct".to_string()];
    config.bootstrap_timeout = Duration::from_secs(20);
    config
}

/// One hub start that ends holding a sidecar-owned tunnel.
fn started_hub(directory: &Path) -> TorPreferenceState {
    let state = TorPreferenceState::load_with_arti_proxy_config(
        directory.join("tor-preference.json"),
        Some(sidecar_config()),
    );
    state
        .set_preference(TorPreference::Tor)
        .expect("persist Tor choice");
    state
}

/// 20 hub starts, 20 sidecar-reported ports, none of them 9150. All 20
/// states stay alive at once, so 20 distinct ports means 20 distinct
/// loopback listeners the hub actually holds.
#[test]
fn twenty_hub_starts_use_twenty_reported_sidecar_ports() {
    let mut held = Vec::new();
    let mut ports = Vec::new();
    for index in 0..20 {
        let directory = tempfile::tempdir().expect("temporary preference directory");
        let state = started_hub(directory.path());
        let addr = state
            .tor_socks_addr()
            .expect("read tunnel state")
            .unwrap_or_else(|| panic!("hub start {index} holds no sidecar-owned tunnel"));
        assert!(addr.ip().is_loopback(), "start {index} bound {addr}");
        ports.push(addr.port());
        held.push((directory, state));
    }

    let on_9150 = ports
        .iter()
        .filter(|port| **port == TOR_BROWSER_SOCKS_PORT)
        .count();
    let distinct: HashSet<u16> = ports.iter().copied().collect();
    println!(
        "TASK4907 starts=20 reported_ports={} distinct={} on_9150={} ports={ports:?}",
        ports.len(),
        distinct.len(),
        on_9150
    );
    assert_eq!(ports.len(), 20, "every hub start must report a port");
    assert_eq!(on_9150, 0, "no hub start may sit on the Tor Browser port");
    assert_eq!(
        distinct.len(),
        20,
        "20 live hub starts must hold 20 distinct sidecar-owned ports, got {ports:?}"
    );
}

/// While OSL sends a store upload through its own reported port, a fake
/// Tor Browser listener on 127.0.0.1:9150 hears nothing at all.
#[test]
fn fake_9150_listener_records_zero_greetings_during_an_osl_send() {
    // If this bind fails, something on this machine already owns 9150 and
    // the zero-greeting claim would be unprovable; fail loudly, never skip.
    let fake = TcpListener::bind(("127.0.0.1", TOR_BROWSER_SOCKS_PORT))
        .expect("bind the fake Tor Browser listener on 127.0.0.1:9150");
    let greetings = Arc::new(AtomicUsize::new(0));
    let recorded = Arc::clone(&greetings);
    thread::spawn(move || {
        for stream in fake.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut first = [0_u8; 1];
            if matches!(stream.read(&mut first), Ok(read) if read > 0) {
                recorded.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    let backend = TcpListener::bind("127.0.0.1:0").expect("bind store backend");
    let backend_addr = backend.local_addr().expect("read backend address");
    let (request_seen_tx, request_seen_rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = backend.accept().expect("accept proxied store upload");
        let mut request = [0_u8; 2048];
        let read = stream.read(&mut request).expect("read store upload");
        assert!(std::str::from_utf8(&request[..read])
            .expect("HTTP request is UTF-8")
            .starts_with("POST /v1/blob HTTP/1.1"));
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 43\r\nConnection: close\r\n\r\n{\"id\":\"0011223344556677\",\"expires_at\":1234}",
            )
            .expect("write store response");
        request_seen_tx.send(()).expect("report backend request");
    });

    let directory = tempfile::tempdir().expect("temporary preference directory");
    std::fs::write(
        directory.path().join("keyserver.json"),
        format!(
            r#"{{"cipher_store_url":"http://127.0.0.1:{}"}}"#,
            backend_addr.port()
        ),
    )
    .expect("write local store route");
    let state = started_hub(directory.path());
    let owned = state
        .tor_socks_addr()
        .expect("read tunnel state")
        .expect("hub holds a sidecar-owned tunnel");
    assert_ne!(
        owned.port(),
        TOR_BROWSER_SOCKS_PORT,
        "the owned port must not be the borrowed Tor Browser port"
    );

    let route = state.authorize_store().expect("healthy Tor is authorized");
    assert_eq!(route.authorization(), NetworkAuthorization::Tor);
    let client = route
        .cipher_store_client(directory.path())
        .expect("build routed store client");
    let token = [7_u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    let uploaded = client
        .upload(b"ciphertext", ipc::cipher_store_client::TTL_1H, &token)
        .expect("store upload routes through the sidecar's owned port");
    assert_eq!(uploaded.id_hex, "0011223344556677");
    request_seen_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("backend is reached through the sidecar");

    // Give any stray connection to 9150 time to arrive before counting.
    thread::sleep(Duration::from_millis(300));
    let heard = greetings.load(Ordering::SeqCst);
    println!(
        "TASK4907 owned_port={} greetings_on_9150={heard}",
        owned.port()
    );
    assert_eq!(heard, 0, "the fake 9150 listener must record 0 greetings");
}

/// The Tor route's shipping source never mentions the borrowed port again.
#[test]
fn the_tor_route_has_zero_live_uses_of_9150() {
    let root = workspace_root();
    let mut live = Vec::new();
    for relative in [
        "crates/transport/src/tor.rs",
        "crates/transport/src/lib.rs",
        "apps/osl-hub/src/tor_pref.rs",
    ] {
        let source = std::fs::read_to_string(root.join(relative)).expect("read tor route source");
        // Everything before the test module is the shipping region.
        let shipping = match source.find("\n#[cfg(test)]\nmod ") {
            Some(index) => &source[..index],
            None => source.as_str(),
        };
        for (number, line) in shipping.lines().enumerate() {
            if line.contains(&TOR_BROWSER_SOCKS_PORT.to_string()) {
                live.push(format!("{relative}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    println!("TASK4907 live_9150_uses={}", live.len());
    assert!(
        live.is_empty(),
        "the Tor route still mentions the borrowed port:\n{}",
        live.join("\n")
    );
}
