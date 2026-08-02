//! Durable, encrypted outbound queue for messages composed while offline.
//!
//! The queue stores already-encrypted transport envelopes, never plaintext.
//! Its sole backing record is written through [`SecureLocalStore`], which
//! authenticates and encrypts the whole queue at rest. A caller invokes
//! [`OfflineSendQueue::drain_after_reconnect`] after connectivity returns; an
//! item is removed only after its delivery callback reports success. Delivery
//! must use [`PendingSend::idempotency_key`] so a successful remote delivery
//! followed by a local persistence failure can safely be retried.
//!
//! There is deliberately no eviction policy. Reaching the caller-selected
//! bound returns [`OfflineSendQueueError::Full`] and leaves every live record
//! untouched.

use crate::secure_local_store::{RecordId, SecureLocalStore, SecureLocalStoreError};
use serde::{Deserialize, Serialize};

const QUEUE_NAMESPACE: &str = "offline-send-queue";
const QUEUE_RECORD_KEY: &str = "outbound-v1";

/// One already-encrypted outbound envelope awaiting transport delivery.
///
/// `idempotency_key` is stable across reconnect attempts. It must be passed to
/// the remote delivery path unchanged; that gives a retry after a crash or a
/// failed local dequeue the same effect as a single send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSend {
    pub idempotency_key: String,
    pub encrypted_envelope: Vec<u8>,
}

impl PendingSend {
    pub fn new(idempotency_key: impl Into<String>, encrypted_envelope: Vec<u8>) -> Self {
        Self {
            idempotency_key: idempotency_key.into(),
            encrypted_envelope,
        }
    }
}

/// Result of inserting an item into the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnqueueOutcome {
    /// The item was persisted and will be retried on reconnect.
    Enqueued,
    /// The same idempotency key was already pending, so no duplicate was made.
    AlreadyQueued,
}

/// Summary of one reconnect drain attempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrainOutcome {
    pub delivered: usize,
    pub still_pending: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum OfflineSendQueueError {
    #[error("offline send queue capacity must be greater than zero")]
    ZeroCapacity,

    #[error("offline send queue is full ({capacity} live records); no records were evicted")]
    Full { capacity: usize },

    #[error("offline send requires a non-empty idempotency key")]
    EmptyIdempotencyKey,

    #[error("offline send requires a non-empty encrypted envelope")]
    EmptyEnvelope,

    #[error("offline send queue data is malformed: {0}")]
    Malformed(String),

    #[error("offline send queue storage unavailable: {0}")]
    Store(#[from] SecureLocalStoreError),

    #[error("delivery failed; the undelivered item remains queued: {0}")]
    DeliveryFailed(String),
}

/// Bounded queue backed by one encrypted local record.
///
/// The generic storage boundary makes the queue usable with the application
/// disk backend and straightforward to test without a filesystem. The queue
/// itself has no connectivity state: calling `drain_after_reconnect` is the
/// explicit reconnect signal, avoiding hidden background retries while offline.
pub struct OfflineSendQueue<S: SecureLocalStore> {
    store: S,
    capacity: usize,
}

impl<S: SecureLocalStore> OfflineSendQueue<S> {
    pub fn new(store: S, capacity: usize) -> Result<Self, OfflineSendQueueError> {
        if capacity == 0 {
            return Err(OfflineSendQueueError::ZeroCapacity);
        }
        Ok(Self { store, capacity })
    }

    /// Persist an encrypted envelope for later delivery.
    ///
    /// A duplicate key is intentionally a no-op: callers may retry after an
    /// uncertain UI-to-disk handoff without multiplying a logical send.
    pub fn enqueue(&self, item: PendingSend) -> Result<EnqueueOutcome, OfflineSendQueueError> {
        validate_item(&item)?;
        let mut items = self.load()?;
        if items
            .iter()
            .any(|pending| pending.idempotency_key == item.idempotency_key)
        {
            return Ok(EnqueueOutcome::AlreadyQueued);
        }
        if items.len() >= self.capacity {
            return Err(OfflineSendQueueError::Full {
                capacity: self.capacity,
            });
        }
        items.push(item);
        self.save(&items)?;
        Ok(EnqueueOutcome::Enqueued)
    }

    /// Return pending sends in first-in-first-out order.
    pub fn pending(&self) -> Result<Vec<PendingSend>, OfflineSendQueueError> {
        self.load()
    }

    /// Retry queued sends after reconnecting.
    ///
    /// The callback is called in FIFO order. On its first error, this method
    /// stops immediately and preserves that item plus all later items. On a
    /// successful callback, the queue is persisted without the item before
    /// moving to the next one. If that persistence fails, a later reconnect
    /// may submit the same idempotency key again, which is safe by contract.
    pub fn drain_after_reconnect<F, E>(
        &self,
        mut deliver: F,
    ) -> Result<DrainOutcome, OfflineSendQueueError>
    where
        F: FnMut(&PendingSend) -> Result<(), E>,
        E: std::fmt::Display,
    {
        let mut items = self.load()?;
        let mut delivered = 0;

        while let Some(item) = items.first().cloned() {
            deliver(&item)
                .map_err(|error| OfflineSendQueueError::DeliveryFailed(error.to_string()))?;
            items.remove(0);
            self.save(&items)?;
            delivered += 1;
        }

        Ok(DrainOutcome {
            delivered,
            still_pending: 0,
        })
    }

    fn record_id() -> RecordId {
        RecordId::new(QUEUE_NAMESPACE, QUEUE_RECORD_KEY)
    }

    fn load(&self) -> Result<Vec<PendingSend>, OfflineSendQueueError> {
        match self.store.get(&Self::record_id()) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| OfflineSendQueueError::Malformed(error.to_string())),
            Err(SecureLocalStoreError::NotFound) => Ok(Vec::new()),
            Err(error) => Err(error.into()),
        }
    }

    fn save(&self, items: &[PendingSend]) -> Result<(), OfflineSendQueueError> {
        let bytes = serde_json::to_vec(items)
            .map_err(|error| OfflineSendQueueError::Malformed(error.to_string()))?;
        self.store.put(&Self::record_id(), &bytes)?;
        Ok(())
    }
}

