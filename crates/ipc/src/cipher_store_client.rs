//! HTTP client for the OSL cipher-store Worker (Phase 2 pivot).
//!
//! Endpoints exposed by `cipher-store-cf`:
//!   POST   /v1/blob          body: raw ciphertext, header X-OSL-TTL-Seconds
//!   GET    /v1/blob/:id_hex  → raw ciphertext bytes
//!   DELETE /v1/blob/:id_hex
//!
//! Subpoena-resistance posture: the client sends no identifier, no
//! cookie, no auth header. The 8-byte ID returned by upload IS the
//! capability (whoever has it can fetch / delete; whoever doesn't can
//! see only an opaque encrypted blob). Phase 6 will wrap upload + fetch
//! in Privacy Pass anonymous credentials.

use std::path::Path;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::StatusCode;

/// Built-in production cipher-store. Overridable per install via
/// `<config_dir>/keyserver.json` field `cipher_store_url` (same file
/// the keyserver base URL override lives in — one config surface for
/// both backends).
pub const DEFAULT_CIPHER_STORE_BASE_URL: &str = "https://ciphers.oslprivacy.com";

/// Allowed TTL values mirror the server-side validation in
/// `cipher-store-cf/src/endpoints/blob.ts`. Client rejects bad values
/// before the network roundtrip.
pub const TTL_1H: u32 = 60 * 60;
pub const TTL_24H: u32 = 24 * 60 * 60;
pub const TTL_72H: u32 = 72 * 60 * 60;
pub const TTL_7D: u32 = 7 * 24 * 60 * 60;

fn is_valid_ttl(ttl: u32) -> bool {
    ttl == TTL_1H || ttl == TTL_24H || ttl == TTL_72H || ttl == TTL_7D
}

/// Phase 6 capability-token length. HMAC-SHA256 truncated to 16
/// bytes (128 bits) — enough security margin against brute force
/// while keeping the header short. The worker stores the hex form
/// of this exact byte length.
pub const FETCH_TOKEN_BYTES: usize = 16;

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Resolve the cipher-store base URL from the config dir's
/// `keyserver.json`, falling back to the built-in production default.
pub fn resolve_cipher_store_base_url(dir: &Path) -> String {
    let path = dir.join("keyserver.json");
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(s) = v.get("cipher_store_url").and_then(|x| x.as_str()) {
                if !s.is_empty() {
                    return s.trim_end_matches('/').to_string();
                }
            }
        }
    }
    DEFAULT_CIPHER_STORE_BASE_URL.to_string()
}

/// Successful upload response.
#[derive(Debug, Clone)]
pub struct UploadResult {
    /// 16 hex chars = 8 random bytes the server assigned.
    pub id_hex: String,
    /// Unix-epoch seconds at which the server will delete this blob.
    pub expires_at: i64,
}

/// Errors that surface from the cipher-store client. Kept narrow on
/// purpose: callers route everything to a single user-facing toast
/// ("upload failed, retry"), so we don't need granular discrimination
/// in the IPC layer.
#[derive(Debug, thiserror::Error)]
pub enum CipherStoreError {
    #[error("invalid TTL {0}; must be 3600 (1h), 86400 (24h), 259200 (72h), or 604800 (7d)")]
    BadTtl(u32),
    #[error("blob exceeds {max} bytes (got {got})")]
    BlobTooLarge { got: usize, max: usize },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("server returned {status}: {body}")]
    Status { status: u16, body: String },
    #[error("server response malformed: {0}")]
    ParseError(String),
    #[error("blob not found or expired")]
    NotFound,
    #[error("rate limit hit, retry later")]
    RateLimited,
}

const MAX_BLOB_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Thin client wrapping a reused reqwest::blocking::Client. Cheap to
/// construct; callers can hold one per AppState.
pub struct CipherStoreClient {
    base_url: String,
    http: Client,
}

impl CipherStoreClient {
    pub fn new(base_url: impl Into<String>) -> Result<Self, CipherStoreError> {
        let http = Client::builder().timeout(REQUEST_TIMEOUT).build()?;
        Ok(Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            http,
        })
    }

    /// Upload a ciphertext blob with the chosen TTL + a per-blob
    /// capability token (Phase 6). Returns the server-assigned
    /// 16-hex-char ID and absolute expiry. The worker stores the
    /// token alongside the blob and rejects future fetch/delete
    /// requests that don't present a matching one.
    pub fn upload(
        &self,
        body: &[u8],
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<UploadResult, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        if body.is_empty() || body.len() > MAX_BLOB_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: body.len(),
                max: MAX_BLOB_BYTES,
            });
        }
        let url = format!("{}/v1/blob", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("content-type", "application/octet-stream")
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .body(body.to_vec())
            .send()?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        // Body shape: { "id": "<16 hex>", "expires_at": <i64> }
        let json: serde_json::Value = resp
            .json()
            .map_err(|e| CipherStoreError::ParseError(e.to_string()))?;
        let id_hex = json
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CipherStoreError::ParseError("missing id".into()))?
            .to_string();
        if id_hex.len() != 16 || !id_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CipherStoreError::ParseError(format!(
                "id has unexpected shape: {id_hex:?}"
            )));
        }
        let expires_at = json
            .get("expires_at")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| CipherStoreError::ParseError("missing expires_at".into()))?;
        Ok(UploadResult { id_hex, expires_at })
    }

    /// Fetch ciphertext bytes by ID + capability token (Phase 6).
    /// Returns `NotFound` for missing / expired / burned blobs.
    /// Returns `Status { status: 401|403, .. }` when the token is
    /// missing or doesn't match the one the uploader recorded.
    pub fn fetch(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<Vec<u8>, CipherStoreError> {
        let url = format!("{}/v1/blob/{}", self.base_url, id_hex);
        let resp = self
            .http
            .get(&url)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Err(CipherStoreError::NotFound);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes()?;
        Ok(bytes.to_vec())
    }

    /// Burn (delete) a blob. Idempotent — second call returns Ok even
    /// if the row was already gone. Phase 6: same capability-token
    /// gate as fetch so a leaked blob_id alone cannot be used to
    /// DoS-delete a conversation's blobs.
    pub fn delete(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        let url = format!("{}/v1/blob/{}", self.base_url, id_hex);
        let resp = self
            .http
            .delete(&url)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        // 204 No Content on success; the server also returns 204 when
        // the row didn't exist (idempotent delete).
        if !status.is_success() && status != StatusCode::NO_CONTENT {
            let body = resp.text().unwrap_or_default();
            return Err(CipherStoreError::Status {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_allowlist_matches_the_cipher_store_worker() {
        assert_eq!(TTL_1H, 3_600);
        assert_eq!(TTL_24H, 86_400);
        assert_eq!(TTL_72H, 259_200);
        assert_eq!(TTL_7D, 604_800);
        for ttl in [TTL_1H, TTL_24H, TTL_72H, TTL_7D] {
            assert!(is_valid_ttl(ttl));
        }
        for ttl in [0, 3_599, 3_601, 86_399, 604_801, u32::MAX] {
            assert!(!is_valid_ttl(ttl));
        }
    }

    #[test]
    fn bad_ttl_error_lists_every_exact_option() {
        assert_eq!(
            CipherStoreError::BadTtl(3_601).to_string(),
            "invalid TTL 3601; must be 3600 (1h), 86400 (24h), 259200 (72h), or 604800 (7d)"
        );
    }
}
