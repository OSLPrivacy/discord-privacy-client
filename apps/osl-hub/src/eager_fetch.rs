//! Mandatory pointer-arrival fetch and the durable offline burn outbox.
//!
//! A pointer is not a promise to fetch later.  `EagerFetchDriver::on_pointer_arrival`
//! fetches and asks the trusted local store to decrypt and persist immediately;
//! an open handler only reads the already-persisted result.  ACK emission is
//! deliberately absent here: T6-R5 owns the one post-persistence ACK point.
//!
//! The burn outbox is encrypted at rest, bounded, restart-safe, and refuses a
//! full queue rather than evicting a live delete.  Its `manage_cap` is carried
//! verbatim through reconnect retries: it is a non-expiring bearer capability,
//! not a signed command that may be re-stamped.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_PENDING_BURNS: usize = 256;
const MAX_QUEUE_FILE_BYTES: u64 = 256 * 1024;
const QUEUE_VERSION: u8 = 1;

/// The authority which arrived in an authenticated pointer.  The driver never
/// derives, logs, or re-signs either capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointerArrival {
    pub blob_id: String,
    pub fetch_cap: Vec<u8>,
    pub manage_cap: Vec<u8>,
}

impl From<ipc::prose_token::ProseTokenPointerArrival> for PointerArrival {
    fn from(pointer: ipc::prose_token::ProseTokenPointerArrival) -> Self {
        Self {
            blob_id: pointer.blob_id,
            fetch_cap: pointer.fetch_cap.to_vec(),
            manage_cap: pointer.manage_cap.to_vec(),
        }
    }
}

/// Network operations supplied by T6-R2's cipher-store client adapter.
pub trait CipherStoreTransport {
    fn fetch(&mut self, blob_id: &str, fetch_cap: &[u8]) -> Result<Vec<u8>, String>;
    fn burn(&mut self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String>;
}

fn fixed_store_token(
    token: &[u8],
) -> Result<[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES], String> {
    token
        .try_into()
        .map_err(|_| "cipher-store capability has the wrong length".to_owned())
}

fn canonical_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn deployed_bridge_blob_id(value: &str) -> bool {
    canonical_lower_hex(value, ipc::prose_token::BRIDGE_ID_BYTES * 2)
}

fn destination_capability_blob_id(value: &str) -> bool {
    canonical_lower_hex(value, ipc::cipher_store_client::FETCH_TOKEN_BYTES * 2)
}

impl CipherStoreTransport for ipc::cipher_store_client::CipherStoreClient {
    fn fetch(&mut self, blob_id: &str, fetch_cap: &[u8]) -> Result<Vec<u8>, String> {
        let token = fixed_store_token(fetch_cap)?;
        if deployed_bridge_blob_id(blob_id) {
            return self
                .fetch_legacy_token(blob_id, &token)
                .map_err(|error| error.to_string());
        }
        if destination_capability_blob_id(blob_id) {
            return ipc::cipher_store_client::CipherStoreClient::fetch(self, blob_id, &token)
                .map_err(|error| error.to_string());
        }
        Err("cipher-store blob id has the wrong shape".to_owned())
    }

    fn burn(&mut self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String> {
        let token = fixed_store_token(manage_cap)?;
        if deployed_bridge_blob_id(blob_id) {
            return self
                .delete(blob_id, &token)
                .map_err(|error| error.to_string());
        }
        if destination_capability_blob_id(blob_id) {
            return ipc::cipher_store_client::CipherStoreClient::burn(self, blob_id, &token)
                .map_err(|error| error.to_string());
        }
        Err("cipher-store blob id has the wrong shape".to_owned())
    }
}

/// Trusted local persistence.  It must authenticate/decrypt and durably write
/// before returning `Ok`; the driver intentionally has no open-triggered API.
pub trait LocalMessageStore {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String>;
    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String>;
}

/// Production cipher-store plug for eager fetch.
///
/// This is only an adapter: all HTTP verbs, headers, routing policy, status
/// handling, and token formatting remain owned by `ipc::CipherStoreClient`.
pub struct CipherStoreClientTransport {
    client: ipc::cipher_store_client::CipherStoreClient,
}

impl CipherStoreClientTransport {
    pub fn new(client: ipc::cipher_store_client::CipherStoreClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &ipc::cipher_store_client::CipherStoreClient {
        &self.client
    }

    pub fn into_client(self) -> ipc::cipher_store_client::CipherStoreClient {
        self.client
    }
}

fn fixed_capability(
    label: &str,
    capability: &[u8],
) -> Result<[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES], String> {
    capability.try_into().map_err(|_| {
        format!(
            "{label} must be exactly {} bytes",
            ipc::cipher_store_client::FETCH_TOKEN_BYTES
        )
    })
}

impl CipherStoreTransport for CipherStoreClientTransport {
    fn fetch(&mut self, blob_id: &str, fetch_cap: &[u8]) -> Result<Vec<u8>, String> {
        let fetch_cap = fixed_capability("fetch capability", fetch_cap)?;
        self.client
            .fetch(blob_id, &fetch_cap)
            .map_err(|error| error.to_string())
    }

