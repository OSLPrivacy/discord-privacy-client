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

use crate::peer_map::PeerMap;
use crate::whitelist_state::WhitelistState;
use crypto::x25519;
use keystore::{Identity, KeyServerClient};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use store::MessageStore;

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
        self.entries
            .lock()
            .expect("pubkey cache mutex poisoned")
            .len()
    }

    /// Whether the cache has zero entries. Provided so clippy
    /// doesn't flag `len() == 0` against [`Self::len`].
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// REGISTER-FIX (TOFU): one peer key-change the user must
/// acknowledge. Raised when a peer's `ik_ed25519_pub` returned by
/// `fetch_pubkeys` differs from the trusted first-seen baseline in
/// `peer_map`. Surfaced (NOT warn-swallowed) and held until the user
/// explicitly accepts (baseline → new) or declines (baseline kept).
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyChangeAlert {
    pub discord_id: String,
    pub osl_user_id: Option<String>,
    /// base64 Ed25519 pub previously trusted (the TOFU baseline).
    pub old_ed25519_pub: String,
    /// base64 Ed25519 pub the keyserver just returned.
    pub new_ed25519_pub: String,
    /// Safety number of the NEW key (for out-of-band comparison).
    pub new_safety_number: String,
    /// First time this change was observed (ISO-8601).
    pub first_observed: String,
}

#[derive(Default)]
pub struct AppState {
    pub identity: Mutex<Option<Identity>>,
    pub keyserver: Mutex<Option<KeyServerClient>>,

    /// D: set true by `run_autostart` when THIS launch regenerated
    /// the local identity outside a burn. `state_reload`'s post-gate
    /// reload consumes it to durably clear every peer's stale
    /// `ratchet_state` (the bootstrap pre-gate clear can't persist
    /// when the peer_map is encrypt-at-rest and the key isn't
    /// installed yet). Launch-scoped; never persisted.
    pub identity_regenerated_this_launch: std::sync::atomic::AtomicBool,

    /// REGISTER-FIX: a security-relevant registration outcome the
    /// user MUST see (NOT warn-swallowed) — set when `/v1/register`
    /// returns 403 "user_id registered to a different key" (our
    /// snowflake is held by another key: squat or lost key). Read +
    /// cleared by `cmd_osl_take_registration_alert`.
    pub registration_alert: Mutex<Option<String>>,

    /// REGISTER-FIX (TOFU): pending peer key-change alerts, keyed by
    /// peer Discord id. Populated by the `fetch_pubkeys` TOFU check;
    /// drained/resolved via the key-change IPC commands. In-memory:
    /// a relaunch re-derives them on the next fetch if still changed.
    pub key_change_alerts: Mutex<HashMap<String, KeyChangeAlert>>,
    pub sender_pubkey_cache: SenderPubkeyCache,
    /// Discord-id → OSL-user-id translation for receive-side
    /// decryption. Populated at bootstrap from
    /// `<osl_config_dir>/peer_map.json`. Empty by default — an
    /// empty map causes every receive to return `UnknownSender`,
    /// which the JS hook treats as "leave cover in place." See
    /// [`crate::peer_map`].
    pub peer_map: Mutex<PeerMap>,

    /// Persistent at-rest-encrypted message store. Opened at
    /// bootstrap once the identity secret is available; held as
    /// `None` if open fails, in which case `cmd_osl_decrypt_message`
    /// still succeeds (plaintext is just not persisted) and
    /// `cmd_osl_load_channel_history` returns an empty list. See
    /// `crates/store` for the on-disk crypto + schema posture.
    pub message_store: Mutex<Option<MessageStore>>,

    /// Per-scope whitelist + encryption-toggle state, mirroring
    /// `<config_dir>/whitelist_state.json`. Empty by default —
    /// 7b's send-path queries this every encrypt to decide whether
    /// + who to wrap K for. Loaded at bootstrap (Phase 7b
    /// integration). Mutating Tauri commands must write-through
    /// to disk via `crate::whitelist_state::write_whitelist_state`.
    pub whitelist_state: Mutex<WhitelistState>,

    // 9-C1: `pending_invitations` field removed alongside the
    // invitation handshake subsystem. The on-disk
    // `pending_invitations.json` is unconditionally deleted at
    // bootstrap.
    /// Phase 7d-B1: one-time recovery token issued by
    /// `osl_verify_recovery_phrase` and consumed by
    /// `osl_set_main_password_after_recovery`. Tuple is
    /// (token, expiry_unix_secs, phrase). In-memory only — a
    /// crash between phrase verify and password set discards the
    /// token; the user re-enters the phrase. Cleared (`take()`)
    /// by the consume path regardless of match.
    pub recovery_token: Mutex<Option<(String, i64, String)>>,

    /// Phase 7d-B2: stealth-mode session flag. Set to true by
    /// `osl_stealth_mode_engage` after a successful stealth-password
    /// gate verify. The initialization_script consults this flag
    /// (via a URL hash, since boot.js can't synchronously call
    /// Tauri commands) to skip the entire OSL feature install
    /// — the user sees vanilla Discord for the rest of the
    /// session. Reset on app restart (in-memory only).
    pub stealth_active: Mutex<bool>,

