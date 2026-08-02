//! Local prepaid-license expiry policy.
//!
//! The redeemed entitlement end is sealed in `license.json`, so it remains
//! available before any network activity.  The local clock is authoritative
//! for this limited offline check: once the cached period has ended, optional
//! paid access must not survive merely because the last server status was
//! `ACTIVE`.

/// Returns true when a cached prepaid entitlement has reached its end.
///
/// Older cache shapes and non-prepaid records have no local expiry, so `None`
/// deliberately remains non-expired.  An entitlement ending at `now` is no
/// longer valid; the valid interval is `[redeemed_at, expires_at)`.
pub fn is_license_expired(expires_at: Option<i64>, now: i64) -> bool {
    expires_at.is_some_and(|expires_at| expires_at <= now)
}
