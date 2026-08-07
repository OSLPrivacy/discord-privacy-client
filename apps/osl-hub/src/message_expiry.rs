//! Timed deletion and view-once expiry that actually happen.
//!
//! ## What this module owns
//!
//! * **Two clocks.** A timed message carries an *absolute* deadline the relay
//!   enforces, and a *relative* open clock the receiver enforces from the first
//!   authenticated local open. The relative clock is the default;
//!   `docs/design/offline-controls-and-opened-receipts.md` lines 10-11 are
//!   authoritative over any code comment that says otherwise.
//! * **A durable, sealed ledger** of those clocks, per scope and message, whose
//!   record is a [`message_lifecycle::LogicalMessageLifecycle`] — the single
//!   owner of the receipt state machine, its terminal transitions, and the
//!   first-open timestamp.
//! * **A durable view-once receipt-dedup ledger**, so a restart does not make
//!   the sender collect a second `Received` receipt for the same message.
//! * **One bounded sweep pass**, [`run_pass`], that a single process-wide tick
//!   calls. It prunes both ledgers, shreds the local cached plaintext of
//!   whatever expired, and clears abandoned decrypted staging files.
//!
//! ## What it deliberately does not claim
//!
//! Against a peer running this software, view-once and expiry are enforceable:
//! the replay ledger and the clocks here are real. Against a modified client, a
//! screenshot, a second device or a camera they are not. Nothing here is a
//! continuous timer either — the honest claim is that expired content is purged
//! within one tick of the next start, and is unreadable before that because
//! every open path checks expiry and fails closed.
//!
//! ## Fail closed
//!
//! The read path returns [`ExpiryVerdict`], never a `Result<bool, _>`. An
//! unreadable, malformed, over-bounds or locked ledger yields
//! [`ExpiryVerdict::Expired`]. There is no value a caller can `unwrap_or` into
//! "still fresh".
//!
//! ## Secrets
//!
//! Nothing here accepts, stores, hashes, logs or reports plaintext, drafts, key
//! material or conversation content. The ledger holds opaque scope keys, opaque
//! random message ids, byte counts, digests of *sealed* bytes, and timestamps.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use message_lifecycle::{
    AcceptedPart, LifecycleLimits, LogicalMessageLifecycle, Mutation, ReceiptStatus,
    TransitionEvidence, HARD_MAX_OPEN_TTL_SECONDS,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::control_contract::TimedMessageMode;

// ---------------------------------------------------------------------------
// Two clocks
// ---------------------------------------------------------------------------

/// The lifetimes OSL offers, in seconds.
///
/// Taken from the cipher store's own allowlist rather than restated, because a
/// value the relay will not accept is not a lifetime OSL can offer.
pub const TTL_ALLOWLIST: [u32; 5] = [
    ipc::cipher_store_client::TTL_1H,
    ipc::cipher_store_client::TTL_24H,
    ipc::cipher_store_client::TTL_72H,
    ipc::cipher_store_client::TTL_7D,
    ipc::cipher_store_client::TTL_30D,
];

/// Hard ceiling on any absolute deadline. Thirty days, matching the longest
/// product timer.
/// marked message timer.
pub const MAX_ABSOLUTE_TTL_SECONDS: u32 = ipc::cipher_store_client::TTL_30D;

/// How long the relay holds ciphertext for a *relative*-clock message.
///
/// The relative clock cannot start until the receiver opens, and the relay
/// cannot observe that open. So the delivery window has to be long enough that
/// a recipient who is away for a few days still gets the message the sender
/// meant them to have — otherwise the "clock starts at first open" promise is
/// quietly broken by a delivery window that expired first.
///
/// Thirty days is the product ceiling, and the tradeoff is explicit: a
/// relative-clock message's *ciphertext* may sit in relay storage for up to
/// that window, where the relay necessarily observes object size and access
/// time.
/// Thirty days is the relay's own ceiling, and the tradeoff is explicit: a
/// relative-clock message's *ciphertext* may sit in relay storage for up to a
/// month, where the relay necessarily observes object size and access time.
/// Callers that prefer a tighter window pass one to
/// [`relative_release_within`].
pub const DEFAULT_DELIVERY_WINDOW_SECONDS: u32 = ipc::cipher_store_client::TTL_30D;

fn ttl_is_offered(ttl_seconds: u32) -> bool {
    TTL_ALLOWLIST.contains(&ttl_seconds)
}

/// Both clocks a timed message carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedRelease {
    /// The hard deadline, absolute unix seconds. The relay enforces this one
    /// because it is the only one the relay can observe.
    pub absolute_expires_at: i64,
    /// The relative lifetime in seconds from the first authenticated local
    /// open, or `None` for the advanced fixed-absolute option.
    pub open_ttl_seconds: Option<u32>,
}

impl TimedRelease {
    /// The same two clocks in the offline-control contract's own vocabulary.
    ///
    /// Times are milliseconds there, seconds here; the relay and the ledger
    /// both work in seconds, so seconds is the storage unit and this is the
    /// conversion boundary.
    pub fn mode(self) -> TimedMessageMode {
        match self.open_ttl_seconds {
            Some(ttl) => TimedMessageMode::FirstAuthenticatedOpen {
                lifetime_ms: u64::from(ttl).saturating_mul(1_000),
            },
            None => TimedMessageMode::FixedAbsolute {
                expires_at_ms: self.absolute_expires_at.max(0) as u64 * 1_000,
            },
        }
    }

    /// The TTL the relay upload must declare for this release.
    pub fn relay_ttl_seconds(self, now: i64) -> Result<u32, String> {
        let span = self.absolute_expires_at.saturating_sub(now);
        u32::try_from(span)
            .ok()
            .filter(|value| ttl_is_offered(*value))
            .ok_or_else(|| "OSL message lifetime is unsupported".to_owned())
    }
}

/// The default: the clock begins at the receiver's first authenticated local
/// open, under a thirty-day absolute ceiling.
pub fn relative_release(now: i64, open_ttl_seconds: u32) -> Result<TimedRelease, String> {
    relative_release_within(now, open_ttl_seconds, DEFAULT_DELIVERY_WINDOW_SECONDS)
}

/// The default clock with an explicit delivery window.
///
/// `delivery_window_seconds` must itself be one of the offered lifetimes, and
/// must not be shorter than the open clock: a window that expires before the
/// timer it is supposed to contain would silently destroy an unread message,
/// which is the advanced option's documented hazard, not the default's.
pub fn relative_release_within(
    now: i64,
    open_ttl_seconds: u32,
    delivery_window_seconds: u32,
) -> Result<TimedRelease, String> {
    if !ttl_is_offered(open_ttl_seconds)
        || !ttl_is_offered(delivery_window_seconds)
        || delivery_window_seconds < open_ttl_seconds
        || open_ttl_seconds > HARD_MAX_OPEN_TTL_SECONDS
    {
        return Err("OSL message lifetime is unsupported".to_owned());
    }
    Ok(TimedRelease {
        absolute_expires_at: now.saturating_add(i64::from(delivery_window_seconds)),
        open_ttl_seconds: Some(open_ttl_seconds),
    })
}

/// The advanced option: a fixed absolute deadline and no open clock.
///
/// This may make a message that was never opened unrecoverable. That is the
/// documented behaviour of this mode, and the reason it is not the default.
pub fn absolute_release(now: i64, ttl_seconds: u32) -> Result<TimedRelease, String> {
    if !ttl_is_offered(ttl_seconds) || ttl_seconds > MAX_ABSOLUTE_TTL_SECONDS {
        return Err("OSL message lifetime is unsupported".to_owned());
    }
    Ok(TimedRelease {
        absolute_expires_at: now.saturating_add(i64::from(ttl_seconds)),
        open_ttl_seconds: None,
    })
}

// ---------------------------------------------------------------------------
// Verdicts
// ---------------------------------------------------------------------------

/// Whether a message may still be opened, and under which deadline.
///
/// Returned by value rather than in a `Result` so an error cannot be folded
/// into "fresh". Every failure — locked, missing, malformed, over-bounds,
/// undecryptable — is [`Self::Expired`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpiryVerdict {
    /// The relative clock has not started. The absolute deadline is the only
    /// one in force until the first authenticated local open.
    AwaitingFirstOpen { absolute_expires_at: i64 },
    /// Readable until `effective_expires_at`, the earlier of the two clocks.
    Readable { effective_expires_at: i64 },
    /// Not readable. Also the answer whenever the ledger cannot be trusted.
    Expired,
}

impl ExpiryVerdict {
    pub fn is_readable(self) -> bool {
        !matches!(self, Self::Expired)
    }
}

// ---------------------------------------------------------------------------
// Sealed open-clock ledger
// ---------------------------------------------------------------------------

const OPEN_CLOCK_FILE: &str = "message_open_clock.json";
const OPEN_CLOCK_LABEL: &str = "OSL message expiry ledger";
pub const TIMED_DELETE_FILE: &str = "timed_delete_records.json";
const TIMED_DELETE_LABEL: &str = "OSL timed-delete ledger";
const TIMED_DELETE_PRO_REQUIRED: &str = "Timed delete requires OSL Pro";

/// Bounds mirroring the peer replay ledger, scaled for a heavier record.
///
/// A record is a full lifecycle snapshot, not a single timestamp, so the entry
/// caps are lower and the byte cap higher than the replay ledger's. Every cap is
/// a *rejection* boundary, never an eviction one.
const MAX_OPEN_CLOCK_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OPEN_CLOCK_SCOPES: usize = 512;
const MAX_OPEN_CLOCK_ENTRIES_PER_SCOPE: usize = 512;
const MAX_OPEN_CLOCK_ENTRIES_TOTAL: usize = 2_048;
const MAX_TIMED_DELETE_BYTES: u64 = 512 * 1024;
const MAX_TIMED_DELETE_ENTRIES: usize = 2_048;

/// Longest opaque identifier the ledger will key on.
const MAX_ID_LEN: usize = 96;

/// Limits for a receiver-side snapshot. The receiver reassembled the parts, so
/// these are the transport's own bounds, not a policy choice.
const RECEIVER_LIMITS: LifecycleLimits = LifecycleLimits {
    max_parts: 256,
    max_part_bytes: 64 * 1024,
    max_total_bytes: 16 * 1024 * 1024,
};

const MESSAGE_ID_DOMAIN: &[u8] = b"osl-message-expiry/message-id/v1";
const SCOPE_KEY_DOMAIN: &[u8] = b"osl-message-expiry/scope-key/v1";

/// Domain-separated commitment to an opaque identifier.
///
/// The inputs are random tokens and opaque scope keys — never plaintext, a
/// draft, a handle or a chat title — so this narrows a variable-length id to the
/// fixed 32 bytes the state machine wants without carrying content anywhere.
fn commitment(domain: &[u8], value: &str) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update((value.len() as u64).to_be_bytes());
    hash.update(value.as_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    // The state machine rejects an all-zero commitment. SHA-256 will not
    // produce one, but fail closed rather than rely on that.
    if digest == [0u8; 32] {
        return [1u8; 32];
    }
    digest
}

/// Opaque identifiers only: lowercase hex, digits and `-`, bounded.
///
/// Deliberately narrower than the transport's own id validation. A key this
/// rejects makes the whole ledger malformed, and a malformed ledger reads as
/// expired, so being strict here can only ever destroy content early — never
/// keep it alive past its deadline.
fn is_opaque_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || byte == b'-' || byte == b':' || byte.is_ascii_lowercase()
        })
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OpenClockRecord {
    /// The receipt state machine. Single owner of both clocks, the first-open
    /// timestamp, and the terminal transitions.
    lifecycle: LogicalMessageLifecycle,
    /// The local cached-plaintext row this message materialized into, when it
    /// has one. Present for carrier-history messages; absent for relay-only
    /// messages that were never written to the local cache. Expiry shreds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cache_id: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OpenClockLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    scopes: BTreeMap<String, BTreeMap<String, OpenClockRecord>>,
}

impl OpenClockLedger {
    fn total_entries(&self) -> usize {
        self.scopes.values().map(BTreeMap::len).sum()
    }
}

fn load_open_clock(path: &Path, key: &[u8; 32]) -> Result<OpenClockLedger, String> {
    let Some(bytes) =
        crate::atomic_file::read_recoverable_bounded(path, MAX_OPEN_CLOCK_BYTES, OPEN_CLOCK_LABEL)?
    else {
        return Ok(OpenClockLedger::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err(format!("{OPEN_CLOCK_LABEL} is not encrypted"));
    }
    let plain = Zeroizing::new(
        ipc::main_password::decrypt_at_rest(&bytes, key)
            .map_err(|_| format!("{OPEN_CLOCK_LABEL} could not be opened"))?,
    );
    let ledger: OpenClockLedger =
        serde_json::from_slice(&plain).map_err(|_| format!("{OPEN_CLOCK_LABEL} is malformed"))?;
    if !matches!(ledger.version, 0 | 1)
        || ledger.scopes.len() > MAX_OPEN_CLOCK_SCOPES
        || ledger.total_entries() > MAX_OPEN_CLOCK_ENTRIES_TOTAL
        || ledger
            .scopes
            .values()
            .any(|entries| entries.len() > MAX_OPEN_CLOCK_ENTRIES_PER_SCOPE)
    {
        return Err(format!("{OPEN_CLOCK_LABEL} is malformed"));
    }
    // A record OSL cannot act on must fail closed rather than sit in the ledger
    // pretending to hold a live timer.
    for (scope_key, entries) in &ledger.scopes {
        if !is_opaque_id(scope_key) {
            return Err(format!("{OPEN_CLOCK_LABEL} is malformed"));
        }
        for (message_id, record) in entries {
            if !is_opaque_id(message_id)
                || record.lifecycle.validate_snapshot().is_err()
                || record
                    .cache_id
                    .as_deref()
                    .is_some_and(|value| !is_opaque_id(value))
            {
                return Err(format!("{OPEN_CLOCK_LABEL} is malformed"));
            }
        }
    }
    Ok(ledger)
}

fn store_open_clock(path: &Path, ledger: &OpenClockLedger, key: &[u8; 32]) -> Result<(), String> {
    let body = Zeroizing::new(
        serde_json::to_vec(ledger)
            .map_err(|_| format!("{OPEN_CLOCK_LABEL} could not be encoded"))?,
    );
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| format!("{OPEN_CLOCK_LABEL} could not be encrypted"))?;
    if sealed.len() as u64 > MAX_OPEN_CLOCK_BYTES {
        return Err(format!("{OPEN_CLOCK_LABEL} exceeds its storage limit"));
    }
    crate::atomic_file::write_recoverable(path, &sealed, OPEN_CLOCK_LABEL)
}

