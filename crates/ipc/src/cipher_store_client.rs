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

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::Deserialize;

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
    #[error("attachment I/O failed: {0}")]
    Io(#[from] io::Error),
}

const MAX_BLOB_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_SEALED_ATTACHMENT_BYTES: u64 = 537_919_488;
const LEGACY_DIRECT_ATTACHMENT_BYTES: u64 = 26 * 1024 * 1024;
pub const ATTACHMENT_MULTIPART_PART_BYTES: u64 = 8 * 1024 * 1024;
pub const ATTACHMENT_MULTIPART_MAX_PARTS: u32 = 65;
const ATTACHMENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentSessionResponse {
    id: String,
    expires_at: i64,
    size_bytes: u64,
    max_part_bytes: u64,
    max_parts: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentPartResponse {
    part_number: u32,
    size_bytes: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachmentCompleteResponse {
    id: String,
    expires_at: i64,
    size_bytes: u64,
}

struct ExactPartReader<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for ExactPartReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let maximum = usize::try_from(self.remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = self.inner.read(&mut buffer[..maximum])?;
        if read == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "attachment part truncated"));
        }
        self.remaining -= read as u64;
        Ok(read)
    }
}

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

    /// Stream one already-sealed attachment file to the R2-backed endpoint.
    /// The file never crosses renderer IPC and is never copied into a Vec.
    pub fn upload_attachment_file(
        &self,
        mut sealed: File,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<UploadResult, CipherStoreError> {
        if !is_valid_ttl(ttl_seconds) {
            return Err(CipherStoreError::BadTtl(ttl_seconds));
        }
        let length = sealed.metadata()?.len();
        if length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(length).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        sealed.seek(SeekFrom::Start(0))?;
        if length > LEGACY_DIRECT_ATTACHMENT_BYTES {
            return self.upload_attachment_multipart(sealed, length, ttl_seconds, fetch_token);
        }
        let response = self
            .http
            .post(format!("{}/v1/attachment", self.base_url))
            .timeout(ATTACHMENT_REQUEST_TIMEOUT)
            .header("content-type", "application/octet-stream")
            .header("content-length", length)
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .body(reqwest::blocking::Body::sized(sealed, length))
            .send()?;
        parse_upload_response(response, 32)
    }

    fn upload_attachment_multipart(
        &self,
        sealed: File,
        length: u64,
        ttl_seconds: u32,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<UploadResult, CipherStoreError> {
        let token = hex_lower(fetch_token);
        let response = self
            .http
            .post(format!("{}/v1/attachment/session", self.base_url))
            .timeout(ATTACHMENT_REQUEST_TIMEOUT)
            .header("content-length", 0)
            .header("x-osl-ttl-seconds", ttl_seconds.to_string())
            .header("x-osl-fetch-token", &token)
            .header("x-osl-size-bytes", length.to_string())
            .send()?;
        let session: AttachmentSessionResponse = parse_bounded_json(response)?;
        validate_attachment_id(&session.id)?;
        if session.expires_at <= 0
            || session.size_bytes != length
            || session.max_part_bytes != ATTACHMENT_MULTIPART_PART_BYTES
            || session.max_parts != ATTACHMENT_MULTIPART_MAX_PARTS
        {
            let _ = self.delete_attachment(&session.id, fetch_token);
            return Err(CipherStoreError::ParseError("multipart session has unexpected shape".to_owned()));
        }
        let plan = match multipart_plan(length, session.max_part_bytes, session.max_parts) {
            Ok(plan) => plan,
            Err(error) => {
                let _ = self.delete_attachment(&session.id, fetch_token);
                return Err(error);
            }
        };
        let result = (|| {
            for (part_number, offset, part_length) in plan {
                let mut part_file = sealed.try_clone()?;
                part_file.seek(SeekFrom::Start(offset))?;
                let reader = ExactPartReader { inner: part_file, remaining: part_length };
                let response = self
                    .http
                    .put(format!("{}/v1/attachment/{}/part/{part_number}", self.base_url, session.id))
                    .timeout(ATTACHMENT_REQUEST_TIMEOUT)
                    .header("content-type", "application/octet-stream")
                    .header("content-length", part_length)
                    .header("x-osl-fetch-token", &token)
                    .body(reqwest::blocking::Body::sized(reader, part_length))
                    .send()?;
                let receipt: AttachmentPartResponse = parse_bounded_json(response)?;
                if receipt.part_number != part_number || receipt.size_bytes != part_length {
                    return Err(CipherStoreError::ParseError("multipart part receipt mismatch".to_owned()));
                }
            }
            if sealed.metadata()?.len() != length {
                return Err(CipherStoreError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "sealed attachment changed during upload",
                )));
            }
            let response = self
                .http
                .post(format!("{}/v1/attachment/{}/complete", self.base_url, session.id))
                .timeout(ATTACHMENT_REQUEST_TIMEOUT)
                .header("content-length", 0)
                .header("x-osl-fetch-token", &token)
                .send()?;
            let complete: AttachmentCompleteResponse = parse_bounded_json(response)?;
            if complete.id != session.id || complete.expires_at != session.expires_at || complete.size_bytes != length {
                return Err(CipherStoreError::ParseError("multipart completion receipt mismatch".to_owned()));
            }
            Ok(UploadResult { id_hex: complete.id, expires_at: complete.expires_at })
        })();
        if result.is_err() {
            let _ = self.delete_attachment(&session.id, fetch_token);
        }
        result
    }

    /// Stream an opaque sealed attachment into a trusted native writer.
    /// Callers must use a newly-created OSL staging file and remove it on any
    /// error; this function writes at most the authoritative sealed bound + 1.
    pub fn fetch_attachment_to_writer(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
        output: &mut impl Write,
    ) -> Result<u64, CipherStoreError> {
        validate_attachment_id(id_hex)?;
        let response = self
            .http
            .get(format!("{}/v1/attachment/{id_hex}", self.base_url))
            .timeout(ATTACHMENT_REQUEST_TIMEOUT)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(CipherStoreError::NotFound);
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !status.is_success() {
            return Err(status_error(response));
        }
        if response
            .content_length()
            .is_some_and(|length| length == 0 || length > MAX_SEALED_ATTACHMENT_BYTES)
        {
            return Err(CipherStoreError::BlobTooLarge {
                got: response
                    .content_length()
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        let mut limited = response.take(MAX_SEALED_ATTACHMENT_BYTES + 1);
        let copied = io::copy(&mut limited, output)?;
        if copied == 0 || copied > MAX_SEALED_ATTACHMENT_BYTES {
            return Err(CipherStoreError::BlobTooLarge {
                got: usize::try_from(copied).unwrap_or(usize::MAX),
                max: MAX_SEALED_ATTACHMENT_BYTES as usize,
            });
        }
        Ok(copied)
    }

    pub fn delete_attachment(
        &self,
        id_hex: &str,
        fetch_token: &[u8; FETCH_TOKEN_BYTES],
    ) -> Result<(), CipherStoreError> {
        validate_attachment_id(id_hex)?;
        let response = self
            .http
            .delete(format!("{}/v1/attachment/{id_hex}", self.base_url))
            .timeout(ATTACHMENT_REQUEST_TIMEOUT)
            .header("x-osl-fetch-token", hex_lower(fetch_token))
            .send()?;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(CipherStoreError::RateLimited);
        }
        if !response.status().is_success() {
            return Err(status_error(response));
        }
        Ok(())
    }
}

fn validate_attachment_id(id_hex: &str) -> Result<(), CipherStoreError> {
    if id_hex.len() != 32
        || !id_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CipherStoreError::ParseError(
            "attachment id has unexpected shape".to_owned(),
        ));
    }
    Ok(())
}

