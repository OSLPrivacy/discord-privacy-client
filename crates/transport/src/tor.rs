//! Fail-closed Tor transport for OSL-owned store and keyserver traffic.
//!
//! The only HTTP client exposed here is built with `Proxy::all` and a
//! `socks5h` endpoint. That makes the SOCKS proxy resolve names and applies to
//! both HTTP and HTTPS; callers cannot obtain a route-specific direct client
//! from this module.

use reqwest::blocking::Client;
use reqwest::Proxy;
use serde::Deserialize;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// Configuration for the signed OSL Tor sidecar packaged with the hub.
#[derive(Debug, Clone)]
pub struct TorSidecarConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub status_timeout: Duration,
}

impl TorSidecarConfig {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            status_timeout: Duration::from_secs(45),
        }
    }
}

/// A running Arti proxy and the only client OSL store traffic may use while
/// Tor is selected.
pub struct TorTransport {
    socks_addr: SocketAddr,
    child: Mutex<Option<Child>>,
}

impl TorTransport {
    /// Start the OSL sidecar and wait for its JSON `listening` status line.
    ///
    /// The reported address is the only address retained by the transport.
    /// Returning an error leaves no usable HTTP client, so a selected Tor
    /// route cannot silently fall back to a direct connection.
    pub fn start(config: TorSidecarConfig) -> Result<Self, TorError> {
        let mut child = Command::new(&config.program)
            .args(&config.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|source| TorError::Spawn {
                program: config.program.clone(),
                source,
            })?;

        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                stop_child(&mut child);
                return Err(TorError::MissingStatusStream);
            }
        };
        let (status_tx, status_rx) = mpsc::sync_channel(1);
        thread::spawn(move || drain_status_stream(stdout, status_tx));

        let socks_addr = match status_rx.recv_timeout(config.status_timeout) {
            Ok(Ok(addr)) => addr,
            Ok(Err(error)) => {
                stop_child(&mut child);
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                stop_child(&mut child);
                return Err(TorError::StatusTimeout(config.status_timeout));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let status = child.try_wait().map_err(TorError::ChildStatus)?;
                stop_child(&mut child);
                return match status {
                    Some(status) => Err(TorError::Exited(status)),
                    None => Err(TorError::StatusStreamClosed),
                };
            }
        };

        let transport = Self {
            socks_addr,
            child: Mutex::new(Some(child)),
        };
        Ok(transport)
    }

    /// The exact loopback address reported by this transport's child.
    pub fn socks_addr(&self) -> SocketAddr {
        self.socks_addr
    }

    /// Build the sole store/keyserver HTTP client available through this
    /// transport. `Proxy::all` covers HTTPS as well as HTTP.
    pub fn store_client(&self) -> Result<Client, TorError> {
        self.ensure_running()?;
        build_store_client(self.socks_addr)
    }

    /// Stop the Arti child. This is idempotent and is also performed during
    /// normal teardown and panic unwinding.
    pub fn stop(&self) {
        let Ok(mut child) = self.child.lock() else {
            return;
        };
        if let Some(mut child) = child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn ensure_running(&self) -> Result<(), TorError> {
        let mut child = self.child.lock().map_err(|_| TorError::Poisoned)?;
        let Some(child) = child.as_mut() else {
            return Err(TorError::Stopped);
        };
        match child.try_wait().map_err(TorError::ChildStatus)? {
            Some(status) => Err(TorError::Exited(status)),
            None => Ok(()),
        }
    }
}

#[derive(Deserialize)]
struct SidecarStatusLine {
    event: String,
    addr: Option<String>,
    ip: Option<String>,
    port: Option<u16>,
}

fn drain_status_stream(
    stdout: std::process::ChildStdout,
    status_tx: mpsc::SyncSender<Result<SocketAddr, TorError>>,
) {
    let mut reported = false;
    for line in BufReader::new(stdout).lines() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                if !reported {
                    let _ = status_tx.send(Err(TorError::ReadStatus(error)));
                }
                return;
            }
        };
        let status: SidecarStatusLine = match serde_json::from_str(&line) {
            Ok(status) => status,
            Err(error) => {
                if !reported {
                    let _ = status_tx.send(Err(TorError::InvalidStatus(error)));
                }
                return;
            }
        };
        if reported || status.event != "listening" {
            continue;
        }
        let result = reported_socks_addr(status);
        reported = true;
        let _ = status_tx.send(result);
    }
}

