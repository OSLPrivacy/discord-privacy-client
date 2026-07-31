//! Bilateral burn: sender sequencing, opaque commitments, the receiver replay
//! ledger, and the durable sender-side outbox.
//!
//! Wire types live in [`crate::wire_v2`] (`0x0A` notice, `0x0B` ack); the
//! bodies live in [`crate::control_messages`]. This module is the state
//! machine, and it is deliberately pure: no I/O, no clock, no globals. The Hub
//! (`apps/osl-hub/src/security.rs`) owns encrypted persistence and the network;
//! the broker owns transport. That split is what makes every adversarial case
//! below testable in the `ipc` suite.
//!
//! # The requirement
//!
//! "if u burn the other person needs to burn ur stuff too" — the owner. Read
//! literally, and that literal reading is also the entire authorisation model:
//!
//! > **Burn authority is authorship.** A revocation destroys the content its
//! > sender authored, in one conversation, and nothing else. It needs no
//! > consent grant, quorum or scope signature, because you are only ever
//! > destroying your own content. `docs/design/burn-contract.md` asks for
//! > time-bounded consent grants for *remote friend burn*; that machinery
//! > exists for the case where one identity asks another to destroy content it
//! > did **not** author. This path never does that, so the grant is not the
//! > gate — authorship is.
//!
//! # Threat notes, stated rather than implied
//!
//! - **No forward secrecy.** Notices ride `encrypt_v3` (PQ-hybrid,
//!   ratchet-independent) because a burn has to work when the Double Ratchet
//!   is desynced, which is precisely when it is needed — the same reason
//!   `SKDM_REQUEST` and `SESSION_RESET` do. `encrypt_v3` is keyed by long-term
//!   identity keys, so an adversary who compromises an identity secret can
//!   retroactively forge revocations from that identity for as long as the key
//!   is valid, exactly as they can retroactively read that identity's content.
//!   Burn inherits the identity-key compromise story; it does not improve on it.
//! - **The commitment key is not a secret from the peers.** It is derived from
//!   the two identity X25519 public keys, so either peer — and anyone who knows
//!   both public keys — can compute a commitment. That is intentional and
//!   sufficient: commitments exist to keep plaintext scope identifiers out of
//!   the envelope and to give both sides a stable, unlinkable-across-pairs
//!   handle. They are **not** an authentication mechanism; authentication is
//!   the envelope.
//! - **A peer can over-burn their own content.** `burn_upto_seq` is chosen by
//!   the author, so an author can retire content of theirs we have not seen
//!   yet ([`MAX_BURN_LOOKAHEAD`] bounds how far). They are entitled to; it is
//!   their content. It cannot touch ours or a third member's.
//!
//! # Why a replay can never destroy later content
//!
//! Three independent facts, any one of which is sufficient:
//!
//! 1. `burn_epoch` is monotonic per (sender, scope) and the ledger keeps
//!    `last_burn_epoch`. A notice with `epoch <= last_burn_epoch` is dropped
//!    before any state changes.
//! 2. Destruction is bounded by `burn_upto_seq`, which was fixed when the
//!    notice was authored. Content sent afterwards has a strictly greater
//!    `send_seq`, so it is out of range even if the epoch check were removed.
//! 3. `burn_id = HMAC(key, "burn" ‖ scope_commitment ‖ epoch ‖ upto_seq)` binds
//!    the parameters to the identifier. An attacker who edits `burn_upto_seq`
//!    upward cannot produce the matching id, and the journal refuses a known id
//!    presented with different parameters.
//!
//! The legacy client's model — a permanent `peer_map.burned_scopes` flag — has
//! none of these properties: one stale or replayed `0x01` killed that
//! conversation forever, including content sent after it. That is a permanent
//! denial of service and this module does not reproduce it. Even an inbound
//! legacy `0x01` is converted to a bounded revocation; see
//! [`legacy_burn_notice`].

use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::control_messages::{RevocationAck, RevocationNotice};

// ---- Domain separation ----

/// HKDF info for the per-pair commitment key.
const HKDF_INFO_SCOPE_COMMIT: &[u8] = b"OSL/burn/scope-commit/v1";
/// HMAC label for a scope commitment.
const LABEL_SCOPE: &[u8] = b"scope";
/// HMAC label for a burn identifier.
const LABEL_BURN: &[u8] = b"burn";
/// HMAC label for an explicit per-message commitment.
const LABEL_MESSAGE: &[u8] = b"msg";
/// HMAC label for the keyserver revocation-lane collapse key.
const LABEL_LANE: &[u8] = b"lane";

// ---- Bounds. Every one of these rejects rather than forgets. ----

/// Maximum number of (peer, scope) pairs the receiver ledger tracks.
pub const MAX_LEDGER_SCOPES: usize = 512;

/// Maximum journalled burns per (peer, scope).
pub const MAX_JOURNAL_ENTRIES_PER_SCOPE: usize = 64;

/// Maximum journalled burns across the whole ledger. `docs/design/burn-contract.md`:
/// "Replay journals are hard bounded and fail closed instead of silently
/// forgetting live replay state." So the bound is enforced by **refusing** a
/// new burn, never by evicting an old journal entry — evicting one would make
/// the burn it recorded replayable again.
pub const MAX_JOURNAL_ENTRIES_TOTAL: usize = 8_192;

/// How far above the highest `send_seq` we have actually seen from a sender a
/// `burn_upto_seq` may reach. Burn-before-message is legitimate (their message
/// and their burn can cross in flight, and we may simply be behind), so the
/// window has to be generous; an unbounded one would let a peer pin their own
/// side of the conversation shut at `u64::MAX`.
pub const MAX_BURN_LOOKAHEAD: u64 = 1_048_576;

/// Maximum tracked scopes in the sender-side sequence/epoch counters.
pub const MAX_COUNTER_SCOPES: usize = 4_096;

/// Maximum entries in the durable revocation outbox. Unacknowledged entries are
/// **never** evicted to make room — the POST is refused instead, so the caller
/// finds out. Compare `keyserver-cf` `evictOldestPending`, which silently
/// deletes the oldest undelivered rows and is why a queued burn could be
/// destroyed by the sender's own next 32 messages.
pub const MAX_OUTBOX_ENTRIES: usize = 512;

/// Retry backoff (seconds) for an unacknowledged outbox entry, indexed by
/// attempt count and clamped at the last element. Retried on every drain and on
/// a tick; there is no attempt limit, because giving up would silently convert
/// "not yet delivered" into "never delivered".
pub const OUTBOX_BACKOFF_SECS: [i64; 7] = [0, 15, 60, 300, 900, 3_600, 21_600];

// ---- User-facing claims. Three separate statements, per
// `docs/design/osl-gui-final-plan.md:496-500`. ----

/// Claim 1 — the strong one, and the one to lead with. Unilateral: it does not
/// depend on any peer doing anything.
pub const CLAIM_CONTENT_EXPIRY: &str =
    "OSL content expiry: the keys for your messages in this conversation are destroyed, \
     so that content can no longer be decrypted by anyone — including you.";

/// Claim 2 — verifiable on this device.
pub const CLAIM_LOCAL_REMOVAL: &str =
    "Local removal: OSL deleted its local copies and key mappings for this conversation \
     from this device.";

