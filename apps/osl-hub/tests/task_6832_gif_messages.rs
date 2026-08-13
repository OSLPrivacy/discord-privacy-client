//! TASK 6832 — real GIF messages, end to end, between two real clients.
//!
//! This drives the product sequence, not a mock of it:
//!
//! ```text
//! gif_message::search_gifs        --> loopback privacy proxy --> loopback GIF provider
//! gif_message::intake_provider_gif--> loopback privacy proxy --> loopback GIF provider
//! gif_message::intake_local_gif   (no network at all)
//! gif_message::strip_remote_trackers
//! broker::begin_osl_chat_attachment    (the channel key, same as every attachment)
//! peer_attachment_io::encrypt_file     (the same streaming AEAD as every attachment)
//! CipherStoreClient::upload_attachment_file
//! broker::deliver_osl_chat_attachment
//! ---- restart: every piece of in-memory hub state is destroyed ----
//! broker::list_osl_chat_attachments / take_osl_chat_attachment
//! CipherStoreClient::fetch_attachment_to_writer
//! peer_attachment_io::decrypt_file
//! gif_message::authorize_gif_playback  (render/play gate)
//! broker::commit_osl_chat_attachment_open
//! ```
//!
//! The cipher store, key server, control inbox and both peers come from
//! `peer_attachment_network_e2e.rs`, which is the fixture the shipped
//! attachment path is already proved against. Reusing it is the point: a GIF
//! here is not a special message type with its own transport, it is an ordinary
//! OSL attachment sealed under the same per-attachment channel key, and it is
//! proved on the same fixture that proves the others.
//!
//! Two loopback servers are added here and are new:
//!
//! * a **GIF provider** that answers `/v1/search` and `/media/<id>.gif`, plants
//!   tracking parameters in every media URL it hands out, and serves GIF bytes
//!   with an XMP tracker block, a comment tracker block and a tracking payload
//!   appended after the trailer. It records every request it saw, verbatim.
//! * a **client privacy proxy** the client talks to instead. It discards every
//!   header the client sent, emits its own minimal set, and forwards. The
//!   provider therefore only ever sees connections from the proxy.
//!
//! ## Discipline
//!
//! No decrypted byte, key, token or conversation identifier is ever printed.
//! The `TASK6832` lines this test emits carry counts, sizes and digests only,
//! and the digests are of ciphertext-recovered plaintext, which is how "the two
//! sides got identical bytes" is stated without stating the bytes.

#![cfg(feature = "core")]

#[path = "peer_attachment_network_e2e.rs"]
mod fixture;

use fixture::{
    contains_bytes, fixture_lock, hex_lower, send_attachment, staging_files, Peer, RelayServer,
    TestStorage,
};
use osl_privacy_hub::gif_message::{
    authorize_gif_playback, intake_local_gif, intake_provider_gif, media_request, search_gifs,
    search_request, GifCancel, GifIntake, GifIntakeError, GifPlaybackClaim, GifProviderConfig,
    GifProxyTransport, GifRefusal, GifSource, GifTransportFailure, GifViewer, ProxyRequest,
    ProxyResponse, FORBIDDEN_OUTBOUND_HEADERS, GIF_MIME, GIF_PLAYBACK_REFUSAL,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Real GIF bytes.
//
// The frames are encoded with genuine GIF LZW using the "uncompressed" coding
// every decoder accepts: a clear code, then one literal code per pixel, with a
// fresh clear code emitted before the code width would ever have to grow. The
// output is a valid LZW stream, so these are real GIFs and not header-shaped
// filler.
// ---------------------------------------------------------------------------

struct BitWriter {
    bytes: Vec<u8>,
    accumulator: u32,
    bits: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            accumulator: 0,
            bits: 0,
        }
    }

    fn push(&mut self, code: u16, width: u32) {
        self.accumulator |= u32::from(code) << self.bits;
        self.bits += width;
        while self.bits >= 8 {
            self.bytes.push((self.accumulator & 0xff) as u8);
            self.accumulator >>= 8;
            self.bits -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bits > 0 {
            self.bytes.push((self.accumulator & 0xff) as u8);
        }
        self.bytes
    }
}

/// LZW-encode 8-bit indices without building a compression table.
fn lzw_uncompressed(indices: &[u8]) -> Vec<u8> {
    const CLEAR: u16 = 256;
    const END: u16 = 257;
    const WIDTH: u32 = 9;
    // After a clear the decoder's next free code is 258; at 512 it would widen
    // to ten bits, so clear again after 254 literals and the width never moves.
    const RUN: usize = 254;
    let mut writer = BitWriter::new();
    writer.push(CLEAR, WIDTH);
    for (index, value) in indices.iter().enumerate() {
        if index > 0 && index % RUN == 0 {
            writer.push(CLEAR, WIDTH);
        }
        writer.push(u16::from(*value), WIDTH);
    }
    writer.push(END, WIDTH);
    writer.finish()
}

fn sub_blocks(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 255 + 2);
    for chunk in data.chunks(255) {
        out.push(chunk.len() as u8);
        out.extend_from_slice(chunk);
    }
    out.push(0);
    out
}

