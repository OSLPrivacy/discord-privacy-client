//! T14-T12: encrypted OSL Chat sends survive offline/reconnect without eviction.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use ipc::{
    offline_send_queue::{OfflineSendQueue, OfflineSendQueueError},
    secure_local_store::{RawBackend, SealedStore, SecureLocalStoreError},
};
use osl_privacy_hub::osl_chat_queue::OslChatSendQueue;

#[derive(Clone, Default)]
struct Memory(Arc<Mutex<HashMap<String, Vec<u8>>>>);
impl RawBackend for Memory {
    fn write_blob(&self, key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
        self.0.lock().unwrap().insert(key.to_owned(), blob.to_vec());
        Ok(())
    }
    fn read_blob(&self, key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }
    fn remove_blob(&self, key: &str) -> Result<(), SecureLocalStoreError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

#[test]
fn t14_t12_full_queue_refuses_without_evicting_and_reconnect_reuses_the_envelope() {
    let backing = Memory::default();
    let queue = OslChatSendQueue::new(
        OfflineSendQueue::new(SealedStore::new([3; 32], backing.clone()), 1).unwrap(),
    );
    queue
        .enqueue_encrypted("chat-message-1", vec![9, 8, 7])
        .unwrap();

    assert!(matches!(
        queue.enqueue_encrypted("chat-message-2", vec![6]),
        Err(OfflineSendQueueError::Full { capacity: 1 })
    ));
    assert_eq!(
        queue.pending().unwrap().len(),
        1,
        "Q1 must retain the live record"
    );

    let sent = Arc::new(Mutex::new(Vec::new()));
    let sent_by_callback = sent.clone();
    queue
        .drain_on_reconnect(move |pending| -> Result<(), &'static str> {
            sent_by_callback.lock().unwrap().push((
                pending.idempotency_key.clone(),
                pending.encrypted_envelope.clone(),
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        *sent.lock().unwrap(),
        vec![("chat-message-1".to_owned(), vec![9, 8, 7])]
    );
    assert!(queue.pending().unwrap().is_empty());
}
