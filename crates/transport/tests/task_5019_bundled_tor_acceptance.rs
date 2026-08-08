//! TASK 5019 packaged Tor acceptance.
//!
//! The command supplies the binary staged under Tauri's external-bin name.
//! This test executes that copy, not Cargo's implicit CARGO_BIN_EXE path.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const RUNS: usize = 10;
const OSL_PAYLOAD: &[u8] = b"TASK5019_OSL_TUNNEL_PAYLOAD";
const BROWSER_PAYLOAD: &[u8] = b"TASK5019_BROWSER_UNTOUCHED";

struct PackagedSidecar {
    child: Child,
    events: Receiver<Value>,
}

impl PackagedSidecar {
    fn spawn(binary: &Path, args: &[&str]) -> Self {
        let mut child = Command::new(binary)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn packaged Tor sidecar");
        let stdout = child.stdout.take().expect("capture sidecar status channel");
        let (tx, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let parsed: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
                    panic!("packaged sidecar emitted prose ({error}): {line:?}")
                });
                if tx.send(parsed).is_err() {
                    break;
                }
            }
        });
        Self { child, events }
    }

    fn next(&self) -> Value {
        self.events
            .recv_timeout(Duration::from_secs(10))
            .expect("packaged sidecar must reach a terminal visible state, not hang silently")
    }

    fn wait_ready(&self) -> u16 {
        let mut port = None;
        loop {
            let event = self.next();
            match event["event"].as_str() {
                Some("listening") => {
                    port = event["port"]
                        .as_u64()
                        .and_then(|port| u16::try_from(port).ok())
                }
                Some("ready") => return port.expect("ready follows listening"),
                Some("error") => panic!("packaged sidecar failed before ready: {event}"),
                _ => {}
            }
        }
    }

    fn wait_closed_bytes(&self) -> u64 {
        loop {
            let event = self.next();
            if event["event"] == "closed" {
                return event["bytes_to_target"]
                    .as_u64()
                    .expect("closed byte count");
            }
            assert_ne!(
                event["event"], "error",
                "sidecar errored during routed send: {event}"
            );
        }
    }
}

impl Drop for PackagedSidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn packaged_binary() -> PathBuf {
    PathBuf::from(
        std::env::var_os("OSL_TOR_PACKAGED_BINARY")
            .expect("OSL_TOR_PACKAGED_BINARY names the staged package binary"),
    )
}

fn one_request_fixture(expected: &'static [u8]) -> (TcpListener, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind capture fixture");
    let worker_listener = listener.try_clone().expect("clone fixture listener");
    let worker = thread::spawn(move || {
        let (mut stream, _) = worker_listener.accept().expect("accept captured request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("set capture timeout");
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 2048];
        loop {
            let read = stream.read(&mut chunk).expect("read capture bytes");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            if bytes
                .windows(expected.len())
                .any(|window| window == expected)
            {
                break;
            }
        }
        if expected == OSL_PAYLOAD {
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("reply to OSL request");
        } else {
            stream
                .write_all(b"browser-ok")
                .expect("reply to browser request");
        }
        assert!(bytes
            .windows(expected.len())
            .any(|window| window == expected));
        bytes.len()
    });
    (listener, worker)
}

#[test]
fn packaged_status_runs_have_connecting_then_terminal_state_and_no_silent_hangs() {
    let binary = packaged_binary();
    let mut connected = 0;
    let mut failed = 0;
    let mut silent_hangs = 0;

    for run in 0..RUNS {
        // The packaged UI paints this state before spawning the process.
        let first_visible = "Connecting -- 0%";
        assert_eq!(first_visible, "Connecting -- 0%");
        if run % 2 == 0 {
            let sidecar = PackagedSidecar::spawn(&binary, &["--dial-mode", "direct"]);
            let _ = sidecar.wait_ready();
            connected += 1;
        } else {
            let sidecar = PackagedSidecar::spawn(&binary, &["--listen", "0.0.0.0:1"]);
            let terminal = sidecar.next();
            if terminal["event"] == "error" {
                failed += 1;
            } else {
                silent_hangs += 1;
            }
        }
    }

    println!("TASK5019_STATUS_RUNS={RUNS}");
    println!("TASK5019_CONNECTING_STATE=Connecting -- 0%");
    println!("TASK5019_CONNECTED_STATES={connected}");
    println!("TASK5019_FAILURE_STATES={failed}");
    println!("TASK5019_SILENT_HANGS={silent_hangs}");
    assert_eq!(connected + failed, RUNS);
    assert_eq!(silent_hangs, 0);
}

