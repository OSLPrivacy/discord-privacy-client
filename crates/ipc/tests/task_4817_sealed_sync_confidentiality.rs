//! TASK 4817 — prove every allowed sync payload remains end-to-end encrypted.
//!
//! Two paired devices sync one independently generated high-entropy marked
//! value for every allowed kind in the TASK 4811 registry, over the ordinary
//! self-message path from TASK 4807, through a live release relay process with
//! a recording TCP tap in front of it. Then:
//!
//! * a separate process restarts the destination device from disk and decrypts
//!   every source value again;
//! * a generated inventory of eleven relay surfaces — process memory, request
//!   bodies, response bodies, queue, database, blobs, caches, logs, telemetry,
//!   crash export and packet capture — is scanned byte for byte for the marked
//!   values, the field names, field-name/value pairs and the actual content
//!   keys, in raw, hex, base64 and UTF-16LE encodings;
//! * an independent non-key holder and a wrong device key are handed every
//!   captured envelope;
//! * ordinary user messages on the same conversation are sent as a control and
//!   compared attribute for attribute;
//! * a throwaway mutation puts correctly merging plaintext sync bodies inside
//!   the same ordinary-looking envelope and is pushed through a second live
//!   relay, where the same scanner must recover them and name every location.
//!
//! There is no TLS anywhere on the path. The tap records the application bytes
//! themselves, so nothing here can be a claim about transport security.

use ipc::ordinary_sync::{FieldWiseState, VersionStamp, ADDED_SERVER_VISIBLE_MESSAGE_KINDS};
use ipc::sealed_sync::{
    self, at_rest, open_sync_message, seal_allowed_sync_value, seal_ordinary_message, SyncValue,
};
use ipc::sync_policy;
use ipc::wire_v2::{self, RecipientV3, SLOT_V3_BYTES};
use keystore::{identity_from_entropy, Identity};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const SEALED_SYNC_SOURCE: &str = include_str!("../src/sealed_sync.rs");

/// The persisted field name each allowed kind carries. Coverage is checked
/// against the 4811 registry, not against this table.
const KIND_FIELDS: &[(&str, &str)] = &[
    ("identity_public_profile", "profile_display_name"),
    ("friend_roster", "roster_entry_alias"),
    ("peer_public_key_bundles", "bundle_fingerprint"),
    ("safety_number_pins", "pinned_safety_number"),
    ("conversation_membership", "membership_row"),
    ("server_whitelist_rules", "server_rule_body"),
    ("channel_whitelist_rules", "channel_rule_body"),
    ("message_metadata", "message_metadata_row"),
    ("attachment_pointers", "attachment_pointer_url"),
    ("post_pairing_messages", "post_pairing_message_text"),
];

const SYNC_CHANNEL: &str = "c8a41f7d2b6e";
const MUTATION_CHANNEL: &str = "f2d90c5a1e83";

// ---------------------------------------------------------------- starvation

/// Which leg of the proof this run deliberately leaves out. Every value other
/// than `None` must make the check exit 1 naming what is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Starve {
    None,
    AllowedKind(String),
    ReceiverReadback,
    RelaySurface(String),
    WrongKey,
    OrdinaryControl,
    PlaintextMutation,
    TlsOnly,
    EnvelopeShapeOnly,
    RefuseSync,
}

fn starve() -> Starve {
    let raw = std::env::var("TASK4817_STARVE").unwrap_or_default();
    let (head, tail) = match raw.split_once(':') {
        Some((head, tail)) => (head, tail.to_owned()),
        None => (raw.as_str(), String::new()),
    };
    match head {
        "" => Starve::None,
        "allowed_kind" => Starve::AllowedKind(tail),
        "receiver_readback" => Starve::ReceiverReadback,
        "relay_surface" => Starve::RelaySurface(tail),
        "wrong_key" => Starve::WrongKey,
        "ordinary_control" => Starve::OrdinaryControl,
        "plaintext_mutation" => Starve::PlaintextMutation,
        "tls_only" => Starve::TlsOnly,
        "envelope_shape_only" => Starve::EnvelopeShapeOnly,
        "refuse_sync" => Starve::RefuseSync,
        other => {
            eprintln!("TASK4817_ERROR unknown starve case {other}");
            std::process::exit(2);
        }
    }
}

fn missing(reason: &str) -> ! {
    println!("TASK4817_MISSING {reason}");
    println!("TASK4817_RESULT=fail");
    let _ = std::io::stdout().flush();
    std::process::exit(1);
}

// --------------------------------------------------------------- byte search

/// Boyer–Moore–Horspool with a 4-byte prefix bloom so ~300 patterns can be run
/// over every byte of a multi-hundred-megabyte inventory in one pass each.
struct Scanner {
    patterns: Vec<Pattern>,
    filter: Vec<u64>,
    index: HashMap<[u8; 4], Vec<usize>>,
}

#[derive(Clone)]
struct Pattern {
    label: String,
    class: PatternClass,
    bytes: Vec<u8>,
    encoding: &'static str,
}

#[derive(Clone, PartialEq, Eq, Debug)]
enum PatternClass {
    MarkedValue(String),
    FieldName(String),
    FieldValuePair(String),
    ContentKey(String),
    Envelope(usize),
}

fn bloom_index(bytes: &[u8]) -> usize {
    ((bytes[0] as usize)
        | ((bytes[1] as usize) << 3)
        | ((bytes[2] as usize) << 6)
        | ((bytes[3] as usize) << 9))
        & 0xFFFF
}

impl Scanner {
    fn new(patterns: Vec<Pattern>) -> Self {
        let mut filter = vec![0u64; 1024];
        let mut index: HashMap<[u8; 4], Vec<usize>> = HashMap::new();
        for (ix, pattern) in patterns.iter().enumerate() {
            assert!(
                pattern.bytes.len() >= 8,
                "pattern {} is too short to be evidence",
                pattern.label
            );
            let prefix: [u8; 4] = pattern.bytes[..4].try_into().expect("prefix");
            let slot = bloom_index(&prefix);
            filter[slot / 64] |= 1u64 << (slot % 64);
            index.entry(prefix).or_default().push(ix);
        }
        Scanner {
            patterns,
            filter,
            index,
        }
    }

