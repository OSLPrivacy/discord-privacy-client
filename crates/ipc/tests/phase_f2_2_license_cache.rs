//! F2.2: license-cache commands.
//!
//! Exercises the `_with_dir`-style test seams added on top of
//! `cmd_osl_validate_license` / `cmd_osl_get_license_state` /
//! `cmd_osl_clear_license`. The seams take an explicit config dir
//! (and, for validate, an explicit keyserver base URL) so tests
//! can point at a `tempdir()` and an in-process mock HTTP server
//! instead of `%APPDATA%\osl` and the real keyserver.
//!
//! The keystore-level mechanics (Sealer round-trip, tamper
//! rejection, version/method mismatch, classify_state) live in
//! `crates/keystore/tests/license_cache_test.rs`; this file
//! covers the IPC-layer orchestration.

use ipc::commands::{
    cmd_osl_clear_license_with_dir, cmd_osl_get_license_state_with_dir,
    cmd_osl_validate_license_with_dir_and_url,
};
use ipc::AppState;
use keystore::{
    classify_state, save_license_cache, select_best_sealer, LicenseCacheInner, LicenseState,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use tempfile::tempdir;

// ---- helpers ----

/// One-shot mock HTTP server. Copy of the harness in
/// `crates/keystore/tests/client_test.rs`. Returns the bound port
/// + a channel that yields the captured request bytes once the
/// server fulfils the handshake.
fn one_shot_server(response: Vec<u8>) -> (u16, mpsc::Receiver<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 4096];
        let mut acc = Vec::new();
        let header_end = loop {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break acc.len();
            }
            acc.extend_from_slice(&buf[..n]);
            if let Some(p) = acc.windows(4).position(|w| w == b"\r\n\r\n") {
                break p;
            }
        };
        let header_text = std::str::from_utf8(&acc[..header_end]).unwrap();
        let cl = header_text
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body_so_far = acc[header_end + 4..].len();
        while body_so_far < cl {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            acc.extend_from_slice(&buf[..n]);
            body_so_far += n;
        }
        let _ = tx.send(acc);
        let _ = stream.write_all(&response);
    });
    (port, rx)
}

fn ok_response(status: &str, current_period_end: &str, checksum_ok: bool) -> Vec<u8> {
    let body = format!(
        r#"{{"status":"{status}","current_period_end":{current_period_end},"checksum_ok":{checksum_ok}}}"#
    );
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

fn http_status_response(status_line: &str, body: &str) -> Vec<u8> {
    let mut response = Vec::new();
    response.extend_from_slice(format!("HTTP/1.1 {status_line}\r\n").as_bytes());
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

fn seed_cache_with_status(dir: &std::path::Path, status: &str) {
    let inner = LicenseCacheInner {
        license_plaintext: "OSL-2222-3333-4444-5555".to_string(),
        last_validated_status: status.to_string(),
        current_period_end: Some(1_800_000_000),
        last_validated_at: 1_700_000_000,
        checksum_ok: true,
    };
    let sealer = select_best_sealer();
    save_license_cache(&dir.join("license.json"), &inner, sealer.as_ref()).unwrap();
}

// ---- cmd_osl_get_license_state ----

#[test]
fn get_license_state_no_cache_returns_unconfigured() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "Unconfigured");
    assert_eq!(dto.current_period_end, None);
    assert_eq!(dto.last_validated_at, None);
}

#[test]
fn get_license_state_cached_active_maps_to_paid() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache_with_status(dir.path(), "ACTIVE");
    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    assert_eq!(dto.state, LicenseState::Paid);
    assert_eq!(dto.raw_status, "ACTIVE");
    assert_eq!(dto.current_period_end, Some(1_800_000_000));
    assert_eq!(dto.last_validated_at, Some(1_700_000_000));
}

#[test]
fn get_license_state_cached_expired_maps_to_free() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache_with_status(dir.path(), "EXPIRED");
    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    assert_eq!(dto.state, LicenseState::Free);
    // raw_status surfaces so the UI can render the exact keyserver
    // string ("Subscription expired" vs "Subscription revoked").
    assert_eq!(dto.raw_status, "EXPIRED");
}

#[test]
fn get_license_state_malformed_cache_falls_back_to_unconfigured() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    // Write a garbage license.json (not valid JSON for the wrapper).
    std::fs::write(dir.path().join("license.json"), b"definitely not json").unwrap();
    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "Unconfigured");
}

// ---- cmd_osl_clear_license ----

#[test]
fn clear_license_removes_existing_cache() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache_with_status(dir.path(), "ACTIVE");
    assert!(dir.path().join("license.json").exists());
    cmd_osl_clear_license_with_dir(&state, dir.path()).unwrap();
    assert!(!dir.path().join("license.json").exists());
}

#[test]
fn clear_license_idempotent_on_already_absent() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    assert!(!dir.path().join("license.json").exists());
    cmd_osl_clear_license_with_dir(&state, dir.path()).expect("first clear is a no-op");
    // Re-running must also succeed.
    cmd_osl_clear_license_with_dir(&state, dir.path()).expect("second clear is a no-op");
    assert!(!dir.path().join("license.json").exists());
}

