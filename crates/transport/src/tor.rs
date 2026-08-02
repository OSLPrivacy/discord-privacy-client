//! Fail-closed Tor transport for OSL-owned store and keyserver traffic.
//!
//! The only HTTP client exposed here is built with `Proxy::all` and a
//! `socks5h` endpoint. That makes the SOCKS proxy resolve names and applies to
//! both HTTP and HTTPS; callers cannot obtain a route-specific direct client
//! from this module.

use reqwest::blocking::Client;
use reqwest::Proxy;
use std::io;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

/// The loopback SOCKS listener exposed by Arti's proxy mode.
pub const DEFAULT_SOCKS_ADDR: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 9150));

/// Configuration for the signed Arti proxy executable packaged with OSL.
#[derive(Debug, Clone)]
pub struct ArtiProxyConfig {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub socks_addr: SocketAddr,
    pub bootstrap_timeout: Duration,
    pub readiness_poll_interval: Duration,
}

impl ArtiProxyConfig {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            socks_addr: DEFAULT_SOCKS_ADDR,
            bootstrap_timeout: Duration::from_secs(45),
            readiness_poll_interval: Duration::from_millis(200),
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
    /// Start Arti and wait until its SOCKS listener accepts connections.
    ///
    /// Returning an error leaves no usable HTTP client, so callers cannot
    /// silently fall back to a direct connection when Tor is selected.
    pub fn start(config: ArtiProxyConfig) -> Result<Self, TorError> {
        if !config.socks_addr.ip().is_loopback() {
            return Err(TorError::NonLoopbackProxy(config.socks_addr));
        }

        let child = Command::new(&config.program)
            .args(&config.args)
            .spawn()
            .map_err(|source| TorError::Spawn {
                program: config.program,
                source,
            })?;
        let transport = Self {
            socks_addr: config.socks_addr,
            child: Mutex::new(Some(child)),
        };

        if let Err(error) =
            transport.wait_for_bootstrap(config.bootstrap_timeout, config.readiness_poll_interval)
        {
            transport.stop();
            return Err(error);
        }
        Ok(transport)
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

    fn wait_for_bootstrap(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<(), TorError> {
        let deadline = Instant::now() + timeout;
        loop {
            self.ensure_running()?;
            if TcpStream::connect_timeout(&self.socks_addr, poll_interval).is_ok() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(TorError::BootstrapTimeout(timeout));
            }
            thread::sleep(poll_interval);
        }
    }
}

fn build_store_client(socks_addr: SocketAddr) -> Result<Client, TorError> {
    let proxy = Proxy::all(format!("socks5h://{socks_addr}")).map_err(TorError::Proxy)?;
    Client::builder()
        .proxy(proxy)
        .build()
        .map_err(TorError::Client)
}

impl Drop for TorTransport {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Error)]
pub enum TorError {
    #[error("Arti SOCKS proxy must listen on loopback, got {0}")]
    NonLoopbackProxy(SocketAddr),
    #[error("failed to start signed Arti proxy at {program}")]
    Spawn {
        program: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Arti proxy did not bootstrap within {0:?}")]
    BootstrapTimeout(Duration),
    #[error("Arti proxy exited before or during use: {0}")]
    Exited(std::process::ExitStatus),
    #[error("Arti proxy has been stopped")]
    Stopped,
    #[error("failed to inspect Arti proxy status")]
    ChildStatus(#[source] io::Error),
    #[error("Arti proxy process state was poisoned")]
    Poisoned,
    #[error("invalid SOCKS proxy configuration")]
    Proxy(#[source] reqwest::Error),
    #[error("failed to build Tor-only HTTP client")]
    Client(#[source] reqwest::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
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
