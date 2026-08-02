//! Capability-negative fetches that make a fetch after a realtime reply
//! indistinguishable from ordinary constant-rate cover traffic.
//!
//! A decoy is deliberately just an otherwise valid request for an unknown
//! object with a fresh bearer-shaped capability.  The cipher store has no
//! decoy route or marker: its ordinary negative surface is the shared 404.

use rand::{rngs::OsRng, RngCore};

const ID_BYTES: usize = 16;

/// One ordinary-looking fetch which is expected to receive the normal 404.
/// Neither field is a pointer, identity, account, or stable tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecoyFetch {
    blob_id: [u8; ID_BYTES],
    fetch_cap: [u8; ID_BYTES],
}

impl DecoyFetch {
    /// Create fresh, independent values for one capability-negative request.
    pub fn random() -> Self {
        let mut blob_id = [0u8; ID_BYTES];
        let mut fetch_cap = [0u8; ID_BYTES];
        OsRng.fill_bytes(&mut blob_id);
        OsRng.fill_bytes(&mut fetch_cap);
        Self { blob_id, fetch_cap }
    }

    #[cfg(test)]
    fn from_bytes(blob_id: [u8; ID_BYTES], fetch_cap: [u8; ID_BYTES]) -> Self {
        Self { blob_id, fetch_cap }
    }

    /// Make the same path and header values used by an ordinary blob fetch.
    /// Callers must make this request through the normal fetch transport.
    pub fn fetch_with<F, E>(&self, fetch: F) -> Result<(), E>
    where
        F: FnOnce(&str, &str) -> Result<(), E>,
    {
        fetch(&hex(&self.blob_id), &hex(&self.fetch_cap))
    }
}

fn hex(bytes: &[u8; ID_BYTES]) -> String {
    let mut value = String::with_capacity(ID_BYTES * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realtime_client::{BlobId, CarrierPointer, RealtimeClient, ScheduledFetch};
    use std::time::Duration;

    #[test]
    fn t1_t56_decoy_and_authorized_fetches_keep_the_same_cadence_and_shape() {
        let decoy = DecoyFetch::from_bytes([0x11; ID_BYTES], [0x22; ID_BYTES]);
        let mut observed = None;
        decoy
            .fetch_with(|id, cap| {
                observed = Some((id.to_owned(), cap.to_owned()));
                Ok::<_, ()>(())
            })
            .expect("decoy request is shaped locally");
        let (id, cap) = observed.expect("one ordinary fetch request");
        assert_eq!(id.len(), 32);
        assert_eq!(cap.len(), 32);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(cap.bytes().all(|byte| byte.is_ascii_hexdigit()));

        let mut client = RealtimeClient::new(Duration::ZERO);
        let blob = BlobId::from_bytes([7; ID_BYTES]);
        client.remember_carrier_pointer(blob, CarrierPointer::from_carrier("real-capability"));
        let frame = wakeup([9; ID_BYTES], [7; ID_BYTES]);
        client.receive_frame(&frame).expect("valid fixed-size wakeup");

        match client.take_fetch_work().expect("one action for the reply") {
            ScheduledFetch::Authorized(fetch) => fetch
                .fetch_with(|actual_blob, capability| {
                    assert_eq!(actual_blob, blob);
                    assert_eq!(capability, "real-capability");
                    Ok::<_, ()>(())
                })
                .expect("authorized request is shaped locally"),
            ScheduledFetch::Decoy(_) => panic!("known pointer must use its ordinary fetch"),
        }

        // An idle/unknown reply still has one fetch-shaped follow-up, so the
        // observer cannot infer a pointer match from the cadence.
        client.receive_frame(&wakeup([8; ID_BYTES], [6; ID_BYTES])).unwrap();
        assert!(matches!(client.take_fetch_work(), Some(ScheduledFetch::Decoy(_))));
        let (_, first) = client.next_outbound_frame();
        let (_, second) = client.next_outbound_frame();
        assert_eq!(first.len(), second.len());
    }

    fn wakeup(tag: [u8; ID_BYTES], blob: [u8; ID_BYTES]) -> String {
        let body = format!(
            r#"{{"delivery_tag":"{}","blob_id":"{}"}}"#,
            hex(&tag),
            hex(&blob)
        );
        format!("{body:<width$}", width = crate::realtime_client::FRAME_BYTES)
    }
}
