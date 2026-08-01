//! T16-G1: payment provenance must stop at the license-validation boundary.
//!
//! Each mock response carries a different server-only payment marker. The
//! client must reduce all three to the same validation response, sealed cache,
//! and DTO exposed to the OSL UI.

use ipc::commands::cmd_osl_validate_license_with_dir_and_url;
use keystore::{
    load_license_cache, select_best_sealer, LicenseCacheInner, LicenseValidateResponse,
};
use osl_privacy_hub::core_bridge::{license_state, HubCoreState, HubLicenseState};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq, Eq)]
struct ClientView {
    validation_response: LicenseValidateResponse,
    sealed_cache: LicenseCacheInner,
    app_dto: HubLicenseState,
}

fn temp_dir(payment_method: &str) -> PathBuf {
    let unique = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "osl-payment-method-invisible-{payment_method}-{now}-{unique}"
    ));
    std::fs::create_dir_all(&path).expect("create isolated license cache directory");
    path
}

fn mock_validate_server(payment_method: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock validation server");
    let address = listener.local_addr().expect("mock server address");
    let payment_method = payment_method.to_owned();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept validation request");
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set request timeout");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("read validation request");
        assert!(read > 0, "client must send a validation request");

        let body = format!(
            r#"{{"status":"ACTIVE","current_period_end":1800000000,"checksum_ok":true,"payment_method":"{payment_method}"}}"#
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write validation response");
    });
    (format!("http://{address}"), worker)
}

fn validate_as(payment_method: &str) -> ClientView {
    let cache_dir = temp_dir(payment_method);
    let (url, server) = mock_validate_server(payment_method);
    let state = HubCoreState::default();
    let validation_response = cmd_osl_validate_license_with_dir_and_url(
        &state.osl,
        "OSL-AB12-CD34-EF56-GH78".to_owned(),
        &cache_dir,
        &url,
    )
    .expect("payment provenance must not prevent activation");
    server.join().expect("mock validation server must finish");

    let sealer = select_best_sealer();
    let sealed_cache = load_license_cache(&cache_dir.join("license.json"), sealer.as_ref())
        .expect("durable validation result must be sealed locally");
    let app_dto = license_state(&state).expect("app-facing license DTO");
    std::fs::remove_dir_all(&cache_dir).expect("remove isolated license cache directory");

    ClientView {
        validation_response,
        sealed_cache,
        app_dto,
    }
}

#[test]
fn btc_xmr_and_card_payment_provenance_are_invisible_to_the_app() {
    let mut views = [validate_as("btc"), validate_as("xmr"), validate_as("card")];

    // Validation time naturally differs between independent requests. It is a
    // local observation time, not payment provenance, so normalise only that
    // value before comparing every remaining stored field and DTO field.
    for view in &mut views {
        assert!(view.sealed_cache.last_validated_at > 0);
        assert!(view.app_dto.last_validated_at.is_some());
        view.sealed_cache.last_validated_at = 0;
        view.app_dto.last_validated_at = None;
    }

    assert_eq!(
        views[0], views[1],
        "BTC and XMR must have the same client view"
    );
    assert_eq!(
        views[1], views[2],
        "XMR and card must have the same client view"
    );
}
