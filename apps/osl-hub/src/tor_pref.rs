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
        let base_url = ipc::cipher_store_client::resolve_cipher_store_base_url(config_dir)
            .map_err(|error| error.to_string())?;
        match self {
            Self::Direct => CipherStoreClient::new(base_url).map_err(|error| match &error {
                ipc::cipher_store_client::CipherStoreError::RouteUnavailable(message) => {
                    message.clone()
                }
                ipc::cipher_store_client::CipherStoreError::ConfigOverrideRefused { .. } => {
                    error.to_string()
                }
                _ => "OSL store transport is unavailable".to_owned(),
            }),
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
        let state = Self {
            path,
            preference: Mutex::new(preference),
            tor_config,
            tor_client: Mutex::new(tor_client),
        };
        state.publish_process_route();
        state
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
        {
            let mut current = self
                .preference
                .lock()
                .map_err(|_| "OSL network preference is unavailable".to_owned())?;
            *current = Some(preference);
        }
        self.publish_process_route();
        Ok(preference)
    }

    /// Push the current choice + tunnel health into the process-wide egress
    /// interlock.
    ///
    /// The per-command gate below only covers commands somebody remembered to
    /// gate. This covers the rest: while Tor is selected, every constructor in
    /// the workspace that would otherwise build its own direct client either
    /// adopts this tunnel or refuses. See `keystore::egress`.
    pub fn publish_process_route(&self) {
        match self.preference().ok().flatten() {
            Some(TorPreference::Tor) => match self.ready_tor_client() {
                Ok(Some(client)) => keystore::egress::route_through_tor(client),
                // Selected but unhealthy, or the transport state itself is
                // unreadable. Both are refusals, never a direct fallback.
                Ok(None) | Err(_) => keystore::egress::seal(),
            },
            Some(TorPreference::Direct) | None => keystore::egress::permit_clearnet(),
        }
    }

    /// Authorize before the caller can construct or send a store request.
    pub fn authorize_store(&self) -> Result<AuthorizedStoreRoute, String> {
        let preference = self.preference()?;
        let tor_http = if preference == Some(TorPreference::Tor) {
            self.ready_tor_client()?
        } else {
            None
        };
        // Re-publish on every authorization: a tunnel that died since the last
        // send must seal the rest of the process too, not only this command.
        match (preference, tor_http.clone()) {
            (Some(TorPreference::Tor), Some(client)) => keystore::egress::route_through_tor(client),
            (Some(TorPreference::Tor), None) => keystore::egress::seal(),
            _ => keystore::egress::permit_clearnet(),
        }
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
        let _route = keystore::egress::restore_clearnet_on_drop();
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
        let _route = keystore::egress::restore_clearnet_on_drop();
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
        let _route = keystore::egress::restore_clearnet_on_drop();
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
            (
                "async fn prepare_native_discord_overlay_text(",
                "broker::prepare_native_discord_overlay_text_with_route_clients(",
            ),
            (
                "fn prepare_whatsapp_qa_protected_text_blocking(",
                "broker::prepare_whatsapp_qa_peer_prose_text(",
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

    /// Every place in the workspace that builds its own HTTP client, counted.
    ///
    /// A gate that only checks the paths somebody remembered is how the
    /// ungated sends got shipped in the first place. This is the census that
    /// replaces remembering: five construction sites exist, four of them are
    /// behind [`keystore::egress`], and the fifth *is* the Tor transport. Add
    /// a sixth anywhere -- a new command, a new crate, a new helper -- and
    /// this fails until it is either routed or added here on purpose.
    const DIRECT_HTTP_CLIENT_CONSTRUCTION_SITES: &[(&str, usize)] = &[
        // Interlocked: adopts the authorized tunnel or refuses.
        ("apps/osl-hub/src/osl_mail.rs", 1),
        ("crates/ipc/src/cipher_store_client.rs", 1),
        ("crates/keystore/src/client.rs", 1),
        ("crates/keystore/src/username.rs", 1),
        // The Tor transport itself. This one builds the SOCKS client every
        // other site adopts, so it must not consult the interlock.
        ("crates/transport/src/tor.rs", 1),
    ];

    /// Files whose client construction must sit inside the interlock's
    /// `Build` arm rather than running unconditionally.
    const INTERLOCKED_CONSTRUCTION_SITES: &[&str] = &[
        "apps/osl-hub/src/osl_mail.rs",
        "crates/ipc/src/cipher_store_client.rs",
        "crates/keystore/src/client.rs",
        "crates/keystore/src/username.rs",
    ];

    fn workspace_root() -> PathBuf {
        // `apps/osl-hub` -> workspace root.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root is two levels above this crate")
            .to_path_buf()
    }

    fn shipping_source_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut pending = vec![root.join("crates"), root.join("apps")];
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Build output is not source, and `tests/` is not shipping
                    // code; both would only add noise to the census.
                    if !matches!(
                        path.file_name().and_then(|name| name.to_str()),
                        Some("target") | Some("node_modules") | Some("tests")
                    ) {
                        pending.push(path);
                    }
                } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                    files.push(path);
                }
            }
        }
        files.sort();
        files
    }

    /// The part of a file that ships, i.e. everything before its test module.
    fn shipping_region(source: &str) -> &str {
        match source.find("\n#[cfg(test)]\nmod ") {
            Some(index) => &source[..index],
            None => source,
        }
    }

    fn direct_client_constructions(source: &str) -> usize {
        shipping_region(source)
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .map(|line| line.matches("Client::builder()").count())
            .sum()
    }

    #[test]
    fn no_new_egress_path_can_build_its_own_http_client() {
        let root = workspace_root();
        let mut found = Vec::new();
        for path in shipping_source_files(&root) {
            let source = std::fs::read_to_string(&path).expect("read a workspace source file");
            let count = direct_client_constructions(&source);
            if count > 0 {
                let relative = path
                    .strip_prefix(&root)
                    .expect("every scanned file is under the workspace root")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push((relative, count));
            }
        }
        let expected: Vec<(String, usize)> = DIRECT_HTTP_CLIENT_CONSTRUCTION_SITES
            .iter()
            .map(|(path, count)| ((*path).to_owned(), *count))
            .collect();
        assert_eq!(
            found, expected,
            "a new HTTP client is built somewhere the Tor gate does not cover. Route it through \
             an authorized client, or send it through a constructor that consults \
             keystore::egress, and only then record it here."
        );

        for relative in INTERLOCKED_CONSTRUCTION_SITES {
            let source =
                std::fs::read_to_string(root.join(relative)).expect("read an interlocked source");
            assert!(
                shipping_region(&source).contains("DirectClientDecision::Build"),
                "{relative} builds an HTTP client without asking keystore::egress first"
            );
            assert!(
                shipping_region(&source).contains("DirectClientDecision::Refuse"),
                "{relative} has no refusal arm, so a selected Tor with no tunnel would fall \
                 back to clearnet"
            );
        }
    }

    /// The commands the renderer can call, frozen.
    ///
    /// This list is not documentation: it is the point at which somebody
    /// adding a command has to decide whether it can reach the network. A new
    /// command fails this test until it is added here, and adding it means
    /// having answered that question.
    const HUB_COMMANDS: &[&str] = &[
        "record_native_discord_qa_send_stage",
        "get_onboarding_preferences",
        "set_hub_screenshot_protection",
        "save_onboarding_preferences",
        "save_scrub_setup",
        "save_burn_review_state",
        "get_burn_review_state",
        "set_tor_preference",
        "get_follow_active_app_choice",
        "set_follow_active_app_choice",
        "scan_local_privacy",
        "open_hosted_session_scan",
        "request_hosted_session_scan",
        "request_hosted_session_scan_command",
        "scan_discord_own_messages_for_deletion",
        "preview_discord_guided_deletion",
        "execute_discord_guided_deletion",
        "save_osl_profile",
        "initialize_scrub_index",
        "set_scrub_index_manifest",
        "get_scrub_index_manifest",
        "get_scrub_index_scan",
        "append_scrub_index_chunk",
        "get_scrub_index_status",
        "cancel_scrub_index",
        "list_linked_services",
        "list_scrub_accounts",
        "create_hub_named_server",
        "list_hub_named_servers",
        "get_core_readiness",
        "list_core_features",
        "get_hub_license_state",
        "osl_mail_get_status",
        "osl_mail_provision",
        "osl_mail_send",
        "osl_mail_plan_protected_forward",
        "osl_mail_forward_protected",
        "osl_mail_burn",
        "get_mass_cleanup_capabilities",
        "discover_mass_cleanup_targets",
        "execute_mass_cleanup_batch",
        "get_autoscrub_run_fl",
        "start_autoscrub_reviewed_run",
        "request_autoscrub_global_stop",
        "keep_scanning_after_autoscrub_stop_request",
        "stop_autoscrub_now_after_stop_request",
        "compose_scrub_erasure_request",
        "validate_hub_activation_code",
        "clear_hub_activation_code",
        "unlock_hub_password_gate",
        "create_hub_osl_identity",
        "import_hub_osl_identity_phrase",
        "setup_hub_main_password",
        "view_hub_recovery_phrase",
        "reset_hub_main_password_after_recovery",
        "check_hub_password_reset_phrase",
        "get_hub_recovery_kit_unsaved",
        "set_hub_recovery_kit_unsaved",
        "lock_hub_session",
        "emit_active_session_reset",
        "get_hub_password_role_status",
        "set_hub_stealth_password",
        "remove_hub_stealth_password",
        "set_hub_burn_password",
        "remove_hub_burn_password",
        "set_hub_notifications_enabled",
        "list_hub_app_notifications",
        "get_hub_chat_approval_suggestion_choice",
        "set_hub_chat_approval_suggestion_choice",
        "answer_hub_chat_approval_suggestion",
        "set_hub_app_notification_choice",
        "list_hub_app_notification_choices",
        "set_hub_look_choice",
        "get_hub_look_choice",
        "list_hub_look_choices",
        "check_hub_for_updates",
        "install_hub_update",
        "open_hub_releases_page",
        "open_hub_source_repository",
        "list_native_apps",
        "install_native_app",
        "get_mullvad_status",
        "install_mullvad",
        "open_mullvad",
        "list_components",
        "install_component",
        "remove_component",
        "list_browser_imports",
        "open_browser_import",
        "list_browser_profiles_for_consent",
        "grant_browser_profile_consent",
        "scan_consented_browser_profile",
        "load_detected_browser_footprint",
        "revoke_detected_browser_footprint",
        "get_firefox_status",
        "install_firefox",
        "begin_browser_account_import",
        "begin_protected_browser_import",
        "finish_protected_browser_import",
        "launch_firefox_service",
        "get_default_browser_companion_status",
        "host_default_browser_companion",
        "resize_default_browser_companion",
        "focus_default_browser_companion",
        "detach_default_browser_companion",
        "host_native_app_window",
        "native_app_takeover_requires_consent",
        "discord_marker_available",
        "resize_native_app_window",
        "focus_native_app_window",
        "detach_native_app_window",
        "get_signal_protected_send_readiness",
        "claim_whatsapp_qa_window",
        "get_whatsapp_qa_protection_status",
        "begin_whatsapp_visual_binding",
        "confirm_whatsapp_visual_binding",
        "prepare_whatsapp_qa_protected_text",
        "open_whatsapp_qa_protected_text",
        "resize_whatsapp_qa_window",
        "set_native_discord_protected_overlay_open",
        "request_native_discord_visible_row_qa_receipt",
        "send_native_discord_overlay_carrier",
        "set_native_discord_covertext_enabled",
        "get_native_discord_overlay_state",
        "set_native_discord_overlay_security",
        "prepare_native_discord_overlay_text",
        "send_native_discord_qa_atomic_text",
        "send_native_discord_qa_probe",
        "run_native_discord_headless_qa",
        "poll_native_discord_headless_qa",
        "rehydrate_native_discord_overlay_history",
        "open_native_discord_overlay_text",
        "reveal_native_discord_overlay_view_once",
        "prepare_osl_chat_text",
        "open_osl_chat_text",
        "list_osl_chat_history",
        "burn_osl_chat_history",
        "select_osl_chat_attachment",
        "list_osl_chat_attachments",
        "open_osl_chat_attachment",
        "select_native_discord_overlay_attachment",
        "list_native_discord_overlay_attachments",
        "open_native_discord_overlay_attachment",
        "burn_native_discord_overlay_chat",
        "host_mullvad_window",
        "resize_mullvad_window",
        "focus_mullvad_window",
        "restore_mullvad_window",
        "create_service_account",
        "open_service_host",
        "close_service_host",
        "set_local_protected_sheet_open",
        "remove_service_account",
        "activate_local_loopback_context",
        "activate_manual_peer_context",
        "activate_native_manual_peer_context",
        "activate_osl_chat_context",
        "set_osl_chat_capture_preference",
        "close_osl_chat_context",
        "prepare_peer_prose_text",
        "open_peer_prose_text",
        "prepare_encrypted_text",
        "decrypt_hub_capsule",
        "export_hub_friend_code",
        "copy_hub_friend_invite",
        "create_hub_private_contact_link",
        "get_hub_private_contact_link_status",
        "add_hub_private_contact_link",
        "add_hub_friend",
        "create_one_use_invite_link",
        "claim_hub_username",
        "get_hub_username_status",
        "add_hub_friend_by_username",
        "get_osl_profile",
        "set_owner_profile_picture",
        "read_owner_profile_picture",
        "clear_owner_profile_picture",
        // Local-only: derives an HKDF subkey from the on-device storage-key
        // authority and returns it to the local webview. No socket, no
        // constructor that consults keystore::egress.
        "get_osl_chat_local_state_key",
        "verify_hub_friend_safety_number",
        "remove_hub_friend",
        "list_hub_people",
        "add_allowed_place_record",
        "remove_allowed_place_record",
        "list_allowed_place_records",
        "query_allowed_place_allowed",
        "get_hub_friend_wide_whitelist_action_help",
        "set_hub_friend_account_reach_everywhere",
        "compare_allowed_place_direction_state",
        "set_hub_friend_nickname",
        "add_group_member_permission",
        "remove_group_member_permission",
        "list_group_member_permissions",
        "add_allowed_place_record",
        "remove_allowed_place_record",
        "query_allowed_place_record",
        "list_allowed_place_records",
        "query_allowed_place_allowed",
        "compare_allowed_place_direction_state",
        "list_whatsapp_whitelist_kinds",
        "get_hub_friend_future_account_auto_whitelist",
        "set_hub_friend_future_account_auto_whitelist",
        "set_hub_friend_account_reach_choice",
        "list_hub_friend_account_reach_choices",
        "set_active_hub_friend_permission",
        "set_active_hub_friend_reach",
        "revoke_active_hub_friend_scope",
        "get_active_hub_context_security",
        "set_active_hub_context_security",
        "prepare_local_protected_text_with_policy",
        "prepare_hub_attachment",
        "open_hub_attachment",
        "decrypt_local_protected_capsule",
        "list_hub_identities",
        "create_hub_identity_slot",
        "recover_hub_identity_slot",
        "switch_hub_identity",
        "burn_active_hub_identity",
        "execute_hub_full_cleanup",
        "get_hub_service_burn_readiness",
        "burn_hub_service_account",
        "burn_active_hub_context",
        "list_active_hub_context_burn_choices",
        "get_hub_revocation_status",
        "ai_carrier_status",
        "set_ai_carrier_preview_enabled",
        "build_integrity_status",
        "verify_peer_build_integrity",
        "list_bad_message_rules",
    ];

    /// The commands that hold an authorized route open across their own send.
    ///
    /// Everything else reaches the network only through constructors that
    /// consult [`keystore::egress`], which is what makes them safe without a
    /// per-command gate. Losing a gate from this list is a regression, so the
    /// set is compared exactly rather than by containment.
    /// Declaration order, so a moved command is as visible as a removed gate.
    const COMMANDS_HOLDING_AN_AUTHORIZED_ROUTE: &[&str] = &[
        "prepare_whatsapp_qa_protected_text",
        "prepare_native_discord_overlay_text",
        "prepare_osl_chat_text",
        "prepare_peer_prose_text",
    ];

    fn command_names(source: &str) -> Vec<String> {
        let mut names = Vec::new();
        for (index, line) in source.lines().enumerate() {
            if line.trim() != "#[tauri::command]" {
                continue;
            }
            let name = source
                .lines()
                .skip(index + 1)
                .find_map(|following| {
                    let mut signature = following.trim_start();
                    signature = signature.strip_prefix("pub ").unwrap_or(signature);
                    signature = signature.strip_prefix("async ").unwrap_or(signature);
                    signature.strip_prefix("fn ").map(|rest| {
                        rest.split(['(', '<'])
                            .next()
                            .unwrap_or_default()
                            .trim()
                            .to_owned()
                    })
                })
                .expect("every #[tauri::command] is followed by its function");
            names.push(name);
        }
        names
    }

    /// The body of `name`, from its signature to the next command or the end.
    fn command_body<'a>(source: &'a str, name: &str) -> &'a str {
        let signature = format!("fn {name}(");
        let start = source
            .find(&signature)
            .expect("a listed command exists in main.rs");
        let rest = &source[start..];
        match rest[1..].find("\n#[tauri::command]") {
            Some(end) => &rest[..end + 1],
            None => rest,
        }
    }

    #[test]
    fn every_hub_command_is_accounted_for_by_the_tor_gate() {
        let source = include_str!("main.rs");
        let names = command_names(source);
        assert_eq!(
            names,
            HUB_COMMANDS
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>(),
            "the hub's command surface changed. Decide, for each added command, whether it can \
             reach the network: hold an authorized route across the send like \
             prepare_osl_chat_text does, or reach the network only through a constructor that \
             consults keystore::egress. Then record it here."
        );

        let gated: Vec<&str> = HUB_COMMANDS
            .iter()
            .copied()
            .filter(|name| {
                command_body(source, name)
                    .contains("app.state::<TorPreferenceState>().authorize_store()?")
            })
            .collect();
        assert_eq!(
            gated, COMMANDS_HOLDING_AN_AUTHORIZED_ROUTE,
            "the set of commands that authorize a route before sending changed"
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
}