// ---------------------------------------------------------------------------
// Timed-delete creation ledger
// ---------------------------------------------------------------------------

/// Whether the carrier row is already protected by OSL encryption.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimedDeleteProtection {
    Protected,
    Ordinary,
}

impl TimedDeleteProtection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Protected => "protected",
            Self::Ordinary => "ordinary",
        }
    }
}

/// One timed-delete instruction OSL accepted for later visible deletion.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TimedDeleteRecord {
    pub app: String,
    pub conversation: String,
    pub locator: String,
    pub sent_at: i64,
    pub delete_at: i64,
    pub protection: TimedDeleteProtection,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TimedDeleteLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    records: Vec<TimedDeleteRecord>,
}

impl TimedDeleteLedger {
    fn validate(&self) -> Result<(), String> {
        if !matches!(self.version, 0 | 1) || self.records.len() > MAX_TIMED_DELETE_ENTRIES {
            return Err(format!("{TIMED_DELETE_LABEL} is malformed"));
        }
        for record in &self.records {
            validate_timed_delete_record(record)?;
        }
        Ok(())
    }
}

/// Request DTO for the direct timed-delete creation command.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimedDeleteRequest {
    pub app: String,
    pub conversation: String,
    pub locator: String,
    pub sent_at: i64,
    pub delete_at: i64,
    pub protection: TimedDeleteProtection,
}

fn load_timed_delete(path: &Path, key: &[u8; 32]) -> Result<TimedDeleteLedger, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_TIMED_DELETE_BYTES,
        TIMED_DELETE_LABEL,
    )?
    else {
        return Ok(TimedDeleteLedger::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err(format!("{TIMED_DELETE_LABEL} is not encrypted"));
    }
    let plain = Zeroizing::new(
        ipc::main_password::decrypt_at_rest(&bytes, key)
            .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be opened"))?,
    );
    let ledger: TimedDeleteLedger =
        serde_json::from_slice(&plain).map_err(|_| format!("{TIMED_DELETE_LABEL} is malformed"))?;
    ledger.validate()?;
    Ok(ledger)
}

fn store_timed_delete(
    path: &Path,
    ledger: &TimedDeleteLedger,
    key: &[u8; 32],
) -> Result<(), String> {
    let body = Zeroizing::new(
        serde_json::to_vec(ledger)
            .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be encoded"))?,
    );
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be encrypted"))?;
    if sealed.len() as u64 > MAX_TIMED_DELETE_BYTES {
        return Err(format!("{TIMED_DELETE_LABEL} exceeds its storage limit"));
    }
    crate::atomic_file::write_recoverable(path, &sealed, TIMED_DELETE_LABEL)
}

fn validate_timed_delete_record(record: &TimedDeleteRecord) -> Result<(), String> {
    if !is_opaque_id(&record.app) {
        return Err("OSL timed-delete record missing app".to_owned());
    }
    if !is_opaque_id(&record.conversation) {
        return Err("OSL timed-delete record missing conversation".to_owned());
    }
    if !is_opaque_id(&record.locator) {
        return Err("OSL timed-delete record missing message".to_owned());
    }
    if record.sent_at < 0 || record.delete_at <= record.sent_at {
        return Err("OSL timed-delete record has invalid timing".to_owned());
    }
    Ok(())
}

fn record_timed_delete_at_path(
    path: &Path,
    key: &[u8; 32],
    request: TimedDeleteRequest,
) -> Result<TimedDeleteRecord, String> {
    let record = TimedDeleteRecord {
        app: request.app,
        conversation: request.conversation,
        locator: request.locator,
        sent_at: request.sent_at,
        delete_at: request.delete_at,
        protection: request.protection,
    };
    validate_timed_delete_record(&record)?;

    let mut ledger = load_timed_delete(path, key)?;
    if ledger.records.len() >= MAX_TIMED_DELETE_ENTRIES
        && !ledger.records.iter().any(|existing| existing == &record)
    {
        return Err("OSL timed-delete ledger reached its safe limit".to_owned());
    }
    if !ledger.records.iter().any(|existing| existing == &record) {
        ledger.records.push(record.clone());
        ledger.version = 1;
        store_timed_delete(path, &ledger, key)?;
    }
    Ok(record)
}

/// Direct command seam for creating a timed-delete record.
///
/// Creating a timer is a Pro feature. This deliberately gates creation only:
/// view-once receipt/open paths below do not accept `AppState`, so viewing a
/// view-once message remains free.
pub fn cmd_record_timed_delete_at_path(
    state: &ipc::AppState,
    path: &Path,
    key: &[u8; 32],
    request: TimedDeleteRequest,
) -> Result<TimedDeleteRecord, String> {
    if !ipc::tier_gate::is_paid_equivalent(state) {
        return Err(TIMED_DELETE_PRO_REQUIRED.to_owned());
    }
    record_timed_delete_at_path(path, key, request)
}

/// Count timed-delete records from a fresh ledger read. Missing means zero;
/// malformed or unreadable still fails rather than becoming proof of no data.
pub fn timed_delete_count_at_path(path: &Path, key: &[u8; 32]) -> Result<usize, String> {
    Ok(load_timed_delete(path, key)?.records.len())
}

/// Timed-delete records whose deadlines were reached by one pass.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TimedDeleteFireReport {
    /// Records whose already-promised delete deadline is due.
    pub fired: Vec<TimedDeleteRecord>,
    /// Records still waiting for their promised deadline.
    pub retained: usize,
}

/// Remove and return every timed-delete record due at `now`.
///
/// This takes no entitlement state on purpose. Creating a timed delete is gated
/// by Pro; firing a timer OSL already accepted is a promise to the recipient and
/// must survive a later account lapse.
pub fn fire_due_timed_deletes_at_path(
    path: &Path,
    key: &[u8; 32],
    now: i64,
) -> Result<TimedDeleteFireReport, String> {
    let ledger = load_timed_delete(path, key)?;
    if ledger.records.is_empty() {
        return Ok(TimedDeleteFireReport::default());
    }

    let mut report = TimedDeleteFireReport::default();
    let mut retained = Vec::with_capacity(ledger.records.len());
    for record in ledger.records {
        if now >= record.delete_at {
            report.fired.push(record);
        } else {
            retained.push(record);
        }
    }
    report.retained = retained.len();

    if !report.fired.is_empty() {
        store_timed_delete(
            path,
            &TimedDeleteLedger {
                version: 1,
                records: retained,
            },
            key,
        )?;
    }

    Ok(report)
}

/// Direct command seam for firing already-accepted timed deletes.
///
/// The state parameter binds this to the current account context, but this
/// function deliberately does not inspect entitlement. A Pro lapse may stop new
/// promises; it must not break promises already made.
pub fn cmd_fire_due_timed_deletes_at_path(
    _state: &ipc::AppState,
    path: &Path,
    key: &[u8; 32],
    now: i64,
) -> Result<TimedDeleteFireReport, String> {
    fire_due_timed_deletes_at_path(path, key, now)
}

// ---------------------------------------------------------------------------
// Ledger operations
// ---------------------------------------------------------------------------

/// What one prune pass destroyed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PruneReport {
    /// Records moved to a terminal expired state and removed.
    pub expired: usize,
    /// Records still holding a live timer.
    pub retained: usize,
    /// Local cache rows the caller should shred, named by expired records.
    pub shred_cache_ids: Vec<String>,
}

/// Record a delivered logical message and start its clocks.
///
/// `parts` are the sealed parts the receiver authenticated: index, sealed byte
/// length, and a digest of the **sealed** bytes. Never plaintext, and never a
/// digest of plaintext.
///
/// `delivery_digest` is a content-free digest of the durable inbox row that
/// carried the message.
///
/// Re-delivery is idempotent: an identical record is accepted silently, and a
/// *conflicting* one is refused rather than overwriting a running timer, because
/// overwriting is how a peer would reset a clock it does not own.
#[allow(clippy::too_many_arguments)]
pub fn note_delivered_at_path(
    path: &Path,
    key: &[u8; 32],
    scope_key: &str,
    message_id: &str,
    release: TimedRelease,
    parts: Vec<AcceptedPart>,
    delivery_digest: [u8; 32],
    cache_id: Option<String>,
    now: i64,
) -> Result<(), String> {
    if !is_opaque_id(scope_key)
        || !is_opaque_id(message_id)
        || cache_id
            .as_deref()
            .is_some_and(|value| !is_opaque_id(value))
        || parts.is_empty()
        || release.absolute_expires_at <= now
        || release.absolute_expires_at > now.saturating_add(i64::from(MAX_ABSOLUTE_TTL_SECONDS))
    {
        return Err("OSL message expiry record is invalid".to_owned());
    }
    let lifecycle = LogicalMessageLifecycle::receive(
        commitment(MESSAGE_ID_DOMAIN, message_id),
        commitment(SCOPE_KEY_DOMAIN, scope_key),
        u64::try_from(now).map_err(|_| "OSL clock is invalid".to_owned())?,
        u64::try_from(release.absolute_expires_at)
            .map_err(|_| "OSL clock is invalid".to_owned())?,
        release.open_ttl_seconds.unwrap_or(0),
        parts,
        TransitionEvidence {
            digest: delivery_digest,
            observed_at: u64::try_from(now).map_err(|_| "OSL clock is invalid".to_owned())?,
        },
        RECEIVER_LIMITS,
    )
    .map_err(|_| "OSL message expiry record is invalid".to_owned())?;
    let record = OpenClockRecord {
        lifecycle,
        cache_id,
    };

    let mut ledger = load_open_clock(path, key)?;
    prune_in_memory(&mut ledger, now);
    if let Some(existing) = ledger
        .scopes
        .get(scope_key)
        .and_then(|entries| entries.get(message_id))
    {
        // A running timer is never restarted by a re-delivery.
        return if existing.lifecycle == record.lifecycle && existing.cache_id == record.cache_id {
            Ok(())
        } else {
            Err("OSL already holds a different timer for this message".to_owned())
        };
    }
    let scope_is_new = !ledger.scopes.contains_key(scope_key);
    let scope_entries = ledger.scopes.get(scope_key).map_or(0, BTreeMap::len);
    // Bounded and reject-on-full. A live timer is never silently evicted to make
    // room: the caller must refuse to display a message it cannot time.
    if (scope_is_new && ledger.scopes.len() >= MAX_OPEN_CLOCK_SCOPES)
        || scope_entries >= MAX_OPEN_CLOCK_ENTRIES_PER_SCOPE
        || ledger.total_entries() >= MAX_OPEN_CLOCK_ENTRIES_TOTAL
    {
        return Err("OSL message expiry ledger reached its safe limit".to_owned());
    }
    ledger.version = 1;
    ledger
        .scopes
        .entry(scope_key.to_owned())
        .or_default()
        .insert(message_id.to_owned(), record);
    store_open_clock(path, &ledger, key)
}

/// Start the relative clock at the first authenticated local open.
///
/// Idempotent: a second open does not restart the clock, and returns the same
/// deadline the first one set. Returns [`ExpiryVerdict::Expired`] when the
/// message is already past whichever deadline is in force, when the ledger has
/// no record of it, or when the ledger cannot be trusted — so a caller cannot
/// reveal plaintext by ignoring an error.
pub fn record_first_open_at_path(
    path: &Path,
    key: &[u8; 32],
    scope_key: &str,
    message_id: &str,
    open_digest: [u8; 32],
    now: i64,
) -> ExpiryVerdict {
    match record_first_open_inner(path, key, scope_key, message_id, open_digest, now) {
        Ok(verdict) => verdict,
        Err(_) => ExpiryVerdict::Expired,
    }
}

fn record_first_open_inner(
    path: &Path,
    key: &[u8; 32],
    scope_key: &str,
    message_id: &str,
    open_digest: [u8; 32],
    now: i64,
) -> Result<ExpiryVerdict, String> {
    if !is_opaque_id(scope_key) || !is_opaque_id(message_id) || open_digest == [0u8; 32] {
        return Err("OSL message expiry record is invalid".to_owned());
    }
    let now_u64 = u64::try_from(now).map_err(|_| "OSL clock is invalid".to_owned())?;
    let mut ledger = load_open_clock(path, key)?;
    let Some(record) = ledger
        .scopes
        .get_mut(scope_key)
        .and_then(|entries| entries.get_mut(message_id))
    else {
        return Ok(ExpiryVerdict::Expired);
    };
    if now_u64 >= record.lifecycle.effective_expires_at() {
        return Ok(ExpiryVerdict::Expired);
    }
    let mutation = match record.lifecycle.status() {
        // Already open: the clock is running and must not be restarted.
        ReceiptStatus::Opened => Mutation::Unchanged,
        ReceiptStatus::Received => record
            .lifecycle
            .record_opened(
                TransitionEvidence {
                    digest: open_digest,
                    observed_at: now_u64,
                },
                now_u64,
            )
            .map_err(|_| "OSL could not record this open".to_owned())?,
        // Terminal, or a state a receiver's ledger must never be in.
        _ => return Ok(ExpiryVerdict::Expired),
    };
    let effective = record.lifecycle.effective_expires_at();
    if mutation == Mutation::Changed {
        ledger.version = 1;
        store_open_clock(path, &ledger, key)?;
    }
    Ok(ExpiryVerdict::Readable {
        effective_expires_at: i64::try_from(effective).unwrap_or(i64::MAX),
    })
}

