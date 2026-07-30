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

use std::path::PathBuf;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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

/// In-memory truth about the latest attempt to publish this device's public
/// identity to the configured keyserver.  A constructed HTTP client is not
/// proof that the remote registration exists, so the Hub must gate encrypted
/// messaging on this state rather than [`AppState::has_keyserver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CloudRegistrationState {
    NotAttempted = 0,
    Pending = 1,
    Registered = 2,
    Conflict = 3,
    Offline = 4,
}

impl CloudRegistrationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAttempted => "notAttempted",
            Self::Pending => "pending",
            Self::Registered => "registered",
            Self::Conflict => "conflict",
            Self::Offline => "offline",
        }
    }
}

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
                Some(entry.pubkey)
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

/// One complete peer-bundle change the user must acknowledge. Raised
/// when any signed Ed25519, X25519, ML-KEM or ratchet-bootstrap key
/// differs from the trusted baseline. The live keys remain unchanged
/// until the user verifies the new bundle number and accepts it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeyChangeAlert {
    pub discord_id: String,
    pub osl_user_id: Option<String>,
    /// base64 Ed25519 pub previously trusted (the TOFU baseline).
    pub old_ed25519_pub: String,
    /// base64 Ed25519 pub the keyserver just returned.
    pub new_ed25519_pub: String,
    /// Safety number of the complete new bundle.
    pub new_safety_number: String,
    /// First time this change was observed (ISO-8601).
    pub first_observed: String,
    /// Complete pending bundle. Public key material, but not part of
    /// the renderer DTO; acceptance adopts exactly what the displayed
    /// bundle safety number covered.
    #[serde(skip_serializing)]
    pub pending_bundle: crate::tofu::KeyBundle,
}

pub struct AppState {
    /// Serializes in-process Discord-account switches. The active account
    /// directory is process-global, so two concurrent switch commands must
    /// never interleave validation, marker updates, and state reloads.
    pub account_switch_lock: Mutex<()>,
    pub identity: Mutex<Option<Identity>>,
    /// Live prekey lifecycle for the loaded identity. This is absent
    /// until an identity is explicitly installed; default AppState must
    /// refuse prekey-dependent work rather than manufacturing authority.
    pub prekey_state: Mutex<Option<keystore::PrekeyState>>,
    pub keyserver: Mutex<Option<KeyServerClient>>,
    /// Production duress wipe engine for this launch. It is constructed from
    /// the active account directory plus the device-level password directory
    /// during AppState setup, then invoked by password-gate paths that detect
    /// a duress condition.
    pub duress_engine: Mutex<keystore::DuressEngine>,
    pub duress_journal_path: PathBuf,

    /// Latest confirmed outcome of this process's remote public-key
    /// registration. This is deliberately launch-local: every launch retries
    /// registration and must prove the current public keys again.
    pub cloud_registration_state: AtomicU8,

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

    /// Sealed OSL-RN session and pin store for the active account. The sealer
    /// is process-local and selected with the same best-available policy as
    /// identity storage; callers must not construct ad hoc plaintext RN stores.
    pub rn_session_store: crate::wire_rn::RnSessionStore,
    pub rn_session_sealer: Box<dyn keystore::Sealer>,

    /// Per-scope whitelist + encryption-toggle state, mirroring
    /// `<config_dir>/whitelist_state.json`. Empty by default —
    /// 7b's send-path queries this every encrypt to decide whether
    /// and who to wrap K for. Loaded at bootstrap (Phase 7b integration).
    /// Mutating Tauri commands must write-through
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

    /// Owner go/no-go switch for v=5 group sender chains. Defaults enabled so
    /// group/server sends take the sender-key path unless a caller explicitly
    /// disables it for compatibility testing.
    pub sender_keys_enabled: AtomicBool,

    /// Runtime gate for OSL-RN wire-in.
    ///
    /// Defaults false, is in-memory only, and is separate from
    /// `wire_rn::RN_WIRE_IN_ENABLED`, which remains the compile-time review
    /// fuse for builds that still must not wire OSL-RN into production flows.
    pub rn_wire_in_enabled: AtomicBool,

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

    /// Auto-recovery throttle + replay + act-on-symptom guard for the
    /// SKDM_REQUEST / SESSION_RESET control messages. In-memory only
    /// (see [`crate::recovery::RecoveryGuard`]); a relaunch resets it,
    /// whose worst case is one extra idempotent recovery round.
    pub recovery_guard: Mutex<crate::recovery::RecoveryGuard>,

