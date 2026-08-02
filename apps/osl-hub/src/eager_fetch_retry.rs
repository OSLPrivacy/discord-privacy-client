//! Reservation-safe eager-fetch retries.
//!
//! The cipher-store adapter behind the closure must use T2-40's blob
//! reservation endpoint.  That reservation is created before any bytes are
//! read and covers every retry; this helper deliberately never treats a
//! transient read error as a consume/delete decision.

/// One initial request plus two immediate retries keeps unattended eager fetch
/// useful on ordinary short network failures without turning it into an
/// unbounded background loop.
pub const RESERVED_FETCH_ATTEMPTS: usize = 3;

/// Retry a fetch that is already protected by the server-side reservation.
///
/// The last transport error is returned unchanged.  The helper owns neither a
/// capability nor an identity, so it cannot accidentally alter the anonymous
/// reservation contract.
pub fn retry_reserved_fetch<F>(mut fetch: F) -> Result<Vec<u8>, String>
where
    F: FnMut() -> Result<Vec<u8>, String>,
{
    let mut last_error = None;
    for _ in 0..RESERVED_FETCH_ATTEMPTS {
        match fetch() {
            Ok(bytes) => return Ok(bytes),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.expect("reserved fetch always attempts at least once"))
}

#[cfg(test)]
mod tests {
    use super::{retry_reserved_fetch, RESERVED_FETCH_ATTEMPTS};

    #[test]
    fn returns_the_final_transient_error_after_the_bounded_attempts() {
        let mut attempts = 0;
        let error = retry_reserved_fetch(|| {
            attempts += 1;
            Err("network dropped".to_owned())
        })
        .unwrap_err();
        assert_eq!(attempts, RESERVED_FETCH_ATTEMPTS);
        assert_eq!(error, "network dropped");
    }
}
