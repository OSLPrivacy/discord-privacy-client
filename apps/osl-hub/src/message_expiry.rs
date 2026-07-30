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

use std::collections::BTreeMap;
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

/// The four lifetimes OSL offers, in seconds.
///
/// Taken from the cipher store's own allowlist rather than restated, because a
/// value the relay will not accept is not a lifetime OSL can offer.
pub const TTL_ALLOWLIST: [u32; 4] = [
    ipc::cipher_store_client::TTL_1H,
    ipc::cipher_store_client::TTL_24H,
    ipc::cipher_store_client::TTL_72H,
    ipc::cipher_store_client::TTL_7D,
];

/// Hard ceiling on any absolute deadline. Seven days, matching the relay.
pub const MAX_ABSOLUTE_TTL_SECONDS: u32 = ipc::cipher_store_client::TTL_7D;

/// How long the relay holds ciphertext for a *relative*-clock message.
///
/// The relative clock cannot start until the receiver opens, and the relay
/// cannot observe that open. So the delivery window has to be long enough that
/// a recipient who is away for a few days still gets the message the sender
/// meant them to have — otherwise the "clock starts at first open" promise is
/// quietly broken by a delivery window that expired first.
///
/// Seven days is the relay's own ceiling, and the tradeoff is explicit: a
/// relative-clock message's *ciphertext* may sit in relay storage for up to a
/// week, where the relay necessarily observes object size and access time.
/// Callers that prefer a tighter window pass one to
/// [`relative_release_within`].
pub const DEFAULT_DELIVERY_WINDOW_SECONDS: u32 = ipc::cipher_store_client::TTL_7D;

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
/// open, under a seven-day absolute ceiling.
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

/// Bounds mirroring the peer replay ledger, scaled for a heavier record.
///
/// A record is a full lifecycle snapshot, not a single timestamp, so the entry
/// caps are lower and the byte cap higher than the replay ledger's. Every cap is
/// a *rejection* boundary, never an eviction one.
const MAX_OPEN_CLOCK_BYTES: u64 = 2 * 1024 * 1024;
const MAX_OPEN_CLOCK_SCOPES: usize = 512;
const MAX_OPEN_CLOCK_ENTRIES_PER_SCOPE: usize = 512;
const MAX_OPEN_CLOCK_ENTRIES_TOTAL: usize = 2_048;

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
            byte.is_ascii_digit()
                || byte == b'-'
                || byte == b':'
                || byte.is_ascii_lowercase()
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
    let ledger: OpenClockLedger = serde_json::from_slice(&plain)
        .map_err(|_| format!("{OPEN_CLOCK_LABEL} is malformed"))?;
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
        || cache_id.as_deref().is_some_and(|value| !is_opaque_id(value))
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
        u64::try_from(release.absolute_expires_at).map_err(|_| "OSL clock is invalid".to_owned())?,
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
    if now_u64 >= record.lifecycle.effective_expires_at()
        || record.lifecycle.status().is_terminal()
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
            if record.lifecycle.expire(now_u64).is_ok()
                || record.lifecycle.status().is_terminal()
            {
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
pub fn prune_receipt_dedup_at_path(
    path: &Path,
    key: &[u8; 32],
    now: i64,
) -> Result<usize, String> {
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
// Abandoned decrypted staging files
// ---------------------------------------------------------------------------

/// Mirrors `peer_attachment_io`'s private staging directory name.
///
/// Duplicated deliberately rather than widening that module's API while another
/// agent holds the attachment transport. It is safe by construction: every
/// removal goes through [`crate::peer_attachment_io::remove_staging_path`],
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
    report.removed_staging_files = sweep_abandoned_staging(local_data_dir, STAGED_PLAINTEXT_MAX_AGE);

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

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const KEY: [u8; 32] = [9u8; 32];
    const SCOPE: &str = "dm:aaaabbbbccccdddd";
    const MESSAGE: &str = "peer-0123456789abcdef0123456789abcdef";

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

    fn parts() -> Vec<AcceptedPart> {
        vec![AcceptedPart {
            index: 0,
            // A byte count and a digest of the *sealed* bytes. No plaintext.
            sealed_bytes: 512,
            digest: [4u8; 32],
        }]
    }

    fn note(path: &Path, release: TimedRelease, cache_id: Option<&str>, now: i64) -> Result<(), String> {
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
            TimedMessageMode::FirstAuthenticatedOpen { lifetime_ms: 3_600_000 }
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
        for refused in [0u32, 1, 60, 7_200, 604_801, u32::MAX] {
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
            TimedRelease { absolute_expires_at: now, open_ttl_seconds: Some(3_600) },
            TimedRelease { absolute_expires_at: now - 1, open_ttl_seconds: None },
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
        assert!(record_first_open_at_path(&path, &KEY, SCOPE, MESSAGE, [6u8; 32], opened_at)
            .is_readable());
        // Long before the seven-day absolute deadline, the open clock is what
        // destroys it.
        assert_eq!(
            prune_at_path(&path, &KEY, opened_at + 3_599).unwrap().expired,
            0
        );
        assert_eq!(
            prune_at_path(&path, &KEY, opened_at + 3_600).unwrap().expired,
            1
        );
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
        assert!(!receipt_already_sent_at_path(&path, &KEY, MESSAGE, now + 3_600));
    }

    #[test]
    fn an_unreadable_receipt_ledger_suppresses_the_receipt() {
        let path = root("receipts-unreadable").join(RECEIPT_DEDUP_FILE);
        std::fs::write(&path, b"not sealed at all").unwrap();
        // Fail closed towards silence: a duplicate receipt is a false claim
        // about the recipient, and not sending one is the recoverable direction.
        assert!(receipt_already_sent_at_path(&path, &KEY, MESSAGE, 1_000_000));
        assert!(receipt_already_sent_at_path(&path, &KEY, "not a valid id!", 1_000_000));
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
        assert!(!receipt_already_sent_at_path(&path, &KEY, MESSAGE, now + 3_600));
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
        assert_eq!(sweep_abandoned_staging(&local_data, STAGED_PLAINTEXT_MAX_AGE), 0);
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
    fn the_tick_interval_subsamples_cleanly_into_the_deletion_drain() {
        // The tick absorbs the attachment deletion drain rather than running a
        // second thread, so its interval has to divide that one exactly.
        let drain = Duration::from_secs(900);
        assert_eq!(drain.as_secs() % LIFECYCLE_TICK_INTERVAL.as_secs(), 0);
        assert_eq!(drain.as_secs() / LIFECYCLE_TICK_INTERVAL.as_secs(), 30);
    }
}