fn status_error(response: reqwest::blocking::Response) -> CipherStoreError {
    let status = response.status().as_u16();
    let mut body_bytes = Vec::with_capacity(4 * 1024);
    let _ = response.take(4 * 1024).read_to_end(&mut body_bytes);
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    CipherStoreError::Status { status, body }
}

fn parse_bounded_json<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T, CipherStoreError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CipherStoreError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(status_error(response));
    }
    serde_json::from_reader(response.take(4 * 1024))
        .map_err(|error| CipherStoreError::ParseError(error.to_string()))
}

fn multipart_plan(
    length: u64,
    part_bytes: u64,
    max_parts: u32,
) -> Result<Vec<(u32, u64, u64)>, CipherStoreError> {
    if length == 0
        || length > MAX_SEALED_ATTACHMENT_BYTES
        || part_bytes == 0
        || part_bytes > ATTACHMENT_MULTIPART_PART_BYTES
        || max_parts == 0
        || max_parts > ATTACHMENT_MULTIPART_MAX_PARTS
    {
        return Err(CipherStoreError::ParseError("invalid multipart bounds".to_owned()));
    }
    let count = length.div_ceil(part_bytes);
    if count > u64::from(max_parts) {
        return Err(CipherStoreError::BlobTooLarge {
            got: usize::try_from(length).unwrap_or(usize::MAX),
            max: usize::try_from(part_bytes.saturating_mul(u64::from(max_parts))).unwrap_or(usize::MAX),
        });
    }
    let mut plan = Vec::with_capacity(count as usize);
    let mut offset = 0u64;
    for part_number in 1..=count as u32 {
        let size = (length - offset).min(part_bytes);
        plan.push((part_number, offset, size));
        offset += size;
    }
    Ok(plan)
}