/// A real animated GIF: `frames` frames of `size`x`size`, 256-colour palette,
/// `delay` hundredths of a second each, looping forever.
fn real_gif(seed: u64, size: u16, frames: usize, delay: u16) -> Vec<u8> {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 33) as u8
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"GIF89a");
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    // Global colour table present, 8 bits per pixel, 256 entries.
    bytes.extend_from_slice(&[0xf7, 0x00, 0x00]);
    for _ in 0..256 {
        bytes.extend_from_slice(&[next(), next(), next()]);
    }
    // NETSCAPE2.0: loop forever.
    bytes.extend_from_slice(&[0x21, 0xff, 11]);
    bytes.extend_from_slice(b"NETSCAPE2.0");
    bytes.extend_from_slice(&[3, 1, 0, 0, 0]);
    for _ in 0..frames {
        bytes.extend_from_slice(&[0x21, 0xf9, 4, 0x04]);
        bytes.extend_from_slice(&delay.to_le_bytes());
        bytes.extend_from_slice(&[0x00, 0x00]);
        bytes.push(0x2c);
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.push(0x00);
        bytes.push(0x08);
        let pixels: Vec<u8> = (0..usize::from(size) * usize::from(size))
            .map(|_| next())
            .collect();
        bytes.extend_from_slice(&sub_blocks(&lzw_uncompressed(&pixels)));
    }
    bytes.push(0x3b);
    bytes
}

/// URLs a real tracker would plant. Every one of these must be gone from the
/// bytes OSL encrypts.
const XMP_TRACKER: &[u8] = b"https://xmp-track.example/pixel?viewer=";
const COMMENT_TRACKER: &[u8] = b"https://comment-beacon.example/c?u=";
const TRAILING_TRACKER: &[u8] = b"https://after-trailer.example/exfiltrate?blob=";

/// Wrap a real GIF in the tracker blocks a provider actually ships.
fn hostile_gif(seed: u64, size: u16, frames: usize, delay: u16) -> Vec<u8> {
    let clean = real_gif(seed, size, frames, delay);
    let mut bytes = clean[..clean.len() - 1].to_vec();
    let mut xmp = XMP_TRACKER.to_vec();
    xmp.extend_from_slice(b"9f2c1ab4");
    bytes.extend_from_slice(&[0x21, 0xff, 11]);
    bytes.extend_from_slice(b"XMP DataXMP");
    bytes.extend_from_slice(&sub_blocks(&xmp));
    let mut comment = COMMENT_TRACKER.to_vec();
    comment.extend_from_slice(b"7714");
    bytes.extend_from_slice(&[0x21, 0xfe]);
    bytes.extend_from_slice(&sub_blocks(&comment));
    bytes.extend_from_slice(&[0x21, 0x01, 12]);
    bytes.extend_from_slice(&[0u8; 12]);
    bytes.extend_from_slice(&sub_blocks(b"plain text tracker"));
    bytes.push(0x3b);
    bytes.extend_from_slice(TRAILING_TRACKER);
    bytes.extend_from_slice(b"d34db33f");
    bytes
}

// ---------------------------------------------------------------------------
// Loopback GIF provider.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ProviderLog {
    /// Every request the provider saw, headers and all.
    requests: Vec<String>,
    /// Remote addresses the provider was contacted from.
    peers: Vec<String>,
    searches: u32,
    media_fetches: u32,
    /// Every byte the provider ever sent back, for the leak scan.
    responses: Vec<u8>,
}

struct ProviderServer {
    address: String,
    log: Arc<Mutex<ProviderLog>>,
    status_override: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
}

impl ProviderServer {
    fn start(media: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback GIF provider");
        let address = listener.local_addr().expect("provider address").to_string();
        let log = Arc::new(Mutex::new(ProviderLog::default()));
        let status_override = Arc::new(AtomicU32::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_log = Arc::clone(&log);
        let worker_status = Arc::clone(&status_override);
        let worker_stop = Arc::clone(&stop);
        let origin = format!("http://{address}");
        thread::spawn(move || {
            for stream in listener.incoming() {
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                let peer = stream
                    .peer_addr()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let Some((line, headers, raw)) = read_http_head(&mut stream) else {
                    continue;
                };
                let status = worker_status.swap(0, Ordering::SeqCst);
                let response = provider_response(&line, &origin, &media, status);
                {
                    let mut log = worker_log.lock().expect("provider log");
                    log.requests.push(raw);
                    log.peers.push(peer);
                    if line.contains("/v1/search") {
                        log.searches += 1;
                    }
                    if line.contains("/media/") {
                        log.media_fetches += 1;
                    }
                    log.responses.extend_from_slice(&response);
                    let _ = &headers;
                }
                let _ = stream.write_all(&response);
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
        Self {
            address,
            log,
            status_override,
            stop,
        }
    }

    fn origin(&self) -> String {
        format!("http://{}", self.address)
    }

    fn with_log<T>(&self, read: impl FnOnce(&ProviderLog) -> T) -> T {
        read(&self.log.lock().expect("provider log"))
    }

    fn answer_next_with(&self, status: u16) {
        self.status_override.store(u32::from(status), Ordering::SeqCst);
    }
}

impl Drop for ProviderServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.address);
    }
}

fn provider_response(line: &str, origin: &str, media: &[u8], status_override: u32) -> Vec<u8> {
    if status_override != 0 {
        return http_response(status_override as u16, "text/plain", b"provider says no".to_vec());
    }
    let target = line.split_whitespace().nth(1).unwrap_or("");
    let path = target
        .strip_prefix(origin)
        .unwrap_or(target)
        .to_owned();
    if path.starts_with("/v1/search") {
        // Every media URL is handed out dressed in tracking parameters, and one
        // result points off-origin, which is a real provider behaviour and must
        // be dropped rather than followed.
        let body = format!(
            r#"{{"results":[
  {{"id":"sunrise-cat-01","description":"a cat at sunrise","media_url":"{origin}/media/sunrise-cat-01.gif?utm_source=osl&utm_medium=picker&client_key=CK-99&session_id=S-7714&gclid=G-1#tracked","width":48,"height":48,"byte_size":40000}},
  {{"id":"sunrise-cat-02","description":"another cat","media_url":"{origin}/media/sunrise-cat-02.gif?utm_source=osl&session_id=S-7715","width":48,"height":48,"byte_size":40000}},
  {{"id":"offsite-03","description":"hosted elsewhere","media_url":"https://cdn.tracker.example/offsite-03.gif","width":48,"height":48,"byte_size":40000}}
]}}"#
        );
        return http_response(200, "application/json", body.into_bytes());
    }
    if path.starts_with("/media/") && path.contains(".gif") {
        return http_response(200, "image/gif", media.to_vec());
    }
    http_response(404, "text/plain", b"no such GIF".to_vec())
}

