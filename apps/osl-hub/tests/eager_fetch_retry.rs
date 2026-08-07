//! TF-91: a dropped eager-fetch read must not consume the view-once blob.

use osl_privacy_hub::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore, PointerArrival,
};

struct ReservedBlobNetwork {
    attempts: usize,
    reservation_active: bool,
    blob: Option<Vec<u8>>,
}

impl CipherStoreTransport for ReservedBlobNetwork {
    fn fetch(&mut self, _: &str, _: &[u8]) -> Result<Vec<u8>, String> {
        self.attempts += 1;
        // T2-40's CAS has already reserved this blob. The first transfer drops
        // before it yields bytes, but the reservation remains open for retry.
        if self.attempts == 1 {
            return Err("connection dropped before first byte completed".to_owned());
        }
        if !self.reservation_active {
            return Err("blob was incorrectly consumed on the first attempt".to_owned());
        }
        self.blob.clone().ok_or_else(|| "blob missing".to_owned())
    }

    fn burn(&mut self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct Store(Option<Vec<u8>>);

impl LocalMessageStore for Store {
    fn decrypt_and_persist(&mut self, _: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.0 = Some(ciphertext.to_vec());
        Ok(())
    }

    fn destroy_local(&mut self, _: &str) -> Result<(), String> {
        self.0 = None;
        Ok(())
    }
}

#[test]
fn tf_91_flaky_network_retries_inside_one_reservation_without_losing_the_payload() {
    let temp = tempfile::tempdir().unwrap();
    let network = ReservedBlobNetwork {
        attempts: 0,
        reservation_active: true,
        blob: Some(b"sealed view-once payload".to_vec()),
    };
    let queue = EncryptedBurnQueue::new(temp.path().join("burns.enc"), [7; 32]);
    let mut driver = EagerFetchDriver::new(network, Store::default(), queue);
    let pointer = PointerArrival {
        blob_id: "view-once".to_owned(),
        fetch_seed: [1; ipc::prose_token::BRIDGE_SEED_BYTES],
        server_blob_id: [0x33; ipc::prose_token::BRIDGE_ID_BYTES],
        seed: [0x44; ipc::prose_token::BRIDGE_SEED_BYTES],
    };

    driver.on_pointer_arrival(&pointer).unwrap();
    let (network, store, _) = driver.into_parts();
    assert_eq!(network.attempts, 2);
    assert_eq!(store.0.as_deref(), Some(&b"sealed view-once payload"[..]));
    assert!(
        network.blob.is_some(),
        "the reservation, not first-byte consumption, governs deletion"
    );
}