fn validate_item(item: &PendingSend) -> Result<(), OfflineSendQueueError> {
    if item.idempotency_key.is_empty() {
        return Err(OfflineSendQueueError::EmptyIdempotencyKey);
    }
    if item.encrypted_envelope.is_empty() {
        return Err(OfflineSendQueueError::EmptyEnvelope);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure_local_store::{RawBackend, SealedStore};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MemoryBackend(Arc<Mutex<HashMap<String, Vec<u8>>>>);

    impl RawBackend for MemoryBackend {
        fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            self.0
                .lock()
                .expect("memory backend lock")
                .insert(storage_key.to_owned(), blob.to_vec());
            Ok(())
        }

        fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            Ok(self
                .0
                .lock()
                .expect("memory backend lock")
                .get(storage_key)
                .cloned())
        }

        fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
            self.0
                .lock()
                .expect("memory backend lock")
                .remove(storage_key);
            Ok(())
        }
    }

    fn queue(capacity: usize) -> OfflineSendQueue<SealedStore<MemoryBackend>> {
        OfflineSendQueue::new(
            SealedStore::new([0x5a; 32], MemoryBackend::default()),
            capacity,
        )
        .expect("non-zero capacity")
    }

    fn pending(key: &str) -> PendingSend {
        PendingSend::new(key, vec![0xc1, 0x70, 0x7e, 0x78])
    }

    #[test]
    fn never_evicts_a_live_record_when_full() {
        let queue = queue(1);
        assert!(matches!(
            queue.enqueue(pending("first")),
            Ok(EnqueueOutcome::Enqueued)
        ));

        assert!(matches!(
            queue.enqueue(pending("second")),
            Err(OfflineSendQueueError::Full { capacity: 1 })
        ));

        assert_eq!(
            queue.pending().expect("queued record"),
            vec![pending("first")]
        );
    }

    #[test]
    fn failed_reconnect_delivery_leaves_the_item_for_a_later_retry() {
        let queue = queue(2);
        queue.enqueue(pending("retry-me")).expect("enqueue");

        let first_attempt = queue.drain_after_reconnect(|_| Err::<(), _>("network down"));
        assert!(matches!(
            first_attempt,
            Err(OfflineSendQueueError::DeliveryFailed(_))
        ));
        assert_eq!(
            queue.pending().expect("still queued"),
            vec![pending("retry-me")]
        );

        let mut delivered_keys = Vec::new();
        let outcome = queue
            .drain_after_reconnect(|item| {
                delivered_keys.push(item.idempotency_key.clone());
                Ok::<(), String>(())
            })
            .expect("reconnected delivery");
        assert_eq!(
            outcome,
            DrainOutcome {
                delivered: 1,
                still_pending: 0
            }
        );
        assert_eq!(delivered_keys, vec!["retry-me"]);
        assert!(queue.pending().expect("empty queue").is_empty());
    }

    #[test]
    fn queue_is_encrypted_and_survives_a_reopen() {
        let backend = MemoryBackend::default();
        let queue =
            OfflineSendQueue::new(SealedStore::new([0x17; 32], backend.clone()), 2).expect("queue");
        queue.enqueue(pending("durable")).expect("enqueue");

        let persisted = backend
            .0
            .lock()
            .expect("memory backend lock")
            .values()
            .next()
            .cloned()
            .expect("sealed record");
        assert_ne!(
            persisted,
            serde_json::to_vec(&vec![pending("durable")]).expect("json")
        );

        let reopened = OfflineSendQueue::new(SealedStore::new([0x17; 32], backend), 2)
            .expect("reopened queue");
        assert_eq!(
            reopened.pending().expect("reloaded queue"),
            vec![pending("durable")]
        );
    }
}
