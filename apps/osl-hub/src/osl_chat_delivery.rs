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
#[cfg(feature = "core")]
use crate::shipping_receive::ShippingProtectedOverlay;
use crate::shipping_receive::{self, ArrivedMessageRow, ShippingReceiveJournal};

pub const OSL_CHAT_POINTER_PATH_ENV: &str = "OSL_CHAT_POINTER_PATH";

pub fn osl_chat_pointer_path_enabled_from_env() -> bool {
    std::env::var(OSL_CHAT_POINTER_PATH_ENV)
        .map(|value| value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(true)
}

/// Fetch, authenticate/decrypt, and durably persist an OSL Chat payload on
/// pointer arrival.  ACK ownership remains with the post-persistence path.
pub fn route_osl_chat_arrival<P, F>(
    carrier_row_id: impl Into<String>,
    payload: P,
    journal: &mut ShippingReceiveJournal,
    open: F,
) -> Result<(), String>
where
    F: FnOnce(ArrivedMessageRow<P>) -> Result<(), String>,
{
    shipping_receive::receive_arrived_message(
        ArrivedMessageRow::osl_chats(carrier_row_id, payload),
        journal,
        open,
    )
}

/// Receive the already authenticated text emitted by a connected OSL Chats
/// session and present it through the same protected eye record as email.
/// A missing connection is deliberately an error, rather than a local record
/// insertion: there is no receive event without the live session transport.
#[cfg(feature = "core")]
pub fn route_osl_chat_protected_text_arrival(
    connection_live: bool,
    message_id: impl Into<String>,
    cover_text: impl Into<String>,
    private_words: impl Into<String>,
    journal: &mut ShippingReceiveJournal,
    overlay: &mut ShippingProtectedOverlay,
) -> Result<(), String> {
    if !connection_live {
        return Err("OSL Chats receive connection is unavailable".to_owned());
    }
    let app_row_id = message_id.into();
    let record = crate::broker::ProtectedRowRecord {
        app_id: "osl-chats".to_owned(),
        app_row_id: app_row_id.clone(),
        cover_text: cover_text.into(),
        private_words: private_words.into(),
        opened: true,
    };
    shipping_receive::receive_arrived_message(
        ArrivedMessageRow::osl_chats(app_row_id, record),
        journal,
        |row| overlay.record_opened(row),
    )
}

pub fn route_osl_chat_arrived_row<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    pointer: &PointerArrival,
    journal: &mut ShippingReceiveJournal,
) -> Result<(), String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    route_osl_chat_arrival(pointer.blob_id.clone(), pointer.clone(), journal, |row| {
        driver.on_pointer_arrival(&row.payload)
    })
}

/// Compatibility entry point for existing pointer consumers.  It still enters
/// the shared shipping job; callers that need service attribution can retain
/// the supplied journal through [`route_osl_chat_arrived_row`].
pub fn receive_osl_chat_pointer<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    pointer: &PointerArrival,
) -> Result<(), String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    let mut journal = ShippingReceiveJournal::default();
    route_osl_chat_arrived_row(driver, pointer, &mut journal)
}

pub fn route_osl_chat_authorized_arrival<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    fetch: AuthorizedFetch,
    pointer_path_enabled: bool,
    journal: &mut ShippingReceiveJournal,
) -> Result<bool, String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    if !pointer_path_enabled {
        return Ok(false);
    }
    fetch.fetch_with(|blob_id, bearer_capability| {
        route_osl_chat_arrived_row(
            driver,
            &PointerArrival {
                blob_id: blob_id.to_hex(),
                fetch_cap: bearer_capability.as_bytes().to_vec(),
                manage_cap: Vec::new(),
            },
            journal,
        )?;
        Ok::<(), String>(())
    })?;
    Ok(true)
}

/// Compatibility entry point for the realtime pointer path.  It preserves
/// arrival semantics while funnelling each actual fetch through the shared
/// shipping job.
pub fn receive_osl_chat_authorized_fetch<T, S>(
    driver: &mut EagerFetchDriver<T, S>,
    fetch: AuthorizedFetch,
    pointer_path_enabled: bool,
) -> Result<bool, String>
where
    T: CipherStoreTransport,
    S: LocalMessageStore,
{
    let mut journal = ShippingReceiveJournal::default();
    route_osl_chat_authorized_arrival(driver, fetch, pointer_path_enabled, &mut journal)
}