    /// 7d-FIX1: explicit-burn ledger. Mirrors
    /// `burned_scopes.json`; the boot.js receive observer pulls
    /// this list at install via `osl_list_burned_scopes` and skips
    /// decrypt dispatch for any message whose scope appears here.
    /// `osl_burn_scope_data` appends; `osl_unburn_scope` removes;
    /// `cmd_osl_set_whitelist` also evicts on re-whitelist (decision
    /// B from the spec — re-whitelist removes the burn entry so
    /// fresh messages decrypt normally).
    pub burned_scopes: Mutex<crate::burned_scopes_file::BurnedScopesFile>,

    /// Phase 9-A3: per-group sender-keys state, mirroring
    /// `sender_key_state.json`. One row per group/server scope. The
    /// send-side dispatcher consults this to decide v=5 vs v=3,
    /// installs/rotates sender chains, and persists on every send.
    /// The recv-side path consults it to recover the
    /// per-(scope, sender) receiver chain.
    pub sender_key_state: Mutex<crate::sender_key_state::SenderKeyStateFile>,

    /// Phase 9-A3: in-memory cache of the current channel-member set
    /// per channel_id. Populated by `osl_membership_update` (boot.js
    /// pushes gateway-derived membership). Consulted by the v=5 send
    /// dispatcher to detect membership changes against
    /// `SenderChain.last_known_members`. Not persisted: only the
    /// SenderChain's snapshot is durable.
    pub channel_members: Mutex<std::collections::HashMap<String, Vec<String>>>,

    /// Phase 9-B1: app-wide user preferences (stego mode selector,
    /// Mode 1 preview confirmations). Mirrors
    /// `<config_dir>/app_preferences.json`. Loaded at bootstrap.
    /// Mutated via `osl_set_app_preferences`; write-through to disk
    /// is the caller's responsibility.
    pub app_preferences: Mutex<crate::app_preferences::AppPreferences>,

    /// Phase 9-B1: per-channel Mode 1 receive-side reassembly state.
    /// Sessions are bounded to 16 concurrent and expire after 5
    /// minutes (see [`stego::ReassemblyBuffer`]). Not persisted —
    /// receive replays after a restart will reassemble fresh.
    pub mode1_reassembly: Mutex<HashMap<String, stego::ReassemblyBuffer>>,

    /// Phase 9-C2: ephemeral list of the user's Discord friend ids
    /// (relationships with type=1). Pushed from boot.js's gateway-tap
    /// READY handler via `osl_set_friend_ids`; consumed by the
    /// settings-window's Bulk Whitelist modal. Not persisted —
    /// repopulated on every Discord reconnect.
    pub friend_ids: Mutex<Vec<String>>,

    /// Phase 9-C2: ephemeral list of guilds the user has access to,
    /// each carrying the gateway-loaded subset of members. Pushed
    /// from boot.js's gateway-tap GUILD_CREATE handler via
    /// `osl_set_guild_list`; consumed by the Bulk Whitelist modal's
    /// server-picker. Not persisted; member_ids may be partial for
    /// large guilds (Discord ships only ~100 online members at
    /// GUILD_CREATE time).
    pub guild_list: Mutex<Vec<crate::commands::GuildDto>>,

    /// Phase 9-C3: per-server "encrypt new channels by default"
    /// preference. Persisted alongside `whitelist_state.json` in the
    /// envelope's `server_defaults` field. Separate Mutex from
    /// `whitelist_state` for lock granularity — the CHANNEL_CREATE
    /// auto-apply hook reads this without taking the (potentially
    /// contended) whitelist_state lock.
    pub server_defaults:
        Mutex<std::collections::HashMap<String, crate::whitelist_state::ServerDefaults>>,

    /// 9-TD1.4: most-recent disk-persist failure message. Pre-TD1
    /// every `persist_*_now` swallowed errors silently with a
    /// `tracing::warn!`; the user thought their whitelist / burn /
    /// preference change was saved but it only lived in memory.
    /// Each persist path now stores its failure here (overwriting
    /// any prior value — last-write-wins is fine for "something
    /// went wrong, please retry" UX). `cmd_osl_take_last_persist_error`
    /// reads + clears the slot so the JS layer can surface a toast.
    pub last_persist_error: Mutex<Option<String>>,

    /// F2.4: in-memory license classification. Populated at launch
    /// by `crate::license_lifecycle::launch_classify` (synchronous
    /// cache load — no network) and refreshed by
    /// `refresh_license_state` (called from bootstrap's async
    /// follow-up + the 6h cron task in main.rs setup). Read by
    /// `cmd_osl_get_license_state` on every render of any UI that
    /// gates on paid status; F3's ad gate will be the heaviest
    /// reader.
    ///
    /// Defaults to `LicenseStateDto::unconfigured()` so a fresh
    /// `AppState` (or one whose launch hook hasn't run yet)
    /// reads as Free, not as a crashed unwrap.
    pub license_state: Mutex<keystore::LicenseStateDto>,
    // F3.6 pivot: `launch_time` and `free_tier_unlocked_until`
    // (added in F3.1 for the 60-min launch-window + ad-unlock
    // model) are removed. The new model has unlimited free text
    // encryption + paid-only attachments; no clocks, no unlocks.
    // The license_state mutex above is the sole tier surface.
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    pub fn has_identity(&self) -> bool {
        self.identity
            .lock()
            .expect("identity mutex poisoned")
            .is_some()
    }

    pub fn has_keyserver(&self) -> bool {
        self.keyserver
            .lock()
            .expect("keyserver mutex poisoned")
            .is_some()
    }
}
