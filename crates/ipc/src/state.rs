//! Shared application state held in Tauri's `State<>` slot.
//!
//! v1 alpha holds:
//! - The currently-loaded [`keystore::Identity`] (if any).
//! - The configured [`keystore::KeyServerClient`] (if any).
//! - A bounded TTL cache of fetched sender public keys (Phase 5
//!   receive-side decoding — see [`SenderPubkeyCache`]).
//!
//! All fields live behind a `Mutex` to allow blocking access from
//! sync command handlers (the keystore HTTP client is sync; tauri
//! command handlers wrap it in `spawn_blocking`).
//!
//! v1 stable extends this with: ratchet state per peer, sender-keys
//! state per group, wrapped-key cache, manifest cache, etc.

use crypto::x25519;
use keystore::{Identity, KeyServerClient};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Time-to-live for cached sender public keys. Bounded staleness
/// when a peer rotates their identity key — Phase 5 doesn't have
/// push-based invalidation, so cached entries can hold an
/// outdated key for at most this long. Re-registration through
/// the keyserver overwrites the public-key record but our cache
/// would still serve the prior value until the entry expires;
/// after expiry we refetch and pick up the new key.
///
/// 30 minutes balances staleness against keyserver request
/// volume. Identity-key rotation is a rare event in practice
/// (tied to duress reinstall or major-incident response, NOT
/// per-conversation lifecycle), so a half-hour staleness window
/// is acceptable; an active dogfood session of N peers exchanging
/// M messages takes O(N) keyserver requests rather than O(M).
/// Long-term answer is keyserver-pushed invalidation events over
/// a websocket; v2.
pub const SENDER_PUBKEY_CACHE_TTL: Duration = Duration::from_secs(1800);

/// Single cache entry: the fetched X25519 public key plus the
/// `Instant` it was inserted, used for TTL eviction.
#[derive(Clone)]
struct CachedPubkey {
    pubkey: x25519::PublicKey,
    inserted_at: Instant,
}

/// Bounded TTL cache from `user_id` to fetched X25519 public key.
///
/// Used by [`crate::commands::cmd_osl_decrypt_message`] to avoid
/// hitting the keyserver on every incoming message. Cache
/// invariant: entries older than [`SENDER_PUBKEY_CACHE_TTL`] are
/// treated as absent (lazy eviction on read).
///
/// Bounds: the cache is uncapped in entry count. For the
/// closed-beta dogfood of two-to-three users this is fine; if a
/// pathological pattern adds many user_ids over time, the worst-
/// case memory is `N * (32 bytes pubkey + 16 bytes Instant + ~16
/// bytes hashmap overhead)`. v2 adds an LRU cap if scale demands.
#[derive(Default)]
pub struct SenderPubkeyCache {
    entries: Mutex<HashMap<String, CachedPubkey>>,
}

impl SenderPubkeyCache {
    /// Look up a cached entry. Returns `None` if absent OR if
    /// expired (lazy eviction in this case).
    pub fn get(&self, user_id: &str) -> Option<x25519::PublicKey> {
        let mut guard = self.entries.lock().expect("pubkey cache mutex poisoned");
        match guard.get(user_id) {
            Some(entry) if entry.inserted_at.elapsed() < SENDER_PUBKEY_CACHE_TTL => {
                Some(entry.pubkey.clone())
            }
            Some(_) => {
                // Expired — evict so the entry doesn't stay
                // around indefinitely on the rare path where it's
                // looked up but never re-inserted.
                guard.remove(user_id);
                None
            }
            None => None,
        }
    }

    /// Insert or replace a cache entry. Replaces any prior entry
    /// for `user_id` regardless of staleness.
    pub fn insert(&self, user_id: String, pubkey: x25519::PublicKey) {
        let mut guard = self.entries.lock().expect("pubkey cache mutex poisoned");
        guard.insert(
            user_id,
            CachedPubkey {
                pubkey,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Evict every entry. Useful for cmd_init_keyserver when the
    /// keyserver URL changes and prior cached entries should not
    /// be trusted under the new keyserver.
    pub fn clear(&self) {
        let mut guard = self.entries.lock().expect("pubkey cache mutex poisoned");
        guard.clear();
    }

    /// Count of currently-resident entries (including not-yet-
    /// evicted expired ones). Diagnostic and used by integration
    /// tests; production code rarely calls this.
    pub fn len(&self) -> usize {
        self.entries.lock().expect("pubkey cache mutex poisoned").len()
    }

    /// Whether the cache has zero entries. Provided so clippy
    /// doesn't flag `len() == 0` against [`Self::len`].
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Default)]
pub struct AppState {
    pub identity: Mutex<Option<Identity>>,
    pub keyserver: Mutex<Option<KeyServerClient>>,
    pub sender_pubkey_cache: SenderPubkeyCache,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    pub fn has_identity(&self) -> bool {
        self.identity.lock().expect("identity mutex poisoned").is_some()
    }

    pub fn has_keyserver(&self) -> bool {
        self.keyserver.lock().expect("keyserver mutex poisoned").is_some()
    }
}