    /// W1: durable scope-membership accrual (server/channel/GC →
    /// observed peers), fed by the gateway taps. The oracle the
    /// Option-B whitelist precedence + dynamic recipient resolution
    /// consult. Mirrors `membership.json`; safe to lose (re-accrues).
    pub scope_membership: Mutex<crate::membership::ScopeMembership>,
    // F3.6 pivot: `launch_time` and `free_tier_unlocked_until`
    // (added in F3.1 for the 60-min launch-window + ad-unlock
    // model) are removed. The new model has unlimited free text
    // encryption + paid-only attachments; no clocks, no unlocks.
    // The license_state mutex above is the sole tier surface.
}

impl Default for AppState {
    fn default() -> Self {
        let (duress_engine, duress_journal_path) = default_production_duress_engine();
        Self {
            account_switch_lock: Mutex::new(()),
            identity: Mutex::new(None),
            prekey_state: Mutex::new(None),
            keyserver: Mutex::new(None),
            duress_engine: Mutex::new(duress_engine),
            duress_journal_path,
            cloud_registration_state: AtomicU8::new(CloudRegistrationState::NotAttempted as u8),
            identity_regenerated_this_launch: AtomicBool::new(false),
            registration_alert: Mutex::new(None),
            key_change_alerts: Mutex::new(HashMap::new()),
            sender_pubkey_cache: SenderPubkeyCache::default(),
            peer_map: Mutex::new(PeerMap::default()),
            message_store: Mutex::new(None),
            rn_session_store: default_rn_session_store(),
            rn_session_sealer: keystore::select_best_sealer(),
            whitelist_state: Mutex::new(WhitelistState::default()),
            recovery_token: Mutex::new(None),
            stealth_active: Mutex::new(false),
            burned_scopes: Mutex::new(crate::burned_scopes_file::BurnedScopesFile::default()),
            sender_key_state: Mutex::new(crate::sender_key_state::SenderKeyStateFile::default()),
            sender_keys_enabled: AtomicBool::new(true),
            channel_members: Mutex::new(HashMap::new()),
            app_preferences: Mutex::new(crate::app_preferences::AppPreferences::default()),
            mode1_reassembly: Mutex::new(HashMap::new()),
            friend_ids: Mutex::new(Vec::new()),
            guild_list: Mutex::new(Vec::new()),
            server_defaults: Mutex::new(HashMap::new()),
            last_persist_error: Mutex::new(None),
            license_state: Mutex::new(keystore::LicenseStateDto::default()),
            recovery_guard: Mutex::new(crate::recovery::RecoveryGuard::default()),
            scope_membership: Mutex::new(crate::membership::ScopeMembership::default()),
            // RN starts unwired. wire_rn::RN_WIRE_IN_ENABLED is the compile-time
            // fuse; this runtime flag must never default to a more permissive value.
            rn_wire_in_enabled: AtomicBool::new(false),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    /// Install an identity and construct its live prekey state in the
    /// same AppState transition. Callers that bypass this helper leave
    /// prekey-dependent production paths unavailable.
    pub fn install_identity(&self, identity: Identity) {
        self.try_install_identity(identity)
            .expect("identity/prekey mutex poisoned");
    }

    pub fn try_install_identity(&self, identity: Identity) -> Result<(), &'static str> {
        self.try_install_identity_at(identity, current_unix_seconds())
    }

    fn install_identity_at(&self, identity: Identity, now_unix_seconds: u64) {
        self.try_install_identity_at(identity, now_unix_seconds)
            .expect("identity/prekey mutex poisoned");
    }

    fn try_install_identity_at(
        &self,
        identity: Identity,
        now_unix_seconds: u64,
    ) -> Result<(), &'static str> {
        let prekeys = keystore::PrekeyState::new(
            &identity,
            keystore::PrekeyConfig::default(),
            now_unix_seconds,
        );
        self.try_install_identity_with_prekey_state(identity, prekeys)
    }

    /// Install an identity with a prekey state already loaded from durable
    /// storage. Startup uses this to avoid replacing the published prekey pool
    /// with a fresh, unpublished one.
    pub fn install_identity_with_prekey_state(
        &self,
        identity: Identity,
        prekeys: keystore::PrekeyState,
    ) {
        self.try_install_identity_with_prekey_state(identity, prekeys)
            .expect("identity/prekey mutex poisoned");
    }

