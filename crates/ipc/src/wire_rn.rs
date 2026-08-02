//! OSL-RN (wire `0x10`) integration: version selection with downgrade
//! protection, and sealed per-peer ratchet session persistence.
//!
//! This module is the *only* sanctioned entry point to
//! [`osl_ratchet_next`] from application code. It exists because the
//! raw crate deliberately cannot enforce two things that are policy,
//! not protocol:
//!
//! 1. **Which wire version to use for a given peer** — and, crucially,
//!    that the answer never moves *downward*.
//! 2. **Where the ratchet's secret state lives** — sealed at rest,
//!    written atomically, and degrading to a clean re-handshake rather
//!    than to a silent plaintext or legacy send when it is lost.
//!
//! The production send path calls [`select_wire_version`] as a
//! downgrade guard before sending legacy wire formats. OSL-RN
//! encryption/decryption remains behind [`RN_WIRE_IN_ENABLED`].
//!
//! # Downgrade protection: what is enforced here
//!
//! `osl_ratchet_next::negotiate` binds the negotiated version into the
//! handshake `SK`, so two parties that disagree about what was
//! negotiated fail closed. That is necessary but not sufficient: it
//! cannot help if the local side never *chooses* OSL-RN, which is
//! exactly what an attacker who strips the capability from a peer
//! record achieves.
//!
//! So this module adds a **sticky, monotone version pin**
//! ([`RnPeerPin`]):
//!
//! - The pin only ever moves upward ([`RnPeerPin::raise_to_rn`] is the
//!   only mutator, and there is no lowering operation *at all* — not a
//!   private one, not a test-only one).
//! - Once a peer's pin says OSL-RN, [`select_wire_version`] can return
//!   [`SelectedVersion::Rn`] or an error. It is structurally incapable
//!   of returning [`SelectedVersion::LegacyV3`]: that arm is guarded by
//!   the pin check before the capability check is ever consulted.
//! - There is **no "try OSL-RN, fall back on error"** anywhere. An
//!   authentication failure is not an input to version selection.
//!
//! ## Why the pin is stored separately from the session state
//!
//! This is load-bearing and easy to get wrong. If the pin lived inside
//! the session blob, then losing or deleting the session — a supported,
//! expected event that must degrade to a clean re-handshake — would
//! also drop the pin, and the very next send would be permitted to use
//! v=3. Deleting one file would be a complete downgrade attack.
//!
//! The pin therefore lives in its own file, and:
//!
//! - Losing the **session** costs a re-handshake and nothing else.
//! - Losing the **pin** is fail-closed too: [`load_pin`] treats an
//!   unreadable or corrupt pin file as [`RnPeerPin::UNKNOWN`] only when
//!   the file is *absent*. A file that exists but does not parse is an
//!   error, never a silent reset to "v=3 is fine".
//!
//! # What this module never does
//!
//! It does not log, hash into a diagnostic, persist in plaintext, or
//! `Debug`-print plaintext, draft text, or key material. The session
//! blob is sealed before it reaches the filesystem and the plaintext
//! export is zeroized on every path, including error paths.

use osl_ratchet_next::{
    negotiate::Negotiation, KemPublic, KemSecret, LocalPrekeys, Opened, PeerBundle, SecureSession,
    Session, SessionParams, XPublic, XSecret, MLKEM_EK, WIRE_VERSION_RN,
};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use subtle::ConstantTimeEq;
use zeroize::Zeroize;

/// Legacy wire version this module is guarding against falling back to.
/// Mirrors `crate::wire_v2::WIRE_VERSION_V3` without depending on it, so
/// the guard cannot be silently broken by an edit over there.
pub const LEGACY_WIRE_VERSION_V3: u8 = 0x03;

/// Single switch that turns OSL-RN send/receive wire-in on.
///
/// The stronger wire is enabled after its delivery preconditions landed:
/// live-capability advertisement, one canonical session directory, and a
/// per-peer writer lock.
pub const RN_WIRE_IN_ENABLED: bool = true;

/// Canonical subdirectory for sealed OSL-RN session state.
///
/// Every production path must construct its store with
/// [`RnSessionStore::for_config_dir`] so send, receive, and initiation
/// operate on the same persisted ratchet state.
pub const RN_SESSION_DIR: &str = "rn_sessions";

/// Negotiation context for the Discord manual-peer path. A fixed
/// constant: it is an input to the handshake `SK`, so both sides must
/// use the identical value, and it must never be derived from anything
/// that varies per install or per build.
pub const RN_CONTEXT_DISCORD_MANUAL: &[u8] = b"osl-hub/discord-manual-peer/v1";

/// State-blob format version for the sealed session file.
const SESSION_BLOB_VERSION: u32 = 2;
/// Format version for the separately persisted send-counter high-water mark.
const SESSION_SEND_FLOOR_VERSION: u32 = 1;
/// State-blob format version for the pin file.
const PIN_BLOB_VERSION: u32 = 1;

/// Hard ceiling on one peer's sealed session file, enforced on **both**
/// the write and the read.
///
/// `osl_ratchet_next` bounds a live session at roughly 300 KiB
/// (`DESIGN.md` §8); sealing adds a nonce and tag and the base64 in the
/// JSON envelope multiplies by 4/3, so a legitimate worst-case file is
/// ~410 KiB. 1 MiB leaves headroom without letting an unbounded read
/// happen. Checking on write means a bug that inflated the export is
/// caught where it is diagnosable; checking on read means a file that
/// grew by some other route cannot be turned into a 1-GiB allocation
/// before any authentication happens.
const MAX_SESSION_FILE_BYTES: u64 = 1024 * 1024;

/// Ceiling on the pin file. A pin is two small integers.
const MAX_PIN_FILE_BYTES: u64 = 4 * 1024;
/// Ceiling on the small, unsealed send-counter floor record.
const MAX_SESSION_SEND_FLOOR_FILE_BYTES: u64 = 4 * 1024;

/// Ceiling on how many peers we hold sealed ratchet state for.
///
/// At [`MAX_SESSION_FILE_BYTES`] each, this bounds the store at 512 MiB
/// worst case and — far more importantly — bounds the `read_dir` scan
/// and the number of live conversations one profile can be pushed into
/// holding by a peer-creation flood.
///
/// **Reaching the cap refuses the new record; it never evicts an
/// existing one.** Silently evicting would destroy a live conversation's
/// ratchet, and because the pin deliberately survives session loss the
/// victim conversation would then be unable to send at all until a
/// re-handshake. Refusing is loud, recoverable, and cannot be used by a
/// third party to knock out a conversation they are not part of. This
/// mirrors `docs/design/offline-controls-and-opened-receipts.md:7-8`
/// ("Neither structure may silently evict live records when full").
/// [`RnSessionStore::delete_session`] is the only way to free a slot and
/// it is always an explicit, local decision.
const MAX_SESSION_RECORDS: usize = 512;

/// Policy ceiling on the skipped-key cache an imported session may
/// claim.
///
/// `import_state` restores the `SkipParams` that were *exported*, so the
/// file decides the cache size. The file is sealed, so this is not an
/// attacker-controlled value — but a rolled-back profile, a
/// hand-modified build, or a future version with looser defaults could
/// silently raise this process's memory ceiling. Refusing keeps the
/// worst-case resident cost of a loaded session a property of *this*
/// build rather than of whatever wrote the file.
const MAX_SKIPPED_KEYS_POLICY: usize = 4096;

/// Errors from this module.
///
/// **Deliberately has no variant meaning "fall back to v=3".** Every
/// failure here is terminal for the OSL-RN path: the caller either sends
/// OSL-RN or reports failure to the user. A caller that pattern-matches
/// this enum looking for permission to downgrade will not find it.
#[derive(Debug, thiserror::Error)]
pub enum RnError {
    /// OSL-RN send/receive is present but deliberately not wired in.
    #[error("OSL-RN wire-in is disabled")]
    WireInDisabled,

    /// The peer is pinned to OSL-RN but we were asked to consider, or
    /// could not produce, anything else. Fail closed.
    #[error("peer is pinned to OSL-RN; refusing to send a legacy v=3 message")]
    PinnedToRn,

    /// The peer does not advertise OSL-RN and policy requires it.
    #[error("policy requires OSL-RN for this peer but the peer does not support it")]
    RnRequiredButUnsupported,

    /// Another thread or process is currently advancing this peer's
    /// session. Retrying from the state already loaded by this caller
    /// would reuse a deterministic body nonce, so the caller must fail
    /// and let its outbox retry from a freshly loaded state later.
    #[error("OSL-RN session for this peer is busy with another writer")]
    WriterBusy,

    /// A restored session state would reuse one or more deterministic
    /// message-key nonces that this device has already consumed.
    #[error("refusing rolled-back OSL-RN session blob: send counter {blob_counter} is below persisted high-water mark {high_water}")]
    RolledBackSession { blob_counter: u32, high_water: u32 },

    /// The active sealer would write the session state in plaintext.
    /// Refused: the export contains every secret the session holds.
    #[error("refusing to persist ratchet state: the active sealer does not encrypt at rest")]
    PlaintextSealerRefused,

    /// A peer's ML-KEM encapsulation key was not the expected length.
    #[error("peer ML-KEM-768 encapsulation key has wrong length")]
    BadPeerKemKey,

    /// B5 prekey state or a fetched B5 prekey bundle could not be
    /// adapted into the already-authenticated OSL-RN handshake shape.
    #[error("B5 prekey supply cannot be adapted to OSL-RN: {0}")]
    PrekeyAdapter(&'static str),

    /// The protocol layer rejected something.
    #[error("OSL-RN protocol error: {0}")]
    Protocol(String),

    /// A sealed state blob was larger than the on-disk bound. Refused
    /// rather than written, so the bound cannot be exceeded by a bug in
    /// the layer above.
    #[error(
        "refusing to persist ratchet state: sealed blob is {got} bytes, over the {max}-byte bound"
    )]
    StateTooLarge { got: usize, max: u64 },

    /// The store already holds the maximum number of peer records.
    /// **No existing record is evicted to make room** — see
    /// `MAX_SESSION_RECORDS`.
    #[error("refusing a new peer record: the sealed session store already holds {held} peers (cap {max}); no live record is evicted to make room")]
    StoreFull { held: usize, max: usize },

    /// An imported session claimed a skipped-key cache larger than this
    /// build's policy ceiling.
    #[error("session state claims a skipped-key cache of {got} keys, over this build's {max}-key ceiling")]
    SkippedCacheTooLarge { got: usize, max: usize },

    /// Sealed-storage failure.
    #[error("OSL-RN state storage error: {0}")]
    Storage(String),
}

impl From<osl_ratchet_next::Error> for RnError {
    fn from(e: osl_ratchet_next::Error) -> Self {
        // `osl_ratchet_next::Error` is already scrubbed of key material
        // and collapses every AEAD failure into one indistinguishable
        // variant, so forwarding its `Display` leaks nothing.
        RnError::Protocol(e.to_string())
    }
}

#[cfg(not(test))]
fn wire_in_enabled() -> bool {
    RN_WIRE_IN_ENABLED
}

#[cfg(test)]
thread_local! {
    static RN_WIRE_IN_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn wire_in_enabled() -> bool {
    RN_WIRE_IN_TEST_OVERRIDE.with(|enabled| enabled.get().unwrap_or(RN_WIRE_IN_ENABLED))
}

pub fn app_state_wire_in_enabled(state: &crate::AppState) -> bool {
    state.rn_wire_in_enabled()
}

// ---------------------------------------------------------------
// B5 prekey supply adapters
// ---------------------------------------------------------------

/// Adapt local B5 prekey state into the OSL-RN receive-side handshake
/// shape.
///
/// The adapter validates that every public half in the B5 state is
/// actually bound to the corresponding local secret before it is handed
/// to the ratchet. A mismatch is a refusal, never permission to proceed
/// with a weaker or unbound handshake.
pub fn local_prekeys_from_b5(
    identity: &keystore::Identity,
    prekeys: &keystore::PrekeyState,
) -> Result<LocalPrekeys, RnError> {
    if crypto::x25519::derive_public(&identity.x25519_secret) != identity.x25519_public {
        return Err(RnError::PrekeyAdapter(
            "local identity X25519 public key does not match the secret key",
        ));
    }
    if !verify_spk_signature(
        identity.ed25519_public.as_bytes(),
        &prekeys.current_spk.public,
        &prekeys.current_spk.signature,
    )? {
        return Err(RnError::PrekeyAdapter(
            "local signed prekey signature is invalid",
        ));
    }
    if derived_x25519_public(prekeys.current_spk.secret) != prekeys.current_spk.public {
        return Err(RnError::PrekeyAdapter(
            "local signed prekey public key does not match the secret key",
        ));
    }

    let mut one_time_prekeys = Vec::with_capacity(prekeys.opk_pool.len());
    for opk in &prekeys.opk_pool {
        if derived_x25519_public(opk.secret) != opk.public {
            return Err(RnError::PrekeyAdapter(
                "local one-time prekey public key does not match the secret key",
            ));
        }
        let rn_id = b5_opk_id_to_rn(opk.id)?;
        one_time_prekeys.push((rn_id, XSecret::from_bytes(opk.secret)));
    }

    Ok(LocalPrekeys {
        identity: XSecret::from_bytes(*identity.x25519_secret.as_bytes()),
        signed_prekey: XSecret::from_bytes(prekeys.current_spk.secret),
        one_time_prekeys,
        pq_prekey: KemSecret::from_bytes(identity.mlkem_secret_bytes())
            .map_err(|_| RnError::PrekeyAdapter("local ML-KEM decapsulation key is malformed"))?,
    })
}

/// Adapt a fetched B5 prekey bundle into the OSL-RN initiator-side
/// handshake shape.
///
/// This verifies the signed prekey binding carried by the B5 supply.
/// Whole-record identity authentication still belongs to the caller's
/// TOFU / platform-binding layer, matching `osl_ratchet_next`'s
/// `PeerBundle` trust boundary.
pub fn peer_bundle_from_b5(
    bundle: &keystore::client::PrekeyBundleResponse,
) -> Result<PeerBundle, RnError> {
    let identity = decode_b64_array::<32>(&bundle.ik_x25519_pub, "peer identity X25519 key")?;
    let ed25519 = decode_b64_array::<32>(&bundle.ik_ed25519_pub, "peer Ed25519 key")?;
    let signed_prekey = decode_b64_array::<32>(&bundle.spk_pub, "peer signed prekey")?;
    let spk_signature =
        decode_b64_array::<64>(&bundle.spk_signature, "peer signed prekey signature")?;
    if !verify_spk_signature(&ed25519, &signed_prekey, &spk_signature)? {
        return Err(RnError::PrekeyAdapter(
            "peer signed prekey signature is invalid",
        ));
    }

    let pq = decode_b64_array::<MLKEM_EK>(&bundle.ik_mlkem768_pub, "peer ML-KEM key")?;
    let one_time_prekey = match &bundle.opk {
        Some(opk) => {
            let opk_public = decode_b64_array::<32>(&opk.pub_b64, "peer one-time prekey")?;
            Some((b5_opk_id_to_rn(opk.id)?, XPublic::from_bytes(opk_public)))
        }
        None => None,
    };

    Ok(PeerBundle {
        identity: XPublic::from_bytes(identity),
        signed_prekey: XPublic::from_bytes(signed_prekey),
        one_time_prekey,
        pq_prekey: KemPublic::from_bytes(&pq)
            .map_err(|_| RnError::PrekeyAdapter("peer ML-KEM key is malformed"))?,
    })
}

/// Adapt a fetched prekey response only after it has been merged into
/// a caller-authenticated identity bundle.
fn peer_bundle_from_verified_b5(
    merged: &keystore::identity_bundle::MergedIdentityBundle,
) -> Result<PeerBundle, RnError> {
    let one_time_prekey = merged
        .prekey
        .opk
        .map(|(id, public)| {
            let rn_id = b5_opk_id_to_rn(id)?;
            Ok::<_, RnError>((rn_id, XPublic::from_bytes(public)))
        })
        .transpose()?;

    Ok(PeerBundle {
        identity: XPublic::from_bytes(merged.identity.x25519_identity_pub),
        signed_prekey: XPublic::from_bytes(merged.prekey.spk_x25519_pub),
        one_time_prekey,
        pq_prekey: KemPublic::from_bytes(&merged.identity.mlkem768_identity_pub)
            .map_err(|_| RnError::PrekeyAdapter("peer ML-KEM key is malformed"))?,
    })
}

fn verify_spk_signature(
    ed25519_public: &[u8; 32],
    spk_public: &[u8; 32],
    signature: &[u8; 64],
) -> Result<bool, RnError> {
    let public = crypto::ed25519::PublicKey::from_bytes(*ed25519_public);
    let signature = crypto::ed25519::Signature::from_bytes(*signature);
    crypto::ed25519::verify(&public, spk_public, &signature)
        .map_err(|_| RnError::PrekeyAdapter("signed prekey signature is malformed"))
}

fn derived_x25519_public(secret: [u8; 32]) -> [u8; 32] {
    let secret = crypto::x25519::SecretKey::from_bytes(secret);
    *crypto::x25519::derive_public(&secret).as_bytes()
}

fn decode_b64_array<const N: usize>(value: &str, field: &'static str) -> Result<[u8; N], RnError> {
    use base64::Engine as _;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| RnError::PrekeyAdapter("prekey bundle field is not base64"))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| RnError::PrekeyAdapter(field))
}

fn b5_opk_id_to_rn(id: u32) -> Result<u32, RnError> {
    id.checked_add(1).ok_or(RnError::PrekeyAdapter(
        "one-time prekey id cannot be represented on RN wire",
    ))
}