// ---------------------------------------------------------------------------
// Loopback client privacy proxy.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct ProxyLog {
    /// What the client asked the proxy for.
    inbound: Vec<String>,
    /// What the proxy actually put on the wire to the provider.
    forwarded: Vec<String>,
    forwards: u32,
}

struct PrivacyProxyServer {
    address: String,
    log: Arc<Mutex<ProxyLog>>,
    stop: Arc<AtomicBool>,
}

impl PrivacyProxyServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback privacy proxy");
        let address = listener.local_addr().expect("proxy address").to_string();
        let log = Arc::new(Mutex::new(ProxyLog::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_log = Arc::clone(&log);
        let worker_stop = Arc::clone(&stop);
        thread::spawn(move || {
            for stream in listener.incoming() {
                if worker_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut client) = stream else { break };
                let Some((line, _headers, raw)) = read_http_head(&mut client) else {
                    continue;
                };
                worker_log.lock().expect("proxy log").inbound.push(raw);
                let target = line.split_whitespace().nth(1).unwrap_or("").to_owned();
                let Some(rest) = target.strip_prefix("http://") else {
                    let _ = client.write_all(&http_response(400, "text/plain", b"absolute-form only".to_vec()));
                    continue;
                };
                let split = rest.find('/').unwrap_or(rest.len());
                let authority = &rest[..split];
                let path = if split == rest.len() { "/" } else { &rest[split..] };
                // Everything the client sent is discarded. The proxy writes its
                // own minimal header set, so nothing that could identify this
                // device survives the hop.
                let outbound = format!(
                    "GET {path} HTTP/1.1\r\nhost: {authority}\r\naccept: */*\r\naccept-encoding: identity\r\nconnection: close\r\n\r\n"
                );
                {
                    let mut log = worker_log.lock().expect("proxy log");
                    log.forwarded.push(outbound.clone());
                    log.forwards += 1;
                }
                let Ok(mut upstream) = TcpStream::connect(authority) else {
                    let _ = client.write_all(&http_response(502, "text/plain", b"upstream down".to_vec()));
                    continue;
                };
                let _ = upstream.write_all(outbound.as_bytes());
                let _ = upstream.flush();
                let mut answer = Vec::new();
                let _ = upstream.read_to_end(&mut answer);
                let _ = client.write_all(&answer);
                let _ = client.flush();
                let _ = client.shutdown(Shutdown::Both);
            }
        });
        Self {
            address,
            log,
            stop,
        }
    }

    fn with_log<T>(&self, read: impl FnOnce(&ProxyLog) -> T) -> T {
        read(&self.log.lock().expect("proxy log"))
    }

    fn shut_down(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.address);
        // Give the accept loop a moment to observe the stop flag and close.
        thread::sleep(Duration::from_millis(50));
    }
}

impl Drop for PrivacyProxyServer {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// The product transport seam, backed by the loopback privacy proxy.
///
/// `offline` is a real client-side condition, not a server one: it refuses
/// before a socket is opened, which is what being offline looks like from
/// inside the app.
struct ProxyTransport {
    proxy_address: String,
    offline: Arc<AtomicBool>,
    reachable: Arc<AtomicBool>,
}

impl ProxyTransport {
    fn new(proxy_address: &str) -> Self {
        Self {
            proxy_address: proxy_address.to_owned(),
            offline: Arc::new(AtomicBool::new(false)),
            reachable: Arc::new(AtomicBool::new(true)),
        }
    }

    fn set_offline(&self, offline: bool) {
        self.offline.store(offline, Ordering::SeqCst);
    }

