//! `osl-chats-service` — the deployed OSL Chats authorization service.
//!
//! This is the process an installed OSL Chats client talks to. It owns the
//! roster, the channels, the threads and the message ciphertext, it verifies
//! that every request was signed by an installed identity, and it consults
//! `ipc::chats_service_authority` before it emits a byte. It holds no client
//! state and shares no memory with any client: a client reaches it only over
//! TCP, and an auditor reads its decisions only from the journal it writes.
//!
//!   osl-chats-service --data-dir DIR --fixture FILE --ready-file FILE
//!                     [--build-tag TAG] [--port N]
//!
//! On bind it writes the listening port to `--ready-file`. Every handled
//! request appends one line to `DIR/audit.jsonl` and rewrites `DIR/rights.json`.
//! Neither file is ever served to a client.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::Engine as _;
use ipc::chats_service_authority::{
    fingerprint_of, signing_payload, ChatsAuthority, ChatsFixture, REFUSAL_SENTENCE,
    REFUSE_BAD_SIGNATURE, REFUSE_REPLAYED_NONCE, REFUSE_UNKNOWN_IDENTITY,
};

fn main() -> ExitCode {
    let mut data_dir: Option<PathBuf> = None;
    let mut fixture_path: Option<PathBuf> = None;
    let mut ready_file: Option<PathBuf> = None;
    let mut build_tag = "production".to_owned();
    let mut port: u16 = 0;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match arg.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(value())),
            "--fixture" => fixture_path = Some(PathBuf::from(value())),
            "--ready-file" => ready_file = Some(PathBuf::from(value())),
            "--build-tag" => build_tag = value(),
            "--port" => port = value().parse().unwrap_or(0),
            other => {
                eprintln!("osl-chats-service: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    let (data_dir, fixture_path, ready_file) = match (data_dir, fixture_path, ready_file) {
        (Some(data), Some(fixture), Some(ready)) => (data, fixture, ready),
        _ => {
            eprintln!("osl-chats-service: --data-dir, --fixture and --ready-file are required");
            return ExitCode::from(2);
        }
    };

    if let Err(error) = std::fs::create_dir_all(&data_dir) {
        eprintln!("osl-chats-service: cannot make {}: {error}", data_dir.display());
        return ExitCode::from(2);
    }

    let fixture_bytes = match std::fs::read(&fixture_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("osl-chats-service: cannot read fixture: {error}");
            return ExitCode::from(2);
        }
    };
    let fixture: ChatsFixture = match serde_json::from_slice(&fixture_bytes) {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("osl-chats-service: bad fixture: {error}");
            return ExitCode::from(2);
        }
    };
    let mut authority = match ChatsAuthority::seed(&build_tag, &fixture) {
        Ok(authority) => authority,
        Err(error) => {
            eprintln!("osl-chats-service: cannot seed: {error}");
            return ExitCode::from(2);
        }
    };

    let audit_path = data_dir.join("audit.jsonl");
    let rights_path = data_dir.join("rights.json");
    let _ = File::create(&audit_path);
    write_rights(&rights_path, &authority);

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("osl-chats-service: cannot bind: {error}");
            return ExitCode::from(2);
        }
    };
    let bound = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(error) => {
            eprintln!("osl-chats-service: cannot read port: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = std::fs::write(&ready_file, format!("{bound}\n")) {
        eprintln!("osl-chats-service: cannot write ready file: {error}");
        return ExitCode::from(2);
    }
    println!("TASK6120SERVICE listening port={bound} tag={build_tag} enclave={}", authority.enclave_id());
    let _ = std::io::stdout().flush();

    let mut seen_nonces: BTreeSet<String> = BTreeSet::new();
    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let request = match read_request(&mut stream) {
            Some(request) => request,
            None => continue,
        };
        if request.method == "GET" && request.path == "/healthz" {
            let body = format!("{{\"ok\":true,\"tag\":\"{build_tag}\"}}");
            respond(&mut stream, 200, body.as_bytes());
            continue;
        }
        if request.method == "POST" && request.path == "/admin/quit" {
            respond(&mut stream, 200, b"{\"ok\":true}");
            break;
        }
        let op = match request.path.strip_prefix("/v1/chats/") {
            Some(op) if request.method == "POST" => op.to_owned(),
            _ => {
                respond(&mut stream, 404, b"{\"ok\":false,\"code\":\"unknown-route\"}");
                continue;
            }
        };

        let key_b64 = request.header("x-osl-key");
        let sig_b64 = request.header("x-osl-sig");
        let nonce = request.header("x-osl-nonce");

        let before = authority.rights_snapshot();
        let identity = authority.identity_for_key(&key_b64).cloned();
        let (label, person, fingerprint) = match &identity {
            Some(identity) => (
                identity.label.clone(),
                identity.person_name.clone(),
                identity.fingerprint.clone(),
            ),
            None => (
                "unknown".to_owned(),
                String::new(),
                fingerprint_of(&key_b64).unwrap_or_else(|_| "invalid".to_owned()),
            ),
        };

        let mut outcome = if identity.is_none() {
            authority.audit_only(label, person, fingerprint, &op, REFUSE_UNKNOWN_IDENTITY, 403)
        } else if !nonce.is_empty() && !seen_nonces.insert(nonce.clone()) {
            authority.audit_only(label, person, fingerprint, &op, REFUSE_REPLAYED_NONCE, 401)
        } else if !signature_holds(&key_b64, &sig_b64, &op, &nonce, &request.body) {
            authority.audit_only(label, person, fingerprint, &op, REFUSE_BAD_SIGNATURE, 401)
        } else {
            authority.handle(&key_b64, &op, &request.body)
        };

        if let Some(previous) = before.get(&outcome.audit.rights_subject) {
            outcome.audit.rights_before = previous.clone();
        }
        append_audit(&audit_path, &outcome.audit);
        write_rights(&rights_path, &authority);
        respond(&mut stream, outcome.status, &outcome.body);
    }
    ExitCode::SUCCESS
}

