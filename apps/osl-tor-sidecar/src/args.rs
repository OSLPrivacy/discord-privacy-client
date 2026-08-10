//! Command-line configuration. Parsing is hand-rolled to keep the supply
//! surface small; failures come back as messages for main() to emit as
//! JSON `error` events, and this module never prints anything itself.

use std::net::SocketAddr;
use std::path::PathBuf;

use arti_client::config::BridgeConfigBuilder;

/// How upstream connections are made for accepted SOCKS requests.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DialMode {
    /// Route every target through the embedded Arti Tor client (default).
    Tor,
    /// Dial targets directly over plain TCP, loopback destinations only.
    /// Exists so hermetic tests can prove the listener and SOCKS path
    /// against a local fixture; the loopback restriction means this mode
    /// can never carry cleartext to the wide network.
    Direct,
    /// Hermetic acceptance mode that may contact only one fixture bridge.
    BridgeFixture,
}

impl DialMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DialMode::Tor => "tor",
            DialMode::Direct => "direct",
            DialMode::BridgeFixture => "bridge_fixture",
        }
    }
}

pub struct Config {
    /// SOCKS listener address. Always loopback; the default port 0 asks
    /// the OS for an ephemeral port, which is reported on stdout. The
    /// sidecar never adopts another Tor implementation's well-known
    /// SOCKS port — it owns whatever the OS hands it.
    pub listen: SocketAddr,
    pub dial_mode: DialMode,
    /// Arti state directory (keys, guard state). Required in tor mode.
    pub state_dir: Option<PathBuf>,
    /// Arti cache directory (network documents). Required in tor mode.
    pub cache_dir: Option<PathBuf>,
    pub bridge_config: Option<PathBuf>,
    pub transport_program: Option<PathBuf>,
    pub bridge_fixture: Option<SocketAddr>,
}

const DEFAULT_LISTEN: &str = "127.0.0.1:0";

const USAGE: &str = "osl-tor-sidecar --dial-mode <tor|direct|bridge-fixture> \
[--listen 127.0.0.1:0] [--state-dir DIR] [--cache-dir DIR] \
[--bridge-config FILE --transport-program FILE] [--bridge-fixture ADDR]";

/// Parse argv (without the program name) into a Config.
pub fn parse<I: Iterator<Item = String>>(mut argv: I) -> Result<Config, String> {
    let mut listen: Option<String> = None;
    let mut dial_mode: Option<DialMode> = None;
    let mut state_dir: Option<PathBuf> = None;
    let mut cache_dir: Option<PathBuf> = None;
    let mut bridge_config = None;
    let mut transport_program = None;
    let mut bridge_fixture = None;

    while let Some(flag) = argv.next() {
        let mut value = |name: &str| -> Result<String, String> {
            argv.next()
                .ok_or_else(|| format!("missing value for {name}; usage: {USAGE}"))
        };
        match flag.as_str() {
            "--listen" => listen = Some(value("--listen")?),
            "--dial-mode" => {
                dial_mode = Some(match value("--dial-mode")?.as_str() {
                    "tor" => DialMode::Tor,
                    "direct" => DialMode::Direct,
                    "bridge-fixture" => DialMode::BridgeFixture,
                    other => {
                        return Err(format!(
                            "unknown dial mode {other:?}; expected tor or direct"
                        ))
                    }
                });
            }
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--cache-dir" => cache_dir = Some(PathBuf::from(value("--cache-dir")?)),
            "--bridge-config" => bridge_config = Some(PathBuf::from(value("--bridge-config")?)),
            "--transport-program" => {
                transport_program = Some(PathBuf::from(value("--transport-program")?))
            }
            "--bridge-fixture" => {
                let text = value("--bridge-fixture")?;
                bridge_fixture = Some(
                    text.parse()
                        .map_err(|_| format!("invalid bridge fixture address {text:?}"))?,
                );
            }
            other => return Err(format!("unknown argument {other:?}; usage: {USAGE}")),
        }
    }

    let listen_text = listen.unwrap_or_else(|| DEFAULT_LISTEN.to_string());
    let listen: SocketAddr = listen_text
        .parse()
        .map_err(|_| format!("could not parse listen address {listen_text:?}"))?;

    // A routable bind would offer this Tor client to the network.
    if !listen.ip().is_loopback() {
        return Err(format!(
            "listen address {listen} is not loopback; the sidecar only serves local clients"
        ));
    }

    let dial_mode = dial_mode.unwrap_or(DialMode::Tor);

    // Defaulting Arti's dirs to a home-relative path would hide where
    // OSL keeps Tor material, so the supervisor must be explicit.
    if dial_mode == DialMode::Tor && (state_dir.is_none() || cache_dir.is_none()) {
        return Err("tor dial mode requires --state-dir and --cache-dir".to_string());
    }
    if bridge_config.is_some() && dial_mode != DialMode::Tor {
        return Err("--bridge-config is only valid with tor dial mode".to_string());
    }
    if bridge_config.is_some() && transport_program.is_none() {
        return Err("--bridge-config requires --transport-program".to_string());
    }
    if transport_program.is_some() && bridge_config.is_none() {
        return Err("--transport-program requires --bridge-config".to_string());
    }
    if let Some(path) = &bridge_config {
        validate_bridge_config(path)?;
    }
    if let Some(path) = &transport_program {
        if !path.is_file() {
            return Err(format!("missing pluggable transport: {}", path.display()));
        }
    }
    if dial_mode == DialMode::BridgeFixture && bridge_fixture.is_none() {
        return Err("bridge-fixture dial mode requires --bridge-fixture".to_string());
    }
    if bridge_fixture.is_some() && dial_mode != DialMode::BridgeFixture {
        return Err("--bridge-fixture is only valid with bridge-fixture dial mode".to_string());
    }

    Ok(Config {
        listen,
        dial_mode,
        state_dir,
        cache_dir,
        bridge_config,
        transport_program,
        bridge_fixture,
    })
}

fn validate_bridge_config(path: &std::path::Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("could not read bridge config {}: {error}", path.display()))?;
    let mut count = 0usize;
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let _: BridgeConfigBuilder = line
            .parse()
            .map_err(|error| format!("invalid bridge line: {error}"))?;
        count += 1;
    }
    if count == 0 {
        return Err("bridge config contains no bridge lines".to_string());
    }
    Ok(())
}