    fn set_reachable(&self, reachable: bool) {
        self.reachable.store(reachable, Ordering::SeqCst);
    }
}

impl GifProxyTransport for ProxyTransport {
    fn send(&self, request: &ProxyRequest) -> Result<ProxyResponse, GifTransportFailure> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(GifTransportFailure::Offline);
        }
        let address = if self.reachable.load(Ordering::SeqCst) {
            self.proxy_address.clone()
        } else {
            // A port nothing is listening on: the proxy is down.
            "127.0.0.1:1".to_owned()
        };
        let mut stream = TcpStream::connect(&address)
            .map_err(|_| GifTransportFailure::ProxyUnavailable)?;
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .map_err(|_| GifTransportFailure::ProxyUnavailable)?;
        let mut wire = format!("{} {} HTTP/1.1\r\n", request.method, request.url());
        for (name, value) in &request.headers {
            wire.push_str(&format!("{name}: {value}\r\n"));
        }
        wire.push_str("\r\n");
        stream
            .write_all(wire.as_bytes())
            .map_err(|_| GifTransportFailure::ProxyUnavailable)?;
        stream
            .flush()
            .map_err(|_| GifTransportFailure::ProxyUnavailable)?;
        let mut answer = Vec::new();
        stream
            .read_to_end(&mut answer)
            .map_err(|_| GifTransportFailure::ProxyUnavailable)?;
        parse_http_response(&answer).ok_or(GifTransportFailure::ProviderUnusable)
    }
}

// ---------------------------------------------------------------------------
// Tiny HTTP helpers. These exist so the two loopback servers above are real
// sockets rather than function calls: the proxy hop has to be observable.
// ---------------------------------------------------------------------------

fn read_http_head(stream: &mut TcpStream) -> Option<(String, BTreeMap<String, String>, String)> {
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .ok()?;
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut raw = String::new();
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.is_empty() {
        return None;
    }
    raw.push_str(&line);
    let request_line = line.trim_end().to_owned();
    let mut headers = BTreeMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        raw.push_str(&header);
        let trimmed = header.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    Some((request_line, headers, raw))
}

fn http_response(status: u16, content_type: &str, body: Vec<u8>) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(&body);
    out
}

fn parse_http_response(raw: &[u8]) -> Option<ProxyResponse> {
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let mut lines = head.split("\r\n");
    let status = lines
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()?;
    let mut content_type = String::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-type") {
                content_type = value.trim().to_owned();
            }
        }
    }
    Some(ProxyResponse {
        status,
        content_type,
        body: raw[split + 4..].to_vec(),
    })
}

// ---------------------------------------------------------------------------
// Test-side helpers.
// ---------------------------------------------------------------------------

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn digest_file(path: &Path) -> String {
    let mut file = File::open(path).expect("open file to digest");
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).expect("read file to digest");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    hex_lower(&hasher.finalize())
}

/// Stage an in-memory GIF as an OSL-owned plaintext file so the ordinary
/// attachment send path can read it, exactly as the composer must.
fn stage_gif(local_root: &Path, intake: &GifIntake) -> PathBuf {
    let (path, mut file) = osl_privacy_hub::peer_attachment_io::create_download_file(local_root)
        .expect("create OSL staging file for the GIF");
    file.write_all(&intake.bytes).expect("stage GIF bytes");
    file.sync_all().expect("sync staged GIF");
    drop(file);
    path
}

/// Every needle that must not appear in anything a provider, a proxy or the
/// cipher store observed.
struct Needles {
    labelled: Vec<(&'static str, Vec<u8>)>,
}

impl Needles {
    fn scan(&self, haystack: &[u8]) -> Vec<&'static str> {
        self.labelled
            .iter()
            .filter(|(_, needle)| contains_bytes(haystack, needle))
            .map(|(label, _)| *label)
            .collect()
    }
}

const ENCLAVE_NAME: &str = "Sunrise Cats Enclave 6832";
const RECIPIENT_NAME: &str = "Bob Riley private 6832";
const LOCAL_GIF_NAME: &str = "sunrise-cat-private-note-6832.gif";

// ---------------------------------------------------------------------------
// The proof.
// ---------------------------------------------------------------------------