/// Claim 3 — the honest bound. Never phrased as deletion.
pub const CLAIM_RECIPIENT_COPIES: &str =
    "Recipient copies: anyone who already opened a message keeps whatever they kept. \
     OSL sent them a burn request and their app honours it the next time it connects — \
     there is no bound on when that happens, and no guarantee that it ever does.";

/// Status strings for the delivery of one revocation. `docs/design/osl-gui-final-plan.md:494`
/// forbids ever rendering `Sent request` as `Deleted`, so "Deleted" appears
/// nowhere in this module.
pub const STATUS_SENT_REQUEST: &str = "Sent request";
pub const STATUS_ACKNOWLEDGED: &str = "Acknowledged by peer";
pub const STATUS_NOT_ACKNOWLEDGED: &str = "Not acknowledged";

// ---- Errors ----

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RevocationError {
    /// Commitment-key derivation failed.
    #[error("revocation commitment key could not be derived")]
    KeyDerivation,

    /// The receiver ledger already tracks [`MAX_LEDGER_SCOPES`] pairs and this
    /// notice is for a new one. Fail closed: refuse, do not evict.
    #[error("revocation ledger is full")]
    LedgerFull,

    /// A journal bound was reached. Refuse the burn so the sender retries and
    /// the operator can be told, rather than forgetting live replay state.
    #[error("revocation replay journal is full")]
    JournalFull,

    /// Sender-side counters are full.
    #[error("revocation counters are full")]
    CountersFull,

    /// The outbox is full of unacknowledged revocations.
    #[error("revocation outbox is full")]
    OutboxFull,

    /// Sequence or epoch arithmetic would wrap.
    #[error("revocation counter exhausted")]
    CounterExhausted,
}

// ---- Commitments ----

type HmacSha256 = Hmac<Sha256>;

fn mac(key: &[u8; 32], parts: &[&[u8]]) -> [u8; 32] {
    let mut m =
        <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC-SHA256 accepts a 32-byte key");
    for part in parts {
        // Length-prefix every component so no two distinct tuples can produce
        // the same MAC input.
        m.update(&(part.len() as u64).to_be_bytes());
        m.update(part);
    }
    let out = m.finalize().into_bytes();
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&out);
    buf
}

/// Constant-time equality for a commitment or identifier. Used everywhere a
/// 32-byte MAC-derived value is compared, so a mismatch cannot be located by
/// timing.
pub fn ct_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.ct_eq(right).into()
}

/// Derive the per-pair commitment key from the two identity X25519 public keys.
///
/// Sorted so both ends compute the same key without agreeing on who is "first",
/// and derived rather than concatenated so the key is a uniform 32 bytes with
/// its own domain separator. Not a secret from the peers (see the module note);
/// its job is to keep plaintext scope identifiers out of notices and to make a
/// commitment meaningless outside this pair.
pub fn scope_commit_key(
    self_x25519_pub: &[u8; 32],
    peer_x25519_pub: &[u8; 32],
) -> Result<[u8; 32], RevocationError> {
    let (first, second) = if self_x25519_pub <= peer_x25519_pub {
        (self_x25519_pub, peer_x25519_pub)
    } else {
        (peer_x25519_pub, self_x25519_pub)
    };
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(first);
    ikm[32..].copy_from_slice(second);
    crypto::hkdf::derive_32(&[], &ikm, HKDF_INFO_SCOPE_COMMIT)
        .map_err(|_| RevocationError::KeyDerivation)
}

/// `HMAC(key, "scope" ‖ storage_key)`. The only thing a notice says about which
/// conversation it means.
pub fn scope_commitment(key: &[u8; 32], storage_key: &str) -> [u8; 32] {
    mac(key, &[LABEL_SCOPE, storage_key.as_bytes()])
}

/// `HMAC(key, "burn" ‖ scope_commitment ‖ epoch ‖ upto_seq)`.
///
/// Binding the parameters into the identifier is what makes "reusing a burn id
/// with another set of parameters is rejected" checkable without storing the
/// parameters anywhere trusted.
pub fn burn_id(
    key: &[u8; 32],
    scope_commitment: &[u8; 32],
    burn_epoch: u64,
    burn_upto_seq: u64,
) -> [u8; 32] {
    mac(
        key,
        &[
            LABEL_BURN,
            scope_commitment,
            &burn_epoch.to_be_bytes(),
            &burn_upto_seq.to_be_bytes(),
        ],
    )
}

/// `HMAC(key, "lane" ‖ scope_commitment ‖ epoch)` — the keyserver's collapse key
/// for the revocation lane.
///
/// Deliberately **excludes** `burn_upto_seq`, unlike [`burn_id`]. The lane
/// collapses on `(scope, epoch)`, so a client that re-issues the same epoch with
/// a corrected range upserts onto the queued row instead of appending a
/// near-duplicate. Two different epochs are two different burns and get two rows.
///
/// The server sees only 32 opaque bytes: it learns neither the scope nor the
/// epoch, and cannot link one pair's lane to another's, because the key is
/// pair-specific.
pub fn lane_collapse_key(key: &[u8; 32], scope_commitment: &[u8; 32], burn_epoch: u64) -> [u8; 32] {
    mac(
        key,
        &[LABEL_LANE, scope_commitment, &burn_epoch.to_be_bytes()],
    )
}

/// `HMAC(key, "msg" ‖ scope_commitment ‖ message_id)` — an opaque handle for one
/// message, so an explicit target list carries no service message ids.
pub fn message_commitment(
    key: &[u8; 32],
    scope_commitment: &[u8; 32],
    message_id: &str,
) -> [u8; 32] {
    mac(
        key,
        &[LABEL_MESSAGE, scope_commitment, message_id.as_bytes()],
    )
}

fn hex32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        out.push(char::from_digit((b & 0x0f) as u32, 16).unwrap_or('0'));
    }
    out
}

// ---- Receiver ledger ----

/// One journalled burn. Parameters are kept so a repeat presentation of the
/// same id with *different* parameters is distinguishable from an identical
/// retry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub burn_epoch: u64,
    pub burn_upto_seq: u64,
    pub applied_at: i64,
}

/// Per-(peer, scope) receiver state. Keyed in [`RevocationLedger`] by
/// `hex(scope_commitment)`, which already binds the peer — the commitment key
/// is pair-specific — so this file contains no identifiers, handles or scope
/// names at all.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeRevocationState {
    /// Highest authenticated content `send_seq` accepted from this sender.
    pub high_water: u64,
    /// Highest honoured `burn_epoch`. The replay gate.
    pub last_burn_epoch: u64,
    /// Content from this sender with `send_seq <= burn_floor` is destroyed and
    /// refused. **Durable**, which is what makes burn-before-message fail
    /// closed: a message that arrives later with a sequence at or below the
    /// floor is never rendered.
    pub burn_floor: u64,
    /// `hex(burn_id)` -> parameters.
    pub journal: BTreeMap<String, JournalEntry>,
}

/// Receiver-side replay state. Persisted encrypted at rest by the Hub.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationLedger {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub scopes: BTreeMap<String, ScopeRevocationState>,
}