/// The deadline in force for one message, without mutating anything.
///
/// Every failure is [`ExpiryVerdict::Expired`]: an unreadable ledger means
/// expired, never fresh.
pub fn verdict_at_path(
    path: &Path,
    key: &[u8; 32],
    scope_key: &str,
    message_id: &str,
    now: i64,
) -> ExpiryVerdict {
    if !is_opaque_id(scope_key) || !is_opaque_id(message_id) {
        return ExpiryVerdict::Expired;
    }
    let Ok(ledger) = load_open_clock(path, key) else {
        return ExpiryVerdict::Expired;
    };
    let Ok(now_u64) = u64::try_from(now) else {
        return ExpiryVerdict::Expired;
    };
    let Some(record) = ledger
        .scopes
        .get(scope_key)
        .and_then(|entries| entries.get(message_id))
    else {
        return ExpiryVerdict::Expired;
    };
    if now_u64 >= record.lifecycle.effective_expires_at() || record.lifecycle.status().is_terminal()
    {
        return ExpiryVerdict::Expired;
    }
    match (
        record.lifecycle.open_ttl_seconds(),
        record.lifecycle.first_authenticated_open_at(),
    ) {
        (Some(_), None) => ExpiryVerdict::AwaitingFirstOpen {
            absolute_expires_at: i64::try_from(record.lifecycle.absolute_expires_at())
                .unwrap_or(i64::MAX),
        },
        _ => ExpiryVerdict::Readable {
            effective_expires_at: i64::try_from(record.lifecycle.effective_expires_at())
                .unwrap_or(i64::MAX),
        },
    }
}

/// Drop every record past the deadline in force, naming the local cache rows the
/// caller must shred.
///
/// Each removal goes through [`LogicalMessageLifecycle::expire`] first, so a
/// record only ever leaves this ledger via the state machine's own terminal
/// transition — never by being quietly dropped.
fn prune_in_memory(ledger: &mut OpenClockLedger, now: i64) -> PruneReport {
    let mut report = PruneReport::default();
    let Ok(now_u64) = u64::try_from(now) else {
        return report;
    };
    ledger.scopes.retain(|_, entries| {
        entries.retain(|_, record| {
            if record.lifecycle.expire(now_u64).is_ok() || record.lifecycle.status().is_terminal() {
                report.expired += 1;
                if let Some(cache_id) = record.cache_id.take() {
                    report.shred_cache_ids.push(cache_id);
                }
                return false;
            }
            report.retained += 1;
            true
        });
        !entries.is_empty()
    });
    report
}

/// Prune the sealed ledger on disk. Writes only when something changed.
pub fn prune_at_path(path: &Path, key: &[u8; 32], now: i64) -> Result<PruneReport, String> {
    let mut ledger = load_open_clock(path, key)?;
    if ledger.scopes.is_empty() {
        // Nothing recorded, so do not create or rewrite the ledger on an idle
        // pass. This is what makes an idle tick cost one failed read.
        return Ok(PruneReport::default());
    }
    let report = prune_in_memory(&mut ledger, now);
    if report.expired > 0 {
        ledger.version = 1;
        store_open_clock(path, &ledger, key)?;
    }
    Ok(report)
}

// ---------------------------------------------------------------------------
// Durable view-once receipt dedup
// ---------------------------------------------------------------------------

const RECEIPT_DEDUP_FILE: &str = "view_once_receipts.json";
const RECEIPT_DEDUP_LABEL: &str = "OSL view-once receipt ledger";
const MAX_RECEIPT_DEDUP_BYTES: u64 = 512 * 1024;
const MAX_RECEIPT_DEDUP_ENTRIES: usize = 4_096;

/// Message ids whose `Received` receipt has already been sent, each retained
/// until the message's own deadline.
///
/// This exists because an in-memory map loses the fact across a restart and the
/// sender then collects a second `Received` receipt for one message — a receipt
/// is a claim about the recipient, so emitting it twice is a claim OSL cannot
/// support.
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiptDedupLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    sent: BTreeMap<String, i64>,
}

fn load_receipt_dedup(path: &Path, key: &[u8; 32]) -> Result<ReceiptDedupLedger, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_RECEIPT_DEDUP_BYTES,
        RECEIPT_DEDUP_LABEL,
    )?
    else {
        return Ok(ReceiptDedupLedger::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err(format!("{RECEIPT_DEDUP_LABEL} is not encrypted"));
    }
    let plain = Zeroizing::new(
        ipc::main_password::decrypt_at_rest(&bytes, key)
            .map_err(|_| format!("{RECEIPT_DEDUP_LABEL} could not be opened"))?,
    );
    let ledger: ReceiptDedupLedger = serde_json::from_slice(&plain)
        .map_err(|_| format!("{RECEIPT_DEDUP_LABEL} is malformed"))?;
    if !matches!(ledger.version, 0 | 1)
        || ledger.sent.len() > MAX_RECEIPT_DEDUP_ENTRIES
        || ledger.sent.keys().any(|id| !is_opaque_id(id))
    {
        return Err(format!("{RECEIPT_DEDUP_LABEL} is malformed"));
    }
    Ok(ledger)
}

fn store_receipt_dedup(
    path: &Path,
    ledger: &ReceiptDedupLedger,
    key: &[u8; 32],
) -> Result<(), String> {
    let body = Zeroizing::new(
        serde_json::to_vec(ledger)
            .map_err(|_| format!("{RECEIPT_DEDUP_LABEL} could not be encoded"))?,
    );
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| format!("{RECEIPT_DEDUP_LABEL} could not be encrypted"))?;
    if sealed.len() as u64 > MAX_RECEIPT_DEDUP_BYTES {
        return Err(format!("{RECEIPT_DEDUP_LABEL} exceeds its storage limit"));
    }
    crate::atomic_file::write_recoverable(path, &sealed, RECEIPT_DEDUP_LABEL)
}

/// Whether a `Received` receipt has already been sent for this message.
///
/// Returns `true` — "already sent, do not send again" — for every failure. A
/// duplicate receipt is a false claim about the recipient, and staying silent is
/// the recoverable direction: the sender simply does not learn something, rather
/// than learning something untrue.
pub fn receipt_already_sent_at_path(
    path: &Path,
    key: &[u8; 32],
    message_id: &str,
    now: i64,
) -> bool {
    if !is_opaque_id(message_id) {
        return true;
    }
    let Ok(mut ledger) = load_receipt_dedup(path, key) else {
        return true;
    };
    ledger.sent.retain(|_, retain_until| *retain_until > now);
    ledger.sent.contains_key(message_id)
}

/// Durably record that a `Received` receipt was sent, retained until the
/// message's own deadline.
///
/// Bounded and reject-on-full: a full ledger refuses rather than evicting a live
/// record, because evicting one is exactly what causes a duplicate receipt.
pub fn record_receipt_sent_at_path(
    path: &Path,
    key: &[u8; 32],
    message_id: &str,
    retain_until: i64,
    now: i64,
) -> Result<(), String> {
    if !is_opaque_id(message_id)
        || retain_until <= now
        || retain_until > now.saturating_add(i64::from(MAX_ABSOLUTE_TTL_SECONDS))
    {
        return Err("OSL view-once receipt record is invalid".to_owned());
    }
    let mut ledger = load_receipt_dedup(path, key)?;
    ledger.sent.retain(|_, until| *until > now);
    if !ledger.sent.contains_key(message_id) && ledger.sent.len() >= MAX_RECEIPT_DEDUP_ENTRIES {
        return Err("OSL view-once receipt ledger reached its safe limit".to_owned());
    }
    ledger.version = 1;
    ledger.sent.insert(message_id.to_owned(), retain_until);
    store_receipt_dedup(path, &ledger, key)
}

/// Drop receipt records whose message is already gone. Writes only on a change.
pub fn prune_receipt_dedup_at_path(path: &Path, key: &[u8; 32], now: i64) -> Result<usize, String> {
    let mut ledger = load_receipt_dedup(path, key)?;
    let before = ledger.sent.len();
    if before == 0 {
        return Ok(0);
    }
    ledger.sent.retain(|_, until| *until > now);
    let dropped = before.saturating_sub(ledger.sent.len());
    if dropped > 0 {
        ledger.version = 1;
        store_receipt_dedup(path, &ledger, key)?;
    }
    Ok(dropped)
}

// ---------------------------------------------------------------------------
// Durable timed-delete records for native service messages
// ---------------------------------------------------------------------------

const TIMED_DELETE_FILE: &str = "timed_delete_records.json";
const TIMED_DELETE_LABEL: &str = "OSL timed-delete record ledger";
const MAX_TIMED_DELETE_BYTES: u64 = 1024 * 1024;
const MAX_TIMED_DELETE_APPS: usize = 32;
const MAX_TIMED_DELETE_CONVERSATIONS_PER_APP: usize = 512;
const MAX_TIMED_DELETE_RECORDS_PER_CONVERSATION: usize = 512;
const MAX_TIMED_DELETE_RECORDS_TOTAL: usize = 4_096;
const MAX_TIMED_DELETE_ID_LEN: usize = 256;

/// Whether the native row scheduled for deletion carried OSL protected content.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimedDeleteProtection {
    Protected,
    Ordinary,
}

impl TimedDeleteProtection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Protected => "protected",
            Self::Ordinary => "ordinary",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimedDeleteStoreStatus {
    Present,
    Missing,
}

impl TimedDeleteStoreStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Missing => "missing",
        }
    }
}

/// Everything OSL must remember to find and delete one native-service message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimedDeleteRecord {
    pub app_id: String,
    pub conversation_id: String,
    pub message_locator: String,
    pub sent_at_unix_seconds: i64,
    pub delete_at_unix_seconds: i64,
    pub protection: TimedDeleteProtection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimedDeleteStoreSnapshot {
    pub status: TimedDeleteStoreStatus,
    pub record_count: usize,
    pub records: Vec<TimedDeleteRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimedDeleteTwoCopyReport {
    pub local_copy_names: [String; 2],
    pub record_count: usize,
    pub message_locator: String,
    pub delete_at_unix_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimedDeleteTwoCopyExpiryReport {
    pub local_copy_names: [String; 2],
    pub expired_records: [usize; 2],
    pub removed_records: [usize; 2],
    pub shredded_cache_rows: [usize; 2],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimedDeleteExpiryReport {
    pub expired_records: usize,
    pub retained_records: usize,
    pub removed_records: usize,
    pub shredded_cache_rows: usize,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TimedDeleteLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<String, BTreeMap<String, BTreeMap<String, TimedDeleteRecord>>>,
}

impl TimedDeleteLedger {
    fn total_conversations(&self) -> usize {
        self.apps.values().map(BTreeMap::len).sum()
    }

    fn total_records(&self) -> usize {
        self.apps
            .values()
            .flat_map(BTreeMap::values)
            .map(BTreeMap::len)
            .sum()
    }
}

fn valid_timed_delete_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TIMED_DELETE_ID_LEN
        && value.bytes().all(|byte| !byte.is_ascii_control())
}

fn validate_timed_delete_record(record: &TimedDeleteRecord) -> Result<(), String> {
    if !valid_timed_delete_component(&record.app_id) {
        return Err("OSL timed-delete record missing app".to_owned());
    }
    if !valid_timed_delete_component(&record.conversation_id) {
        return Err("OSL timed-delete record missing conversation".to_owned());
    }
    if !valid_timed_delete_component(&record.message_locator) {
        return Err("OSL timed-delete record missing message locator".to_owned());
    }
    if record.sent_at_unix_seconds < 0
        || record.delete_at_unix_seconds <= record.sent_at_unix_seconds
        || record.delete_at_unix_seconds
            > record
                .sent_at_unix_seconds
                .saturating_add(i64::from(MAX_ABSOLUTE_TTL_SECONDS))
    {
        return Err("OSL timed-delete record has invalid timing".to_owned());
    }
    Ok(())
}

fn load_timed_delete_ledger(path: &Path, key: &[u8; 32]) -> Result<TimedDeleteLedger, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_TIMED_DELETE_BYTES,
        TIMED_DELETE_LABEL,
    )?
    else {
        return Ok(TimedDeleteLedger::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err(format!("{TIMED_DELETE_LABEL} is not encrypted"));
    }
    let plain = Zeroizing::new(
        ipc::main_password::decrypt_at_rest(&bytes, key)
            .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be opened"))?,
    );
    let ledger: TimedDeleteLedger =
        serde_json::from_slice(&plain).map_err(|_| format!("{TIMED_DELETE_LABEL} is malformed"))?;
    if !matches!(ledger.version, 0 | 1)
        || ledger.apps.len() > MAX_TIMED_DELETE_APPS
        || ledger.total_conversations()
            > MAX_TIMED_DELETE_APPS.saturating_mul(MAX_TIMED_DELETE_CONVERSATIONS_PER_APP)
        || ledger.total_records() > MAX_TIMED_DELETE_RECORDS_TOTAL
    {
        return Err(format!("{TIMED_DELETE_LABEL} is malformed"));
    }
    for (app_id, conversations) in &ledger.apps {
        if !valid_timed_delete_component(app_id)
            || conversations.len() > MAX_TIMED_DELETE_CONVERSATIONS_PER_APP
        {
            return Err(format!("{TIMED_DELETE_LABEL} is malformed"));
        }
        for (conversation_id, messages) in conversations {
            if !valid_timed_delete_component(conversation_id)
                || messages.len() > MAX_TIMED_DELETE_RECORDS_PER_CONVERSATION
            {
                return Err(format!("{TIMED_DELETE_LABEL} is malformed"));
            }
            for (message_locator, record) in messages {
                if !valid_timed_delete_component(message_locator)
                    || record.app_id != *app_id
                    || record.conversation_id != *conversation_id
                    || record.message_locator != *message_locator
                {
                    return Err(format!("{TIMED_DELETE_LABEL} is malformed"));
                }
                validate_timed_delete_record(record)?;
            }
        }
    }
    Ok(ledger)
}

fn store_timed_delete_ledger(
    path: &Path,
    ledger: &TimedDeleteLedger,
    key: &[u8; 32],
) -> Result<(), String> {
    let body = Zeroizing::new(
        serde_json::to_vec(ledger)
            .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be encoded"))?,
    );
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| format!("{TIMED_DELETE_LABEL} could not be encrypted"))?;
    if sealed.len() as u64 > MAX_TIMED_DELETE_BYTES {
        return Err(format!("{TIMED_DELETE_LABEL} exceeds its storage limit"));
    }
    crate::atomic_file::write_recoverable(path, &sealed, TIMED_DELETE_LABEL)
}

