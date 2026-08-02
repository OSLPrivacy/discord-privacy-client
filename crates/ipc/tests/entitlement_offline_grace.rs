//! T16-T14: Offline entitlement grace remains bounded.
//!
//! An unreachable keyserver may retain a recently validated paid entitlement
//! for seven days, but must not advance the durable validation timestamp. The
//! same cache therefore becomes Free after the boundary. A clock rollback is
//! intentionally treated as grace rather than locking out a traveller.

use ipc::license_lifecycle::{offline_grace_from_cache, refresh_license_state_with_url};
use ipc::AppState;
use keystore::{save_license_cache, select_best_sealer, LicenseCacheInner, LicenseState};
use std::net::TcpListener;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

const DAY: i64 = 86_400;

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_secs() as i64
}

fn unreachable_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve a local port");
    let port = listener.local_addr().expect("read local port").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn paid_cache(last_validated_at: i64) -> LicenseCacheInner {
    LicenseCacheInner {
        license_plaintext: "OSL-2222-3333-4444-5555".to_string(),
        last_validated_status: "ACTIVE".to_string(),
        redeemed_at: None,
        expires_at: None,
        current_period_end: Some(unix_now() + 30 * DAY),
        last_validated_at,
        checksum_ok: true,
    }
}

fn write_cache(dir: &std::path::Path, cache: &LicenseCacheInner) {
    let sealer = select_best_sealer();
    save_license_cache(&dir.join("license.json"), cache, sealer.as_ref())
        .expect("seal paid license cache");
}

fn read_cache(dir: &std::path::Path) -> LicenseCacheInner {
    let sealer = select_best_sealer();
    keystore::load_license_cache(&dir.join("license.json"), sealer.as_ref())
        .expect("load paid license cache")
}

#[test]
fn unreachable_validation_is_grace_for_six_days_then_free_on_day_eight() {
    let now = unix_now();
    let url = unreachable_url();

    let within_grace = tempdir().expect("temporary config directory");
    let six_day_timestamp = now - 6 * DAY;
    write_cache(within_grace.path(), &paid_cache(six_day_timestamp));
    let six_day_state = refresh_license_state_with_url(&AppState::new(), within_grace.path(), &url);
    assert_eq!(six_day_state.state, LicenseState::PaidOfflineGrace);
    assert_eq!(six_day_state.last_validated_at, Some(six_day_timestamp));
    assert_eq!(
        read_cache(within_grace.path()).last_validated_at,
        six_day_timestamp
    );

    let past_grace = tempdir().expect("temporary config directory");
    let eight_day_timestamp = now - 8 * DAY;
    write_cache(past_grace.path(), &paid_cache(eight_day_timestamp));
    let eight_day_state = refresh_license_state_with_url(&AppState::new(), past_grace.path(), &url);
    assert_eq!(eight_day_state.state, LicenseState::Free);
    assert_eq!(eight_day_state.last_validated_at, Some(eight_day_timestamp));
    assert_eq!(
        read_cache(past_grace.path()).last_validated_at,
        eight_day_timestamp
    );
}

#[test]
fn clock_rollback_keeps_a_paid_traveller_in_offline_grace() {
    let last_validated_at = 1_700_000_000;
    let cache = paid_cache(last_validated_at);

    // A backwards local clock cannot distinguish tampering from an honest
    // traveller correcting time. It must not turn optional access off.
    let rolled_back_now = last_validated_at - 30 * DAY;
    let state = offline_grace_from_cache(&cache, rolled_back_now);

    assert_eq!(state.state, LicenseState::PaidOfflineGrace);
    assert_eq!(state.last_validated_at, Some(last_validated_at));
}
