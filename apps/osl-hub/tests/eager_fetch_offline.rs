use std::collections::BTreeMap;

use osl_privacy_hub::eager_fetch::{
    CipherStoreTransport, EagerFetchDriver, EncryptedBurnQueue, LocalMessageStore, PointerArrival,
    MAX_PENDING_BURNS,
};

#[derive(Default)]
struct Network {
    online: bool,
    blobs: BTreeMap<String, Vec<u8>>,
    burned: Vec<(String, Vec<u8>)>,
}

impl CipherStoreTransport for Network {
    fn fetch(&mut self, blob_id: &str, _: &[u8]) -> Result<Vec<u8>, String> {
        if !self.online {
            return Err("network is down".to_owned());
        }
        self.blobs
            .get(blob_id)
            .cloned()
            .ok_or_else(|| "blob gone".to_owned())
    }
    fn burn(&mut self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String> {
        if !self.online {
            return Err("network is down".to_owned());
        }
        self.blobs.remove(blob_id);
        self.burned.push((blob_id.to_owned(), manage_cap.to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct Local {
    plaintext: BTreeMap<String, Vec<u8>>,
    opens: usize,
}

impl Local {
    fn open(&mut self, blob_id: &str) -> Result<Vec<u8>, String> {
        self.opens += 1;
        self.plaintext
            .get(blob_id)
            .cloned()
            .ok_or_else(|| "not persisted".to_owned())
    }
}

impl LocalMessageStore for Local {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        self.plaintext
            .insert(blob_id.to_owned(), ciphertext.to_vec());
        Ok(())
    }
    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        self.plaintext.remove(blob_id);
        Ok(())
    }
}

fn pointer(id: &str) -> PointerArrival {
    PointerArrival {
        blob_id: id.to_owned(),
        fetch_cap: vec![7; 32],
        manage_cap: vec![9; 32],
    }
}

#[test]
fn t6_t28_pointer_fetches_before_open_and_offline_burn_survives_restart_without_eviction() {
    let temp = tempfile::tempdir().unwrap();
    let queue_path = temp.path().join("offline-burns.enc");
    let queue = EncryptedBurnQueue::new(&queue_path, [3; 32]);
    let mut network = Network {
        online: true,
        blobs: BTreeMap::from([("a".to_owned(), b"plain".to_vec())]),
        burned: vec![],
    };
    let mut driver = EagerFetchDriver::new(network, Local::default(), queue);

    driver.on_pointer_arrival(&pointer("a")).unwrap();
    let (mut network, mut local, queue) = driver.into_parts();
    network.online = false;
    assert_eq!(
        local.open("a").unwrap(),
        b"plain",
        "open reads the eager-persisted local copy while hard-down"
    );
    assert_eq!(local.opens, 1);

    let mut driver = EagerFetchDriver::new(network, local, queue);
    driver.burn_offline(&pointer("a")).unwrap();
    let (mut network, mut local, queue) = driver.into_parts();
    assert!(
        local.open("a").is_err(),
        "local destruction does not wait for reconnect"
    );
    assert_eq!(queue.pending().unwrap().len(), 1);
    assert!(!std::fs::read(&queue_path)
        .unwrap()
        .windows(b"plain".len())
        .any(|part| part == b"plain"));

    // Constructing a fresh queue is the process-restart boundary.
    let queue = EncryptedBurnQueue::new(&queue_path, [3; 32]);
    network.online = true;
    let mut restarted = EagerFetchDriver::new(network, local, queue);
    assert_eq!(restarted.drain_burns_on_reconnect().unwrap(), 1);
    let (network, _, queue) = restarted.into_parts();
    assert_eq!(
        network.burned,
        vec![("a".to_owned(), vec![9; 32])],
        "manage capability is sent unchanged"
    );
    assert!(queue.pending().unwrap().is_empty());

    for index in 0..MAX_PENDING_BURNS {
        queue.enqueue(&format!("live-{index}"), &[1]).unwrap();
    }
    assert!(queue.enqueue("overflow", &[1]).is_err());
    assert_eq!(
        queue.pending().unwrap().len(),
        MAX_PENDING_BURNS,
        "a full queue never evicts a live record"
    );
}
