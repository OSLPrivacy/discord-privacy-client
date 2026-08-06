//! OSL Chat's pointer-arrival receive boundary.
//!
//! This deliberately is only a lane adapter.  The shared eager-fetch driver
//! owns the reservation/retry and persistence ordering; OSL Chat must invoke
//! it when an authenticated pointer arrives, never when its conversation view
//! happens to be open.

use crate::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, LocalMessageStore, PointerArrival,
};
use crate::realtime_client::AuthorizedFetch;

pub const OSL_CHAT_POINTER_PATH_ENV: &str = "OSL_CHAT_POINTER_PATH";

pub fn osl_chat_pointer_path_enabled_from_env() -> bool {
    std::env::var(OSL_CHAT_POINTER_PATH_ENV)
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

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

pub fn receive_osl_chat_authorized_fetch<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    fetch: AuthorizedFetch,
    pointer_path_enabled: bool,
) -> Result<bool, String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    if !pointer_path_enabled {
        return Ok(false);
    }
    fetch.fetch_with(|blob_id, bearer_capability| {
        receive_osl_chat_pointer(
            driver,
            &PointerArrival {
                blob_id: blob_id.to_hex(),
                fetch_cap: bearer_capability.as_bytes().to_vec(),
                manage_cap: Vec::new(),
            },
        )?;
        Ok::<(), String>(())
    })?;
    Ok(true)
}
