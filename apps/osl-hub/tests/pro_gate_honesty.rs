//! T16-E6: a missing Mass Cleanup adapter must not look like a tier refusal.

use ipc::AppState;
use keystore::{LicenseState, LicenseStateDto};
use osl_privacy_hub::{
    mass_cleanup::{discover_targets, MassCleanupDiscoveryRequest},
    models::ServiceKind,
};

const PRO_REQUIRED: &str = "Mass Cleanup requires an active Pro license";
const DISCOVERY_UNAVAILABLE: &str =
    "Mass Cleanup discovery is unavailable because no reviewed local service adapter is installed";

fn state_with_license(state: LicenseState, raw_status: &str) -> AppState {
    let state_holder = AppState::new();
    *state_holder
        .license_state
        .lock()
        .expect("license state lock") = LicenseStateDto {
        state,
        raw_status: raw_status.to_owned(),
        current_period_end: None,
        last_validated_at: None,
    };
    state_holder
}

fn reviewed_discovery_request() -> MassCleanupDiscoveryRequest {
    MassCleanupDiscoveryRequest {
        service_id: ServiceKind::Telegram,
        account_id: "acct-telegram-1".to_owned(),
    }
}

#[test]
fn mass_cleanup_names_the_missing_adapter_to_pro_and_only_pro_is_tier_refused() {
    for (license, raw_status) in [
        (LicenseState::Paid, "ACTIVE"),
        (LicenseState::PaidOfflineGrace, "OFFLINE_GRACE"),
    ] {
        let state = state_with_license(license, raw_status);
        assert_eq!(
            discover_targets(&state, reviewed_discovery_request()).unwrap_err(),
            DISCOVERY_UNAVAILABLE,
            "an entitled install must be told that this build lacks a reviewed adapter"
        );
    }

    let free = state_with_license(LicenseState::Free, "EXPIRED");
    assert_eq!(
        discover_targets(&free, reviewed_discovery_request()).unwrap_err(),
        PRO_REQUIRED,
        "only a Free install should receive the tier refusal"
    );
}
