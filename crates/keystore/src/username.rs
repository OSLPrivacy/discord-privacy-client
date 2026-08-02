//! Privacy-preserving exact username resolution.
//!
//! A username is never sent to the directory.  The client sends only the
//! first four hexadecimal characters of a domain-separated SHA-256 digest,
//! receives a fixed-size bucket, and compares the remaining digest locally.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};
use thiserror::Error;

const BUCKET_DOMAIN: &[u8] = b"OSL-USERNAME-BUCKET-v1";
const BUCKET_ROWS: usize = 1024;
const PREFIX_HEX_LEN: usize = 4;
const SUFFIX_HEX_LEN: usize = 60;
const DIRECTORY_ORIGIN: &str = "https://keyserver.oslprivacy.com";

/// The identity information authenticated by a matching bucket row.
///
/// Callers must fetch the full key bundle separately and require that its
/// Ed25519 key equals this value before treating that bundle as resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedIdentity {
    pub user_id: String,
    pub ed25519_public: [u8; 32],
}

#[derive(Debug, Error)]
pub enum UsernameResolveError {
    #[error("invalid username directory base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("username bucket transport error: {0}")]
    Transport(String),
    #[error("username bucket returned status {0}")]
    HttpStatus(u16),
    #[error("username bucket is not valid UTF-8")]
    InvalidUtf8,
    #[error("username bucket must contain exactly {expected} rows, got {actual}")]
    WrongRowCount { expected: usize, actual: usize },
    #[error("username bucket row {row} is malformed")]
    MalformedRow { row: usize },
    #[error("username bucket rows are not strictly sorted by suffix")]
    UnsortedRows,
}

pub type Result<T> = core::result::Result<T, UsernameResolveError>;

/// Resolve `name` against OSL's production username directory.
///
/// [`Resolver`] is exposed as well so the application can use its configured
/// keyserver origin and tests can use a fixture server without contacting a
/// live directory.
pub fn resolve(name: &str) -> Result<Option<ResolvedIdentity>> {
    Resolver::new(DIRECTORY_ORIGIN)?.resolve(name)
}

/// A client for the fixed-size username directory bucket endpoint.
#[derive(Clone)]
pub struct Resolver {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl Resolver {
    /// Construct a resolver for a directory origin. The supplied origin is
    /// also useful for fixture servers in tests; production origin policy is
    /// enforced by the application-level keyserver client when this resolver
    /// is wired into it.
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let parsed = reqwest::Url::parse(base_url.as_ref())
            .map_err(|error| UsernameResolveError::InvalidBaseUrl(error.to_string()))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err(UsernameResolveError::InvalidBaseUrl(
                "URL scheme must be http or https".into(),
            ));
        }
        if parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(UsernameResolveError::InvalidBaseUrl(
                "URL must be an origin without credentials, query, or fragment".into(),
            ));
        }

        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| UsernameResolveError::Transport(error.to_string()))?;
        Ok(Self {
            base_url: parsed.as_str().trim_end_matches('/').to_owned(),
            client,
        })
    }

    /// Resolve `name` through its k-anonymity bucket.
    pub fn resolve(&self, name: &str) -> Result<Option<ResolvedIdentity>> {
        let digest = username_digest(name);
        let digest_hex = hex(&digest);
        let prefix = &digest_hex[..PREFIX_HEX_LEN];
        let suffix = &digest_hex[PREFIX_HEX_LEN..];
        let url = format!("{}/v1/username-bucket/{prefix}", self.base_url);
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|error| UsernameResolveError::Transport(error.to_string()))?;
        if !response.status().is_success() {
            return Err(UsernameResolveError::HttpStatus(response.status().as_u16()));
        }
        let body = response
            .bytes()
            .map_err(|error| UsernameResolveError::Transport(error.to_string()))?;
        resolve_bucket(suffix, &body)
    }
}

fn username_digest(name: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(BUCKET_DOMAIN);
    hasher.update(name.as_bytes());
    hasher.finalize().into()
}

fn resolve_bucket(suffix: &str, body: &[u8]) -> Result<Option<ResolvedIdentity>> {
    let body = std::str::from_utf8(body).map_err(|_| UsernameResolveError::InvalidUtf8)?;
    if !body.ends_with('\n') {
        return Err(UsernameResolveError::WrongRowCount {
            expected: BUCKET_ROWS,
            actual: body.lines().count(),
        });
    }
    let rows: Vec<&str> = body.split_terminator('\n').collect();
    if rows.len() != BUCKET_ROWS {
        return Err(UsernameResolveError::WrongRowCount {
            expected: BUCKET_ROWS,
            actual: rows.len(),
        });
    }

    let mut previous_suffix: Option<&str> = None;
    let mut resolved = None;
    for (index, row) in rows.into_iter().enumerate() {
        let row_number = index + 1;
        let Some((row_suffix, rest)) = row.split_once(':') else {
            return Err(UsernameResolveError::MalformedRow { row: row_number });
        };
        let Some((user_id, ed25519_b64)) = rest.split_once(':') else {
            return Err(UsernameResolveError::MalformedRow { row: row_number });
        };
        if row_suffix.len() != SUFFIX_HEX_LEN
            || !row_suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
            || user_id.is_empty()
            || user_id.contains(':')
        {
            return Err(UsernameResolveError::MalformedRow { row: row_number });
        }
        if previous_suffix.is_some_and(|previous| previous >= row_suffix) {
            return Err(UsernameResolveError::UnsortedRows);
        }
        previous_suffix = Some(row_suffix);

        let ed25519 = STANDARD
            .decode(ed25519_b64)
            .ok()
            .and_then(|bytes| <[u8; 32]>::try_from(bytes.as_slice()).ok())
            .ok_or(UsernameResolveError::MalformedRow { row: row_number })?;
        if row_suffix == suffix {
            resolved = Some(ResolvedIdentity {
                user_id: user_id.to_owned(),
                ed25519_public: ed25519,
            });
        }
    }
    Ok(resolved)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