impl RevocationLedger {
    pub fn total_journal_entries(&self) -> usize {
        self.scopes.values().map(|s| s.journal.len()).sum()
    }

    fn state(&self, scope_commitment: &[u8; 32]) -> Option<&ScopeRevocationState> {
        self.scopes.get(&hex32(scope_commitment))
    }
}

/// What a receiver did with an inbound notice.
///
/// [`Self::Applied`] and [`Self::AlreadyApplied`] both ack `applied = true`, and
/// so does [`Self::Replay`] — the burn *is* in force in all three cases. Only a
/// refusal acks false. Collapsing them is deliberate: an ack that distinguished
/// "I destroyed it just now" from "I had nothing left" would be a
/// did-you-still-have-it oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundDecision {
    /// Newly applied by this notice.
    Applied,
    /// This exact `(burn_id, epoch, upto_seq)` was already applied.
    AlreadyApplied,
    /// Stale epoch. Ignored; the older burn it re-asserts remains in force.
    Replay,
    /// Refused. `burn_id` did not match its parameters, or the id was already
    /// journalled with different parameters, or a parameter was out of range.
    Refused(RefusalReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalReason {
    /// `burn_id` is not `HMAC(key, epoch ‖ upto_seq)` for the stated
    /// parameters — a forgery or a tampered notice.
    BurnIdMismatch,
    /// A journalled id presented with different parameters.
    BurnIdReuse,
    /// `burn_upto_seq` exceeds `high_water + MAX_BURN_LOOKAHEAD`.
    UptoSeqOutOfRange,
}

impl InboundDecision {
    /// The value that goes on the wire in [`RevocationAck`].
    pub fn applied(self) -> bool {
        match self {
            Self::Applied | Self::AlreadyApplied | Self::Replay => true,
            Self::Refused(_) => false,
        }
    }
}

/// Result of applying an inbound notice: the decision plus exactly what the
/// caller must destroy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundOutcome {
    pub decision: InboundDecision,
    /// Destroy this sender's content in this scope with `send_seq <= this`.
    /// Zero when nothing is to be destroyed.
    pub destroy_upto_seq: u64,
    /// Explicit message commitments to destroy in addition to the prefix.
    pub destroy_message_commitments: Vec<[u8; 32]>,
    /// The ack to return. Built here so a caller cannot accidentally leak a
    /// richer status onto the wire.
    pub ack: RevocationAck,
}

/// Apply an inbound revocation to the ledger.
///
/// `key` must be the commitment key for the **authenticated** sender of the
/// envelope. The caller has already matched `notice.scope_commitment` against
/// one of its own scopes for that sender (see [`match_scope_commitment`]);
/// this function never learns, and cannot leak, which scope that is.
///
/// Ordering inside: validate, then gate on replay, then take capacity, then
/// mutate. The ledger is only written when the burn is genuinely being applied,
/// so a refusal cannot consume journal space.
pub fn apply_inbound_revocation(
    ledger: &mut RevocationLedger,
    key: &[u8; 32],
    notice: &RevocationNotice,
    now: i64,
) -> Result<InboundOutcome, RevocationError> {
    let expected = burn_id(
        key,
        &notice.scope_commitment,
        notice.burn_epoch,
        notice.burn_upto_seq,
    );
    let ack_for = |decision: InboundDecision, destroy: u64, msgs: Vec<[u8; 32]>| InboundOutcome {
        decision,
        destroy_upto_seq: destroy,
        destroy_message_commitments: msgs,
        ack: RevocationAck {
            burn_id: notice.burn_id,
            applied: decision.applied(),
        },
    };

    if !ct_eq(&expected, &notice.burn_id) {
        return Ok(ack_for(
            InboundDecision::Refused(RefusalReason::BurnIdMismatch),
            0,
            Vec::new(),
        ));
    }

    let slot = hex32(&notice.scope_commitment);
    let known = ledger.scopes.contains_key(&slot);
    if !known && ledger.scopes.len() >= MAX_LEDGER_SCOPES {
        return Err(RevocationError::LedgerFull);
    }
    let existing = ledger.scopes.get(&slot);
    let high_water = existing.map(|s| s.high_water).unwrap_or(0);
    let last_epoch = existing.map(|s| s.last_burn_epoch).unwrap_or(0);
    let journalled = existing.and_then(|s| s.journal.get(&hex32(&notice.burn_id)).cloned());

    // An id we have already journalled. Identical parameters is an idempotent
    // retry; anything else is the reuse the contract forbids. Checked before
    // the epoch gate so an honest retry is never misreported as a replay.
    if let Some(entry) = journalled {
        if entry.burn_epoch == notice.burn_epoch && entry.burn_upto_seq == notice.burn_upto_seq {
            return Ok(ack_for(InboundDecision::AlreadyApplied, 0, Vec::new()));
        }
        return Ok(ack_for(
            InboundDecision::Refused(RefusalReason::BurnIdReuse),
            0,
            Vec::new(),
        ));
    }

    if notice.burn_epoch <= last_epoch {
        // Replay, or a reordered older burn. Idempotent and inert: no floor
        // movement, so a captured notice cannot reach content sent after it.
        return Ok(ack_for(InboundDecision::Replay, 0, Vec::new()));
    }

    if notice.burn_upto_seq > high_water.saturating_add(MAX_BURN_LOOKAHEAD) {
        return Ok(ack_for(
            InboundDecision::Refused(RefusalReason::UptoSeqOutOfRange),
            0,
            Vec::new(),
        ));
    }

    // Capacity, taken only now that the burn is certain to be applied.
    if ledger.total_journal_entries() >= MAX_JOURNAL_ENTRIES_TOTAL {
        return Err(RevocationError::JournalFull);
    }
    if existing.map(|s| s.journal.len()).unwrap_or(0) >= MAX_JOURNAL_ENTRIES_PER_SCOPE {
        return Err(RevocationError::JournalFull);
    }

    let state = ledger.scopes.entry(slot).or_default();
    state.last_burn_epoch = notice.burn_epoch;
    state.burn_floor = state.burn_floor.max(notice.burn_upto_seq);
    state.journal.insert(
        hex32(&notice.burn_id),
        JournalEntry {
            burn_epoch: notice.burn_epoch,
            burn_upto_seq: notice.burn_upto_seq,
            applied_at: now,
        },
    );
    ledger.version = 1;

    Ok(ack_for(
        InboundDecision::Applied,
        notice.burn_upto_seq,
        notice.message_commitments.clone(),
    ))
}

/// Find which of our own conversations with this sender a notice refers to.
///
/// `candidates` is the list of `storage_key`s we hold for the authenticated
/// sender. Comparison is constant-time. Returns the matching storage key, or
/// `None` — an unmatched commitment is simply not our business and must not be
/// probed further.
pub fn match_scope_commitment<'a, I>(
    key: &[u8; 32],
    commitment: &[u8; 32],
    candidates: I,
) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut found: Option<&'a str> = None;
    for candidate in candidates {
        if ct_eq(&scope_commitment(key, candidate), commitment) && found.is_none() {
            found = Some(candidate);
        }
    }
    found
}

