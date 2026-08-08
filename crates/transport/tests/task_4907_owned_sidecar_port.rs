//! TASK 4907: consume the SOCKS address reported by OSL's own sidecar and
//! prove the live route never borrows Tor Browser's conventional port.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use transport::tor::{TorSidecarConfig, TorTransport};

const TOR_BROWSER_PORT: u16 = 9150;
const STARTS: usize = 20;

struct ListenerGuard {
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_http_fixture() -> (u16, Arc<AtomicUsize>, ListenerGuard) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback HTTP fixture");
    listener
        .set_nonblocking(true)
        .expect("make HTTP fixture nonblocking");
    let port = listener.local_addr().expect("HTTP fixture address").port();
    let requests = Arc::new(AtomicUsize::new(0));
    let thread_requests = Arc::clone(&requests);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut request = [0_u8; 2048];
                    if stream.read(&mut request).is_ok_and(|read| read > 0) {
                        thread_requests.fetch_add(1, Ordering::SeqCst);
                        let _ = stream.write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("HTTP fixture accept failed: {error}"),
            }
        }
    });
    (
        port,
        requests,
        ListenerGuard {
            stop,
            thread: Some(handle),
        },
    )
}

fn consume_socks_target(stream: &mut TcpStream, address_type: u8) -> std::io::Result<()> {
    let address_bytes = match address_type {
        1 => 4,
        4 => 16,
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length)?;
            usize::from(length[0])
        }
        _ => return Ok(()),
    };
    let mut target_and_port = vec![0_u8; address_bytes + 2];
    stream.read_exact(&mut target_and_port)
}

fn record_decoy_connection(mut stream: TcpStream, accepts: &AtomicUsize, greetings: &AtomicUsize) {
    accepts.fetch_add(1, Ordering::SeqCst);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut greeting = [0_u8; 2];
    if stream.read_exact(&mut greeting).is_err() || greeting[0] != 5 {
        return;
    }
    greetings.fetch_add(1, Ordering::SeqCst);
    let mut methods = vec![0_u8; usize::from(greeting[1])];
    if stream.read_exact(&mut methods).is_err() || stream.write_all(&[5, 0]).is_err() {
        return;
    }
    let mut request = [0_u8; 4];
    if stream.read_exact(&mut request).is_err() {
        return;
    }
    let _ = consume_socks_target(&mut stream, request[3]);
    // A break-it route aimed at 9150 must fail promptly, after leaving a
    // measurable greeting, rather than waiting for Reqwest's full timeout.
    let _ = stream.write_all(&[5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
    let _ = stream.shutdown(Shutdown::Both);
}

fn spawn_9150_decoy() -> (Arc<AtomicUsize>, Arc<AtomicUsize>, ListenerGuard) {
    let listener = TcpListener::bind(("127.0.0.1", TOR_BROWSER_PORT))
        .expect("bind the Tor Browser port decoy on 127.0.0.1:9150");
    listener
        .set_nonblocking(true)
        .expect("make 9150 decoy nonblocking");
    let accepts = Arc::new(AtomicUsize::new(0));
    let greetings = Arc::new(AtomicUsize::new(0));
    let thread_accepts = Arc::clone(&accepts);
    let thread_greetings = Arc::clone(&greetings);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    record_decoy_connection(stream, &thread_accepts, &thread_greetings)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) => panic!("9150 decoy accept failed: {error}"),
            }
        }
    });
    (
        accepts,
        greetings,
        ListenerGuard {
            stop,
            thread: Some(handle),
        },
    )
}

fn source_contract() -> usize {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let transport_source = std::fs::read_to_string(manifest.join("src/tor.rs"))
        .expect("read transport Tor route source");
    let hub_source = std::fs::read_to_string(manifest.join("../../apps/osl-hub/src/tor_pref.rs"))
        .expect("read hub Tor preference source");
    let main_source = std::fs::read_to_string(manifest.join("../../apps/osl-hub/src/main.rs"))
        .expect("read hub startup source");

    assert!(
        hub_source.contains("TorTransport::start(config)"),
        "hub no longer starts the owned Tor transport"
    );
    assert!(
        main_source.contains("load_with_tor_sidecar_config"),
        "hub startup no longer installs the owned-sidecar preference state"
    );
    transport_source.matches("127.0.0.1:9150").count()
        + hub_source.matches("127.0.0.1:9150").count()
}