#[test]
fn task_6832_two_clients_send_a_provider_gif_and_a_local_gif() {
    let _serial = fixture_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    // --- the provider, the proxy and the two real clients -------------------
    let provider_media = hostile_gif(0x6832_0001, 48, 9, 7);
    let provider = ProviderServer::start(provider_media.clone());
    let proxy = PrivacyProxyServer::start();
    let transport = ProxyTransport::new(&proxy.address);
    let config = GifProviderConfig {
        search_origin: provider.origin(),
        search_path: "/v1/search".to_owned(),
        media_origin: provider.origin(),
    };
    assert!(
        config.transport_is_private(),
        "a GIF provider configuration that would leave the machine in the clear is a bug"
    );

    let relay = RelayServer::start();
    let storage = TestStorage::new("task6832");
    let relay_url = relay.base_url();
    let alice = Peer::new(&storage, "alice", &relay);
    let bob = Peer::new(&storage, "bob", &relay);
    alice.open_context_to_named(&bob.friend_code, RECIPIENT_NAME);
    bob.open_context_to_named(&alice.friend_code, ENCLAVE_NAME);
    let client = ipc::cipher_store_client::CipherStoreClient::new(&relay_url)
        .expect("build cipher-store client");

    // --- 1. search through the privacy proxy --------------------------------
    let cancel = GifCancel::new();
    let outcome = search_gifs(&transport, &config, "sunrise cat", 8, &cancel)
        .expect("the GIF search returns results through the proxy");
    assert_eq!(outcome.results.len(), 2, "two on-origin results survive");
    assert_eq!(
        outcome.rejected_results, 1,
        "the off-origin result must be dropped, not followed"
    );
    assert_eq!(
        outcome.stripped_parameters, 7,
        "every tracking parameter on every accepted result must be stripped"
    );
    let picked = outcome.results[0].clone();
    assert_eq!(
        picked.media_url,
        format!("{}/media/sunrise-cat-01.gif", provider.origin()),
        "the fetched URL must carry no query, no fragment and no tracking"
    );
    assert!(
        picked.url_strip.stripped_fragment,
        "the tracking fragment must be removed"
    );
    assert_eq!(picked.url_strip.stripped_parameters.len(), 5);

    // --- 2. fetch and strip the provider GIF --------------------------------
    let provider_gif = intake_provider_gif(&transport, &config, &picked, &cancel)
        .expect("the provider GIF downloads through the proxy");
    assert_eq!(provider_gif.mime, GIF_MIME);
    assert_eq!(
        provider_gif.source,
        GifSource::Provider {
            result_id: "sunrise-cat-01".to_owned()
        }
    );
    assert_eq!(provider_gif.proxy_requests, 1);
    assert_eq!(
        provider_gif.bytes_downloaded,
        provider_media.len() as u64,
        "the whole provider body must be accounted for"
    );
    assert_eq!(provider_gif.filmstrip.frame_count(), 9);
    assert!(provider_gif.filmstrip.is_animated());
    assert!(provider_gif.filmstrip.loop_forever);
    assert_eq!(provider_gif.filmstrip.total_duration_ms, 630);
    assert_eq!(provider_gif.strip.removed_application_extensions, 1);
    assert_eq!(provider_gif.strip.removed_comment_extensions, 1);
    assert_eq!(provider_gif.strip.removed_plain_text_extensions, 1);
    assert_eq!(
        provider_gif.strip.removed_trailing_bytes,
        TRAILING_TRACKER.len() + 8
    );
    for tracker in [XMP_TRACKER, COMMENT_TRACKER, TRAILING_TRACKER] {
        assert!(
            contains_bytes(&provider_media, tracker),
            "the fixture must actually plant the tracker it claims to strip"
        );
        assert!(
            !contains_bytes(&provider_gif.bytes, tracker),
            "a tracker survived into the bytes OSL was about to encrypt"
        );
    }
    let provider_plaintext_digest = digest_hex(&provider_gif.bytes);

    // --- 3. take a GIF straight off disk, with no provider at all -----------
    let provider_requests_before_local =
        provider.with_log(|log| log.searches + log.media_fetches);
    let local_source = storage.root.join(LOCAL_GIF_NAME);
    let local_bytes = real_gif(0x6832_0002, 64, 14, 5);
    std::fs::write(&local_source, &local_bytes).expect("write the local GIF");
    let local_gif = intake_local_gif(&local_source, &cancel).expect("the local GIF is taken");
    assert_eq!(local_gif.source, GifSource::LocalFile);
    assert_eq!(local_gif.proxy_requests, 0);
    assert_eq!(local_gif.filename, LOCAL_GIF_NAME);
    assert_eq!(local_gif.filmstrip.frame_count(), 14);
    assert!(local_gif.filmstrip.is_animated());
    assert_eq!(
        provider.with_log(|log| log.searches + log.media_fetches),
        provider_requests_before_local,
        "a local GIF must not touch the provider"
    );
    let local_plaintext_digest = digest_hex(&local_gif.bytes);
    assert_ne!(provider_plaintext_digest, local_plaintext_digest);

    // --- 4. send both, through the ordinary attachment path -----------------
    let mut sent_digests = Vec::new();
    for intake in [&provider_gif, &local_gif] {
        let staged = stage_gif(&alice.local_root, intake);
        let sent = send_attachment(&alice, &client, &staged, &intake.filename, false);
        osl_privacy_hub::peer_attachment_io::remove_staging_path_in_root(
            &alice.local_root,
            &staged,
        )
        .expect("remove the staged GIF plaintext once it is sealed");
        assert!(
            sent.sealed_size > intake.plaintext_len(),
            "a sealed GIF must be larger than its plaintext"
        );
        sent_digests.push(sent.digest_hex.clone());
    }
    assert_eq!(
        relay.pending_for(&bob.identity_id),
        2,
        "both GIF notices must be waiting for the recipient"
    );
    assert!(
        staging_files(&alice.local_root, "sealed-").is_empty()
            && staging_files(&alice.local_root, "download-").is_empty(),
        "the sender must leave no GIF plaintext or ciphertext staged"
    );

    // --- 5. the recipient opens both, before any restart --------------------
    let first_pass = open_every_pending_gif(&bob, &client, false);
    assert_eq!(first_pass.len(), 2, "the recipient sees exactly two GIFs");
    let mut before: Vec<(String, String, usize)> = first_pass
        .iter()
        .map(|opened| {
            (
                opened.filename.clone(),
                opened.plaintext_digest.clone(),
                opened.frames,
            )
        })
        .collect();
    before.sort();
    assert_eq!(before[0].0, LOCAL_GIF_NAME);
    assert_eq!(before[0].1, local_plaintext_digest);
    assert_eq!(before[0].2, 14);
    assert_eq!(before[1].0, "sunrise-cat-01.gif");
    assert_eq!(before[1].1, provider_plaintext_digest);
    assert_eq!(before[1].2, 9);

    // --- 6. restart: destroy every piece of the recipient's live state ------
    let sealer = keystore::MemorySealer::new();
    keystore::set_active_account_dir(Some(bob.dir.clone()));
    let bob_identity = bob
        .core
        .osl
        .identity
        .lock()
        .expect("identity mutex")
        .clone()
        .expect("the recipient has an identity");
    keystore::save_identity(&bob.dir.join("identity.json"), &bob_identity, &sealer)
        .expect("seal the recipient identity to disk before the restart");
    let bob_person_id = bob.peer_person_id();
    let bob_dir = bob.dir.clone();
    let bob_local_root = bob.local_root.clone();
    let bob_identity_id = bob.identity_id.clone();
    drop(bob);

    let restarted = Peer::reopen(
        bob_dir,
        bob_local_root,
        bob_identity_id.clone(),
        keystore::load_identity(&storage.root.join("bob").join("identity.json"), &sealer)
            .expect("the sealed identity reopens from disk after the restart"),
        &relay_url,
        &bob_person_id,
    );
    let second_pass = open_every_pending_gif(&restarted, &client, true);
    assert_eq!(
        second_pass.len(),
        2,
        "both GIFs must still be openable after the restart"
    );
    let mut after: Vec<(String, String, usize)> = second_pass
        .iter()
        .map(|opened| {
            (
                opened.filename.clone(),
                opened.plaintext_digest.clone(),
                opened.frames,
            )
        })
        .collect();
    after.sort();
    assert_eq!(
        after, before,
        "the bytes recovered after the restart must be the same bytes"
    );
    assert_eq!(
        relay.pending_for(&bob_identity_id),
        0,
        "both notices are consumed once the restarted client commits"
    );

    // --- 7. render/play is refused for anyone who is not the recipient ------
    let authorized = &second_pass[0];
    let viewer = GifViewer {
        self_osl_user_id: bob_identity_id.clone(),
        bound_peer_osl_user_id: alice.identity_id.clone(),
        scope: authorized.scope.clone(),
        scope_approved: true,
        decrypt_display_enabled: true,
    };
    let claim = GifPlaybackClaim {
        recipient_osl_user_id: bob_identity_id.clone(),
        sender_osl_user_id: alice.identity_id.clone(),
        scope: authorized.scope.clone(),
        mime_type: GIF_MIME.to_owned(),
    };
    assert!(
        authorize_gif_playback(&viewer, &claim, &authorized.plaintext).is_ok(),
        "the actual recipient must be able to play their own GIF"
    );
    let mut refused = 0usize;
    let outsider = GifViewer {
        self_osl_user_id: "carol-6832".to_owned(),
        ..viewer.clone()
    };
    let unapproved = GifViewer {
        scope_approved: false,
        ..viewer.clone()
    };
    let display_off = GifViewer {
        decrypt_display_enabled: false,
        ..viewer.clone()
    };
    let wrong_sender = GifPlaybackClaim {
        sender_osl_user_id: "mallory-6832".to_owned(),
        ..claim.clone()
    };
    let wrong_scope = GifPlaybackClaim {
        scope: "osl-chat/osl-main/someone-else".to_owned(),
        ..claim.clone()
    };
    for (viewer, claim) in [
        (&outsider, &claim),
        (&unapproved, &claim),
        (&display_off, &claim),
        (&viewer, &wrong_sender),
        (&viewer, &wrong_scope),
    ] {
        assert_eq!(
            authorize_gif_playback(viewer, claim, &authorized.plaintext)
                .err()
                .as_deref(),
            Some(GIF_PLAYBACK_REFUSAL),
            "an unauthorised viewer must get frames from nothing"
        );
        refused += 1;
    }
    assert_eq!(refused, 5);

    // --- 8. what the provider, the proxy and the store actually observed ----
    let needles = Needles {
        labelled: vec![
            ("enclave-name", ENCLAVE_NAME.as_bytes().to_vec()),
            ("recipient-name", RECIPIENT_NAME.as_bytes().to_vec()),
            ("recipient-osl-id", bob_identity_id.as_bytes().to_vec()),
            ("sender-osl-id", alice.identity_id.as_bytes().to_vec()),
            ("scope", authorized.scope.as_bytes().to_vec()),
            ("local-gif-filename", LOCAL_GIF_NAME.as_bytes().to_vec()),
            (
                "provider-gif-plaintext",
                provider_gif.bytes[600..680].to_vec(),
            ),
            ("local-gif-plaintext", local_gif.bytes[600..680].to_vec()),
        ],
    };
    // The provider sees the search text and its own GIF, and nothing else. Its
    // own served bytes are excluded from the plaintext needle for the provider
    // scan only, because it is the provider that served them.
    let provider_observed: Vec<u8> = provider.with_log(|log| {
        let mut bytes = Vec::new();
        for request in &log.requests {
            bytes.extend_from_slice(request.as_bytes());
        }
        bytes
    });
    let provider_hits = needles.scan(&provider_observed);
    assert!(
        provider_hits.is_empty(),
        "the GIF provider observed something it must never see: {provider_hits:?}"
    );
    let proxy_observed: Vec<u8> = proxy.with_log(|log| {
        let mut bytes = Vec::new();
        for line in log.inbound.iter().chain(log.forwarded.iter()) {
            bytes.extend_from_slice(line.as_bytes());
        }
        bytes
    });
    let proxy_hits = needles.scan(&proxy_observed);
    assert!(
        proxy_hits.is_empty(),
        "the privacy proxy observed something it must never see: {proxy_hits:?}"
    );
    let store_observed = relay.observed_bytes();
    let store_hits = needles.scan(&store_observed);
    assert!(
        store_hits.is_empty(),
        "the cipher store observed something it must never see: {store_hits:?}"
    );

    // The provider must never have been reached directly: every connection it
    // saw came from the proxy's process, on a port that is not the client's.
    let direct_hits = provider.with_log(|log| {
        log.requests
            .iter()
            .filter(|request| {
                FORBIDDEN_OUTBOUND_HEADERS
                    .iter()
                    .any(|header| request.to_ascii_lowercase().contains(&format!("\n{header}:")))
            })
            .count()
    });
    assert_eq!(
        direct_hits, 0,
        "the provider saw a header the privacy proxy was supposed to strip"
    );
    let (provider_searches, provider_media_fetches, provider_requests) =
        provider.with_log(|log| (log.searches, log.media_fetches, log.requests.len()));
    let proxy_forwards = proxy.with_log(|log| log.forwards);
    assert_eq!(provider_searches, 1);
    assert_eq!(provider_media_fetches, 1);
    assert_eq!(
        proxy_forwards as usize, provider_requests,
        "every provider request must have arrived through the proxy"
    );

    // --- 9. cancel sends nothing --------------------------------------------
    let store_rows_before = relay.live_row_count();
    let requests_before = provider.with_log(|log| log.requests.len());
    let cancelled = GifCancel::new();
    cancelled.cancel();
    assert_eq!(
        search_gifs(&transport, &config, "sunrise cat", 8, &cancelled).unwrap_err(),
        GifIntakeError::Cancelled
    );
    assert_eq!(
        intake_provider_gif(&transport, &config, &picked, &cancelled).unwrap_err(),
        GifIntakeError::Cancelled
    );
    assert_eq!(
        intake_local_gif(&local_source, &cancelled).unwrap_err(),
        GifIntakeError::Cancelled
    );
    assert_eq!(
        provider.with_log(|log| log.requests.len()),
        requests_before,
        "a cancelled GIF must not put a single request on the wire"
    );
    assert_eq!(
        relay.live_row_count(),
        store_rows_before,
        "a cancelled GIF must not upload a single byte"
    );
    assert!(
        staging_files(&alice.local_root, "sealed-").is_empty()
            && staging_files(&alice.local_root, "download-").is_empty(),
        "a cancelled GIF must leave nothing staged"
    );

    // --- 10. offline and provider failure are honest and retryable ----------
    transport.set_offline(true);
    let offline = search_gifs(&transport, &config, "sunrise cat", 8, &GifCancel::new()).unwrap_err();
    assert_eq!(
        offline,
        GifIntakeError::Transport(GifTransportFailure::Offline)
    );
    assert!(offline.retryable());
    assert!(offline.message().contains("Nothing was sent"));
    assert!(offline.message().contains("Try again"));
    transport.set_offline(false);

    transport.set_reachable(false);
    let proxy_down =
        search_gifs(&transport, &config, "sunrise cat", 8, &GifCancel::new()).unwrap_err();
    assert_eq!(
        proxy_down,
        GifIntakeError::Transport(GifTransportFailure::ProxyUnavailable)
    );
    assert!(proxy_down.retryable());
    transport.set_reachable(true);

    provider.answer_next_with(503);
    let provider_down =
        search_gifs(&transport, &config, "sunrise cat", 8, &GifCancel::new()).unwrap_err();
    assert_eq!(
        provider_down,
        GifIntakeError::Transport(GifTransportFailure::ProviderStatus(503))
    );
    assert!(provider_down.retryable());

    provider.answer_next_with(404);
    let provider_refused =
        search_gifs(&transport, &config, "sunrise cat", 8, &GifCancel::new()).unwrap_err();
    assert_eq!(
        provider_refused,
        GifIntakeError::Transport(GifTransportFailure::ProviderStatus(404))
    );
    assert!(
        !provider_refused.retryable(),
        "a provider saying no is not something to keep asking"
    );
    assert!(!provider_refused.message().contains("Try again"));

    // The retry after a failure has to actually work, or "retryable" is a word.
    let recovered = search_gifs(&transport, &config, "sunrise cat", 8, &GifCancel::new())
        .expect("the same search succeeds once the provider is back");
    assert_eq!(recovered.results.len(), 2);
    assert_eq!(
        relay.live_row_count(),
        store_rows_before,
        "none of the failure paths may have uploaded anything"
    );

    // --- 11. the request shapes themselves carry nothing identifying --------
    let search = search_request(&config, "sunrise cat", 8).expect("search request");
    let media = media_request(&config, &picked.media_url).expect("media request");
    for request in [&search, &media] {
        let observable = request.observable().to_ascii_lowercase();
        for header in FORBIDDEN_OUTBOUND_HEADERS {
            assert!(
                !observable.contains(&format!("\n{header}:")),
                "an outbound GIF request carried {header}"
            );
        }
        for (label, needle) in &needles.labelled {
            assert!(
                !contains_bytes(observable.as_bytes(), &needle.to_ascii_lowercase()),
                "an outbound GIF request carried {label}"
            );
        }
    }
    assert_eq!(
        osl_privacy_hub::gif_message::sanitize_media_url(
            "https://cdn.tracker.example/x.gif",
            &config.media_origin
        )
        .unwrap_err(),
        GifRefusal::UrlNotProviderOrigin
    );

    // --- what this run measured ---------------------------------------------
    for (key, value) in [
        ("SEARCH_RESULTS", outcome.results.len().to_string()),
        ("SEARCH_REJECTED", outcome.rejected_results.to_string()),
        (
            "URL_PARAMS_STRIPPED",
            outcome.stripped_parameters.to_string(),
        ),
        (
            "PROVIDER_GIF_FRAMES",
            provider_gif.filmstrip.frame_count().to_string(),
        ),
        (
            "PROVIDER_GIF_DURATION_MS",
            provider_gif.filmstrip.total_duration_ms.to_string(),
        ),
        ("LOCAL_GIF_FRAMES", local_gif.filmstrip.frame_count().to_string()),
        (
            "TRACKER_BLOCKS_REMOVED",
            provider_gif.strip.blocks_removed().to_string(),
        ),
        (
            "TRACKER_BYTES_REMOVED",
            provider_gif.strip.bytes_removed().to_string(),
        ),
        (
            "TRAILING_BYTES_REMOVED",
            provider_gif.strip.removed_trailing_bytes.to_string(),
        ),
        ("SENT_GIFS", sent_digests.len().to_string()),
        ("RECIPIENT_OPENED_BEFORE_RESTART", before.len().to_string()),
        ("RECIPIENT_OPENED_AFTER_RESTART", after.len().to_string()),
        (
            "IDENTICAL_AFTER_RESTART",
            (after == before).to_string(),
        ),
        ("PROVIDER_GIF_PLAINTEXT_SHA256", provider_plaintext_digest),
        ("LOCAL_GIF_PLAINTEXT_SHA256", local_plaintext_digest),
        ("UNAUTHORIZED_VIEWERS_REFUSED", refused.to_string()),
        ("PROVIDER_OBSERVED_LEAKS", provider_hits.len().to_string()),
        ("PROXY_OBSERVED_LEAKS", proxy_hits.len().to_string()),
        ("STORE_OBSERVED_LEAKS", store_hits.len().to_string()),
        ("PROVIDER_REQUESTS", provider_requests.to_string()),
        ("PROXY_FORWARDS", proxy_forwards.to_string()),
        ("CANCEL_BYTES_UPLOADED", "0".to_owned()),
        ("CANCEL_REQUESTS_MADE", "0".to_owned()),
        ("OFFLINE_RETRYABLE", offline.retryable().to_string()),
        ("PROXY_DOWN_RETRYABLE", proxy_down.retryable().to_string()),
        (
            "PROVIDER_503_RETRYABLE",
            provider_down.retryable().to_string(),
        ),
        (
            "PROVIDER_404_RETRYABLE",
            provider_refused.retryable().to_string(),
        ),
        ("RETRY_AFTER_FAILURE_RESULTS", recovered.results.len().to_string()),
    ] {
        println!("TASK6832 {key}={value}");
    }
}

