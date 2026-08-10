//! Upstream dialing and relaying. The production path hands targets to
//! an embedded Arti Tor client; the fixture path dials loopback targets
//! directly so tests can run against a local server without Tor.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use arti_client::config::pt::TransportConfigBuilder;
use arti_client::config::{BridgeConfigBuilder, CfgPath, TorClientConfigBuilder};
use arti_client::{BootstrapBehavior, DangerouslyIntoTorAddr, DataStream, TorClient};
use tokio::net::TcpStream;
use tokio::sync::OnceCell;
use tor_rtcompat::PreferredRuntime;

use crate::args::{Config, DialMode};
use crate::socks::{Target, TargetHost};
use crate::status::{StatusEvent, StatusSink};

/// Upstream dial deadline. Cold-cache Tor circuit builds are slow, so
/// this is generous; expiry surfaces as HOST_UNREACHABLE to the client.
const DIAL_TIMEOUT: Duration = Duration::from_secs(180);

/// An established upstream connection, ready to relay.
pub enum Upstream {
    Direct(TcpStream),
    Tor(Box<DataStream>),
}

/// Makes upstream connections according to the configured dial mode.
pub struct Dialer {
    mode: DialMode,
    state_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    bridge_config: Option<PathBuf>,
    transport_program: Option<PathBuf>,
    bridge_fixture: Option<SocketAddr>,
    /// Built lazily on the first tor-mode dial, so the listener is
    /// reported before bootstrap work begins; concurrent dials share it.
    tor: OnceCell<Arc<TorClient<PreferredRuntime>>>,
}

impl Dialer {
    pub fn new(config: &Config) -> Dialer {
        Dialer {
            mode: config.dial_mode,
            state_dir: config.state_dir.clone(),
            cache_dir: config.cache_dir.clone(),
            bridge_config: config.bridge_config.clone(),
            transport_program: config.transport_program.clone(),
            bridge_fixture: config.bridge_fixture,
            tor: OnceCell::new(),
        }
    }

    pub async fn dial(&self, sink: &StatusSink, target: &Target) -> Result<Upstream, String> {
        let attempt = async {
            match self.mode {
                DialMode::Direct => self.dial_direct(target).await,
                DialMode::Tor => self.dial_tor(sink, target).await,
                DialMode::BridgeFixture => self.dial_bridge_fixture(sink, target).await,
            }
        };
        match tokio::time::timeout(DIAL_TIMEOUT, attempt).await {
            Ok(outcome) => outcome,
            Err(_) => Err(format!(
                "dial of {} timed out after {}s",
                target.describe(),
                DIAL_TIMEOUT.as_secs()
            )),
        }
    }

    /// The acceptance bridge speaks one line containing the requested target,
    /// then relays bytes. The sidecar itself never opens the target socket.
    async fn dial_bridge_fixture(
        &self,
        sink: &StatusSink,
        target: &Target,
    ) -> Result<Upstream, String> {
        use tokio::io::AsyncWriteExt;
        let bridge = self
            .bridge_fixture
            .ok_or_else(|| "bridge fixture is not configured".to_string())?;
        sink.emit(&StatusEvent::Bootstrap {
            state: "bridge_connecting",
            percent: 50,
            bridge_in_use: true,
        });
        let mut stream = TcpStream::connect(bridge)
            .await
            .map_err(|error| format!("fixture bridge {bridge} unavailable: {error}"))?;
        stream
            .write_all(format!("{}\n", target.describe()).as_bytes())
            .await
            .map_err(|error| format!("fixture bridge handshake failed: {error}"))?;
        sink.emit(&StatusEvent::Ready {
            bridge_in_use: true,
        });
        Ok(Upstream::Direct(stream))
    }

