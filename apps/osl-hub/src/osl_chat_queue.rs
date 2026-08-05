//! OSL Chat adapter for the shared encrypted offline-send queue.
//!
//! The queue engine lives in `ipc` so every send lane gets the same bounded,
//! encrypted-at-rest and idempotent retry semantics.  This adapter is the
//! OSL-Chat-specific boundary: callers hand it an envelope *after* encryption,
//! never draft plaintext.
//!
//! # What is actually queued, and why only this
//!
//! An OSL Chat chunk is delivered by **two** posts, in this order
//! (`broker::prepare_peer_inbox_text_with_route_clients`):
//!
//! 1. `post_native_overlay_wrapped_key` — the server-held wrapped key.
//! 2. `post_control_inbox` — the sealed relay notice that makes it readable.
//!
//! Only the window *between* those two is queued. That is deliberate and it is
//! the only window where a durable retry is both necessary and safe:
//!
//! - If step 1 fails, nothing reached the server, the send is refused, and the
//!   composer keeps the draft. There is nothing to complete later.
//! - If step 1 succeeded and step 2 failed, the recipient holds a key for a
//!   message they will never be told about. Delivery is *half done* and cannot
//!   be undone; the only honest resolutions are to finish it or to lie about
//!   it. Replaying step 2 finishes it, and replaying step 2 alone cannot
//!   duplicate the wrapped key.
//!
//! Only [`keystore::Error::Transport`] enqueues — a status code is the server
//! answering, which is a refusal to be surfaced, not an outage to retry through.

use std::path::Path;

use ipc::{
    offline_send_queue::{
        DrainOutcome, EnqueueOutcome, OfflineSendQueue, OfflineSendQueueError, PendingSend,
    },
    secure_local_store::{SealedStore, SecureLocalStore},
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::secure_disk_backend::SecureDiskBackend;

/// HKDF info label separating the send queue's at-rest key from every other
/// consumer of the file storage key.
pub const HKDF_INFO_OSL_CHAT_SEND_QUEUE: &[u8] = b"osl/osl-chat/send-queue/v1";

/// Directory name, under the caller's data dir, holding the queue records.
pub const OSL_CHAT_SEND_QUEUE_DIR: &str = "osl-chat-send-queue";

/// Bounded, and a full queue fails closed rather than evicting a live message.
pub const OSL_CHAT_SEND_QUEUE_CAPACITY: usize = 64;

/// The production queue type: the shared engine over the one production
/// [`ipc::secure_local_store::RawBackend`].
pub type OslChatDiskSendQueue = OslChatSendQueue<SealedStore<SecureDiskBackend>>;

/// A relay notice whose wrapped key is already on the server and which still
/// has to be posted. Every field is either already-ciphertext or routing
/// metadata the key server sees anyway; no draft plaintext is ever persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedRelayNotice {
    pub message_id: String,
    pub recipient_osl_user_id: String,
    pub scope_id: String,
    /// The sealed relay-notice bundle, verbatim. Re-signing happens at post
    /// time; the bundle itself is what must survive unchanged.
    pub bundle: Vec<u8>,
    /// After this the message is dead and the record is dropped rather than
    /// posted, so a permanently offline peer cannot pin a record forever.
    pub expires_at: i64,
}

