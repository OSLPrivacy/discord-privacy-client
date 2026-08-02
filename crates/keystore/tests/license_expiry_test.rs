use keystore::is_license_expired;

#[test]
fn local_prepaid_expiry_ends_at_the_cached_timestamp() {
    let now = 1_800_000_000;

    assert!(!is_license_expired(Some(now + 1), now));
    assert!(is_license_expired(Some(now), now));
    assert!(is_license_expired(Some(now - 1), now));
}

#[test]
fn cache_without_prepaid_expiry_is_not_locally_expired() {
    assert!(!is_license_expired(None, 1_800_000_000));
}