// ---- cmd_osl_validate_license cache-write policy ----

#[test]
fn validate_license_writes_cache_on_success() {
    let response = ok_response("ACTIVE", "1800000000", true);
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");

    let resp = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-2222-3333-4444-5555".to_string(),
        dir.path(),
        &url,
    )
    .expect("validate should succeed against mock");
    assert_eq!(resp.status, "ACTIVE");
    assert_eq!(resp.current_period_end, Some(1_800_000_000));

    // The cache file must exist after a successful round-trip.
    let cache_path = dir.path().join("license.json");
    assert!(cache_path.exists(), "cache file MUST be written on success");

    // ...and the cached status must match what was just validated.
    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    assert_eq!(dto.raw_status, "ACTIVE");
    assert_eq!(dto.state, classify_state("ACTIVE"));
}

#[test]
fn validate_license_does_not_write_cache_on_unreachable() {
    // Bind + drop a port to guarantee connect-refused; client
    // returns Error::Transport. F2.4 will honour the cached state
    // when this happens; F2.2 must leave the cache untouched.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");
    let result = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-2222-3333-4444-5555".to_string(),
        dir.path(),
        &url,
    );
    assert!(result.is_err(), "expected an error from unreachable host");
    let err = result.unwrap_err();
    // F3.2: error shape is `OSL-VALIDATE-ERR:{json}` with
    // `kind = "unreachable"`. Pre-F3.2 this was a free-form
    // "OSL: keyserver unreachable: …" string.
    assert!(
        err.starts_with("OSL-VALIDATE-ERR:"),
        "error should carry the typed prefix: {err}"
    );
    let json_tail = &err["OSL-VALIDATE-ERR:".len()..];
    let parsed: serde_json::Value = serde_json::from_str(json_tail)
        .unwrap_or_else(|e| panic!("JSON tail did not parse: {e}; raw={err}"));
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("unreachable"),
        "expected kind=unreachable, got {parsed:?}"
    );

    // The cache file must NOT exist — F2.4's offline-grace
    // depends on a previous good cache, not on an empty one.
    assert!(
        !dir.path().join("license.json").exists(),
        "cache MUST NOT be written on transport failure"
    );
}

#[test]
fn validate_license_preserves_existing_cache_on_unreachable() {
    // Pre-seed a known-good cache. An unreachable validate must
    // NOT clobber it (this is the load-bearing property F2.4
    // builds on).
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache_with_status(dir.path(), "ACTIVE");
    let pre_bytes = std::fs::read(dir.path().join("license.json")).unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("http://127.0.0.1:{port}");
    let _ = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-NEW-KEY-VALUE-FAKEDATA".to_string(),
        dir.path(),
        &url,
    );

    let post_bytes = std::fs::read(dir.path().join("license.json")).unwrap();
    assert_eq!(
        pre_bytes, post_bytes,
        "cache bytes must be byte-identical after a transport failure"
    );
}

#[test]
fn validate_license_does_not_write_cache_on_http_error() {
    // 429 rate limit → Error::HttpStatus. F2.2 must NOT write the
    // cache (the keyserver answered but we didn't get a status).
    let response = http_status_response("429 Too Many Requests", r#"{"error":"rate_limited"}"#);
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");
    let result = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-2222-3333-4444-5555".to_string(),
        dir.path(),
        &url,
    );
    assert!(result.is_err());
    let err = result.unwrap_err();
    // F3.2: error shape is `OSL-VALIDATE-ERR:{json}` with
    // `kind = "rejected"`; the 429 body lands in the `body` field.
    assert!(
        err.starts_with("OSL-VALIDATE-ERR:"),
        "error should carry the typed prefix: {err}"
    );
    let json_tail = &err["OSL-VALIDATE-ERR:".len()..];
    let parsed: serde_json::Value = serde_json::from_str(json_tail)
        .unwrap_or_else(|e| panic!("JSON tail did not parse: {e}; raw={err}"));
    assert_eq!(
        parsed.get("kind").and_then(|v| v.as_str()),
        Some("rejected"),
        "expected kind=rejected, got {parsed:?}"
    );
    assert_eq!(
        parsed.get("status").and_then(|v| v.as_u64()),
        Some(429),
        "expected status=429, got {parsed:?}"
    );
    assert!(
        !dir.path().join("license.json").exists(),
        "cache MUST NOT be written on HttpStatus error"
    );
}

#[test]
fn validate_license_cache_round_trips_through_get_state() {
    // End-to-end check: validate(success) → get_license_state
    // returns the same status/period that we just persisted.
    let response = ok_response("GRACE", "1900000000", true);
    let (port, _rx) = one_shot_server(response);
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");
    cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-AAAA-BBBB-CCCC-DDDD".to_string(),
        dir.path(),
        &url,
    )
    .unwrap();

    let dto = cmd_osl_get_license_state_with_dir(&state, dir.path()).unwrap();
    // GRACE is a paid-tier status (Stripe payment-retry window).
    assert_eq!(dto.state, LicenseState::Paid);
    assert_eq!(dto.raw_status, "GRACE");
    assert_eq!(dto.current_period_end, Some(1_900_000_000));
    assert!(dto.last_validated_at.is_some());
}
