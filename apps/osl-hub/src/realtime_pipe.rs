//! Blocking WebSocket pipe for the realtime wakeup state machine.
//!
//! The pure wakeup client owns cadence and frame contents.  This module owns
//! only the network pipe: open the service connection, write the fixed text
//! frames, read replies, and reconnect with the existing jitter policy after a
//! socket failure.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::{rngs::OsRng, RngCore};

use crate::realtime_client::{
    AuthorizedFetch, FrameError, RealtimeClient, ScheduledFetch, FRAME_BYTES,
};
use crate::realtime_decoy::DecoyFetch;
use crate::realtime_resume::ReconnectSchedule;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IO_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CONTROL_PAYLOAD_BYTES: usize = 125;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeEndpoint {
    host: String,
    port: u16,
    path_and_query: String,
}

impl RealtimeEndpoint {
    pub fn parse(url: &str) -> Result<Self, RealtimePipeError> {
        let parsed = url::Url::parse(url).map_err(|_| RealtimePipeError::InvalidEndpoint)?;
        if parsed.scheme() != "ws" {
            return Err(RealtimePipeError::UnsupportedScheme);
        }
        let host = parsed
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or(RealtimePipeError::InvalidEndpoint)?
            .to_owned();
        let port = parsed
            .port_or_known_default()
            .ok_or(RealtimePipeError::InvalidEndpoint)?;
        let path = if parsed.path().is_empty() {
            "/"
        } else {
            parsed.path()
        };
        let path_and_query = match parsed.query() {
            Some(query) => format!("{path}?{query}"),
            None => path.to_owned(),
        };
        Ok(Self {
            host,
            port,
            path_and_query,
        })
    }

    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn as_url(&self) -> String {
        format!("ws://{}{}", self.authority(), self.path_and_query)
    }

    fn socket_addrs(&self) -> Result<Vec<SocketAddr>, RealtimePipeError> {
        self.authority()
            .to_socket_addrs()
            .map(|addrs| addrs.collect())
            .map_err(RealtimePipeError::Io)
    }
}

#[derive(Debug)]
pub enum RealtimePipeError {
    InvalidEndpoint,
    UnsupportedScheme,
    NoSocketAddress,
    HandshakeRefused,
    UnexpectedClose,
    FrameTooLarge,
    Protocol(String),
    Wakeup(FrameError),
    RouteUnavailable(&'static str),
    Io(std::io::Error),
}

impl From<std::io::Error> for RealtimePipeError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct RealtimeSocket {
    stream: TcpStream,
}

/// Open the realtime WebSocket connection to the running service.
pub fn open_realtime_connection(
    endpoint: &RealtimeEndpoint,
) -> Result<RealtimeSocket, RealtimePipeError> {
    match keystore::egress::socket_route_decision() {
        keystore::egress::SocketRouteDecision::Tor(socks_addr) => {
            let stream = transport::tor::connect_tcp_via_socks(
                socks_addr,
                &endpoint.host,
                endpoint.port,
                CONNECT_TIMEOUT,
            )
            .map_err(|error| RealtimePipeError::Io(std::io::Error::other(error)))?;
            stream.set_read_timeout(Some(IO_TIMEOUT))?;
            stream.set_write_timeout(Some(IO_TIMEOUT))?;
            return finish_websocket_handshake(stream, endpoint);
        }
        keystore::egress::SocketRouteDecision::Refuse => {
            return Err(RealtimePipeError::RouteUnavailable(
                keystore::egress::TOR_UNAVAILABLE,
            ));
        }
        keystore::egress::SocketRouteDecision::Direct => {}
    }

    let addrs = endpoint.socket_addrs()?;
    let mut last_error = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => {
                stream.set_read_timeout(Some(IO_TIMEOUT))?;
                stream.set_write_timeout(Some(IO_TIMEOUT))?;
                return finish_websocket_handshake(stream, endpoint);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error
        .map(RealtimePipeError::Io)
        .unwrap_or(RealtimePipeError::NoSocketAddress))
}

fn finish_websocket_handshake(
    mut stream: TcpStream,
    endpoint: &RealtimeEndpoint,
) -> Result<RealtimeSocket, RealtimePipeError> {
    let key = websocket_key();
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n\
         \r\n",
        endpoint.path_and_query,
        endpoint.authority(),
        key
    );
    stream.write_all(request.as_bytes())?;