// ---------------------------------------------------------------
// Version selection
// ---------------------------------------------------------------

/// Which wire version a send should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatchetPolicyDecision {
    /// The existing PQ-hybrid wrap. No forward secrecy.
    LegacyV3,
    /// OSL-RN, wire `0x10`.
    Rn,
}

/// Backwards-compatible name for callers that only care about the RN
/// wire-version decision.
pub type SelectedVersion = RatchetPolicyDecision;

/// Per-peer policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RnPolicy {
    /// Use OSL-RN when the peer supports it. First contact with a peer
    /// that does not advertise support falls back to v=3 — which is the
    /// residual first-contact exposure documented in
    /// `osl_ratchet_next::negotiate`.
    #[default]
    Opportunistic,
    /// Never send anything but OSL-RN to this peer, even on first
    /// contact. Closes the first-contact gap at the cost of being unable
    /// to talk to legacy peers at all.
    Required,
}

/// The sticky, monotone per-peer version pin.
///
/// There is intentionally no way to lower `min_wire_version`. The type
/// exposes exactly one mutator and it only raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RnPeerPin {
    /// The lowest wire version we are willing to *send* to this peer.
    min_wire_version: u8,
}

impl RnPeerPin {
    /// No observation yet: legacy sends are still permitted.
    pub const UNKNOWN: RnPeerPin = RnPeerPin {
        min_wire_version: LEGACY_WIRE_VERSION_V3,
    };

    /// Raise the pin to OSL-RN. Idempotent, and the only mutator.
    pub fn raise_to_rn(&mut self) {
        if self.min_wire_version < WIRE_VERSION_RN {
            self.min_wire_version = WIRE_VERSION_RN;
        }
    }

    /// Is this peer pinned to OSL-RN?
    pub fn is_pinned_to_rn(&self) -> bool {
        self.min_wire_version >= WIRE_VERSION_RN
    }

    pub fn min_wire_version(&self) -> u8 {
        self.min_wire_version
    }

    /// Reject a pin value we never write, so a hand-edited or truncated
    /// file cannot install a nonsense floor.
    fn validated(self) -> Result<Self, RnError> {
        if self.min_wire_version == LEGACY_WIRE_VERSION_V3
            || self.min_wire_version == WIRE_VERSION_RN
        {
            Ok(self)
        } else {
            Err(RnError::Storage(
                "pin file carries an unrecognised minimum wire version".into(),
            ))
        }
    }
}

/// Choose the wire version for a send.
///
/// The pin is consulted **first**. When the peer is pinned to OSL-RN
/// this function cannot return [`SelectedVersion::LegacyV3`] for any
/// combination of the remaining arguments — that is the downgrade
/// guarantee, and it is enforced by control flow rather than by a
/// caller remembering to check.
pub fn select_wire_version(
    pin: &RnPeerPin,
    peer_capabilities: keystore::client::PeerCapabilities,
    policy: RnPolicy,
) -> Result<SelectedVersion, RnError> {
    if pin.is_pinned_to_rn() {
        // Pinned. The only permitted outcomes are OSL-RN or an error.
        return if peer_capabilities.supports_rn() {
            Ok(SelectedVersion::Rn)
        } else {
            // A peer that previously spoke OSL-RN and now claims not to
            // is either rolled back or being impersonated. Either way,
            // refusing is correct.
            Err(RnError::PinnedToRn)
        };
    }

    // Bit 0 establishes the sticky pin, but cannot safely select a new
    // OSL-RN send: fuse-closed builds already advertise it. Bit 1 is only
    // advertised by builds that can actually accept the RN wire.
    let peer_supports_rn_live = peer_capabilities.supports_rn_live();
    match (policy, peer_supports_rn_live) {
        (_, true) => Ok(SelectedVersion::Rn),
        (RnPolicy::Required, false) => Err(RnError::RnRequiredButUnsupported),
        (RnPolicy::Opportunistic, false) => Ok(SelectedVersion::LegacyV3),
    }
}

// ---------------------------------------------------------------
// Negotiation digest construction
// ---------------------------------------------------------------

/// Build the negotiation binding digest for a session we are
/// *initiating*, from the peer record we hold.
pub fn initiator_binding(
    peer_identity_x25519: &[u8; 32],
    peer_mlkem768_ek: &[u8],
    own_identity_x25519: &[u8; 32],
    context: &[u8],
) -> Result<[u8; 32], RnError> {
    if peer_mlkem768_ek.len() != MLKEM_EK {
        return Err(RnError::BadPeerKemKey);
    }
    rn_negotiation(
        peer_identity_x25519,
        peer_mlkem768_ek,
        own_identity_x25519,
        context,
    )
    .digest()
    .map_err(RnError::from)
}

/// Build the negotiation binding digest for a session we are
/// *accepting*, from our own published keys plus the initiator identity
/// carried in the bootstrap preamble.
///
/// The initiator identity is **not authenticated** at this point; a
/// wrong value simply produces a digest that does not match, i.e. a
/// clean `AuthFailed`. It must not be treated as proof of authorship
/// until the accept succeeds.
pub fn responder_binding(
    own_identity_x25519: &[u8; 32],
    own_mlkem768_ek: &[u8],
    initiator_identity_x25519: &[u8; 32],
    context: &[u8],
) -> Result<[u8; 32], RnError> {
    if own_mlkem768_ek.len() != MLKEM_EK {
        return Err(RnError::BadPeerKemKey);
    }
    rn_negotiation(
        own_identity_x25519,
        own_mlkem768_ek,
        initiator_identity_x25519,
        context,
    )
    .digest()
    .map_err(RnError::from)
}

fn rn_negotiation<'a>(
    responder_identity_x25519: &'a [u8; 32],
    responder_mlkem768_ek: &'a [u8],
    initiator_identity_x25519: &'a [u8; 32],
    context: &'a [u8],
) -> Negotiation<'a> {
    let mut negotiation = Negotiation::for_rn(
        responder_identity_x25519,
        responder_mlkem768_ek,
        initiator_identity_x25519,
        context,
    );
    negotiation.min_acceptable_version = WIRE_VERSION_RN;
    negotiation
}

// ---------------------------------------------------------------
// Sealed per-peer state
// ---------------------------------------------------------------

/// Sealed, per-peer storage for ratchet session state and version pins.
///
/// One file per peer per kind. Per-peer granularity keeps a torn write
/// from taking out more than one conversation and avoids rewriting every
/// session on every message.
#[derive(Clone)]
pub struct RnSessionStore {
    dir: PathBuf,
}

/// Owns the exclusive per-peer writer lock until the ratchet transition has
/// been durably saved. The lock file is deliberately separate from the sealed
/// state so a crash leaves no ambiguous partially-written session record.
struct RnSessionWriterLock {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl Drop for RnSessionWriterLock {
    fn drop(&mut self) {
        // Windows does not permit unlinking an open file, so close before
        // removal. A process crash still leaves the lock file behind, which
        // fails closed as WriterBusy rather than allowing a stale-state retry.
        drop(self.file.take());
        let _ = std::fs::remove_file(&self.path);
    }
}

impl std::fmt::Debug for RnSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RnSessionStore")
            .field("dir", &"<redacted>")
            .finish()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SealedBlob {
    version: u32,
    method: String,
    /// Duplicates the authenticated session counter so it can be compared to
    /// the durable high-water mark before the session is used.
    sending_counter: u32,
    sealed_b64: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SessionSendFloor {
    version: u32,
    high_water: u32,
}

impl RnSessionStore {
    /// Build the session store for an account configuration directory.
    ///
    /// Older builds used `rn/`; migrate it into the canonical directory
    /// before opening so a previously initiated session remains available to
    /// all current send and receive paths.
    pub fn for_config_dir(config_dir: impl AsRef<Path>) -> Result<Self, RnError> {
        let config_dir = config_dir.as_ref();
        let canonical_dir = config_dir.join(RN_SESSION_DIR);
        let legacy_dir = config_dir.join("rn");

        if legacy_dir.exists() {
            if !legacy_dir.is_dir() {
                return Err(RnError::Storage(format!(
                    "legacy RN session path is not a directory: {}",
                    legacy_dir.display()
                )));
            }
            if !canonical_dir.exists() {
                std::fs::rename(&legacy_dir, &canonical_dir)
                    .map_err(|e| RnError::Storage(format!("migrate legacy RN session dir: {e}")))?;
            } else {
                if !canonical_dir.is_dir() {
                    return Err(RnError::Storage(format!(
                        "canonical RN session path is not a directory: {}",
                        canonical_dir.display()
                    )));
                }
                let entries = std::fs::read_dir(&legacy_dir)
                    .map_err(|e| RnError::Storage(format!("scan legacy RN session dir: {e}")))?
                    .map(|entry| {
                        entry
                            .map_err(|e| {
                                RnError::Storage(format!("read legacy RN session dir: {e}"))
                            })
                            .map(|entry| (entry.path(), canonical_dir.join(entry.file_name())))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some((_, destination)) =
                    entries.iter().find(|(_, destination)| destination.exists())
                {
                    return Err(RnError::Storage(format!(
                        "cannot migrate legacy RN session entry; canonical path already exists: {}",
                        destination.display()
                    )));
                }
                for (source, destination) in entries {
                    std::fs::rename(source, destination).map_err(|e| {
                        RnError::Storage(format!("migrate legacy RN session entry: {e}"))
                    })?;
                }
                std::fs::remove_dir(&legacy_dir).map_err(|e| {
                    RnError::Storage(format!("remove migrated legacy RN session dir: {e}"))
                })?;
            }
        }

        Ok(Self::new(canonical_dir))
    }

    /// `dir` should be a subdirectory of the active account directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        RnSessionStore { dir: dir.into() }
    }

    /// Storage key for a peer: a domain-separated hash of the peer's
    /// public identity key, truncated to 16 bytes and hex-encoded.
    ///
    /// The input is public (it is a public key), so this leaks nothing;
    /// hashing it just keeps raw key bytes out of filenames and gives a
    /// fixed-length name.
    fn peer_key(peer_identity_x25519: &[u8; 32]) -> String {
        let mut h = Sha256::new();
        h.update(b"OSL-RN/v1/session-file/");
        h.update(peer_identity_x25519);
        let out = h.finalize();
        out.iter().take(16).map(|b| format!("{b:02x}")).collect()
    }

    fn session_path(&self, peer_identity_x25519: &[u8; 32]) -> PathBuf {
        self.dir
            .join(format!("{}.session", Self::peer_key(peer_identity_x25519)))
    }

    fn writer_lock_path(&self, peer_identity_x25519: &[u8; 32]) -> PathBuf {
        self.dir
            .join(format!("{}.lock", Self::peer_key(peer_identity_x25519)))
    }

    /// Acquire the non-blocking, cross-process writer exclusion for one peer.
    ///
    /// The caller must keep the returned guard alive over its whole
    /// load/advance/save transition. A busy writer is an explicit failure:
    /// queuing or retrying from already-loaded state would duplicate the
    /// message key and its deterministic body nonce.
    fn acquire_writer_lock(
        &self,
        peer_identity_x25519: &[u8; 32],
    ) -> Result<RnSessionWriterLock, RnError> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| RnError::Storage(format!("create state dir for writer lock: {e}")))?;
        let path = self.writer_lock_path(peer_identity_x25519);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::AlreadyExists => RnError::WriterBusy,
                _ => RnError::Storage(format!("acquire RN session writer lock: {e}")),
            })?;
        Ok(RnSessionWriterLock {
            path,
            file: Some(file),
        })
    }

    fn pin_path(&self, peer_identity_x25519: &[u8; 32]) -> PathBuf {
        self.dir
            .join(format!("{}.pin", Self::peer_key(peer_identity_x25519)))
    }

    fn pin_floor_path(&self, peer_identity_x25519: &[u8; 32]) -> PathBuf {
        self.dir.join(format!(
            "{}.pin-floor",
            Self::peer_key(peer_identity_x25519)
        ))
    }

    /// This file is deliberately separate from the restorable session blob.
    /// Replacing a `.session` with an old backup therefore cannot lower the
    /// last locally consumed send counter.
    fn send_floor_path(&self, peer_identity_x25519: &[u8; 32]) -> PathBuf {
        self.dir.join(format!(
            "{}.send-floor",
            Self::peer_key(peer_identity_x25519)
        ))
    }

    // ---- session state ----

    /// Persist a session, sealed.
    ///
    /// Refuses outright if the active sealer writes plaintext: the
    /// export contains the root key, every chain key, every header key,
    /// cached message keys and the ML-KEM decapsulation key.
    ///
    /// **Call this before the message leaves the machine, not after.**
    /// Persisting after a successful send means a crash in between
    /// leaves the peer's ratchet ahead of ours, which is unrecoverable
    /// without a re-handshake. Persisting first costs at most one burnt
    /// counter if the send then fails, which the ratchet tolerates as an
    /// ordinary skipped message.
    pub fn save_session(
        &self,
        peer_identity_x25519: &[u8; 32],
        session: &Session,
    ) -> Result<(), RnError> {
        let sealer = keystore::select_best_sealer();
        self.save_session_with_sealer(peer_identity_x25519, session, sealer.as_ref())
    }

    pub fn save_session_with_sealer(
        &self,
        peer_identity_x25519: &[u8; 32],
        session: &Session,
        sealer: &dyn keystore::sealer::Sealer,
    ) -> Result<(), RnError> {
        if sealer.requires_insecure_banner() {
            return Err(RnError::PlaintextSealerRefused);
        }
        let mut plain = SecureSession::export(session).map_err(RnError::from)?;
        let sealed = sealer.seal(&plain);
        plain.zeroize();
        let sealed = sealed.map_err(|e| RnError::Storage(e.to_string()))?;

        let blob = SealedBlob {
            version: SESSION_BLOB_VERSION,
            method: sealer.method_label().to_string(),
            sending_counter: session.sending_counter(),
            sealed_b64: b64(&sealed),
        };
        let json = serde_json::to_vec(&blob)
            .map_err(|e| RnError::Storage(format!("serialize session blob: {e}")))?;
        if json.len() as u64 > MAX_SESSION_FILE_BYTES {
            return Err(RnError::StateTooLarge {
                got: json.len(),
                max: MAX_SESSION_FILE_BYTES,
            });
        }

        // Record cap. Only a *new* peer can be refused; overwriting an
        // existing record is always allowed, so a conversation already
        // under way can never be starved by the cap.
        let path = self.session_path(peer_identity_x25519);
        if !path.exists() {
            let held = self.session_record_count()?;
            if held >= MAX_SESSION_RECORDS {
                return Err(RnError::StoreFull {
                    held,
                    max: MAX_SESSION_RECORDS,
                });
            }
        }
        // Persist the floor first. If the subsequent session write fails, the
        // old blob is refused rather than allowed to reissue a nonce.
        self.persist_send_floor(peer_identity_x25519, blob.sending_counter)?;
        atomic_write(&path, &json)
    }

    /// How many peer session records the store currently holds.
    ///
    /// Counts `*.session` only, so the `*.tmp` file `atomic_write` uses
    /// mid-write is never counted.
    fn session_record_count(&self) -> Result<usize, RnError> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(RnError::Storage(format!("scan state dir: {e}"))),
        };
        let mut held = 0usize;
        for entry in entries {
            let entry = entry.map_err(|e| RnError::Storage(format!("scan state dir: {e}")))?;
            if entry.path().extension().and_then(|x| x.to_str()) == Some("session") {
                held = held.saturating_add(1);
            }
        }
        Ok(held)
    }

    /// Load a session.
    ///
    /// `Ok(None)` means "no session on file" — the caller must
    /// re-handshake. That is the **only** degradation path, and it is
    /// deliberately not the same thing as an error: a missing session is
    /// routine, a corrupt one is not.
    pub fn load_session(
        &self,
        peer_identity_x25519: &[u8; 32],
    ) -> Result<Option<Session>, RnError> {
        let sealer = keystore::select_best_sealer();
        self.load_session_with_sealer(peer_identity_x25519, sealer.as_ref())
    }

    pub fn load_session_with_sealer(
        &self,
        peer_identity_x25519: &[u8; 32],
        sealer: &dyn keystore::sealer::Sealer,
    ) -> Result<Option<Session>, RnError> {
        let path = self.session_path(peer_identity_x25519);
        let bytes = match read_bounded(&path, MAX_SESSION_FILE_BYTES, "session")? {
            Some(b) => b,
            None => return Ok(None),
        };
        let blob: SealedBlob = match serde_json::from_slice(&bytes) {
            Ok(blob) => blob,
            Err(_) if looks_like_legacy_v4_session_blob(&bytes) => {
                retire_legacy_v4_session_file(&path)?;
                return Ok(None);
            }
            Err(e) => return Err(RnError::Storage(format!("parse session blob: {e}"))),
        };
        if blob.version != SESSION_BLOB_VERSION {
            return Err(RnError::Storage(format!(
                "session blob version {} != {SESSION_BLOB_VERSION}",
                blob.version
            )));
        }
        if let Some(high_water) = self.load_send_floor(peer_identity_x25519)? {
            if blob.sending_counter < high_water {
                return Err(RnError::RolledBackSession {
                    blob_counter: blob.sending_counter,
                    high_water,
                });
            }
        }
        // Constant-time compare of the method label: it is not secret,
        // but comparing it in constant time costs nothing and keeps the
        // "no data-dependent branches on stored blob fields" habit.
        if blob
            .method
            .as_bytes()
            .ct_eq(sealer.method_label().as_bytes())
            .unwrap_u8()
            == 0
        {
            return Err(RnError::Storage(
                "session blob was sealed by a different sealer".into(),
            ));
        }
        let sealed =
            unb64(&blob.sealed_b64).map_err(|e| RnError::Storage(format!("blob base64: {e}")))?;
        let mut plain = sealer
            .unseal(&sealed)
            .map_err(|e| RnError::Storage(e.to_string()))?;
        let session = <Session as SecureSession>::import(&plain);
        plain.zeroize();
        let session = session.map_err(RnError::from)?;

        if session.sending_counter() != blob.sending_counter {
            return Err(RnError::Storage(
                "session blob send counter does not match sealed session state".into(),
            ));
        }

        // The blob dictates the skipped-key caps it was exported with.
        // Refuse one that would raise this build's memory ceiling.
        let claimed = session.skip_params().max_total_keys;
        if claimed > MAX_SKIPPED_KEYS_POLICY {
            return Err(RnError::SkippedCacheTooLarge {
                got: claimed,
                max: MAX_SKIPPED_KEYS_POLICY,
            });
        }
        Ok(Some(session))
    }

    fn load_send_floor(&self, peer_identity_x25519: &[u8; 32]) -> Result<Option<u32>, RnError> {
        let path = self.send_floor_path(peer_identity_x25519);
        let Some(bytes) = read_bounded(&path, MAX_SESSION_SEND_FLOOR_FILE_BYTES, "send floor")?
        else {
            return Ok(None);
        };
        let floor: SessionSendFloor = serde_json::from_slice(&bytes)
            .map_err(|e| RnError::Storage(format!("parse send floor: {e}")))?;
        if floor.version != SESSION_SEND_FLOOR_VERSION {
            return Err(RnError::Storage(format!(
                "send floor version {} != {SESSION_SEND_FLOOR_VERSION}",
                floor.version
            )));
        }
        Ok(Some(floor.high_water))
    }

    fn persist_send_floor(
        &self,
        peer_identity_x25519: &[u8; 32],
        sending_counter: u32,
    ) -> Result<(), RnError> {
        if let Some(high_water) = self.load_send_floor(peer_identity_x25519)? {
            if sending_counter < high_water {
                return Err(RnError::RolledBackSession {
                    blob_counter: sending_counter,
                    high_water,
                });
            }
        }
        let floor = SessionSendFloor {
            version: SESSION_SEND_FLOOR_VERSION,
            high_water: sending_counter,
        };
        let json = serde_json::to_vec(&floor)
            .map_err(|e| RnError::Storage(format!("serialize send floor: {e}")))?;
        atomic_write(&self.send_floor_path(peer_identity_x25519), &json)
    }

    /// Delete a peer's session, forcing a clean re-handshake.
    ///
    /// Does **not** touch the pin: the peer stays pinned to OSL-RN, so
    /// the re-handshake cannot be a v=3 send. This asymmetry is the
    /// whole reason the two live in separate files.
    pub fn delete_session(&self, peer_identity_x25519: &[u8; 32]) -> Result<(), RnError> {
        match std::fs::remove_file(self.session_path(peer_identity_x25519)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(RnError::Storage(format!("delete session: {e}"))),
        }
    }

    // ---- pins ----

    /// Load a peer's pin. An absent file is [`RnPeerPin::UNKNOWN`]; a
    /// present but unparseable one is an error, never a silent reset.
    pub fn load_pin(&self, peer_identity_x25519: &[u8; 32]) -> Result<RnPeerPin, RnError> {
        let pin = self.load_pin_file(&self.pin_path(peer_identity_x25519), "pin")?;
        let floor = self.load_pin_floor(peer_identity_x25519)?;
        match (pin, floor) {
            (Some(pin), Some(floor)) if pin.min_wire_version() < floor.min_wire_version() => Err(
                RnError::Storage("pin file is below the persisted RN downgrade floor".into()),
            ),
            (Some(pin), _) => Ok(pin),
            (None, Some(floor)) => Ok(floor),
            (None, None) => Ok(RnPeerPin::UNKNOWN),
        }
    }

    /// Raise a peer's pin to OSL-RN and persist it.
    ///
    /// Read-modify-write through [`RnPeerPin::raise_to_rn`], so a
    /// concurrent writer can only ever raise it too. There is no
    /// `write_pin` that takes an arbitrary value.
    pub fn raise_pin_to_rn(&self, peer_identity_x25519: &[u8; 32]) -> Result<RnPeerPin, RnError> {
        let mut pin = self.load_pin(peer_identity_x25519)?;
        if pin.is_pinned_to_rn() {
            self.persist_pin_floor_to_rn(peer_identity_x25519)?;
            return Ok(pin);
        }
        pin.raise_to_rn();
        self.persist_pin_floor_to_rn(peer_identity_x25519)?;
        let json = serde_json::to_vec(&serde_json::json!({
            "version": PIN_BLOB_VERSION,
            "pin": pin,
        }))
        .map_err(|e| RnError::Storage(format!("serialize pin: {e}")))?;
        atomic_write(&self.pin_path(peer_identity_x25519), &json)?;
        Ok(pin)
    }

    fn load_pin_floor(
        &self,
        peer_identity_x25519: &[u8; 32],
    ) -> Result<Option<RnPeerPin>, RnError> {
        let Some(floor) =
            self.load_pin_file(&self.pin_floor_path(peer_identity_x25519), "pin floor")?
        else {
            return Ok(None);
        };
        if floor.is_pinned_to_rn() {
            Ok(Some(floor))
        } else {
            Err(RnError::Storage(
                "pin floor file is below the RN downgrade floor".into(),
            ))
        }
    }

    fn load_pin_file(&self, path: &Path, what: &str) -> Result<Option<RnPeerPin>, RnError> {
        let bytes = match read_bounded(path, MAX_PIN_FILE_BYTES, what)? {
            Some(b) => b,
            None => return Ok(None),
        };
        #[derive(serde::Deserialize)]
        struct PinFile {
            version: u32,
            pin: RnPeerPin,
        }
        let file: PinFile = serde_json::from_slice(&bytes)
            .map_err(|e| RnError::Storage(format!("parse {what}: {e}")))?;
        if file.version != PIN_BLOB_VERSION {
            return Err(RnError::Storage(format!(
                "{what} version {} != {PIN_BLOB_VERSION}",
                file.version
            )));
        }
        file.pin.validated().map(Some)
    }

    fn persist_pin_floor_to_rn(&self, peer_identity_x25519: &[u8; 32]) -> Result<(), RnError> {
        let floor = RnPeerPin {
            min_wire_version: WIRE_VERSION_RN,
        };
        let json = serde_json::to_vec(&serde_json::json!({
            "version": PIN_BLOB_VERSION,
            "pin": floor,
        }))
        .map_err(|e| RnError::Storage(format!("serialize pin floor: {e}")))?;
        atomic_write(&self.pin_floor_path(peer_identity_x25519), &json)
    }
}