    /// Every occurrence of every pattern in `haystack`, as (pattern, offset).
    fn scan(&self, haystack: &[u8]) -> Vec<(usize, usize)> {
        let mut hits = Vec::new();
        if haystack.len() < 4 {
            return hits;
        }
        for offset in 0..=haystack.len() - 4 {
            let window = &haystack[offset..offset + 4];
            let slot = bloom_index(window);
            if self.filter[slot / 64] & (1u64 << (slot % 64)) == 0 {
                continue;
            }
            let prefix: [u8; 4] = window.try_into().expect("window");
            let Some(candidates) = self.index.get(&prefix) else {
                continue;
            };
            for &candidate in candidates {
                let pattern = &self.patterns[candidate];
                if haystack.len() - offset >= pattern.bytes.len()
                    && &haystack[offset..offset + pattern.bytes.len()] == pattern.bytes.as_slice()
                {
                    hits.push((candidate, offset));
                }
            }
        }
        hits
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_upper(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.encode(bytes)
}

/// Base64 substrings that survive every byte alignment: whatever offset the
/// secret sits at inside a larger base64 stream, one of these appears.
fn base64_alignments(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for shift in 0..3usize {
        let mut padded = vec![0u8; shift];
        padded.extend_from_slice(bytes);
        let encoded = base64_of(&padded);
        let start = (shift * 4 + 2) / 3;
        if encoded.len() > start + 8 {
            let end = encoded.len() - 4;
            if end > start + 8 {
                out.push(encoded[start..end].to_owned());
            }
        }
    }
    out
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

/// Every encoding of one secret string a relay surface could plausibly hold.
fn string_patterns(label: &str, class: PatternClass, text: &str) -> Vec<Pattern> {
    let mut out = vec![
        Pattern {
            label: format!("{label}/raw"),
            class: class.clone(),
            bytes: text.as_bytes().to_vec(),
            encoding: "raw",
        },
        Pattern {
            label: format!("{label}/hex-lower"),
            class: class.clone(),
            bytes: hex_lower(text.as_bytes()).into_bytes(),
            encoding: "hex-lower",
        },
        Pattern {
            label: format!("{label}/hex-upper"),
            class: class.clone(),
            bytes: hex_upper(text.as_bytes()).into_bytes(),
            encoding: "hex-upper",
        },
        Pattern {
            label: format!("{label}/utf16le"),
            class: class.clone(),
            bytes: utf16le(text),
            encoding: "utf16le",
        },
    ];
    for (ix, encoded) in base64_alignments(text.as_bytes()).into_iter().enumerate() {
        out.push(Pattern {
            label: format!("{label}/base64-{ix}"),
            class: class.clone(),
            bytes: encoded.into_bytes(),
            encoding: "base64",
        });
    }
    out
}

/// Every encoding of one raw key.
fn key_patterns(label: &str, bytes: &[u8]) -> Vec<Pattern> {
    let class = PatternClass::ContentKey(label.to_owned());
    let mut out = vec![
        Pattern {
            label: format!("{label}/raw"),
            class: class.clone(),
            bytes: bytes.to_vec(),
            encoding: "raw",
        },
        Pattern {
            label: format!("{label}/hex-lower"),
            class: class.clone(),
            bytes: hex_lower(bytes).into_bytes(),
            encoding: "hex-lower",
        },
        Pattern {
            label: format!("{label}/hex-upper"),
            class: class.clone(),
            bytes: hex_upper(bytes).into_bytes(),
            encoding: "hex-upper",
        },
    ];
    for (ix, encoded) in base64_alignments(bytes).into_iter().enumerate() {
        out.push(Pattern {
            label: format!("{label}/base64-{ix}"),
            class: class.clone(),
            bytes: encoded.into_bytes(),
            encoding: "base64",
        });
    }
    out
}

// -------------------------------------------------------------- relay driving

fn target_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    let exe = std::env::current_exe().expect("test exe path");
    exe.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .expect("target dir from test exe")
        .to_path_buf()
}

fn release_bin(name: &str) -> PathBuf {
    let path = target_dir().join("release").join(name);
    if !path.exists() {
        missing(&format!(
            "release relay binary not built: {} (cargo build --release --manifest-path tools/task-4817-relay/Cargo.toml)",
            path.display()
        ));
    }
    path
}

fn debug_bin(name: &str) -> PathBuf {
    let path = target_dir().join("debug").join(name);
    if !path.exists() {
        missing(&format!(
            "restart readback binary not built: {} (cargo build -p ipc --bin task-4817-readback)",
            path.display()
        ));
    }
    path
}

fn spawn_with_ready(mut command: Command, prefix: &str) -> (Child, u16) {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn relay process");
    let stdout = child.stdout.take().expect("child stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("ready line");
    assert!(
        line.starts_with(prefix),
        "unexpected ready line from child: {line}"
    );
    let port: u16 = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("port="))
        .and_then(|value| value.parse().ok())
        .expect("port in ready line");
    std::thread::spawn(move || {
        let mut sink = String::new();
        let _ = reader.read_to_string(&mut sink);
    });
    (child, port)
}

struct LiveRelay {
    relay: Child,
    tap: Child,
    tap_port: u16,
    data_dir: PathBuf,
    capture: PathBuf,
}

impl LiveRelay {
    fn start(root: &Path, name: &str) -> Self {
        let data_dir = root.join(format!("{name}-relay-data"));
        let capture = root.join(format!("{name}-capture.bin"));
        fs::create_dir_all(&data_dir).expect("relay data dir");

        let mut relay_cmd = Command::new(release_bin("task-4817-relay"));
        relay_cmd
            .arg("--port")
            .arg("0")
            .arg("--data-dir")
            .arg(&data_dir);
        let (relay, relay_port) = spawn_with_ready(relay_cmd, "RELAY_READY");

        let mut tap_cmd = Command::new(release_bin("task-4817-tap"));
        tap_cmd
            .arg("--listen")
            .arg("0")
            .arg("--upstream")
            .arg(relay_port.to_string())
            .arg("--capture")
            .arg(&capture);
        let (tap, tap_port) = spawn_with_ready(tap_cmd, "TAP_READY");

        LiveRelay {
            relay,
            tap,
            tap_port,
            data_dir,
            capture,
        }
    }

    fn stop(&mut self) {
        let _ = http(self.tap_port, "POST", "/relay/v1/admin/shutdown", Some("{}"));
        let _ = self.tap.kill();
        let _ = self.relay.kill();
        let _ = self.tap.wait();
        let _ = self.relay.wait();
    }
}

fn http(port: u16, method: &str, path: &str, body: Option<&str>) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect relay");
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("send request");
    stream.flush().expect("flush request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read relay response");
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default()
}

fn post_message(port: u16, channel: &str, wire: &str) -> String {
    let body = format!("{{\"content\":\"{wire}\",\"tts\":false}}");
    http(
        port,
        "POST",
        &format!("/relay/v1/channels/{channel}/messages"),
        Some(&body),
    )
}

fn fetch_messages(port: u16, channel: &str) -> Vec<String> {
    let body = http(
        port,
        "GET",
        &format!("/relay/v1/channels/{channel}/messages?after=0"),
        None,
    );
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("relay listing json");
    parsed
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row["content"].as_str().map(|s| s.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------------ devices

struct Device {
    label: &'static str,
    entropy: [u8; 16],
    identity: Identity,
    device_key: [u8; 32],
    dir: PathBuf,
}

impl Device {
    fn new(label: &'static str, account: &str, root: &Path) -> Self {
        let mut entropy = [0u8; 16];
        entropy.copy_from_slice(&crypto::random::random_bytes(16));
        let identity = identity_from_entropy(entropy, account.to_owned());
        let mut device_key = [0u8; 32];
        device_key.copy_from_slice(&crypto::random::random_bytes(32));
        let dir = root.join(format!("device-{label}"));
        fs::create_dir_all(&dir).expect("device dir");
        Device {
            label,
            entropy,
            identity,
            device_key,
            dir,
        }
    }

    fn recipient(&self) -> RecipientV3 {
        RecipientV3 {
            x25519_pub: self.identity.x25519_public,
            mlkem_pub: self.identity.mlkem_encapsulation_key(),
        }
    }
}

// ---------------------------------------------------------------- inventory

#[derive(Debug, Clone)]
struct SurfaceReport {
    name: &'static str,
    files: Vec<(PathBuf, u64)>,
    bytes_total: u64,
    bytes_scanned: u64,
    envelope_hits: usize,
    distinct_envelopes: usize,
    marked_value_hits: Vec<String>,
    field_name_hits: Vec<String>,
    pair_hits: Vec<String>,
    content_key_hits: Vec<String>,
    scanned: bool,
}

fn surface_files(data_dir: &Path, capture: &Path, name: &str) -> Vec<PathBuf> {
    let glob = |sub: &str, ext: &str| -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = fs::read_dir(data_dir.join(sub))
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                    .filter(|path| path.extension().map(|e| e == ext).unwrap_or(false))
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    };
    match name {
        "process_memory" => vec![data_dir.join("memory-dump.bin")],
        "request_bodies" => vec![data_dir.join("requests.log")],
        "response_bodies" => vec![data_dir.join("responses.log")],
        "queues" => vec![data_dir.join("queue.bin")],
        "databases" => vec![data_dir.join("relay.db")],
        "blobs" => glob("blobs", "blob"),
        "caches" => glob("cache", "cache"),
        "logs" => vec![data_dir.join("relay.log")],
        "telemetry" => vec![data_dir.join("telemetry.jsonl")],
        "crash_exports" => vec![data_dir.join("crash-export.bin")],
        "packet_capture" => vec![capture.to_path_buf()],
        other => panic!("unknown surface {other}"),
    }
}

const SURFACES: &[&str] = &[
    "process_memory",
    "request_bodies",
    "response_bodies",
    "queues",
    "databases",
    "blobs",
    "caches",
    "logs",
    "telemetry",
    "crash_exports",
    "packet_capture",
];

fn inventory(
    data_dir: &Path,
    capture: &Path,
    scanner: &Scanner,
    scan_content: bool,
    recovered: &mut Vec<String>,
) -> Vec<SurfaceReport> {
    let mut reports = Vec::new();
    for name in SURFACES {
        let files = surface_files(data_dir, capture, name);
        let mut sized = Vec::new();
        let mut bytes_total = 0u64;
        let mut bytes_scanned = 0u64;
        let mut envelope_hits = 0usize;
        let mut distinct = BTreeSet::new();
        let mut marked = Vec::new();
        let mut field_names = Vec::new();
        let mut pairs = Vec::new();
        let mut keys = Vec::new();
        for path in files {
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            sized.push((path.clone(), bytes.len() as u64));
            bytes_total += bytes.len() as u64;
            if !scan_content {
                continue;
            }
            bytes_scanned += bytes.len() as u64;
            for (pattern_ix, offset) in scanner.scan(&bytes) {
                let pattern = &scanner.patterns[pattern_ix];
                let location = format!(
                    "surface={name} file={} offset={offset} label={} encoding={}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    pattern.label,
                    pattern.encoding
                );
                match &pattern.class {
                    PatternClass::Envelope(ix) => {
                        envelope_hits += 1;
                        distinct.insert(*ix);
                    }
                    PatternClass::MarkedValue(kind) => {
                        marked.push(location.clone());
                        recovered.push(format!("{location} kind={kind} class=marked_value"));
                    }
                    PatternClass::FieldName(kind) => {
                        field_names.push(location.clone());
                        recovered.push(format!("{location} kind={kind} class=field_name"));
                    }
                    PatternClass::FieldValuePair(kind) => {
                        pairs.push(location.clone());
                        recovered.push(format!("{location} kind={kind} class=field_value_pair"));
                    }
                    PatternClass::ContentKey(which) => {
                        keys.push(location.clone());
                        recovered.push(format!("{location} key={which} class=content_key"));
                    }
                }
            }
        }
        reports.push(SurfaceReport {
            name,
            files: sized,
            bytes_total,
            bytes_scanned,
            envelope_hits,
            distinct_envelopes: distinct.len(),
            marked_value_hits: marked,
            field_name_hits: field_names,
            pair_hits: pairs,
            content_key_hits: keys,
            scanned: scan_content,
        });
    }
    reports
}

// ------------------------------------------------------- key auditor helpers

/// Re-derive the per-message content key straight from captured wire bytes,
/// independently of the sealing code, using the destination device's keys.
fn recover_body_key(wire: &str, receiver: &Identity) -> Option<[u8; 32]> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use crypto::{aes_gcm, hkdf, ml_kem_768, pqxdh, x25519};

    let raw = STANDARD.decode(wire.strip_prefix("DPC0::")?).ok()?;
    if raw.first().copied()? != wire_v2::WIRE_VERSION_V3 {
        return None;
    }
    let mut sender_bytes = [0u8; 32];
    sender_bytes.copy_from_slice(raw.get(2..34)?);
    let sender_ik = x25519::PublicKey::from_bytes(sender_bytes);
    let n = *raw.get(34)? as usize;
    let mlkem_sk = receiver.mlkem_decapsulation_key();
    let our_pub_hash = {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(receiver.x25519_public.as_bytes());
        digest[..8].to_vec()
    };
    for slot in 0..n {
        let base = 35 + slot * SLOT_V3_BYTES;
        if raw.get(base..base + 8)? != our_pub_hash.as_slice() {
            continue;
        }
        let mut cursor = base + 8;
        let mut ek_bytes = [0u8; 32];
        ek_bytes.copy_from_slice(raw.get(cursor..cursor + 32)?);
        cursor += 32;
        let ct_len = u16::from_le_bytes([raw[cursor], raw[cursor + 1]]) as usize;
        cursor += 2;
        let mut ct_bytes = [0u8; ml_kem_768::CIPHERTEXT_SIZE];
        ct_bytes.copy_from_slice(raw.get(cursor..cursor + ct_len)?);
        cursor += ct_len;
        let mut wrap_nonce = [0u8; aes_gcm::NONCE_SIZE];
        wrap_nonce.copy_from_slice(raw.get(cursor..cursor + aes_gcm::NONCE_SIZE)?);
        cursor += aes_gcm::NONCE_SIZE;
        let wrap_ct = raw.get(cursor..cursor + aes_gcm::KEY_SIZE + aes_gcm::TAG_SIZE)?;

        let handshake = pqxdh::InitiatorHandshake {
            ek_x25519_pub: x25519::PublicKey::from_bytes(ek_bytes),
            mlkem_ciphertext: ml_kem_768::Ciphertext::from_bytes(&ct_bytes),
            no_opk: true,
            opk_id: None,
        };
        let session = pqxdh::respond(
            &receiver.x25519_secret,
            &receiver.x25519_secret,
            None,
            &mlkem_sk,
            &sender_ik,
            &handshake,
        )
        .ok()?;
        let wrap_key_bytes =
            hkdf::derive_32(&[], session.as_bytes(), wire_v2::HKDF_INFO_WRAP_V3).ok()?;
        let wrap_key = aes_gcm::Key::from_bytes(wrap_key_bytes);
        let nonce = aes_gcm::Nonce::from_bytes(wrap_nonce);
        let k = aes_gcm::open(&wrap_key, &nonce, wire_v2::AD_WRAP_V3, wrap_ct).ok()?;
        let mut out = [0u8; 32];
        out.copy_from_slice(&k);
        return Some(out);
    }
    None
}

/// The body region a server sees: everything after the slots and body nonce.
fn body_ciphertext(wire: &str) -> Vec<u8> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").expect("prefix"))
        .expect("base64");
    let n = raw[34] as usize;
    let start = 35 + n * SLOT_V3_BYTES + 12;
    raw[start..].to_vec()
}

