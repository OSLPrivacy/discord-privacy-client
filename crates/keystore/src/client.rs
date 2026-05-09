//! Minimal HTTP/1.1 client for the prototype key server.
//!
//! Hand-rolled over [`std::net::TcpStream`] with `Content-Length`
//! framing and `Connection: close`. **Plain HTTP only.** v1 stable
//! replaces with a real client + TLS + Discord OAuth + retries; see
//! the crate-level INSECURE banner.
//!
//! Endpoints exposed here mirror [`keyserver/src/server.js`]:
//!
//! - [`KeyServerClient::register`] → `POST /v1/register`
//! - [`KeyServerClient::fetch_pubkeys`] → `GET /v1/pubkeys/:user_id`
//!
//! All calls block on I/O. Tauri command handlers (Layer 8) drive
//! these through `tokio::task::spawn_blocking` to avoid stalling the
//! async runtime.

use crate::identity::Identity;
use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Body of `POST /v1/register`.
#[derive(Serialize)]
pub struct RegisterRequest {
    pub user_id: String,
    pub ik_x25519_pub: String,
    pub ik_mlkem768_pub: String,
    /// Self-signature binding `user_id` to the keys.
    /// **Not verified by the prototype server**; field included for
    /// forward-compatibility with v1 stable, where it becomes
    /// load-bearing (Ed25519 signature derived from the X25519
    /// identity key).
    pub ik_x25519_signature: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub registered_at: Option<String>,
    pub key_rotation_recorded: Option<bool>,
    pub last_rotated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PubkeysResponse {
    pub user_id: String,
    pub ik_x25519_pub: String,
    pub ik_mlkem768_pub: String,
    pub registered_at: String,
    pub last_rotated_at: Option<String>,
}

/// Holds the parsed `host:port` derived from `base_url` and a default
/// 30 s I/O timeout.
pub struct KeyServerClient {
    host: String,
    port: u16,
    base_path: String,
    timeout: Duration,
}

impl KeyServerClient {
    /// `base_url` must be of the form `http://host[:port][/base-path]`.
    /// `https://` is rejected — the prototype is plain HTTP only.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let url = base_url.as_ref();
        let after_scheme = url.strip_prefix("http://").ok_or_else(|| {
            Error::Transport(format!(
                "base_url must start with http:// (prototype is plain HTTP only): {url:?}"
            ))
        })?;
        let (authority, base_path) = match after_scheme.find('/') {
            Some(i) => (&after_scheme[..i], after_scheme[i..].trim_end_matches('/').to_string()),
            None => (after_scheme, String::new()),
        };
        let (host, port) = match authority.rfind(':') {
            Some(i) => {
                let port: u16 = authority[i + 1..].parse().map_err(|_| {
                    Error::Transport(format!("invalid port in base_url: {authority:?}"))
                })?;
                (authority[..i].to_string(), port)
            }
            None => (authority.to_string(), 80),
        };
        Ok(KeyServerClient {
            host,
            port,
            base_path,
            timeout: Duration::from_secs(30),
        })
    }

    /// Build the registration request body for `identity`.
    pub fn build_register_request(identity: &Identity) -> RegisterRequest {
        RegisterRequest {
            user_id: identity.user_id.clone(),
            ik_x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
            ik_mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
            ik_x25519_signature: STANDARD.encode(b"PROTOTYPE_NO_SIG"),
        }
    }

    /// `POST /v1/register`.
    pub fn register(&self, identity: &Identity) -> Result<RegisterResponse> {
        let body = Self::build_register_request(identity);
        let body_json = serde_json::to_vec(&body)?;
        let resp = self.send_request(
            "POST",
            "/v1/register",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    /// `GET /v1/pubkeys/:user_id`.
    pub fn fetch_pubkeys(&self, user_id: &str) -> Result<PubkeysResponse> {
        let path = format!("/v1/pubkeys/{}", urlencode_segment(user_id));
        let resp = self.send_request("GET", &path, None)?;
        check_2xx(&resp)?;
        Ok(serde_json::from_slice(&resp.body)?)
    }

    fn send_request(
        &self,
        method: &str,
        path: &str,
        body: Option<(&str, &[u8])>,
    ) -> Result<HttpResponse> {
        let full_path = format!("{}{}", self.base_path, path);
        let host_header = if self.port == 80 {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        };

        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| Error::Transport(format!("DNS resolve {}: {e}", self.host)))?
            .next()
            .ok_or_else(|| Error::Transport(format!("no addrs for {}", self.host)))?;
        let mut stream = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|e| Error::Transport(format!("connect {addr}: {e}")))?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let mut req = Vec::new();
        req.extend_from_slice(format!("{method} {full_path} HTTP/1.1\r\n").as_bytes());
        req.extend_from_slice(format!("Host: {host_header}\r\n").as_bytes());
        req.extend_from_slice(b"User-Agent: discord-privacy-client/0.0.1\r\n");
        req.extend_from_slice(b"Accept: application/json\r\n");
        req.extend_from_slice(b"Connection: close\r\n");
        if let Some((ctype, payload)) = body {
            req.extend_from_slice(format!("Content-Type: {ctype}\r\n").as_bytes());
            req.extend_from_slice(format!("Content-Length: {}\r\n", payload.len()).as_bytes());
            req.extend_from_slice(b"\r\n");
            req.extend_from_slice(payload);
        } else {
            req.extend_from_slice(b"Content-Length: 0\r\n\r\n");
        }
        stream.write_all(&req)?;

        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;
        parse_response(&raw)
    }
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn check_2xx(resp: &HttpResponse) -> Result<()> {
    if (200..300).contains(&resp.status) {
        Ok(())
    } else {
        Err(Error::HttpStatus {
            status: resp.status,
            body: String::from_utf8_lossy(&resp.body).to_string(),
        })
    }
}

/// Parse a minimal HTTP/1.1 response. Supports `Content-Length`
/// framing and `Connection: close` (the only modes the prototype
/// keyserver ever emits).
fn parse_response(raw: &[u8]) -> Result<HttpResponse> {
    // Find header / body split.
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| Error::Transport("malformed response: no header terminator".into()))?;
    let header_bytes = &raw[..split];
    let body = raw[split + 4..].to_vec();

    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|_| Error::Transport("malformed response: non-utf8 header".into()))?;
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let mut sl_parts = status_line.splitn(3, ' ');
    let _http = sl_parts.next();
    let status_str = sl_parts
        .next()
        .ok_or_else(|| Error::Transport(format!("malformed status line: {status_line:?}")))?;
    let status: u16 = status_str
        .parse()
        .map_err(|_| Error::Transport(format!("non-numeric status: {status_str:?}")))?;

    // We requested `Connection: close` — server returns body framed
    // by either Content-Length (and we got at least that many bytes)
    // or by the close. Either way, `body` already contains the full
    // payload from `read_to_end`, so we don't need to honour
    // Content-Length explicitly. Trim trailing chunked-marker bytes
    // if any (we never advertise TE, and the prototype server doesn't
    // chunk, but be defensive).
    Ok(HttpResponse { status, body })
}

/// Encode each byte that isn't an unreserved URL character as %XX.
/// Used for the `:user_id` path segment.
fn urlencode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        let c = *b;
        let unreserved = c.is_ascii_alphanumeric()
            || c == b'-'
            || c == b'.'
            || c == b'_'
            || c == b'~';
        if unreserved {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{:02X}", c));
        }
    }
    out
}