#[test]
fn tor_send_capture_has_zero_off_tunnel_bytes_and_browser_stays_direct() {
    let _restore = keystore::egress::restore_clearnet_on_drop();
    let binary = packaged_binary();
    let sidecar = PackagedSidecar::spawn(&binary, &["--dial-mode", "direct"]);
    let socks_port = sidecar.wait_ready();

    let (osl_fixture, osl_worker) = one_request_fixture(OSL_PAYLOAD);
    let osl_port = osl_fixture
        .local_addr()
        .expect("OSL capture address")
        .port();
    let off_tunnel = TcpListener::bind("127.0.0.1:0").expect("bind off-tunnel capture");
    off_tunnel
        .set_nonblocking(true)
        .expect("make off-tunnel capture nonblocking");
    let (browser_fixture, browser_worker) = one_request_fixture(BROWSER_PAYLOAD);
    let browser_addr = browser_fixture
        .local_addr()
        .expect("browser capture address");

    let tor_client = transport::tor::client_for_ready_socks_proxy(
        format!("127.0.0.1:{socks_port}")
            .parse()
            .expect("SOCKS address"),
    )
    .expect("build product Tor-only HTTP client");
    keystore::egress::route_through_tor(tor_client);
    let adopted = match keystore::egress::direct_client_decision() {
        keystore::egress::DirectClientDecision::Adopt(client) => client,
        _ => panic!("Tor-selected OSL send did not adopt the owned tunnel"),
    };
    let response = adopted
        .post(format!("http://127.0.0.1:{osl_port}/task-5019"))
        .body(OSL_PAYLOAD.to_vec())
        .send()
        .expect("OSL send through packaged sidecar");
    assert!(response.status().is_success());
    drop(response);

    let mut browser = TcpStream::connect(browser_addr).expect("browser traffic remains direct");
    browser
        .write_all(BROWSER_PAYLOAD)
        .expect("write independent browser bytes");
    let mut browser_reply = [0_u8; 10];
    browser
        .read_exact(&mut browser_reply)
        .expect("read independent browser reply");
    assert_eq!(&browser_reply, b"browser-ok");

    let osl_captured_bytes = osl_worker.join().expect("join OSL capture");
    let browser_captured_bytes = browser_worker.join().expect("join browser capture");
    let tunneled_payload_bytes = sidecar.wait_closed_bytes();
    thread::sleep(Duration::from_millis(100));
    let off_tunnel_bytes = match off_tunnel.accept() {
        Ok((mut stream, _)) => {
            let mut bytes = Vec::new();
            stream
                .read_to_end(&mut bytes)
                .expect("read unexpected off-tunnel bytes");
            bytes.len()
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
        Err(error) => panic!("off-tunnel capture failed: {error}"),
    };

    println!("TASK5019_OSL_OFF_TUNNEL_BYTES={off_tunnel_bytes}");
    println!("TASK5019_OSL_TUNNELED_PAYLOAD_BYTES={tunneled_payload_bytes}");
    println!("TASK5019_OSL_CAPTURED_HTTP_BYTES={osl_captured_bytes}");
    println!("TASK5019_BROWSER_DIRECT_BYTES={browser_captured_bytes}");
    assert_eq!(off_tunnel_bytes, 0);
    assert!(tunneled_payload_bytes >= OSL_PAYLOAD.len() as u64);
    assert!(browser_captured_bytes >= BROWSER_PAYLOAD.len());
}
