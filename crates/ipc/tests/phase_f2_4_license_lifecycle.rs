//! F2.4: refresh lifecycle + offline-grace + launch classify.
//!
//! Two layers under test:
//!   - `offline_grace_from_cache(cache, now)` — pure policy
//!     function exercised directly (no I/O, no network).
//!   - `refresh_license_state_with_url(state, dir, base_url)`
//!     — full sync path against an in-process mock keyserver.
//!     Asserts the cache-write policy, the AppState stamp, and
//!     the typed-Error dispatch and the shared seven-day stale-entitlement
//!     ceiling for Transport, HttpStatus, and malformed JSON failures.
//!
//! Also: the F2.4 cache-write tidy-up in cmd_osl_validate_license
//! (UNKNOWN / checksum_ok:false must NOT persist license.json).
//! Also: launch_classify synchronously stamps AppState.

use ipc::commands::cmd_osl_validate_license_with_dir_and_url;
use ipc::license_lifecycle::{
    launch_classify, offline_grace_from_cache, refresh_license_state_with_url,
};
use ipc::AppState;
use keystore::{
    save_license_cache, select_best_sealer, LicenseCacheInner, LicenseState, LicenseStateDto,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;
use tempfile::tempdir;

// ---- helpers ----

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One-shot mock HTTP server (same harness used by the F2.2
/// integration tests). Accepts one connection, captures the
/// request bytes, replies with `response`.
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

fn ok_response(status: &str, period_end: &str, checksum_ok: bool) -> Vec<u8> {
    let body = format!(
        r#"{{"status":"{status}","current_period_end":{period_end},"checksum_ok":{checksum_ok}}}"#
    );
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

fn http_429_response() -> Vec<u8> {
    let body = r#"{"error":"rate_limited"}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 429 Too Many Requests\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

fn malformed_json_response() -> Vec<u8> {
    let body = r#"{"status": "ACTIVE"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

fn seed_cache(dir: &std::path::Path, status: &str, last_validated_at: i64) {
    let inner = LicenseCacheInner {
        license_plaintext: "OSL-2222-3333-4444-5555".to_string(),
        last_validated_status: status.to_string(),
        current_period_end: Some(1_800_000_000),
        last_validated_at,
        checksum_ok: true,
    };
    let sealer = select_best_sealer();
    save_license_cache(&dir.join("license.json"), &inner, sealer.as_ref()).unwrap();
}

fn read_cache(dir: &std::path::Path) -> LicenseCacheInner {
    let sealer = select_best_sealer();
    keystore::load_license_cache(&dir.join("license.json"), sealer.as_ref())
        .expect("cache should still load")
}

const SEVEN_DAYS_SEC: i64 = 7 * 86_400;

// ---- offline_grace_from_cache: pure policy ----

#[test]
fn offline_grace_paid_status_within_window_returns_paid_offline_grace() {
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "ACTIVE".to_string(),
        current_period_end: Some(now + 30 * 86_400),
        last_validated_at: now - 3 * 86_400, // 3 days ago, well within grace
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);
    assert_eq!(dto.raw_status, "ACTIVE");
    assert_eq!(dto.last_validated_at, Some(now - 3 * 86_400));
}

#[test]
fn offline_grace_paid_status_at_window_boundary_still_grace() {
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "GRACE".to_string(),
        current_period_end: None,
        // last_validated_at + 7d - 1s — still inside the window.
        last_validated_at: now - SEVEN_DAYS_SEC + 1,
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);
}

#[test]
fn offline_grace_paid_status_past_window_slides_to_free() {
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "ACTIVE".to_string(),
        current_period_end: Some(now + 30 * 86_400),
        // 8 days ago — outside grace.
        last_validated_at: now - 8 * 86_400,
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::Free);
    // raw_status still ACTIVE — the UI may show "Subscription
    // unverified (offline >7 days)" but for now classify=Free.
    assert_eq!(dto.raw_status, "ACTIVE");
}

#[test]
fn offline_grace_cancelled_within_window_is_paid_grace() {
    // CANCELLED is paid-equivalent (period_end not yet passed),
    // so it gets the grace window too.
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "CANCELLED".to_string(),
        current_period_end: Some(now + 7 * 86_400),
        last_validated_at: now - 2 * 86_400,
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);
}

#[test]
fn offline_grace_expired_within_window_is_free_no_grace() {
    // EXPIRED is NOT paid-equivalent. The cache says "you used
    // to be paid but the period ended"; grace only extends a
    // currently-paid status across a brief keyserver outage.
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "EXPIRED".to_string(),
        current_period_end: Some(now - 86_400),
        last_validated_at: now - 60, // a minute ago, plenty fresh
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::Free);
}

#[test]
fn offline_grace_revoked_within_window_is_free() {
    let now = 1_700_000_000;
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-...".to_string(),
        last_validated_status: "REVOKED".to_string(),
        current_period_end: None,
        last_validated_at: now - 60,
        checksum_ok: true,
    };
    let dto = offline_grace_from_cache(&cache, now);
    assert_eq!(dto.state, LicenseState::Free);
}

// ---- refresh_license_state_with_url: full sync path ----

