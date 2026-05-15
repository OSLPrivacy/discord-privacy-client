//! F2.2: license cache + LicenseState classification.

use keystore::{
    classify_state, load_license_cache, save_license_cache, Error, LicenseCacheInner,
    LicenseCacheOnDisk, LicenseState, LicenseStateDto, MemorySealer, NoOpSealer, Sealer,
};
use tempfile::tempdir;

fn make_inner() -> LicenseCacheInner {
    LicenseCacheInner {
        license_plaintext: "OSL-2222-3333-4444-5555".to_string(),
        last_validated_status: "ACTIVE".to_string(),
        current_period_end: Some(1_800_000_000),
        last_validated_at: 1_700_000_000,
        checksum_ok: true,
    }
}

// ---- round-trip ----

#[test]
fn save_then_load_returns_same_inner() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = MemorySealer::new();
    let inner = make_inner();
    save_license_cache(&path, &inner, &sealer).unwrap();
    let loaded = load_license_cache(&path, &sealer).unwrap();
    assert_eq!(loaded, inner);
}

#[test]
fn round_trip_preserves_optional_fields_when_null() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = MemorySealer::new();
    let inner = LicenseCacheInner {
        license_plaintext: "OSL-AAAA-BBBB-CCCC-DDDD".to_string(),
        last_validated_status: "PENDING".to_string(),
        current_period_end: None,
        last_validated_at: 0,
        checksum_ok: false,
    };
    save_license_cache(&path, &inner, &sealer).unwrap();
    let loaded = load_license_cache(&path, &sealer).unwrap();
    assert_eq!(loaded.current_period_end, None);
    assert!(!loaded.checksum_ok);
}

#[test]
fn plaintext_is_not_visible_in_on_disk_wrapper() {
    // The cache must NOT have the license string anywhere outside
    // the AEAD-sealed sealed_b64. The outer wrapper carries version
    // + method + sealed_b64 (and optionally insecure_banner).
    // Sanity check the bytes on disk against the user-visible
    // plaintext.
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = MemorySealer::new();
    let inner = make_inner();
    save_license_cache(&path, &inner, &sealer).unwrap();
    let raw_bytes = std::fs::read(&path).unwrap();
    let raw = std::str::from_utf8(&raw_bytes).unwrap();
    assert!(
        !raw.contains("OSL-2222-3333-4444-5555"),
        "plaintext license leaked into outer wrapper:\n{raw}"
    );
    assert!(
        !raw.contains("ACTIVE"),
        "status leaked into outer wrapper:\n{raw}"
    );
}

// ---- tamper + error variants ----

#[test]
fn tampered_ciphertext_rejected_on_load() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = MemorySealer::new();
    save_license_cache(&path, &make_inner(), &sealer).unwrap();

    // Mutate one byte of `sealed_b64` so the AEAD tag check fails.
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut wrapper: LicenseCacheOnDisk = serde_json::from_str(&raw).unwrap();
    let mut bytes: Vec<u8> = wrapper.sealed_b64.bytes().collect();
    // Flip a high-bit char in the middle of the base64 string —
    // picking somewhere that decodes into the AEAD ciphertext body.
    let mid = bytes.len() / 2;
    bytes[mid] = if bytes[mid] == b'A' { b'B' } else { b'A' };
    wrapper.sealed_b64 = String::from_utf8(bytes).unwrap();
    std::fs::write(&path, serde_json::to_vec(&wrapper).unwrap()).unwrap();

    match load_license_cache(&path, &sealer) {
        Err(Error::Sealer(_)) => {}
        Err(other) => panic!("expected Error::Sealer on AEAD tamper, got {other:?}"),
        Ok(_) => panic!("tampered cache must NOT unseal"),
    }
}

#[test]
fn version_mismatch_returns_blob_version_mismatch_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = MemorySealer::new();
    save_license_cache(&path, &make_inner(), &sealer).unwrap();

    // Bump version on disk to a value the loader rejects.
    let raw = std::fs::read_to_string(&path).unwrap();
    let mut wrapper: LicenseCacheOnDisk = serde_json::from_str(&raw).unwrap();
    wrapper.version = 999;
    std::fs::write(&path, serde_json::to_vec(&wrapper).unwrap()).unwrap();

    match load_license_cache(&path, &sealer) {
        Err(Error::BlobVersionMismatch { got, expected }) => {
            assert_eq!(got, 999);
            assert_eq!(expected, 1);
        }
        other => panic!("expected BlobVersionMismatch, got {other:?}"),
    }
}

