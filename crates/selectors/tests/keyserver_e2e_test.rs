//! End-to-end manifest fetch: spawn the Node keyserver with a
//! Node-signed manifest envelope and verify the Rust
//! `selectors::verify_manifest` accepts it.
//!
//! Locks the cross-language byte agreement: Node's
//! `canonicalManifestBytes` and Rust's `canonical_manifest_bytes`
//! must produce byte-identical output, otherwise the Ed25519
//! signature won't verify on the Rust side.
//!
//! Skipped automatically if `node` isn't on PATH or `npm install`
//! hasn't been run for the keyserver.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use selectors::{
    parse_signed_manifest, verify_manifest, FetchError, ManifestFetcher, ManifestSource,
    ManifestState, SourceLabel,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn keyserver_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("keyserver");
    p
}

fn skip_if_keyserver_unavailable() -> bool {
    let dir = keyserver_dir();
    if !dir.join("node_modules").exists() {
        eprintln!(
            "skipping selectors e2e: {} has no node_modules — run `npm install`",
            dir.display()
        );
        return true;
    }
    if which("node").is_none() {
        eprintln!("skipping selectors e2e: `node` not on PATH");
        return true;
    }
    false
}

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let p = entry.join(prog);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

struct ServerHandle {
    child: Child,
    port: u16,
    _manifest_file: tempfile::NamedTempFile,
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Use a tiny helper Node script to produce a SignedManifest envelope
/// (avoids re-implementing canonicalManifestBytes in the Rust test).
/// Returns `(envelope_json, trusted_pub_b64, issued_at_unix_seconds)`.
fn node_signed_envelope(issued_at_unix_seconds: u64) -> (String, String, u64) {
    let script = format!(
        r#"
import {{ generateKeyPairSync, sign }} from 'node:crypto';
import {{ canonicalManifestBytes }} from './src/canonical.js';
const {{ privateKey, publicKey }} = generateKeyPairSync('ed25519');
const der = publicKey.export({{ format: 'der', type: 'spki' }});
const raw = der.subarray(der.length - 32);
const pubB64 = raw.toString('base64');
const manifest = {{
  version: 1,
  issued_at_unix_seconds: {issued_at_unix_seconds},
  client_min_version: '0.1.0',
  selectors: {{ MessageContent: 'abcd', MessageTextarea: 'wxyz' }},
}};
const bytes = canonicalManifestBytes(manifest);
const sig = sign(null, bytes, privateKey);
const env = {{
  version: 1,
  manifest_b64: bytes.toString('base64'),
  signature_b64: sig.toString('base64'),
  signing_key_b64: pubB64,
}};
process.stdout.write(JSON.stringify({{ envelope: env, pub: pubB64 }}));
"#
    );
    let dir = keyserver_dir();
    let mut child = Command::new("node")
        .arg("--input-type=module")
        .arg("-e")
        .arg(&script)
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn node helper");
    let mut out = String::new();
    child
        .stdout
        .as_mut()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "node helper failed: {status:?}");
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    (
        parsed["envelope"].to_string(),
        parsed["pub"].as_str().unwrap().to_string(),
        issued_at_unix_seconds,
    )
}