/// Whether an inbound content message may be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentDecision {
    Accept,
    /// At or below a durable burn floor. This is the burn-before-message case:
    /// the floor outlives the notice, so a message that arrives afterwards is
    /// refused at decrypt and never rendered.
    RefusedBurned,
    /// The ledger could not be read. **Fail closed** — a read error means
    /// "already burned", never "fresh". `docs/design/burn-contract.md`:
    /// journals "fail closed instead of silently forgetting live replay state".
    RefusedLedgerUnavailable,
}

/// Gate one inbound content message against the burn floor.
pub fn accept_content(
    ledger: &RevocationLedger,
    scope_commitment: &[u8; 32],
    send_seq: u64,
) -> ContentDecision {
    match ledger.state(scope_commitment) {
        Some(state) if send_seq <= state.burn_floor => ContentDecision::RefusedBurned,
        _ => ContentDecision::Accept,
    }
}

/// Record that a content message was accepted, raising the high-water mark.
///
/// The mark is what an inbound legacy `0x01` and a `MAX_BURN_LOOKAHEAD` check
/// are measured against, so it must be recorded for every accepted message.
pub fn record_content_accepted(
    ledger: &mut RevocationLedger,
    scope_commitment: &[u8; 32],
    send_seq: u64,
) -> Result<(), RevocationError> {
    let slot = hex32(scope_commitment);
    if !ledger.scopes.contains_key(&slot) && ledger.scopes.len() >= MAX_LEDGER_SCOPES {
        return Err(RevocationError::LedgerFull);
    }
    let state = ledger.scopes.entry(slot).or_default();
    state.high_water = state.high_water.max(send_seq);
    ledger.version = 1;
    Ok(())
}

/// Convert an inbound legacy `MSG_TYPE_BURN` (`0x01`) into a bounded revocation.
///
/// The legacy marker carries no epoch and no sequence, so the only honest
/// reading is "everything of theirs I currently hold": `burn_upto_seq` is the
/// present high-water mark. The epoch is the next local one, which makes a
/// second `0x01` from the same peer idempotent-but-refreshable rather than
/// cumulative.
///
/// What this deliberately does **not** do is set a permanent scope flag. That
/// is the legacy `peer_map.burned_scopes` defect: it turned one stale or
/// replayed marker into a conversation that could never be used again,
/// including for content sent afterwards. Bounding an old peer's marker at the
/// current high-water mark fixes the denial of service for old peers too,
/// without asking them to upgrade.
pub fn legacy_burn_notice(
    ledger: &RevocationLedger,
    key: &[u8; 32],
    scope_commitment: &[u8; 32],
    now: i64,
) -> RevocationNotice {
    let state = ledger.state(scope_commitment);
    let upto = state.map(|s| s.high_water).unwrap_or(0);
    let epoch = state
        .map(|s| s.last_burn_epoch)
        .unwrap_or(0)
        .saturating_add(1);
    RevocationNotice {
        scope_commitment: *scope_commitment,
        burn_epoch: epoch,
        burn_upto_seq: upto,
        message_commitments: Vec::new(),
        burn_id: burn_id(key, scope_commitment, epoch, upto),
        issued_at: now,
    }
}

// ---- Sender-side counters ----

/// Sender-local monotonic counters, keyed by `hex(scope_commitment)`.
///
/// `send_seq` is per (sender, scope) and must be carried **inside** the
/// authenticated envelope of every content message, so a receiver cannot be
/// made to mis-order or replay content by a network attacker.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendCounters {
    #[serde(default)]
    pub version: u32,
    /// Last issued content sequence per scope.
    #[serde(default)]
    pub send_seq: BTreeMap<String, u64>,
    /// Last issued burn epoch per scope.
    #[serde(default)]
    pub burn_epoch: BTreeMap<String, u64>,
}

impl SendCounters {
    /// Allocate the next content sequence for a scope. Strictly increasing,
    /// starting at 1 — zero is reserved so "no sequence" and "the first
    /// message" are distinguishable.
    pub fn next_send_seq(&mut self, scope_commitment: &[u8; 32]) -> Result<u64, RevocationError> {
        Self::bump(&mut self.send_seq, scope_commitment)
    }

    /// Allocate the next burn epoch for a scope. Strictly increasing.
    pub fn next_burn_epoch(&mut self, scope_commitment: &[u8; 32]) -> Result<u64, RevocationError> {
        Self::bump(&mut self.burn_epoch, scope_commitment)
    }

    /// Highest content sequence issued for a scope, without allocating. This is
    /// the natural `burn_upto_seq` for "burn everything I have said here".
    pub fn current_send_seq(&self, scope_commitment: &[u8; 32]) -> u64 {
        self.send_seq
            .get(&hex32(scope_commitment))
            .copied()
            .unwrap_or(0)
    }

    fn bump(
        map: &mut BTreeMap<String, u64>,
        scope_commitment: &[u8; 32],
    ) -> Result<u64, RevocationError> {
        let slot = hex32(scope_commitment);
        if !map.contains_key(&slot) && map.len() >= MAX_COUNTER_SCOPES {
            return Err(RevocationError::CountersFull);
        }
        let entry = map.entry(slot).or_insert(0);
        let next = entry
            .checked_add(1)
            .ok_or(RevocationError::CounterExhausted)?;
        *entry = next;
        Ok(next)
    }
}

// ---- Sender-side durable outbox ----

/// One queued revocation.
///
/// `notice_b64` is the base64 CBOR [`RevocationNotice`] — commitments and
/// integers only, no plaintext scope or message identifier, and the file it
/// lives in is additionally encrypted at rest by the Hub. It is stored
/// pre-envelope rather than pre-sealed so each retry gets a fresh `encrypt_v3`
/// envelope (fresh ephemeral, fresh ML-KEM encapsulation) and so a peer's key
/// rotation does not strand a queued burn inside an envelope only the old key
/// could open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationOutboxEntry {
    /// OSL protocol identifier of the recipient. Needed to address the POST; it
    /// is not a service handle and is already the `recipient_id` the keyserver
    /// sees.
    pub recipient_id: String,
    /// Keyserver routing label. Already server-visible on the existing
    /// transport; carried here so a retry addresses the same lane.
    pub scope_id_label: String,
    /// Local `Scope::storage_key` this burn belongs to, so a status line can be
    /// shown for one conversation. Local-only; never on the wire.
    pub storage_key: String,
    pub burn_id_hex: String,
    /// `hex(lane_collapse_key(..))`. The keyserver's upsert key for the
    /// revocation lane: a retry for this same `(scope, epoch)` replaces the
    /// queued row instead of appending. Stored rather than recomputed so a retry
    /// cannot address a different row than the original post did.
    #[serde(default)]
    pub collapse_key_hex: String,
    pub burn_epoch: u64,
    pub burn_upto_seq: u64,
    pub notice_b64: String,
    #[serde(default)]
    pub attempts: u32,
    #[serde(default)]
    pub next_attempt_at: i64,
    #[serde(default)]
    pub acknowledged: bool,
    #[serde(default)]
    pub created_at: i64,
}

/// Durable, bounded, never-silently-evicting sender-side queue.
///
/// Today there is no sender-side retry at all: `post_control_inbox` is called
/// once and a failure is reported to the caller and forgotten. For a burn that
/// is the difference between "the peer will honour this when they next connect"
/// and "nothing was ever sent".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevocationOutbox {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<RevocationOutboxEntry>,
}

