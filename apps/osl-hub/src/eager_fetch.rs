//! Mandatory pointer-arrival fetch and the durable offline burn outbox.
//!
//! A pointer is not a promise to fetch later.  `EagerFetchDriver::on_pointer_arrival`
//! fetches and asks the trusted local store to decrypt and persist immediately;
//! an open handler only reads the already-persisted result.  ACK emission is
//! deliberately absent here: T6-R5 owns the one post-persistence ACK point.
//!
//! The burn outbox is encrypted at rest, bounded, restart-safe, and refuses a
//! full queue rather than evicting a live delete. On the deployed bridge path
//! the fetch token is also the delete token, so reconnect retries preserve that
//! exact derived token rather than inventing a separate manage permission.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub const MAX_PENDING_BURNS: usize = 256;
const MAX_QUEUE_FILE_BYTES: u64 = 256 * 1024;
const QUEUE_VERSION: u8 = 1;

/// The bridge authority recovered from an authenticated shipping pointer.
///
/// The deployed service assigns the store id and accepts one token derived from
/// the carrier seed. It does not return or honor the retired
/// `fetch_cap`/`manage_cap` split.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointerArrival {
    pub blob_id: String,
    pub fetch_seed: [u8; ipc::prose_token::BRIDGE_SEED_BYTES],
}

impl PointerArrival {
    pub fn fetch_token(&self) -> [u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES] {
        ipc::prose_token::bridge_fetch_token_from_seed(&self.fetch_seed)
    }
}
/// The authenticated prose-token pointer shape the deployed service uses.
///
/// Production carries the server-assigned blob id and a seed; the fetch token
/// is derived from that seed when the pointer arrives. The retired shape with
/// separate `fetch_cap` and `manage_cap` fields is not accepted here.
pub type PointerArrival = ipc::prose_token::BridgePointer;

/// Network operations supplied by T6-R2's cipher-store client adapter.
pub trait CipherStoreTransport {
    fn fetch(&mut self, blob_id: &str, fetch_token: &[u8]) -> Result<Vec<u8>, String>;
    fn burn(&mut self, blob_id: &str, fetch_token: &[u8]) -> Result<(), String>;
    fn burn(&mut self, blob_id: &str, burn_capability: &[u8]) -> Result<(), String>;
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
/// Production transport plug for fetch-on-arrival.
///
/// It delegates to the existing bridge cipher-store client method; it does not
/// create another HTTP fetch implementation.
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
    pub fn from_config_dir(config_dir: &Path) -> Result<Self, String> {
        let base_url = ipc::cipher_store_client::resolve_cipher_store_base_url(config_dir)
            .map_err(|error| format!("OSL: cipher-store config: {error}"))?;
        let client = ipc::cipher_store_client::CipherStoreClient::new(base_url)
            .map_err(|error| format!("OSL: cipher-store client: {error}"))?;
        Ok(Self::new(client))
    }
}

impl CipherStoreTransport for CipherStoreClientTransport {
    fn fetch(&mut self, blob_id: &str, fetch_token: &[u8]) -> Result<Vec<u8>, String> {
        let fetch_token: [u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES] = fetch_token
            .try_into()
            .map_err(|_| "OSL: fetch token has the wrong length".to_owned())?;
        self.client
            .fetch_legacy_token(blob_id, &fetch_token)
            .map_err(|error| format!("OSL: cipher-store fetch: {error}"))
    }

    fn burn(&mut self, blob_id: &str, burn_capability: &[u8]) -> Result<(), String> {
        let burn_capability: [u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES] = burn_capability
            .try_into()
            .map_err(|_| "OSL: burn capability has the wrong length".to_owned())?;
        self.client
            .delete(blob_id, &burn_capability)
            .map_err(|error| format!("OSL: cipher-store burn: {error}"))
    }
}

/// Production local plug for fetch-on-arrival.
///
/// It rebuilds the normal OSL wire from the fetched object, then delegates to
/// the existing IPC decrypt/open path, which owns authentication and durable
/// `MessageStore` persistence.
pub struct ExistingMessageStoreOpener<'a> {
    state: &'a ipc::state::AppState,
    channel_id: String,
    sender_discord_id: String,
}