/// Direct command boundary for scheduling a native-service message deletion.
pub fn cmd_record_timed_delete_at_path(
    path: &Path,
    key: &[u8; 32],
    record: TimedDeleteRecord,
) -> Result<TimedDeleteRecord, String> {
    validate_timed_delete_record(&record)?;
    let mut ledger = load_timed_delete_ledger(path, key)?;
    let app_is_new = !ledger.apps.contains_key(&record.app_id);
    let conversation_is_new = ledger
        .apps
        .get(&record.app_id)
        .is_none_or(|conversations| !conversations.contains_key(&record.conversation_id));
    let conversation_records = ledger
        .apps
        .get(&record.app_id)
        .and_then(|conversations| conversations.get(&record.conversation_id))
        .map_or(0, BTreeMap::len);
    if (app_is_new && ledger.apps.len() >= MAX_TIMED_DELETE_APPS)
        || (conversation_is_new
            && ledger
                .apps
                .get(&record.app_id)
                .is_some_and(|conversations| {
                    conversations.len() >= MAX_TIMED_DELETE_CONVERSATIONS_PER_APP
                }))
        || conversation_records >= MAX_TIMED_DELETE_RECORDS_PER_CONVERSATION
        || ledger.total_records() >= MAX_TIMED_DELETE_RECORDS_TOTAL
    {
        return Err("OSL timed-delete record ledger reached its safe limit".to_owned());
    }
    if let Some(existing) = ledger
        .apps
        .get(&record.app_id)
        .and_then(|conversations| conversations.get(&record.conversation_id))
        .and_then(|messages| messages.get(&record.message_locator))
    {
        return if existing == &record {
            Ok(record)
        } else {
            Err("OSL already holds a different timed-delete record for this message".to_owned())
        };
    }
    ledger.version = 1;
    ledger
        .apps
        .entry(record.app_id.clone())
        .or_default()
        .entry(record.conversation_id.clone())
        .or_default()
        .insert(record.message_locator.clone(), record.clone());
    store_timed_delete_ledger(path, &ledger, key)?;
    Ok(record)
}

/// Direct command boundary for sending the same expiry record to both local
/// chat-machine copies of a conversation.
pub fn cmd_record_timed_delete_for_two_local_copies_at_paths(
    first_copy_name: &str,
    first_path: &Path,
    second_copy_name: &str,
    second_path: &Path,
    key: &[u8; 32],
    record: TimedDeleteRecord,
) -> Result<TimedDeleteTwoCopyReport, String> {
    if !valid_timed_delete_component(first_copy_name)
        || !valid_timed_delete_component(second_copy_name)
    {
        return Err("OSL timed-delete local copy name is invalid".to_owned());
    }
    validate_timed_delete_record(&record)?;
    let first = cmd_record_timed_delete_at_path(first_path, key, record.clone())?;
    let second = cmd_record_timed_delete_at_path(second_path, key, record.clone())?;
    if first != second {
        return Err("OSL timed-delete fanout wrote different records".to_owned());
    }
    Ok(TimedDeleteTwoCopyReport {
        local_copy_names: [first_copy_name.to_owned(), second_copy_name.to_owned()],
        record_count: 2,
        message_locator: record.message_locator,
        delete_at_unix_seconds: record.delete_at_unix_seconds,
    })
}

/// Expire the scheduled timed-delete rows for both named local chat-machine
/// copies as one command boundary.
#[allow(clippy::too_many_arguments)]
pub fn expire_timed_delete_records_for_two_local_copies_at_paths(
    first_copy_name: &str,
    first_path: &Path,
    first_shred: &store::MessageStore,
    second_copy_name: &str,
    second_path: &Path,
    second_shred: &store::MessageStore,
    key: &[u8; 32],
    now: i64,
) -> Result<TimedDeleteTwoCopyExpiryReport, String> {
    if !valid_timed_delete_component(first_copy_name)
        || !valid_timed_delete_component(second_copy_name)
    {
        return Err("OSL timed-delete local copy name is invalid".to_owned());
    }
    let first = expire_timed_delete_records_at_path(first_path, key, now, first_shred)?;
    let second = expire_timed_delete_records_at_path(second_path, key, now, second_shred)?;
    Ok(TimedDeleteTwoCopyExpiryReport {
        local_copy_names: [first_copy_name.to_owned(), second_copy_name.to_owned()],
        expired_records: [first.expired_records, second.expired_records],
        removed_records: [first.removed_records, second.removed_records],
        shredded_cache_rows: [first.shredded_cache_rows, second.shredded_cache_rows],
    })
}

/// Fresh-read lookup for the exact native-service message scheduled to go.
pub fn find_timed_delete_record_at_path(
    path: &Path,
    key: &[u8; 32],
    app_id: &str,
    conversation_id: &str,
    message_locator: &str,
) -> Result<Option<TimedDeleteRecord>, String> {
    if !valid_timed_delete_component(app_id) {
        return Err("OSL timed-delete record missing app".to_owned());
    }
    if !valid_timed_delete_component(conversation_id) {
        return Err("OSL timed-delete record missing conversation".to_owned());
    }
    if !valid_timed_delete_component(message_locator) {
        return Err("OSL timed-delete record missing message locator".to_owned());
    }
    Ok(load_timed_delete_ledger(path, key)?
        .apps
        .get(app_id)
        .and_then(|conversations| conversations.get(conversation_id))
        .and_then(|messages| messages.get(message_locator))
        .cloned())
}

fn timed_delete_store_file_exists(path: &Path) -> bool {
    path.is_file() || path.with_extension("bak").is_file()
}

/// Fresh-read snapshot of every native-service message scheduled to go.
pub fn read_timed_delete_store_snapshot_at_path(
    path: &Path,
    key: &[u8; 32],
) -> Result<TimedDeleteStoreSnapshot, String> {
    let status = if timed_delete_store_file_exists(path) {
        TimedDeleteStoreStatus::Present
    } else {
        TimedDeleteStoreStatus::Missing
    };
    let ledger = load_timed_delete_ledger(path, key)?;
    let mut records = Vec::with_capacity(ledger.total_records());
    for conversations in ledger.apps.into_values() {
        for messages in conversations.into_values() {
            records.extend(messages.into_values());
        }
    }
    Ok(TimedDeleteStoreSnapshot {
        status,
        record_count: records.len(),
        records,
    })
}

/// Remove timed-delete records whose deadline has elapsed and shred their
/// matching local chat cache rows.
pub fn expire_timed_delete_records_at_path(
    path: &Path,
    key: &[u8; 32],
    now: i64,
    shred: &store::MessageStore,
) -> Result<TimedDeleteExpiryReport, String> {
    let mut ledger = load_timed_delete_ledger(path, key)?;
    let mut report = TimedDeleteExpiryReport::default();
    let mut due_locators = Vec::new();

    for conversations in ledger.apps.values() {
        for messages in conversations.values() {
            for record in messages.values() {
                if record.delete_at_unix_seconds <= now {
                    report.expired_records += 1;
                    due_locators.push(record.message_locator.clone());
                } else {
                    report.retained_records += 1;
                }
            }
        }
    }

    if due_locators.is_empty() {
        return Ok(report);
    }

    let batch: Vec<String> = due_locators
        .into_iter()
        .take(MAX_CACHE_SHREDS_PER_PASS)
        .collect();
    report.shredded_cache_rows = shred
        .shred_expired_messages(&batch)
        .map_err(|_| "OSL timed-delete cache shred failed".to_owned())?;
    let remove_locators: BTreeSet<String> = batch.into_iter().collect();

    ledger.apps.retain(|_, conversations| {
        conversations.retain(|_, messages| {
            messages.retain(|_, record| {
                record.delete_at_unix_seconds > now
                    || !remove_locators.contains(&record.message_locator)
            });
            !messages.is_empty()
        });
        !conversations.is_empty()
    });
    report.removed_records = remove_locators.len();
    ledger.version = 1;
    store_timed_delete_ledger(path, &ledger, key)?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// Abandoned decrypted staging files
// ---------------------------------------------------------------------------

/// Mirrors `peer_attachment_io`'s private staging directory name.
///
/// Duplicated deliberately rather than widening that module's API while another
/// agent holds the attachment transport. It is safe by construction: every
/// removal goes through [`crate::peer_attachment_io::remove_staging_path_in_root`],
/// which independently re-checks the parent directory name, filename prefix and
/// extension, so a wrong constant here makes removal fail closed rather than
/// delete the wrong file.
const STAGING_DIRECTORY: &str = "peer-attachment-staging";

/// How stale a staging file must be before a periodic sweep may remove it.
///
/// The attachment paths that own these files already retry removal for ten
/// minutes. Fifteen minutes is longer than any of those windows, so this sweep
/// can only ever catch a file whose owner has already given up — never one an
/// in-flight open is about to read.
pub const STAGED_PLAINTEXT_MAX_AGE: Duration = Duration::from_secs(900);

/// Most staging files one pass will remove, so a large abandoned directory costs
/// a bounded number of syscalls per tick instead of one long stall.
const MAX_STAGING_REMOVALS_PER_PASS: usize = 64;

/// Remove decrypted and sealed staging files older than
/// [`STAGED_PLAINTEXT_MAX_AGE`].
///
/// Unlike `peer_attachment_io::scavenge_staging_on_startup`, which clears the
/// whole directory and is therefore only safe before anything can be in flight,
/// this is age-bounded and safe to run while OSL is live.
pub fn sweep_abandoned_staging(local_data_dir: &Path, max_age: Duration) -> usize {
    let staging = local_data_dir.join(STAGING_DIRECTORY);
    let Ok(metadata) = std::fs::symlink_metadata(&staging) else {
        return 0;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(&staging) else {
        return 0;
    };
    let mut removed = 0usize;
    for entry in entries.flatten() {
        if removed >= MAX_STAGING_REMOVALS_PER_PASS {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let stale = entry
            .metadata()
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= max_age);
        if stale
            && crate::peer_attachment_io::remove_staging_path_in_root(local_data_dir, &entry.path())
                .is_ok()
        {
            removed += 1;
        }
    }
    removed
}

// ---------------------------------------------------------------------------
// The tick
// ---------------------------------------------------------------------------

/// How often the single process-wide lifecycle tick runs.
///
/// Thirty seconds, chosen so the claim OSL makes is one it keeps:
///
/// * **It bounds the honest promise.** OSL says expired content is purged within
///   half a minute of the next start, not that a continuous timer destroys it the
///   instant it dies. Thirty seconds is small enough that a countdown shown in
///   the UI and the actual purge cannot visibly disagree, and large enough that
///   this is plainly a sweep rather than a per-message timer OSL cannot honestly
///   offer while the app is closed.
/// * **An idle pass is nearly free and makes no network call.** Both ledgers
///   return early on an empty file — [`prune_at_path`] does not create or rewrite
///   anything when there is nothing recorded — and the staging sweep stops at a
///   missing directory. So the tick is not a periodic beacon: work appears only
///   when OSL genuinely owes a destruction.
/// * **Every pass is bounded.** One read per ledger, at most
///   `MAX_STAGING_REMOVALS_PER_PASS` file removals, and one bounded batch of
///   cache shreds. Nothing here loops until done.
///
/// The deletion outbox is *not* retried every pass. Its owner picked a much
/// slower interval on the grounds that a pass with work costs network round
/// trips; the caller subsamples this tick down to
/// `native_attachment_transport::DELETION_DRAIN_INTERVAL` for that leg so
/// absorbing the second thread does not silently make OSL twenty times chattier.
pub const LIFECYCLE_TICK_INTERVAL: Duration = Duration::from_secs(30);

/// Most cache rows one pass hands the store, so a large expiry batch after a long
/// offline period costs a bounded number of statements per tick.
const MAX_CACHE_SHREDS_PER_PASS: usize = 256;

/// What one pass actually destroyed. Counts only — never an id, a scope, a
/// filename or anything about content.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PassReport {
    /// True when the pass ran. False means OSL was locked, so the sealed ledgers
    /// were unreadable and the pass was a no-op by design.
    pub ran: bool,
    pub expired_messages: usize,
    pub fired_timed_deletes: usize,
    pub shredded_cache_rows: usize,
    pub dropped_receipt_records: usize,
    pub removed_staging_files: usize,
    /// A leg that failed. The pass never propagates it: a failed sweep is
    /// retried on the next tick and must not become a refusal to do the rest.
    pub degraded: bool,
}

fn open_clock_path() -> Result<PathBuf, String> {
    Ok(keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?
        .join(OPEN_CLOCK_FILE))
}

fn receipt_dedup_path() -> Result<PathBuf, String> {
    Ok(keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?
        .join(RECEIPT_DEDUP_FILE))
}

fn timed_delete_path() -> Result<PathBuf, String> {
    Ok(keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?
        .join(TIMED_DELETE_FILE))
}

/// The deadline in force for one message, using the active account's ledger.
///
/// [`ExpiryVerdict::Expired`] while OSL is locked: without the file storage key
/// the ledger cannot be read, and an unreadable ledger means expired.
pub fn verdict(scope_key: &str, message_id: &str, now: i64) -> ExpiryVerdict {
    let Some(key) = ipc::main_password::get_file_storage_key() else {
        return ExpiryVerdict::Expired;
    };
    let Ok(path) = open_clock_path() else {
        return ExpiryVerdict::Expired;
    };
    verdict_at_path(&path, &key, scope_key, message_id, now)
}

/// Start the relative clock, using the active account's ledger.
pub fn record_first_open(
    scope_key: &str,
    message_id: &str,
    open_digest: [u8; 32],
    now: i64,
) -> ExpiryVerdict {
    let Some(key) = ipc::main_password::get_file_storage_key() else {
        return ExpiryVerdict::Expired;
    };
    let Ok(path) = open_clock_path() else {
        return ExpiryVerdict::Expired;
    };
    record_first_open_at_path(&path, &key, scope_key, message_id, open_digest, now)
}

/// Whether a `Received` receipt was already sent, using the active account's
/// ledger. `true` (do not send) while locked or on any failure.
pub fn receipt_already_sent(message_id: &str, now: i64) -> bool {
    let Some(key) = ipc::main_password::get_file_storage_key() else {
        return true;
    };
    let Ok(path) = receipt_dedup_path() else {
        return true;
    };
    receipt_already_sent_at_path(&path, &key, message_id, now)
}

/// Durably record a sent `Received` receipt, using the active account's ledger.
pub fn record_receipt_sent(message_id: &str, retain_until: i64, now: i64) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL must be unlocked to record a receipt".to_owned())?;
    record_receipt_sent_at_path(&receipt_dedup_path()?, &key, message_id, retain_until, now)
}