fn signature_holds(key_b64: &str, sig_b64: &str, op: &str, nonce: &str, body: &[u8]) -> bool {
    let engine = base64::engine::general_purpose::STANDARD;
    let key_bytes = match engine.decode(key_b64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let sig_bytes = match engine.decode(sig_b64) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let key: [u8; crypto::ed25519::PUBLIC_KEY_SIZE] = match key_bytes.try_into() {
        Ok(key) => key,
        Err(_) => return false,
    };
    let signature: [u8; crypto::ed25519::SIGNATURE_SIZE] = match sig_bytes.try_into() {
        Ok(signature) => signature,
        Err(_) => return false,
    };
    let public = crypto::ed25519::PublicKey::from_bytes(key);
    let signature = crypto::ed25519::Signature::from_bytes(signature);
    crypto::ed25519::verify(&public, &signing_payload(op, nonce, body), &signature).unwrap_or(false)
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> String {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut start = String::new();
    if reader.read_line(&mut start).ok()? == 0 {
        return None;
    }
    let mut parts = start.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();

    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if name == "content-length" {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: u16, body: &[u8]) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn append_audit(path: &Path, entry: &ipc::chats_service_authority::AuditEntry) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(line) = serde_json::to_string(entry) {
            let _ = writeln!(file, "{line}");
        }
    }
}

fn write_rights(path: &Path, authority: &ChatsAuthority) {
    if let Ok(text) = serde_json::to_string_pretty(&authority.rights_snapshot()) {
        let _ = std::fs::write(path, text);
    }
}

// Keeps the refusal sentence referenced from the service binary so a change to
// the wording is a change to this deployed surface, not only to the library.
#[allow(dead_code)]
const _REFUSAL_SENTENCE: &str = REFUSAL_SENTENCE;