// ---------------------------------------------------------------
// Session lifecycle helpers
// ---------------------------------------------------------------

/// Start an OSL-RN session towards a peer and persist it.
///
/// Raises the peer's pin only when the caller provides authenticated
/// capabilities that support OSL-RN. Absent or unverified capabilities
/// are not irreversible evidence, so they must not self-inflict a
/// permanent downgrade refusal.
#[allow(clippy::too_many_arguments)]
pub fn initiate_and_persist(
    store: &RnSessionStore,
    own_identity_secret: &osl_ratchet_next::XSecret,
    own_identity_public: &[u8; 32],
    peer: &PeerBundle,
    caps: keystore::client::PeerCapabilities,
    peer_mlkem768_ek: &[u8],
    context: &[u8],
    params: SessionParams,
) -> Result<Session, RnError> {
    let sealer = keystore::select_best_sealer();
    initiate_and_persist_with_sealer(
        store,
        sealer.as_ref(),
        own_identity_secret,
        own_identity_public,
        peer,
        caps,
        peer_mlkem768_ek,
        context,
        params,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn initiate_and_persist_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    own_identity_secret: &osl_ratchet_next::XSecret,
    own_identity_public: &[u8; 32],
    peer: &PeerBundle,
    caps: keystore::client::PeerCapabilities,
    peer_mlkem768_ek: &[u8],
    context: &[u8],
    params: SessionParams,
) -> Result<Session, RnError> {
    let peer_for_handshake;
    let peer = if context == RN_CONTEXT_DISCORD_MANUAL {
        // The current command responder reconstructs receive-side prekeys from
        // sealed identity fields only; it has no durable OPK consumption path
        // in this manual Discord context. Do not create a bootstrap that the
        // corresponding responder cannot authenticate. B5 adapter contexts keep
        // their OPKs and remain covered by the local_prekeys_from_b5 tests.
        peer_for_handshake = PeerBundle {
            identity: peer.identity,
            signed_prekey: peer.signed_prekey,
            one_time_prekey: None,
            pq_prekey: peer.pq_prekey.clone(),
        };
        &peer_for_handshake
    } else {
        peer
    };
    let peer_id = *peer.identity.as_bytes();
    let _writer_lock = store.acquire_writer_lock(&peer_id)?;
    let binding = initiator_binding(
        peer.identity.as_bytes(),
        peer_mlkem768_ek,
        own_identity_public,
        context,
    )?;
    let session = osl_ratchet_next::initiate_rn_bound(own_identity_secret, peer, &binding, params)
        .map_err(RnError::from)?;
    store.save_session_with_sealer(&peer_id, &session, sealer)?;
    if caps.supports_rn() {
        store.raise_pin_to_rn(&peer_id)?;
    }
    Ok(session)
}

/// Accept an inbound OSL-RN bootstrap message and persist the resulting
/// session.
///
/// Raises the peer's pin on success — a peer whose bootstrap
/// authenticated demonstrably speaks OSL-RN, which is stronger evidence
/// than any advertisement.
pub fn accept_and_persist(
    store: &RnSessionStore,
    local: &LocalPrekeys,
    own_identity_public: &[u8; 32],
    own_mlkem768_ek: &[u8],
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    let sealer = keystore::select_best_sealer();
    accept_and_persist_with_sealer(
        store,
        sealer.as_ref(),
        local,
        own_identity_public,
        own_mlkem768_ek,
        wire,
        context,
        params,
    )
}

// Eight separate arguments on purpose. This is the testable accept/persist
// boundary: store, sealer, local prekeys, public identity material, wire
// payload, context, and session params are independently supplied inputs.
#[allow(clippy::too_many_arguments)]
pub fn accept_and_persist_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    local: &LocalPrekeys,
    own_identity_public: &[u8; 32],
    own_mlkem768_ek: &[u8],
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    let initiator = osl_ratchet_next::peek_bootstrap_initiator_identity(wire)
        .map_err(RnError::from)?
        .ok_or_else(|| RnError::Protocol("not a bootstrap message".into()))?;
    let _writer_lock = store.acquire_writer_lock(initiator.as_bytes())?;
    let binding = responder_binding(
        own_identity_public,
        own_mlkem768_ek,
        initiator.as_bytes(),
        context,
    )?;
    let (session, opened) =
        osl_ratchet_next::accept_rn_bound(local, wire, &binding, params).map_err(RnError::from)?;
    let peer_id = *initiator.as_bytes();
    store.save_session_with_sealer(&peer_id, &session, sealer)?;
    store.raise_pin_to_rn(&peer_id)?;
    Ok((session, opened))
}

/// Accept an inbound OSL-RN bootstrap against live B5 prekey state.
///
/// The OPK named by the authenticated bootstrap is consumed only after
/// the RN accept path has authenticated the first message and the
/// resulting session has been sealed. A replay of the same bootstrap
/// therefore fails because the one-time key is no longer available.
pub fn accept_b5_prekey_state_and_persist(
    store: &RnSessionStore,
    identity: &keystore::Identity,
    prekeys: &mut keystore::PrekeyState,
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    let sealer = keystore::select_best_sealer();
    accept_b5_prekey_state_and_persist_with_sealer(
        store,
        sealer.as_ref(),
        identity,
        prekeys,
        wire,
        context,
        params,
    )
}

/// Accept an inbound OSL-RN bootstrap message using live B5 prekey
/// state, consuming the B5 OPK exactly once after authentication.
pub fn accept_b5_prekeys_and_persist(
    store: &RnSessionStore,
    identity: &keystore::Identity,
    prekeys: &mut keystore::PrekeyState,
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    accept_b5_prekey_state_and_persist(store, identity, prekeys, wire, context, params)
}

pub fn accept_b5_prekey_state_and_persist_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    identity: &keystore::Identity,
    prekeys: &mut keystore::PrekeyState,
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    let initiator = osl_ratchet_next::peek_bootstrap_initiator_identity(wire)
        .map_err(RnError::from)?
        .ok_or_else(|| RnError::Protocol("not a bootstrap message".into()))?;
    let consumed_b5_opk = b5_opk_id_from_bootstrap_wire(wire)?;
    let local = local_prekeys_from_b5(identity, prekeys)?;
    let binding = responder_binding(
        identity.x25519_public.as_bytes(),
        &identity.mlkem_public_bytes,
        initiator.as_bytes(),
        context,
    )?;
    let (session, opened) =
        osl_ratchet_next::accept_rn_bound(&local, wire, &binding, params).map_err(RnError::from)?;

    if let Some(opk_id) = consumed_b5_opk {
        if !prekeys.consume_opk(opk_id) {
            return Err(RnError::PrekeyAdapter(
                "accepted one-time prekey was not present for consumption",
            ));
        }
    }

    let peer_id = *initiator.as_bytes();
    store.save_session_with_sealer(&peer_id, &session, sealer)?;
    store.raise_pin_to_rn(&peer_id)?;
    Ok((session, opened))
}

pub fn accept_b5_prekeys_and_persist_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    identity: &keystore::Identity,
    prekeys: &mut keystore::PrekeyState,
    wire: &str,
    context: &[u8],
    params: SessionParams,
) -> Result<(Session, Opened), RnError> {
    accept_b5_prekey_state_and_persist_with_sealer(
        store, sealer, identity, prekeys, wire, context, params,
    )
}

/// Result of fetching the peer's B5 prekey bundle and bootstrapping an
/// OSL-RN first-contact session.
///
/// Deliberately no `Debug`: the live session is secret-bearing state.
pub struct FirstContactRnSession {
    pub session: Session,
    pub remaining_opk_count: u32,
    pub accepted_identity_revision: u64,
}

/// Fetch the peer's B5 prekey bundle and use it to start an OSL-RN
/// first-contact handshake.
///
/// The trust order is load-bearing:
/// 1. Verify the caller-provided identity bundle against the caller's
///    pinned Ed25519 authority.
/// 2. Select OSL-RN only from that authenticated capability bitmap.
/// 3. Fetch the prekey response.
/// 4. Merge the response into the authenticated bundle, refusing any
///    substituted identity field or unauthenticated SPK.
/// 5. Adapt the verified merge result into the RN handshake shape.
///
/// A server response, an absent capability, or a missing pinned signer
/// never becomes authority for first contact.
#[allow(clippy::too_many_arguments)]
pub fn fetch_prekey_bundle_and_initiate_first_contact(
    client: &keystore::KeyServerClient,
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    requester: &keystore::Identity,
    recipient_user_id: &str,
    peer_identity_bundle: &keystore::identity_bundle::IdentityBundle,
    pinned_peer_signer: &crypto::ed25519::PublicKey,
    last_known_revision: Option<u64>,
    policy: RnPolicy,
    context: &[u8],
    params: SessionParams,
) -> Result<FirstContactRnSession, RnError> {
    if peer_identity_bundle.capability_bundle > keystore::client::RN_CAP_MAX {
        return Err(RnError::PrekeyAdapter(
            "peer capability bitmap is unsupported",
        ));
    }
    let accepted_identity_revision = keystore::identity_bundle::BundleVerifyPolicy::new()
        .verify(
            peer_identity_bundle,
            pinned_peer_signer,
            last_known_revision,
        )
        .map_err(|_| RnError::PrekeyAdapter("peer identity bundle is not authenticated"))?;
    let caps = keystore::client::PeerCapabilities::Verified(peer_identity_bundle.capability_bundle);
    let pin = store.load_pin(&peer_identity_bundle.x25519_identity_pub)?;
    if select_wire_version(&pin, caps, policy)? != SelectedVersion::Rn {
        return Err(RnError::RnRequiredButUnsupported);
    }

    let response = client
        .fetch_prekey_bundle(requester, recipient_user_id)
        .map_err(|_| RnError::PrekeyAdapter("prekey bundle fetch failed"))?;
    let remaining_opk_count = response.remaining_opk_count;
    let merged = peer_identity_bundle
        .merge_prekey_bundle_response(&response, last_known_revision)
        .map_err(|_| {
            RnError::PrekeyAdapter("fetched prekey bundle does not match authenticated identity")
        })?;
    let peer = peer_bundle_from_verified_b5(&merged)?;
    let own_identity_secret = XSecret::from_bytes(*requester.x25519_secret.as_bytes());
    let session = initiate_and_persist_with_sealer(
        store,
        sealer,
        &own_identity_secret,
        requester.x25519_public.as_bytes(),
        &peer,
        caps,
        &merged.identity.mlkem768_identity_pub,
        context,
        params,
    )?;

    Ok(FirstContactRnSession {
        session,
        remaining_opk_count,
        accepted_identity_revision,
    })
}