    /// Fixture path: plain TCP, loopback only.
    async fn dial_direct(&self, target: &Target) -> Result<Upstream, String> {
        let ip = match &target.host {
            TargetHost::Ip(ip) => *ip,
            TargetHost::Name(name) => {
                return Err(format!(
                    "direct dial refuses hostname {name:?}: loopback IP targets only"
                ));
            }
        };
        // Never carry direct-mode cleartext to a routable address.
        if !ip.is_loopback() {
            return Err(format!("direct dial refuses non-loopback target {ip}"));
        }
        let addr = SocketAddr::new(ip, target.port);
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|error| format!("direct dial of {addr} failed: {error}"))?;
        Ok(Upstream::Direct(stream))
    }

    /// Production path: hand the target to the embedded Arti client.
    async fn dial_tor(&self, sink: &StatusSink, target: &Target) -> Result<Upstream, String> {
        let client = self.tor_client(sink).await?;
        let stream = match &target.host {
            // Raw-IP targets skip exit-side DNS; arti makes that opt-in.
            TargetHost::Ip(ip) => {
                let addr = SocketAddr::new(*ip, target.port)
                    .into_tor_addr_dangerously()
                    .map_err(|error| format!("ip target rejected: {error}"))?;
                client.connect(addr).await
            }
            // Hostnames go to Tor unresolved; exit relays do the lookup.
            TargetHost::Name(name) => client.connect((name.as_str(), target.port)).await,
        }
        .map_err(|error| format!("tor dial of {} failed: {error}", target.describe()))?;
        Ok(Upstream::Tor(Box::new(stream)))
    }

    async fn tor_client(
        &self,
        sink: &StatusSink,
    ) -> Result<Arc<TorClient<PreferredRuntime>>, String> {
        let client = self
            .tor
            .get_or_try_init(|| self.build_tor_client(sink))
            .await?;
        Ok(Arc::clone(client))
    }

    async fn build_tor_client(
        &self,
        sink: &StatusSink,
    ) -> Result<Arc<TorClient<PreferredRuntime>>, String> {
        // args::parse enforces these in tor mode; reaching here without
        // them means a Config was hand-built wrong.
        let (Some(state_dir), Some(cache_dir)) = (&self.state_dir, &self.cache_dir) else {
            return Err("tor dial mode has no state/cache directories".to_string());
        };
        let bridge_in_use = self.bridge_config.is_some();
        sink.emit(&StatusEvent::Bootstrap {
            state: "creating",
            percent: 0,
            bridge_in_use,
        });
        let mut builder = TorClientConfigBuilder::from_directories(state_dir, cache_dir);
        if let Some(bridge_path) = &self.bridge_config {
            let text = std::fs::read_to_string(bridge_path).map_err(|error| {
                format!(
                    "could not read bridge config {}: {error}",
                    bridge_path.display()
                )
            })?;
            let mut count = 0usize;
            for line in text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
            {
                let bridge: BridgeConfigBuilder = line
                    .parse()
                    .map_err(|error| format!("invalid bridge line: {error}"))?;
                builder.bridges().bridges().push(bridge);
                count += 1;
            }
            if count == 0 {
                return Err("bridge config contains no bridge lines".to_string());
            }
            let program = self
                .transport_program
                .as_ref()
                .ok_or_else(|| "bridge mode has no pluggable transport program".to_string())?;
            let mut transport = TransportConfigBuilder::default();
            transport
                .protocols(vec!["oslbridge"
                    .parse()
                    .map_err(|error| format!("transport name rejected: {error}"))?])
                .path(CfgPath::new(program.display().to_string().into()))
                .run_on_startup(true);
            builder.bridges().transports().push(transport);
        }
        let config = builder
            .build()
            .map_err(|error| format!("arti configuration rejected: {error}"))?;
        // OnDemand defers the network bootstrap until the first stream
        // request needs it; later dials await the same attempt.
        let client = TorClient::builder()
            .config(config)
            .bootstrap_behavior(BootstrapBehavior::OnDemand)
            .create_unbootstrapped()
            .map_err(|error| format!("arti client creation failed: {error}"))?;
        sink.emit(&StatusEvent::Bootstrap {
            state: "created",
            percent: 10,
            bridge_in_use,
        });
        client
            .bootstrap()
            .await
            .map_err(|error| format!("tor bootstrap failed: {error}"))?;
        sink.emit(&StatusEvent::Ready { bridge_in_use });
        Ok(client)
    }
}

/// Forward the pipelined readahead, then relay until both sides finish.
/// Returns (bytes client→target, bytes target→client); readahead counts
/// as client→target traffic, which is what it is.
pub async fn relay(
    client: &mut TcpStream,
    upstream: Upstream,
    readahead: &[u8],
) -> Result<(u64, u64), String> {
    match upstream {
        Upstream::Direct(mut stream) => run_relay(client, &mut stream, readahead).await,
        Upstream::Tor(mut stream) => run_relay(client, stream.as_mut(), readahead).await,
    }
}

async fn run_relay<S>(
    client: &mut TcpStream,
    upstream: &mut S,
    readahead: &[u8],
) -> Result<(u64, u64), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;

    if !readahead.is_empty() {
        upstream
            .write_all(readahead)
            .await
            .map_err(|error| format!("forwarding pipelined bytes failed: {error}"))?;
    }
    let (to_target, from_target) = tokio::io::copy_bidirectional(client, upstream)
        .await
        .map_err(|error| format!("relay ended with error: {error}"))?;
    Ok((to_target + readahead.len() as u64, from_target))
}