impl RevocationOutbox {
    /// Queue a revocation, collapsing rather than appending.
    ///
    /// A second burn for the same `(recipient, scope, epoch)` **replaces** the
    /// queued one. The keyserver lane is collapsible the same way, so a client
    /// that retries a burn cannot fill either side's lane with near-duplicates.
    ///
    /// Returns [`RevocationError::OutboxFull`] when there is no room and no
    /// entry to collapse into. Acknowledged entries are pruned first (they are
    /// history, kept only for the status line); an **unacknowledged** entry is
    /// never dropped to make room, because that is exactly the silent loss this
    /// outbox exists to prevent.
    pub fn enqueue(&mut self, entry: RevocationOutboxEntry) -> Result<(), RevocationError> {
        if let Some(slot) = self.entries.iter_mut().find(|e| {
            e.recipient_id == entry.recipient_id
                && e.scope_id_label == entry.scope_id_label
                && e.burn_epoch == entry.burn_epoch
        }) {
            *slot = entry;
            self.version = 1;
            return Ok(());
        }
        if self.entries.len() >= MAX_OUTBOX_ENTRIES {
            let before = self.entries.len();
            self.entries.retain(|e| !e.acknowledged);
            if self.entries.len() == before {
                return Err(RevocationError::OutboxFull);
            }
        }
        if self.entries.len() >= MAX_OUTBOX_ENTRIES {
            return Err(RevocationError::OutboxFull);
        }
        self.entries.push(entry);
        self.version = 1;
        Ok(())
    }

    /// Entries due for a send attempt at `now`, oldest first.
    pub fn due(&self, now: i64) -> Vec<&RevocationOutboxEntry> {
        let mut out: Vec<&RevocationOutboxEntry> = self
            .entries
            .iter()
            .filter(|e| !e.acknowledged && e.next_attempt_at <= now)
            .collect();
        out.sort_by_key(|e| e.created_at);
        out
    }