#[test]
fn twenty_starts_route_through_twenty_reported_sidecar_ports() {
    let sidecar = std::env::var_os("OSL_TOR_SIDECAR_TEST_BIN")
        .map(PathBuf::from)
        .expect("OSL_TOR_SIDECAR_TEST_BIN must name the real task-4905 sidecar binary");
    assert!(
        sidecar.is_file(),
        "sidecar binary is missing: {}",
        sidecar.display()
    );

    let (fixture_port, fixture_requests, _fixture_guard) = spawn_http_fixture();
    let (decoy_accepts, decoy_greetings, _decoy_guard) = spawn_9150_decoy();
    let mut transports = Vec::with_capacity(STARTS);
    let mut ports = Vec::with_capacity(STARTS);
    for _ in 0..STARTS {
        let mut config = TorSidecarConfig::new(&sidecar);
        config.args = vec!["--dial-mode".to_owned(), "direct".to_owned()];
        config.status_timeout = Duration::from_secs(10);
        let transport = TorTransport::start(config).expect("start hub-owned sidecar transport");
        ports.push(transport.socks_addr().port());
        transports.push(transport);
    }

    // Run all sends before aggregate assertions. A deliberately broken route
    // then reaches the decoy and produces the 4907b red metric.
    let mut successful_routes = 0usize;
    for (request, transport) in transports.iter().enumerate() {
        let result = transport.store_client().and_then(|http| {
            http.get(format!(
                "http://127.0.0.1:{fixture_port}/task-4907/{request}"
            ))
            .send()
            .map_err(transport::tor::TorError::Client)
        });
        if result.is_ok_and(|response| response.status().is_success()) {
            successful_routes += 1;
        }
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    while fixture_requests.load(Ordering::SeqCst) < STARTS && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(50));

    let distinct_ports = ports.iter().copied().collect::<HashSet<_>>().len();
    let ports_equal_9150 = ports
        .iter()
        .filter(|port| **port == TOR_BROWSER_PORT)
        .count();
    let fixture_count = fixture_requests.load(Ordering::SeqCst);
    let decoy_accept_count = decoy_accepts.load(Ordering::SeqCst);
    let decoy_greeting_count = decoy_greetings.load(Ordering::SeqCst);
    let live_route_uses = source_contract();

    println!("TASK4907_REPORTED_PORTS={ports:?}");
    println!("TASK4907_DISTINCT_REPORTED_PORTS={distinct_ports}");
    println!("TASK4907_PORTS_EQUAL_9150={ports_equal_9150}");
    println!("TASK4907_SUCCESSFUL_OWNED_ROUTES={successful_routes}");
    println!("TASK4907_HTTP_FIXTURE_REQUESTS={fixture_count}");
    println!("TASK4907_FAKE_9150_ACCEPTS={decoy_accept_count}");
    println!("TASK4907_FAKE_9150_GREETINGS={decoy_greeting_count}");
    println!("TASK4907_LIVE_ROUTE_EXACT_127_0_0_1_9150_USES={live_route_uses}");

    assert_eq!(ports.len(), STARTS, "expected one reported port per start");
    assert_eq!(distinct_ports, STARTS, "expected 20 distinct owned ports");
    assert_eq!(ports_equal_9150, 0, "transport adopted Tor Browser's port");
    assert_eq!(
        successful_routes, STARTS,
        "not every client used its sidecar"
    );
    assert_eq!(fixture_count, STARTS, "fixture missed routed requests");
    assert_eq!(decoy_accept_count, 0, "OSL connected to the 9150 decoy");
    assert_eq!(decoy_greeting_count, 0, "OSL sent a SOCKS greeting to 9150");
    assert_eq!(live_route_uses, 0, "live route still names 127.0.0.1:9150");
}