fn reported_socks_addr(status: SidecarStatusLine) -> Result<SocketAddr, TorError> {
    let text = status.addr.ok_or(TorError::IncompleteListeningStatus)?;
    let addr: SocketAddr = text
        .parse()
        .map_err(|_| TorError::InvalidListeningAddress(text.clone()))?;
    if !addr.ip().is_loopback() {
        return Err(TorError::NonLoopbackProxy(addr));
    }
    let expected_ip = addr.ip().to_string();
    if addr.port() == 0
        || status.ip.as_deref() != Some(expected_ip.as_str())
        || status.port != Some(addr.port())
    {
        return Err(TorError::InconsistentListeningStatus(addr));
    }
    Ok(addr)
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Build a Tor-routed client for a SOCKS listener whose lifecycle is already
/// owned by the caller. Production callers should prefer [`TorTransport`];
/// tests use this to exercise routing against a local SOCKS fixture without a
/// real Arti daemon.
pub fn client_for_ready_socks_proxy(socks_addr: SocketAddr) -> Result<Client, TorError> {
    if !socks_addr.ip().is_loopback() {
        return Err(TorError::NonLoopbackProxy(socks_addr));
    }
    build_store_client(socks_addr)
}

/// Open a raw TCP stream through the exact SOCKS5 listener owned by OSL.
///
/// Hostnames are deliberately handed to SOCKS (address type 3), so raw
/// realtime connections do not leak a DNS lookup before entering Tor.
pub fn connect_tcp_via_socks(
    socks_addr: SocketAddr,
    target_host: &str,
    target_port: u16,
    timeout: Duration,
) -> Result<TcpStream, TorError> {
    if !socks_addr.ip().is_loopback() {
        return Err(TorError::NonLoopbackProxy(socks_addr));
    }
    if target_host.is_empty() || target_host.as_bytes().len() > u8::MAX as usize {
        return Err(TorError::InvalidSocksTarget);
    }

    let mut stream =
        TcpStream::connect_timeout(&socks_addr, timeout).map_err(TorError::SocksConnect)?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
        .map_err(TorError::SocksConnect)?;
    stream
        .write_all(&[5, 1, 0])
        .map_err(TorError::SocksConnect)?;
    let mut method = [0_u8; 2];
    stream
        .read_exact(&mut method)
        .map_err(TorError::SocksConnect)?;
    if method != [5, 0] {
        return Err(TorError::SocksRefused);
    }

    let mut request = vec![5, 1, 0];
    match target_host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            request.push(3);
            request.push(target_host.len() as u8);
            request.extend_from_slice(target_host.as_bytes());
        }
    }
    request.extend_from_slice(&target_port.to_be_bytes());
    stream.write_all(&request).map_err(TorError::SocksConnect)?;

    let mut response = [0_u8; 4];
    stream
        .read_exact(&mut response)
        .map_err(TorError::SocksConnect)?;
    if response[0] != 5 || response[1] != 0 {
        return Err(TorError::SocksRefused);
    }
    let address_len = match response[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0_u8; 1];
            stream
                .read_exact(&mut len)
                .map_err(TorError::SocksConnect)?;
            usize::from(len[0])
        }
        _ => return Err(TorError::SocksRefused),
    };
    let mut bound_address_and_port = vec![0_u8; address_len + 2];
    stream
        .read_exact(&mut bound_address_and_port)
        .map_err(TorError::SocksConnect)?;
    Ok(stream)
}

fn build_store_client(socks_addr: SocketAddr) -> Result<Client, TorError> {
    let proxy = Proxy::all(format!("socks5h://{socks_addr}")).map_err(TorError::Proxy)?;
    keystore::blocking_http::off_async_context(|| {
        Client::builder()
            .proxy(proxy)
            .timeout(Duration::from_secs(30))
            .http1_title_case_headers()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("discord-privacy-client/0.0.1")
            .build()
            .map_err(TorError::Client)
    })
}

