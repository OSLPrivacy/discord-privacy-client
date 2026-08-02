//! OSL Chat's pointer-arrival receive boundary.
//!
//! This deliberately is only a lane adapter.  The shared eager-fetch driver
//! owns the reservation/retry and persistence ordering; OSL Chat must invoke
//! it when an authenticated pointer arrives, never when its conversation view
//! happens to be open.

use crate::eager_fetch::{CipherStoreTransport, EagerFetchDriver, LocalMessageStore, PointerArrival};

/// Fetch, authenticate/decrypt, and durably persist an OSL Chat payload on
/// pointer arrival.  ACK ownership remains with the post-persistence path.
pub fn receive_osl_chat_pointer<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    pointer: &PointerArrival,
) -> Result<(), String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    driver.on_pointer_arrival(pointer)
}
