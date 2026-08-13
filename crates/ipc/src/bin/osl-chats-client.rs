//! `osl-chats-client` — an installed OSL Chats client.
//!
//! One install directory is one client. `--generate` mints that install's own
//! Ed25519 identity and never emits the secret; every later invocation loads
//! the secret from its own install directory, signs the request, and speaks to
//! the deployed service over TCP. Nothing is shared between installs, and
//! nothing is shared with the service beyond the public key it registered.
//!
//!   osl-chats-client --generate --install-dir DIR --label L --person "Name"
//!   osl-chats-client --install-dir DIR --base URL --op OP --nonce N
//!                    --body FILE --out FILE [--forge-public B64] [--tamper]
//!
//! `--out` receives the raw response bytes, status line and headers included,
//! exactly as they came off the socket. Nothing filters them: what the checker
//! scans is what the hostile client actually received.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::ExitCode;

use base64::Engine as _;
use ipc::chats_service_authority::{fingerprint_of, signing_payload};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledIdentity {
    label: String,
    person_name: String,
    secret_hex: String,
    public_key_b64: String,
}

fn main() -> ExitCode {
    let mut generate = false;
    let mut install_dir: Option<PathBuf> = None;
    let mut label = String::new();
    let mut person = String::new();
    let mut base = String::new();
    let mut op = String::new();
    let mut nonce = String::new();
    let mut body_path: Option<PathBuf> = None;
    let mut out_path: Option<PathBuf> = None;
    let mut forge_public: Option<String> = None;
    let mut path_override: Option<String> = None;
    let mut tamper = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--generate" => generate = true,
            "--tamper" => tamper = true,
            "--install-dir" => install_dir = Some(PathBuf::from(args.next().unwrap_or_default())),
            "--label" => label = args.next().unwrap_or_default(),
            "--person" => person = args.next().unwrap_or_default(),
            "--base" => base = args.next().unwrap_or_default(),
            "--op" => op = args.next().unwrap_or_default(),
            "--nonce" => nonce = args.next().unwrap_or_default(),
            "--body" => body_path = Some(PathBuf::from(args.next().unwrap_or_default())),
            "--out" => out_path = Some(PathBuf::from(args.next().unwrap_or_default())),
            "--forge-public" => forge_public = Some(args.next().unwrap_or_default()),
            // Reaches a path the manifest does not classify, so the check can
            // ask the deployed router what it does with an unknown route.
            "--path" => path_override = Some(args.next().unwrap_or_default()),
            other => {
                eprintln!("osl-chats-client: unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }

    let Some(install_dir) = install_dir else {
        eprintln!("osl-chats-client: --install-dir is required");
        return ExitCode::from(2);
    };

    if generate {
        if let Err(error) = std::fs::create_dir_all(&install_dir) {
            eprintln!("osl-chats-client: cannot make install dir: {error}");
            return ExitCode::from(2);
        }
        let (secret, public) = crypto::ed25519::generate_keypair();
        let public_key_b64 = base64::engine::general_purpose::STANDARD.encode(public.as_bytes());
        let identity = InstalledIdentity {
            label: label.clone(),
            person_name: person.clone(),
            secret_hex: hex::encode(secret.as_bytes()),
            public_key_b64: public_key_b64.clone(),
        };
        let text = match serde_json::to_string_pretty(&identity) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("osl-chats-client: cannot write identity: {error}");
                return ExitCode::from(2);
            }
        };
        if let Err(error) = std::fs::write(install_dir.join("identity.json"), text) {
            eprintln!("osl-chats-client: cannot write identity: {error}");
            return ExitCode::from(2);
        }
        // Installing a client installs its route manifest: every endpoint this
        // client can reach, what parent identifiers it must carry, whose
        // authorship it may claim, and which roles are meant to be allowed.
        let manifest = ipc::chats_route_manifest::manifest_json();
        if let Err(error) = std::fs::write(install_dir.join("route-manifest.json"), &manifest) {
            eprintln!("osl-chats-client: cannot write route manifest: {error}");
            return ExitCode::from(2);
        }
        let fingerprint = fingerprint_of(&public_key_b64).unwrap_or_default();
        println!("TASK6120CLIENT generated label={label} person={person} public={public_key_b64} fp={fingerprint}");
        println!(
            "TASK6206CLIENT manifest label={label} routes={} bytes={}",
            ipc::chats_route_manifest::CLIENT_ROUTES.len(),
            manifest.len()
        );
        return ExitCode::SUCCESS;
    }

    let identity: InstalledIdentity = match std::fs::read(install_dir.join("identity.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(identity) => identity,
        None => {
            eprintln!("osl-chats-client: no installed identity in {}", install_dir.display());
            return ExitCode::from(2);
        }
    };

    let body = match body_path {
        Some(path) => match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("osl-chats-client: cannot read body: {error}");
                return ExitCode::from(2);
            }
        },
        None => b"{}".to_vec(),
    };

    let secret_bytes: [u8; crypto::ed25519::SECRET_KEY_SIZE] =
        match hex::decode(&identity.secret_hex).ok().and_then(|bytes| bytes.try_into().ok()) {
            Some(bytes) => bytes,
            None => {
                eprintln!("osl-chats-client: install secret is unreadable");
                return ExitCode::from(2);
            }
        };
    let secret = crypto::ed25519::SecretKey::from_bytes(secret_bytes);

    // The signature always covers the bytes this client believes it is
    // sending. `--tamper` changes the bytes afterwards, which is what a
    // request-rewriting attacker does.
    let signature = crypto::ed25519::sign(&secret, &signing_payload(&op, &nonce, &body));
    let sent_body = if tamper {
        let mut tampered = body.clone();
        tampered.push(b' ');
        tampered
    } else {
        body.clone()
    };
    let presented_key = forge_public.unwrap_or_else(|| identity.public_key_b64.clone());
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature.as_bytes());

    let host_port = base.trim_start_matches("http://").trim_end_matches('/').to_owned();
    let mut stream = match TcpStream::connect(&host_port) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("osl-chats-client: cannot reach {host_port}: {error}");
            return ExitCode::from(3);
        }
    };
    let path = path_override.unwrap_or_else(|| format!("/v1/chats/{op}"));
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: {host_port}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nx-osl-key: {presented_key}\r\nx-osl-sig: {signature_b64}\r\nx-osl-nonce: {nonce}\r\nconnection: close\r\n\r\n",
        sent_body.len()
    );
    if stream.write_all(request.as_bytes()).is_err() || stream.write_all(&sent_body).is_err() {
        eprintln!("osl-chats-client: cannot send");
        return ExitCode::from(3);
    }
    let _ = stream.flush();

    let mut raw = Vec::new();
    if let Err(error) = stream.read_to_end(&mut raw) {
        eprintln!("osl-chats-client: cannot read reply: {error}");
        return ExitCode::from(3);
    }

    let status = raw
        .split(|byte| *byte == b'\n')
        .next()
        .and_then(|line| String::from_utf8_lossy(line).split_whitespace().nth(1).map(str::to_owned))
        .unwrap_or_default();

    if let Some(out_path) = out_path {
        if let Err(error) = std::fs::write(&out_path, &raw) {
            eprintln!("osl-chats-client: cannot save reply: {error}");
            return ExitCode::from(3);
        }
    }
    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "TASK6120CLIENT label={} op={op} status={status} bytes={}",
        identity.label,
        raw.len()
    );
    let _ = stdout.flush();
    ExitCode::SUCCESS
}
