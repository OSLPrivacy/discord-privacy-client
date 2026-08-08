//! Command-line configuration. Parsing is hand-rolled to keep the supply
//! surface small; failures come back as messages for main() to emit as
//! JSON `error` events, and this module never prints anything itself.

use std::net::SocketAddr;
use std::path::PathBuf;

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
}

impl DialMode {
    pub fn as_str(self) -> &'static str {
        match self {
            DialMode::Tor => "tor",
            DialMode::Direct => "direct",
        }
    }
}

/// Fully-validated sidecar configuration.
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
}

const DEFAULT_LISTEN: &str = "127.0.0.1:0";

const USAGE: &str = "osl-tor-sidecar --dial-mode <tor|direct> \
[--listen 127.0.0.1:0] [--state-dir DIR] [--cache-dir DIR]";

/// Parse argv (without the program name) into a Config.
pub fn parse<I: Iterator<Item = String>>(mut argv: I) -> Result<Config, String> {
    let mut listen: Option<String> = None;
    let mut dial_mode: Option<DialMode> = None;
    let mut state_dir: Option<PathBuf> = None;
    let mut cache_dir: Option<PathBuf> = None;

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
                    other => {
                        return Err(format!(
                            "unknown dial mode {other:?}; expected tor or direct"
                        ))
                    }
                });
            }
            "--state-dir" => state_dir = Some(PathBuf::from(value("--state-dir")?)),
            "--cache-dir" => cache_dir = Some(PathBuf::from(value("--cache-dir")?)),
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

    Ok(Config {
        listen,
        dial_mode,
        state_dir,
        cache_dir,
    })
}