#[test]
fn refresh_success_writes_cache_and_stamps_appstate() {
    let response = ok_response("ACTIVE", "1900000000", true);
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    // Seed cache with stale data; the refresh must overwrite.
    seed_cache(dir.path(), "GRACE", unix_now() - 60);

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));

    assert_eq!(dto.state, LicenseState::Paid);
    assert_eq!(dto.raw_status, "ACTIVE");
    assert_eq!(dto.current_period_end, Some(1_900_000_000));
    // last_validated_at should be ~now (within 5s of test start)
    let now = unix_now();
    assert!(
        dto.last_validated_at.unwrap() >= now - 5,
        "expected last_validated_at to be recent"
    );

    // Cache on disk also got the refresh.
    let cache = read_cache(dir.path());
    assert_eq!(cache.last_validated_status, "ACTIVE");
    assert!(cache.last_validated_at >= now - 5);

    // AppState stamped.
    let app = state.license_state.lock().unwrap().clone();
    assert_eq!(app.state, LicenseState::Paid);
    assert_eq!(app.raw_status, "ACTIVE");
}

#[test]
fn refresh_transport_within_grace_returns_offline_grace() {
    // Bind+drop a port → connect-refused → Error::Transport.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    // Cache is fresh (1 hour ago), status ACTIVE.
    let last_validated = unix_now() - 3600;
    seed_cache(dir.path(), "ACTIVE", last_validated);
    let pre_bytes = std::fs::read(dir.path().join("license.json")).unwrap();

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));

    // Within-grace + paid status → PaidOfflineGrace.
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);
    // last_validated_at MUST NOT be bumped — that's what makes
    // the window eventually slide closed.
    assert_eq!(dto.last_validated_at, Some(last_validated));

    // Cache bytes unchanged.
    let post_bytes = std::fs::read(dir.path().join("license.json")).unwrap();
    assert_eq!(
        pre_bytes, post_bytes,
        "cache MUST be untouched on Transport error"
    );

    // AppState stamped.
    assert_eq!(
        state.license_state.lock().unwrap().state,
        LicenseState::PaidOfflineGrace
    );
}

#[test]
fn refresh_transport_past_grace_returns_free() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    // 8 days ago — past grace.
    let last_validated = unix_now() - 8 * 86_400;
    seed_cache(dir.path(), "ACTIVE", last_validated);

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));
    assert_eq!(dto.state, LicenseState::Free);
    // Cache still preserves the original last_validated_at.
    let cache = read_cache(dir.path());
    assert_eq!(cache.last_validated_at, last_validated);
}

#[test]
fn refresh_transport_with_expired_cache_returns_free_no_grace() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    // Fresh cache (1 minute ago) but status is EXPIRED — grace
    // only applies to ACTIVE/CANCELLED/GRACE.
    seed_cache(dir.path(), "EXPIRED", unix_now() - 60);

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));
    assert_eq!(dto.state, LicenseState::Free);
}

#[test]
fn refresh_http_status_within_grace_returns_bounded_grace() {
    // A 429 is not an authoritative entitlement result. It may use the recent
    // cache, but it must not refresh the cache timestamp.
    let response = http_429_response();
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache(dir.path(), "ACTIVE", unix_now() - 60);

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);
    assert_eq!(dto.raw_status, "ACTIVE");

    // Cache untouched on HttpStatus too.
    let cache = read_cache(dir.path());
    assert_eq!(cache.last_validated_status, "ACTIVE");
}

#[test]
fn refresh_http_status_past_grace_returns_free() {
    let (port, _rx) = one_shot_server(http_429_response());
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let last_validated = unix_now() - 8 * 86_400;
    seed_cache(dir.path(), "ACTIVE", last_validated);

    let dto =
        refresh_license_state_with_url(&state, dir.path(), &format!("http://127.0.0.1:{port}"));
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(read_cache(dir.path()).last_validated_at, last_validated);
}

#[test]
fn refresh_malformed_json_obeys_same_grace_ceiling() {
    let state = AppState::new();
    let fresh = tempdir().unwrap();
    seed_cache(fresh.path(), "ACTIVE", unix_now() - 60);
    let (fresh_port, _fresh_rx) = one_shot_server(malformed_json_response());
    let dto = refresh_license_state_with_url(
        &state,
        fresh.path(),
        &format!("http://127.0.0.1:{fresh_port}"),
    );
    assert_eq!(dto.state, LicenseState::PaidOfflineGrace);

    let stale = tempdir().unwrap();
    let last_validated = unix_now() - 8 * 86_400;
    seed_cache(stale.path(), "ACTIVE", last_validated);
    let (stale_port, _stale_rx) = one_shot_server(malformed_json_response());
    let dto = refresh_license_state_with_url(
        &state,
        stale.path(),
        &format!("http://127.0.0.1:{stale_port}"),
    );
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(read_cache(stale.path()).last_validated_at, last_validated);
}

#[test]
fn refresh_invalid_endpoint_obeys_same_grace_ceiling() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache(dir.path(), "ACTIVE", unix_now() - 8 * 86_400);

    let dto = refresh_license_state_with_url(&state, dir.path(), "not a valid URL");
    assert_eq!(dto.state, LicenseState::Free);
}