    /// Record a failed or unconfirmed attempt and schedule the next one.
    pub fn record_attempt(&mut self, burn_id_hex: &str, now: i64) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.burn_id_hex == burn_id_hex)
        {
            e.attempts = e.attempts.saturating_add(1);
            let idx = (e.attempts as usize).min(OUTBOX_BACKOFF_SECS.len() - 1);
            e.next_attempt_at = now.saturating_add(OUTBOX_BACKOFF_SECS[idx]);
            self.version = 1;
        }
    }

    /// Record a peer ack. Idempotent.
    pub fn record_acknowledged(&mut self, burn_id_hex: &str) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.burn_id_hex == burn_id_hex)
        {
            e.acknowledged = true;
            self.version = 1;
        }
    }

    /// Delivery status for one queued revocation, as a user-facing string.
    ///
    /// Never returns "Deleted".
    pub fn status(&self, burn_id_hex: &str) -> &'static str {
        match self.entries.iter().find(|e| e.burn_id_hex == burn_id_hex) {
            Some(e) if e.acknowledged => STATUS_ACKNOWLEDGED,
            Some(e) if e.attempts > 0 => STATUS_SENT_REQUEST,
            Some(_) => STATUS_NOT_ACKNOWLEDGED,
            None => STATUS_NOT_ACKNOWLEDGED,
        }
    }

    pub fn unacknowledged(&self) -> usize {
        self.entries.iter().filter(|e| !e.acknowledged).count()
    }

    /// Aggregate status for one conversation, for the burn receipt line.
    ///
    /// Deliberately pessimistic: a single unacknowledged peer keeps the whole
    /// conversation at `Sent request` (or `Not acknowledged` if nothing has even
    /// been attempted yet). Never returns "Deleted".
    pub fn status_for_scope(&self, storage_key: &str) -> &'static str {
        let mut any = false;
        let mut all_acked = true;
        let mut any_attempted = false;
        for e in self.entries.iter().filter(|e| e.storage_key == storage_key) {
            any = true;
            if e.acknowledged {
                any_attempted = true;
            } else {
                all_acked = false;
                if e.attempts > 0 {
                    any_attempted = true;
                }
            }
        }
        match (any, all_acked, any_attempted) {
            (false, _, _) => STATUS_NOT_ACKNOWLEDGED,
            (true, true, _) => STATUS_ACKNOWLEDGED,
            (true, false, true) => STATUS_SENT_REQUEST,
            (true, false, false) => STATUS_NOT_ACKNOWLEDGED,
        }
    }

    pub fn pending_for_scope(&self, storage_key: &str) -> usize {
        self.entries
            .iter()
            .filter(|e| e.storage_key == storage_key && !e.acknowledged)
            .count()
    }

    pub fn acknowledged_for_scope(&self, storage_key: &str) -> usize {
        self.entries
            .iter()
            .filter(|e| e.storage_key == storage_key && e.acknowledged)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELF_PK: [u8; 32] = [1u8; 32];
    const PEER_PK: [u8; 32] = [2u8; 32];
    const OTHER_PK: [u8; 32] = [3u8; 32];
    const SCOPE: &str = "dm:pair-a";

    fn key() -> [u8; 32] {
        scope_commit_key(&SELF_PK, &PEER_PK).unwrap()
    }

    fn notice(k: &[u8; 32], commitment: &[u8; 32], epoch: u64, upto: u64) -> RevocationNotice {
        RevocationNotice {
            scope_commitment: *commitment,
            burn_epoch: epoch,
            burn_upto_seq: upto,
            message_commitments: Vec::new(),
            burn_id: burn_id(k, commitment, epoch, upto),
            issued_at: 1_700_000_000,
        }
    }

    #[test]
    fn commit_key_is_order_independent_and_pair_specific() {
        assert_eq!(
            scope_commit_key(&SELF_PK, &PEER_PK).unwrap(),
            scope_commit_key(&PEER_PK, &SELF_PK).unwrap()
        );
        assert_ne!(
            scope_commit_key(&SELF_PK, &PEER_PK).unwrap(),
            scope_commit_key(&SELF_PK, &OTHER_PK).unwrap()
        );
    }

    #[test]
    fn a_commitment_reveals_nothing_and_differs_per_pair() {
        let a = scope_commitment(&key(), SCOPE);
        let b = scope_commitment(&scope_commit_key(&SELF_PK, &OTHER_PK).unwrap(), SCOPE);
        // Same conversation label, different pair -> unlinkable commitments.
        assert_ne!(a, b);
        // And the label is not recoverable by inspection: no byte of the
        // commitment equals the label's bytes in position.
        assert_ne!(
            &a[..SCOPE.len().min(32)],
            &SCOPE.as_bytes()[..SCOPE.len().min(32)]
        );
    }

    #[test]
    fn burn_id_binds_every_parameter() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let base = burn_id(&k, &c, 3, 10);
        assert_ne!(base, burn_id(&k, &c, 4, 10));
        assert_ne!(base, burn_id(&k, &c, 3, 11));
        assert_ne!(base, burn_id(&k, &scope_commitment(&k, "dm:other"), 3, 10));
    }

    /// Adversarial: forged burn. A peer (or anyone) who does not hold the
    /// commitment key cannot produce a matching `burn_id`, so nothing is
    /// destroyed and the ack is false.
    #[test]
    fn a_forged_burn_is_refused_and_destroys_nothing() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        let mut forged = notice(&k, &c, 1, 5);
        forged.burn_id = [0xAAu8; 32];
        let out = apply_inbound_revocation(&mut ledger, &k, &forged, 100).unwrap();
        assert_eq!(
            out.decision,
            InboundDecision::Refused(RefusalReason::BurnIdMismatch)
        );
        assert_eq!(out.destroy_upto_seq, 0);
        assert!(!out.ack.applied);
        assert!(
            ledger.scopes.is_empty(),
            "a refusal must not consume ledger space"
        );
    }

    /// Adversarial: a notice whose `burn_upto_seq` was edited upward in transit.
    /// The id no longer matches, so it is refused rather than widened.
    #[test]
    fn tampering_with_upto_seq_invalidates_the_notice() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        let mut tampered = notice(&k, &c, 1, 5);
        tampered.burn_upto_seq = 5_000;
        let out = apply_inbound_revocation(&mut ledger, &k, &tampered, 100).unwrap();
        assert_eq!(
            out.decision,
            InboundDecision::Refused(RefusalReason::BurnIdMismatch)
        );
    }

    /// Adversarial: replayed burn. Idempotent, and — the load-bearing part —
    /// it cannot reach content sent after the original.
    #[test]
    fn a_replayed_burn_cannot_destroy_later_content() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        for seq in 1..=5 {
            record_content_accepted(&mut ledger, &c, seq).unwrap();
        }
        let first = notice(&k, &c, 1, 5);
        assert_eq!(
            apply_inbound_revocation(&mut ledger, &k, &first, 100)
                .unwrap()
                .decision,
            InboundDecision::Applied
        );
        // Life goes on: messages 6..10 arrive and are accepted.
        for seq in 6..=10 {
            assert_eq!(accept_content(&ledger, &c, seq), ContentDecision::Accept);
            record_content_accepted(&mut ledger, &c, seq).unwrap();
        }
        // The captured notice is replayed verbatim.
        let out = apply_inbound_revocation(&mut ledger, &k, &first, 200).unwrap();
        assert_eq!(out.decision, InboundDecision::AlreadyApplied);
        assert_eq!(out.destroy_upto_seq, 0);
        assert!(
            out.ack.applied,
            "already-applied acks the same value as applied"
        );
        // Floor never moved, so 6..10 survive.
        assert_eq!(ledger.scopes[&hex32(&c)].burn_floor, 5);
        for seq in 6..=10 {
            assert_eq!(accept_content(&ledger, &c, seq), ContentDecision::Accept);
        }
    }

    /// A stale-epoch notice with a *wider* range — the strongest form of the
    /// replay attack — is dropped before it can move the floor.
    #[test]
    fn a_stale_epoch_cannot_widen_the_floor() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        for seq in 1..=20 {
            record_content_accepted(&mut ledger, &c, seq).unwrap();
        }
        apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, 5, 3), 100).unwrap();
        // Epoch 4 < 5, but asks for far more. Correctly signed for its own
        // parameters, so this is not a forgery — it is a reordered/old burn.
        let out = apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, 4, 20), 101).unwrap();
        assert_eq!(out.decision, InboundDecision::Replay);
        assert_eq!(out.destroy_upto_seq, 0);
        assert_eq!(ledger.scopes[&hex32(&c)].burn_floor, 3);
        assert_eq!(accept_content(&ledger, &c, 20), ContentDecision::Accept);
    }

    /// Adversarial: burn-before-message. The floor is durable, so a message
    /// that turns up afterwards inside the burnt range is refused at decrypt
    /// and never rendered.
    #[test]
    fn burn_before_message_fails_closed() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        // Nothing seen yet; the author burns their first ten messages.
        apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, 1, 10), 100).unwrap();
        for seq in 1..=10 {
            assert_eq!(
                accept_content(&ledger, &c, seq),
                ContentDecision::RefusedBurned,
                "seq {seq} is at or below a durable floor"
            );
        }
        assert_eq!(accept_content(&ledger, &c, 11), ContentDecision::Accept);
    }

    /// Adversarial: burning another author's content. There is no mechanism to
    /// even express it — the ledger slot is keyed by a *pair-specific*
    /// commitment, so a notice from peer B lands in a different slot than
    /// peer C's content in the same conversation, and the caller wipes with
    /// `Some(sender)`.
    #[test]
    fn a_burn_cannot_reach_another_authors_content() {
        let k_peer = key();
        let k_other = scope_commit_key(&SELF_PK, &OTHER_PK).unwrap();
        let c_peer = scope_commitment(&k_peer, SCOPE);
        let c_other = scope_commitment(&k_other, SCOPE);
        assert_ne!(c_peer, c_other);
        let mut ledger = RevocationLedger::default();
        for seq in 1..=5 {
            record_content_accepted(&mut ledger, &c_other, seq).unwrap();
        }
        apply_inbound_revocation(&mut ledger, &k_peer, &notice(&k_peer, &c_peer, 1, 5), 100)
            .unwrap();
        // The other author's content in the very same conversation is untouched.
        for seq in 1..=5 {
            assert_eq!(
                accept_content(&ledger, &c_other, seq),
                ContentDecision::Accept
            );
        }
        // And peer B's notice does not verify under peer C's key, so it cannot
        // be re-aimed by swapping the commitment.
        let mut aimed = notice(&k_peer, &c_peer, 2, 5);
        aimed.scope_commitment = c_other;
        assert_eq!(
            apply_inbound_revocation(&mut ledger, &k_other, &aimed, 101)
                .unwrap()
                .decision,
            InboundDecision::Refused(RefusalReason::BurnIdMismatch)
        );
    }

    /// Reusing an id with other parameters, first line of defence: the id is a
    /// MAC over those parameters, so the substitution simply does not verify.
    #[test]
    fn reusing_a_burn_id_with_other_parameters_is_rejected() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        let good = notice(&k, &c, 1, 4);
        apply_inbound_revocation(&mut ledger, &k, &good, 100).unwrap();
        let mut reused = notice(&k, &c, 2, 9);
        reused.burn_id = good.burn_id;
        let out = apply_inbound_revocation(&mut ledger, &k, &reused, 101).unwrap();
        assert_eq!(
            out.decision,
            InboundDecision::Refused(RefusalReason::BurnIdMismatch)
        );
        assert_eq!(ledger.scopes[&hex32(&c)].burn_floor, 4);
    }

    /// Second line of defence, independent of the MAC: the journal itself
    /// refuses a known id whose recorded parameters disagree. Only reachable if
    /// HMAC-SHA256 were broken (or a ledger file were tampered with on disk),
    /// which is exactly why it is here — and why it is tested by seeding the
    /// journal directly rather than by forging a MAC.
    #[test]
    fn the_journal_refuses_a_known_id_with_disagreeing_parameters() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        let incoming = notice(&k, &c, 2, 9);
        ledger.scopes.insert(
            hex32(&c),
            ScopeRevocationState {
                high_water: 20,
                last_burn_epoch: 1,
                burn_floor: 4,
                journal: BTreeMap::from([(
                    hex32(&incoming.burn_id),
                    JournalEntry {
                        burn_epoch: 1,
                        burn_upto_seq: 4,
                        applied_at: 100,
                    },
                )]),
            },
        );
        let out = apply_inbound_revocation(&mut ledger, &k, &incoming, 101).unwrap();
        assert_eq!(
            out.decision,
            InboundDecision::Refused(RefusalReason::BurnIdReuse)
        );
        assert!(!out.ack.applied);
        assert_eq!(ledger.scopes[&hex32(&c)].burn_floor, 4);
    }

    /// The lane collapse key must depend on the epoch but NOT on the range, or
    /// re-issuing the same epoch with a corrected range would append a second
    /// queued row instead of replacing the first.
    #[test]
    fn the_lane_collapse_key_is_per_epoch_not_per_range() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        assert_eq!(
            lane_collapse_key(&k, &c, 3),
            lane_collapse_key(&k, &c, 3),
            "deterministic"
        );
        assert_ne!(lane_collapse_key(&k, &c, 3), lane_collapse_key(&k, &c, 4));
        assert_ne!(
            lane_collapse_key(&k, &c, 3),
            lane_collapse_key(&k, &scope_commitment(&k, "dm:other"), 3)
        );
        // Not the burn id, so the server cannot correlate the two.
        assert_ne!(lane_collapse_key(&k, &c, 3), burn_id(&k, &c, 3, 0));
        // Pair-specific, so one pair's lane is unlinkable from another's.
        let other = scope_commit_key(&SELF_PK, &OTHER_PK).unwrap();
        assert_ne!(
            lane_collapse_key(&k, &c, 3),
            lane_collapse_key(&other, &c, 3)
        );
    }

    #[test]
    fn an_out_of_range_upto_seq_is_refused_not_clamped() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        let out =
            apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, 1, u64::MAX), 100).unwrap();
        assert_eq!(
            out.decision,
            InboundDecision::Refused(RefusalReason::UptoSeqOutOfRange)
        );
        assert!(ledger.scopes.is_empty());
    }

    #[test]
    fn the_journal_rejects_rather_than_forgets_when_full() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        for epoch in 1..=MAX_JOURNAL_ENTRIES_PER_SCOPE as u64 {
            assert_eq!(
                apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, epoch, epoch), 100)
                    .unwrap()
                    .decision,
                InboundDecision::Applied
            );
        }
        let overflow = MAX_JOURNAL_ENTRIES_PER_SCOPE as u64 + 1;
        assert_eq!(
            apply_inbound_revocation(&mut ledger, &k, &notice(&k, &c, overflow, overflow), 100),
            Err(RevocationError::JournalFull)
        );
        // Nothing was forgotten: every earlier burn is still journalled, so
        // none of them became replayable.
        assert_eq!(
            ledger.scopes[&hex32(&c)].journal.len(),
            MAX_JOURNAL_ENTRIES_PER_SCOPE
        );
    }

    #[test]
    fn the_ledger_refuses_a_new_pair_when_full_rather_than_evicting() {
        let k = key();
        let mut ledger = RevocationLedger::default();
        for i in 0..MAX_LEDGER_SCOPES {
            let c = scope_commitment(&k, &format!("dm:{i}"));
            record_content_accepted(&mut ledger, &c, 1).unwrap();
        }
        let fresh = scope_commitment(&k, "dm:one-too-many");
        assert_eq!(
            record_content_accepted(&mut ledger, &fresh, 1),
            Err(RevocationError::LedgerFull)
        );
        assert_eq!(
            apply_inbound_revocation(&mut ledger, &k, &notice(&k, &fresh, 1, 1), 100),
            Err(RevocationError::LedgerFull)
        );
        assert_eq!(ledger.scopes.len(), MAX_LEDGER_SCOPES);
    }

    /// Legacy `0x01` interop: honoured, bounded, and specifically NOT a
    /// permanent scope flag.
    #[test]
    fn legacy_burn_is_bounded_at_what_we_currently_hold() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut ledger = RevocationLedger::default();
        for seq in 1..=7 {
            record_content_accepted(&mut ledger, &c, seq).unwrap();
        }
        let converted = legacy_burn_notice(&ledger, &k, &c, 500);
        assert_eq!(converted.burn_upto_seq, 7);
        assert_eq!(converted.burn_epoch, 1);
        let out = apply_inbound_revocation(&mut ledger, &k, &converted, 500).unwrap();
        assert_eq!(out.decision, InboundDecision::Applied);
        assert_eq!(out.destroy_upto_seq, 7);
        // The old peer keeps talking afterwards and is NOT permanently dead.
        assert_eq!(accept_content(&ledger, &c, 8), ContentDecision::Accept);
        record_content_accepted(&mut ledger, &c, 8).unwrap();
        // A second legacy marker re-reads the (now higher) high-water mark.
        let again = legacy_burn_notice(&ledger, &k, &c, 600);
        assert_eq!(again.burn_upto_seq, 8);
        assert_eq!(again.burn_epoch, 2);
        assert_eq!(
            apply_inbound_revocation(&mut ledger, &k, &again, 600)
                .unwrap()
                .decision,
            InboundDecision::Applied
        );
    }

    #[test]
    fn a_legacy_burn_for_an_unknown_scope_does_not_wedge_it() {
        let k = key();
        let c = scope_commitment(&k, "dm:never-seen");
        let mut ledger = RevocationLedger::default();
        let converted = legacy_burn_notice(&ledger, &k, &c, 10);
        assert_eq!(converted.burn_upto_seq, 0);
        apply_inbound_revocation(&mut ledger, &k, &converted, 10).unwrap();
        // Sequences start at 1, so a zero floor blocks nothing at all.
        assert_eq!(accept_content(&ledger, &c, 1), ContentDecision::Accept);
    }

    #[test]
    fn send_sequences_are_strictly_increasing_and_start_at_one() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let mut counters = SendCounters::default();
        assert_eq!(counters.current_send_seq(&c), 0);
        assert_eq!(counters.next_send_seq(&c).unwrap(), 1);
        assert_eq!(counters.next_send_seq(&c).unwrap(), 2);
        assert_eq!(counters.current_send_seq(&c), 2);
        assert_eq!(counters.next_burn_epoch(&c).unwrap(), 1);
        // Independent budgets per scope.
        let d = scope_commitment(&k, "dm:other");
        assert_eq!(counters.next_send_seq(&d).unwrap(), 1);
    }

    #[test]
    fn match_scope_commitment_finds_our_own_scope_and_nothing_else() {
        let k = key();
        let c = scope_commitment(&k, SCOPE);
        let candidates = ["dm:x", SCOPE, "gc:y"];
        assert_eq!(
            match_scope_commitment(&k, &c, candidates.iter().copied()),
            Some(SCOPE)
        );
        let stranger = scope_commitment(&scope_commit_key(&SELF_PK, &OTHER_PK).unwrap(), SCOPE);
        assert_eq!(
            match_scope_commitment(&k, &stranger, candidates.iter().copied()),
            None
        );
    }

    fn outbox_entry(recipient: &str, epoch: u64, id: &str) -> RevocationOutboxEntry {
        RevocationOutboxEntry {
            recipient_id: recipient.to_owned(),
            scope_id_label: "lane".to_owned(),
            storage_key: SCOPE.to_owned(),
            burn_id_hex: id.to_owned(),
            collapse_key_hex: format!("{id}-lane"),
            burn_epoch: epoch,
            burn_upto_seq: 1,
            notice_b64: "AAAA".to_owned(),
            attempts: 0,
            next_attempt_at: 0,
            acknowledged: false,
            created_at: 0,
        }
    }

    /// A conversation's status is pessimistic: one unacknowledged peer keeps it
    /// off `Acknowledged by peer`, and it is never "Deleted".
    #[test]
    fn scope_status_needs_every_peer_to_acknowledge() {
        let mut outbox = RevocationOutbox::default();
        assert_eq!(outbox.status_for_scope(SCOPE), STATUS_NOT_ACKNOWLEDGED);
        outbox.enqueue(outbox_entry("peer-a", 1, "aa")).unwrap();
        outbox.enqueue(outbox_entry("peer-b", 1, "bb")).unwrap();
        assert_eq!(outbox.status_for_scope(SCOPE), STATUS_NOT_ACKNOWLEDGED);
        outbox.record_attempt("aa", 0);
        assert_eq!(outbox.status_for_scope(SCOPE), STATUS_SENT_REQUEST);
        outbox.record_acknowledged("aa");
        assert_eq!(
            outbox.status_for_scope(SCOPE),
            STATUS_SENT_REQUEST,
            "one peer acknowledging is not the conversation acknowledging"
        );
        assert_eq!(outbox.pending_for_scope(SCOPE), 1);
        outbox.record_acknowledged("bb");
        assert_eq!(outbox.status_for_scope(SCOPE), STATUS_ACKNOWLEDGED);
        assert_eq!(outbox.pending_for_scope(SCOPE), 0);
        assert_eq!(outbox.acknowledged_for_scope(SCOPE), 2);
        // Another conversation is unaffected.
        assert_eq!(
            outbox.status_for_scope("dm:elsewhere"),
            STATUS_NOT_ACKNOWLEDGED
        );
    }

    /// Adversarial: offline peer. The entry stays queued, is retried with
    /// backoff, and is never dropped.
    #[test]
    fn an_offline_peer_leaves_the_revocation_queued_and_retried() {
        let mut outbox = RevocationOutbox::default();
        outbox.enqueue(outbox_entry("peer", 1, "aa")).unwrap();
        assert_eq!(outbox.due(0).len(), 1);
        outbox.record_attempt("aa", 0);
        assert_eq!(outbox.status("aa"), STATUS_SENT_REQUEST);
        // Backed off, then due again.
        assert!(outbox.due(1).is_empty());
        assert_eq!(outbox.due(OUTBOX_BACKOFF_SECS[1]).len(), 1);
        assert_eq!(outbox.unacknowledged(), 1);
        outbox.record_acknowledged("aa");
        assert_eq!(outbox.status("aa"), STATUS_ACKNOWLEDGED);
        assert_eq!(outbox.unacknowledged(), 0);
        assert!(outbox.due(i64::MAX).is_empty());
    }

    #[test]
    fn the_outbox_collapses_a_repeat_burn_for_the_same_epoch() {
        let mut outbox = RevocationOutbox::default();
        outbox.enqueue(outbox_entry("peer", 1, "aa")).unwrap();
        let mut second = outbox_entry("peer", 1, "bb");
        second.burn_upto_seq = 9;
        outbox.enqueue(second).unwrap();
        assert_eq!(outbox.entries.len(), 1);
        assert_eq!(outbox.entries[0].burn_upto_seq, 9);
        // A different epoch is a different burn and appends.
        outbox.enqueue(outbox_entry("peer", 2, "cc")).unwrap();
        assert_eq!(outbox.entries.len(), 2);
    }

    /// Adversarial: revocation lane full. The enqueue is **refused** so the
    /// caller knows, and no unacknowledged revocation is thrown away.
    #[test]
    fn a_full_outbox_refuses_rather_than_dropping_an_unacknowledged_burn() {
        let mut outbox = RevocationOutbox::default();
        for i in 0..MAX_OUTBOX_ENTRIES {
            outbox
                .enqueue(outbox_entry("peer", i as u64, &format!("{i:04x}")))
                .unwrap();
        }
        assert_eq!(
            outbox.enqueue(outbox_entry("peer", 9_999, "ffff")),
            Err(RevocationError::OutboxFull)
        );
        assert_eq!(outbox.entries.len(), MAX_OUTBOX_ENTRIES);
        assert_eq!(outbox.unacknowledged(), MAX_OUTBOX_ENTRIES);
        // Acknowledged history is what makes room, never a pending burn.
        outbox.record_acknowledged("0000");
        outbox.enqueue(outbox_entry("peer", 9_999, "ffff")).unwrap();
        assert_eq!(outbox.unacknowledged(), MAX_OUTBOX_ENTRIES);
    }

    /// `osl-gui-final-plan.md:494`: "`Sent request` is never displayed as
    /// `Deleted`." The status a burn reports must therefore never be the word
    /// Deleted, and the only claim that may use "deleted" at all is the local
    /// one, which is true and verifiable on this device.
    #[test]
    fn no_status_ever_reads_as_deleted() {
        for status in [
            STATUS_SENT_REQUEST,
            STATUS_ACKNOWLEDGED,
            STATUS_NOT_ACKNOWLEDGED,
        ] {
            assert!(
                !status.to_lowercase().contains("delete"),
                "osl-gui-final-plan.md:494 forbids rendering a burn status as Deleted"
            );
        }
        let mut outbox = RevocationOutbox::default();
        outbox.enqueue(outbox_entry("peer", 1, "aa")).unwrap();
        for now in [0i64, 10, i64::MAX] {
            let _ = outbox.due(now);
            assert!(!outbox.status("aa").to_lowercase().contains("delete"));
        }
    }

    /// The three claims are separate statements with different strengths, and
    /// the recipient-copies one states its bound rather than implying it.
    #[test]
    fn the_three_claims_are_distinct_and_the_bound_is_explicit() {
        assert_ne!(CLAIM_CONTENT_EXPIRY, CLAIM_LOCAL_REMOVAL);
        assert_ne!(CLAIM_LOCAL_REMOVAL, CLAIM_RECIPIENT_COPIES);
        // Only the local claim may say "deleted"; it is the verifiable one.
        assert!(!CLAIM_CONTENT_EXPIRY.to_lowercase().contains("delete"));
        assert!(!CLAIM_RECIPIENT_COPIES.to_lowercase().contains("delete"));
        assert!(CLAIM_LOCAL_REMOVAL.contains("from this device"));
        assert!(CLAIM_RECIPIENT_COPIES.contains("no guarantee"));
        assert!(CLAIM_RECIPIENT_COPIES.contains("no bound on when"));
        // Nothing claims the platform removed anything.
        for claim in [
            CLAIM_CONTENT_EXPIRY,
            CLAIM_LOCAL_REMOVAL,
            CLAIM_RECIPIENT_COPIES,
        ] {
            let lowered = claim.to_lowercase();
            assert!(!lowered.contains("discord"));
            assert!(!lowered.contains("from the platform"));
        }
    }
}
