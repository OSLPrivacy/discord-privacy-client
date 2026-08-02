//! Durable RN outbox boundary.
//!
//! An RN send advances its ratchet at seal time.  Consequently transport
//! retry must be deliberately unable to call the sealer: this module seals
//! once while enqueuing and delegates every later delivery attempt to the
//! encrypted-envelope queue unchanged.

use crate::offline_send_queue::{
    DrainOutcome, EnqueueOutcome, OfflineSendQueue, OfflineSendQueueError, PendingSend,
};
use crate::secure_local_store::SecureLocalStore;

/// Errors from the RN enqueue boundary.
#[derive(Debug, thiserror::Error)]
pub enum RnOutboxError {
    #[error("RN encryption failed before enqueue: {0}")]
    Encrypt(String),

    #[error("RN outbox storage failed: {0}")]
    Queue(#[from] OfflineSendQueueError),
}

/// Seals an RN payload once before handing its immutable wire to the durable
/// offline queue.  The queue is the sole retry owner.
pub struct RnOutbox<S: SecureLocalStore> {
    queue: OfflineSendQueue<S>,
}

impl<S: SecureLocalStore> RnOutbox<S> {
    pub fn new(queue: OfflineSendQueue<S>) -> Self {
        Self { queue }
    }

    /// Encrypt and persist an RN wire exactly once for this logical send.
    ///
    /// Checking for the idempotency key before sealing is important: a caller
    /// recovering from an uncertain enqueue result must not advance the RN
    /// chain again merely to discover that the original wire was retained.
    pub fn enqueue<F, E>(
        &self,
        idempotency_key: impl Into<String>,
        encrypt_rn: F,
    ) -> Result<EnqueueOutcome, RnOutboxError>
    where
        F: FnOnce() -> Result<String, E>,
        E: std::fmt::Display,
    {
        let idempotency_key = idempotency_key.into();
        if self
            .queue
            .pending()?
            .iter()
            .any(|pending| pending.idempotency_key == idempotency_key)
        {
            return Ok(EnqueueOutcome::AlreadyQueued);
        }

        let wire = encrypt_rn().map_err(|error| RnOutboxError::Encrypt(error.to_string()))?;
        self.queue
            .enqueue(PendingSend::new(idempotency_key, wire.into_bytes()))
            .map_err(Into::into)
    }

    /// Retry delivery without access to plaintext or the RN sealer.
    pub fn drain_after_reconnect<F, E>(&self, deliver: F) -> Result<DrainOutcome, RnOutboxError>
    where
        F: FnMut(&PendingSend) -> Result<(), E>,
        E: std::fmt::Display,
    {
        self.queue
            .drain_after_reconnect(deliver)
            .map_err(Into::into)
    }

    pub fn pending(&self) -> Result<Vec<PendingSend>, RnOutboxError> {
        self.queue.pending().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secure_local_store::{RawBackend, SealedStore, SecureLocalStoreError};
    use osl_ratchet_next::{
        decrypt_rn, encrypt_rn, test_support::established_pair_with_params, SessionParams,
        SkipParams,
    };
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MemoryBackend(Arc<Mutex<HashMap<String, Vec<u8>>>>);

    impl RawBackend for MemoryBackend {
        fn write_blob(&self, key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
            self.0
                .lock()
                .expect("memory backend lock")
                .insert(key.into(), blob.into());
            Ok(())
        }

        fn read_blob(&self, key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
            Ok(self
                .0
                .lock()
                .expect("memory backend lock")
                .get(key)
                .cloned())
        }

        fn remove_blob(&self, key: &str) -> Result<(), SecureLocalStoreError> {
            self.0.lock().expect("memory backend lock").remove(key);
            Ok(())
        }
    }

    fn outbox() -> RnOutbox<SealedStore<MemoryBackend>> {
        let queue =
            OfflineSendQueue::new(SealedStore::new([0x19; 32], MemoryBackend::default()), 2)
                .expect("valid queue");
        RnOutbox::new(queue)
    }

    #[test]
    fn t19_t20_retries_600_times_without_reencrypting_the_queued_wire() {
        let outbox = outbox();
        let encryptions = AtomicUsize::new(0);
        let params = SessionParams {
            skip: SkipParams {
                max_skip_per_message: 512,
                ..SkipParams::default()
            },
            ..SessionParams::default()
        };
        let (mut sender, mut receiver, _) = established_pair_with_params(0x19_d1, params);

        outbox
            .enqueue("logical-rn-message", || {
                encryptions.fetch_add(1, Ordering::SeqCst);
                encrypt_rn(&mut sender, 7, b"one durable RN wire")
                    .map_err(|error| error.to_string())
            })
            .expect("encrypt and queue once");

        for _ in 0..600 {
            assert!(matches!(
                outbox.drain_after_reconnect(|_| Err::<(), _>("transport unavailable")),
                Err(RnOutboxError::Queue(OfflineSendQueueError::DeliveryFailed(
                    _
                )))
            ));
        }

        let mut delivered = Vec::new();
        outbox
            .drain_after_reconnect(|item| {
                delivered.push(item.encrypted_envelope.clone());
                Ok::<(), String>(())
            })
            .expect("the retained wire delivers after reconnect");

        assert_eq!(encryptions.load(Ordering::SeqCst), 1);
        assert_eq!(delivered.len(), 1);
        let delivered_wire = String::from_utf8(delivered.pop().expect("one delivered wire"))
            .expect("RN wire is text");
        assert_eq!(
            decrypt_rn(&mut receiver, &delivered_wire)
                .expect("a receiver limited to 512 skips opens the retained wire")
                .plaintext,
            b"one durable RN wire"
        );
        assert!(outbox.pending().expect("empty after delivery").is_empty());
    }

    #[test]
    fn duplicate_enqueue_does_not_advance_the_ratchet_again() {
        let outbox = outbox();
        let encryptions = AtomicUsize::new(0);
        outbox
            .enqueue("same-logical-message", || {
                encryptions.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>("first-rn-wire".to_owned())
            })
            .expect("first enqueue");

        let duplicate = outbox
            .enqueue("same-logical-message", || {
                encryptions.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>("must-not-be-sealed".to_owned())
            })
            .expect("duplicate is a no-op");

        assert_eq!(duplicate, EnqueueOutcome::AlreadyQueued);
        assert_eq!(encryptions.load(Ordering::SeqCst), 1);
    }
}