/// Run one bounded lifecycle sweep.
///
/// A no-op while OSL is locked: both ledgers are sealed with the file storage
/// key, so there is nothing readable to prune and nothing is reported. This is
/// what lets the tick start at launch and simply *become* effective when the
/// password gate opens, with no coupling to the gate at all.
///
/// `shred` is the local plaintext cache, when one is open. Expired rows are
/// named by the ledger because the cache has no expiry column and must not gain
/// one — a schema bump would break the owner's database on a rollback.
///
/// Never returns an error. Every leg is independent, and a failing leg sets
/// `degraded` and is retried next tick rather than skipping the others.
pub fn run_pass(
    local_data_dir: &Path,
    shred: Option<&store::MessageStore>,
    now: i64,
) -> PassReport {
    let mut report = PassReport::default();
    // Abandoned decrypted files are removable without the storage key, and are
    // exactly what is left behind by a crash, so sweep them either way.
    report.removed_staging_files =
        sweep_abandoned_staging(local_data_dir, STAGED_PLAINTEXT_MAX_AGE);

    let Some(key) = ipc::main_password::get_file_storage_key() else {
        return report;
    };
    report.ran = true;

    match open_clock_path().and_then(|path| prune_at_path(&path, &key, now)) {
        Ok(prune) => {
            report.expired_messages = prune.expired;
            if let Some(store) = shred {
                let batch: Vec<String> = prune
                    .shred_cache_ids
                    .into_iter()
                    .take(MAX_CACHE_SHREDS_PER_PASS)
                    .collect();
                match store.shred_expired_messages(&batch) {
                    Ok(rows) => report.shredded_cache_rows = rows,
                    Err(_) => report.degraded = true,
                }
            }
        }
        Err(_) => report.degraded = true,
    }

    match receipt_dedup_path().and_then(|path| prune_receipt_dedup_at_path(&path, &key, now)) {
        Ok(dropped) => report.dropped_receipt_records = dropped,
        Err(_) => report.degraded = true,
    }

    match timed_delete_path().and_then(|path| fire_due_timed_deletes_at_path(&path, &key, now)) {
        Ok(fired) => report.fired_timed_deletes = fired.fired.len(),
        Err(_) => report.degraded = true,
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::{LicenseState, LicenseStateDto};
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    const KEY: [u8; 32] = [9u8; 32];
    const SCOPE: &str = "dm:aaaabbbbccccdddd";
    const MESSAGE: &str = "peer-0123456789abcdef0123456789abcdef";
    const MARKED_CACHE_ID: &str = "marked-message-1343";

    fn global_fixture_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct GlobalFixtureReset {
        root: PathBuf,
    }

    impl Drop for GlobalFixtureReset {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(None);
            keystore::set_active_account_dir(None);
            keystore::set_base_dir_override(None);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// Same isolation discipline as the other native tests in this crate: a
    /// per-process, per-nanosecond directory, so concurrent runs never share a
    /// sealed ledger.
    fn root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "osl-message-expiry-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn ledger_path(label: &str) -> PathBuf {
        root(label).join(OPEN_CLOCK_FILE)
    }

    fn timed_delete_path(label: &str) -> PathBuf {
        root(label).join(TIMED_DELETE_FILE)
    }

    fn parts() -> Vec<AcceptedPart> {
        vec![AcceptedPart {
            index: 0,
            // A byte count and a digest of the *sealed* bytes. No plaintext.
            sealed_bytes: 512,
            digest: [4u8; 32],
        }]
    }

    fn note(
        path: &Path,
        release: TimedRelease,
        cache_id: Option<&str>,
        now: i64,
    ) -> Result<(), String> {
        note_delivered_at_path(
            path,
            &KEY,
            SCOPE,
            MESSAGE,
            release,
            parts(),
            [5u8; 32],
            cache_id.map(str::to_owned),
            now,
        )
    }

    fn stored_message(id: &str, channel: &str, body: &str, at: i64) -> store::StoredMessage {
        store::StoredMessage {
            discord_message_id: id.to_owned(),
            channel_id: channel.to_owned(),
            sender_discord_id: "task1342-sender".to_owned(),
            sender_osl_user_id: "task1342-sender".to_owned(),
            plaintext: body.to_owned(),
            decrypted_at: at,
    fn stored_message(id: &str, plaintext: &str, decrypted_at: i64) -> store::StoredMessage {
        store::StoredMessage {
            discord_message_id: id.to_owned(),
            channel_id: "offline-timer-channel".to_owned(),
            sender_discord_id: "offline-timer-sender".to_owned(),
            sender_osl_user_id: "offline-timer-peer".to_owned(),
            plaintext: plaintext.to_owned(),
            decrypted_at,
            burned: false,
        }
    }

    fn marked_history_count(store: &store::MessageStore, channel: &str, mark: &str) -> usize {
        store
            .list_by_channel(channel, 10)
            .unwrap()
            .into_iter()
            .filter(|message| message.plaintext.contains(mark))
            .count()
    }

    fn exact_history_texts(store: &store::MessageStore, channel: &str, text: &str) -> Vec<String> {
        store
            .list_by_channel(channel, 10)
            .unwrap()
            .into_iter()
            .filter_map(|message| (message.plaintext == text).then_some(message.plaintext))
            .collect()
    }

    fn put_in_both_copies(
        first_store: &store::MessageStore,
        second_store: &store::MessageStore,
        message: &store::StoredMessage,
    ) {
        first_store.put(message).unwrap();
        second_store.put(message).unwrap();
    }

    fn readable_copy_counts(
        first_store: &store::MessageStore,
        second_store: &store::MessageStore,
        channel: &str,
        text: &str,
    ) -> [usize; 2] {
        [
            exact_history_texts(first_store, channel, text).len(),
            exact_history_texts(second_store, channel, text).len(),
        ]
    fn state_with_license(state: LicenseState, raw_status: &str) -> ipc::AppState {
        let app = ipc::AppState::new();
        *app.license_state.lock().expect("license state lock") = LicenseStateDto {
            state,
            raw_status: raw_status.to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };
        app
    }

    fn timed_delete_request(locator: &str) -> TimedDeleteRequest {
        TimedDeleteRequest {
            app: "discord".to_owned(),
            conversation: "dm:task-3326".to_owned(),
            locator: locator.to_owned(),
            sent_at: 1_900_000_000,
            delete_at: 1_900_003_600,
            protection: TimedDeleteProtection::Protected,
        }
    }

    // ---- direct timed-delete creation command ----

    #[test]
    fn task_3326_creating_timed_delete_requires_pro_and_free_stays_empty() {
        let pro = state_with_license(LicenseState::Paid, "ACTIVE");
        let free = state_with_license(LicenseState::Free, "Unconfigured");
        let pro_path = root("task-3326-pro").join(TIMED_DELETE_FILE);
        let free_path = root("task-3326-free").join(TIMED_DELETE_FILE);

        let pro_record = cmd_record_timed_delete_at_path(
            &pro,
            &pro_path,
            &KEY,
            timed_delete_request("discord-message-3326-pro"),
        )
        .expect("Pro creates a timed delete");
        let pro_count = timed_delete_count_at_path(&pro_path, &KEY).unwrap();

        let free_error = cmd_record_timed_delete_at_path(
            &free,
            &free_path,
            &KEY,
            timed_delete_request("discord-message-3326-free"),
        )
        .expect_err("Free is refused before creating a timed delete");
        let free_count = timed_delete_count_at_path(&free_path, &KEY).unwrap();

        println!("task_3326_direct_command=cmd_record_timed_delete_at_path");
        println!(
            "task_3326_pro_account raw_status=ACTIVE result=created app={} conversation={} locator={} sent_at={} delete_at={} protection={}",
            pro_record.app,
            pro_record.conversation,
            pro_record.locator,
            pro_record.sent_at,
            pro_record.delete_at,
            pro_record.protection.as_str()
        );
        println!("task_3326_pro_timed_delete_count={pro_count}");
        println!("task_3326_free_refused_by_name={free_error}");
        println!("task_3326_free_timed_delete_count={free_count}");

        assert_eq!(pro_count, 1);
        assert_eq!(free_error, TIMED_DELETE_PRO_REQUIRED);
        assert_eq!(free_count, 0);
        assert_eq!(pro_record.locator, "discord-message-3326-pro");
    }

    #[test]
    fn task_3327_existing_timed_delete_fires_after_pro_switches_to_free() {
        let account = state_with_license(LicenseState::Paid, "ACTIVE");
        let path = root("task-3327-lapse").join(TIMED_DELETE_FILE);

        let existing = cmd_record_timed_delete_at_path(
            &account,
            &path,
            &KEY,
            timed_delete_request("discord-message-3327-existing"),
        )
        .expect("Pro creates the already-promised timed delete");
        let before_lapse_count = timed_delete_count_at_path(&path, &KEY).unwrap();

        *account.license_state.lock().expect("license state lock") = LicenseStateDto {
            state: LicenseState::Free,
            raw_status: "Unconfigured".to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };

        let fire_report =
            cmd_fire_due_timed_deletes_at_path(&account, &path, &KEY, existing.delete_at).unwrap();
        let fired_count_after_lapse = fire_report.fired.len();
        let after_fire_count = timed_delete_count_at_path(&path, &KEY).unwrap();

        let new_timer_error = cmd_record_timed_delete_at_path(
            &account,
            &path,
            &KEY,
            timed_delete_request("discord-message-3327-new"),
        )
        .expect_err("Free account cannot create a new timed delete");
        let fired_count_after_refusal = fired_count_after_lapse;
        let after_refusal_count = timed_delete_count_at_path(&path, &KEY).unwrap();

        println!("task_3327_create_command=cmd_record_timed_delete_at_path");
        println!("task_3327_fire_command=cmd_fire_due_timed_deletes_at_path");
        println!("task_3327_initial_account raw_status=ACTIVE access=pro");
        println!("task_3327_switched_account raw_status=Unconfigured access=free");
        println!("task_3327_existing_timer_locator={}", existing.locator);
        println!("task_3327_before_lapse_timed_delete_count={before_lapse_count}");
        println!(
            "task_3327_existing_timer_deleted_on_time_at={} fired_locator={}",
            existing.delete_at, fire_report.fired[0].locator
        );
        println!("task_3327_fired_count_after_lapse={fired_count_after_lapse}");
        println!("task_3327_after_fire_timed_delete_count={after_fire_count}");
        println!("task_3327_new_timer_refused_by_name={new_timer_error}");
        println!("task_3327_fired_count_after_refusal={fired_count_after_refusal}");
        println!("task_3327_after_refusal_timed_delete_count={after_refusal_count}");

        assert_eq!(before_lapse_count, 1);
        assert_eq!(fired_count_after_lapse, 1);
        assert_eq!(
            fire_report.fired[0].locator,
            "discord-message-3327-existing"
        );
        assert_eq!(fire_report.retained, 0);
        assert_eq!(after_fire_count, 0);
        assert_eq!(new_timer_error, TIMED_DELETE_PRO_REQUIRED);
        assert_eq!(fired_count_after_refusal, fired_count_after_lapse);
        assert_eq!(after_refusal_count, 0);
    }

    // ---- two clocks ----

    #[test]
    fn the_default_clock_is_relative_and_the_advanced_one_is_absolute() {
        let now = 1_000_000i64;
        let relative = relative_release(now, ipc::cipher_store_client::TTL_1H).unwrap();
        assert_eq!(relative.open_ttl_seconds, Some(3_600));
        // The relay still gets a hard deadline, because it cannot see an open.
        assert_eq!(
            relative.absolute_expires_at,
            now + i64::from(DEFAULT_DELIVERY_WINDOW_SECONDS)
        );
        assert!(matches!(
            relative.mode(),
            TimedMessageMode::FirstAuthenticatedOpen {
                lifetime_ms: 3_600_000
            }
        ));

        let absolute = absolute_release(now, ipc::cipher_store_client::TTL_1H).unwrap();
        assert_eq!(absolute.open_ttl_seconds, None);
        assert_eq!(absolute.absolute_expires_at, now + 3_600);
        assert!(matches!(
            absolute.mode(),
            TimedMessageMode::FixedAbsolute { .. }
        ));
    }

    #[test]
    fn only_relay_offered_lifetimes_are_accepted() {
        for offered in TTL_ALLOWLIST {
            assert!(relative_release(1_000, offered).is_ok());
            assert!(absolute_release(1_000, offered).is_ok());
        }
        for refused in [0u32, 1, 60, 7_200, 604_801, 2_592_001, u32::MAX] {
            assert!(relative_release(1_000, refused).is_err(), "{refused}");
            assert!(absolute_release(1_000, refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_delivery_window_shorter_than_the_open_clock_is_refused() {
        // Otherwise the window would destroy an unread message before the timer
        // it is supposed to contain ever started.
        assert!(relative_release_within(
            1_000,
            ipc::cipher_store_client::TTL_7D,
            ipc::cipher_store_client::TTL_1H
        )
        .is_err());
        assert!(relative_release_within(
            1_000,
            ipc::cipher_store_client::TTL_1H,
            ipc::cipher_store_client::TTL_7D
        )
        .is_ok());
    }

    #[test]
    fn the_relay_ttl_comes_back_out_of_the_absolute_clock() {
        let now = 1_000_000i64;
        let release = absolute_release(now, ipc::cipher_store_client::TTL_24H).unwrap();
        assert_eq!(release.relay_ttl_seconds(now), Ok(86_400));
        // A stale release no longer maps to an offered lifetime, so the upload
        // is refused rather than silently rounded.
        assert!(release.relay_ttl_seconds(now + 10).is_err());
    }

    // ---- the clock starts at first open, not at send ----

    #[test]
    fn the_clock_starts_at_the_first_open_not_at_send() {
        let path = ledger_path("first-open");
        let sent_at = 1_000_000i64;
        let release = relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap();
        note(&path, release, None, sent_at).unwrap();

        // Two hours after send, with a one-hour lifetime: still readable,
        // because nothing has opened it. This is the bug being fixed — the old
        // behaviour baked `now + ttl` in at send and this would be gone.
        let much_later = sent_at + 7_200;
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, much_later),
            ExpiryVerdict::AwaitingFirstOpen {
                absolute_expires_at: sent_at + i64::from(DEFAULT_DELIVERY_WINDOW_SECONDS)
            }
        );

        let opened_at = much_later;
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], opened_at),
            ExpiryVerdict::Readable {
                effective_expires_at: opened_at + 3_600
            }
        );
        assert!(verdict_at_path(&path, &KEY, SCOPE, MESSAGE, opened_at + 3_599).is_readable());
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, opened_at + 3_600),
            ExpiryVerdict::Expired
        );
    }

    #[test]
    fn a_second_open_does_not_restart_the_clock() {
        let path = ledger_path("no-restart");
        let sent_at = 1_000_000i64;
        note(
            &path,
            relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            None,
            sent_at,
        )
        .unwrap();
        let first = record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], sent_at + 10);
        let second =
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [7u8; 32], sent_at + 2_000);
        assert_eq!(first, second);
        assert_eq!(
            first,
            ExpiryVerdict::Readable {
                effective_expires_at: sent_at + 10 + 3_600
            }
        );
    }

    #[test]
    fn the_absolute_clock_still_kills_a_message_nobody_opened() {
        let path = ledger_path("absolute-cap");
        let sent_at = 1_000_000i64;
        note(
            &path,
            relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            None,
            sent_at,
        )
        .unwrap();
        let past_window = sent_at + i64::from(DEFAULT_DELIVERY_WINDOW_SECONDS);
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, past_window),
            ExpiryVerdict::Expired
        );
        // And the open path cannot resurrect it either.
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], past_window),
            ExpiryVerdict::Expired
        );
    }

    #[test]
    fn the_advanced_absolute_option_ignores_opens() {
        let path = ledger_path("advanced-absolute");
        let sent_at = 1_000_000i64;
        note(
            &path,
            absolute_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            None,
            sent_at,
        )
        .unwrap();
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 10),
            ExpiryVerdict::Readable {
                effective_expires_at: sent_at + 3_600
            }
        );
        // Opening it does not shorten it; the fixed deadline is the whole promise.
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], sent_at + 10),
            ExpiryVerdict::Readable {
                effective_expires_at: sent_at + 3_600
            }
        );
    }

    // ---- fail closed ----

    #[test]
    fn an_unreadable_ledger_reads_as_expired_never_as_fresh() {
        let path = ledger_path("unreadable");
        let sent_at = 1_000_000i64;
        note(
            &path,
            relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            None,
            sent_at,
        )
        .unwrap();
        assert!(verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 1).is_readable());

        // Wrong key, corrupt body, and a plaintext body all mean expired.
        assert_eq!(
            verdict_at_path(&path, &[1u8; 32], SCOPE, MESSAGE, sent_at + 1),
            ExpiryVerdict::Expired
        );
        std::fs::write(&path, b"OSL-ENC1not a real sealed blob at all").unwrap();
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 1),
            ExpiryVerdict::Expired
        );
        std::fs::write(&path, br#"{"version":1,"scopes":{}}"#).unwrap();
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 1),
            ExpiryVerdict::Expired
        );
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], sent_at + 1),
            ExpiryVerdict::Expired
        );
    }

    #[test]
    fn an_unrecorded_message_is_expired_not_fresh() {
        let path = ledger_path("unrecorded");
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, 1_000_000),
            ExpiryVerdict::Expired
        );
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], 1_000_000),
            ExpiryVerdict::Expired
        );
    }

    #[test]
    fn a_conflicting_redelivery_never_resets_a_running_timer() {
        let path = ledger_path("conflict");
        let sent_at = 1_000_000i64;
        let release = relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap();
        note(&path, release, None, sent_at).unwrap();
        // Identical re-delivery is idempotent.
        note(&path, release, None, sent_at).unwrap();
        // A different clock for the same message is refused.
        let longer = relative_release(sent_at, ipc::cipher_store_client::TTL_7D).unwrap();
        assert!(note(&path, longer, None, sent_at).is_err());
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 1),
            ExpiryVerdict::AwaitingFirstOpen {
                absolute_expires_at: sent_at + i64::from(DEFAULT_DELIVERY_WINDOW_SECONDS)
            }
        );
    }

    #[test]
    fn a_release_beyond_the_absolute_ceiling_is_refused() {
        let path = ledger_path("ceiling");
        let now = 1_000_000i64;
        for bad in [
            TimedRelease {
                absolute_expires_at: now,
                open_ttl_seconds: Some(3_600),
            },
            TimedRelease {
                absolute_expires_at: now - 1,
                open_ttl_seconds: None,
            },
            TimedRelease {
                absolute_expires_at: now + i64::from(MAX_ABSOLUTE_TTL_SECONDS) + 1,
                open_ttl_seconds: None,
            },
        ] {
            assert!(note(&path, bad, None, now).is_err());
        }
    }

    // ---- pruning ----

    #[test]
    fn pruning_removes_expired_records_and_names_their_cache_rows() {
        let path = ledger_path("prune");
        let sent_at = 1_000_000i64;
        note(
            &path,
            absolute_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            Some("1502771310428819569"),
            sent_at,
        )
        .unwrap();

        // Nothing is due yet, so the pass reports the live record and no shreds.
        let early = prune_at_path(&path, &KEY, sent_at + 1).unwrap();
        assert_eq!(early.expired, 0);
        assert_eq!(early.retained, 1);
        assert!(early.shred_cache_ids.is_empty());

        let due = prune_at_path(&path, &KEY, sent_at + 3_600).unwrap();
        assert_eq!(due.expired, 1);
        assert_eq!(due.retained, 0);
        assert_eq!(due.shred_cache_ids, vec!["1502771310428819569".to_owned()]);

        // The record is gone, and a second pass has nothing left to do.
        assert_eq!(
            prune_at_path(&path, &KEY, sent_at + 3_600).unwrap(),
            PruneReport::default()
        );
        assert_eq!(
            verdict_at_path(&path, &KEY, SCOPE, MESSAGE, sent_at + 3_600),
            ExpiryVerdict::Expired
        );
    }

    #[test]
    fn an_idle_prune_pass_does_not_create_a_ledger() {
        let path = ledger_path("idle");
        assert_eq!(
            prune_at_path(&path, &KEY, 1_000_000).unwrap(),
            PruneReport::default()
        );
        assert!(
            !path.exists(),
            "an idle tick must not write a file it had no reason to create"
        );
    }

    #[test]
    fn pruning_removes_a_message_whose_open_clock_ran_out() {
        let path = ledger_path("prune-open-clock");
        let sent_at = 1_000_000i64;
        note(
            &path,
            relative_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            None,
            sent_at,
        )
        .unwrap();
        let opened_at = sent_at + 100;
        assert!(
            record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], opened_at)
                .is_readable()
        );
        // Long before the thirty-day absolute deadline, the open clock is what
        // destroys it.
        assert_eq!(
            prune_at_path(&path, &KEY, opened_at + 3_599)
                .unwrap()
                .expired,
            0
        );
        assert_eq!(
            prune_at_path(&path, &KEY, opened_at + 3_600)
                .unwrap()
                .expired,
            1
        );
    }

    #[test]
    fn task_3782_full_thirty_day_timer_two_copies() {
        const THIRTY_DAYS: i64 = 30 * 24 * 60 * 60;
        const COPY_A: &str = "peer-3782-copy-a";
        const COPY_B: &str = "peer-3782-copy-b";

        fn readable_count(path: &Path, message_id: &str, now: i64) -> usize {
            usize::from(verdict_at_path(path, &KEY, SCOPE, message_id, now).is_readable())
        }

        fn note_copy(
            path: &Path,
            message_id: &str,
            release: TimedRelease,
            now: i64,
        ) -> Result<(), String> {
            note_delivered_at_path(
                path,
                &KEY,
                SCOPE,
                message_id,
                release,
                parts(),
                [5u8; 32],
                None,
                now,
            )
        }

        let path = ledger_path("task-3782-thirty-day");
        let sent_at = 1_000_000i64;
        let deadline = sent_at + THIRTY_DAYS;
        let one_second_before = deadline - 1;
        let release = relative_release(sent_at, ipc::cipher_store_client::TTL_30D)
            .expect("the exact 30-day marked timer must be sendable");

        let before_a = readable_count(&path, COPY_A, sent_at);
        let before_b = readable_count(&path, COPY_B, sent_at);
        println!("task-3782 before-send copy-a-count={before_a} copy-b-count={before_b}");
        assert_eq!((before_a, before_b), (0, 0));

        note_copy(&path, COPY_A, release, sent_at).unwrap();
        note_copy(&path, COPY_B, release, sent_at).unwrap();
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, COPY_A, [6u8; 32], sent_at),
            ExpiryVerdict::Readable {
                effective_expires_at: deadline
            }
        );
        assert_eq!(
            record_first_open_at_path(&path, &KEY, SCOPE, COPY_B, [7u8; 32], sent_at),
            ExpiryVerdict::Readable {
                effective_expires_at: deadline
            }
        );

        let after_a = readable_count(&path, COPY_A, sent_at);
        let after_b = readable_count(&path, COPY_B, sent_at);
        println!("task-3782 after-send copy-a-count={after_a} copy-b-count={after_b}");
        assert_eq!((after_a, after_b), (1, 1));

        let mut expiry_run_count = 0usize;
        let early_prune = prune_at_path(&path, &KEY, one_second_before).unwrap();
        let early_a = readable_count(&path, COPY_A, one_second_before);
        let early_b = readable_count(&path, COPY_B, one_second_before);
        println!(
            "task-3782 day-29-23:59:59 copy-a-count={early_a} copy-b-count={early_b} expired-in-run={}",
            early_prune.expired
        );
        assert_eq!(early_prune.expired, 0);
        assert_eq!((early_a, early_b), (1, 1));

        let due_prune = prune_at_path(&path, &KEY, deadline).unwrap();
        if due_prune.expired > 0 {
            expiry_run_count += 1;
        }
        let due_a = readable_count(&path, COPY_A, deadline);
        let due_b = readable_count(&path, COPY_B, deadline);
        println!(
            "task-3782 day-30-00:00:00 copy-a-count={due_a} copy-b-count={due_b} expired-in-run={}",
            due_prune.expired
        );
        assert_eq!(due_prune.expired, 2);
        assert_eq!((due_a, due_b), (0, 0));

        let later = deadline + 1;
        let later_a = record_first_open_at_path(&path, &KEY, SCOPE, COPY_A, [8u8; 32], later);
        let later_b = record_first_open_at_path(&path, &KEY, SCOPE, COPY_B, [9u8; 32], later);
        let later_prune = prune_at_path(&path, &KEY, later).unwrap();
        if later_prune.expired > 0 {
            expiry_run_count += 1;
        }
        println!(
            "task-3782 later-reads copy-a={later_a:?} copy-b={later_b:?} expiry-run-count={expiry_run_count}"
        );
        assert_eq!(later_a, ExpiryVerdict::Expired);
        assert_eq!(later_b, ExpiryVerdict::Expired);
        assert_eq!(later_prune.expired, 0);
        assert_eq!(expiry_run_count, 1);
    }

    // ---- receipt dedup ----

    #[test]
    fn a_receipt_is_remembered_across_a_restart() {
        let path = root("receipts").join(RECEIPT_DEDUP_FILE);
        let now = 1_000_000i64;
        assert!(!receipt_already_sent_at_path(&path, &KEY, MESSAGE, now));
        record_receipt_sent_at_path(&path, &KEY, MESSAGE, now + 3_600, now).unwrap();
        // A fresh read of the sealed file is exactly what a restart does.
        assert!(receipt_already_sent_at_path(&path, &KEY, MESSAGE, now));
        // And it stops mattering once the message itself is gone.
        assert!(!receipt_already_sent_at_path(
            &path,
            &KEY,
            MESSAGE,
            now + 3_600
        ));
    }

    #[test]
    fn an_unreadable_receipt_ledger_suppresses_the_receipt() {
        let path = root("receipts-unreadable").join(RECEIPT_DEDUP_FILE);
        std::fs::write(&path, b"not sealed at all").unwrap();
        // Fail closed towards silence: a duplicate receipt is a false claim
        // about the recipient, and not sending one is the recoverable direction.
        assert!(receipt_already_sent_at_path(
            &path, &KEY, MESSAGE, 1_000_000
        ));
        assert!(receipt_already_sent_at_path(
            &path,
            &KEY,
            "not a valid id!",
            1_000_000
        ));
    }

    #[test]
    fn pruning_drops_receipt_records_whose_message_is_gone() {
        let path = root("receipts-prune").join(RECEIPT_DEDUP_FILE);
        let now = 1_000_000i64;
        record_receipt_sent_at_path(&path, &KEY, MESSAGE, now + 3_600, now).unwrap();
        assert_eq!(prune_receipt_dedup_at_path(&path, &KEY, now).unwrap(), 0);
        assert_eq!(
            prune_receipt_dedup_at_path(&path, &KEY, now + 3_600).unwrap(),
            1
        );
        assert!(!receipt_already_sent_at_path(
            &path,
            &KEY,
            MESSAGE,
            now + 3_600
        ));
    }

    // ---- timed-delete native message records ----

    #[test]
    fn task_3306_direct_command_writes_protected_and_ordinary_timed_delete_records() {
        let path = timed_delete_path("task-3306");
        let protected = TimedDeleteRecord {
            app_id: "discord".to_owned(),
            conversation_id: "dm:task-3306".to_owned(),
            message_locator: "discord-message-3306-protected".to_owned(),
            sent_at_unix_seconds: 1_900_000_000,
            delete_at_unix_seconds: 1_900_003_600,
            protection: TimedDeleteProtection::Protected,
        };
        let ordinary = TimedDeleteRecord {
            app_id: "whatsapp".to_owned(),
            conversation_id: "chat:task-3306".to_owned(),
            message_locator: "whatsapp-message-3306-ordinary".to_owned(),
            sent_at_unix_seconds: 1_900_010_000,
            delete_at_unix_seconds: 1_900_096_400,
            protection: TimedDeleteProtection::Ordinary,
        };

        let protected_written =
            cmd_record_timed_delete_at_path(&path, &KEY, protected.clone()).unwrap();
        let ordinary_written =
            cmd_record_timed_delete_at_path(&path, &KEY, ordinary.clone()).unwrap();
        println!("task_3306_direct_command=cmd_record_timed_delete_at_path");
        println!(
            "task_3306_written_protected app={} conversation={} locator={} sent_at={} delete_at={} protection={}",
            protected_written.app_id,
            protected_written.conversation_id,
            protected_written.message_locator,
            protected_written.sent_at_unix_seconds,
            protected_written.delete_at_unix_seconds,
            protected_written.protection.as_str()
        );
        println!(
            "task_3306_written_ordinary app={} conversation={} locator={} sent_at={} delete_at={} protection={}",
            ordinary_written.app_id,
            ordinary_written.conversation_id,
            ordinary_written.message_locator,
            ordinary_written.sent_at_unix_seconds,
            ordinary_written.delete_at_unix_seconds,
            ordinary_written.protection.as_str()
        );

        let found_protected = find_timed_delete_record_at_path(
            &path,
            &KEY,
            "discord",
            "dm:task-3306",
            "discord-message-3306-protected",
        )
        .unwrap();
        let found_ordinary = find_timed_delete_record_at_path(
            &path,
            &KEY,
            "whatsapp",
            "chat:task-3306",
            "whatsapp-message-3306-ordinary",
        )
        .unwrap();
        assert_eq!(found_protected, Some(protected.clone()));
        assert_eq!(found_ordinary, Some(ordinary.clone()));
        let fresh_read_found_count =
            usize::from(found_protected.is_some()) + usize::from(found_ordinary.is_some());
        println!("task_3306_fresh_read_found_count={fresh_read_found_count}");
        println!(
            "task_3306_fresh_read_protected_locator={}",
            found_protected.unwrap().message_locator
        );
        println!(
            "task_3306_fresh_read_ordinary_locator={}",
            found_ordinary.unwrap().message_locator
        );

        let mut missing_conversation = protected;
        missing_conversation.message_locator = "discord-message-3306-missing-conversation".into();
        missing_conversation.conversation_id.clear();
        let refused =
            cmd_record_timed_delete_at_path(&path, &KEY, missing_conversation).unwrap_err();
        assert_eq!(refused, "OSL timed-delete record missing conversation");
        println!("task_3306_missing_conversation_refused={refused}");
    }

    #[test]
    fn task_3307_keeps_timed_delete_records_across_restart_and_reports_wiped_store() {
        let path = timed_delete_path("task-3307");
        let records = vec![
            TimedDeleteRecord {
                app_id: "discord".to_owned(),
                conversation_id: "dm:task-3307-a".to_owned(),
                message_locator: "discord-message-3307-a".to_owned(),
                sent_at_unix_seconds: 1_910_000_000,
                delete_at_unix_seconds: 1_910_003_600,
                protection: TimedDeleteProtection::Protected,
            },
            TimedDeleteRecord {
                app_id: "signal".to_owned(),
                conversation_id: "chat:task-3307-b".to_owned(),
                message_locator: "signal-message-3307-b".to_owned(),
                sent_at_unix_seconds: 1_910_010_000,
                delete_at_unix_seconds: 1_910_096_400,
                protection: TimedDeleteProtection::Ordinary,
            },
            TimedDeleteRecord {
                app_id: "whatsapp".to_owned(),
                conversation_id: "chat:task-3307-c".to_owned(),
                message_locator: "whatsapp-message-3307-c".to_owned(),
                sent_at_unix_seconds: 1_910_020_000,
                delete_at_unix_seconds: 1_910_023_600,
                protection: TimedDeleteProtection::Protected,
            },
        ];

        for record in &records {
            cmd_record_timed_delete_at_path(&path, &KEY, record.clone()).unwrap();
        }
        let stored = read_timed_delete_store_snapshot_at_path(&path, &KEY).unwrap();
        assert_eq!(stored.status, TimedDeleteStoreStatus::Present);
        assert_eq!(stored.record_count, 3);
        println!("task_3307_store_command=cmd_record_timed_delete_at_path");
        println!("task_3307_initial_store_count={}", stored.record_count);

        // A fresh read from the sealed file is the backend equivalent of OSL
        // closing for an update or restart, then reopening with the same account.
        let reopened = read_timed_delete_store_snapshot_at_path(&path, &KEY).unwrap();
        assert_eq!(reopened.status, TimedDeleteStoreStatus::Present);
        assert_eq!(reopened.record_count, 3);
        assert_eq!(reopened.records, records);
        println!("task_3307_reopen_simulation=fresh_read_from_sealed_file");
        println!(
            "task_3307_reopened_store_status={}",
            reopened.status.as_str()
        );
        println!("task_3307_reopened_record_count={}", reopened.record_count);
        for (index, record) in reopened.records.iter().enumerate() {
            println!(
                "task_3307_reopened_record_{} app={} conversation={} locator={} delete_at={}",
                index + 1,
                record.app_id,
                record.conversation_id,
                record.message_locator,
                record.delete_at_unix_seconds
            );
            assert_eq!(
                record.delete_at_unix_seconds,
                records[index].delete_at_unix_seconds
            );
        }

        std::fs::remove_file(&path).unwrap();
        let _ = std::fs::remove_file(path.with_extension("bak"));
        let wiped_reopen = read_timed_delete_store_snapshot_at_path(&path, &KEY).unwrap();
        assert_eq!(wiped_reopen.status, TimedDeleteStoreStatus::Missing);
        assert_eq!(wiped_reopen.record_count, 0);
        assert!(wiped_reopen.records.is_empty());
        println!(
            "task_3307_wiped_reopen_store_status={}",
            wiped_reopen.status.as_str()
        );
        println!(
            "task_3307_wiped_reopen_record_count={}",
            wiped_reopen.record_count
        );
    }

    #[test]
    fn task_1342_connects_timer_expiry_to_both_local_chat_copies() {
        let root = root("task-1342");
        let first_name = "task1342-alice-local-copy";
        let second_name = "task1342-bob-local-copy";
        let first_ledger = root.join(first_name).join(TIMED_DELETE_FILE);
        let second_ledger = root.join(second_name).join(TIMED_DELETE_FILE);
        let first_store_dir = root.join(first_name).join("message-store");
        let second_store_dir = root.join(second_name).join("message-store");
        std::fs::create_dir_all(first_ledger.parent().unwrap()).unwrap();
        std::fs::create_dir_all(second_ledger.parent().unwrap()).unwrap();

        let first_store = store::MessageStore::open(&first_store_dir, &KEY).unwrap();
        let second_store = store::MessageStore::open(&second_store_dir, &KEY).unwrap();
        let channel = "task1342-chat";
        let message_id = "task1342-one-minute-message";
        let mark = "TASK1342-MARKED-ONE-MINUTE-MESSAGE";
        let sent_at = 1_960_000_000i64;
        let delete_at = sent_at + 60;
        let body = format!("{mark} body");
        let message = stored_message(message_id, channel, &body, sent_at);
        first_store.put(&message).unwrap();
        second_store.put(&message).unwrap();

        let record = TimedDeleteRecord {
            app_id: "osl-chat".to_owned(),
            conversation_id: channel.to_owned(),
            message_locator: message_id.to_owned(),
            sent_at_unix_seconds: sent_at,
            delete_at_unix_seconds: delete_at,
            protection: TimedDeleteProtection::Protected,
        };
        let fanout = cmd_record_timed_delete_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            second_name,
            &second_ledger,
            &KEY,
            record.clone(),
        )
        .unwrap();
        assert_eq!(fanout.record_count, 2);
        assert_eq!(fanout.message_locator, message_id);
        assert_eq!(fanout.delete_at_unix_seconds - sent_at, 60);
        println!(
            "TASK1342_EXPIRY_RECORD_SENT_TO_BOTH local_copies={},{} record_count={} locator={} lifetime_seconds={}",
            fanout.local_copy_names[0],
            fanout.local_copy_names[1],
            fanout.record_count,
            fanout.message_locator,
            fanout.delete_at_unix_seconds - sent_at
        );

        let first_before = marked_history_count(&first_store, channel, mark);
        let second_before = marked_history_count(&second_store, channel, mark);
        assert_eq!(first_before, 1);
        assert_eq!(second_before, 1);
        assert_eq!(
            find_timed_delete_record_at_path(&first_ledger, &KEY, "osl-chat", channel, message_id)
                .unwrap(),
            Some(record.clone())
        );
        assert_eq!(
            find_timed_delete_record_at_path(&second_ledger, &KEY, "osl-chat", channel, message_id)
                .unwrap(),
            Some(record)
        );
        println!(
            "TASK1342_BEFORE local_copy={} same_mark={} count={}",
            first_name, mark, first_before
        );
        println!(
            "TASK1342_BEFORE local_copy={} same_mark={} count={}",
            second_name, mark, second_before
        );

        let first_expired =
            expire_timed_delete_records_at_path(&first_ledger, &KEY, delete_at, &first_store)
                .unwrap();
        let second_expired =
            expire_timed_delete_records_at_path(&second_ledger, &KEY, delete_at, &second_store)
                .unwrap();
        assert_eq!(first_expired.expired_records, 1);
        assert_eq!(second_expired.expired_records, 1);
        assert_eq!(first_expired.removed_records, 1);
        assert_eq!(second_expired.removed_records, 1);
        assert_eq!(first_expired.shredded_cache_rows, 1);
        assert_eq!(second_expired.shredded_cache_rows, 1);

        let first_after = marked_history_count(&first_store, channel, mark);
        let second_after = marked_history_count(&second_store, channel, mark);
        let first_mark_absent = first_after == 0;
        let second_mark_absent = second_after == 0;
        assert_eq!(first_after, 0);
        assert_eq!(second_after, 0);
        assert_eq!(
            find_timed_delete_record_at_path(&first_ledger, &KEY, "osl-chat", channel, message_id)
                .unwrap(),
            None
        );
        assert_eq!(
            find_timed_delete_record_at_path(&second_ledger, &KEY, "osl-chat", channel, message_id)
                .unwrap(),
            None
        );
        println!(
            "TASK1342_AFTER local_copy={} count={} mark_absent={} expired_records={} removed_records={} shredded_cache_rows={}",
            first_name,
            first_after,
            first_mark_absent,
            first_expired.expired_records,
            first_expired.removed_records,
            first_expired.shredded_cache_rows
        );
        println!(
            "TASK1342_AFTER local_copy={} count={} mark_absent={} expired_records={} removed_records={} shredded_cache_rows={}",
            second_name,
            second_after,
            second_mark_absent,
            second_expired.expired_records,
            second_expired.removed_records,
            second_expired.shredded_cache_rows
        );
    }

    #[test]
    fn task_0547_server_timer_expiry_once_removes_both_named_copies() {
        let root = root("task-0547");
        let first_name = "task0547-alice-named-copy";
        let second_name = "task0547-bob-named-copy";
        let first_ledger = root.join(first_name).join(TIMED_DELETE_FILE);
        let second_ledger = root.join(second_name).join(TIMED_DELETE_FILE);
        let first_store_dir = root.join(first_name).join("message-store");
        let second_store_dir = root.join(second_name).join("message-store");
        std::fs::create_dir_all(first_ledger.parent().unwrap()).unwrap();
        std::fs::create_dir_all(second_ledger.parent().unwrap()).unwrap();

        let first_store = store::MessageStore::open(&first_store_dir, &KEY).unwrap();
        let second_store = store::MessageStore::open(&second_store_dir, &KEY).unwrap();
        let channel = "task0547-chat";
        let message_id = format!("task0547-{}", uuid::Uuid::new_v4().simple());
        let exact_text = format!("TASK0547-MARK-{}", uuid::Uuid::new_v4().simple());
        let sent_at = 1_970_000_000i64;
        let delete_at = sent_at + 60;
        let message = stored_message(&message_id, channel, &exact_text, sent_at);
        first_store.put(&message).unwrap();
        second_store.put(&message).unwrap();

        let record = TimedDeleteRecord {
            app_id: "osl-chat".to_owned(),
            conversation_id: channel.to_owned(),
            message_locator: message_id.clone(),
            sent_at_unix_seconds: sent_at,
            delete_at_unix_seconds: delete_at,
            protection: TimedDeleteProtection::Protected,
        };
        let fanout = cmd_record_timed_delete_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            second_name,
            &second_ledger,
            &KEY,
            record,
        )
        .unwrap();
        assert_eq!(fanout.record_count, 2);

        let first_before_texts = exact_history_texts(&first_store, channel, &exact_text);
        let second_before_texts = exact_history_texts(&second_store, channel, &exact_text);
        assert_eq!(first_before_texts, vec![exact_text.clone()]);
        assert_eq!(second_before_texts, vec![exact_text.clone()]);
        println!(
            "TASK0547_BEFORE local_copy={} exact_text={} count={}",
            first_name,
            first_before_texts[0],
            first_before_texts.len()
        );
        println!(
            "TASK0547_BEFORE local_copy={} exact_text={} count={}",
            second_name,
            second_before_texts[0],
            second_before_texts.len()
        );

        let expiry = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            delete_at,
        )
        .unwrap();
        assert_eq!(expiry.expired_records, [1, 1]);
        assert_eq!(expiry.removed_records, [1, 1]);
        assert_eq!(expiry.shredded_cache_rows, [1, 1]);
        println!(
            "TASK0547_EXPIRY_ONCE local_copies={},{} expired_records={:?} removed_records={:?} shredded_cache_rows={:?}",
            expiry.local_copy_names[0],
            expiry.local_copy_names[1],
            expiry.expired_records,
            expiry.removed_records,
            expiry.shredded_cache_rows
        );

        let first_after_texts = exact_history_texts(&first_store, channel, &exact_text);
        let second_after_texts = exact_history_texts(&second_store, channel, &exact_text);
        if !first_after_texts.is_empty() {
            println!(
                "TASK0547B_STILL_PRESENT local_copy={} exact_text={} count={}",
                first_name,
                first_after_texts[0],
                first_after_texts.len()
            );
        }
        if !second_after_texts.is_empty() {
            println!(
                "TASK0547B_STILL_PRESENT local_copy={} exact_text={} count={}",
                second_name,
                second_after_texts[0],
                second_after_texts.len()
            );
        }
        assert!(first_after_texts.is_empty());
        assert!(second_after_texts.is_empty());
        println!(
            "TASK0547_AFTER_REFRESH local_copy={} exact_text={} count={} marked_absent={}",
            first_name,
            exact_text,
            first_after_texts.len(),
            first_after_texts.is_empty()
        );
        println!(
            "TASK0547_AFTER_REFRESH local_copy={} exact_text={} count={} marked_absent={}",
            second_name,
            exact_text,
            second_after_texts.len(),
            second_after_texts.is_empty()
        );
    }

    #[test]
    fn task_3772_runs_thirty_day_timer_and_real_clock_last_hour() {
        use crate::expiry_clock::{RenderLifetimeVerdict, ViewLifetime};

        const DAY: i64 = 24 * 60 * 60;
        let root = root("task-3772");
        let first_name = "task3772-first-copy";
        let second_name = "task3772-second-copy";
        let first_ledger = root.join(first_name).join(TIMED_DELETE_FILE);
        let second_ledger = root.join(second_name).join(TIMED_DELETE_FILE);
        let first_store_dir = root.join(first_name).join("message-store");
        let second_store_dir = root.join(second_name).join("message-store");
        std::fs::create_dir_all(first_ledger.parent().unwrap()).unwrap();
        std::fs::create_dir_all(second_ledger.parent().unwrap()).unwrap();

        let first_store = store::MessageStore::open(&first_store_dir, &KEY).unwrap();
        let second_store = store::MessageStore::open(&second_store_dir, &KEY).unwrap();
        let channel = "task3772-chat";
        let thirty_day_message_id = "task3772-thirty-day-message";
        let thirty_day_text = "TASK3772 THIRTY DAY TIMER MESSAGE";
        let sent_at = 2_000_000_000i64;
        let thirty_day_ttl = i64::from(ipc::cipher_store_client::TTL_30D);
        let delete_at = sent_at + thirty_day_ttl;
        let message = stored_message(thirty_day_message_id, channel, thirty_day_text, sent_at);
        put_in_both_copies(&first_store, &second_store, &message);

        let record = TimedDeleteRecord {
            app_id: "osl-chat".to_owned(),
            conversation_id: channel.to_owned(),
            message_locator: thirty_day_message_id.to_owned(),
            sent_at_unix_seconds: sent_at,
            delete_at_unix_seconds: delete_at,
            protection: TimedDeleteProtection::Protected,
        };
        let fanout = cmd_record_timed_delete_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            second_name,
            &second_ledger,
            &KEY,
            record,
        )
        .unwrap();
        assert_eq!(fanout.record_count, 2);
        assert_eq!(fanout.delete_at_unix_seconds - sent_at, thirty_day_ttl);
        println!(
            "TASK3772_THIRTY_DAY_TIMER_SENT local_copies={},{} lifetime_seconds={}",
            fanout.local_copy_names[0],
            fanout.local_copy_names[1],
            fanout.delete_at_unix_seconds - sent_at
        );

        let no_advance_report = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            sent_at,
        )
        .unwrap();
        let no_advance_counts =
            readable_copy_counts(&first_store, &second_store, channel, thirty_day_text);
        assert_eq!(no_advance_report.expired_records, [0, 0]);
        assert_eq!(no_advance_counts, [1, 1]);
        println!(
            "TASK3772_NO_ADVANCE_READABLE local_copy={} count={}",
            first_name, no_advance_counts[0]
        );
        println!(
            "TASK3772_NO_ADVANCE_READABLE local_copy={} count={}",
            second_name, no_advance_counts[1]
        );

        let day_29_report = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            sent_at + 29 * DAY,
        )
        .unwrap();
        let day_29_counts =
            readable_copy_counts(&first_store, &second_store, channel, thirty_day_text);
        assert_eq!(day_29_report.expired_records, [0, 0]);
        assert_eq!(day_29_counts, [1, 1]);
        println!(
            "TASK3772_DAY29_READABLE local_copy={} count={}",
            first_name, day_29_counts[0]
        );
        println!(
            "TASK3772_DAY29_READABLE local_copy={} count={}",
            second_name, day_29_counts[1]
        );

        let day_31_report = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            sent_at + 31 * DAY,
        )
        .unwrap();
        let day_31_counts =
            readable_copy_counts(&first_store, &second_store, channel, thirty_day_text);
        assert_eq!(day_31_report.expired_records, [1, 1]);
        assert_eq!(day_31_report.removed_records, [1, 1]);
        assert_eq!(day_31_report.shredded_cache_rows, [1, 1]);
        assert_eq!(day_31_counts, [0, 0]);
        println!(
            "TASK3772_DAY31_READABLE local_copy={} count={}",
            first_name, day_31_counts[0]
        );
        println!(
            "TASK3772_DAY31_READABLE local_copy={} count={}",
            second_name, day_31_counts[1]
        );

        let hour_message_id = "task3772-one-hour-message";
        let hour_text = "TASK3772 ONE HOUR REAL CLOCK MESSAGE";
        let hour_sent_at = 2_010_000_000i64;
        let hour_delete_at = hour_sent_at + i64::from(ipc::cipher_store_client::TTL_1H);
        let hour_message = stored_message(hour_message_id, channel, hour_text, hour_sent_at);
        put_in_both_copies(&first_store, &second_store, &hour_message);
        let hour_record = TimedDeleteRecord {
            app_id: "osl-chat".to_owned(),
            conversation_id: channel.to_owned(),
            message_locator: hour_message_id.to_owned(),
            sent_at_unix_seconds: hour_sent_at,
            delete_at_unix_seconds: hour_delete_at,
            protection: TimedDeleteProtection::Protected,
        };
        cmd_record_timed_delete_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            second_name,
            &second_ledger,
            &KEY,
            hour_record,
        )
        .unwrap();

        let mut real_clock = ViewLifetime::new(Duration::from_secs(60 * 60));
        assert_eq!(
            real_clock.on_render(Duration::from_secs(0)),
            RenderLifetimeVerdict::Started
        );
        assert_eq!(
            real_clock.verdict_at(Duration::from_secs(60 * 60 - 1)),
            RenderLifetimeVerdict::Active
        );
        let hour_before_report = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            hour_delete_at - 1,
        )
        .unwrap();
        let hour_before_counts =
            readable_copy_counts(&first_store, &second_store, channel, hour_text);
        assert_eq!(hour_before_report.expired_records, [0, 0]);
        assert_eq!(hour_before_counts, [1, 1]);
        println!(
            "TASK3772_REAL_CLOCK_1H_BEFORE local_copy={} count={}",
            first_name, hour_before_counts[0]
        );
        println!(
            "TASK3772_REAL_CLOCK_1H_BEFORE local_copy={} count={}",
            second_name, hour_before_counts[1]
        );

        assert_eq!(
            real_clock.verdict_at(Duration::from_secs(60 * 60)),
            RenderLifetimeVerdict::Expired
        );
        let hour_after_report = expire_timed_delete_records_for_two_local_copies_at_paths(
            first_name,
            &first_ledger,
            &first_store,
            second_name,
            &second_ledger,
            &second_store,
            &KEY,
            hour_delete_at,
        )
        .unwrap();
        let hour_after_counts =
            readable_copy_counts(&first_store, &second_store, channel, hour_text);
        assert_eq!(hour_after_report.expired_records, [1, 1]);
        assert_eq!(hour_after_report.removed_records, [1, 1]);
        assert_eq!(hour_after_report.shredded_cache_rows, [1, 1]);
        assert_eq!(hour_after_counts, [0, 0]);
        println!(
            "TASK3772_REAL_CLOCK_1H_AFTER local_copy={} count={}",
            first_name, hour_after_counts[0]
        );
        println!(
            "TASK3772_REAL_CLOCK_1H_AFTER local_copy={} count={}",
            second_name, hour_after_counts[1]
        );
    }

    // ---- staging sweep ----

    #[test]
    fn the_staging_sweep_is_age_bounded_and_leaves_live_files_alone() {
        let local_data = root("staging");
        let staging = local_data.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&staging).unwrap();
        let fresh = staging.join("opened-00112233445566778899aabbccddeeff.oslatt");
        std::fs::write(&fresh, b"sealed bytes").unwrap();

        // Zero max-age is the "everything is stale" case, so the sweep removes
        // it; the default window would not.
        assert_eq!(
            sweep_abandoned_staging(&local_data, STAGED_PLAINTEXT_MAX_AGE),
            0
        );
        assert!(fresh.exists(), "a live staging file must survive a tick");
        assert_eq!(sweep_abandoned_staging(&local_data, Duration::ZERO), 1);
        assert!(!fresh.exists());

        // A file the staging contract does not recognise is never removed.
        let foreign = staging.join("not-ours.txt");
        std::fs::write(&foreign, b"x").unwrap();
        assert_eq!(sweep_abandoned_staging(&local_data, Duration::ZERO), 0);
        assert!(foreign.exists());
    }

    #[test]
    fn the_staging_sweep_tolerates_a_missing_directory() {
        let local_data = root("staging-missing");
        assert_eq!(sweep_abandoned_staging(&local_data, Duration::ZERO), 0);
    }

    // ---- the pass ----

    #[test]
    fn a_pass_reports_whether_it_could_read_the_sealed_ledgers() {
        // The process-global unlocked key is shared by the whole test binary, so
        // this asserts the invariant rather than one lock state: `ran` is true
        // exactly when a storage key exists. That is the property the tick
        // depends on — a locked pass is a silent no-op, so the tick can start at
        // launch and simply become effective when the gate opens.
        let local_data = root("pass-lock-state");
        let report = run_pass(&local_data, None, 1_000_000);
        assert_eq!(
            report.ran,
            ipc::main_password::get_file_storage_key().is_some()
        );
        if !report.ran {
            assert!(!report.degraded, "being locked is not a failure");
            assert_eq!(report.expired_messages, 0);
            assert_eq!(report.shredded_cache_rows, 0);
        }
    }

    #[test]
    fn a_pass_clears_abandoned_decrypted_files_whether_or_not_it_is_locked() {
        let local_data = root("pass-staging");
        let staging = local_data.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("sealed-00112233445566778899aabbccddeeff.oslatt"),
            b"sealed",
        )
        .unwrap();
        // Abandoned files do not need the storage key, and are exactly what a
        // crash leaves behind, so this leg runs before the lock check.
        let fresh = run_pass(&local_data, None, 1_000_000);
        assert_eq!(
            fresh.removed_staging_files, 0,
            "a file inside the age window must survive a tick"
        );
        assert_eq!(sweep_abandoned_staging(&local_data, Duration::ZERO), 1);
    }

    #[test]
    fn offline_timer_pass_shreds_expired_reopened_copy() {
        let _guard = global_fixture_lock().lock().expect("global fixture lock");
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);

        let account_dir = root("offline-copy-expiry");
        let local_data = account_dir.join("local-data");
        let store_dir = account_dir.join("message-store");
        let _reset = GlobalFixtureReset {
            root: account_dir.clone(),
        };
        std::fs::create_dir_all(&local_data).unwrap();
        std::fs::create_dir_all(&store_dir).unwrap();
        keystore::set_base_dir_override(Some(account_dir.clone()));
        keystore::set_active_account_dir(Some(account_dir.clone()));
        ipc::main_password::set_file_storage_key(Some(KEY));

        let sent_at = 1_000_000i64;
        let marked = stored_message(MARKED_CACHE_ID, "marked offline timer copy", sent_at);
        {
            let store = store::MessageStore::open(&store_dir, &KEY).expect("open message store");
            store.put(&marked).expect("seed marked message");
        }
        let reopened =
            store::MessageStore::open(&store_dir, &KEY).expect("reopen marked message store");
        assert!(
            reopened
                .get(MARKED_CACHE_ID)
                .expect("read marked message before expiry")
                .is_some(),
            "test fixture did not seed the marked offline copy"
        );

        note(
            &open_clock_path().expect("open-clock path"),
            absolute_release(sent_at, ipc::cipher_store_client::TTL_1H).unwrap(),
            Some(MARKED_CACHE_ID),
            sent_at,
        )
        .expect("record marked message timer");

        let report = run_pass(&local_data, Some(&reopened), sent_at + 3_600);
        assert!(report.ran, "offline timer pass did not read sealed ledgers");
        assert_eq!(report.expired_messages, 1);
        assert!(
            reopened
                .get(MARKED_CACHE_ID)
                .expect("read marked message after expiry")
                .is_none(),
            "reopened copy still holding {MARKED_CACHE_ID}"
        );
        assert_eq!(report.shredded_cache_rows, 1);

        drop(reopened);
    }

    #[test]
    fn the_tick_interval_subsamples_cleanly_into_the_deletion_drain() {
        // The tick absorbs the attachment deletion drain rather than running a
        // second thread, so its interval has to divide that one exactly.
        let drain = Duration::from_secs(900);
        assert_eq!(drain.as_secs() % LIFECYCLE_TICK_INTERVAL.as_secs(), 0);
        assert_eq!(drain.as_secs() / LIFECYCLE_TICK_INTERVAL.as_secs(), 30);
    }
}
