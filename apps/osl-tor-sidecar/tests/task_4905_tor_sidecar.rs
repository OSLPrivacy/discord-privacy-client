//! TASK 4905: the OSL-owned Tor sidecar.
//!
//! Proves the finish line with real processes:
//!   - 20 starts bind 20 loopback addresses, each with an OS-chosen
//!     ephemeral port (requested port 0), and none of them is 9150;
//!   - every stdout line parses as JSON and 0 lines are prose;
//!   - a SOCKS CONNECT through the reported port reaches a fixture TCP
//!     server exactly once;
//!   - the sidecar's own source stays between 400 and 650 non-test lines
//!     with 0 uses of 9150.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

const TOR_BROWSER_SOCKS_PORT: u16 = 9150;
const FIXTURE_PING: &[u8] = b"task4905-ping";
const FIXTURE_PONG: &[u8] = b"task4905-fixture-pong";

/// A running sidecar process with its stdout captured line by line.
struct Sidecar {
    child: Child,
    reader: BufReader<ChildStdout>,
    /// Every stdout line ever read from this process, verbatim.
    lines: Vec<String>,
}

impl Sidecar {
    fn start() -> Sidecar {
        let mut child = Command::new(env!("CARGO_BIN_EXE_osl-tor-sidecar"))
            .args(["--dial-mode", "direct"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn osl-tor-sidecar");
        let reader = BufReader::new(child.stdout.take().expect("captured stdout"));
        Sidecar {
            child,
            reader,
            lines: Vec::new(),
        }
    }

    /// Read the next stdout line and require it to be one JSON object.
    fn next_json(&mut self) -> Value {
        let mut line = String::new();
        let got = self.reader.read_line(&mut line).expect("read sidecar stdout");
        assert!(got > 0, "sidecar stdout closed before the expected event");
        let trimmed = line.trim_end_matches('\n').to_string();
        let parsed = serde_json::from_str::<Value>(&trimmed).unwrap_or_else(|error| {
            panic!("stdout line is not JSON ({error}): {trimmed:?}")
        });
        self.lines.push(trimmed);
        parsed
    }

    /// Consume events until the `listening` event; return (ip, port) and
    /// the `requested_listen` string from the preceding `start` event.
    fn wait_listening(&mut self) -> (IpAddr, u16, String) {
        let mut requested = String::new();
        loop {
            let event = self.next_json();
            match event["event"].as_str().expect("event tag") {
                "start" => {
                    requested = event["requested_listen"]
                        .as_str()
                        .expect("requested_listen")
                        .to_string();
                }
                "listening" => {
                    let ip: IpAddr = event["ip"].as_str().expect("ip").parse().expect("ip parses");
                    let port =
                        u16::try_from(event["port"].as_u64().expect("port")).expect("port fits");
                    return (ip, port, requested);
                }
                other => panic!("unexpected event before listening: {other}"),
            }
        }
    }

    /// Kill the process and parse every remaining stdout line as JSON.
    /// Returns the number of lines that failed to parse (prose lines).
    fn finish(mut self) -> (Vec<String>, usize) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let mut rest = String::new();
        let _ = self.reader.read_to_string(&mut rest);
        for line in rest.lines() {
            self.lines.push(line.to_string());
        }
        let lines = std::mem::take(&mut self.lines);
        let prose = lines
            .iter()
            .filter(|line| serde_json::from_str::<Value>(line).is_err())
            .count();
        (lines, prose)
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 20 starts bind 20 loopback addresses, every port picked by the OS
/// from a port-0 request, none of them the Tor Browser SOCKS port.
#[test]
fn twenty_starts_bind_twenty_os_chosen_loopback_ports() {
    let mut sidecars: Vec<Sidecar> = (0..20).map(|_| Sidecar::start()).collect();
    let mut ports = HashSet::new();
    for sidecar in &mut sidecars {
        let (ip, port, requested) = sidecar.wait_listening();
        assert!(ip.is_loopback(), "listener bound non-loopback {ip}");
        assert_eq!(requested, "127.0.0.1:0", "sidecar must ask for port 0");
        assert_ne!(port, 0, "reported port must be the OS-chosen one");
        assert_ne!(port, TOR_BROWSER_SOCKS_PORT, "must not borrow 9150");
        ports.insert(port);
    }
    // All 20 processes are alive at once, so 20 distinct ports means 20
    // distinct loopback socket addresses actually bound.
    assert_eq!(ports.len(), 20, "expected 20 distinct loopback bindings");
    let mut total_prose = 0;
    for sidecar in sidecars {
        let (_, prose) = sidecar.finish();
        total_prose += prose;
    }
    assert_eq!(total_prose, 0, "stdout must contain 0 prose log lines");
}

/// One SOCKS CONNECT through the reported port reaches the fixture TCP
/// server exactly once, and the whole stdout stream is JSON-only.
#[test]
fn socks_connect_reaches_fixture_exactly_once() {
    let fixture = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let fixture_addr = fixture.local_addr().expect("fixture addr");
    let accepts = Arc::new(AtomicUsize::new(0));
    let fixture_accepts = Arc::clone(&accepts);
    thread::spawn(move || {
        for stream in fixture.incoming() {
            let Ok(mut stream) = stream else { break };
            fixture_accepts.fetch_add(1, Ordering::SeqCst);
            let mut ping = vec![0u8; FIXTURE_PING.len()];
            if stream.read_exact(&mut ping).is_ok() {
                assert_eq!(ping, FIXTURE_PING, "fixture got mangled payload");
                let _ = stream.write_all(FIXTURE_PONG);
            }
        }
    });

    let mut sidecar = Sidecar::start();
    let (ip, port, _) = sidecar.wait_listening();

    // Hand-rolled SOCKS5 CONNECT: greeting, no-auth, IPv4 request.
    let mut conn = TcpStream::connect((ip, port)).expect("connect to sidecar SOCKS port");
    conn.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    conn.write_all(&[0x05, 0x01, 0x00]).expect("send greeting");
    let mut method = [0u8; 2];
    conn.read_exact(&mut method).expect("read method choice");
    assert_eq!(method, [0x05, 0x00], "expected no-auth SOCKS5");

    let ip4 = match fixture_addr.ip() {
        IpAddr::V4(ip4) => ip4.octets(),
        other => panic!("fixture bound unexpected family {other}"),
    };
    let fixture_port = fixture_addr.port().to_be_bytes();
    let mut request = vec![0x05, 0x01, 0x00, 0x01];
    request.extend_from_slice(&ip4);
    request.extend_from_slice(&fixture_port);
    conn.write_all(&request).expect("send CONNECT");
    let mut reply = [0u8; 10];
    conn.read_exact(&mut reply).expect("read CONNECT reply");
    assert_eq!(reply[0], 0x05, "reply version");
    assert_eq!(reply[1], 0x00, "CONNECT must succeed through the sidecar");

    conn.write_all(FIXTURE_PING).expect("send payload");
    let mut pong = vec![0u8; FIXTURE_PONG.len()];
    conn.read_exact(&mut pong).expect("read fixture response");
    assert_eq!(pong, FIXTURE_PONG, "payload must round-trip the relay");
    drop(conn);

    // Give any spurious second connection time to appear before counting.
    thread::sleep(Duration::from_millis(300));
    assert_eq!(
        accepts.load(Ordering::SeqCst),
        1,
        "fixture must be reached exactly once"
    );

    let (lines, prose) = sidecar.finish();
    assert_eq!(prose, 0, "stdout must contain 0 prose log lines");
    let events: Vec<String> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("json line")["event"]
            .as_str()
            .expect("event tag")
            .to_string())
        .collect();
    assert!(events.iter().any(|e| e == "connect_ok"), "missing connect_ok in {events:?}");
}

/// The sidecar's own source is 400-650 non-test lines, contains no test
/// code (so those lines are all non-test lines), and never mentions the
/// borrowed Tor Browser port.
#[test]
fn source_stays_in_budget_and_never_uses_the_borrowed_port() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut total_lines = 0usize;
    let mut borrowed_port_uses = 0usize;
    let mut test_markers = 0usize;
    let mut files = 0usize;
    for entry in std::fs::read_dir(&src).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).expect("read source file");
        total_lines += text.lines().count();
        borrowed_port_uses += text.matches(&TOR_BROWSER_SOCKS_PORT.to_string()).count();
        test_markers += text.matches("#[cfg(test)]").count() + text.matches("#[test]").count();
    }
    assert!(files >= 4, "expected the sidecar modules under src/");
    assert_eq!(test_markers, 0, "src/ must hold only non-test lines");
    assert!(
        total_lines > 400 && total_lines < 650,
        "sidecar source must stay between 400 and 650 non-test lines, got {total_lines}"
    );
    assert_eq!(borrowed_port_uses, 0, "source must never mention port 9150");
}