/// Replace the body ciphertext of a genuine envelope with plaintext, keeping
/// every server-visible byte of the header and the exact same length. This is
/// the throwaway mutation: an ordinary-looking envelope with a readable body.
fn plaintext_bodied_envelope(sealed_wire: &str, padded_plaintext: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    let mut raw = STANDARD
        .decode(sealed_wire.strip_prefix("DPC0::").expect("prefix"))
        .expect("base64");
    let n = raw[34] as usize;
    let start = 35 + n * SLOT_V3_BYTES + 12;
    let body_len = raw.len() - start;
    assert_eq!(
        body_len,
        padded_plaintext.len() + 16,
        "mutation must preserve the server-visible body length"
    );
    raw[start..start + padded_plaintext.len()].copy_from_slice(padded_plaintext);
    format!("DPC0::{}", STANDARD.encode(&raw))
}

/// Positive-control patterns for one sealed envelope. Both the head and the
/// middle of the base64 body are used: a surface that keeps only a prefix (the
/// telemetry stream) still has to show that it observed the traffic.
fn envelope_patterns(ix: usize, wire: &str) -> Vec<Pattern> {
    let body = wire.strip_prefix("DPC0::").expect("prefix");
    let mid = body.len() / 2;
    vec![
        Pattern {
            label: format!("envelope:{ix}/head"),
            class: PatternClass::Envelope(ix),
            bytes: body[..48].as_bytes().to_vec(),
            encoding: "raw",
        },
        Pattern {
            label: format!("envelope:{ix}/middle"),
            class: PatternClass::Envelope(ix),
            bytes: body[mid..mid + 48].as_bytes().to_vec(),
            encoding: "raw",
        },
    ]
}