    pub fn try_install_identity_with_prekey_state(
        &self,
        identity: Identity,
        prekeys: keystore::PrekeyState,
    ) -> Result<(), &'static str> {
        let mut identity_slot = self
            .identity
            .lock()
            .map_err(|_| "identity mutex poisoned")?;
        let mut prekey_slot = self
            .prekey_state
            .lock()
            .map_err(|_| "prekey_state mutex poisoned")?;
        *identity_slot = Some(identity);
        *prekey_slot = Some(prekeys);
        Ok(())
    }

    pub fn set_prekey_state(&self, prekeys: keystore::PrekeyState) {
        *self
            .prekey_state
            .lock()
            .expect("prekey_state mutex poisoned") = Some(prekeys);
    }

    pub fn clear_prekey_state(&self) {
        *self
            .prekey_state
            .lock()
            .expect("prekey_state mutex poisoned") = None;
    }

    /// Clear identity-owned live state. Account switches, imports, and burn
    /// resets must not leave a stale prekey pool associated with no identity.
    pub fn clear_identity(&self) {
        *self.identity.lock().expect("identity mutex poisoned") = None;
        self.clear_prekey_state();
    }

    pub fn has_identity(&self) -> bool {
        self.identity
            .lock()
            .expect("identity mutex poisoned")
            .is_some()
    }

    pub fn has_prekey_state(&self) -> bool {
        self.prekey_state
            .lock()
            .expect("prekey_state mutex poisoned")
            .is_some()
    }

    pub fn has_keyserver(&self) -> bool {
        self.keyserver
            .lock()
            .expect("keyserver mutex poisoned")
            .is_some()
    }

    pub fn set_cloud_registration_state(&self, state: CloudRegistrationState) {
        self.cloud_registration_state
            .store(state as u8, Ordering::Release);
    }

    pub fn cloud_registration_state(&self) -> CloudRegistrationState {
        match self.cloud_registration_state.load(Ordering::Acquire) {
            1 => CloudRegistrationState::Pending,
            2 => CloudRegistrationState::Registered,
            3 => CloudRegistrationState::Conflict,
            4 => CloudRegistrationState::Offline,
            _ => CloudRegistrationState::NotAttempted,
        }
    }

    pub fn rn_wire_in_enabled(&self) -> bool {
        self.rn_wire_in_enabled.load(Ordering::Acquire)
    }

    pub fn set_rn_wire_in_enabled(&self, enabled: bool) {
        self.rn_wire_in_enabled.store(enabled, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rn_wire_in_runtime_gate_defaults_to_refusal() {
        let state = AppState::new();

        assert!(!state.rn_wire_in_enabled());
    }

    #[test]
    fn rn_wire_in_runtime_gate_is_app_state_controlled() {
        let state = AppState::new();

        state.set_rn_wire_in_enabled(true);
        assert!(state.rn_wire_in_enabled());
        state.set_rn_wire_in_enabled(false);
        assert!(!state.rn_wire_in_enabled());
    }


    #[test]
    fn app_state_constructs_production_duress_engine() {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let account_dir = temp.path().join("account");
        let password_dir = temp.path().join("base");
        std::fs::create_dir_all(account_dir.join("store")).unwrap();
        std::fs::create_dir_all(&password_dir).unwrap();
        std::fs::write(account_dir.join("identity.json"), b"identity").unwrap();
        std::fs::write(account_dir.join("prekeys.json"), b"prekeys").unwrap();
        std::fs::write(account_dir.join("store").join("messages.sqlite"), b"cache").unwrap();
        std::fs::write(password_dir.join("password_marker.json"), b"marker").unwrap();

        let mut config =
            keystore::ProductionDuressConfig::new(account_dir.clone(), password_dir.clone());
        config.purge_keyring = Some(Box::new(|| Ok(())));
        config.wipe_prekeys = Some(Box::new(|| Ok(())));
        config.wipe_double_ratchet = Some(Box::new(|| Ok(())));
        config.wipe_sender_keys = Some(Box::new(|| Ok(())));
        config.wipe_peer_ratchets = Some(Box::new(|| Ok(())));
        config.zeroize_in_memory = Some(Box::new(|| Ok(())));
        let parts = keystore::build_production_duress_handlers(config);

        let mut state = AppState::new();
        state.duress_journal_path = parts.journal_path.clone();
        *state
            .duress_engine
            .lock()
            .expect("duress_engine mutex poisoned") =
            keystore::DuressEngine::new(parts.journal_path, parts.paths, parts.handlers);

        assert_eq!(
            state.duress_journal_path,
            account_dir.join("duress.journal")
        );
        let report = state
            .duress_engine
            .lock()
            .expect("duress_engine mutex poisoned")
            .execute()
            .expect("state-held duress engine runs");

        assert!(report.completed);
        assert!(report.failed_steps().is_empty());
        assert!(report.skipped_steps().is_empty());
        assert!(!account_dir.join("identity.json").exists());
        assert!(!account_dir.join("prekeys.json").exists());
        assert!(!account_dir.join("store").exists());
        assert!(!password_dir.join("password_marker.json").exists());
        assert!(!state.duress_journal_path.exists());

    }
}

fn default_rn_session_store() -> crate::wire_rn::RnSessionStore {
    let dir = keystore::osl_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("osl-rn-session-store-unconfigured"));
    crate::wire_rn::RnSessionStore::new(dir.join("rn_sessions"))
}