impl<'a> ExistingMessageStoreOpener<'a> {
    pub fn new(
        state: &'a ipc::state::AppState,
        channel_id: impl Into<String>,
        sender_discord_id: impl Into<String>,
    ) -> Self {
        Self {
            state,
            channel_id: channel_id.into(),
            sender_discord_id: sender_discord_id.into(),
        }
    }
}

impl LocalMessageStore for ExistingMessageStoreOpener<'_> {
    fn decrypt_and_persist(&mut self, blob_id: &str, ciphertext: &[u8]) -> Result<(), String> {
        let wire = ipc::prose_token::prose_token_bridge_object_to_wire(ciphertext)
            .map_err(|error| format!("OSL: bridge object open: {error}"))?;
        ipc::commands::cmd_osl_decrypt_message_v2(
            self.state,
            Some(blob_id.to_owned()),
            self.channel_id.clone(),
            self.sender_discord_id.clone(),
            wire,
            None,
            None,
        )
        .map(|_| ())
    }

    fn destroy_local(&mut self, blob_id: &str) -> Result<(), String> {
        ipc::commands::cmd_osl_burn_message(self.state, blob_id.to_owned())
    }
}

/// Eager receive and offline-burn coordinator.
pub struct EagerFetchDriver<T, S> {
    transport: T,
    store: S,
    burns: EncryptedBurnQueue,
    reservations: FetchReservations,
}

impl<T: CipherStoreTransport, S: LocalMessageStore> EagerFetchDriver<T, S> {
    pub fn new(transport: T, store: S, burns: EncryptedBurnQueue) -> Self {
        Self::with_reservations(transport, store, burns, FetchReservations::default())
    }

    pub fn with_reservations(
        transport: T,
        store: S,
        burns: EncryptedBurnQueue,
        reservations: FetchReservations,
    ) -> Self {
        Self {
            transport,
            store,
            burns,
            reservations,
        }
    }

    /// Called by the authenticated pointer-arrival path, never by an open UI.
    /// Retries retain the cipher-store reservation created before the first
    /// byte. A failed fetch/decrypt/write leaves no ACK decision to this task.
    pub fn on_pointer_arrival(&mut self, pointer: &PointerArrival) -> Result<(), String> {
        self.fetch_and_persist_once(pointer).map(|_| ())
    }

    /// Called by the slower conversation/inbox look path. It shares the same
    /// per-blob reservation as pointer arrival, so two triggers cannot fetch the
    /// same ciphertext concurrently.
    pub fn on_poll_arrival(&mut self, pointer: &PointerArrival) -> Result<bool, String> {
        self.fetch_and_persist_once(pointer)
    }

    fn fetch_and_persist_once(&mut self, pointer: &PointerArrival) -> Result<bool, String> {
        let Some(permit) = self.reservations.reserve(&pointer.blob_id)? else {
            return Ok(false);
        };
        let fetch_token = pointer.fetch_token();
        let ciphertext = crate::eager_fetch_retry::retry_reserved_fetch(|| {
            self.transport.fetch(&pointer.blob_id, &fetch_token)
        })?;
        self.store
            .decrypt_and_persist(&pointer.blob_id, &ciphertext)?;
        permit.commit()?;
        Ok(true)
        let blob_id = pointer.blob_id_hex();
        let fetch_token = pointer.fetch_token();
        let ciphertext = crate::eager_fetch_retry::retry_reserved_fetch(|| {
            self.transport.fetch(&blob_id, &fetch_token)
        })?;
        self.store.decrypt_and_persist(&blob_id, &ciphertext)
    }

    /// Local destruction is first.  Capacity is checked before destruction so
    /// a full durable queue fails closed without silently dropping a server
    /// delete; after destruction the exact received capability is persisted.
    pub fn burn_offline(&mut self, pointer: &PointerArrival) -> Result<(), String> {
        let blob_id = pointer.blob_id_hex();
        if !self.burns.can_enqueue(&blob_id)? {
            return Err("offline burn queue is full; no live delete was evicted".to_owned());
        }
        self.store.destroy_local(&pointer.blob_id)?;
        let fetch_token = pointer.fetch_token();
        self.burns.enqueue(&pointer.blob_id, &fetch_token)
        self.store.destroy_local(&blob_id)?;
        let burn_capability = pointer.fetch_token();
        self.burns.enqueue(&blob_id, &burn_capability)
    }