/// Encrypt with a persisted OSL-RN session.
///
/// The gate is checked before any state load or crypto operation. When
/// enabled, the ordering is load, encrypt
/// (advancing the session), save the advanced session, then return the
/// wire. If saving fails, the wire is not returned.
pub fn send_rn(
    store: &RnSessionStore,
    peer_identity_x25519: &[u8; 32],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, RnError> {
    let sealer = keystore::select_best_sealer();
    send_rn_with_sealer(
        store,
        sealer.as_ref(),
        peer_identity_x25519,
        msg_type,
        plaintext,
    )
}

pub fn send_rn_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, RnError> {
    if !wire_in_enabled() {
        return Err(RnError::WireInDisabled);
    }

    send_rn_after_gate(store, sealer, peer_identity_x25519, msg_type, plaintext)
}

/// Encrypt with a persisted OSL-RN session, gated by [`crate::AppState`].
pub fn send_rn_for_state(
    state: &crate::AppState,
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, RnError> {
    if !app_state_wire_in_enabled(state) {
        return Err(RnError::WireInDisabled);
    }

    send_rn_after_gate(store, sealer, peer_identity_x25519, msg_type, plaintext)
}

fn send_rn_after_gate(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    msg_type: u8,
    plaintext: &[u8],
) -> Result<String, RnError> {
    let _writer_lock = store.acquire_writer_lock(peer_identity_x25519)?;
    let mut session = store
        .load_session_with_sealer(peer_identity_x25519, sealer)?
        .ok_or_else(|| RnError::Protocol("no OSL-RN session on file".into()))?;
    let wire =
        osl_ratchet_next::encrypt_rn(&mut session, msg_type, plaintext).map_err(RnError::from)?;
    store.save_session_with_sealer(peer_identity_x25519, &session, sealer)?;
    Ok(wire)
}

/// Decrypt with a persisted OSL-RN session.
///
/// The gate is checked before any state load or crypto operation. When
/// enabled, the receive-side ratchet advance
/// is persisted before the opened plaintext is returned.
pub fn receive_rn(
    store: &RnSessionStore,
    peer_identity_x25519: &[u8; 32],
    wire: &str,
) -> Result<Opened, RnError> {
    let sealer = keystore::select_best_sealer();
    receive_rn_with_sealer(store, sealer.as_ref(), peer_identity_x25519, wire)
}

pub fn receive_rn_with_sealer(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    wire: &str,
) -> Result<Opened, RnError> {
    if !wire_in_enabled() {
        return Err(RnError::WireInDisabled);
    }

    receive_rn_after_gate(store, sealer, peer_identity_x25519, wire)
}

/// Decrypt with a persisted OSL-RN session, gated by [`crate::AppState`].
pub fn receive_rn_for_state(
    state: &crate::AppState,
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    wire: &str,
) -> Result<Opened, RnError> {
    if !app_state_wire_in_enabled(state) {
        return Err(RnError::WireInDisabled);
    }

    receive_rn_after_gate(store, sealer, peer_identity_x25519, wire)
}

fn receive_rn_after_gate(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    wire: &str,
) -> Result<Opened, RnError> {
    let _writer_lock = store.acquire_writer_lock(peer_identity_x25519)?;
    let mut session = store
        .load_session_with_sealer(peer_identity_x25519, sealer)?
        .ok_or_else(|| RnError::Protocol("no OSL-RN session on file".into()))?;
    let opened = osl_ratchet_next::decrypt_rn(&mut session, wire).map_err(RnError::from)?;
    store.save_session_with_sealer(peer_identity_x25519, &session, sealer)?;
    Ok(opened)
}

// ---------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn unb64(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(s)
}

fn rn_opk_id_to_b5(id: u32) -> Result<u32, RnError> {
    id.checked_sub(1).ok_or(RnError::PrekeyAdapter(
        "RN no-OPK sentinel cannot be consumed as a B5 one-time prekey",
    ))
}

fn peek_bootstrap_one_time_prekey_id(wire: &str) -> Result<Option<u32>, RnError> {
    const RN_FLAG_BOOTSTRAP: u8 = 0x01;
    const RN_FLAG_RESERVED_MASK: u8 = 0xFE;
    const PREAMBLE_OPK_OFFSET: usize = 2 + 32 + 32;

    let body = wire
        .strip_prefix("DPC0::")
        .ok_or_else(|| RnError::Protocol("malformed OSL-RN wire prefix".into()))?;
    let raw = unb64(body).map_err(|e| RnError::Protocol(format!("wire base64: {e}")))?;
    if raw.first().copied() != Some(WIRE_VERSION_RN) {
        let got = raw.first().copied().unwrap_or_default();
        return Err(RnError::Protocol(format!(
            "wrong OSL-RN wire version {got}, expected {WIRE_VERSION_RN}"
        )));
    }
    let flags = *raw
        .get(1)
        .ok_or_else(|| RnError::Protocol("malformed OSL-RN wire flags".into()))?;
    if flags & RN_FLAG_RESERVED_MASK != 0 {
        return Err(RnError::Protocol("reserved OSL-RN flag bits set".into()));
    }
    if flags & RN_FLAG_BOOTSTRAP == 0 {
        return Ok(None);
    }
    let rest = raw
        .get(PREAMBLE_OPK_OFFSET..)
        .ok_or_else(|| RnError::Protocol("truncated OSL-RN bootstrap preamble".into()))?;
    Ok(Some(read_canonical_u32_varint(rest)?))
}

fn read_canonical_u32_varint(bytes: &[u8]) -> Result<u32, RnError> {
    let mut result: u64 = 0;
    for group in 0..5 {
        let byte = *bytes
            .get(group)
            .ok_or_else(|| RnError::Protocol("truncated OSL-RN OPK id".into()))?;
        let payload = u64::from(byte & 0x7f);
        result |= payload << (group * 7);
        if result > u64::from(u32::MAX) {
            return Err(RnError::Protocol("OSL-RN OPK id overflow".into()));
        }
        if byte & 0x80 == 0 {
            let value = result as u32;
            let canonical_len = if value < (1 << 7) {
                1
            } else if value < (1 << 14) {
                2
            } else if value < (1 << 21) {
                3
            } else if value < (1 << 28) {
                4
            } else {
                5
            };
            if group + 1 != canonical_len {
                return Err(RnError::Protocol("non-canonical OSL-RN OPK id".into()));
            }
            return Ok(value);
        }
    }
    Err(RnError::Protocol("overlong OSL-RN OPK id".into()))
}

/// Read a state file with a hard byte ceiling.
///
/// `Ok(None)` means the file is absent — the one routine degradation
/// (session: re-handshake; pin: `UNKNOWN`). Anything else, including a
/// file over the ceiling, is an error: a state file that cannot be read
/// correctly must never be indistinguishable from one that is not there,
/// because for the pin those two answers differ by an entire downgrade.
///
/// The size is checked against `metadata()` *before* reading, and the
/// read itself is `take`-limited so a file that grows between the stat
/// and the read still cannot allocate past the ceiling.
fn read_bounded(path: &Path, max: u64, what: &str) -> Result<Option<Vec<u8>>, RnError> {
    use std::io::Read as _;

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(RnError::Storage(format!("open {what}: {e}"))),
    };
    let len = file
        .metadata()
        .map_err(|e| RnError::Storage(format!("stat {what}: {e}")))?
        .len();
    if len > max {
        return Err(RnError::Storage(format!(
            "{what} file is {len} bytes, over the {max}-byte bound"
        )));
    }
    let mut buf = Vec::with_capacity(len as usize);
    file.take(max.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| RnError::Storage(format!("read {what}: {e}")))?;
    if buf.len() as u64 > max {
        return Err(RnError::Storage(format!(
            "{what} file grew past the {max}-byte bound while being read"
        )));
    }
    Ok(Some(buf))
}

fn looks_like_legacy_v4_session_blob(bytes: &[u8]) -> bool {
    serde_json::from_slice::<crypto::ratchet::RatchetStateOnDisk>(bytes).is_ok()
}

fn retire_legacy_v4_session_file(path: &Path) -> Result<(), RnError> {
    let mut retired = path.with_extension("session.legacy-v4-retired");
    for suffix in 1.. {
        if !retired.exists() {
            break;
        }
        retired = path.with_extension(format!("session.legacy-v4-retired-{suffix}"));
    }
    std::fs::rename(path, &retired)
        .map_err(|e| RnError::Storage(format!("retire legacy v4 session file: {e}")))
}

fn b5_opk_id_from_bootstrap_wire(wire: &str) -> Result<Option<u32>, RnError> {
    match peek_bootstrap_one_time_prekey_id(wire)? {
        None | Some(0) => Ok(None),
        Some(id) => Ok(Some(rn_opk_id_to_b5(id)?)),
    }
}