    fn burn(&mut self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String> {
        let manage_cap = fixed_capability("manage capability", manage_cap)?;
        self.client
            .burn(blob_id, &manage_cap)
            .map_err(|error| error.to_string())
    }
}

/// Conversation metadata the message store needs but the fetch driver must not
/// invent from a blob capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrivalMessageContext {
    pub channel_id: String,
    pub sender_discord_id: String,
    pub sender_osl_user_id: String,
}

/// Production local plug for eager fetch.
///
/// The fetched bytes are opened as the same UTF-8 message body shape the app's
/// current plaintext persistence path stores, then written through
/// `store::MessageStore::put`. Reading for display goes back through
/// `MessageStore::get`, so this adapter does not grow a parallel opener.
pub struct MessageStoreArrivalOpener<'a> {
    store: &'a store::MessageStore,
    context: ArrivalMessageContext,
}

impl<'a> MessageStoreArrivalOpener<'a> {
    pub fn new(store: &'a store::MessageStore, context: ArrivalMessageContext) -> Self {
        Self { store, context }
    }

    pub fn open_persisted(&self, blob_id: &str) -> Result<String, String> {
        self.store
            .get(blob_id)
            .map_err(|error| error.to_string())?
            .map(|message| message.plaintext)
            .ok_or_else(|| "message was not persisted".to_owned())
    }
}

impl LocalMessageStore for MessageStoreArrivalOpener<'_> {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        let plaintext = std::str::from_utf8(ciphertext)
            .map_err(|_| "fetched message is not UTF-8 plaintext".to_owned())?;
        self.store
            .put(&store::StoredMessage {
                edit_revision: Default::default(),
                reply_parent_id: Default::default(),
                discord_message_id: blob_id.to_owned(),
                channel_id: self.context.channel_id.clone(),
                sender_discord_id: self.context.sender_discord_id.clone(),
                sender_osl_user_id: self.context.sender_osl_user_id.clone(),
                plaintext: plaintext.to_owned(),
                decrypted_at: ipc::main_password::now_unix_secs_pub(),
                burned: false,
            })
            .map_err(|error| error.to_string())
    }

    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        self.store
            .mark_burned(blob_id)
            .map_err(|error| error.to_string())
    }
}

/// Eager receive and offline-burn coordinator.
pub struct EagerFetchDriver<T, S> {
    transport: T,
    store: S,
    burns: EncryptedBurnQueue,
}

impl<T: CipherStoreTransport, S: LocalMessageStore> EagerFetchDriver<T, S> {
    pub fn new(transport: T, store: S, burns: EncryptedBurnQueue) -> Self {
        Self {
            transport,
            store,
            burns,
        }
    }

    /// Called by the authenticated pointer-arrival path, never by an open UI.
    /// Retries retain the cipher-store reservation created before the first
    /// byte. A failed fetch/decrypt/write leaves no ACK decision to this task.
    pub fn on_pointer_arrival(&mut self, pointer: &PointerArrival) -> Result<(), String> {
        let ciphertext = crate::eager_fetch_retry::retry_reserved_fetch(|| {
            self.transport.fetch(&pointer.blob_id, &pointer.fetch_cap)
        })?;
        self.store
            .decrypt_and_persist(&pointer.blob_id, &ciphertext)
    }

    /// Local destruction is first.  Capacity is checked before destruction so
    /// a full durable queue fails closed without silently dropping a server
    /// delete; after destruction the exact received capability is persisted.
    pub fn burn_offline(&mut self, pointer: &PointerArrival) -> Result<(), String> {
        if !self.burns.can_enqueue(&pointer.blob_id)? {
            return Err("offline burn queue is full; no live delete was evicted".to_owned());
        }
        self.store.destroy_local(&pointer.blob_id)?;
        self.burns.enqueue(&pointer.blob_id, &pointer.manage_cap)
    }

