//! Server side of the SOCKS handshake, delegated to `tor-socksproto` (the
//! crate Arti's own proxy uses), covering the SOCKS4/4a/5 Tor dialects.

use std::net::IpAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tor_socksproto::{Handshake, NextStep, SocksProxyHandshake, SocksRequest};
use tor_socksproto::{SocksAddr, SocksCmd, SocksStatus};

/// The destination a completed handshake asked for.
pub enum TargetHost {
    Ip(IpAddr),
    /// A hostname; in tor mode it goes to the Tor network unresolved,
    /// which keeps DNS lookups off the local resolver.
    Name(String),
}

pub struct Target {
    pub host: TargetHost,
    pub port: u16,
}

impl Target {
    pub fn from_request(request: &SocksRequest) -> Target {
        let host = match request.addr() {
            SocksAddr::Ip(ip) => TargetHost::Ip(*ip),
            SocksAddr::Hostname(name) => TargetHost::Name(name.as_ref().to_string()),
        };
        Target {
            host,
            port: request.port(),
        }
    }

    /// Human-independent `host:port` form used in status events.
    pub fn describe(&self) -> String {
        match &self.host {
            TargetHost::Ip(ip) => format!("{ip}:{}", self.port),
            TargetHost::Name(name) => format!("{name}:{}", self.port),
        }
    }
}

/// The parsed request plus any bytes the client pipelined after it,
/// which must be forwarded upstream before relaying begins.
pub struct CompletedHandshake {
    pub request: SocksRequest,
    pub readahead: Vec<u8>,
}

/// Drive the SOCKS handshake to completion on `stream`, following the
/// send/recv/finished contract of `tor_socksproto::Handshake::step`.
pub async fn run_handshake(stream: &mut TcpStream) -> Result<CompletedHandshake, String> {
    let mut handshake = SocksProxyHandshake::new();
    let mut buffer = tor_socksproto::Buffer::new();
    loop {
        let step = handshake
            .step(&mut buffer)
            .map_err(|error| format!("socks handshake failed: {error}"))?;
        match step {
            NextStep::Send(data) => {
                stream
                    .write_all(&data)
                    .await
                    .map_err(|error| format!("write to socks client failed: {error}"))?;
            }
            NextStep::Recv(mut recv) => {
                let got = stream
                    .read(recv.buf())
                    .await
                    .map_err(|error| format!("read from socks client failed: {error}"))?;
                if got == 0 {
                    return Err("socks client closed mid-handshake".to_string());
                }
                recv.note_received(got)
                    .map_err(|error| format!("socks handshake rejected input: {error}"))?;
            }
            NextStep::Finished(finished) => {
                let (request, readahead) = finished.into_output_and_vec();
                return Ok(CompletedHandshake { request, readahead });
            }
        }
    }
}

/// Only CONNECT is served: BIND and UDP_ASSOCIATE have no meaning over
/// Tor streams, and RESOLVE is not part of the sidecar's contract.
pub fn is_supported(request: &SocksRequest) -> bool {
    request.command() == SocksCmd::CONNECT
}

/// Send the success or failure reply. Per `tor-socksproto`, an address
/// belongs in the reply only for RESOLVE, so CONNECT replies carry the
/// unspecified address in the client's address family.
pub async fn send_reply(
    stream: &mut TcpStream,
    request: &SocksRequest,
    status: SocksStatus,
) -> Result<(), String> {
    let reply = request
        .reply(status, None)
        .map_err(|error| format!("could not encode socks reply: {error}"))?;
    stream
        .write_all(&reply)
        .await
        .map_err(|error| format!("write of socks reply failed: {error}"))
}
