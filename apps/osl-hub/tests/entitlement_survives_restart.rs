#![cfg(feature = "core")]

use osl_privacy_hub::core_bridge::{license_state, HubCoreState};

#[test]
fn paid_sealed_entitlement_is_available_after_bootstrap_without_network() {
    let unique = format!(
        "osl-hub-entitlement-restart-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos()
    );
    let base_dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&base_dir).expect("create isolated entitlement directory");

    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(base_dir.clone()));
    let entitlement = keystore::LicenseCacheInner {
        license_plaintext: "OSL-2222-3333-4444-5555".to_owned(),
        last_validated_status: "ACTIVE".to_owned(),
        current_period_end: Some(1_800_000_000),
        last_validated_at: 1_700_000_000,
        checksum_ok: true,
    };
    let sealer = keystore::select_best_sealer();
    keystore::save_license_cache(
        &base_dir.join("license.json"),
        &entitlement,
        sealer.as_ref(),
    )
    .expect("seal paid entitlement");

    let state = HubCoreState::bootstrap_from_disk();
    let license = license_state(&state).expect("read bootstrapped entitlement");

    keystore::set_base_dir_override(None);
    std::fs::remove_dir_all(&base_dir).expect("remove isolated entitlement directory");

    assert_eq!(license.access, "pro");
    assert_eq!(license.status, "ACTIVE");
}
