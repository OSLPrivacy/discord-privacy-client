//! Offline queue for user-sent statutory erasure requests.
//!
//! Queue entries are already encrypted before they enter this type. Persistence
//! belongs in the authenticated encrypted local store described in
//! `offline-controls-and-opened-receipts.md`; this in-memory state machine
//! deliberately never retains a plaintext request. A queued request is
//! `Pending`, not sent and never a verified deletion.

use std::collections::BTreeMap;

const MAX_QUEUED_ERASURE_REQUESTS: usize = 4_096;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SealedErasureRequest {
    request_id: [u8; 32],
    ciphertext: Vec<u8>,
}

/// Plaintext exists only while a caller's composer encrypts it. The queue never
/// stores this type.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErasureRequestDraft {
    pub request_id: [u8; 32],
    pub plaintext: String,
}

impl SealedErasureRequest {
    pub fn new(request_id: [u8; 32], ciphertext: Vec<u8>) -> Result<Self, ErasureQueueError> {
        if request_id.iter().all(|byte| *byte == 0) || ciphertext.is_empty() {
            return Err(ErasureQueueError::InvalidSealedRequest);
        }
        Ok(Self {
            request_id,
            ciphertext,
        })
    }

    pub fn request_id(&self) -> [u8; 32] {
        self.request_id
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureRequestStatus {
    /// Composed offline or awaiting a successful reconnect delivery.
    Pending,
    /// The user's mailbox transport accepted the sealed request. This remains
    /// an `Unknown` deletion outcome until a later re-scan verifies it.
    Sent,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct QueuedErasureRequest {
    sealed: SealedErasureRequest,
    status: ErasureRequestStatus,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QueueInsertDisposition {
    Queued,
    AlreadyQueued,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureQueueError {
    InvalidSealedRequest,
    QueueFull,
    RequestIdCollision,
}

/// Encrypted local queue. Call [`Self::send_after_reconnect`] only after the
/// network reconnects; failed transmissions leave entries pending for retry.
#[derive(Debug, Default)]
pub struct ErasureRequestQueue {
    entries: BTreeMap<[u8; 32], QueuedErasureRequest>,
}

impl ErasureRequestQueue {
    /// Compose and encrypt while offline, then queue the resulting sealed
    /// envelope for the next reconnect.
    pub fn compose_offline(
        &mut self,
        draft: ErasureRequestDraft,
        composer: &impl ErasureRequestComposer,
    ) -> Result<QueueInsertDisposition, ErasureQueueError> {
        self.enqueue_offline(composer.compose_and_encrypt(draft)?)
    }

    pub fn enqueue_offline(
        &mut self,
        sealed: SealedErasureRequest,
    ) -> Result<QueueInsertDisposition, ErasureQueueError> {
        if let Some(existing) = self.entries.get(&sealed.request_id) {
            return if existing.sealed == sealed {
                Ok(QueueInsertDisposition::AlreadyQueued)
            } else {
                Err(ErasureQueueError::RequestIdCollision)
            };
        }
        if self.entries.len() >= MAX_QUEUED_ERASURE_REQUESTS {
            return Err(ErasureQueueError::QueueFull);
        }
        self.entries.insert(
            sealed.request_id,
            QueuedErasureRequest {
                sealed,
                status: ErasureRequestStatus::Pending,
            },
        );
        Ok(QueueInsertDisposition::Queued)
    }

    pub fn status(&self, request_id: [u8; 32]) -> Option<ErasureRequestStatus> {
        self.entries.get(&request_id).map(|entry| entry.status)
    }

    pub fn send_after_reconnect(&mut self, transport: &mut impl ErasureRequestTransport) {
        for entry in self.entries.values_mut() {
            if entry.status == ErasureRequestStatus::Pending
                && transport.send(&entry.sealed).is_ok()
            {
                entry.status = ErasureRequestStatus::Sent;
            }
        }
    }
}

pub trait ErasureRequestComposer {
    fn compose_and_encrypt(
        &self,
        draft: ErasureRequestDraft,
    ) -> Result<SealedErasureRequest, ErasureQueueError>;
}

/// The transport is responsible for submitting the request through the user's
/// own mailbox; it receives only the sealed envelope.
pub trait ErasureRequestTransport {
    fn send(&mut self, request: &SealedErasureRequest) -> Result<(), ()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestComposer;

    impl ErasureRequestComposer for TestComposer {
        fn compose_and_encrypt(
            &self,
            draft: ErasureRequestDraft,
        ) -> Result<SealedErasureRequest, ErasureQueueError> {
            let ciphertext = draft.plaintext.bytes().map(|byte| byte ^ 0xA5).collect();
            SealedErasureRequest::new(draft.request_id, ciphertext)
        }
    }

    struct RecordingTransport {
        delivered: Vec<[u8; 32]>,
        available: bool,
    }

    impl ErasureRequestTransport for RecordingTransport {
        fn send(&mut self, request: &SealedErasureRequest) -> Result<(), ()> {
            if !self.available {
                return Err(());
            }
            self.delivered.push(request.request_id());
            Ok(())
        }
    }

    #[test]
    fn compose_offline_queues_pending_then_sends_on_reconnect() {
        let request_id = [7; 32];
        let mut queue = ErasureRequestQueue::default();
        assert_eq!(
            queue.compose_offline(
                ErasureRequestDraft {
                    request_id,
                    plaintext: "please erase my account data".to_owned(),
                },
                &TestComposer,
            ),
            Ok(QueueInsertDisposition::Queued)
        );

        assert_eq!(
            queue.status(request_id),
            Some(ErasureRequestStatus::Pending)
        );

        let mut offline = RecordingTransport {
            delivered: Vec::new(),
            available: false,
        };
        queue.send_after_reconnect(&mut offline);
        assert!(offline.delivered.is_empty());
        assert_eq!(
            queue.status(request_id),
            Some(ErasureRequestStatus::Pending)
        );

        let mut reconnected = RecordingTransport {
            delivered: Vec::new(),
            available: true,
        };
        queue.send_after_reconnect(&mut reconnected);
        assert_eq!(reconnected.delivered, vec![request_id]);
        assert_eq!(queue.status(request_id), Some(ErasureRequestStatus::Sent));
    }
}