    let mut response = Vec::with_capacity(512);
    let mut byte = [0_u8; 1];
    while !response.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(RealtimePipeError::UnexpectedClose);
        }
        response.push(byte[0]);
        if response.len() > 8192 {
            return Err(RealtimePipeError::HandshakeRefused);
        }
    }
    let response = String::from_utf8_lossy(&response);
    let mut lines = response.lines();
    let status = lines.next().unwrap_or_default();
    if !status.contains(" 101 ") {
        return Err(RealtimePipeError::HandshakeRefused);
    }
    let mut upgraded = false;
    let mut connection_upgrade = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("upgrade") && value.trim().eq_ignore_ascii_case("websocket") {
            upgraded = true;
        }
        if name.eq_ignore_ascii_case("connection")
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("upgrade"))
        {
            connection_upgrade = true;
        }
    }
    if !upgraded || !connection_upgrade {
        return Err(RealtimePipeError::HandshakeRefused);
    }
    Ok(RealtimeSocket { stream })
}

fn websocket_key() -> String {
    let mut nonce = [0_u8; 16];
    OsRng.fill_bytes(&mut nonce);
    BASE64.encode(nonce)
}

impl RealtimeSocket {
    pub fn send_text(&mut self, text: &str) -> Result<(), RealtimePipeError> {
        write_text_frame(&mut self.stream, text.as_bytes())
    }