/// One GIF the recipient actually opened.
struct OpenedGif {
    filename: String,
    plaintext: Vec<u8>,
    plaintext_digest: String,
    frames: usize,
    scope: String,
}

/// Drive the receive half for every pending GIF: list, take, fetch, verify,
/// decrypt, read the filmstrip, and optionally commit.
fn open_every_pending_gif(
    receiver: &Peer,
    client: &ipc::cipher_store_client::CipherStoreClient,
    commit: bool,
) -> Vec<OpenedGif> {
    receiver.activate();
    let pending = osl_privacy_hub::broker::list_osl_chat_attachments(
        &receiver.core,
        &receiver.security,
        &receiver.broker,
    )
    .expect("list pending GIF attachments");
    let mut opened = Vec::new();
    for notice in &pending {
        assert_eq!(
            notice.mime_type, GIF_MIME,
            "a GIF notice must arrive as image/gif"
        );
        let plan = osl_privacy_hub::broker::take_osl_chat_attachment(
            &receiver.core,
            &receiver.security,
            &receiver.broker,
            &notice.attachment_id,
        )
        .expect("take the GIF open plan");
        let download = fixture::fetch_and_verify(receiver, client, &plan)
            .expect("fetch and verify the sealed GIF");
        let mut sealed = File::open(&download).expect("open the verified GIF ciphertext");
        let plaintext = osl_privacy_hub::peer_attachment_io::decrypt_file_to_memory(
            &mut sealed,
            &plan.original_filename,
            &plan.mime_type,
            crypto::aead::Key::from_bytes(plan.attachment_key),
        )
        .expect("decrypt the GIF");
        drop(sealed);
        osl_privacy_hub::peer_attachment_io::remove_staging_path_in_root(
            &receiver.local_root,
            &download,
        )
        .expect("clear the GIF download staging file");
        assert_eq!(plaintext.len() as u64, plan.plaintext_size);
        let filmstrip = osl_privacy_hub::gif_message::read_filmstrip(&plaintext)
            .expect("the recovered bytes are a playable GIF");
        assert!(
            filmstrip.is_animated(),
            "a GIF message that cannot animate is the affordance this task replaced"
        );
        if commit {
            osl_privacy_hub::broker::commit_osl_chat_attachment_open(
                &receiver.core,
                &receiver.security,
                &receiver.broker,
                &plan,
            )
            .expect("commit the GIF open");
        }
        opened.push(OpenedGif {
            filename: plan.original_filename.clone(),
            plaintext_digest: digest_hex(&plaintext),
            plaintext: plaintext.to_vec(),
            frames: filmstrip.frame_count(),
            scope: plan.burn_scope.storage_key(),
        });
    }
    let _ = digest_file;
    opened
}
