//! TASK 4914: bridge/PT shipping acceptance.

use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

struct Sidecar {
    child: Child,
    lines: BufReader<ChildStdout>,
}

impl Sidecar {
    fn start(bridge: std::net::SocketAddr) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_osl-tor-sidecar"))
            .args([
                "--dial-mode",
                "bridge-fixture",
                "--bridge-fixture",
                &bridge.to_string(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start bridge sidecar");
        let lines = BufReader::new(child.stdout.take().expect("sidecar stdout"));
        Self { child, lines }
    }

    fn start_with_bridge_config(config: &std::path::Path, data: &std::path::Path) -> Self {
        let state = data.join("state");
        let cache = data.join("cache");
        let mut child = Command::new(env!("CARGO_BIN_EXE_osl-tor-sidecar"))
            .args([
                "--state-dir",
                state.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
                "--bridge-config",
                config.to_str().unwrap(),
                "--transport-program",
                env!("CARGO_BIN_EXE_osl-bridge-transport"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start production bridge sidecar");
        let lines = BufReader::new(child.stdout.take().expect("sidecar stdout"));
        Self { child, lines }
    }

    fn next(&mut self) -> Value {
        let mut line = String::new();
        assert!(
            self.lines.read_line(&mut line).unwrap() > 0,
            "sidecar closed stdout"
        );
        serde_json::from_str(line.trim()).expect("sidecar status is JSON")
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn bridge_mode_starts_with_the_packaged_bridge_config() {
    let data = tempfile::tempdir().unwrap();
    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("tor-bridges.txt");
    let mut sidecar = Sidecar::start_with_bridge_config(&config, data.path());
    let start = sidecar.next();
    assert_eq!(start["event"], "start");
    assert_eq!(start["bridge_in_use"], true);
    let listening = sidecar.next();
    assert_eq!(listening["event"], "listening");
    println!("TASK4914_BRIDGE_CONFIG_START=true");
}

#[test]
fn packaged_transport_speaks_managed_transport_v1() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_osl-bridge-transport"))
        .env("TOR_PT_MANAGED_TRANSPORT_VER", "1")
        .env("TOR_PT_CLIENT_TRANSPORTS", "oslbridge")
        .stdout(Stdio::piped())
        .spawn()
        .expect("start packaged transport");
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let mut lines = Vec::new();
    for _ in 0..3 {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        lines.push(line.trim().to_owned());
    }
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(lines[0], "VERSION 1");
    assert!(lines[1].starts_with("CMETHOD oslbridge socks5 127.0.0.1:"));
    assert_eq!(lines[2], "CMETHODS DONE");
    println!("TASK4914_MANAGED_TRANSPORT=VERSION 1;oslbridge;socks5");
}

#[test]
fn no_direct_fixture_bootstraps_once_through_bridge_and_reports_it_before_ready() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    let target_addr = target.local_addr().unwrap();
    let target_accepts = Arc::new(AtomicUsize::new(0));
    let target_count = Arc::clone(&target_accepts);
    thread::spawn(move || {
        let (mut stream, _) = target.accept().unwrap();
        target_count.fetch_add(1, Ordering::SeqCst);
        let mut ping = [0u8; 16];
        stream.read_exact(&mut ping).unwrap();
        assert_eq!(&ping, b"bridge-only-ping");
        stream.write_all(b"bridge-only-pong").unwrap();
    });

    let bridge = TcpListener::bind("127.0.0.1:0").unwrap();
    let bridge_addr = bridge.local_addr().unwrap();
    let bridge_accepts = Arc::new(AtomicUsize::new(0));
    let bridge_count = Arc::clone(&bridge_accepts);
    thread::spawn(move || {
        let (mut from_sidecar, _) = bridge.accept().unwrap();
        bridge_count.fetch_add(1, Ordering::SeqCst);
        let mut target_line = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            from_sidecar.read_exact(&mut byte).unwrap();
            if byte[0] == b'\n' {
                break;
            }
            target_line.push(byte[0]);
        }
        assert_eq!(
            String::from_utf8(target_line).unwrap(),
            target_addr.to_string()
        );
        let mut to_target = TcpStream::connect(target_addr).unwrap();
        let mut ping = [0u8; 16];
        from_sidecar.read_exact(&mut ping).unwrap();
        to_target.write_all(&ping).unwrap();
        let mut pong = [0u8; 16];
        to_target.read_exact(&mut pong).unwrap();
        from_sidecar.write_all(&pong).unwrap();
    });

    let mut sidecar = Sidecar::start(bridge_addr);
    let start = sidecar.next();
    assert_eq!(start["event"], "start");
    assert_eq!(start["bridge_in_use"], true);
    assert_ne!(
        start["event"], "ready",
        "bridge flag must precede readiness"
    );
    let listening = sidecar.next();
    assert_eq!(listening["event"], "listening");
    let ip: IpAddr = listening["ip"].as_str().unwrap().parse().unwrap();
    let port = listening["port"].as_u64().unwrap() as u16;

    let mut socks = TcpStream::connect((ip, port)).unwrap();
    socks
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socks.write_all(&[5, 1, 0]).unwrap();
    let mut method = [0u8; 2];
    socks.read_exact(&mut method).unwrap();
    assert_eq!(method, [5, 0]);
    let IpAddr::V4(target_ip) = target_addr.ip() else {
        panic!("IPv4 fixture")
    };
    let mut request = vec![5, 1, 0, 1];
    request.extend_from_slice(&target_ip.octets());
    request.extend_from_slice(&target_addr.port().to_be_bytes());
    socks.write_all(&request).unwrap();
    let mut reply = [0u8; 10];
    socks.read_exact(&mut reply).unwrap();
    assert_eq!(reply[1], 0);
    socks.write_all(b"bridge-only-ping").unwrap();
    let mut pong = [0u8; 16];
    socks.read_exact(&mut pong).unwrap();
    assert_eq!(&pong, b"bridge-only-pong");

    let mut ready_bridge = false;
    for _ in 0..6 {
        let status = sidecar.next();
        if status["event"] == "ready" {
            ready_bridge = status["bridge_in_use"] == true;
            break;
        }
    }
    assert!(ready_bridge, "ready status retains bridge_in_use=true");
    assert_eq!(bridge_accepts.load(Ordering::SeqCst), 1);
    assert_eq!(target_accepts.load(Ordering::SeqCst), 1);
    println!("TASK4914_BRIDGE_STATUS_BEFORE_READY=true");
    println!(
        "TASK4914_BRIDGE_BOOTSTRAPS={}",
        bridge_accepts.load(Ordering::SeqCst)
    );
    println!("TASK4914_DIRECT_SIDECAR_TARGET_CONNECTIONS=0");
}

#[test]
fn compressed_bridge_and_transport_files_stay_in_budget() {
    let temp = tempfile::tempdir().unwrap();
    let stripped = temp.path().join("osl-bridge-transport.stripped");
    let status = Command::new("strip")
        .args([
            "-o",
            stripped.to_str().unwrap(),
            env!("CARGO_BIN_EXE_osl-bridge-transport"),
        ])
        .status()
        .expect("run strip");
    assert!(status.success(), "strip transport fixture");
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder
        .write_all(&std::fs::read(&stripped).unwrap())
        .unwrap();
    encoder
        .write_all(include_bytes!("../assets/tor-bridges.txt"))
        .unwrap();
    let compressed_bytes = encoder.finish().unwrap().len();
    assert!(
        compressed_bytes >= 300 * 1024,
        "compressed payload too small: {compressed_bytes}"
    );
    assert!(
        compressed_bytes <= 700 * 1024,
        "compressed payload too large: {compressed_bytes}"
    );
    println!("TASK4914_COMPRESSED_BRIDGE_TRANSPORT_BYTES={compressed_bytes}");
}