    pub fn read_text(&mut self) -> Result<String, RealtimePipeError> {
        read_text_frame(&mut self.stream)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RealtimePipeReport {
    pub opened_connections: usize,
    pub frame_offsets: Vec<Duration>,
    pub frame_sizes: Vec<usize>,
    pub reconnect_waits: Vec<Duration>,
    pub pretend_fetches: usize,
    pub authorized_fetches: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RealtimeFetchDrainReport {
    pub pretend_fetches: usize,
    pub authorized_fetches: usize,
}

pub trait RealtimePipeClock {
    fn wait_until(&mut self, scheduled_at: Duration);
    fn wait_for_reconnect(&mut self, delay: Duration);
}

#[derive(Default)]
pub struct SleepingRealtimePipeClock {
    elapsed: Duration,
}

impl RealtimePipeClock for SleepingRealtimePipeClock {
    fn wait_until(&mut self, scheduled_at: Duration) {
        if scheduled_at > self.elapsed {
            std::thread::sleep(scheduled_at - self.elapsed);
        }
        self.elapsed = scheduled_at;
    }

    fn wait_for_reconnect(&mut self, delay: Duration) {
        std::thread::sleep(delay);
        self.elapsed = self
            .elapsed
            .checked_add(delay)
            .expect("realtime pipe clock overflow");
    }
}

pub trait RealtimePipeEntropy {
    fn next_u64(&mut self) -> u64;
}

pub struct OsRealtimePipeEntropy;

impl RealtimePipeEntropy for OsRealtimePipeEntropy {
    fn next_u64(&mut self) -> u64 {
        OsRng.next_u64()
    }
}

/// Drive a finite number of wakeup ticks through the service connection.
///
/// Production callers normally pass [`SleepingRealtimePipeClock`] and
/// [`OsRealtimePipeEntropy`]. Tests can pass deterministic implementations to
/// prove cadence and reconnect delays without sleeping for wall-clock hours.
pub fn run_realtime_pipe_ticks<C, R>(
    endpoint: &RealtimeEndpoint,
    client: &mut RealtimeClient,
    tick_count: usize,
    clock: &mut C,
    entropy: &mut R,
) -> Result<RealtimePipeReport, RealtimePipeError>
where
    C: RealtimePipeClock,
    R: RealtimePipeEntropy,
{
    let mut report = RealtimePipeReport::default();
    let mut reconnect = ReconnectSchedule::new();
    let mut socket = open_socket(endpoint, &mut report)?;

    let mut sent = 0;
    while sent < tick_count {
        let tick = client.next_outbound_tick();
        clock.wait_until(tick.scheduled_at);
        match socket.send_text(&tick.frame) {
            Ok(()) => {
                report.frame_offsets.push(tick.scheduled_at);
                report.frame_sizes.push(tick.frame.len());
                sent += 1;
            }
            Err(error) if is_reconnectable(&error) => {
                socket =
                    reconnect_until_open(endpoint, &mut reconnect, clock, entropy, &mut report)?;
                continue;
            }
            Err(error) => return Err(error),
        }

        match receive_tick(&mut socket, client) {
            Ok(()) => {
                reconnect.connected();
                drain_fetch_work(client, &mut report);
            }
            Err(error) if is_reconnectable(&error) => {
                socket =
                    reconnect_until_open(endpoint, &mut reconnect, clock, entropy, &mut report)?;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(report)
}

fn reconnect_until_open<C, R>(
    endpoint: &RealtimeEndpoint,
    reconnect: &mut ReconnectSchedule,
    clock: &mut C,
    entropy: &mut R,
    report: &mut RealtimePipeReport,
) -> Result<RealtimeSocket, RealtimePipeError>
where
    C: RealtimePipeClock,
    R: RealtimePipeEntropy,
{
    loop {
        let delay = reconnect.next_delay(entropy.next_u64());
        report.reconnect_waits.push(delay);
        clock.wait_for_reconnect(delay);
        match open_socket(endpoint, report) {
            Ok(socket) => return Ok(socket),
            Err(error) if is_reconnectable(&error) => continue,
            Err(error) => return Err(error),
        }
    }
}

fn open_socket(
    endpoint: &RealtimeEndpoint,
    report: &mut RealtimePipeReport,
) -> Result<RealtimeSocket, RealtimePipeError> {
    let socket = open_realtime_connection(endpoint)?;
    report.opened_connections += 1;
    Ok(socket)
}

fn receive_tick(
    socket: &mut RealtimeSocket,
    client: &mut RealtimeClient,
) -> Result<(), RealtimePipeError> {
    let reply = socket.read_text()?;
    if reply.len() != FRAME_BYTES {
        return Err(RealtimePipeError::Protocol(format!(
            "expected {FRAME_BYTES}-byte wakeup frame, got {}",
            reply.len()
        )));
    }
    client
        .receive_frame(&reply)
        .map_err(RealtimePipeError::Wakeup)
}

fn drain_fetch_work(client: &mut RealtimeClient, report: &mut RealtimePipeReport) {
    let drained = drain_scheduled_fetches(client, |_| Ok::<_, ()>(()), |_| Ok::<_, ()>(()))
        .expect("count-only fetch drain cannot fail");
    report.authorized_fetches += drained.authorized_fetches;
    report.pretend_fetches += drained.pretend_fetches;
}

pub fn drain_scheduled_fetches<A, D, E>(
    client: &mut RealtimeClient,
    mut authorized: A,
    mut decoy: D,
) -> Result<RealtimeFetchDrainReport, E>
where
    A: FnMut(AuthorizedFetch) -> Result<(), E>,
    D: FnMut(DecoyFetch) -> Result<(), E>,
{
    let mut report = RealtimeFetchDrainReport::default();
    while let Some(fetch) = client.take_fetch_work() {
        match fetch {
            ScheduledFetch::Authorized(fetch) => {
                authorized(fetch)?;
                report.authorized_fetches += 1;
            }
            ScheduledFetch::Decoy(fetch) => {
                decoy(fetch)?;
                report.pretend_fetches += 1;
            }
        }
    }
    Ok(report)
}

fn is_reconnectable(error: &RealtimePipeError) -> bool {
    matches!(
        error,
        RealtimePipeError::Io(_) | RealtimePipeError::UnexpectedClose
    )
}

fn write_text_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), RealtimePipeError> {
    let mut frame = Vec::with_capacity(payload.len() + 8);
    frame.push(0x81);
    match payload.len() {
        0..=MAX_CONTROL_PAYLOAD_BYTES => frame.push(0x80 | payload.len() as u8),
        126..=65_535 => {
            frame.push(0x80 | 126);
            frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        }
        _ => return Err(RealtimePipeError::FrameTooLarge),
    }
    let mut mask = [0_u8; 4];
    OsRng.fill_bytes(&mut mask);
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % mask.len()]),
    );
    stream.write_all(&frame)?;
    Ok(())
}

fn read_text_frame(stream: &mut TcpStream) -> Result<String, RealtimePipeError> {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header)?;
    let opcode = header[0] & 0x0f;
    if opcode == 0x8 {
        return Err(RealtimePipeError::UnexpectedClose);
    }
    if opcode != 0x1 {
        return Err(RealtimePipeError::Protocol(format!(
            "expected text frame opcode, got {opcode}"
        )));
    }
    let masked = header[1] & 0x80 != 0;
    let mut len = u64::from(header[1] & 0x7f);
    if len == 126 {
        let mut extended = [0_u8; 2];
        stream.read_exact(&mut extended)?;
        len = u64::from(u16::from_be_bytes(extended));
    } else if len == 127 {
        let mut extended = [0_u8; 8];
        stream.read_exact(&mut extended)?;
        len = u64::from_be_bytes(extended);
    }
    if len > FRAME_BYTES as u64 {
        return Err(RealtimePipeError::FrameTooLarge);
    }
    let mut mask = [0_u8; 4];
    if masked {
        stream.read_exact(&mut mask)?;
    }
    let mut payload = vec![0_u8; usize::try_from(len).expect("bounded websocket frame")];
    stream.read_exact(&mut payload)?;
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % mask.len()];
        }
    }
    String::from_utf8(payload)
        .map_err(|_| RealtimePipeError::Protocol("wakeup frame was not utf-8".to_owned()))
}
