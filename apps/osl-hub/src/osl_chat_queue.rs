//! OSL Chat adapter for the shared encrypted offline-send queue.
//!
//! The queue engine lives in `ipc` so every send lane gets the same bounded,
//! encrypted-at-rest and idempotent retry semantics.  This adapter is the
//! OSL-Chat-specific boundary: callers hand it an envelope *after* encryption,
//! never draft plaintext.

use ipc::{
    offline_send_queue::{
        DrainOutcome, EnqueueOutcome, OfflineSendQueue, OfflineSendQueueError, PendingSend,
    },
    secure_local_store::SecureLocalStore,
};

pub struct OslChatSendQueue<S: SecureLocalStore> {
    queue: OfflineSendQueue<S>,
}

impl<S: SecureLocalStore> OslChatSendQueue<S> {
    pub fn new(queue: OfflineSendQueue<S>) -> Self {
        Self { queue }
    }

    /// Persists an already-encrypted OSL Chat envelope for reconnect delivery.
    /// A full queue fails closed; it never discards a live message to make room.
    pub fn enqueue_encrypted(
        &self,
        idempotency_key: impl Into<String>,
        encrypted_envelope: Vec<u8>,
    ) -> Result<EnqueueOutcome, OfflineSendQueueError> {
        self.queue
            .enqueue(PendingSend::new(idempotency_key, encrypted_envelope))
    }

    /// Drain only after the transport reports a reconnect.  The transport must
    /// use the provided stable idempotency key unchanged.
    pub fn drain_on_reconnect<F, E>(
        &self,
        deliver: F,
    ) -> Result<DrainOutcome, OfflineSendQueueError>
    where
        F: FnMut(&PendingSend) -> Result<(), E>,
        E: std::fmt::Display,
    {
        self.queue.drain_after_reconnect(deliver)
    }

    pub fn pending(&self) -> Result<Vec<PendingSend>, OfflineSendQueueError> {
        self.queue.pending()
    }
}