    /// Retry every durable record after reconnect.  A failure retains that
    /// record (and all later records) for a subsequent reconnect.
    pub fn drain_burns_on_reconnect(&mut self) -> Result<usize, String> {
        let pending = self.burns.pending()?;
        let mut drained = 0;
        for burn in pending {
            self.transport.burn(&burn.blob_id, &burn.manage_cap)?;
            self.burns.remove(&burn.blob_id)?;
            drained += 1;
        }
        Ok(drained)
    }

    pub fn into_parts(self) -> (T, S, EncryptedBurnQueue) {
        (self.transport, self.store, self.burns)
    }
}

// `pending()` below is `pub` and returns `Vec<PendingBurn>`, so this type is
// already part of the public surface; leaving it private only broke the
// integration tests that consume that method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingBurn {
    blob_id: String,
    manage_cap: Vec<u8>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BurnQueueDocument {
    version: u8,
    pending: Vec<PendingBurn>,
}

/// Encrypted, crash-recoverable queue for server delete retries.
#[derive(Debug, Clone)]
pub struct EncryptedBurnQueue {
    path: PathBuf,
    key: [u8; 32],
}

impl EncryptedBurnQueue {
    pub fn new(path: impl Into<PathBuf>, key: [u8; 32]) -> Self {
        Self {
            path: path.into(),
            key,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn load(&self) -> Result<BurnQueueDocument, String> {
        let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
            &self.path,
            MAX_QUEUE_FILE_BYTES,
            "offline burn queue",
        )?
        else {
            return Ok(BurnQueueDocument {
                version: QUEUE_VERSION,
                pending: Vec::new(),
            });
        };
        let bytes = ipc::main_password::decrypt_at_rest(&sealed, &self.key)
            .map_err(|_| "offline burn queue could not be decrypted".to_owned())?;
        let document: BurnQueueDocument = serde_json::from_slice(&bytes)
            .map_err(|_| "offline burn queue is malformed".to_owned())?;
        if document.version != QUEUE_VERSION || document.pending.len() > MAX_PENDING_BURNS {
            return Err("offline burn queue has an unsupported shape".to_owned());
        }
        if document
            .pending
            .iter()
            .any(|entry| entry.blob_id.is_empty() || entry.manage_cap.is_empty())
        {
            return Err("offline burn queue has an invalid record".to_owned());
        }
        Ok(document)
    }

    fn save(&self, document: &BurnQueueDocument) -> Result<(), String> {
        let bytes = serde_json::to_vec(document)
            .map_err(|_| "offline burn queue could not be encoded".to_owned())?;
        let sealed = ipc::main_password::encrypt_at_rest(&bytes, &self.key)
            .map_err(|_| "offline burn queue could not be encrypted".to_owned())?;
        crate::atomic_file::write_recoverable(&self.path, &sealed, "offline burn queue")
    }

    pub fn pending(&self) -> Result<Vec<PendingBurn>, String> {
        Ok(self.load()?.pending)
    }

    pub fn can_enqueue(&self, blob_id: &str) -> Result<bool, String> {
        let document = self.load()?;
        Ok(document
            .pending
            .iter()
            .any(|entry| entry.blob_id == blob_id)
            || document.pending.len() < MAX_PENDING_BURNS)
    }

    pub fn enqueue(&self, blob_id: &str, manage_cap: &[u8]) -> Result<(), String> {
        if blob_id.is_empty() || manage_cap.is_empty() {
            return Err("offline burn record is invalid".to_owned());
        }
        let mut document = self.load()?;
        if let Some(existing) = document
            .pending
            .iter_mut()
            .find(|entry| entry.blob_id == blob_id)
        {
            // Replays are idempotent; preserve the original capability rather
            // than accepting a different one for the same live record.
            if existing.manage_cap != manage_cap {
                return Err("offline burn replay changed its capability".to_owned());
            }
            return Ok(());
        }
        if document.pending.len() >= MAX_PENDING_BURNS {
            return Err("offline burn queue is full; no live delete was evicted".to_owned());
        }
        document.pending.push(PendingBurn {
            blob_id: blob_id.to_owned(),
            manage_cap: manage_cap.to_vec(),
        });
        self.save(&document)
    }

    pub fn remove(&self, blob_id: &str) -> Result<(), String> {
        let mut document = self.load()?;
        document.pending.retain(|entry| entry.blob_id != blob_id);
        self.save(&document)
    }
}