fn default_production_duress_engine() -> (keystore::DuressEngine, PathBuf) {
    let account_dir = keystore::osl_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("osl-duress-account-unconfigured"));
    let password_dir = keystore::osl_base_dir().unwrap_or_else(|_| account_dir.clone());
    let parts = keystore::build_production_duress_handlers(keystore::ProductionDuressConfig::new(
        account_dir,
        password_dir,
    ));
    let journal_path = parts.journal_path.clone();
    (
        keystore::DuressEngine::new(parts.journal_path, parts.paths, parts.handlers),
        journal_path,
    )
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod prekey_authority_tests {
    use super::*;

    #[test]
    fn default_state_has_no_prekey_authority() {
        let state = AppState::new();

        assert!(!state.has_identity());
        assert!(!state.has_prekey_state());
    }

    #[test]
    fn installing_identity_constructs_live_prekey_state() {
        let state = AppState::new();
        let identity = keystore::generate_identity("prekey-owner".to_owned());

        state.install_identity_at(identity, 1_700_000_000);

        assert!(state.has_identity());
        let prekeys = state
            .prekey_state
            .lock()
            .expect("prekey_state mutex poisoned");
        let prekeys = prekeys.as_ref().expect("prekey state installed");
        assert_eq!(prekeys.current_spk.rotated_at_unix_seconds, 1_700_000_000);
        assert_eq!(
            prekeys.opk_pool.len(),
            keystore::PrekeyConfig::default().opk_pool_target as usize
        );
    }

    #[test]
    fn installing_identity_with_loaded_prekeys_preserves_persisted_state() {
        let state = AppState::new();
        let identity = keystore::generate_identity("prekey-owner".to_owned());
        let persisted =
            keystore::PrekeyState::new(&identity, keystore::PrekeyConfig::default(), 42);

        state.install_identity_with_prekey_state(identity, persisted);

        let prekeys = state
            .prekey_state
            .lock()
            .expect("prekey_state mutex poisoned");
        let prekeys = prekeys.as_ref().expect("prekey state installed");
        assert_eq!(prekeys.current_spk.rotated_at_unix_seconds, 42);
    }

    #[test]
    fn clearing_identity_also_clears_live_prekey_state() {
        let state = AppState::new();
        state.install_identity_at(
            keystore::generate_identity("prekey-owner".to_owned()),
            1_700_000_000,
        );

        state.clear_identity();

        assert!(!state.has_identity());
        assert!(!state.has_prekey_state());
    }

    #[test]
    fn app_state_constructs_rn_session_store_with_sealer() {
        let state = AppState::new();
        let peer = [0x42u8; 32];

        let absent_pin = state
            .rn_session_store
            .load_pin(&peer)
            .expect("fresh RN store reads an absent pin");

        assert_eq!(absent_pin, crate::wire_rn::RnPeerPin::UNKNOWN);
        assert!(
            !state.rn_session_sealer.requires_insecure_banner(),
            "AppState RN sealer must not be a plaintext sealer"
        );
        assert_ne!(
            state.rn_session_sealer.method_label(),
            keystore::METHOD_NOOP
        );
    }

    #[test]
    fn sender_keys_enabled_defaults_to_enabled_after_owner_go() {
        let state = AppState::new();

        assert!(state.sender_keys_enabled.load(Ordering::Acquire));

        state.sender_keys_enabled.store(false, Ordering::Release);
        assert!(
            !state.sender_keys_enabled.load(Ordering::Acquire),
            "the test must exercise the AppState default, not a hardwired accessor"
        );
    }
}
