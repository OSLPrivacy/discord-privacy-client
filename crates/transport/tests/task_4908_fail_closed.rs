//! TASK 4908: one route decision governs the same ten ordinary sends in both
//! the blocked and ready states, and the desktop egress inventory stays named.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use transport::tor::{TorSidecarConfig, TorTransport};

const SENDS: usize = 10;

struct HttpFixture {
    port: u16,
    writes: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl HttpFixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind HTTP write witness");
        listener
            .set_nonblocking(true)
            .expect("make HTTP witness nonblocking");
        let port = listener.local_addr().expect("HTTP witness address").port();
        let writes = Arc::new(AtomicUsize::new(0));
        let thread_writes = Arc::clone(&writes);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let mut request = [0_u8; 2048];
                        if stream.read(&mut request).is_ok_and(|read| read > 0) {
                            thread_writes.fetch_add(1, Ordering::SeqCst);
                            let _ = stream.write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                            );
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("HTTP witness accept failed: {error}"),
                }
            }
        });
        Self {
            port,
            writes,
            stop,
            worker: Some(worker),
        }
    }

    fn url(&self, attempt: usize) -> String {
        format!("http://127.0.0.1:{}/task-4908/{attempt}", self.port)
    }

    fn count(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    fn wait_for(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.count() < expected && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// This is the same constructor contract used by ordinary OSL HTTP clients.
fn attempt_send(url: &str) -> Result<(), String> {
    let client = match keystore::egress::direct_client_decision() {
        keystore::egress::DirectClientDecision::Build => {
            return Err("task 4908 unexpectedly permitted a direct route".to_owned())
        }
        keystore::egress::DirectClientDecision::Adopt(client) => *client,
        keystore::egress::DirectClientDecision::Refuse => {
            return Err(keystore::egress::TOR_UNAVAILABLE.to_owned())
        }
    };
    let response = client.get(url).send().map_err(|error| error.to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("routed fixture returned {}", response.status()))
    }
}

#[test]
fn ten_blocked_sends_write_nothing_then_ten_ready_sends_use_owned_socks() {
    let _restore = keystore::egress::restore_clearnet_on_drop();
    let fixture = HttpFixture::start();

    keystore::egress::seal();
    let blocked_results = (0..SENDS)
        .map(|attempt| attempt_send(&fixture.url(attempt)))
        .collect::<Vec<_>>();
    thread::sleep(Duration::from_millis(50));
    let blocked_writes = fixture.count();
    let blocked_errors = blocked_results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    println!("TASK4908_BLOCKED_ATTEMPTS={}", blocked_results.len());
    println!("TASK4908_BLOCKED_REFUSALS={}", blocked_errors.len());
    println!("TASK4908_BLOCKED_NETWORK_WRITES={blocked_writes}");
    println!(
        "TASK4908_REFUSAL={}",
        blocked_errors
            .first()
            .map(|error| error.as_str())
            .unwrap_or("NO REFUSAL")
    );
    assert_eq!(blocked_results.len(), SENDS);
    assert!(blocked_errors
        .iter()
        .all(|error| error.as_str() == keystore::egress::TOR_UNAVAILABLE));
    assert_eq!(
        blocked_errors.len(),
        SENDS,
        "a Tor-selected send did not return the exact refusal"
    );
    assert_eq!(blocked_writes, 0, "a blocked send reached the network");

    let sidecar = std::env::var_os("OSL_TOR_SIDECAR_TEST_BIN")
        .map(PathBuf::from)
        .expect("OSL_TOR_SIDECAR_TEST_BIN must name the task-4905 sidecar binary");
    let mut config = TorSidecarConfig::new(sidecar);
    config.args = vec!["--dial-mode".to_owned(), "direct".to_owned()];
    config.status_timeout = Duration::from_secs(10);
    let transport = TorTransport::start(config).expect("start the OSL-owned sidecar");
    let owned_socks_port = transport.socks_addr().port();
    let client = transport.store_client().expect("build owned SOCKS client");
    keystore::egress::route_through_owned_tor(client, transport.socks_addr());
    assert!(matches!(
        keystore::egress::socket_route_decision(),
        keystore::egress::SocketRouteDecision::Tor(addr)
            if addr == transport.socks_addr()
    ));

    let ready_results = (0..SENDS)
        .map(|attempt| attempt_send(&fixture.url(attempt)))
        .collect::<Vec<_>>();
    fixture.wait_for(SENDS);
    let ready_writes = fixture.count() - blocked_writes;

    println!("TASK4908_READY_ATTEMPTS={}", ready_results.len());
    println!("TASK4908_READY_OWNED_SOCKS_PORT={owned_socks_port}");
    println!("TASK4908_READY_WRITES_THROUGH_OWNED_SOCKS={ready_writes}");

    assert!(ready_results.iter().all(Result::is_ok));
    assert_eq!(ready_writes, SENDS);
}

struct InventorySite {
    file: &'static str,
    route_marker: &'static str,
}

/// All non-LAN desktop connection boundaries route through the process-wide
/// decision. Sidecar dialing is the tunnel implementation, not a bypass.
const ROUTED_NON_LAN_EGRESS: &[InventorySite] = &[
    InventorySite {
        file: "apps/osl-hub/src/osl_mail.rs",
        route_marker: "keystore::egress::direct_client_decision()",
    },
    InventorySite {
        file: "crates/ipc/src/cipher_store_client.rs",
        route_marker: "keystore::egress::direct_client_decision()",
    },
    InventorySite {
        file: "crates/ipc/src/commands.rs",
        route_marker: "fn update_site_http_client()",
    },
    InventorySite {
        file: "crates/keystore/src/client.rs",
        route_marker: "crate::egress::direct_client_decision()",
    },
    InventorySite {
        file: "crates/keystore/src/username.rs",
        route_marker: "crate::egress::direct_client_decision()",
    },
    InventorySite {
        file: "apps/osl-hub/src/realtime_pipe.rs",
        route_marker: "keystore::egress::socket_route_decision()",
    },
    InventorySite {
        file: "apps/osl-hub/src/main.rs",
        route_marker: "fn routed_hub_updater(",
    },
];

/// Deferred to Liam's task 4916 product ruling. This is a decision list, not
/// permission to call these paths silent or Tor-routed today.
const LAN_DECISION_LIST_4916: &[InventorySite] = &[
    InventorySite {
        file: "apps/osl-hub/src/osl_lan.rs",
        route_marker: "TcpStream::connect_timeout(&address, IO_TIMEOUT)",
    },
    InventorySite {
        file: "apps/osl-hub/src/osl_lan.rs",
        route_marker: "UdpSocket::bind(\"0.0.0.0:0\")",
    },
    InventorySite {
        file: "apps/osl-hub/src/osl_lan.rs",
        route_marker: ".write_all(&(wire.len() as u32).to_be_bytes())",
    },
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

#[test]
fn every_non_lan_egress_is_routed_and_lan_is_named_for_4916() {
    let root = workspace_root();
    for site in ROUTED_NON_LAN_EGRESS
        .iter()
        .chain(LAN_DECISION_LIST_4916.iter())
    {
        let source = std::fs::read_to_string(root.join(site.file))
            .unwrap_or_else(|error| panic!("read {}: {error}", site.file));
        assert!(
            source.contains(site.route_marker),
            "{} lost inventory marker {}",
            site.file,
            site.route_marker
        );
    }

    let browser = std::fs::read_to_string(root.join("apps/osl-hub/src/website_driver.rs"))
        .expect("read loopback browser controller");
    assert!(browser.contains("ip.is_loopback()"));
    assert!(browser.contains("http://127.0.0.1:{port}"));

    println!(
        "TASK4908_ROUTED_NON_LAN_EGRESS_SITES={}",
        ROUTED_NON_LAN_EGRESS.len()
    );
    println!(
        "TASK4908_4916_LAN_DECISION_SITES={}",
        LAN_DECISION_LIST_4916.len()
    );
    println!("TASK4908_LOCAL_LOOPBACK_CONTROL_SITES=2");
    assert_eq!(ROUTED_NON_LAN_EGRESS.len(), 7);
    assert_eq!(LAN_DECISION_LIST_4916.len(), 3);
}