#[test]
fn refresh_no_cache_returns_free_unconfigured() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    // No cache seeded.
    let dto = refresh_license_state_with_url(&state, dir.path(), "http://127.0.0.1:1");
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "Unconfigured");
    // AppState too.
    assert_eq!(
        state.license_state.lock().unwrap().raw_status,
        "Unconfigured"
    );
}

// ---- launch_classify: synchronous cache-only ----

#[test]
fn launch_classify_no_cache_stamps_unconfigured() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    launch_classify(&state, dir.path());
    let dto = state.license_state.lock().unwrap().clone();
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "Unconfigured");
}

#[test]
fn launch_classify_cached_active_stamps_paid() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache(dir.path(), "ACTIVE", unix_now() - 60);
    launch_classify(&state, dir.path());
    let dto = state.license_state.lock().unwrap().clone();
    assert_eq!(dto.state, LicenseState::Paid);
    assert_eq!(dto.raw_status, "ACTIVE");
}

#[test]
fn launch_classify_never_produces_paid_offline_grace() {
    // The PaidOfflineGrace state requires a failed online
    // attempt; launch_classify is cache-only so it must never
    // produce it. Even a paid-equivalent + stale cache classifies
    // as Paid (the async refresh decides whether to flip to
    // PaidOfflineGrace later).
    let state = AppState::new();
    let dir = tempdir().unwrap();
    // 10 days ago — well past grace, but launch_classify doesn't
    // care about the window.
    seed_cache(dir.path(), "ACTIVE", unix_now() - 10 * 86_400);
    launch_classify(&state, dir.path());
    let dto = state.license_state.lock().unwrap().clone();
    assert!(matches!(dto.state, LicenseState::Paid | LicenseState::Free));
    assert_ne!(dto.state, LicenseState::PaidOfflineGrace);
}

#[test]
fn launch_classify_is_idempotent() {
    // Stamping is a Mutex write; running twice should be safe
    // and converge on the same state.
    let state = AppState::new();
    let dir = tempdir().unwrap();
    seed_cache(dir.path(), "GRACE", unix_now() - 60);
    launch_classify(&state, dir.path());
    let first = state.license_state.lock().unwrap().clone();
    launch_classify(&state, dir.path());
    let second = state.license_state.lock().unwrap().clone();
    assert_eq!(first, second);
}

// ---- F2.4 cache-write tidy-up: UNKNOWN / checksum_ok:false ----

#[test]
fn validate_with_unknown_status_does_not_write_cache() {
    // Server returned 200 with status:"UNKNOWN" (user mistyped
    // a key the keyserver doesn't recognise). The cache MUST NOT
    // be written — otherwise we'd leave a junk license.json on
    // disk that launch_classify / refresh would then re-classify
    // as Free, masking the real "no license entered" state.
    let response = ok_response("UNKNOWN", "null", true);
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
    .expect("validate itself should succeed (HTTP 200)");
    // The DTO surfaces UNKNOWN so the UI can show the right copy.
    assert_eq!(resp.status, "UNKNOWN");

    // Cache MUST NOT exist.
    assert!(
        !dir.path().join("license.json").exists(),
        "UNKNOWN response must NOT write the cache"
    );
}

#[test]
fn validate_with_bad_checksum_does_not_write_cache() {
    // 200 with checksum_ok:false — same "user mistyped" outcome.
    let response = ok_response("UNKNOWN", "null", false);
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");
    let _ = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-MIST-YPED-XXXX-YYYY".to_string(),
        dir.path(),
        &url,
    )
    .unwrap();
    assert!(
        !dir.path().join("license.json").exists(),
        "checksum_ok:false must NOT write the cache"
    );
}

#[test]
fn validate_with_active_status_does_write_cache() {
    // Sanity counter-test: a real ACTIVE response DOES write.
    let response = ok_response("ACTIVE", "1900000000", true);
    let (port, _rx) = one_shot_server(response);

    let state = AppState::new();
    let dir = tempdir().unwrap();
    let url = format!("http://127.0.0.1:{port}");
    let _ = cmd_osl_validate_license_with_dir_and_url(
        &state,
        "OSL-2222-3333-4444-5555".to_string(),
        dir.path(),
        &url,
    )
    .unwrap();
    assert!(
        dir.path().join("license.json").exists(),
        "ACTIVE response MUST write the cache"
    );

    // And AppState should be stamped to Paid immediately (the
    // hot-path validate command keeps AppState fresh too).
    assert_eq!(
        state.license_state.lock().unwrap().state,
        LicenseState::Paid
    );
}

// ---- AppState defaults ----

#[test]
fn fresh_appstate_defaults_to_unconfigured() {
    // Without any launch hook, a brand-new AppState reads as
    // Free/Unconfigured. F3 reading this before the launch hook
    // ran will see "Free" (correct fallback).
    let state = AppState::new();
    let dto = state.license_state.lock().unwrap().clone();
    assert_eq!(dto, LicenseStateDto::unconfigured());
}