#[test]
fn method_mismatch_returns_blob_method_mismatch_error() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let save_sealer = MemorySealer::new();
    save_license_cache(&path, &make_inner(), &save_sealer).unwrap();

    // Try to load with a different sealer (NoOp); the method tag
    // on disk says "memory-test", the active sealer says
    // "noop-insecure". Loader must refuse.
    let load_sealer = NoOpSealer::new();
    match load_license_cache(&path, &load_sealer) {
        Err(Error::BlobMethodMismatch { got, expected }) => {
            assert_eq!(got, "memory-test");
            assert_eq!(expected, "noop-insecure");
        }
        other => panic!("expected BlobMethodMismatch, got {other:?}"),
    }
}

#[test]
fn missing_file_returns_io_not_found() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let sealer = MemorySealer::new();
    match load_license_cache(&path, &sealer) {
        Err(Error::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotFound),
        other => panic!("expected Io NotFound, got {other:?}"),
    }
}

#[test]
fn noop_sealer_path_writes_insecure_banner() {
    // The noop-insecure sealer should set the wrapper's
    // `insecure_banner` field. Operators can grep the file
    // without unsealing.
    let dir = tempdir().unwrap();
    let path = dir.path().join("license.json");
    let sealer = NoOpSealer::new();
    save_license_cache(&path, &make_inner(), &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let wrapper: LicenseCacheOnDisk = serde_json::from_str(&raw).unwrap();
    assert!(wrapper.insecure_banner.is_some());
    assert_eq!(wrapper.method, "noop-insecure");
}

// ---- classify_state ----

#[test]
fn classify_state_maps_paid_statuses() {
    assert_eq!(classify_state("ACTIVE"), LicenseState::Paid);
    assert_eq!(classify_state("CANCELLED"), LicenseState::Paid);
    assert_eq!(classify_state("GRACE"), LicenseState::Paid);
}

#[test]
fn classify_state_maps_free_statuses() {
    assert_eq!(classify_state("EXPIRED"), LicenseState::Free);
    assert_eq!(classify_state("REVOKED"), LicenseState::Free);
    assert_eq!(classify_state("UNKNOWN"), LicenseState::Free);
    assert_eq!(classify_state("PENDING"), LicenseState::Free);
}

#[test]
fn classify_state_unknown_strings_map_to_free() {
    // Defensive: a future keyserver-side status string we haven't
    // wired up should NOT unlock paid features. Default to Free.
    assert_eq!(classify_state("FUTURE_UNKNOWN_STATE"), LicenseState::Free);
    assert_eq!(classify_state(""), LicenseState::Free);
}

// ---- LicenseStateDto ----

#[test]
fn dto_from_active_cache_maps_to_paid() {
    let inner = make_inner();
    let dto = LicenseStateDto::from_cache(&inner);
    assert_eq!(dto.state, LicenseState::Paid);
    assert_eq!(dto.raw_status, "ACTIVE");
    assert_eq!(dto.current_period_end, Some(1_800_000_000));
    assert_eq!(dto.last_validated_at, Some(1_700_000_000));
}

#[test]
fn dto_from_expired_cache_maps_to_free() {
    let mut inner = make_inner();
    inner.last_validated_status = "EXPIRED".to_string();
    let dto = LicenseStateDto::from_cache(&inner);
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "EXPIRED");
}

#[test]
fn dto_unconfigured_is_free_with_marker_status() {
    let dto = LicenseStateDto::unconfigured();
    assert_eq!(dto.state, LicenseState::Free);
    assert_eq!(dto.raw_status, "Unconfigured");
    assert_eq!(dto.current_period_end, None);
    assert_eq!(dto.last_validated_at, None);
}

#[test]
fn paid_offline_grace_variant_exists_for_f24() {
    // The F2.2 classifier never produces PaidOfflineGrace; F2.4
    // overlays it. But the enum variant must exist so F2.4 doesn't
    // have to amend the enum. This test pins the surface.
    let _ = LicenseState::PaidOfflineGrace;
}

// Force the unused-import warning out for Sealer (re-export sanity).
#[test]
fn sealer_trait_re_exported_at_crate_root() {
    fn _accepts_dyn_sealer(_: &dyn Sealer) {}
}
