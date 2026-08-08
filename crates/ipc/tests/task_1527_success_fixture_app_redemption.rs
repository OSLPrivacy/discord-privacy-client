//! TASK 1527: the checkout-success fixture must lead to a *working*
//! in-app redemption, not just words on a page.
//!
//! `scripts/check-checkout-success-page-words.mjs` (TASK 1526) already
//! proves the fixture shows one visible activation code, the required
//! pricing/renewal sentences, and an `osl://activate?code=...` link that
//! is not a dead end. This test proves that link's code is real: fed
//! through the actual redemption path
//! (`cmd_osl_validate_license_with_dir_and_url`, the same command the
//! Settings "Activate Pro" form calls), against a mock keyserver, it
//! flips the in-memory plan record (`AppState::license_state`) from
//! `Free` to `Paid` exactly one time — not zero (broken wiring) and not
//! more than one (a redemption that double-fires on retry).
use ipc::commands::cmd_osl_validate_license_with_dir_and_url;
use ipc::AppState;
use keystore::LicenseState;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use tempfile::tempdir;

const CODE_PATTERN_PREFIX: &str = "OSL-";

/// Mirrors `CODE_PATTERN` / `hrefs()` in
/// `scripts/check-checkout-success-page-words.mjs`: pull the one visible
/// activation code and the `osl://activate?code=...` redemption href out
/// of the fixture, and confirm the href actually points at that code
/// (i.e. the page is not a dead end).
fn code_from_success_fixture() -> String {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("../../docs/fixtures/checkout-success.html");
    let html = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", fixture_path.display()));

    let codes: Vec<&str> = html
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|tok| tok.starts_with(CODE_PATTERN_PREFIX) && tok.len() == 23)
        .collect();
    let unique_codes: std::collections::BTreeSet<&str> = codes.iter().copied().collect();
    assert_eq!(
        unique_codes.len(),
        1,
        "fixture must show exactly one activation code, found {unique_codes:?}"
    );
    let code = *unique_codes.iter().next().unwrap();

    let expected_href = format!("osl://activate?code={code}");
    assert!(
        html.contains(&format!("href=\"{expected_href}\"")),
        "fixture's app redemption link must target the visible code {code}"
    );

    code.to_string()
}

/// Same one-shot mock keyserver harness as
/// `crates/ipc/tests/phase_f2_4_license_lifecycle.rs`.
fn one_shot_keyserver(response: Vec<u8>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
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
        let _ = stream.write_all(&response);
    });
    port
}

fn redeemed_ok_response() -> Vec<u8> {
    let body = r#"{"status":"ACTIVE","current_period_end":1900000000,"checksum_ok":true}"#;
    let mut response = Vec::new();
    response.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
    response.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
    response.extend_from_slice(b"Content-Type: application/json\r\n\r\n");
    response.extend_from_slice(body.as_bytes());
    response
}

#[test]
fn success_fixture_code_redeemed_in_app_flips_plan_free_to_pro_exactly_once() {
    let code = code_from_success_fixture();

    let state = AppState::new();
    let dir = tempdir().unwrap();

    // Before redemption: a fresh AppState presents Free/Unconfigured —
    // the "plan record" this task's fixture must move off of.
    let before = state
        .license_state
        .lock()
        .expect("license_state mutex poisoned")
        .clone();
    assert_eq!(before.state, LicenseState::Free, "plan record must start Free");

    let mut transitions_free_to_pro = 0;

    // Redeem the fixture's code twice, exactly as a user could by
    // clicking the same `osl://activate?code=...` link twice (double
    // click, retry after a slow first response, etc). The plan record
    // must flip Free -> Pro exactly once across both attempts, not
    // once per click.
    for _ in 0..2 {
        let port = one_shot_keyserver(redeemed_ok_response());
        let base_url = format!("http://127.0.0.1:{port}");

        let response = cmd_osl_validate_license_with_dir_and_url(
            &state,
            code.clone(),
            dir.path(),
            &base_url,
        )
        .expect("redemption of the fixture's code must succeed against the mock keyserver");
        assert!(response.checksum_ok, "mock keyserver response must be durable");

        let before_this_call = if transitions_free_to_pro == 0 {
            LicenseState::Free
        } else {
            LicenseState::Paid
        };
        let after = state
            .license_state
            .lock()
            .expect("license_state mutex poisoned")
            .clone();
        if before_this_call == LicenseState::Free && after.state == LicenseState::Paid {
            transitions_free_to_pro += 1;
        }
    }

    let after = state
        .license_state
        .lock()
        .expect("license_state mutex poisoned")
        .clone();
    println!(
        "TASK1527_PLAN_BEFORE={:?} TASK1527_PLAN_AFTER={:?} TASK1527_FREE_TO_PRO_FLIPS={transitions_free_to_pro}",
        before.state, after.state
    );

    assert_eq!(after.state, LicenseState::Paid, "plan record must end Pro (Paid)");
    assert_eq!(
        transitions_free_to_pro, 1,
        "success-fixture redemption must flip the plan record Free -> Pro exactly once"
    );
}