/// Write `bytes` to `path` atomically: temp file in the same directory,
/// fsync the file, rename over the target, fsync the directory.
///
/// A torn write must not be able to produce a *partially valid* session
/// blob — an importer that read half a state export could resurrect
/// consumed message keys, which is a nonce-reuse hazard
/// (`THREAT-MODEL.md` §5). Rename-based replacement makes the file
/// either entirely old or entirely new.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RnError> {
    use std::io::Write as _;

    static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let parent = path
        .parent()
        .ok_or_else(|| RnError::Storage("state path has no parent directory".into()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| RnError::Storage(format!("create state dir: {e}")))?;

    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("tmp.{}.{}", std::process::id(), sequence));
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|e| RnError::Storage(format!("create temp state file: {e}")))?;
        f.write_all(bytes)
            .map_err(|e| RnError::Storage(format!("write temp state file: {e}")))?;
        f.sync_all()
            .map_err(|e| RnError::Storage(format!("fsync temp state file: {e}")))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        // Leave no partial file behind on failure.
        let _ = std::fs::remove_file(&tmp);
        RnError::Storage(format!("rename state file: {e}"))
    })?;
    // Directory fsync so the rename itself is durable. Best-effort:
    // some filesystems refuse to open a directory for sync, and failing
    // the whole save over that would be worse than the weaker
    // durability guarantee.
    if let Ok(d) = std::fs::File::open(parent) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    // These tests intentionally assert the shipped compile-time RN fuse.
    #![allow(clippy::assertions_on_constants)]

    use super::*;
    use keystore::client::{
        PeerCapabilities, PrekeyBundleOpk, PrekeyBundleResponse, RN_CAP_WIRE_RN,
        RN_CAP_WIRE_RN_LIVE,
    };
    use keystore::sealer::{MemorySealer, NoOpSealer};
    use osl_ratchet_next::primitives::x25519_keypair;
    use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;
    use tempfile::TempDir;

    const CTX: &[u8] = b"ipc/tests/wire_rn/v1";

    // ---- B5 prekey supply adapters ----

    fn b5_identity(seed: u8, user_id: &str) -> keystore::Identity {
        keystore::identity_from_entropy([seed; 16], user_id.to_owned())
    }

    fn b5_response(
        identity: &keystore::Identity,
        prekeys: &keystore::PrekeyState,
        opk_index: Option<usize>,
    ) -> PrekeyBundleResponse {
        use base64::Engine as _;

        let opk = opk_index.map(|idx| {
            let opk = &prekeys.opk_pool[idx];
            PrekeyBundleOpk {
                id: opk.id,
                pub_b64: base64::engine::general_purpose::STANDARD.encode(opk.public),
            }
        });
        PrekeyBundleResponse {
            user_id: identity.user_id.clone(),
            ik_x25519_pub: base64::engine::general_purpose::STANDARD
                .encode(identity.x25519_public.as_bytes()),
            ik_ed25519_pub: base64::engine::general_purpose::STANDARD
                .encode(identity.ed25519_public.as_bytes()),
            ik_mlkem768_pub: base64::engine::general_purpose::STANDARD
                .encode(identity.mlkem_public_bytes),
            spk_pub: base64::engine::general_purpose::STANDARD.encode(prekeys.current_spk.public),
            spk_signature: base64::engine::general_purpose::STANDARD
                .encode(prekeys.current_spk.signature),
            spk_rotated_at: keystore::iso_8601_from_unix_seconds(
                prekeys.current_spk.rotated_at_unix_seconds,
            ),
            opk,
            remaining_opk_count: prekeys.opk_pool.len() as u32,
            ik_ratchet_initial_pub: None,
        }
    }

    fn b5_identity_bundle(
        identity: &keystore::Identity,
        capability_bundle: u32,
        revision: u64,
    ) -> keystore::identity_bundle::IdentityBundle {
        let mut bundle = keystore::identity_bundle::IdentityBundle {
            ed25519_identity_pub: *identity.ed25519_public.as_bytes(),
            x25519_identity_pub: *identity.x25519_public.as_bytes(),
            mlkem768_identity_pub: identity.mlkem_public_bytes,
            capability_bundle,
            revision,
            signature: [0u8; crypto::ed25519::SIGNATURE_SIZE],
        };
        let signature = crypto::ed25519::sign(&identity.ed25519_secret, &bundle.signed_bytes());
        bundle.signature = *signature.as_bytes();
        bundle
    }

    fn prekey_response_json(response: &PrekeyBundleResponse) -> Vec<u8> {
        let opk = response.opk.as_ref().map(|opk| {
            serde_json::json!({
                "id": opk.id,
                "pub_b64": &opk.pub_b64,
            })
        });
        serde_json::to_vec(&serde_json::json!({
            "user_id": &response.user_id,
            "ik_x25519_pub": &response.ik_x25519_pub,
            "ik_ed25519_pub": &response.ik_ed25519_pub,
            "ik_mlkem768_pub": &response.ik_mlkem768_pub,
            "spk_pub": &response.spk_pub,
            "spk_signature": &response.spk_signature,
            "spk_rotated_at": &response.spk_rotated_at,
            "opk": opk,
            "remaining_opk_count": response.remaining_opk_count,
            "ik_ratchet_initial_pub": &response.ik_ratchet_initial_pub,
        }))
        .expect("serialize response")
    }

    fn one_shot_prekey_server(body: Vec<u8>) -> (String, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).expect("read request");
            request.extend_from_slice(&buf[..n]);
            tx.send(request).expect("send request");
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).expect("write header");
            stream.write_all(&body).expect("write body");
        });
        (format!("http://{addr}"), rx)
    }

    #[test]
    fn rn_wire_in_flag_is_open() {
        assert!(
            RN_WIRE_IN_ENABLED,
            "the OSL-RN wire-in gate must stay open once its delivery preconditions land"
        );
    }

    #[test]
    fn b5_prekey_supply_adapts_to_an_rn_handshake_with_opk_zero_renumbered() {
        let bob_id = b5_identity(41, "bob-b5-rn");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 10);
        assert_eq!(
            bob_state.opk_pool[0].id, 0,
            "B5 starts its OPK pool at id zero"
        );
        let bob_response = b5_response(&bob_id, &bob_state, Some(0));
        let bob_peer = peer_bundle_from_b5(&bob_response).expect("peer bundle");
        let bob_local = local_prekeys_from_b5(&bob_id, &bob_state).expect("local prekeys");

        assert_eq!(
            bob_peer.one_time_prekey.as_ref().expect("peer opk").0,
            1,
            "B5 id zero must be remapped away from RN's no-OPK sentinel"
        );
        assert!(
            bob_local.one_time_prekeys.iter().any(|(id, _)| *id == 1),
            "local prekeys must use the same remapped RN OPK id"
        );
        assert!(
            !bob_local.one_time_prekeys.iter().any(|(id, _)| *id == 0),
            "RN local prekeys must never contain the no-OPK sentinel"
        );

        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let alice_id = b5_identity(42, "alice-b5-rn");
        let alice_secret = XSecret::from_bytes(*alice_id.x25519_secret.as_bytes());
        let mut rng = seeded_rng(41);
        let mut alice = initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_secret,
            alice_id.x25519_public.as_bytes(),
            &bob_peer,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_id.mlkem_public_bytes,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");
        assert!(
            store
                .load_pin(bob_peer.identity.as_bytes())
                .expect("load pin")
                .is_pinned_to_rn(),
            "verified B5 initiation must pin the peer to RN"
        );

        let wire = alice.encrypt(0, b"from b5", &mut rng).expect("encrypt");

        let (_d2, bob_store) = fresh_store();
        let (_bob, opened) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_local,
            bob_id.x25519_public.as_bytes(),
            &bob_id.mlkem_public_bytes,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept");
        assert_eq!(opened.plaintext, b"from b5");
        assert!(
            bob_store
                .load_pin(alice_id.x25519_public.as_bytes())
                .expect("load pin")
                .is_pinned_to_rn(),
            "accepted B5-backed RN bootstrap must pin the authenticated initiator"
        );
    }

    #[test]
    fn rn_handshake_consumes_local_prekey_state_opk_once() {
        let alice_id = b5_identity(55, "alice-b64");
        let bob_id = b5_identity(56, "bob-b64");
        let mut bob_state =
            keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 80);
        let consumed_b5_id = bob_state.opk_pool[0].id;
        let bob_response = b5_response(&bob_id, &bob_state, Some(0));
        let bob_peer = peer_bundle_from_b5(&bob_response).expect("peer bundle");
        assert_eq!(
            bob_peer.one_time_prekey.as_ref().expect("peer opk").0,
            consumed_b5_id + 1,
            "RN wire id must be the B5 OPK id shifted away from zero"
        );

        let (_alice_dir, alice_store) = fresh_store();
        let (bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let alice_secret = XSecret::from_bytes(*alice_id.x25519_secret.as_bytes());
        let mut alice = initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_secret,
            alice_id.x25519_public.as_bytes(),
            &bob_peer,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_id.mlkem_public_bytes,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");
        let mut rng = seeded_rng(64);
        let wire = alice.encrypt(64, b"consume once", &mut rng).expect("send");

        let before = bob_state.opk_pool.len();
        let (_bob, opened) = accept_b5_prekeys_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_id,
            &mut bob_state,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept and consume");
        assert_eq!(opened.plaintext, b"consume once");
        assert_eq!(bob_state.opk_pool.len(), before - 1);
        assert!(
            !bob_state
                .opk_pool
                .iter()
                .any(|opk| opk.id == consumed_b5_id),
            "the B5 OPK named by the RN bootstrap must be removed"
        );

        let reloaded_bob_store = RnSessionStore::new(bob_dir.path().join("rn"));
        assert!(
            accept_b5_prekeys_and_persist_with_sealer(
                &reloaded_bob_store,
                &sealer,
                &bob_id,
                &mut bob_state,
                &wire,
                CTX,
                SessionParams::default(),
            )
            .is_err(),
            "the same bootstrap must not be acceptable after its OPK is consumed"
        );
    }

    #[test]
    fn consume_opk_is_wired_to_live_fetched_prekey_bundle() {
        let alice_id = b5_identity(57, "alice-b83");
        let bob_id = b5_identity(58, "bob-b83");
        let mut bob_state =
            keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 90);
        let fetched_opk_id = bob_state.opk_pool[0].id;
        let bob_identity_bundle = b5_identity_bundle(&bob_id, RN_CAP_WIRE_RN, 11);
        let response = b5_response(&bob_id, &bob_state, Some(0));
        assert_eq!(response.opk.as_ref().expect("opk").id, fetched_opk_id);
        let (base_url, _rx) = one_shot_prekey_server(prekey_response_json(&response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();

        let mut first_contact = fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &alice_store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &bob_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        )
        .expect("live-fetched first contact");
        let mut rng = seeded_rng(83);
        let wire = first_contact
            .session
            .encrypt(83, b"live fetched opk", &mut rng)
            .expect("bootstrap wire");

        let before_ids: Vec<u32> = bob_state.opk_pool.iter().map(|opk| opk.id).collect();
        let (_bob, opened) = accept_b5_prekeys_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_id,
            &mut bob_state,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept consumes fetched opk");
        assert_eq!(opened.msg_type, 83);
        assert_eq!(opened.plaintext, b"live fetched opk");
        assert!(
            before_ids.contains(&fetched_opk_id),
            "test fixture must start with the fetched OPK live locally"
        );
        assert!(
            !bob_state
                .opk_pool
                .iter()
                .any(|opk| opk.id == fetched_opk_id),
            "accepting a live-fetched RN bundle must consume that same B5 OPK"
        );
    }

    #[test]
    fn peer_b5_bundle_with_an_invalid_spk_signature_is_refused() {
        use base64::Engine as _;

        let bob_id = b5_identity(43, "bob-sensitive-handle");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 20);
        let mut response = b5_response(&bob_id, &bob_state, Some(0));
        let mut sig = base64::engine::general_purpose::STANDARD
            .decode(&response.spk_signature)
            .expect("decode signature");
        sig[0] ^= 0x01;
        response.spk_signature = base64::engine::general_purpose::STANDARD.encode(sig);

        let err = peer_bundle_from_b5(&response).expect_err("invalid signature must refuse");
        assert!(matches!(
            &err,
            RnError::PrekeyAdapter("peer signed prekey signature is invalid")
        ));
        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(!display.contains("bob-sensitive-handle"));
        assert!(!debug.contains("bob-sensitive-handle"));
        assert!(!display.contains(&response.ik_x25519_pub));
        assert!(!debug.contains(&response.ik_x25519_pub));
    }

    #[test]
    fn first_contact_fetches_prekey_bundle_before_initiating_rn_session() {
        let alice_id = b5_identity(46, "alice-fetch-rn");
        let bob_id = b5_identity(47, "bob-fetch-rn");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 50);
        let bob_identity_bundle = b5_identity_bundle(&bob_id, RN_CAP_WIRE_RN, 7);
        let response = b5_response(&bob_id, &bob_state, Some(0));
        let expected_remaining = response.remaining_opk_count;
        let (base_url, rx) = one_shot_prekey_server(prekey_response_json(&response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();

        let first_contact = fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &bob_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        )
        .expect("first contact");

        assert_eq!(first_contact.remaining_opk_count, expected_remaining);
        assert_eq!(first_contact.accepted_identity_revision, 7);
        assert!(
            store
                .load_session_with_sealer(bob_id.x25519_public.as_bytes(), &sealer)
                .expect("load session")
                .is_some(),
            "successful first contact must persist the RN session"
        );
        assert!(
            store
                .load_pin(bob_id.x25519_public.as_bytes())
                .expect("load pin")
                .is_pinned_to_rn(),
            "verified RN capability must raise the peer pin"
        );

        let request = String::from_utf8(rx.recv().expect("request")).expect("utf8");
        assert!(
            request.starts_with("GET /v1/prekey-bundle/bob-fetch-rn?"),
            "first contact must fetch the recipient's prekey bundle"
        );
    }

    #[test]
    fn consume_opk_is_wired_to_live_fetched_prekey_bundle_second_path() {
        let alice_id = b5_identity(83, "alice-live-fetched-rn");
        let bob_id = b5_identity(84, "bob-live-fetched-rn");
        let mut bob_state =
            keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 83);
        let fetched_opk_id = bob_state.opk_pool[0].id;
        let bob_identity_bundle = b5_identity_bundle(&bob_id, RN_CAP_WIRE_RN, 83);
        let response = b5_response(&bob_id, &bob_state, Some(0));
        let (base_url, rx) = one_shot_prekey_server(prekey_response_json(&response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();

        let mut first_contact = fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &alice_store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &bob_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        )
        .expect("first contact");
        let request = String::from_utf8(rx.recv().expect("request")).expect("utf8");
        assert!(
            request.starts_with("GET /v1/prekey-bundle/bob-live-fetched-rn?"),
            "first contact must use the live fetched B5 prekey bundle"
        );

        let mut rng = seeded_rng(0xB83);
        let wire = first_contact
            .session
            .encrypt(83, b"live fetched opk", &mut rng)
            .expect("bootstrap wire");
        let (_session, opened) = accept_b5_prekey_state_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_id,
            &mut bob_state,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept live-fetched bootstrap");

        assert_eq!(opened.msg_type, 83);
        assert_eq!(opened.plaintext, b"live fetched opk");
        assert!(
            !bob_state.consume_opk(fetched_opk_id),
            "the OPK selected by the live fetched bundle must already be consumed"
        );
    }

    #[test]
    fn negotiation_digest_uses_real_capability_floor_end_to_end() {
        let alice_id = b5_identity(53, "alice-b30");
        let bob_id = b5_identity(54, "bob-b30");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 70);
        let bob_identity_bundle = b5_identity_bundle(&bob_id, RN_CAP_WIRE_RN, 9);
        let response = b5_response(&bob_id, &bob_state, Some(0));
        let (base_url, _rx) = one_shot_prekey_server(prekey_response_json(&response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();

        let mut first_contact = fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &alice_store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &bob_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        )
        .expect("first contact uses authenticated RN capability");

        let pin = alice_store
            .load_pin(bob_id.x25519_public.as_bytes())
            .expect("load pin");
        assert_eq!(
            select_wire_version(
                &pin,
                PeerCapabilities::Verified(bob_identity_bundle.capability_bundle),
                RnPolicy::Opportunistic,
            )
            .expect("select"),
            SelectedVersion::Rn
        );
        assert_eq!(
            pin.min_wire_version(),
            WIRE_VERSION_RN,
            "the selected first-contact floor must be the real RN wire floor"
        );

        let mut direct = Negotiation::for_rn(
            &bob_identity_bundle.x25519_identity_pub,
            &bob_identity_bundle.mlkem768_identity_pub,
            alice_id.x25519_public.as_bytes(),
            CTX,
        );
        direct.min_acceptable_version = pin.min_wire_version();
        assert_eq!(
            initiator_binding(
                &bob_identity_bundle.x25519_identity_pub,
                &bob_identity_bundle.mlkem768_identity_pub,
                alice_id.x25519_public.as_bytes(),
                CTX,
            )
            .expect("ipc binding"),
            direct.digest().expect("direct binding"),
            "IPC must use the ratchet negotiation digest with the selected capability floor"
        );

        let mut rng = seeded_rng(0xB30);
        let wire = first_contact
            .session
            .encrypt(21, b"b30 bound bootstrap", &mut rng)
            .expect("encrypt bootstrap");
        let bob_local = local_prekeys_from_b5(&bob_id, &bob_state).expect("bob local prekeys");

        assert!(
            matches!(
                Session::accept(&bob_local, &wire, SessionParams::default(), &mut rng),
                Err(osl_ratchet_next::Error::AuthFailed)
            ),
            "a responder that omits the negotiation digest must fail closed"
        );

        let (_bob_session, opened) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_local,
            bob_id.x25519_public.as_bytes(),
            &bob_id.mlkem_public_bytes,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("bound responder accepts");
        assert_eq!(opened.msg_type, 21);
        assert_eq!(opened.plaintext, b"b30 bound bootstrap");
        assert!(bob_store
            .load_pin(alice_id.x25519_public.as_bytes())
            .expect("load responder pin")
            .is_pinned_to_rn());
    }

    #[test]
    fn first_contact_refuses_without_authenticated_rn_capability_before_fetching() {
        let alice_id = b5_identity(48, "alice-no-rn");
        let bob_id = b5_identity(49, "bob-no-rn");
        let bob_identity_bundle = b5_identity_bundle(&bob_id, 0, 1);
        let client = keystore::KeyServerClient::new("http://127.0.0.1:9").expect("client");
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();

        assert!(matches!(
            fetch_prekey_bundle_and_initiate_first_contact(
                &client,
                &store,
                &sealer,
                &alice_id,
                &bob_id.user_id,
                &bob_identity_bundle,
                &bob_id.ed25519_public,
                None,
                RnPolicy::Opportunistic,
                CTX,
                SessionParams::default(),
            ),
            Err(RnError::RnRequiredButUnsupported)
        ));
        assert!(
            store
                .load_session_with_sealer(bob_id.x25519_public.as_bytes(), &sealer)
                .expect("load session")
                .is_none(),
            "a refused first contact must not persist session state"
        );
    }

    #[test]
    fn first_contact_refuses_substituted_fetched_identity_fields() {
        let alice_id = b5_identity(50, "alice-substitution");
        let bob_id = b5_identity(51, "bob-sensitive-handle");
        let attacker_id = b5_identity(52, "attacker-substitute");
        let attacker_state =
            keystore::PrekeyState::new(&attacker_id, keystore::PrekeyConfig::default(), 60);
        let bob_identity_bundle = b5_identity_bundle(&bob_id, RN_CAP_WIRE_RN, 3);
        let substituted_response = b5_response(&attacker_id, &attacker_state, Some(0));
        let (base_url, _rx) = one_shot_prekey_server(prekey_response_json(&substituted_response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();

        let err = match fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &bob_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        ) {
            Ok(_) => panic!("substituted prekey identity must refuse"),
            Err(err) => err,
        };

        assert!(matches!(
            &err,
            RnError::PrekeyAdapter("fetched prekey bundle does not match authenticated identity")
        ));
        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(!display.contains("bob-sensitive-handle"));
        assert!(!debug.contains("bob-sensitive-handle"));
        assert!(!display.contains(&substituted_response.ik_x25519_pub));
        assert!(!debug.contains(&substituted_response.ik_x25519_pub));
        assert!(
            store
                .load_session_with_sealer(bob_id.x25519_public.as_bytes(), &sealer)
                .expect("load session")
                .is_none(),
            "a refused substitution must not persist session state"
        );
    }

    #[test]
    fn local_b5_prekeys_with_an_unbound_opk_are_refused() {
        let id = b5_identity(44, "local-b5-rn");
        let mut state = keystore::PrekeyState::new(&id, keystore::PrekeyConfig::default(), 30);
        state.opk_pool[0].public[0] ^= 0x01;

        assert!(matches!(
            local_prekeys_from_b5(&id, &state),
            Err(RnError::PrekeyAdapter(
                "local one-time prekey public key does not match the secret key"
            ))
        ));
    }

    #[test]
    fn b5_opk_id_overflow_is_refused_before_entering_rn() {
        let bob_id = b5_identity(45, "bob-opk-overflow");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 40);
        let mut response = b5_response(&bob_id, &bob_state, Some(0));
        response.opk.as_mut().expect("opk").id = u32::MAX;

        assert!(matches!(
            peer_bundle_from_b5(&response),
            Err(RnError::PrekeyAdapter(
                "one-time prekey id cannot be represented on RN wire"
            ))
        ));
    }

    #[test]
    fn rn_handshake_consumes_local_prekey_state_opk_once_second_path() {
        let alice_id = b5_identity(64, "alice-consume-opk-once");
        let bob_id = b5_identity(65, "bob-consume-opk-once");
        let mut bob_state =
            keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 64);
        let consumed_opk_id = bob_state.opk_pool[0].id;
        let bob_response = b5_response(&bob_id, &bob_state, Some(0));
        let bob_peer = peer_bundle_from_b5(&bob_response).expect("peer bundle");
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let alice_secret = XSecret::from_bytes(*alice_id.x25519_secret.as_bytes());
        let mut alice = initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_secret,
            alice_id.x25519_public.as_bytes(),
            &bob_peer,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_id.mlkem_public_bytes,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");
        let mut rng = seeded_rng(0xB64);
        let wire = alice
            .encrypt(64, b"consume exactly once", &mut rng)
            .expect("bootstrap wire");

        let (_session, opened) = accept_b5_prekey_state_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_id,
            &mut bob_state,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("first accept consumes OPK");

        assert_eq!(opened.msg_type, 64);
        assert_eq!(opened.plaintext, b"consume exactly once");
        assert!(
            !bob_state.consume_opk(consumed_opk_id),
            "the authenticated bootstrap must consume its local B5 OPK"
        );
        assert!(
            accept_b5_prekey_state_and_persist_with_sealer(
                &bob_store,
                &sealer,
                &bob_id,
                &mut bob_state,
                &wire,
                CTX,
                SessionParams::default(),
            )
            .is_err(),
            "replaying the same bootstrap must fail once the one-time key is gone"
        );
        assert!(
            !bob_state.consume_opk(consumed_opk_id),
            "a replay must not recreate or consume the OPK a second time"
        );
    }

    // ---- version selection / downgrade ----

    #[test]
    fn ratchet_policy_decision_replaces_scattered_wire_flags() {
        fn wire_generation(decision: crate::commands::RatchetPolicyDecision) -> u8 {
            match decision {
                crate::commands::RatchetPolicyDecision::LegacyV3 => 3,
                crate::commands::RatchetPolicyDecision::LegacyV4Dm => 4,
                crate::commands::RatchetPolicyDecision::SenderKeysV5 => 5,
            }
        }

        let decisions = [
            crate::commands::RatchetPolicyDecision::LegacyV3,
            crate::commands::RatchetPolicyDecision::LegacyV4Dm,
            crate::commands::RatchetPolicyDecision::SenderKeysV5,
        ];
        assert_eq!(
            decisions.map(wire_generation),
            [3, 4, 5],
            "each ratchet send path must be represented by one typed policy decision"
        );
        assert_ne!(
            crate::commands::RatchetPolicyDecision::LegacyV3,
            crate::commands::RatchetPolicyDecision::LegacyV4Dm,
            "the typed decision must keep legacy v3 and retained v4 DM distinct"
        );
        assert_ne!(
            crate::commands::RatchetPolicyDecision::LegacyV4Dm,
            crate::commands::RatchetPolicyDecision::SenderKeysV5,
            "the typed decision must keep retained v4 DM and sender-key v5 distinct"
        );
    }

    #[test]
    fn an_unpinned_peer_without_rn_support_uses_v3() {
        assert_eq!(
            select_wire_version(
                &RnPeerPin::UNKNOWN,
                PeerCapabilities::Absent,
                RnPolicy::Opportunistic
            )
            .expect("select"),
            SelectedVersion::LegacyV3
        );
    }

    #[test]
    fn an_unpinned_peer_with_live_rn_support_uses_rn() {
        assert_eq!(
            select_wire_version(
                &RnPeerPin::UNKNOWN,
                PeerCapabilities::Verified(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE),
                RnPolicy::Opportunistic
            )
            .expect("select"),
            SelectedVersion::Rn
        );
    }

    /// The downgrade attempt: a pinned peer later presents an
    /// authenticated identity bundle whose capability bitmap no longer
    /// advertises OSL-RN. First contact must refuse at the sticky pin
    /// before fetching prekeys or falling back to v=3.
    #[test]
    fn a_pinned_peer_can_never_be_downgraded_to_v3() {
        let alice_id = b5_identity(82, "alice-pinned-downgrade");
        let bob_id = b5_identity(83, "bob-pinned-downgrade");
        let bob_state = keystore::PrekeyState::new(&bob_id, keystore::PrekeyConfig::default(), 82);
        let response = b5_response(&bob_id, &bob_state, Some(0));
        let (base_url, rx) = one_shot_prekey_server(prekey_response_json(&response));
        let client = keystore::KeyServerClient::new(base_url).expect("client");
        let (_dir, store) = fresh_store();
        let sealer = MemorySealer::new();
        let peer = *bob_id.x25519_public.as_bytes();

        store.raise_pin_to_rn(&peer).expect("seed pinned peer");
        let pin = store.load_pin(&peer).expect("load seeded pin");
        assert!(pin.is_pinned_to_rn(), "test fixture must start pinned");

        let downgraded_identity_bundle = b5_identity_bundle(&bob_id, 0, 82);
        let err = match fetch_prekey_bundle_and_initiate_first_contact(
            &client,
            &store,
            &sealer,
            &alice_id,
            &bob_id.user_id,
            &downgraded_identity_bundle,
            &bob_id.ed25519_public,
            None,
            RnPolicy::Opportunistic,
            CTX,
            SessionParams::default(),
        ) {
            Ok(_) => panic!("pinned peer with stripped RN capability must refuse"),
            Err(err) => err,
        };

        assert!(matches!(err, RnError::PinnedToRn));
        assert!(matches!(
            select_wire_version(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic),
            Err(RnError::PinnedToRn)
        ));
        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "downgraded capabilities must be refused before any prekey fetch"
        );
        assert!(
            store
                .load_session_with_sealer(&peer, &sealer)
                .expect("load session")
                .is_none(),
            "a refused downgrade must not persist an RN session"
        );
        assert!(
            store.load_pin(&peer).expect("reload pin").is_pinned_to_rn(),
            "refusing the downgrade must not lower the existing RN pin"
        );
    }

    #[test]
    fn ratchet_policy_decision_selects_rn_or_legacy_by_pin() {
        let mut pinned = RnPeerPin::UNKNOWN;
        pinned.raise_to_rn();

        let rn: RatchetPolicyDecision = select_wire_version(
            &RnPeerPin::UNKNOWN,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE),
            RnPolicy::Opportunistic,
        )
        .expect("verified RN selects RN");
        assert_eq!(rn, RatchetPolicyDecision::Rn);

        let legacy: RatchetPolicyDecision = select_wire_version(
            &RnPeerPin::UNKNOWN,
            PeerCapabilities::Absent,
            RnPolicy::Opportunistic,
        )
        .expect("absent capabilities select legacy only while unpinned");
        assert_eq!(legacy, RatchetPolicyDecision::LegacyV3);

        assert!(
            matches!(
                select_wire_version(&pinned, PeerCapabilities::Absent, RnPolicy::Opportunistic),
                Err(RnError::PinnedToRn)
            ),
            "a single decision enum must refuse the pinned downgrade instead of exposing a legacy flag"
        );
    }

    #[test]
    fn required_policy_refuses_a_peer_without_rn_support() {
        assert!(matches!(
            select_wire_version(
                &RnPeerPin::UNKNOWN,
                PeerCapabilities::Absent,
                RnPolicy::Required
            ),
            Err(RnError::RnRequiredButUnsupported)
        ));
    }

    #[test]
    fn unverified_capabilities_do_not_enable_rn_or_downgrade_pinned_peer() {
        let mut pinned = RnPeerPin::UNKNOWN;
        pinned.raise_to_rn();

        assert!(
            matches!(
                select_wire_version(
                    &pinned,
                    PeerCapabilities::Unverified,
                    RnPolicy::Opportunistic
                ),
                Err(RnError::PinnedToRn)
            ),
            "Unverified capabilities on a pinned peer must fail closed"
        );
        assert_eq!(
            select_wire_version(
                &RnPeerPin::UNKNOWN,
                PeerCapabilities::Unverified,
                RnPolicy::Opportunistic
            )
            .expect("select unpinned"),
            SelectedVersion::LegacyV3,
            "Unverified capabilities on an unpinned peer must resolve to legacy"
        );
    }

    #[test]
    fn unverified_raised_bitmap_cannot_promote_unpinned_peer_to_rn() {
        let caps = PeerCapabilities::Unverified;
        assert_eq!(
            caps.bitmap(),
            0,
            "Unverified capabilities must erase the advertised bitmap"
        );
        assert_eq!(
            select_wire_version(&RnPeerPin::UNKNOWN, caps, RnPolicy::Opportunistic)
                .expect("select opportunistic"),
            SelectedVersion::LegacyV3,
            "an unverified raised bitmap must not select OSL-RN"
        );
        assert!(
            matches!(
                select_wire_version(&RnPeerPin::UNKNOWN, caps, RnPolicy::Required),
                Err(RnError::RnRequiredButUnsupported)
            ),
            "an unverified raised bitmap must not satisfy Required policy"
        );
    }

    #[test]
    fn verified_zero_bitmap_does_not_read_as_capable() {
        let caps = PeerCapabilities::Verified(0);
        assert!(!caps.supports_rn(), "Verified(0) must not support OSL-RN");
        assert_eq!(
            select_wire_version(&RnPeerPin::UNKNOWN, caps, RnPolicy::Opportunistic)
                .expect("select opportunistic"),
            SelectedVersion::LegacyV3,
            "Verified(0) must not select OSL-RN opportunistically"
        );
        assert!(
            matches!(
                select_wire_version(&RnPeerPin::UNKNOWN, caps, RnPolicy::Required),
                Err(RnError::RnRequiredButUnsupported)
            ),
            "Verified(0) must not satisfy Required policy"
        );

        let mut pinned = RnPeerPin::UNKNOWN;
        pinned.raise_to_rn();
        assert!(
            matches!(
                select_wire_version(&pinned, caps, RnPolicy::Opportunistic),
                Err(RnError::PinnedToRn)
            ),
            "Verified(0) on a pinned peer must fail closed"
        );
    }

    #[test]
    fn t19_t24_selection_requires_live_bit_but_pin_keeps_bit_zero_semantics() {
        let mut raised = RnPeerPin::UNKNOWN;
        raised.raise_to_rn();

        for (pin_name, pin) in [("UNKNOWN", RnPeerPin::UNKNOWN), ("raised", raised)] {
            for (caps_name, caps) in [
                ("Absent", PeerCapabilities::Absent),
                ("Unverified", PeerCapabilities::Unverified),
                ("Verified(bit0)", PeerCapabilities::Verified(RN_CAP_WIRE_RN)),
                (
                    "Verified(bit0+bit1)",
                    PeerCapabilities::Verified(RN_CAP_WIRE_RN | RN_CAP_WIRE_RN_LIVE),
                ),
            ] {
                for policy in [RnPolicy::Opportunistic, RnPolicy::Required] {
                    let got = select_wire_version(&pin, caps, policy);
                    if pin.is_pinned_to_rn() {
                        assert!(
                            !matches!(&got, Ok(SelectedVersion::LegacyV3)),
                            "pinned peer selected LegacyV3 for caps={caps_name} policy={policy:?}"
                        );
                    }
                    match (pin.is_pinned_to_rn(), caps.supports_rn(), caps.supports_rn_live(), policy) {
                        // Pins deliberately retain their original bit-0 meaning. A peer
                        // already pinned by an authenticated RN session may keep using RN;
                        // stripping bit 1 must never make it silently downgrade.
                        (true, true, _, _) => assert!(
                            matches!(&got, Ok(SelectedVersion::Rn)),
                            "expected Rn for pin={pin_name} caps={caps_name} policy={policy:?}, got {got:?}"
                        ),
                        (true, false, _, _) => assert!(
                            matches!(&got, Err(RnError::PinnedToRn)),
                            "expected PinnedToRn for pin={pin_name} caps={caps_name} policy={policy:?}, got {got:?}"
                        ),
                        (false, _, true, _) => assert!(
                            matches!(&got, Ok(SelectedVersion::Rn)),
                            "expected Rn for pin={pin_name} caps={caps_name} policy={policy:?}, got {got:?}"
                        ),
                        (false, _, false, RnPolicy::Required) => assert!(
                            matches!(&got, Err(RnError::RnRequiredButUnsupported)),
                            "expected RnRequiredButUnsupported for pin={pin_name} caps={caps_name} policy={policy:?}, got {got:?}"
                        ),
                        (false, _, false, RnPolicy::Opportunistic) => assert!(
                            matches!(&got, Ok(SelectedVersion::LegacyV3)),
                            "expected LegacyV3 for pin={pin_name} caps={caps_name} policy={policy:?}, got {got:?}"
                        ),
                    }
                }
            }
        }
    }

    #[test]
    fn the_pin_is_monotone() {
        let mut pin = RnPeerPin::UNKNOWN;
        assert!(!pin.is_pinned_to_rn());
        pin.raise_to_rn();
        assert!(pin.is_pinned_to_rn());
        // Raising again is idempotent and cannot lower.
        pin.raise_to_rn();
        assert_eq!(pin.min_wire_version(), WIRE_VERSION_RN);

        let (_d, store) = fresh_store();
        let peer = [94u8; 32];
        let old_v3_pin = serde_json::to_vec(&serde_json::json!({
            "version": PIN_BLOB_VERSION,
            "pin": RnPeerPin::UNKNOWN,
        }))
        .expect("serialize old pin");
        store.raise_pin_to_rn(&peer).expect("persist raised pin");
        assert!(store
            .load_pin(&peer)
            .expect("load raised pin")
            .is_pinned_to_rn());

        std::fs::write(store.pin_path(&peer), old_v3_pin).expect("replay old pin file");
        let replay = store.load_pin(&peer);
        assert!(
            matches!(&replay, Err(RnError::Storage(message)) if message.contains("downgrade floor")),
            "replaying an older valid v3 pin file must be refused, got {replay:?}"
        );
        assert!(
            matches!(
                select_wire_version(
                    &RnPeerPin {
                        min_wire_version: WIRE_VERSION_RN,
                    },
                    PeerCapabilities::Absent,
                    RnPolicy::Opportunistic,
                ),
                Err(RnError::PinnedToRn)
            ),
            "the only permitted downgrade outcome after an RN pin is refusal"
        );
    }

    // ---- persistence ----

    fn fresh_store() -> (TempDir, RnSessionStore) {
        let dir = TempDir::new().expect("tempdir");
        let store = RnSessionStore::new(dir.path().join("rn"));
        (dir, store)
    }

    #[test]
    fn rn_session_store_for_config_dir_unifies_send_and_receive_state() {
        let config_dir = TempDir::new().expect("config dir");
        let send_store =
            RnSessionStore::for_config_dir(config_dir.path()).expect("send store from config dir");
        let receive_store = RnSessionStore::for_config_dir(config_dir.path())
            .expect("receive store from config dir");
        assert_eq!(
            send_store.dir, receive_store.dir,
            "send and receive must resolve the same RN session directory"
        );
        assert_eq!(send_store.dir, config_dir.path().join(RN_SESSION_DIR));

        let mut rng = seeded_rng(0xA2);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (identity, _) = x25519_keypair(&mut rng);
        let session = Session::initiate(&identity, &bundle, SessionParams::default(), &mut rng)
            .expect("initiate session");
        let peer = *bundle.identity.as_bytes();
        let sealer = MemorySealer::new();

        send_store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("send helper persists session");
        assert!(
            receive_store
                .load_session_with_sealer(&peer, &sealer)
                .expect("receive helper loads session")
                .is_some(),
            "a session written by send must be readable by receive"
        );
    }

    #[test]
    fn rn_session_store_for_config_dir_migrates_legacy_rn_directory() {
        let config_dir = TempDir::new().expect("config dir");
        let legacy_store = RnSessionStore::new(config_dir.path().join("rn"));
        let mut rng = seeded_rng(0xA3);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (identity, _) = x25519_keypair(&mut rng);
        let session = Session::initiate(&identity, &bundle, SessionParams::default(), &mut rng)
            .expect("initiate session");
        let peer = *bundle.identity.as_bytes();
        let sealer = MemorySealer::new();
        legacy_store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("write legacy session");

        let migrated =
            RnSessionStore::for_config_dir(config_dir.path()).expect("migrate legacy directory");
        assert_eq!(migrated.dir, config_dir.path().join(RN_SESSION_DIR));
        assert!(!config_dir.path().join("rn").exists());
        assert!(
            migrated
                .load_session_with_sealer(&peer, &sealer)
                .expect("load migrated session")
                .is_some(),
            "migration must retain legacy session state"
        );
    }

    #[test]
    fn rn_session_store_debug_does_not_print_its_directory() {
        let (_d, store) = fresh_store();
        let rendered = format!("{store:?}");
        assert!(rendered.contains("RnSessionStore"));
        assert!(
            !rendered.contains(store.dir.to_string_lossy().as_ref()),
            "debug output must not include local account storage paths"
        );
    }

    #[test]
    fn rn_session_writer_lock_child_process() {
        let Ok(dir) = std::env::var("OSL_RN_WRITER_LOCK_CHILD_DIR") else {
            return;
        };
        let store = RnSessionStore::new(dir);
        let peer = [0xc5; 32];
        assert!(matches!(
            store.acquire_writer_lock(&peer),
            Err(RnError::WriterBusy)
        ));
    }

    #[test]
    fn rn_session_writer_lock_excludes_threads_processes_and_stale_sends() {
        use std::process::Command;
        use std::sync::{Arc, Barrier};

        let (_dir, store) = fresh_store();
        let peer = [0xc5; 32];
        let held_lock = store
            .acquire_writer_lock(&peer)
            .expect("first writer acquires lock");

        // Two contenders begin together while another writer owns the peer.
        // They must both fail, rather than load stale state and retry later.
        let barrier = Arc::new(Barrier::new(3));
        let contenders = (0..2)
            .map(|_| {
                let store = store.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    store.acquire_writer_lock(&peer).map(|_| ())
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        for contender in contenders {
            assert!(matches!(
                contender.join().expect("writer contender thread"),
                Err(RnError::WriterBusy)
            ));
        }

        // A separately spawned test process sees the same filesystem lock.
        let status = Command::new(std::env::current_exe().expect("test executable"))
            .arg("--exact")
            .arg("wire_rn::tests::rn_session_writer_lock_child_process")
            .arg("--nocapture")
            .env("OSL_RN_WRITER_LOCK_CHILD_DIR", &store.dir)
            .status()
            .expect("spawn writer-lock child process");
        assert!(status.success(), "child process must receive WriterBusy");

        drop(held_lock);

        let state = crate::AppState::new();
        state.set_rn_wire_in_enabled(true);
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(0xc5);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (identity, _) = x25519_keypair(&mut rng);
        let session = Session::initiate(&identity, &bundle, SessionParams::default(), &mut rng)
            .expect("initiate");
        let session_peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&session_peer, &session, &sealer)
            .expect("seed session");

        let held_lock = store
            .acquire_writer_lock(&session_peer)
            .expect("hold lock for send");
        assert!(matches!(
            send_rn_for_state(
                &state,
                &store,
                &sealer,
                &session_peer,
                7,
                b"must not seal while busy"
            ),
            Err(RnError::WriterBusy)
        ));
        drop(held_lock);

        assert!(
            send_rn_for_state(
                &state,
                &store,
                &sealer,
                &session_peer,
                7,
                b"fresh writer after release"
            )
            .is_ok(),
            "a new writer may only send after it acquires fresh state"
        );
    }

    fn with_wire_in_enabled_for_test<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
        struct Reset(Option<bool>);

        impl Drop for Reset {
            fn drop(&mut self) {
                RN_WIRE_IN_TEST_OVERRIDE.with(|slot| slot.set(self.0));
            }
        }

        let previous = RN_WIRE_IN_TEST_OVERRIDE.with(|slot| {
            let previous = slot.get();
            slot.set(Some(enabled));
            previous
        });
        let _reset = Reset(previous);
        f()
    }

    #[derive(Debug, PartialEq, Eq)]
    struct FileSnapshot(Vec<(String, Vec<u8>)>);

    fn snapshot_files(dir: &Path) -> FileSnapshot {
        fn visit(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
                Err(e) => panic!("read snapshot dir: {e}"),
            };
            for entry in entries {
                let entry = entry.expect("snapshot entry");
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, out);
                } else {
                    let rel = path
                        .strip_prefix(root)
                        .expect("snapshot path under root")
                        .to_string_lossy()
                        .into_owned();
                    let bytes = std::fs::read(&path).expect("snapshot file");
                    out.push((rel, bytes));
                }
            }
        }

        let mut out = Vec::new();
        visit(dir, dir, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        FileSnapshot(out)
    }

    struct SealFailure<'a> {
        loader: &'a MemorySealer,
    }

    impl keystore::sealer::Sealer for SealFailure<'_> {
        fn method_label(&self) -> &'static str {
            keystore::sealer::METHOD_MEMORY
        }

        fn is_tpm_backed(&self) -> bool {
            false
        }

        fn requires_insecure_banner(&self) -> bool {
            false
        }

        fn seal(&self, _plaintext: &[u8]) -> keystore::sealer::Result<Vec<u8>> {
            Err(keystore::sealer::SealerError::Malformed(
                "forced save failure".into(),
            ))
        }

        fn unseal(
            &self,
            ciphertext: &[u8],
        ) -> keystore::sealer::Result<zeroize::Zeroizing<Vec<u8>>> {
            self.loader.unseal(ciphertext)
        }
    }

    #[test]
    fn rn_wire_gate_refuses_send_and_receive_without_touching_store_files() {
        let (_d, store) = fresh_store();
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        std::fs::write(store.dir.join("sentinel"), b"unchanged").expect("sentinel");
        let before = snapshot_files(&store.dir);
        let sealer = MemorySealer::new();
        let peer = [31u8; 32];

        with_wire_in_enabled_for_test(false, || {
            assert!(matches!(
                send_rn_with_sealer(&store, &sealer, &peer, 7, b"blocked"),
                Err(RnError::WireInDisabled)
            ));
            assert!(matches!(
                receive_rn_with_sealer(&store, &sealer, &peer, "not touched"),
                Err(RnError::WireInDisabled)
            ));
        });

        assert_eq!(
            snapshot_files(&store.dir),
            before,
            "disabled send/receive must not create or modify store files"
        );
    }

    #[test]
    fn rn_wire_default_wrappers_keep_selected_sealer_signature() {
        let (_d, store) = fresh_store();
        let peer = [30u8; 32];

        with_wire_in_enabled_for_test(false, || {
            assert!(matches!(
                send_rn(&store, &peer, 7, b"default send wrapper"),
                Err(RnError::WireInDisabled)
            ));
            assert!(matches!(
                receive_rn(&store, &peer, "not touched"),
                Err(RnError::WireInDisabled)
            ));
        });
    }

    #[test]
    fn app_state_runtime_gate_starts_open_with_the_compile_time_fuse() {
        let state = crate::AppState::new();

        assert!(RN_WIRE_IN_ENABLED, "the build fuse must be open");
        assert!(app_state_wire_in_enabled(&state));

        state.set_rn_wire_in_enabled(true);
        assert!(
            app_state_wire_in_enabled(&state),
            "the state-aware gate is controlled by AppState"
        );
    }

    #[test]
    fn app_state_runtime_gate_can_refuse_state_aware_send_and_receive_when_closed() {
        let state = crate::AppState::new();
        state.set_rn_wire_in_enabled(false);
        let (_d, store) = fresh_store();
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        std::fs::write(store.dir.join("sentinel"), b"unchanged").expect("sentinel");
        let before = snapshot_files(&store.dir);
        let sealer = MemorySealer::new();
        let peer = [32u8; 32];

        assert!(matches!(
            send_rn_for_state(&state, &store, &sealer, &peer, 7, b"blocked"),
            Err(RnError::WireInDisabled)
        ));
        assert!(matches!(
            receive_rn_for_state(&state, &store, &sealer, &peer, "not touched"),
            Err(RnError::WireInDisabled)
        ));

        assert_eq!(
            snapshot_files(&store.dir),
            before,
            "state-gated disabled send/receive must not touch store files"
        );
    }

    #[test]
    fn app_state_runtime_gate_allows_state_aware_send_when_enabled() {
        let state = crate::AppState::new();
        state.set_rn_wire_in_enabled(true);
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(38);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        let wire = send_rn_for_state(&state, &store, &sealer, &peer, 7, b"state enabled")
            .expect("state-enabled send");

        assert!(!wire.is_empty());
    }

    #[test]
    fn rn_wire_in_enabled_follows_app_state_controlled_toggle() {
        let state = crate::AppState::new();
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(73);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        assert!(RN_WIRE_IN_ENABLED, "the compile-time fuse must be open");
        let wire = send_rn_for_state(&state, &store, &sealer, &peer, 73, b"allowed")
            .expect("open AppState gate sends");
        assert_eq!(
            osl_ratchet_next::peek_wire_version(&wire),
            Some(WIRE_VERSION_RN)
        );

        state.set_rn_wire_in_enabled(false);
        assert!(matches!(
            send_rn_for_state(&state, &store, &sealer, &peer, 73, b"blocked again"),
            Err(RnError::WireInDisabled)
        ));
    }

    #[test]
    fn send_rn_persists_advanced_state_before_returning_wire() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(31);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        let loaded_before = store
            .load_session_with_sealer(&peer, &sealer)
            .expect("load before")
            .expect("session before");
        let mut exported_before = SecureSession::export(&loaded_before).expect("export before");
        let wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&store, &sealer, &peer, 7, b"persist before return").expect("send")
        });
        assert!(!wire.is_empty(), "send must return the wire after saving");

        let loaded_after = store
            .load_session_with_sealer(&peer, &sealer)
            .expect("load after")
            .expect("session after");
        let mut exported_after = SecureSession::export(&loaded_after).expect("export after");
        assert!(
            exported_after != exported_before,
            "persisted session must already be advanced after send_rn returns"
        );
        exported_before.zeroize();
        exported_after.zeroize();
    }

    #[test]
    fn send_rn_returns_error_without_wire_when_save_fails() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let failing = SealFailure { loader: &sealer };
        let mut rng = seeded_rng(32);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        let result = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&store, &failing, &peer, 8, b"must not escape")
        });
        assert!(
            result.is_err(),
            "save failure must not return a wire string"
        );
        let err = result.expect_err("checked err");
        assert!(
            matches!(&err, RnError::Storage(msg) if msg.contains("forced save failure")),
            "expected forced save failure, got {err:?}"
        );
    }

    #[test]
    fn send_rn_reload_after_crash_does_not_reuse_the_previous_wire() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(33);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        let wire1 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&store, &sealer, &peer, 9, b"same plaintext").expect("first send")
        });
        let wire2 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&store, &sealer, &peer, 9, b"same plaintext")
                .expect("second send after reload")
        });

        assert_ne!(
            wire2, wire1,
            "reloading from persisted post-send state must not reuse the first wire"
        );
    }

    #[test]
    fn receive_rn_rejects_identical_wire_replay() {
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(6);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let bob_peer = *bob_bundle.identity.as_bytes();
        let alice_peer = *alice_ik_pub.as_bytes();

        initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");
        let bootstrap_wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 6, b"bootstrap")
                .expect("bootstrap send")
        });
        accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_prekeys,
            bob_prekeys.identity.public().as_bytes(),
            &bob_ek,
            &bootstrap_wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept bootstrap");

        let wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 7, b"no replay")
                .expect("ordinary send")
        });
        let opened = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire).expect("first receive")
        });
        assert_eq!(opened.msg_type, 7);
        assert_eq!(opened.plaintext, b"no replay");

        assert!(
            with_wire_in_enabled_for_test(true, || {
                receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire)
            })
            .is_err(),
            "a receive-side state save must consume the message key so the identical wire cannot replay"
        );
    }

    #[test]
    fn b93_delayed_out_of_order_receive_rn_uses_persisted_skipped_keys() {
        let (_alice_dir, alice_store) = fresh_store();
        let (bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(93);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let bob_peer = *bob_bundle.identity.as_bytes();
        let alice_peer = *alice_ik_pub.as_bytes();

        initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("alice initiates persisted RN session");

        let bootstrap_wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 10, b"rn-bootstrap")
                .expect("alice sends bootstrap")
        });
        let (_bob_session, opened_bootstrap) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_prekeys,
            bob_prekeys.identity.public().as_bytes(),
            &bob_ek,
            &bootstrap_wire,
            CTX,
            SessionParams::default(),
        )
        .expect("bob accepts bootstrap");
        assert_eq!(opened_bootstrap.msg_type, 10);
        assert_eq!(opened_bootstrap.plaintext, b"rn-bootstrap");

        let wire_1 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 11, b"rn-delayed-1")
                .expect("alice sends m1")
        });
        let wire_2 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 12, b"rn-delayed-2")
                .expect("alice sends m2")
        });
        let wire_3 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 13, b"rn-delivered-first")
                .expect("alice sends m3")
        });

        let delivered_first = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire_3)
                .expect("bob receives m3 first")
        });
        assert_eq!(delivered_first.msg_type, 13);
        assert_eq!(delivered_first.plaintext, b"rn-delivered-first");

        let reloaded_bob_store = RnSessionStore::new(bob_dir.path().join("rn"));
        let delayed_1 = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_1)
                .expect("bob receives delayed m1 from persisted skipped cache")
        });
        let delayed_2 = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_2)
                .expect("bob receives delayed m2 from persisted skipped cache")
        });
        assert_eq!(delayed_1.msg_type, 11);
        assert_eq!(delayed_1.plaintext, b"rn-delayed-1");
        assert_eq!(delayed_2.msg_type, 12);
        assert_eq!(delayed_2.plaintext, b"rn-delayed-2");

        assert!(
            with_wire_in_enabled_for_test(true, || {
                receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_3)
            })
            .is_err(),
            "a previously opened RN message must not replay after delayed delivery"
        );
    }

    #[test]
    fn receive_rn_evicts_skipped_keys_at_bounded_limits() {
        let (_alice_dir, alice_store) = fresh_store();
        let (bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(108);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let bob_peer = *bob_bundle.identity.as_bytes();
        let alice_peer = *alice_ik_pub.as_bytes();
        let capped_params = SessionParams {
            skip: osl_ratchet_next::SkipParams {
                max_skip_per_message: 4,
                max_keys_per_chain: 2,
                max_total_keys: 2,
                max_chains: 1,
                ..osl_ratchet_next::SkipParams::default()
            },
            ..SessionParams::default()
        };

        initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            capped_params,
        )
        .expect("alice initiates persisted RN session");

        let bootstrap_wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 20, b"rn-bootstrap")
                .expect("alice sends bootstrap")
        });
        accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_prekeys,
            bob_prekeys.identity.public().as_bytes(),
            &bob_ek,
            &bootstrap_wire,
            CTX,
            capped_params,
        )
        .expect("bob accepts bootstrap");

        let wire_1 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 21, b"rn-evicted")
                .expect("alice sends m1")
        });
        let wire_2 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 22, b"rn-retained-2")
                .expect("alice sends m2")
        });
        let wire_3 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 23, b"rn-retained-3")
                .expect("alice sends m3")
        });
        let wire_4 = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 24, b"rn-delivered-first")
                .expect("alice sends m4")
        });

        let delivered_first = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire_4)
                .expect("bob receives m4 first")
        });
        assert_eq!(delivered_first.msg_type, 24);
        assert_eq!(delivered_first.plaintext, b"rn-delivered-first");

        let reloaded_bob_store = RnSessionStore::new(bob_dir.path().join("rn"));
        let stored = reloaded_bob_store
            .load_session_with_sealer(&alice_peer, &sealer)
            .expect("load persisted bob session")
            .expect("persisted bob session");
        assert_eq!(
            stored.skipped_key_count(),
            2,
            "receive_rn must persist only the bounded skipped-key cache"
        );

        assert!(
            with_wire_in_enabled_for_test(true, || {
                receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_1)
            })
            .is_err(),
            "the oldest delayed RN message must be evicted when the receive-side cache reaches its cap"
        );

        let delayed_2 = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_2)
                .expect("bob receives retained m2")
        });
        let delayed_3 = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&reloaded_bob_store, &sealer, &alice_peer, &wire_3)
                .expect("bob receives retained m3")
        });
        assert_eq!(delayed_2.msg_type, 22);
        assert_eq!(delayed_2.plaintext, b"rn-retained-2");
        assert_eq!(delayed_3.msg_type, 23);
        assert_eq!(delayed_3.plaintext, b"rn-retained-3");
    }

    #[test]
    fn receive_rn_rejects_identical_wire_replay_second_path() {
        let (_alice_dir, alice_store) = fresh_store();
        let (_bob_dir, bob_store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(6);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let bob_peer = *bob_bundle.identity.as_bytes();
        let alice_peer = *alice_ik_pub.as_bytes();

        initiate_and_persist_with_sealer(
            &alice_store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("alice initiates");
        let bootstrap_wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 6, b"bootstrap")
                .expect("bootstrap send")
        });
        let (_bob_session, opened_bootstrap) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_prekeys,
            bob_prekeys.identity.public().as_bytes(),
            &bob_ek,
            &bootstrap_wire,
            CTX,
            SessionParams::default(),
        )
        .expect("bob accepts bootstrap");
        assert_eq!(opened_bootstrap.plaintext, b"bootstrap");

        let wire = with_wire_in_enabled_for_test(true, || {
            send_rn_with_sealer(&alice_store, &sealer, &bob_peer, 7, b"replay target")
                .expect("send replay target")
        });
        let first_open = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire).expect("first receive")
        });
        assert_eq!(first_open.msg_type, 7);
        assert_eq!(first_open.plaintext, b"replay target");

        let before_replay = store_file_bytes(&bob_store.session_path(&alice_peer));
        let replay = with_wire_in_enabled_for_test(true, || {
            receive_rn_with_sealer(&bob_store, &sealer, &alice_peer, &wire)
        });
        assert!(
            replay.is_err(),
            "the identical RN wire must be rejected on replay"
        );
        assert_eq!(
            store_file_bytes(&bob_store.session_path(&alice_peer)),
            before_replay,
            "a rejected replay must not roll back or advance persisted RN state"
        );
    }

    #[test]
    fn a_plaintext_sealer_is_refused() {
        let (_d, store) = fresh_store();
        let mut rng = seeded_rng(11);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();

        assert!(matches!(
            store.save_session_with_sealer(&peer, &session, &NoOpSealer),
            Err(RnError::PlaintextSealerRefused)
        ));
        // And nothing was written.
        assert!(store
            .load_session_with_sealer(&peer, &MemorySealer::new())
            .expect("load")
            .is_none());
    }

    #[test]
    fn b42_rn_session_store_uses_select_best_sealer_for_at_rest_sessions() {
        let (_d, store) = fresh_store();
        let mut rng = seeded_rng(0xB42);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();

        store
            .save_session(&peer, &session)
            .expect("default save uses selected sealer");

        let raw = std::fs::read(store.session_path(&peer)).expect("read file");
        let blob: SealedBlob = serde_json::from_slice(&raw).expect("parse blob");
        let production_methods = [
            keystore::sealer::METHOD_TPM,
            keystore::sealer::METHOD_KEYRING,
            keystore::sealer::METHOD_EPHEMERAL,
        ];
        assert!(
            production_methods.contains(&blob.method.as_str()),
            "RnSessionStore default path must use select_best_sealer, got {}",
            blob.method
        );
        assert_ne!(blob.method, keystore::sealer::METHOD_NOOP);
        assert_ne!(blob.method, keystore::sealer::METHOD_MEMORY);
        assert!(store
            .load_session(&peer)
            .expect("default load uses selected sealer")
            .is_some());
    }

    #[test]
    fn a_session_survives_a_save_load_round_trip() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(12);
        let (prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let mut alice =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();

        let wire = alice
            .encrypt(0, b"before restart", &mut rng)
            .expect("encrypt");
        store
            .save_session_with_sealer(&peer, &alice, &sealer)
            .expect("save");

        let mut restored = store
            .load_session_with_sealer(&peer, &sealer)
            .expect("load")
            .expect("session present");
        let wire2 = restored
            .encrypt(0, b"after restart", &mut rng)
            .expect("encrypt");

        // Both messages open on the far side, in order.
        let (mut bob, first) =
            Session::accept(&prekeys, &wire, SessionParams::default(), &mut rng).expect("accept");
        assert_eq!(first.plaintext, b"before restart");
        assert_eq!(
            bob.decrypt(&wire2, &mut rng).expect("decrypt").plaintext,
            b"after restart"
        );
    }

    #[test]
    fn a_restored_session_blob_below_the_send_high_water_is_refused_before_a_wire_is_produced() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(0xA6);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (identity, _) = x25519_keypair(&mut rng);
        let session = Session::initiate(&identity, &bundle, SessionParams::default(), &mut rng)
            .expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save initial session");
        let rollback_blob = store_file_bytes(&store.session_path(&peer));

        with_wire_in_enabled_for_test(true, || {
            for i in 0..5 {
                send_rn_with_sealer(&store, &sealer, &peer, 7, format!("message {i}").as_bytes())
                    .expect("advance and persist session");
            }

            std::fs::write(store.session_path(&peer), &rollback_blob)
                .expect("restore earlier session blob");
            let before_refused_send = store_file_bytes(&store.session_path(&peer));
            let refused = send_rn_with_sealer(&store, &sealer, &peer, 7, b"must not produce wire");
            assert!(
                matches!(
                    refused,
                    Err(RnError::RolledBackSession {
                        blob_counter: 0,
                        high_water: 5
                    })
                ),
                "a restored session blob must fail closed before encryption, got {refused:?}"
            );
            assert_eq!(
                store_file_bytes(&store.session_path(&peer)),
                before_refused_send,
                "a refused send must not replace the restored blob or produce a new persisted state"
            );
        });
    }

    #[test]
    fn the_sealed_file_contains_no_recognisable_state() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(13);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        let raw = std::fs::read(store.session_path(&peer)).expect("read file");
        // The plaintext export must not appear anywhere in the file.
        let plain = SecureSession::export(&session).expect("export");
        assert!(
            !contains_subslice(&raw, &plain),
            "sealed file must not contain the plaintext export"
        );
        use base64::Engine as _;
        let standard_b64 = base64::engine::general_purpose::STANDARD.encode(&plain);
        assert!(
            !contains_subslice(&raw, standard_b64.as_bytes()),
            "sealed file must not contain the standard base64 export"
        );
        let url_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&plain);
        assert!(
            !contains_subslice(&raw, url_b64.as_bytes()),
            "sealed file must not contain the base64url no-pad export"
        );
        let lower_hex = plain
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(
            !contains_subslice(&raw, lower_hex.as_bytes()),
            "sealed file must not contain the lowercase hex export"
        );
        // Nor may the peer's identity key appear in the clear. (It is
        // public, but its presence would prove the blob is unsealed.)
        assert!(!contains_subslice(&raw, &peer));
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() || needle.len() > haystack.len() {
            return false;
        }
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    fn store_file_bytes(path: &Path) -> Vec<u8> {
        std::fs::read(path).expect("read store file")
    }

    /// Losing session state must degrade to "no session" — never to a
    /// silent legacy send. The pin must survive.
    #[test]
    fn losing_session_state_keeps_the_pin_and_forces_a_rehandshake() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(14);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();

        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");
        store.raise_pin_to_rn(&peer).expect("pin");

        // Simulate total session loss (crash, corruption, manual wipe).
        store.delete_session(&peer).expect("delete");

        assert!(
            store
                .load_session_with_sealer(&peer, &sealer)
                .expect("load")
                .is_none(),
            "a lost session must read as absent, i.e. re-handshake"
        );
        let pin = store.load_pin(&peer).expect("load pin");
        assert!(pin.is_pinned_to_rn(), "the pin must outlive the session");
        assert!(
            matches!(
                select_wire_version(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic),
                Err(RnError::PinnedToRn)
            ),
            "state loss must not open a downgrade window"
        );
    }

    #[test]
    fn rn_session_store_migrates_or_retires_legacy_v4_sessions() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let peer = [80u8; 32];
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        store.raise_pin_to_rn(&peer).expect("pin");
        let legacy_v4 = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "root_key_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "dhs_secret_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "dhs_pub_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "sending_counter": 0,
            "receiving_counter": 0,
            "prev_sending_count": 0,
            "nhks_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "nhkr_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "skipped": [],
            "ctx": {
                "local_ik_x25519_pub_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "local_ik_mlkem_pub_b64": "",
                "peer_ik_x25519_pub_b64": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                "peer_ik_mlkem_pub_b64": "",
                "conversation_id_b64": "",
                "session_version": 1
            }
        }))
        .expect("legacy v4 json");
        let session_path = store.session_path(&peer);
        std::fs::write(&session_path, &legacy_v4).expect("seed legacy session");

        let loaded = store
            .load_session_with_sealer(&peer, &sealer)
            .expect("legacy session is retired");

        assert!(
            loaded.is_none(),
            "a retired legacy v4 state must force a clean RN re-handshake"
        );
        assert!(
            !session_path.exists(),
            "legacy v4 state must not remain at the active RN session path"
        );
        assert_eq!(
            std::fs::read(session_path.with_extension("session.legacy-v4-retired"))
                .expect("retired legacy file"),
            legacy_v4,
            "retirement must preserve the legacy bytes out of the active load path"
        );
        assert!(store.load_pin(&peer).expect("pin").is_pinned_to_rn());
        assert!(
            matches!(
                select_wire_version(
                    &store.load_pin(&peer).expect("pin reload"),
                    PeerCapabilities::Absent,
                    RnPolicy::Opportunistic,
                ),
                Err(RnError::PinnedToRn)
            ),
            "retiring the session must not allow a silent v3 fallback"
        );
    }

    #[test]
    fn a_corrupt_session_blob_is_an_error_not_a_silent_reset() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(15);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        std::fs::write(store.session_path(&peer), b"{\"not\": \"a blob\"}").expect("clobber");
        assert!(store.load_session_with_sealer(&peer, &sealer).is_err());
    }

    #[test]
    fn a_corrupt_pin_file_is_an_error_not_a_downgrade() {
        let (_d, store) = fresh_store();
        let peer = [9u8; 32];
        store.raise_pin_to_rn(&peer).expect("pin");
        std::fs::write(store.pin_path(&peer), b"garbage").expect("clobber");
        assert!(
            store.load_pin(&peer).is_err(),
            "an unparseable pin must not read as UNKNOWN"
        );
    }

    /// A torn or half-written file is the other half of the corruption
    /// story and behaves differently from garbage JSON: it can still be
    /// *syntactically* plausible for a prefix. Every truncation length
    /// must be an error, and in particular must never read as "no
    /// session" (which would be a silent, unlogged re-handshake) and
    /// never as a successfully imported session.
    #[test]
    fn a_truncated_session_blob_is_an_error_at_every_length() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(21);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let session =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");
        store.raise_pin_to_rn(&peer).expect("pin");

        let path = store.session_path(&peer);
        let whole = std::fs::read(&path).expect("read");
        // Sample truncation lengths across the whole file rather than
        // all of them: the file is ~KiB and the property is uniform.
        let mut lengths: Vec<usize> = (0..whole.len()).step_by(37).collect();
        lengths.push(whole.len().saturating_sub(1));
        for cut in lengths {
            std::fs::write(&path, &whole[..cut]).expect("truncate");
            match store.load_session_with_sealer(&peer, &sealer) {
                Err(_) => {}
                Ok(None) => panic!("a truncated session must not read as absent"),
                Ok(Some(_)) => panic!("a truncated session must not import"),
            }
            // The pin is untouched by any of this, so there is still no
            // downgrade window while the session is unreadable.
            assert!(store.load_pin(&peer).expect("pin").is_pinned_to_rn());
        }

        // Deleting the unreadable file is the sanctioned recovery, and
        // it lands on "re-handshake", not on "v=3 is fine".
        store.delete_session(&peer).expect("delete");
        assert!(store
            .load_session_with_sealer(&peer, &sealer)
            .expect("load")
            .is_none());
        assert!(matches!(
            select_wire_version(
                &store.load_pin(&peer).expect("pin"),
                PeerCapabilities::Absent,
                RnPolicy::Opportunistic
            ),
            Err(RnError::PinnedToRn)
        ));
    }

    /// The read path must refuse an over-size file before allocating for
    /// it, and must not treat it as absent.
    #[test]
    fn an_oversized_session_file_is_refused_without_being_read() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let peer = [42u8; 32];
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        let path = store.session_path(&peer);
        std::fs::write(&path, vec![b'x'; MAX_SESSION_FILE_BYTES as usize + 1]).expect("write");
        assert!(matches!(
            store.load_session_with_sealer(&peer, &sealer),
            Err(RnError::Storage(_))
        ));
        assert!(matches!(
            store.load_session_with_sealer(&peer, &sealer),
            Err(RnError::Storage(message))
                if message == format!(
                    "session file is {} bytes, over the {MAX_SESSION_FILE_BYTES}-byte bound",
                    MAX_SESSION_FILE_BYTES + 1
                )
        ));
    }

    #[test]
    fn an_oversized_pin_file_is_refused_rather_than_read_as_unknown() {
        let (_d, store) = fresh_store();
        let peer = [43u8; 32];
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        std::fs::write(
            store.pin_path(&peer),
            vec![b' '; MAX_PIN_FILE_BYTES as usize + 1],
        )
        .expect("write");
        assert!(
            store.load_pin(&peer).is_err(),
            "an over-size pin file must not read as UNKNOWN"
        );
    }

    /// The record cap must refuse the newcomer, never delete somebody
    /// else's live session to make room.
    #[test]
    fn a_full_store_refuses_a_new_peer_and_evicts_nothing() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(22);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let incumbent =
            Session::initiate(&ik, &bundle, SessionParams::default(), &mut rng).expect("initiate");
        let incumbent_peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&incumbent_peer, &incumbent, &sealer)
            .expect("save");

        // Fill the remaining slots with placeholder records. Their
        // contents do not matter; only the record count does.
        for i in 0..(MAX_SESSION_RECORDS - 1) {
            std::fs::write(store.dir.join(format!("filler{i:04}.session")), b"{}").expect("filler");
        }

        let (_p2, bundle2) = fresh_bundle(&mut rng);
        let (ik2, _) = x25519_keypair(&mut rng);
        let newcomer = Session::initiate(&ik2, &bundle2, SessionParams::default(), &mut rng)
            .expect("initiate");
        let newcomer_peer = *bundle2.identity.as_bytes();
        assert!(matches!(
            store.save_session_with_sealer(&newcomer_peer, &newcomer, &sealer),
            Err(RnError::StoreFull { .. })
        ));

        // The incumbent is untouched and still loads.
        assert!(store
            .load_session_with_sealer(&incumbent_peer, &sealer)
            .expect("load")
            .is_some());
        // Overwriting an existing record is still allowed at the cap:
        // an established conversation is never starved by it.
        store
            .save_session_with_sealer(&incumbent_peer, &incumbent, &sealer)
            .expect("overwrite at cap");
    }

    /// A state blob written under looser skipped-key caps must not
    /// silently raise this build's memory ceiling.
    #[test]
    fn a_session_claiming_an_oversized_skipped_cache_is_refused_on_load() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(23);
        let (_prekeys, bundle) = fresh_bundle(&mut rng);
        let (ik, _) = x25519_keypair(&mut rng);
        let loose = SessionParams {
            skip: osl_ratchet_next::SkipParams {
                max_total_keys: MAX_SKIPPED_KEYS_POLICY + 1,
                ..osl_ratchet_next::SkipParams::default()
            },
            ..SessionParams::default()
        };
        let session = Session::initiate(&ik, &bundle, loose, &mut rng).expect("initiate");
        let peer = *bundle.identity.as_bytes();
        store
            .save_session_with_sealer(&peer, &session, &sealer)
            .expect("save");

        assert!(matches!(
            store.load_session_with_sealer(&peer, &sealer),
            Err(RnError::SkippedCacheTooLarge { .. })
        ));
        // A session at the ceiling still loads.
        let at_ceiling = SessionParams {
            skip: osl_ratchet_next::SkipParams {
                max_total_keys: MAX_SKIPPED_KEYS_POLICY,
                ..osl_ratchet_next::SkipParams::default()
            },
            ..SessionParams::default()
        };
        let ok = Session::initiate(&ik, &bundle, at_ceiling, &mut rng).expect("initiate");
        let peer2 = [77u8; 32];
        store
            .save_session_with_sealer(&peer2, &ok, &sealer)
            .expect("save");
        assert!(store
            .load_session_with_sealer(&peer2, &sealer)
            .expect("load")
            .is_some());
    }

    #[test]
    fn rn_session_store_migrates_or_retires_legacy_v4_sessions_second_path() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let peer = [80u8; 32];
        std::fs::create_dir_all(&store.dir).expect("mkdir");
        std::fs::write(
            store.session_path(&peer),
            br#"{"version":4,"method":"memory","sealed_b64":"bm90LWFuLXJuLXNlc3Npb24="}"#,
        )
        .expect("seed legacy v4-shaped session file");

        let err = match store.load_session_with_sealer(&peer, &sealer) {
            Err(err) => err,
            Ok(None) => panic!("legacy v4-shaped at-rest state must not read as absent"),
            Ok(Some(_)) => panic!("legacy v4-shaped at-rest state must not load as RN"),
        };
        assert!(
            matches!(&err, RnError::Storage(message) if message.contains("session blob version 4")),
            "legacy v4-shaped state must be explicitly refused, got {err:?}"
        );
        assert!(
            with_wire_in_enabled_for_test(true, || {
                send_rn_with_sealer(&store, &sealer, &peer, 80, b"must not reuse v4 state")
            })
            .is_err(),
            "the RN send path must not silently reuse or reset a legacy v4 session file"
        );
    }

    #[test]
    fn an_out_of_range_pin_value_is_rejected() {
        let (_d, store) = fresh_store();
        let peer = [3u8; 32];
        std::fs::create_dir_all(store.dir.clone()).expect("mkdir");
        std::fs::write(
            store.pin_path(&peer),
            br#"{"version":1,"pin":{"min_wire_version":4}}"#,
        )
        .expect("write");
        assert!(store.load_pin(&peer).is_err());
    }

    #[test]
    fn an_absent_pin_is_unknown() {
        let (_d, store) = fresh_store();
        assert_eq!(
            store.load_pin(&[1u8; 32]).expect("load"),
            RnPeerPin::UNKNOWN
        );
    }

    #[test]
    fn initiating_with_absent_capabilities_does_not_pin_or_refuse_legacy() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(34);
        let (_bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let peer = *bob_bundle.identity.as_bytes();

        initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Absent,
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");

        let pin = store.load_pin(&peer).expect("load pin");
        assert_eq!(
            pin,
            RnPeerPin::UNKNOWN,
            "Absent capabilities must not pin on local initiation"
        );
        assert_eq!(
            select_wire_version(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic)
                .expect("select legacy"),
            SelectedVersion::LegacyV3,
            "a later opportunistic legacy send must remain permitted"
        );
    }

    #[test]
    fn initiating_with_unverified_capabilities_does_not_pin_or_refuse_legacy() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(35);
        let (_bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let peer = *bob_bundle.identity.as_bytes();

        initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Unverified,
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");

        let pin = store.load_pin(&peer).expect("load pin");
        assert_eq!(
            pin,
            RnPeerPin::UNKNOWN,
            "Unverified capabilities must not pin on local initiation"
        );
        assert_eq!(
            select_wire_version(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic)
                .expect("select legacy"),
            SelectedVersion::LegacyV3,
            "a later opportunistic legacy send must remain permitted"
        );
    }

    #[test]
    fn initiating_with_verified_rn_capabilities_pins_and_blocks_downgrade() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(36);
        let (_bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let peer = *bob_bundle.identity.as_bytes();

        initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");

        let pin = store.load_pin(&peer).expect("load pin");
        assert!(
            pin.is_pinned_to_rn(),
            "Verified RN capabilities must pin on initiation"
        );
        assert!(
            matches!(
                select_wire_version(&pin, PeerCapabilities::Absent, RnPolicy::Opportunistic),
                Err(RnError::PinnedToRn)
            ),
            "a pinned peer must not downgrade after capability stripping"
        );
    }

    #[test]
    fn verified_initiation_pin_survives_session_state_loss() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(37);
        let (_bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let peer = *bob_bundle.identity.as_bytes();

        initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");
        store.delete_session(&peer).expect("delete session");

        assert!(
            store
                .load_session_with_sealer(&peer, &sealer)
                .expect("load session")
                .is_none(),
            "session deletion must leave no persisted session"
        );
        assert!(
            store.load_pin(&peer).expect("load pin").is_pinned_to_rn(),
            "verified initiation pin must survive session deletion"
        );
    }

    // ---- bound end-to-end through this module ----

    #[test]
    fn initiate_and_accept_through_this_module_round_trips_and_pins_both_sides() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(16);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);
        let bob_ek = bob_bundle.pq_prekey.to_bytes();

        let mut alice = initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            CTX,
            SessionParams::default(),
        )
        .expect("initiate");

        assert!(store
            .load_pin(bob_bundle.identity.as_bytes())
            .expect("pin")
            .is_pinned_to_rn());

        let wire = alice.encrypt(0, b"hello", &mut rng).expect("encrypt");

        // Bob accepts through his own store.
        let (_d2, bob_store) = fresh_store();
        let bob_ik_pub = bob_prekeys.identity.public();
        let (_bob, opened) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &bob_prekeys,
            bob_ik_pub.as_bytes(),
            &bob_ek,
            &wire,
            CTX,
            SessionParams::default(),
        )
        .expect("accept");
        assert_eq!(opened.plaintext, b"hello");

        // Bob has now pinned Alice, on the strength of a message that
        // actually authenticated.
        assert!(bob_store
            .load_pin(alice_ik_pub.as_bytes())
            .expect("pin")
            .is_pinned_to_rn());
    }

    #[test]
    fn manual_discord_context_does_not_consume_opk_without_responder_opk_state() {
        let (_d, store) = fresh_store();
        let sealer = MemorySealer::new();
        let mut rng = seeded_rng(17);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        assert!(
            bob_bundle.one_time_prekey.is_some(),
            "fixture must include an OPK so this tests the context policy"
        );
        let bob_ek = bob_bundle.pq_prekey.to_bytes();
        let (alice_ik, alice_ik_pub) = x25519_keypair(&mut rng);

        let mut alice = initiate_and_persist_with_sealer(
            &store,
            &sealer,
            &alice_ik,
            alice_ik_pub.as_bytes(),
            &bob_bundle,
            PeerCapabilities::Verified(RN_CAP_WIRE_RN),
            &bob_ek,
            RN_CONTEXT_DISCORD_MANUAL,
            SessionParams::default(),
        )
        .expect("manual-context initiate");
        let wire = alice
            .encrypt(0, b"manual context bootstrap", &mut rng)
            .expect("encrypt");

        let mut identity_only_prekeys = bob_prekeys.clone();
        identity_only_prekeys.one_time_prekeys.clear();
        let (_bob_dir, bob_store) = fresh_store();
        let (_bob, opened) = accept_and_persist_with_sealer(
            &bob_store,
            &sealer,
            &identity_only_prekeys,
            bob_prekeys.identity.public().as_bytes(),
            &bob_ek,
            &wire,
            RN_CONTEXT_DISCORD_MANUAL,
            SessionParams::default(),
        )
        .expect("identity-only responder accepts manual-context bootstrap");
        assert_eq!(opened.plaintext, b"manual context bootstrap");
    }

    /// A peer that does not speak OSL-RN: the accept path must reject a
    /// legacy v=3 blob with a version error, so a router can fall
    /// through to the existing decoders rather than treating it as
    /// corruption — and without mutating any OSL-RN state.
    #[test]
    fn a_v3_blob_is_reported_as_a_version_mismatch() {
        use base64::Engine as _;
        assert_eq!(LEGACY_WIRE_VERSION_V3, 0x03);
        let v3 = format!(
            "DPC0::{}",
            base64::engine::general_purpose::STANDARD.encode([0x03, 0x00, 0x01, 0x02])
        );
        let err = osl_ratchet_next::peek_bootstrap_initiator_identity(&v3)
            .expect_err("v3 must not parse as OSL-RN");
        assert!(matches!(
            err,
            osl_ratchet_next::Error::WrongVersion {
                got: LEGACY_WIRE_VERSION_V3,
                expected: WIRE_VERSION_RN
            }
        ));
    }

    /// Two different peers must never collide in storage.
    #[test]
    fn peer_storage_keys_are_distinct() {
        let a = RnSessionStore::peer_key(&[1u8; 32]);
        let b = RnSessionStore::peer_key(&[2u8; 32]);
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
    }

    /// The negotiation context is an input to `SK`; two different call
    /// paths must not produce interchangeable sessions.
    #[test]
    fn different_contexts_produce_different_bindings() {
        let ek = [7u8; MLKEM_EK];
        let a = initiator_binding(&[1u8; 32], &ek, &[2u8; 32], b"path/a").expect("a");
        let b = initiator_binding(&[1u8; 32], &ek, &[2u8; 32], b"path/b").expect("b");
        assert_ne!(a, b);
    }

    #[test]
    fn a_wrong_length_kem_key_is_refused() {
        assert!(matches!(
            initiator_binding(&[1u8; 32], &[0u8; 16], &[2u8; 32], b"ctx"),
            Err(RnError::BadPeerKemKey)
        ));
    }
}
