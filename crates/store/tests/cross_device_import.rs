//! Regression coverage for importing an anchored profile onto another device.
//!
//! The rollback anchor is deliberately device-local.  This test copies a
//! profile after a mutation (generation two) and gives the copied profile a
//! fresh provider, exactly as a second device would have.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use store::{AnchorRecord, MessageStore, MonotonicAnchor, StoreError, StoredMessage};
use tempfile::TempDir;

const SECRET: &[u8; 32] = b"cross-device-import-secret-32byt";

#[derive(Default)]
struct DeviceLocalAnchor {
    records: Mutex<BTreeMap<[u8; 32], AnchorRecord>>,
}

impl MonotonicAnchor for DeviceLocalAnchor {
    fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
        Ok(self.records.lock().unwrap().get(&store_id).cloned())
    }

    fn compare_and_advance(
        &self,
        store_id: [u8; 32],
        expected: Option<AnchorRecord>,
        next: AnchorRecord,
    ) -> Result<(), StoreError> {
        let mut records = self.records.lock().unwrap();
        if records.get(&store_id).cloned() != expected {
            return Err(StoreError::Anchor(
                "test anchor rejected stale compare-and-advance".to_string(),
            ));
        }
        records.insert(store_id, next);
        Ok(())
    }
}

fn checkpoint(dir: &Path) {
    let conn = rusqlite::Connection::open(dir.join("messages.sqlite")).unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
}

fn message() -> StoredMessage {
    StoredMessage {
        discord_message_id: "cross-device-message".to_string(),
        channel_id: "cross-device-channel".to_string(),
        sender_discord_id: "cross-device-sender".to_string(),
        sender_osl_user_id: "cross-device-user".to_string(),
        plaintext: "a mutation advances the anchor".to_string(),
        decrypted_at: 1,
        burned: false,
    }
}

#[test]
fn restored_generation_two_profile_is_refused_by_a_second_devices_empty_anchor() {
    let first_device_profile = TempDir::new().unwrap();
    let second_device_profile = TempDir::new().unwrap();
    let first_device_anchor = Arc::new(DeviceLocalAnchor::default());

    {
        let store =
            MessageStore::open_anchored(first_device_profile.path(), SECRET, first_device_anchor)
                .unwrap();
        store.put(&message()).unwrap();
    }
    checkpoint(first_device_profile.path());
    fs::copy(
        first_device_profile.path().join("messages.sqlite"),
        second_device_profile.path().join("messages.sqlite"),
    )
    .unwrap();

    let second_device_anchor = Arc::new(DeviceLocalAnchor::default());
    let error = match MessageStore::open_anchored(
        second_device_profile.path(),
        SECRET,
        second_device_anchor,
    ) {
        Ok(_) => panic!("a generation-two restored profile must not enroll a fresh device anchor"),
        Err(error) => error,
    };

    assert!(
        matches!(error, StoreError::Anchor(message) if message == "provider lost anchored generation; refusing rollback"),
        "wrong second-device import result: {error}"
    );
}