fn parse_upload_response(
    response: reqwest::blocking::Response,
    expected_id_hex_len: usize,
) -> Result<UploadResult, CipherStoreError> {
    if response.status() == StatusCode::TOO_MANY_REQUESTS {
        return Err(CipherStoreError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(status_error(response));
    }
    let json: serde_json::Value = serde_json::from_reader(response.take(4 * 1024))
        .map_err(|error| CipherStoreError::ParseError(error.to_string()))?;
    let id_hex = json
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CipherStoreError::ParseError("missing id".to_owned()))?
        .to_owned();
    if id_hex.len() != expected_id_hex_len
        || !id_hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CipherStoreError::ParseError(
            "id has unexpected shape".to_owned(),
        ));
    }
    let expires_at = json
        .get("expires_at")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| CipherStoreError::ParseError("missing expires_at".to_owned()))?;
    Ok(UploadResult { id_hex, expires_at })
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

    #[test]
    fn attachment_ids_are_canonical_lowercase_128_bit_values() {
        assert!(validate_attachment_id("0123456789abcdef0123456789abcdef").is_ok());
        assert!(validate_attachment_id("0123456789ABCDEF0123456789ABCDEF").is_err());
        assert!(validate_attachment_id("0123456789abcdef").is_err());
        assert!(validate_attachment_id("0123456789abcdef0123456789abcdeg").is_err());
    }

    #[test]
    fn multipart_plan_covers_the_512_mib_boundary_with_fixed_parts() {
        let plan = multipart_plan(
            MAX_SEALED_ATTACHMENT_BYTES,
            ATTACHMENT_MULTIPART_PART_BYTES,
            ATTACHMENT_MULTIPART_MAX_PARTS,
        )
        .unwrap();
        assert_eq!(plan.len(), 65);
        assert!(plan.iter().all(|(_, _, size)| *size <= ATTACHMENT_MULTIPART_PART_BYTES));
        assert_eq!(plan.iter().map(|(_, _, size)| *size).sum::<u64>(), MAX_SEALED_ATTACHMENT_BYTES);
        assert_eq!(plan.last().unwrap().0, 65);
    }

    #[test]
    fn exact_part_reader_detects_truncation_without_large_allocation() {
        let mut reader = ExactPartReader { inner: io::Cursor::new(vec![1u8; 7]), remaining: 8 };
        let mut output = [0u8; 8];
        assert_eq!(reader.read(&mut output).unwrap(), 7);
        assert_eq!(reader.read(&mut output).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