impl Drop for TorTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Error)]
pub enum TorError {
    #[error("OSL Tor sidecar must listen on loopback, got {0}")]
    NonLoopbackProxy(SocketAddr),
    #[error("failed to start signed OSL Tor sidecar at {program}")]
    Spawn {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("OSL Tor sidecar did not report its SOCKS address within {0:?}")]
    StatusTimeout(Duration),
    #[error("OSL Tor sidecar exited before or during use: {0}")]
    Exited(std::process::ExitStatus),
    #[error("OSL Tor sidecar has been stopped")]
    Stopped,
    #[error("failed to inspect OSL Tor sidecar status")]
    ChildStatus(#[source] io::Error),
    #[error("OSL Tor sidecar process state was poisoned")]
    Poisoned,
    #[error("OSL Tor sidecar stdout was not captured")]
    MissingStatusStream,
    #[error("OSL Tor sidecar status stream closed before reporting a listener")]
    StatusStreamClosed,
    #[error("failed to read OSL Tor sidecar status")]
    ReadStatus(#[source] io::Error),
    #[error("OSL Tor sidecar emitted invalid JSON status")]
    InvalidStatus(#[source] serde_json::Error),
    #[error("OSL Tor sidecar listening status omitted its address")]
    IncompleteListeningStatus,
    #[error("OSL Tor sidecar reported an invalid listening address: {0}")]
    InvalidListeningAddress(String),
    #[error("OSL Tor sidecar listening fields disagree for {0}")]
    InconsistentListeningStatus(SocketAddr),
    #[error("invalid SOCKS proxy configuration")]
    Proxy(#[source] reqwest::Error),
    #[error("failed to build Tor-only HTTP client")]
    Client(#[source] reqwest::Error),
    #[error("failed to connect through OSL's SOCKS tunnel")]
    SocksConnect(#[source] io::Error),
    #[error("OSL's SOCKS tunnel refused the connection")]
    SocksRefused,
    #[error("the SOCKS target is invalid")]
    InvalidSocksTarget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::mpsc;

    #[test]
    fn store_requests_use_socks_and_proxy_resolves_the_store_hostname() {
        let backend = TcpListener::bind("127.0.0.1:0").expect("bind store backend");
        let backend_addr = backend.local_addr().expect("read backend address");
        let (request_seen_tx, request_seen_rx) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = backend.accept().expect("accept proxied store request");
            let mut request = [0_u8; 1024];
            let read = stream.read(&mut request).expect("read store request");
            assert!(std::str::from_utf8(&request[..read])
                .expect("HTTP request is UTF-8")
                .starts_with("GET /v1/blob/capability HTTP/1.1"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .expect("write store response");
            request_seen_tx.send(()).expect("report backend request");
        });

        let proxy = TcpListener::bind("127.0.0.1:0").expect("bind SOCKS proxy");
        let proxy_addr = proxy.local_addr().expect("read proxy address");
        let (proxy_seen_tx, proxy_seen_rx) = mpsc::channel();
        thread::spawn(move || run_test_socks_proxy(proxy, backend_addr, proxy_seen_tx));

        let client = build_store_client(proxy_addr).expect("build SOCKS client");
        let response = client
            .get(format!(
                "http://store.invalid:{}/v1/blob/capability",
                backend_addr.port()
            ))
            .send()
            .expect("store request must be proxied");

        assert!(response.status().is_success());
        assert_eq!(
            proxy_seen_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "store.invalid"
        );
        request_seen_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("backend only receives the request after SOCKS forwarding");
    }

    #[test]
    fn https_store_requests_also_enter_the_socks_tunnel() {
        let proxy = TcpListener::bind("127.0.0.1:0").expect("bind SOCKS proxy");
        let proxy_addr = proxy.local_addr().expect("read proxy address");
        let (proxy_seen_tx, proxy_seen_rx) = mpsc::channel();
        thread::spawn(move || run_socks_connect_probe(proxy, proxy_seen_tx));

        let client = build_store_client(proxy_addr).expect("build SOCKS client");
        let result = client
            .get("https://store.invalid/v1/blob/capability")
            .send();

        // The probe deliberately closes before TLS can complete. The important
        // property is that HTTPS reached SOCKS first; a direct client would
        // attempt local resolution and the probe would see nothing.
        assert!(result.is_err(), "the test SOCKS proxy is not a TLS origin");
        assert_eq!(
            proxy_seen_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "store.invalid"
        );
    }

    fn run_test_socks_proxy(
        proxy: TcpListener,
        backend: SocketAddr,
        proxy_seen_tx: mpsc::Sender<String>,
    ) {
        let (mut client, _) = proxy.accept().expect("accept SOCKS client");
        let mut greeting = [0_u8; 2];
        client
            .read_exact(&mut greeting)
            .expect("read SOCKS greeting");
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0_u8; greeting[1] as usize];
        client.read_exact(&mut methods).expect("read SOCKS methods");
        assert!(methods.contains(&0), "SOCKS client offers no-auth");
        client.write_all(&[5, 0]).expect("accept no-auth SOCKS");

        let mut header = [0_u8; 4];
        client
            .read_exact(&mut header)
            .expect("read SOCKS request header");
        assert_eq!(&header[..3], &[5, 1, 0]);
        let host = match header[3] {
            3 => {
                let mut length = [0_u8; 1];
                client
                    .read_exact(&mut length)
                    .expect("read SOCKS hostname length");
                let mut name = vec![0_u8; length[0] as usize];
                client.read_exact(&mut name).expect("read SOCKS hostname");
                String::from_utf8(name).expect("SOCKS hostname is UTF-8")
            }
            other => panic!("expected SOCKS hostname resolution, got address type {other}"),
        };
        let mut port = [0_u8; 2];
        client.read_exact(&mut port).expect("read SOCKS port");
        assert_eq!(u16::from_be_bytes(port), backend.port());
        proxy_seen_tx.send(host).expect("report SOCKS use");

        let mut upstream = TcpStream::connect(backend).expect("connect store backend");
        client
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
            .expect("confirm SOCKS connection");
        let mut client_to_upstream = client.try_clone().expect("clone SOCKS client");
        let mut upstream_for_copy = upstream.try_clone().expect("clone backend stream");
        let copy = thread::spawn(move || {
            io::copy(&mut client_to_upstream, &mut upstream_for_copy).expect("forward request");
            let _ = upstream_for_copy.shutdown(Shutdown::Write);
        });
        io::copy(&mut upstream, &mut client).expect("forward response");
        copy.join().expect("join request forwarder");
    }

    fn run_socks_connect_probe(proxy: TcpListener, proxy_seen_tx: mpsc::Sender<String>) {
        let (mut client, _) = proxy.accept().expect("accept SOCKS client");
        let mut greeting = [0_u8; 2];
        client
            .read_exact(&mut greeting)
            .expect("read SOCKS greeting");
        let mut methods = vec![0_u8; greeting[1] as usize];
        client.read_exact(&mut methods).expect("read SOCKS methods");
        assert!(methods.contains(&0), "SOCKS client offers no-auth");
        client.write_all(&[5, 0]).expect("accept no-auth SOCKS");

        let mut header = [0_u8; 4];
        client
            .read_exact(&mut header)
            .expect("read SOCKS request header");
        assert_eq!(&header[..3], &[5, 1, 0]);
        assert_eq!(header[3], 3, "SOCKS proxy must resolve the hostname");
        let mut length = [0_u8; 1];
        client
            .read_exact(&mut length)
            .expect("read SOCKS hostname length");
        let mut name = vec![0_u8; length[0] as usize];
        client.read_exact(&mut name).expect("read SOCKS hostname");
        let host = String::from_utf8(name).expect("SOCKS hostname is UTF-8");
        let mut port = [0_u8; 2];
        client.read_exact(&mut port).expect("read SOCKS port");
        assert_eq!(u16::from_be_bytes(port), 443);
        proxy_seen_tx.send(host).expect("report SOCKS use");

        // Report success then close: the TLS handshake is expected to fail,
        // after the routing assertion above has been observed.
        client
            .write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0])
            .expect("confirm SOCKS connection");
    }
}