impl QueuedRelayNotice {
    pub fn encode(&self) -> Result<Vec<u8>, OfflineSendQueueError> {
        serde_json::to_vec(self)
            .map_err(|error| OfflineSendQueueError::Malformed(error.to_string()))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, OfflineSendQueueError> {
        serde_json::from_slice(bytes)
            .map_err(|error| OfflineSendQueueError::Malformed(error.to_string()))
    }
}

/// Derive the queue's at-rest key, exactly the way
/// [`crate::osl_chat_local_state_key`] resolves its authority: the
/// main-password-derived file storage key when the session is unlocked,
/// otherwise the device-bound fallback, and only when no main-password marker
/// exists. A locked profile therefore gets no queue at all rather than a
/// plaintext one.
pub fn osl_chat_send_queue_key(dir: &Path) -> Result<Zeroizing<[u8; 32]>, String> {
    let root = match ipc::main_password::get_file_storage_key() {
        Some(key) => Zeroizing::new(key),
        None => Zeroizing::new(
            ipc::main_password::ensure_device_bound_fallback_file_storage_key(dir).map_err(
                |error| format!("OSL: no storage-key authority for the OSL Chat send queue: {error}"),
            )?,
        ),
    };
    crypto::hkdf::derive_32(&[], &root[..], HKDF_INFO_OSL_CHAT_SEND_QUEUE)
        .map(Zeroizing::new)
        .map_err(|error| format!("OSL: derive OSL Chat send-queue key: {error}"))
}

/// Open the durable OSL Chat send queue rooted at `dir`.
pub fn osl_chat_send_queue(dir: &Path) -> Result<OslChatDiskSendQueue, String> {
    let key = osl_chat_send_queue_key(dir)?;
    let backend = SecureDiskBackend::new(dir.join(OSL_CHAT_SEND_QUEUE_DIR));
    let queue = OfflineSendQueue::new(SealedStore::new(*key, backend), OSL_CHAT_SEND_QUEUE_CAPACITY)
        .map_err(|error| format!("OSL: open OSL Chat send queue: {error}"))?;
    Ok(OslChatSendQueue::new(queue))
}

/// True when a key-server failure means "we could not reach the server", as
/// opposed to the server refusing. Only the former is retryable in a way that
/// makes a durable queue honest.
pub fn is_unreachable(error: &keystore::Error) -> bool {
    matches!(error, keystore::Error::Transport(_))
}

/// Open the durable queue under the account config dir — the same directory
/// authority `peer_attachment_io`'s deletion outbox already resolves through.
pub fn osl_chat_send_queue_at_config_dir() -> Result<OslChatDiskSendQueue, String> {
    let dir = keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?;
    osl_chat_send_queue(&dir)
}

/// Durably record a relay notice whose wrapped key already reached the server.
///
/// Fails loudly rather than silently discarding: a caller that cannot persist
/// the promise must report the message as undelivered, not as queued.
pub fn queue_undelivered_relay_notice(notice: &QueuedRelayNotice) -> Result<EnqueueOutcome, String> {
    osl_chat_send_queue_at_config_dir()?
        .enqueue_relay_notice(notice)
        .map_err(|error| format!("OSL Chat send queue: {error}"))
}

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

    /// Persist a relay notice whose wrapped key is already on the server.
    /// The idempotency key is the physical message id, so re-queuing the same
    /// chunk after a crash cannot multiply it.
    pub fn enqueue_relay_notice(
        &self,
        notice: &QueuedRelayNotice,
    ) -> Result<EnqueueOutcome, OfflineSendQueueError> {
        self.enqueue_encrypted(notice.message_id.clone(), notice.encode()?)
    }

    /// Replay every pending notice. `post` receives each decoded record; a
    /// record already past `expires_at` is dropped without being posted,
    /// because delivering an expired message is worse than not delivering it.
    ///
    /// Stops at the first delivery failure and keeps that record and all later
    /// ones, so a still-offline peer does not burn the queue.
    pub fn drain_relay_notices<F>(
        &self,
        now: i64,
        mut post: F,
    ) -> Result<DrainOutcome, OfflineSendQueueError>
    where
        F: FnMut(&QueuedRelayNotice) -> Result<(), String>,
    {
        self.queue.drain_after_reconnect(|pending| {
            let notice = QueuedRelayNotice::decode(&pending.encrypted_envelope)
                .map_err(|error| error.to_string())?;
            if notice.expires_at <= now {
                return Ok(());
            }
            post(&notice)
        })
    }
}