    /// Retry every durable record after reconnect.  A failure retains that
    /// record (and all later records) for a subsequent reconnect.
    pub fn drain_burns_on_reconnect(&mut self) -> Result<usize, String> {
        let pending = self.burns.pending()?;
        let mut drained = 0;
        for burn in pending {
            self.transport.burn(&burn.blob_id, &burn.fetch_token)?;
            self.burns.remove(&burn.blob_id)?;
            drained += 1;
        }
        Ok(drained)
    }

    pub fn into_parts(self) -> (T, S, EncryptedBurnQueue) {
        (self.transport, self.store, self.burns)
    }
}

#[derive(Clone, Debug, Default)]
pub struct FetchReservations {
    inner: Arc<Mutex<FetchReservationState>>,
}

#[derive(Debug, Default)]
struct FetchReservationState {
    in_flight: BTreeSet<String>,
    completed: BTreeSet<String>,
}

impl FetchReservations {
    fn reserve(&self, blob_id: &str) -> Result<Option<FetchPermit>, String> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "OSL Chat fetch reservations are unavailable".to_owned())?;
        if state.completed.contains(blob_id) || !state.in_flight.insert(blob_id.to_owned()) {
            return Ok(None);
        }
        Ok(Some(FetchPermit {
            reservations: self.clone(),
            blob_id: blob_id.to_owned(),
            committed: false,
        }))
    }

    pub fn completed_count(&self) -> Result<usize, String> {
        self.inner
            .lock()
            .map(|state| state.completed.len())
            .map_err(|_| "OSL Chat fetch reservations are unavailable".to_owned())
    }
}

struct FetchPermit {
    reservations: FetchReservations,
    blob_id: String,
    committed: bool,
}

impl FetchPermit {
    fn commit(mut self) -> Result<(), String> {
        let mut state = self
            .reservations
            .inner
            .lock()
            .map_err(|_| "OSL Chat fetch reservations are unavailable".to_owned())?;
        state.in_flight.remove(&self.blob_id);
        state.completed.insert(self.blob_id.clone());
        self.committed = true;
        Ok(())
    }
}

impl Drop for FetchPermit {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(mut state) = self.reservations.inner.lock() {
            state.in_flight.remove(&self.blob_id);
        }
    }
}

// `pending()` below is `pub` and returns `Vec<PendingBurn>`, so this type is
// already part of the public surface; leaving it private only broke the
// integration tests that consume that method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingBurn {
    blob_id: String,
    fetch_token: Vec<u8>,
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
            .any(|entry| entry.blob_id.is_empty() || entry.fetch_token.is_empty())
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

    pub fn enqueue(&self, blob_id: &str, fetch_token: &[u8]) -> Result<(), String> {
        if blob_id.is_empty() || fetch_token.is_empty() {
            return Err("offline burn record is invalid".to_owned());
        }
        let mut document = self.load()?;
        if let Some(existing) = document
            .pending
            .iter_mut()
            .find(|entry| entry.blob_id == blob_id)
        {
            // Replays are idempotent; preserve the original token rather
            // than accepting a different one for the same live record.
            if existing.fetch_token != fetch_token {
                return Err("offline burn replay changed its token".to_owned());
            }
            return Ok(());
        }
        if document.pending.len() >= MAX_PENDING_BURNS {
            return Err("offline burn queue is full; no live delete was evicted".to_owned());
        }
        document.pending.push(PendingBurn {
            blob_id: blob_id.to_owned(),
            fetch_token: fetch_token.to_vec(),
        });
        self.save(&document)
    }

    pub fn remove(&self, blob_id: &str) -> Result<(), String> {
        let mut document = self.load()?;
        document.pending.retain(|entry| entry.blob_id != blob_id);
        self.save(&document)
    }
}