fn spawn_keyserver_with_manifest(envelope_json: &str) -> ServerHandle {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(envelope_json.as_bytes()).unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut cmd = Command::new("node");
    cmd.arg("src/server.js")
        .current_dir(keyserver_dir())
        .env("PORT", port.to_string())
        .env("KEYSERVER_DB", ":memory:")
        .env("SELECTOR_MANIFEST_PATH", tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn keyserver");

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() >= deadline {
            if let Some(stderr) = child.stderr.take() {
                let mut buf = String::new();
                let mut reader = BufReader::new(stderr);
                while reader.read_line(&mut buf).unwrap_or(0) > 0 {
                    if buf.len() > 4096 {
                        break;
                    }
                }
                eprintln!("keyserver stderr: {buf}");
            }
            let _ = child.kill();
            panic!("keyserver did not become ready within 5s on port {port}");
        }
        if TcpStream::connect_timeout(
            &format!("127.0.0.1:{port}").parse().unwrap(),
            Duration::from_millis(50),
        )
        .is_ok()
        {
            return ServerHandle {
                child,
                port,
                _manifest_file: tmp,
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Tiny HTTP/1.1 GET adapter — selectors crate stays HTTP-free, so
/// this lives in the test.
struct HttpManifestSource {
    label: SourceLabel,
    host: String,
    port: u16,
    path: String,
}

impl ManifestSource for HttpManifestSource {
    fn label(&self) -> SourceLabel {
        self.label
    }
    fn fetch(&self) -> Result<Vec<u8>, FetchError> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| FetchError::Transport(format!("dns: {e}")))?
            .next()
            .ok_or_else(|| FetchError::Transport("no addrs".into()))?;
        let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
            .map_err(|e| FetchError::Transport(format!("connect: {e}")))?;
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n\r\n",
            self.path, self.host, self.port
        );
        s.write_all(req.as_bytes())
            .map_err(|e| FetchError::Transport(format!("write: {e}")))?;
        let mut raw = Vec::new();
        s.read_to_end(&mut raw)
            .map_err(|e| FetchError::Transport(format!("read: {e}")))?;
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| FetchError::Transport("no header end".into()))?;
        let head = std::str::from_utf8(&raw[..split])
            .map_err(|_| FetchError::Transport("non-utf8 head".into()))?;
        let status_line = head.lines().next().unwrap_or("");
        let mut parts = status_line.splitn(3, ' ');
        let _ = parts.next();
        let status_str = parts.next().unwrap_or("0");
        let status: u16 = status_str.parse().unwrap_or(0);
        let body = raw[split + 4..].to_vec();
        if !(200..300).contains(&status) {
            return Err(FetchError::HttpStatus {
                status,
                body: String::from_utf8_lossy(&body).to_string(),
            });
        }
        Ok(body)
    }
}

#[test]
fn rust_verifies_node_signed_manifest_via_real_keyserver() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    // Sign with the Node-side `canonicalManifestBytes` and have the
    // Rust verifier accept it.
    let issued = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (envelope_json, trusted_pub_b64, _) = node_signed_envelope(issued);

    let server = spawn_keyserver_with_manifest(&envelope_json);
    let primary = HttpManifestSource {
        label: SourceLabel::Primary,
        host: "127.0.0.1".into(),
        port: server.port,
        path: "/v1/selector-manifest".into(),
    };
    let cdn = HttpManifestSource {
        label: SourceLabel::CdnMirror,
        host: "127.0.0.1".into(),
        port: server.port,
        path: "/does-not-exist".into(), // will 404 → forced primary success
    };

    let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), trusted_pub_b64.clone());
    let state = f.refresh(issued);
    match state {
        ManifestState::Loaded { manifest, from } => {
            assert_eq!(*from, SourceLabel::Primary);
            assert_eq!(
                manifest.selectors.get("MessageContent").map(String::as_str),
                Some("abcd")
            );
            assert_eq!(manifest.client_min_version, "0.1.0");
        }
        other => panic!("expected Loaded, got {other:?}"),
    }

    // Sanity: parse + verify the envelope directly too, without the
    // fetcher. This is the load-bearing cross-language check.
    let signed = parse_signed_manifest(envelope_json.as_bytes()).unwrap();
    let m = verify_manifest(&signed, &trusted_pub_b64, issued).unwrap();
    assert_eq!(m.selectors.len(), 2);
    assert!(STANDARD.decode(&signed.signing_key_b64).is_ok());
}

#[test]
fn fetcher_falls_back_to_cdn_when_primary_503s() {
    if skip_if_keyserver_unavailable() {
        return;
    }
    // Spawn TWO servers: primary unconfigured (returns 503), CDN
    // serves the signed manifest.
    let issued = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (envelope_json, trusted_pub_b64, _) = node_signed_envelope(issued);

    let primary_server = spawn_keyserver_with_manifest("{}"); // serves 503
    let cdn_server = spawn_keyserver_with_manifest(&envelope_json);

    let primary = HttpManifestSource {
        label: SourceLabel::Primary,
        host: "127.0.0.1".into(),
        port: primary_server.port,
        path: "/v1/selector-manifest".into(),
    };
    let cdn = HttpManifestSource {
        label: SourceLabel::CdnMirror,
        host: "127.0.0.1".into(),
        port: cdn_server.port,
        path: "/v1/selector-manifest".into(),
    };

    let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), trusted_pub_b64);
    let state = f.refresh(issued);
    match state {
        ManifestState::Loaded { from, .. } => {
            assert_eq!(*from, SourceLabel::CdnMirror);
        }
        other => panic!("expected Loaded from CDN, got {other:?}"),
    }
}