fn ordinary_text_of_length(len: usize, seed: usize) -> String {
    const WORDS: &[&str] = &[
        "morning", "the", "bakery", "on", "fifth", "had", "the", "good", "bread", "again", "so",
        "I", "grabbed", "two", "loaves", "and", "walked", "home", "the", "long", "way", "past",
        "the", "canal", "where", "the", "herons", "stand", "all", "afternoon", "doing", "nothing",
        "at", "all", "which", "seems", "like", "a", "reasonable", "way", "to", "spend", "a", "day",
    ];
    let mut out = String::new();
    let mut ix = seed;
    while out.len() < len {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(WORDS[ix % WORDS.len()]);
        ix += 1;
    }
    out.truncate(len);
    out
}

// --------------------------------------------------------------------- check

#[test]
fn task_4817_sealed_sync_confidentiality_finish_line() {
    let starve = starve();
    println!("TASK4817_STARVE={starve:?}");

    let root = std::env::temp_dir().join(format!("task-4817-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("run root");

    // ---- 1. two paired shipping devices ---------------------------------
    let account = "task4817-account";
    let device_a = Device::new("a", account, &root);
    let device_b = Device::new("b", account, &root);
    let recipients = vec![device_a.recipient(), device_b.recipient()];
    println!(
        "TASK4817_PAIRED devices=2 account={account} a_ik={} b_ik={}",
        hex_lower(&device_a.identity.x25519_public.as_bytes()[..8]),
        hex_lower(&device_b.identity.x25519_public.as_bytes()[..8])
    );

    // The destination device persists its seed and the pinned sender identity
    // under its own device key, so the restart process starts from disk alone.
    let seed = serde_json::json!({
        "entropy_hex": hex_lower(&device_b.entropy),
        "user_id": account,
        "paired_sender_ik_hex": hex_lower(device_a.identity.x25519_public.as_bytes()),
    });
    fs::write(device_b.dir.join("device.key"), device_b.device_key).expect("device key");
    at_rest::write_record(
        &device_b.dir,
        &device_b.device_key,
        "identity",
        &serde_json::to_vec(&seed).expect("seed json"),
    )
    .expect("seal identity seed");

    // ---- 2. the allowed kinds and their marked values --------------------
    let allowed: Vec<&str> = sync_policy::allowed_sync_kinds().to_vec();
    let field_of: BTreeMap<&str, &str> = KIND_FIELDS.iter().copied().collect();
    for kind in &allowed {
        if !field_of.contains_key(kind) {
            missing(&format!(
                "allowed kind {kind} from the 4811 registry has no source field in this check"
            ));
        }
    }
    let mut marked: BTreeMap<String, (String, String)> = BTreeMap::new();
    for kind in &allowed {
        let entropy = crypto::random::random_bytes(32);
        let value = format!("OSL4817-{}", hex_lower(&entropy));
        marked.insert(
            (*kind).to_owned(),
            (field_of[kind].to_owned(), value.clone()),
        );
    }
    println!(
        "TASK4817_ALLOWED_KINDS={} MARKED_VALUES={} VALUE_ENTROPY_BITS=256",
        allowed.len(),
        marked.len()
    );

    // ---- 3. live release relay + recording tap ---------------------------
    let mut live = LiveRelay::start(&root, "main");
    println!(
        "TASK4817_RELAY tap_port={} data_dir={} tls=false",
        live.tap_port,
        live.data_dir.display()
    );

    // ---- 4. seal and send one value per allowed kind ---------------------
    let mut sealed_sync_wires: Vec<(String, String)> = Vec::new();
    let mut source_serializations: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut sync_padded_lens: Vec<usize> = Vec::new();
    let mut kinds_sent: Vec<String> = Vec::new();
    if starve != Starve::RefuseSync {
        for (counter, kind) in allowed.iter().enumerate() {
            if let Starve::AllowedKind(skipped) = &starve {
                if skipped == kind {
                    println!("TASK4817_STARVED_KIND={kind}");
                    continue;
                }
            }
            let (field, value) = marked[*kind].clone();
            let sync_value = SyncValue::new(
                (*kind).to_owned(),
                field,
                value,
                counter as u64 + 1,
                device_a.label.to_uppercase(),
            );
            let sealed = seal_allowed_sync_value(
                &sync_value,
                "A",
                "B",
                &device_a.identity.x25519_secret,
                &device_a.identity.x25519_public,
                &recipients,
            )
            .expect("seal allowed sync value");
            source_serializations.insert((*kind).to_owned(), sealed.source_serialization.clone());
            sync_padded_lens.push(sealed.padded_plaintext_len);
            post_message(live.tap_port, SYNC_CHANNEL, &sealed.wire);
            sealed_sync_wires.push(((*kind).to_owned(), sealed.wire));
            kinds_sent.push((*kind).to_owned());
        }
    }
    println!("TASK4817_SYNC_SENT={}", sealed_sync_wires.len());

    // ---- 5. ordinary-message control on the same conversation ------------
    let mut ordinary_wires: Vec<String> = Vec::new();
    if starve != Starve::OrdinaryControl {
        for (ix, padded_len) in sync_padded_lens.iter().enumerate() {
            // An ordinary user message whose padded plaintext lands in the same
            // bucket as the sync record it is the control for.
            let text = ordinary_text_of_length(padded_len.saturating_sub(8), ix);
            let sealed = seal_ordinary_message(
                &text,
                &device_a.identity.x25519_secret,
                &device_a.identity.x25519_public,
                &recipients,
            )
            .expect("seal ordinary message");
            post_message(live.tap_port, SYNC_CHANNEL, &sealed.wire);
            ordinary_wires.push(sealed.wire);
        }
    }
    println!("TASK4817_ORDINARY_CONTROL_SENT={}", ordinary_wires.len());

    // ---- 6. receiver opens, authenticates and merges ---------------------
    let relayed = fetch_messages(live.tap_port, SYNC_CHANNEL);
    println!("TASK4817_RELAY_RETURNED={}", relayed.len());
    let mlkem_b = device_b.identity.mlkem_decapsulation_key();
    let mut merged_state = FieldWiseState::default();
    let mut merged_fields = 0usize;
    let mut sync_bodies = 0usize;
    let mut ordinary_bodies = 0usize;
    for wire in &relayed {
        match open_sync_message(
            wire,
            &device_b.identity.x25519_secret,
            &mlkem_b,
            &device_a.identity.x25519_public,
        ) {
            Ok(authenticated) => {
                sync_bodies += 1;
                merged_fields +=
                    sealed_sync::merge_authenticated(&mut merged_state, &authenticated)
                        .expect("merge authenticated sync body");
            }
            Err(_) => ordinary_bodies += 1,
        }
    }
    println!(
        "TASK4817_RECEIVER sync_bodies={sync_bodies} ordinary_bodies={ordinary_bodies} merged_fields={merged_fields}"
    );
    at_rest::write_record(
        &device_b.dir,
        &device_b.device_key,
        "sync-state",
        &serde_json::to_vec(&merged_state).expect("state json"),
    )
    .expect("seal merged state");

    // ---- 7. authenticate-before-merge controls ---------------------------
    let mut tampered_rejected = 0usize;
    for (_, wire) in &sealed_sync_wires {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let mut raw = STANDARD
            .decode(wire.strip_prefix("DPC0::").expect("prefix"))
            .expect("base64");
        let last = raw.len() - 1;
        raw[last] ^= 0x01;
        let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
        if open_sync_message(
            &tampered,
            &device_b.identity.x25519_secret,
            &mlkem_b,
            &device_a.identity.x25519_public,
        )
        .is_err()
        {
            tampered_rejected += 1;
        }
    }
    let outsider = Device::new("outsider", "task4817-outsider", &root);
    let outsider_recipients = vec![outsider.recipient(), device_b.recipient()];
    let mut foreign_rejected = 0usize;
    for kind in &kinds_sent {
        let (field, value) = marked[kind].clone();
        let sync_value = SyncValue::new(kind.clone(), field, value, 99, "OUTSIDER");
        let sealed = seal_allowed_sync_value(
            &sync_value,
            "OUTSIDER",
            "B",
            &outsider.identity.x25519_secret,
            &outsider.identity.x25519_public,
            &outsider_recipients,
        )
        .expect("seal from outsider");
        if open_sync_message(
            &sealed.wire,
            &device_b.identity.x25519_secret,
            &mlkem_b,
            &device_a.identity.x25519_public,
        )
        .is_err()
        {
            foreign_rejected += 1;
        }
    }
    let plaintext_merge_entry_points = SEALED_SYNC_SOURCE
        .matches("pub fn merge_")
        .count()
        .saturating_sub(
            SEALED_SYNC_SOURCE
                .matches("authenticated: &AuthenticatedSyncBody")
                .count(),
        );
    println!(
        "TASK4817_AUTH_BEFORE_MERGE tampered_rejected={tampered_rejected} foreign_sender_rejected={foreign_rejected} plaintext_merge_entry_points={plaintext_merge_entry_points}"
    );

    // ---- 8. restart the destination device in a separate process ---------
    let mut restart_decrypted: BTreeMap<String, String> = BTreeMap::new();
    let mut restart_persisted: BTreeMap<String, String> = BTreeMap::new();
    if starve != Starve::ReceiverReadback {
        let output = Command::new(debug_bin("task-4817-readback"))
            .arg("--dir")
            .arg(&device_b.dir)
            .arg("--relay-port")
            .arg(live.tap_port.to_string())
            .arg("--channel")
            .arg(SYNC_CHANNEL)
            .output()
            .expect("run restart readback");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.status.success() {
            println!("{}", String::from_utf8_lossy(&output.stderr));
            missing("restart readback process failed");
        }
        for line in stdout.lines() {
            if let Some(rest) = line.strip_prefix("RESTART_DECRYPTED ") {
                let mut kind = String::new();
                let mut value = String::new();
                for field in rest.split(' ') {
                    if let Some(v) = field.strip_prefix("kind=") {
                        kind = v.to_owned();
                    }
                    if let Some(v) = field.strip_prefix("value=") {
                        value = v.to_owned();
                    }
                }
                restart_decrypted.insert(kind, value);
            }
            if let Some(rest) = line.strip_prefix("RESTART_PERSISTED ") {
                let mut field = String::new();
                let mut value = String::new();
                for part in rest.split(' ') {
                    if let Some(v) = part.strip_prefix("field=") {
                        field = v.to_owned();
                    }
                    if let Some(v) = part.strip_prefix("value=") {
                        value = v.to_owned();
                    }
                }
                restart_persisted.insert(field, value);
            }
        }
        println!(
            "TASK4817_RESTART decrypted={} persisted={}",
            restart_decrypted.len(),
            restart_persisted.len()
        );
    } else {
        println!("TASK4817_STARVED_RECEIVER_READBACK=1");
    }

    // ---- 9. content-key auditor, straight off the captured bytes ---------
    let mut body_keys: Vec<[u8; 32]> = Vec::new();
    let mut key_pattern_sources: Vec<(String, Vec<u8>)> = Vec::new();
    for (ix, (_, wire)) in sealed_sync_wires.iter().enumerate() {
        if let Some(key) = recover_body_key(wire, &device_b.identity) {
            body_keys.push(key);
            key_pattern_sources.push((format!("body_key_{ix}"), key.to_vec()));
        }
    }
    for (ix, wire) in ordinary_wires.iter().enumerate() {
        if let Some(key) = recover_body_key(wire, &device_b.identity) {
            body_keys.push(key);
            key_pattern_sources.push((format!("ordinary_body_key_{ix}"), key.to_vec()));
        }
    }
    key_pattern_sources.push((
        "receiver_device_secret".to_owned(),
        device_b.identity.x25519_secret.as_bytes().to_vec(),
    ));
    key_pattern_sources.push((
        "sender_device_secret".to_owned(),
        device_a.identity.x25519_secret.as_bytes().to_vec(),
    ));
    key_pattern_sources.push((
        "receiver_at_rest_key".to_owned(),
        device_b.device_key.to_vec(),
    ));
    let distinct_body_keys: BTreeSet<Vec<u8>> =
        body_keys.iter().map(|key| key.to_vec()).collect();
    println!(
        "TASK4817_CONTENT_KEYS recovered={} distinct={} reused={}",
        body_keys.len(),
        distinct_body_keys.len(),
        body_keys.len() - distinct_body_keys.len()
    );

    // ---- 10. non-key holder and wrong device key -------------------------
    let mut attacker_attempts = 0usize;
    let mut attacker_opened_bytes = 0usize;
    if starve != Starve::WrongKey {
        let mut wrong_entropy = [0u8; 16];
        wrong_entropy.copy_from_slice(&crypto::random::random_bytes(16));
        let wrong_device = identity_from_entropy(wrong_entropy, account.to_owned());
        let attackers: Vec<(&str, &Identity)> = vec![
            ("independent_non_key_holder", &outsider.identity),
            ("wrong_device_key_same_account", &wrong_device),
        ];
        let all_wires: Vec<String> = sealed_sync_wires
            .iter()
            .map(|(_, wire)| wire.clone())
            .chain(ordinary_wires.iter().cloned())
            .collect();
        for (label, attacker) in &attackers {
            let mlkem = attacker.mlkem_decapsulation_key();
            let mut opened = 0usize;
            for wire in &all_wires {
                attacker_attempts += 1;
                if let Ok(plain) = wire_v2::decrypt_v3(wire, &attacker.x25519_secret, &mlkem) {
                    opened += plain.plaintext.len();
                }
                if recover_body_key(wire, attacker).is_some() {
                    opened += 32;
                }
            }
            println!(
                "TASK4817_ATTACKER role={label} attempts={} opened_bytes={opened}",
                all_wires.len()
            );
            attacker_opened_bytes += opened;
        }
    } else {
        println!("TASK4817_STARVED_WRONG_KEY=1");
    }

    // ---- 11. envelope parity --------------------------------------------
    let describe = |wire: &str| -> (u8, u8, u8, usize, usize) {
        let header = sealed_sync::server_visible_header(wire).expect("server visible header");
        let class = sealed_sync::server_visible_size_class(wire).expect("size class");
        (
            header.wire_version,
            header.message_kind,
            header.recipient_count,
            header.header_bytes,
            class,
        )
    };
    let sync_shapes: Vec<(u8, u8, u8, usize, usize)> = sealed_sync_wires
        .iter()
        .map(|(_, wire)| describe(wire))
        .collect();
    let ordinary_shapes: Vec<(u8, u8, u8, usize, usize)> =
        ordinary_wires.iter().map(|wire| describe(wire)).collect();
    let histogram = |shapes: &[(u8, u8, u8, usize, usize)]| -> BTreeMap<usize, usize> {
        let mut out = BTreeMap::new();
        for shape in shapes {
            *out.entry(shape.4).or_insert(0) += 1;
        }
        out
    };
    let sync_hist = histogram(&sync_shapes);
    let ordinary_hist = histogram(&ordinary_shapes);
    let sync_attrs: BTreeSet<(u8, u8, u8, usize)> = sync_shapes
        .iter()
        .map(|s| (s.0, s.1, s.2, s.3))
        .collect();
    let ordinary_attrs: BTreeSet<(u8, u8, u8, usize)> = ordinary_shapes
        .iter()
        .map(|s| (s.0, s.1, s.2, s.3))
        .collect();
    println!(
        "TASK4817_ENVELOPE sync_attrs={sync_attrs:?} ordinary_attrs={ordinary_attrs:?} sync_size_classes={sync_hist:?} ordinary_size_classes={ordinary_hist:?} added_server_visible_kinds={}",
        ADDED_SERVER_VISIBLE_MESSAGE_KINDS.len()
    );

    // Captured bodies must not be the source serialization.
    let mut body_equal_to_source = 0usize;
    let mut shared_runs = 0usize;
    for (kind, wire) in &sealed_sync_wires {
        let body = body_ciphertext(wire);
        let source = &source_serializations[kind];
        if body == *source {
            body_equal_to_source += 1;
        }
        for window in source.windows(8) {
            if body.windows(8).any(|candidate| candidate == window) {
                shared_runs += 1;
            }
        }
    }
    println!(
        "TASK4817_BODY_VS_SOURCE equal={body_equal_to_source} shared_8byte_runs={shared_runs}"
    );

    // ---- 12. build the scanner and take the inventory --------------------
    let mut patterns: Vec<Pattern> = Vec::new();
    for (kind, (field, value)) in &marked {
        patterns.extend(string_patterns(
            &format!("value:{kind}"),
            PatternClass::MarkedValue(kind.clone()),
            value,
        ));
        patterns.extend(string_patterns(
            &format!("field:{kind}"),
            PatternClass::FieldName(kind.clone()),
            field,
        ));
        patterns.extend(string_patterns(
            &format!("pair:{kind}"),
            PatternClass::FieldValuePair(kind.clone()),
            &format!("\"{field}\":\"{value}\""),
        ));
    }
    for (label, bytes) in &key_pattern_sources {
        patterns.extend(key_patterns(label, bytes));
    }
    for (ix, (_, wire)) in sealed_sync_wires.iter().enumerate() {
        patterns.extend(envelope_patterns(ix, wire));
    }
    println!("TASK4817_SCANNER patterns={}", patterns.len());
    let scanner = Scanner::new(patterns);

    http(live.tap_port, "POST", "/relay/v1/admin/dump", Some("{}"));
    if let Starve::RelaySurface(name) = &starve {
        for path in surface_files(&live.data_dir, &live.capture, name) {
            let _ = fs::remove_file(&path);
        }
        println!("TASK4817_STARVED_SURFACE={name}");
    }
    let scan_content = !matches!(starve, Starve::TlsOnly | Starve::EnvelopeShapeOnly);
    let justification = match starve {
        Starve::TlsOnly => "transport_tls_only",
        Starve::EnvelopeShapeOnly => "envelope_shape_only",
        _ => "sealed_body_byte_inventory",
    };
    let mut recovered: Vec<String> = Vec::new();
    let reports = inventory(
        &live.data_dir,
        &live.capture,
        &scanner,
        scan_content,
        &mut recovered,
    );

    let mut bytes_inventoried = 0u64;
    let mut bytes_unscanned = 0u64;
    let mut empty_surfaces: Vec<&str> = Vec::new();
    let mut blind_surfaces: Vec<&str> = Vec::new();
    for report in &reports {
        bytes_inventoried += report.bytes_total;
        bytes_unscanned += report.bytes_total - report.bytes_scanned;
        if report.bytes_total == 0 {
            empty_surfaces.push(report.name);
        }
        if report.scanned && report.distinct_envelopes == 0 {
            blind_surfaces.push(report.name);
        }
        println!(
            "TASK4817_SURFACE name={} files={} bytes_total={} bytes_scanned={} distinct_envelopes={} envelope_hits={} marked_value_hits={} field_name_hits={} pair_hits={} content_key_hits={}",
            report.name,
            report.files.len(),
            report.bytes_total,
            report.bytes_scanned,
            report.distinct_envelopes,
            report.envelope_hits,
            report.marked_value_hits.len(),
            report.field_name_hits.len(),
            report.pair_hits.len(),
            report.content_key_hits.len()
        );
    }
    let marked_total: usize = reports.iter().map(|r| r.marked_value_hits.len()).sum();
    let field_total: usize = reports.iter().map(|r| r.field_name_hits.len()).sum();
    let pair_total: usize = reports.iter().map(|r| r.pair_hits.len()).sum();
    let key_total: usize = reports.iter().map(|r| r.content_key_hits.len()).sum();
    println!(
        "TASK4817_INVENTORY surfaces={} nonempty={} bytes_inventoried={bytes_inventoried} bytes_unscanned={bytes_unscanned} marked_plaintext={marked_total} field_names={field_total} field_value_pairs={pair_total} content_keys={key_total} justification={justification}",
        reports.len(),
        reports.len() - empty_surfaces.len()
    );
    for location in &recovered {
        println!("TASK4817_RECOVERED {location}");
    }

    // ---- 13. the throwaway plaintext mutation, through a live relay ------
    let mut mutation_recovered_kinds: BTreeSet<String> = BTreeSet::new();
    let mut mutation_recovered_surfaces: BTreeSet<String> = BTreeSet::new();
    let mut mutation_locations: Vec<String> = Vec::new();
    let mut mutation_merge_matches = 0usize;
    let mut mutation_shape_matches = 0usize;
    if starve != Starve::PlaintextMutation {
        let mut mutant = LiveRelay::start(&root, "mutation");
        let mut canaries: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut mutant_patterns: Vec<Pattern> = Vec::new();
        let mut mutant_state = FieldWiseState::default();
        let mut mutant_wires: Vec<String> = Vec::new();
        for (counter, kind) in allowed.iter().enumerate() {
            let entropy = crypto::random::random_bytes(32);
            let value = format!("OSL4817M-{}", hex_lower(&entropy));
            let field = field_of[kind].to_owned();
            canaries.insert((*kind).to_owned(), (field.clone(), value.clone()));
            let sync_value = SyncValue::new(
                (*kind).to_owned(),
                field.clone(),
                value.clone(),
                counter as u64 + 1,
                "A",
            );
            let sealed = seal_allowed_sync_value(
                &sync_value,
                "A",
                "B",
                &device_a.identity.x25519_secret,
                &device_a.identity.x25519_public,
                &recipients,
            )
            .expect("seal canary");
            let payload = sealed_sync::sync_body_for(&sync_value, "A", "B");
            let body = sealed_sync::serialize_sync_body(&payload).expect("serialize canary");
            let padded = crypto::padding::pad_text(&body).expect("pad canary");
            let mutated = plaintext_bodied_envelope(&sealed.wire, &padded);

            // The mutation is only interesting if it is invisible from the
            // outside and still merges: both are checked here.
            if sealed_sync::server_visible_header(&mutated).ok()
                == sealed_sync::server_visible_header(&sealed.wire).ok()
                && sealed_sync::server_visible_size_class(&mutated).ok()
                    == sealed_sync::server_visible_size_class(&sealed.wire).ok()
            {
                mutation_shape_matches += 1;
            }
            let recovered_body = body_ciphertext(&mutated);
            let unpadded = crypto::padding::unpad_text(&recovered_body[..padded.len()])
                .expect("unpad mutated body");
            let parsed: serde_json::Value =
                serde_json::from_slice(&unpadded).expect("mutated body json");
            let merged_value = parsed["body"]["fields"][&field].clone();
            mutant_state.set_field(
                field.clone(),
                merged_value.clone(),
                VersionStamp::new(counter as u64 + 1, "A"),
            );
            if merged_value.as_str() == Some(value.as_str()) {
                mutation_merge_matches += 1;
            }

            post_message(mutant.tap_port, MUTATION_CHANNEL, &mutated);
            mutant_wires.push(mutated);

            mutant_patterns.extend(string_patterns(
                &format!("value:{kind}"),
                PatternClass::MarkedValue((*kind).to_owned()),
                &value,
            ));
            mutant_patterns.extend(string_patterns(
                &format!("field:{kind}"),
                PatternClass::FieldName((*kind).to_owned()),
                &field,
            ));
            mutant_patterns.extend(string_patterns(
                &format!("pair:{kind}"),
                PatternClass::FieldValuePair((*kind).to_owned()),
                &format!("\"{field}\":\"{value}\""),
            ));
        }
        for (ix, wire) in mutant_wires.iter().enumerate() {
            mutant_patterns.extend(envelope_patterns(ix, wire));
        }
        http(mutant.tap_port, "POST", "/relay/v1/admin/dump", Some("{}"));
        let mutant_scanner = Scanner::new(mutant_patterns);
        let mut mutant_recovered: Vec<String> = Vec::new();
        let mutant_reports = inventory(
            &mutant.data_dir,
            &mutant.capture,
            &mutant_scanner,
            true,
            &mut mutant_recovered,
        );
        for report in &mutant_reports {
            let hits = report.marked_value_hits.len()
                + report.field_name_hits.len()
                + report.pair_hits.len();
            if hits > 0 {
                mutation_recovered_surfaces.insert(report.name.to_owned());
            }
            println!(
                "TASK4817_MUTATION_SURFACE name={} bytes_total={} marked_value_hits={} field_name_hits={} pair_hits={}",
                report.name,
                report.bytes_total,
                report.marked_value_hits.len(),
                report.field_name_hits.len(),
                report.pair_hits.len()
            );
        }
        for location in &mutant_recovered {
            if let Some(kind) = location
                .split(' ')
                .find_map(|field| field.strip_prefix("kind="))
            {
                mutation_recovered_kinds.insert(kind.to_owned());
            }
            mutation_locations.push(location.clone());
        }
        println!(
            "TASK4817_MUTATION merged_fields={} merge_matches={mutation_merge_matches} shape_matches={mutation_shape_matches} recovered_kinds={} recovered_surfaces={} recovered_locations={}",
            mutant_state.field_count(),
            mutation_recovered_kinds.len(),
            mutation_recovered_surfaces.len(),
            mutation_locations.len()
        );
        for location in &mutation_locations {
            println!("TASK4817_MUTATION_RECOVERED {location}");
        }
        mutant.stop();
    } else {
        println!("TASK4817_STARVED_PLAINTEXT_MUTATION=1");
    }

    live.stop();

    // ---- 14. the finish line ---------------------------------------------
    if justification != "sealed_body_byte_inventory" {
        missing(&format!(
            "confidentiality justified by {justification}: no byte inventory of relay surfaces was taken"
        ));
    }
    if kinds_sent.is_empty() {
        missing("sync was permanently refused: 0 allowed kinds reached the relay, which proves nothing about confidentiality");
    }
    for kind in &allowed {
        if !kinds_sent.iter().any(|sent| sent == kind) {
            missing(&format!(
                "allowed kind {kind} was never synced: no confidentiality proof for it"
            ));
        }
        let (_, value) = &marked[*kind];
        match restart_decrypted.get(*kind) {
            Some(decrypted) if decrypted == value => {}
            Some(other) => missing(&format!(
                "restart readback for {kind} decrypted {other} not the source value"
            )),
            None => missing(&format!(
                "receiver decrypt/readback after restart missing for allowed kind {kind}"
            )),
        }
        let field = field_of[kind.to_owned()];
        match restart_persisted.get(field) {
            Some(persisted) if persisted == value => {}
            _ => missing(&format!(
                "restart readback of the sealed at-rest state missing allowed kind {kind}"
            )),
        }
    }
    if merged_fields != allowed.len() {
        missing(&format!(
            "merged {merged_fields} fields for {} allowed kinds",
            allowed.len()
        ));
    }
    if !empty_surfaces.is_empty() {
        missing(&format!(
            "relay surface inventory is empty for {empty_surfaces:?}: that surface was never captured"
        ));
    }
    if !blind_surfaces.is_empty() {
        missing(&format!(
            "relay surfaces {blind_surfaces:?} hold no sealed envelope bytes: the inventory did not observe the sync traffic there"
        ));
    }
    if bytes_unscanned != 0 {
        missing(&format!(
            "{bytes_unscanned} inventoried bytes were never scanned: the inventory does not reconcile"
        ));
    }
    if !recovered.is_empty() {
        println!(
            "TASK4817_RECOVERED_TOTAL={} on relay surfaces",
            recovered.len()
        );
        missing(&format!(
            "{} marked plaintext/field/key byte locations recovered from relay surfaces (listed above as TASK4817_RECOVERED)",
            recovered.len()
        ));
    }
    if ordinary_wires.len() != sealed_sync_wires.len() || ordinary_wires.is_empty() {
        missing("ordinary-message control missing: sync envelopes have nothing to be compared against");
    }
    if sync_attrs != ordinary_attrs || sync_hist != ordinary_hist {
        missing(&format!(
            "server-visible distribution differs: sync {sync_attrs:?}/{sync_hist:?} ordinary {ordinary_attrs:?}/{ordinary_hist:?}"
        ));
    }
    if attacker_attempts == 0 {
        missing("no wrong-key or non-key-holder attempt was made");
    }
    if attacker_opened_bytes != 0 {
        missing(&format!(
            "a non-key holder or wrong device key opened {attacker_opened_bytes} bytes"
        ));
    }
    if body_equal_to_source != 0 || shared_runs != 0 {
        missing(&format!(
            "captured body matches the source serialization: equal={body_equal_to_source} shared_8byte_runs={shared_runs}"
        ));
    }
    if tampered_rejected != sealed_sync_wires.len() || foreign_rejected != kinds_sent.len() {
        missing(&format!(
            "authenticate-before-merge incomplete: tampered_rejected={tampered_rejected} foreign_rejected={foreign_rejected}"
        ));
    }
    if plaintext_merge_entry_points != 0 {
        missing("a merge entry point exists that does not require an authenticated body");
    }
    if body_keys.len() != sealed_sync_wires.len() + ordinary_wires.len()
        || distinct_body_keys.len() != body_keys.len()
    {
        missing(&format!(
            "content keys are reusable or unaccounted: recovered={} distinct={}",
            body_keys.len(),
            distinct_body_keys.len()
        ));
    }
    if mutation_locations.is_empty() {
        missing("the throwaway plaintext mutation was not exercised through the live relay, so the byte scanner is unproven");
    }
    if mutation_recovered_kinds.len() != allowed.len()
        || mutation_merge_matches != allowed.len()
        || mutation_shape_matches != allowed.len()
    {
        missing(&format!(
            "the plaintext mutation did not behave as a red control: recovered_kinds={} merge_matches={mutation_merge_matches} shape_matches={mutation_shape_matches}",
            mutation_recovered_kinds.len()
        ));
    }

    println!("TASK4817_RESULT=pass");
}
