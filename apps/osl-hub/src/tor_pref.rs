//! Persisted network-route preference and the fail-closed authorization gate.
//!
//! This deliberately decides *before* a send or store request is constructed:
//! a Tor choice with no ready tunnel is a refusal, never permission to use a
//! direct client.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(feature = "core")]
use ipc::cipher_store_client::CipherStoreClient;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use transport::tor::{ArtiProxyConfig, TorTransport};

const PREFERENCE_FILE_VERSION: u8 = 1;
const MAX_PREFERENCE_BYTES: u64 = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TorPreference {
    Direct,
    Tor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelState {
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkOperation {
    Send,
    Store,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkAuthorization {
    Direct,
    Tor,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    ChoiceRequired,
    TorUnavailable(NetworkOperation),
}

pub enum AuthorizedStoreRoute {
    Direct,
    Tor { http: Client },
}

impl AuthorizedStoreRoute {
    pub fn authorization(&self) -> NetworkAuthorization {
        match self {
            Self::Direct => NetworkAuthorization::Direct,
            Self::Tor { .. } => NetworkAuthorization::Tor,
        }
    }

    #[cfg(feature = "core")]
    pub fn cipher_store_client(&self, config_dir: &Path) -> Result<CipherStoreClient, String> {
        let base_url = ipc::cipher_store_client::resolve_cipher_store_base_url(config_dir);
        match self {
            Self::Direct => CipherStoreClient::new(base_url)
                .map_err(|_| "OSL store transport is unavailable".to_owned()),
            Self::Tor { http } => Ok(CipherStoreClient::with_http_client(base_url, http.clone())),
        }
    }

    #[cfg(feature = "core")]
    pub fn tor_keyserver_client(
        &self,
        config_dir: &Path,
    ) -> Result<Option<keystore::KeyServerClient>, String> {
        match self {
            Self::Direct => Ok(None),
            Self::Tor { http } => {
                let base_url = ipc::commands::resolve_keyserver_base_url(config_dir);
                keystore::KeyServerClient::with_http_client(base_url, http.clone())
                    .map(Some)
                    .map_err(|_| "OSL key server transport is unavailable".to_owned())
            }
        }
    }
}

#[derive(Clone)]
struct TorClientFactory {
    store_client: Arc<dyn Fn() -> Result<Client, String> + Send + Sync>,
}

impl TorClientFactory {
    fn new<F>(store_client: F) -> Self
    where
        F: Fn() -> Result<Client, String> + Send + Sync + 'static,
    {
        Self {
            store_client: Arc::new(store_client),
        }
    }

    fn store_client(&self) -> Result<Client, String> {
        (self.store_client)()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedPreference {
    version: u8,
    preference: Option<TorPreference>,
}

/// The native authority for the route selected during onboarding.
///
/// A missing or malformed preference is deliberately kept as `None`: network
/// operations then refuse before a store request is constructed. T1-71 owns
/// tunnel lifecycle; until it reports a ready tunnel, a Tor choice also
/// refuses rather than falling back to direct traffic.
pub struct TorPreferenceState {
    path: PathBuf,
    preference: Mutex<Option<TorPreference>>,
    tor_config: Option<ArtiProxyConfig>,
    tor_client: Mutex<Option<TorClientFactory>>,
}

impl TorPreferenceState {
    pub fn load(path: PathBuf) -> Self {
        Self::load_with_arti_proxy_config(path, None)
    }

    pub fn load_with_arti_proxy_config(path: PathBuf, tor_config: Option<ArtiProxyConfig>) -> Self {
        let preference = read_preference(&path).unwrap_or(None);
        let tor_client = if preference == Some(TorPreference::Tor) {
            tor_config
                .clone()
                .and_then(|config| start_tor_client(config).ok())
        } else {
            None
        };
        Self {
            path,
            preference: Mutex::new(preference),
            tor_config,
            tor_client: Mutex::new(tor_client),
        }
    }

    #[cfg(test)]
    fn load_with_tor_client_factory<F>(path: PathBuf, factory: F) -> Self
    where
        F: Fn() -> Result<Client, String> + Send + Sync + 'static,
    {
        let preference = read_preference(&path).unwrap_or(None);
        Self {
            path,
            preference: Mutex::new(preference),
            tor_config: None,
            tor_client: Mutex::new(Some(TorClientFactory::new(factory))),
        }
    }

    pub fn preference(&self) -> Result<Option<TorPreference>, String> {
        self.preference
            .lock()
            .map(|preference| *preference)
            .map_err(|_| "OSL network preference is unavailable".to_owned())
    }

    pub fn set_preference(&self, preference: TorPreference) -> Result<TorPreference, String> {
        write_preference(&self.path, preference)?;
        self.update_tor_client_for_preference(preference)?;
        let mut current = self
            .preference
            .lock()
            .map_err(|_| "OSL network preference is unavailable".to_owned())?;
        *current = Some(preference);
        Ok(preference)
    }

    /// Authorize before the caller can construct or send a store request.
    pub fn authorize_store(&self) -> Result<AuthorizedStoreRoute, String> {
        let preference = self.preference()?;
        let tor_http = if preference == Some(TorPreference::Tor) {
            self.ready_tor_client()?
        } else {
            None
        };
        let authorization = authorize_network(
            preference,
            if tor_http.is_some() {
                TunnelState::Ready
            } else {
                TunnelState::Unavailable
            },
            NetworkOperation::Store,
        );
        match authorization {
            NetworkAuthorization::Refused(Refusal::ChoiceRequired) => {
                Err("Choose a connection route before sending encrypted text".to_owned())
            }
            NetworkAuthorization::Refused(Refusal::TorUnavailable(_)) => Err(
                "Tor is selected, but its tunnel is unavailable; no message was sent".to_owned(),
            ),
            NetworkAuthorization::Direct => Ok(AuthorizedStoreRoute::Direct),
            NetworkAuthorization::Tor => Ok(AuthorizedStoreRoute::Tor {
                http: tor_http.ok_or_else(|| {
                    "Tor is selected, but its tunnel is unavailable; no message was sent".to_owned()
                })?,
            }),
        }
    }

    fn update_tor_client_for_preference(&self, preference: TorPreference) -> Result<(), String> {
        let mut tor_client = self
            .tor_client
            .lock()
            .map_err(|_| "OSL Tor transport state is unavailable".to_owned())?;
        match preference {
            TorPreference::Direct => {
                *tor_client = None;
            }
            TorPreference::Tor if tor_client.is_none() => {
                *tor_client = self
                    .tor_config
                    .clone()
                    .and_then(|config| start_tor_client(config).ok());
            }
            TorPreference::Tor => {}
        }
        Ok(())
    }

    fn ready_tor_client(&self) -> Result<Option<Client>, String> {
        let mut tor_client = self
            .tor_client
            .lock()
            .map_err(|_| "OSL Tor transport state is unavailable".to_owned())?;
        let Some(factory) = tor_client.as_ref() else {
            return Ok(None);
        };
        match factory.store_client() {
            Ok(client) => Ok(Some(client)),
            Err(_) => {
                *tor_client = None;
                Ok(None)
            }
        }
    }
}

pub fn arti_proxy_config_from_env() -> Option<ArtiProxyConfig> {
    let program =
        std::env::var_os("OSL_ARTI_PROXY_PATH").or_else(|| std::env::var_os("OSL_ARTI_PROXY"))?;
    let mut config = ArtiProxyConfig::new(program);
    if let Ok(args) = std::env::var("OSL_ARTI_PROXY_ARGS") {
        config.args = args
            .split_whitespace()
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
    }
    Some(config)
}

fn start_tor_client(config: ArtiProxyConfig) -> Result<TorClientFactory, String> {
    let transport =
        Arc::new(TorTransport::start(config).map_err(|_| "OSL Tor transport is unavailable")?);
    Ok(TorClientFactory::new(move || {
        transport
            .store_client()
            .map_err(|_| "OSL Tor transport is unavailable".to_owned())
    }))
}

fn read_preference(path: &Path) -> Option<Option<TorPreference>> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PREFERENCE_BYTES,
        "network route preference",
    )
    .ok()
    .flatten()?;
    let persisted = serde_json::from_slice::<PersistedPreference>(&bytes).ok()?;
    (persisted.version == PREFERENCE_FILE_VERSION).then_some(persisted.preference)
}

fn write_preference(path: &Path, preference: TorPreference) -> Result<(), String> {
    let bytes = serde_json::to_vec(&PersistedPreference {
        version: PREFERENCE_FILE_VERSION,
        preference: Some(preference),
    })
    .map_err(|_| "OSL network preference could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(path, &bytes, "network route preference")
}

/// Authorize one network operation without providing any fallback path.
pub fn authorize_network(
    preference: Option<TorPreference>,
    tunnel: TunnelState,
    operation: NetworkOperation,
) -> NetworkAuthorization {
    match preference {
        None => NetworkAuthorization::Refused(Refusal::ChoiceRequired),
        Some(TorPreference::Direct) => NetworkAuthorization::Direct,
        Some(TorPreference::Tor) if tunnel == TunnelState::Ready => NetworkAuthorization::Tor,
        Some(TorPreference::Tor) => {
            NetworkAuthorization::Refused(Refusal::TorUnavailable(operation))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn no_choice_refuses_every_network_operation() {
        for operation in [NetworkOperation::Send, NetworkOperation::Store] {
            assert_eq!(
                authorize_network(None, TunnelState::Ready, operation),
                NetworkAuthorization::Refused(Refusal::ChoiceRequired),
            );
        }
    }

    #[test]
    fn tor_with_an_unavailable_tunnel_fails_closed_for_send_and_store() {
        for operation in [NetworkOperation::Send, NetworkOperation::Store] {
            assert_eq!(
                authorize_network(
                    Some(TorPreference::Tor),
                    TunnelState::Unavailable,
                    operation
                ),
                NetworkAuthorization::Refused(Refusal::TorUnavailable(operation)),
            );
        }
    }

    #[test]
    fn ready_tor_is_authorized_only_as_tor_and_direct_stays_explicit() {
        assert_eq!(
            authorize_network(
                Some(TorPreference::Tor),
                TunnelState::Ready,
                NetworkOperation::Send
            ),
            NetworkAuthorization::Tor,
        );
        assert_eq!(
            authorize_network(
                Some(TorPreference::Direct),
                TunnelState::Unavailable,
                NetworkOperation::Store
            ),
            NetworkAuthorization::Direct,
        );
    }

    #[test]
    fn persisted_tor_with_no_tunnel_cannot_reach_the_store() {
        let directory = tempfile::tempdir().expect("temporary preference directory");
        let state = TorPreferenceState::load(directory.path().join("tor-preference.json"));
        state
            .set_preference(TorPreference::Tor)
            .expect("persist Tor choice");

        assert_eq!(
            state.authorize_store().map(|route| route.authorization()),
            Err("Tor is selected, but its tunnel is unavailable; no message was sent".to_owned())
        );
    }

    #[test]
    fn tor_healthy_store_route_uses_the_socks_tunnel() {
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

        let proxy = TcpListener::bind("127.0.0.1:0").expect("bind SOCKS proxy");
        let proxy_addr = proxy.local_addr().expect("read SOCKS proxy address");
        let (proxy_seen_tx, proxy_seen_rx) = mpsc::channel();
        thread::spawn(move || run_test_socks_proxy(proxy, backend_addr, proxy_seen_tx));

        let directory = tempfile::tempdir().expect("temporary preference directory");
        std::fs::write(
            directory.path().join("keyserver.json"),
            format!(
                r#"{{"cipher_store_url":"http://store.invalid:{}"}}"#,
                backend_addr.port()
            ),
        )
        .expect("write local store route");
        let state = TorPreferenceState::load_with_tor_client_factory(
            directory.path().join("tor-preference.json"),
            move || {
                transport::tor::client_for_ready_socks_proxy(proxy_addr)
                    .map_err(|error| error.to_string())
            },
        );
        state
            .set_preference(TorPreference::Tor)
            .expect("persist Tor choice");

        let route = state.authorize_store().expect("healthy Tor is authorized");
        assert_eq!(route.authorization(), NetworkAuthorization::Tor);
        let client = route
            .cipher_store_client(directory.path())
            .expect("build routed store client");
        let token = [7_u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
        let uploaded = client
            .upload(b"ciphertext", ipc::cipher_store_client::TTL_1H, &token)
            .expect("store upload routes through Tor");

        assert_eq!(uploaded.id_hex, "0011223344556677");
        assert_eq!(
            proxy_seen_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "store.invalid"
        );
        request_seen_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("backend only receives the request after SOCKS forwarding");
    }

    #[test]
    fn tor_selected_but_unhealthy_refuses_before_any_store_send() {
        let backend = TcpListener::bind("127.0.0.1:0").expect("bind store backend");
        backend
            .set_nonblocking(true)
            .expect("make backend probe nonblocking");
        let directory = tempfile::tempdir().expect("temporary preference directory");
        std::fs::write(
            directory.path().join("keyserver.json"),
            format!(
                r#"{{"cipher_store_url":"http://127.0.0.1:{}"}}"#,
                backend.local_addr().expect("read backend address").port()
            ),
        )
        .expect("write local store route");
        let state = TorPreferenceState::load(directory.path().join("tor-preference.json"));
        state
            .set_preference(TorPreference::Tor)
            .expect("persist Tor choice");

        assert_eq!(
            state.authorize_store().map(|route| route.authorization()),
            Err("Tor is selected, but its tunnel is unavailable; no message was sent".to_owned())
        );
        assert!(
            backend.accept().is_err(),
            "unhealthy Tor must refuse before constructing a store request"
        );
    }

    #[test]
    fn shipping_store_sends_are_gated_before_the_broker_is_called() {
        let source = include_str!("main.rs");
        for (command, send) in [
            (
                "async fn prepare_osl_chat_text(",
                "broker::prepare_osl_chat_text_with_route_clients(",
            ),
            (
                "async fn prepare_peer_prose_text(",
                "broker::prepare_peer_prose_text_with_capture_and_store_client(",
            ),
        ] {
            let body = &source[source
                .find(command)
                .expect("shipping send command must exist")..];
            let gate = body
                .find("app.state::<TorPreferenceState>().authorize_store()?;")
                .expect("Tor choice must gate the store send");
            let route_client = body
                .find("route.cipher_store_client(&config_dir)?;")
                .expect("authorized route must build the store client");
            let broker = body.find(send).expect("shipping store send must exist");
            assert!(
                gate < route_client && route_client < broker,
                "the Tor gate must run before encrypted store traffic is constructed"
            );
        }
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
}
