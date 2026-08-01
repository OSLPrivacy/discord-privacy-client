//! Sender keys — group/broadcast messaging construction.
//!
//! Spec: `docs/design/sender-keys.md`.
//!
//! # ⚠️  CUSTOM CONSTRUCTION — UNAUDITED — REQUIRES REVIEW ⚠️
//!
//! **This is not libsignal's standard sender-keys.** It is a custom
//! construction designed for this codebase that combines:
//!
//! - Signal-style encrypted-headers (matching the pairwise ratchet's
//!   HE pattern in [`crate::ratchet`]),
//! - per-message header keys derived alongside message keys from the
//!   same chain key,
//! - a TPM-sealable `RotationRoot` (TPM sealing happens in a separate
//!   `keystore` crate, not yet implemented; for now the root sits in
//!   memory),
//! - rotation that increments `chain_id` and reseeds `CK_0` from a
//!   fresh CSPRNG `RotationRoot`.
//!
//! **v1 alpha ships this UNAUDITED.** A cryptographer review is
//! required before v1 stable. Do not assume the construction inherits
//! libsignal's security guarantees — the deviations from libsignal
//! are non-trivial:
//!
//! - libsignal's sender-keys do not encrypt headers and do not have a
//!   `RotationRoot` distinct from `ChainKey`.
//! - our HK is per-message rather than per-chain, requiring a bounded
//!   forward-search in the receiver decrypt path.
//! - rotation distribution to N recipients is out of scope for this
//!   commit (the plan is to ship the new `(chain_id, RotationRoot)`
//!   tuple via the existing pairwise ratchet — handled at a higher
//!   layer, not here).
//!
//! Until that review lands, treat the security claims of this module
//! as informal: forward secrecy on the chain (HKDF one-way step,
//! `CK_n` cannot derive `CK_{n-1}`), forward secrecy on rotation
//! (`RotationRoot` zeroizes; old chain unrecoverable), AEAD integrity
//! per message under the standard XChaCha20-Poly1305 assumptions.
//! Post-compromise security after a rotation depends on the
//! distribution channel (the pairwise ratchet) being intact.
//!
//! # Construction
//!
//! ```text
//! RotationRoot     = CSPRNG(32 bytes)            // fresh per rotate()
//! CK_0             = HKDF(salt=RotationRoot,
//!                         ikm=u32_le(chain_id),
//!                         info="sender-keys/chain-init")
//! CK_{n+1}         = HKDF(salt=zeros,
//!                         ikm=CK_n,
//!                         info="sender-keys/chain-step")     // one-way
//! MK_n             = HKDF(ikm=CK_n, info="sender-keys/msg-key")
//! HK_n             = HKDF(ikm=CK_n, info="sender-keys/header-key")
//! ```
//!
//! Per send: pull `(MK_n, HK_n)` from `CK_n` *without* mutating it;
//! advance `CK_n → CK_{n+1}`. Old `CK_n` and `MK_n` zeroize on drop.
//!
//! # Wire format
//!
//! ```text
//! header_plaintext = physical_device_id             // 32 bytes
//!                 || u32_be(chain_id)
//!                 || u32_be(n)
//!                 || u32_be(prev_chain_length)
//!                 || u32_be(session_version)        // 48 bytes total
//! enc_header       = AEAD-encrypt(HK_n, header_nonce, "", header_plaintext)
//! AD               = canonical_ad_sender_keys(sender_ik..., physical_device_id, group_id, ...)
//! ciphertext       = AEAD-encrypt(MK_n, message_nonce, AD || enc_header, plaintext)
//! ```
//!
//! # Receive flow
//!
//! 1. Sweep skipped-key cache TTL.
//! 2. **Cache lookup** — for each cached entry, try
//!    `aead::open(entry.hk, ...)` on the wire `enc_header`; if it
//!    decodes and `(header.chain_id, header.n) == (entry.chain_id, entry.n)`,
//!    use the cached `mk` (consume entry on AEAD success).
//! 3. **Forward search** — iterate at most `MAX_SKIPPED_PER_CHAIN + 1`
//!    times, advancing a cloned `CK` each step:
//!    - try `aead::open(HK_at_pos, ...)` on `enc_header`;
//!    - if it decodes, validate `session_version` and the
//!      `chain_id`/`n` self-consistency, then attempt the message
//!      AEAD; on success, atomically commit the chain advance + the
//!      cached `(hk, n, mk)` triples for the skipped slots traversed.
//!    - if not, cache `(hk, n, mk)` for this slot in scratch and
//!      advance the tentative chain.
//! 4. Reject (without state mutation) if the loop exhausts without
//!    a match.
//!
//! # Skipped-key cache
//!
//! - 1000 entries hard cap per [`MAX_SKIPPED_PER_CHAIN`].
//! - 30-day TTL per [`SKIPPED_KEY_TTL`].
//! - FIFO eviction at the cap.
//! - Keyed by `(chain_id, n)` so entries from pre-rotation chains
//!   survive rotation and remain usable for late-arriving old-chain
//!   messages.
//! - Atomic commit on AEAD success (same pattern as the pairwise
//!   ratchet).
//!
//! # Rotation
//!
//! [`SenderChain::rotate`] generates a fresh `RotationRoot`,
//! increments `chain_id`, resets `n` to 0, and stamps
//! `prev_chain_length` with the final `n` of the previous chain. The
//! receiver-side equivalent is [`ReceiverChain::rotate_to`], which
//! installs a new `(chain_id, RotationRoot)` while retaining the
//! skipped-key cache.
//!
//! Rotation triggers (1h timer, 500-message threshold, membership
//! change, suspicious events) live in the Tauri shell — not in this
//! crate. The crate only exposes [`SenderChain::rotate`] mechanically.
//!
//! Distribution of the new `(chain_id, RotationRoot)` tuple to the
//! N group recipients is **out of scope for this commit**. The plan is
//! to wrap the tuple in a pairwise-ratchet payload using
//! [`crate::ratchet`]; that integration happens at a higher layer.

use crate::aead;
use crate::error::{Error, Result};
use crate::hkdf;
use crate::random;
use crate::x25519;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::ZeroizeOnDrop;

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// HKDF info labels — domain-separated for v1.
const CHAIN_INIT_INFO: &[u8] = b"sender-keys/chain-init";
const CHAIN_STEP_INFO: &[u8] = b"sender-keys/chain-step";
const MSG_KEY_INFO: &[u8] = b"sender-keys/msg-key";
const HEADER_KEY_INFO: &[u8] = b"sender-keys/header-key";

/// Wire protocol version. Embedded in every header and AD; receivers
/// reject mismatches before AEAD.
pub const SESSION_VERSION_V1: u32 = 1;

/// Hard cap on cached skipped message keys per receiving chain.
/// Also bounds the forward-search loop in
/// [`ReceiverChain::decrypt_at`].
pub const MAX_SKIPPED_PER_CHAIN: usize = 1000;

/// Maximum receiver chains retained for one peer. The physical device id
/// arrives on the wire, so this bounds both retained skipped keys and the
/// number of receiver chains an inbound message can make us search.
pub const MAX_RECEIVER_CHAINS_PER_PEER: usize = 32;

/// TTL for cached skipped message keys.
pub const SKIPPED_KEY_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Physical sender-device binding size.
pub const PHYSICAL_DEVICE_ID_BYTES: usize = 32;

/// Plaintext header layout:
/// `physical_device_id(32) || u32_be(chain_id) || u32_be(n) ||
/// u32_be(prev_chain_length) || u32_be(session_version)`.
pub const HEADER_BYTES: usize = PHYSICAL_DEVICE_ID_BYTES + 16;

/// Opaque physical-device binding for sender-key chains.
///
/// This is an identifier, so Debug deliberately reports only structure.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalDeviceId([u8; PHYSICAL_DEVICE_ID_BYTES]);

impl PhysicalDeviceId {
    pub fn random() -> Self {
        loop {
            let bytes = random::random_bytes(PHYSICAL_DEVICE_ID_BYTES);
            let mut out = [0u8; PHYSICAL_DEVICE_ID_BYTES];
            out.copy_from_slice(&bytes);
            if out.iter().any(|b| *b != 0) {
                return PhysicalDeviceId(out);
            }
        }
    }

    pub fn from_bytes(bytes: [u8; PHYSICAL_DEVICE_ID_BYTES]) -> Result<Self> {
        if bytes.iter().all(|b| *b == 0) {
            return Err(Error::Internal(
                "sender keys: physical_device_id binding absent".into(),
            ));
        }
        Ok(PhysicalDeviceId(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; PHYSICAL_DEVICE_ID_BYTES] {
        &self.0
    }
}

impl fmt::Debug for PhysicalDeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PhysicalDeviceId([REDACTED])")
    }
}

/// 32-byte sender-keys chain key. One-way HKDF advance, separate
/// derivations for `MK` and `HK`. Zeroizes on drop.
#[derive(Clone, ZeroizeOnDrop)]
struct SenderChainKey([u8; 32]);

impl SenderChainKey {
    fn from_bytes(b: [u8; 32]) -> Self {
        SenderChainKey(b)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// `MK_n = HKDF(salt=zeros, ikm=CK_n, info=msg-key)`.
    fn message_key(&self) -> Result<aead::Key> {
        let bytes = hkdf::derive_32(&[], &self.0, MSG_KEY_INFO)?;
        Ok(aead::Key::from_bytes(bytes))
    }

    /// `HK_n = HKDF(salt=zeros, ikm=CK_n, info=header-key)`.
    fn header_key(&self) -> Result<aead::Key> {
        let bytes = hkdf::derive_32(&[], &self.0, HEADER_KEY_INFO)?;
        Ok(aead::Key::from_bytes(bytes))
    }

    /// `CK_{n+1} = HKDF(salt=zeros, ikm=CK_n, info=chain-step)`.
    fn advance(&mut self) -> Result<()> {
        let next = hkdf::derive_32(&[], &self.0, CHAIN_STEP_INFO)?;
        self.0 = next;
        Ok(())
    }
}

/// 32-byte rotation root. Held in memory in v1 alpha; designed for
/// TPM sealing in a forthcoming `keystore` crate. Zeroizes on drop.
#[derive(Clone, ZeroizeOnDrop)]
struct RotationRoot([u8; 32]);

impl RotationRoot {
    fn random() -> Self {
        let mut out = [0u8; 32];
        let bytes = random::random_bytes(32);
        out.copy_from_slice(&bytes);
        RotationRoot(out)
    }

    fn from_bytes(b: [u8; 32]) -> Self {
        RotationRoot(b)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

fn derive_ck_0(root: &RotationRoot, chain_id: u32) -> Result<SenderChainKey> {
    let bytes = hkdf::derive_32(root.as_bytes(), &chain_id.to_le_bytes(), CHAIN_INIT_INFO)?;
    Ok(SenderChainKey::from_bytes(bytes))
}

/// Plaintext sender-keys header. Carried on every wire message in
/// AEAD-encrypted form (`enc_header`); never plaintext on the outer
/// wire. Public so consumers can introspect the inner header after
/// `enc_header` has been opened in tests or diagnostic tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub physical_device_id: PhysicalDeviceId,
    pub chain_id: u32,
    pub n: u32,
    pub prev_chain_length: u32,
    pub session_version: u32,
}

impl Header {
    /// Fixed serialization:
    /// `physical_device_id(32) || u32_be(chain_id) || u32_be(n) ||
    /// u32_be(prev_chain_length) || u32_be(session_version)`.
    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut out = [0u8; HEADER_BYTES];
        out[..PHYSICAL_DEVICE_ID_BYTES].copy_from_slice(self.physical_device_id.as_bytes());
        out[32..36].copy_from_slice(&self.chain_id.to_be_bytes());
        out[36..40].copy_from_slice(&self.n.to_be_bytes());
        out[40..44].copy_from_slice(&self.prev_chain_length.to_be_bytes());
        out[44..48].copy_from_slice(&self.session_version.to_be_bytes());
        out
    }

    /// Parse fixed bytes back into a [`Header`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != HEADER_BYTES {
            return Err(Error::Internal(format!(
                "sender keys header: wrong length (got {}, want {})",
                bytes.len(),
                HEADER_BYTES
            )));
        }
        let mut device_bytes = [0u8; PHYSICAL_DEVICE_ID_BYTES];
        device_bytes.copy_from_slice(&bytes[..PHYSICAL_DEVICE_ID_BYTES]);
        Ok(Header {
            physical_device_id: PhysicalDeviceId::from_bytes(device_bytes)?,
            chain_id: u32::from_be_bytes(bytes[32..36].try_into().unwrap()),
            n: u32::from_be_bytes(bytes[36..40].try_into().unwrap()),
            prev_chain_length: u32::from_be_bytes(bytes[40..44].try_into().unwrap()),
            session_version: u32::from_be_bytes(bytes[44..48].try_into().unwrap()),
        })
    }
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

/// Canonical length-prefixed AD encoding for sender-keys messages,
/// per `docs/design/sender-keys.md`:
///
/// ```text
/// AD = LP(sender_ik_x25519_pub) || LP(sender_ik_mlkem_pub)
///    || LP(physical_device_id)
///    || LP(group_id)
///    || LP(u32_be(chain_id)) || LP(u32_be(n))
///    || LP(u32_be(prev_chain_length))
///    || LP(u32_be(session_version))
/// ```
///
/// where `LP(x) = u32_be(x.len()) || x`. Deterministic for fixed
/// inputs.
// Eight separate arguments on purpose. This builds a CANONICAL associated-data
// encoding, and every argument is a distinct domain-separated input. Collapsing
// them into a struct would add a way to construct the AD with a field unset or
// in the wrong order, which is exactly the failure this function exists to make
// impossible.
#[allow(clippy::too_many_arguments)]
pub fn canonical_ad_sender_keys(
    sender_ik_x25519_pub: &[u8; 32],
    sender_ik_mlkem_pub: &[u8],
    physical_device_id: &PhysicalDeviceId,
    group_id: &[u8],
    chain_id: u32,
    n: u32,
    prev_chain_length: u32,
    session_version: u32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, sender_ik_x25519_pub);
    write_lp(&mut buf, sender_ik_mlkem_pub);
    write_lp(&mut buf, physical_device_id.as_bytes());
    write_lp(&mut buf, group_id);
    write_lp(&mut buf, &chain_id.to_be_bytes());
    write_lp(&mut buf, &n.to_be_bytes());
    write_lp(&mut buf, &prev_chain_length.to_be_bytes());
    write_lp(&mut buf, &session_version.to_be_bytes());
    buf
}

/// Sender-side identity + group parameters used for the canonical AD
/// encoding on every encrypt/decrypt call.
#[derive(Clone)]
pub struct SenderContext {
    pub sender_ik_x25519_pub: x25519::PublicKey,
    pub sender_ik_mlkem_pub: Vec<u8>,
    pub group_id: Vec<u8>,
    pub session_version: u32,
}

/// On-the-wire encrypted sender-keys message.
#[derive(Clone, Debug)]
pub struct EncryptedMessage {
    pub header_nonce: aead::Nonce,
    pub enc_header: Vec<u8>,
    pub message_nonce: aead::Nonce,
    pub ciphertext: Vec<u8>,
}

/// One cached skipped (header-key, message-key) pair.
///
/// Keyed by `(chain_id, n)`. Pre-rotation entries survive
/// [`ReceiverChain::rotate_to`] so late-arriving old-chain messages
/// can still decrypt.
#[derive(Clone)]
struct SkippedKey {
    chain_id: u32,
    n: u32,
    hk: aead::Key,
    mk: aead::Key,
    inserted_at: SystemTime,
}

#[derive(Clone, Default)]
struct SkippedKeyCache {
    keys: Vec<SkippedKey>,
}

impl SkippedKeyCache {
    fn sweep_expired(&mut self, now: SystemTime) {
        self.keys
            .retain(|k| match now.duration_since(k.inserted_at) {
                Ok(elapsed) => elapsed < SKIPPED_KEY_TTL,
                Err(_) => true,
            });
    }

    fn insert(&mut self, entry: SkippedKey) {
        if self
            .keys
            .iter()
            .any(|existing| existing.chain_id == entry.chain_id && existing.n == entry.n)
        {
            return;
        }
        if self.keys.len() >= MAX_SKIPPED_PER_CHAIN {
            self.keys.remove(0);
        }
        self.keys.push(entry);
    }

    fn len(&self) -> usize {
        self.keys.len()
    }
}

/// Sender side of the sender-keys construction. Owns the rotation
/// root, current chain id, current `CK_n`, and the running counter.
pub struct SenderChain {
    physical_device_id: PhysicalDeviceId,
    rotation_root: RotationRoot,
    chain_id: u32,
    ck_n: SenderChainKey,
    n: u32,
    prev_chain_length: u32,
    /// Phase 9-A3: unix-seconds timestamp at which this chain was
    /// first seeded. Used by the IPC layer's 24-hour rotation
    /// trigger. Stamped at `new()` / `rotate()`; otherwise immutable.
    chain_started_at: u64,
    /// Phase 9-A3: snapshot of the group's member set at the time
    /// the chain was installed/rotated. The IPC layer compares this
    /// against the current channel members on each send; any diff
    /// triggers a rotate. Bytes here are caller-defined opaque
    /// member ids (typically Discord snowflakes as UTF-8 bytes).
    last_known_members: Vec<Vec<u8>>,
    /// Phase 6.2: unix-seconds timestamp of the most recent SKDM
    /// bundle emission for this chain. Lets the IPC send loop skip
    /// SKDM dispatch when the chain hasn't changed AND the last
    /// emission is still within the self-heal window. Stamped by
    /// `new()` / `rotate()` and refreshed by callers via
    /// `mark_skdm_emitted_at`. Defaults to 0 on legacy on-disk
    /// records, which forces a one-time emit on the next send.
    last_skdm_emit_at: u64,
}

impl SenderChain {
    /// Create a fresh sender chain at `chain_id = 0` with a CSPRNG
    /// rotation root and a fresh local physical-device binding.
    pub fn new() -> Result<Self> {
        Self::new_for_physical_device(PhysicalDeviceId::random())
    }

    /// Create a fresh sender chain bound to a caller-provided
    /// physical-device id. A zero/absent binding is refused by
    /// [`PhysicalDeviceId::from_bytes`] before this constructor is
    /// reachable.
    pub fn new_for_physical_device(physical_device_id: PhysicalDeviceId) -> Result<Self> {
        let rotation_root = RotationRoot::random();
        let ck_0 = derive_ck_0(&rotation_root, 0)?;
        let now = now_unix_secs();
        Ok(SenderChain {
            physical_device_id,
            rotation_root,
            chain_id: 0,
            ck_n: ck_0,
            n: 0,
            prev_chain_length: 0,
            chain_started_at: now,
            last_known_members: Vec::new(),
            // Install IS an SKDM-emit event; stamp now so the
            // periodic gate counts from install.
            last_skdm_emit_at: now,
        })
    }

    /// Rotate to a fresh `RotationRoot` and a new `chain_id`. The
    /// previous chain's final `n` is stamped into `prev_chain_length`
    /// so that out-of-band-distributed receivers know where the old
    /// chain ended.
    pub fn rotate(&mut self) -> Result<()> {
        let new_chain_id = self
            .chain_id
            .checked_add(1)
            .ok_or_else(|| Error::Internal("sender keys: chain_id overflow on rotate".into()))?;
        let prev = self.n;
        let now = now_unix_secs();
        self.rotation_root = RotationRoot::random();
        self.chain_id = new_chain_id;
        self.ck_n = derive_ck_0(&self.rotation_root, self.chain_id)?;
        self.n = 0;
        self.prev_chain_length = prev;
        self.chain_started_at = now;
        // last_known_members reset by the orchestrator after rotate
        // (it has the fresh member snapshot to install).
        self.last_known_members.clear();
        // Rotation always emits an SKDM bundle.
        self.last_skdm_emit_at = now;
        Ok(())
    }

    /// Phase 9-A3: unix-seconds timestamp when this chain was seeded.
    pub fn chain_started_at(&self) -> u64 {
        self.chain_started_at
    }

    /// Phase 9-A3: snapshot of the member set at install/rotate time.
    pub fn last_known_members(&self) -> &[Vec<u8>] {
        &self.last_known_members
    }

    /// Phase 6.2: unix-seconds of the most recent SKDM bundle emit.
    /// 0 for chains loaded from pre-6.2 on-disk records (forces a
    /// one-time periodic emit on next send).
    pub fn last_skdm_emit_at(&self) -> u64 {
        self.last_skdm_emit_at
    }

    /// Phase 6.2: stamp the most recent SKDM emit time. Caller is
    /// the IPC encrypt_v5_send loop, which calls this after
    /// successfully building (not necessarily POSTing) the bundle.
    pub fn mark_skdm_emitted_at(&mut self, now: u64) {
        self.last_skdm_emit_at = now;
    }

    /// Phase 9-A3: install the current member-set snapshot. Called
    /// by the IPC layer immediately after `new()`/`rotate()` once it
    /// has gathered the channel's current members.
    pub fn set_last_known_members(&mut self, members: Vec<Vec<u8>>) {
        self.last_known_members = members;
    }

    pub fn current_chain_id(&self) -> u32 {
        self.chain_id
    }

    pub fn current_n(&self) -> u32 {
        self.n
    }

    pub fn current_prev_chain_length(&self) -> u32 {
        self.prev_chain_length
    }

    /// Physical-device binding for this sender chain. Distribution
    /// layers must carry this beside `(chain_id, rotation_root)` so a
    /// receiver never collapses two devices for the same account into
    /// one mutable chain.
    pub fn physical_device_id(&self) -> PhysicalDeviceId {
        self.physical_device_id
    }

    /// Current rotation root bytes. The caller is responsible for
    /// distributing `(chain_id, rotation_root)` to receivers via the
    /// pairwise ratchet (out of scope for this crate).
    pub fn rotation_root_bytes(&self) -> [u8; 32] {
        *self.rotation_root.as_bytes()
    }

    /// Test/diagnostic-only: current `CK_n` bytes. Exposed so
    /// integration tests can verify chain advancement.
    #[doc(hidden)]
    pub fn dbg_ck_bytes(&self) -> [u8; 32] {
        *self.ck_n.as_bytes()
    }

    pub fn encrypt(&mut self, plaintext: &[u8], ctx: &SenderContext) -> Result<EncryptedMessage> {
        let mk = self.ck_n.message_key()?;
        let hk = self.ck_n.header_key()?;
        self.ck_n.advance()?;
        let n = self.n;
        self.n = self
            .n
            .checked_add(1)
            .ok_or_else(|| Error::Internal("sender keys: n overflow".into()))?;

        let header = Header {
            physical_device_id: self.physical_device_id,
            chain_id: self.chain_id,
            n,
            prev_chain_length: self.prev_chain_length,
            session_version: ctx.session_version,
        };
        let header_bytes = header.to_bytes();
        let header_nonce = random::random_nonce();
        let enc_header = aead::seal(&hk, &header_nonce, b"", &header_bytes)?;

        let message_nonce = random::random_nonce();
        let mut full_ad = canonical_ad_sender_keys(
            ctx.sender_ik_x25519_pub.as_bytes(),
            &ctx.sender_ik_mlkem_pub,
            &self.physical_device_id,
            &ctx.group_id,
            self.chain_id,
            n,
            self.prev_chain_length,
            ctx.session_version,
        );
        full_ad.extend_from_slice(&enc_header);
        let ciphertext = aead::seal(&mk, &message_nonce, &full_ad, plaintext)?;

        Ok(EncryptedMessage {
            header_nonce,
            enc_header,
            message_nonce,
            ciphertext,
        })
    }
}

/// Receiver side of the sender-keys construction. One per peer-sender
/// in a group. Tracks the current chain state and a skipped-key cache
/// keyed by `(chain_id, n)` that survives rotations.
pub struct ReceiverChain {
    physical_device_id: PhysicalDeviceId,
    chain_id: u32,
    ck_n: SenderChainKey,
    n: u32,
    skipped: SkippedKeyCache,
}

impl ReceiverChain {
    /// Install a fresh chain for a peer-sender. The
    /// `(chain_id, rotation_root)` tuple comes from the pairwise
    /// ratchet (delivery is out-of-scope for this crate).
    pub fn install(
        chain_id: u32,
        rotation_root: &[u8; 32],
        physical_device_id: PhysicalDeviceId,
    ) -> Result<Self> {
        let root = RotationRoot::from_bytes(*rotation_root);
        let ck_0 = derive_ck_0(&root, chain_id)?;
        Ok(ReceiverChain {
            physical_device_id,
            chain_id,
            ck_n: ck_0,
            n: 0,
            skipped: SkippedKeyCache::default(),
        })
    }

    /// Replace the active chain with a new `(chain_id, rotation_root)`.
    /// The skipped-key cache is **retained** so late-arriving messages
    /// from the previous chain can still decrypt.
    pub fn rotate_to(&mut self, chain_id: u32, rotation_root: &[u8; 32]) -> Result<()> {
        // Rotation is MONOTONE, and re-applying the CURRENT chain is a NO-OP.
        //
        // The attack: replaying a captured SKDM re-derived ck_0 and reset `n` to 0,
        // after which every message already delivered on that chain decrypted
        // again -- defeating the chain-counter refusal by rewinding the counter it
        // depends on.
        //
        // But rejecting `chain_id == self.chain_id` outright breaks a documented
        // liveness contract: the send loop periodically RE-EMITS the same
        // (chain_id, rotation_root) so a receiver who missed the first SKDM can
        // recover, and `apply_skdm_recv` is specified as idempotent. Erroring
        // there turns every self-heal re-emit into a dispatch failure -> backoff
        // -> dead letter.
        //
        // So: forward rotations apply, the current chain is accepted and ignored
        // (leaving `n` intact, which is what closes the replay), and anything
        // older is refused. A different rotation_root offered under the CURRENT
        // chain_id is also ignored rather than installed -- an attacker must not
        // be able to re-seed a live chain.
        if chain_id < self.chain_id {
            return Err(Error::Internal(format!(
                "sender keys: refusing to rewind receiver chain (have chain_id {}, offered {})",
                self.chain_id, chain_id
            )));
        }
        if chain_id == self.chain_id {
            return Ok(());
        }
        let root = RotationRoot::from_bytes(*rotation_root);
        let ck_0 = derive_ck_0(&root, chain_id)?;
        self.chain_id = chain_id;
        self.ck_n = ck_0;
        self.n = 0;
        Ok(())
    }

    pub fn current_chain_id(&self) -> u32 {
        self.chain_id
    }

    pub fn current_n(&self) -> u32 {
        self.n
    }

    pub fn physical_device_id(&self) -> PhysicalDeviceId {
        self.physical_device_id
    }

    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    pub fn decrypt(&mut self, msg: &EncryptedMessage, ctx: &SenderContext) -> Result<Vec<u8>> {
        self.decrypt_at(msg, ctx, SystemTime::now())
    }

    pub fn decrypt_at(
        &mut self,
        msg: &EncryptedMessage,
        ctx: &SenderContext,
        now: SystemTime,
    ) -> Result<Vec<u8>> {
        self.skipped.sweep_expired(now);

        // 1) Try cache.
        if let Some(plaintext) = self.try_decrypt_skipped(msg, ctx)? {
            return Ok(plaintext);
        }

        // 2) Forward-search on current chain. Tentative state until
        //    AEAD success.
        let mut tentative_ck = self.ck_n.clone();
        let mut tentative_n = self.n;
        let mut new_skipped: Vec<SkippedKey> = Vec::new();

        let mut matched: Option<(aead::Key, Header)> = None;
        for _ in 0..=MAX_SKIPPED_PER_CHAIN {
            let hk = tentative_ck.header_key()?;
            if let Ok(header_bytes) = aead::open(&hk, &msg.header_nonce, b"", &msg.enc_header) {
                if let Ok(header) = Header::from_bytes(&header_bytes) {
                    if header.session_version != ctx.session_version {
                        return Err(Error::Internal(format!(
                            "sender keys decrypt: session_version mismatch \
                             (header={}, expected={})",
                            header.session_version, ctx.session_version
                        )));
                    }
                    if header.physical_device_id != self.physical_device_id {
                        return Err(Error::Internal(
                            "sender keys decrypt: physical_device_id binding mismatch".into(),
                        ));
                    }
                    if header.chain_id != self.chain_id {
                        return Err(Error::Internal(format!(
                            "sender keys decrypt: header chain_id {} disagrees with \
                             receiver chain {} (HK collision or malformed header)",
                            header.chain_id, self.chain_id
                        )));
                    }
                    if header.n != tentative_n {
                        return Err(Error::Internal(format!(
                            "sender keys decrypt: header n {} disagrees with tentative \
                             slot {} (malformed header)",
                            header.n, tentative_n
                        )));
                    }
                    let mk = tentative_ck.message_key()?;
                    matched = Some((mk, header));
                    break;
                }
            }
            // No match at this slot — record (hk, n, mk) for tentative
            // skip and advance the cloned chain.
            let mk_skip = tentative_ck.message_key()?;
            new_skipped.push(SkippedKey {
                chain_id: self.chain_id,
                n: tentative_n,
                hk,
                mk: mk_skip,
                inserted_at: now,
            });
            tentative_ck.advance()?;
            tentative_n = tentative_n.checked_add(1).ok_or_else(|| {
                Error::Internal("sender keys: n overflow during forward search".into())
            })?;
        }

        let (mk, header) = matched.ok_or_else(|| {
            Error::Internal(format!(
                "sender keys decrypt: no matching slot within MAX_SKIPPED_PER_CHAIN \
                 ({MAX_SKIPPED_PER_CHAIN}) iterations (replay, oversized gap, unknown chain, \
                 or tampered header)"
            ))
        })?;

        // Decrypt the message ciphertext.
        let mut full_ad = canonical_ad_sender_keys(
            ctx.sender_ik_x25519_pub.as_bytes(),
            &ctx.sender_ik_mlkem_pub,
            &self.physical_device_id,
            &ctx.group_id,
            header.chain_id,
            header.n,
            header.prev_chain_length,
            header.session_version,
        );
        full_ad.extend_from_slice(&msg.enc_header);
        let plaintext = aead::open(&mk, &msg.message_nonce, &full_ad, &msg.ciphertext)?;

        // Commit — atomic.
        tentative_ck.advance()?;
        let new_n = tentative_n
            .checked_add(1)
            .ok_or_else(|| Error::Internal("sender keys: n overflow on commit".into()))?;
        self.ck_n = tentative_ck;
        self.n = new_n;
        for entry in new_skipped {
            self.skipped.insert(entry);
        }
        Ok(plaintext)
    }

    fn try_decrypt_skipped(
        &mut self,
        msg: &EncryptedMessage,
        ctx: &SenderContext,
    ) -> Result<Option<Vec<u8>>> {
        let mut matched_idx: Option<usize> = None;
        let mut matched_header: Option<Header> = None;
        for (idx, entry) in self.skipped.keys.iter().enumerate() {
            if let Ok(header_bytes) = aead::open(&entry.hk, &msg.header_nonce, b"", &msg.enc_header)
            {
                if let Ok(header) = Header::from_bytes(&header_bytes) {
                    if header.chain_id == entry.chain_id && header.n == entry.n {
                        matched_idx = Some(idx);
                        matched_header = Some(header);
                        break;
                    }
                }
            }
        }
        let (idx, header) = match (matched_idx, matched_header) {
            (Some(i), Some(h)) => (i, h),
            _ => return Ok(None),
        };

        if header.session_version != ctx.session_version {
            return Err(Error::Internal(format!(
                "sender keys decrypt: session_version mismatch (header={}, expected={})",
                header.session_version, ctx.session_version
            )));
        }
        if header.physical_device_id != self.physical_device_id {
            return Err(Error::Internal(
                "sender keys decrypt: physical_device_id binding mismatch".into(),
            ));
        }

        let mk = self.skipped.keys[idx].mk.clone();
        let mut full_ad = canonical_ad_sender_keys(
            ctx.sender_ik_x25519_pub.as_bytes(),
            &ctx.sender_ik_mlkem_pub,
            &self.physical_device_id,
            &ctx.group_id,
            header.chain_id,
            header.n,
            header.prev_chain_length,
            header.session_version,
        );
        full_ad.extend_from_slice(&msg.enc_header);
        let plaintext = aead::open(&mk, &msg.message_nonce, &full_ad, &msg.ciphertext)?;
        self.skipped.keys.remove(idx);
        Ok(Some(plaintext))
    }
}

/// Orchestrator for a participant's sender-keys state in a group:
/// one outgoing [`SenderChain`] (optional — set via
/// [`Self::install_sender`]) plus a map of
/// `peer_id → bounded receiver-chain LRU` for incoming senders.
pub struct SenderKeyState {
    sender: Option<SenderChain>,
    receivers: HashMap<Vec<u8>, Vec<ReceiverChain>>,
}

/// Observable result of installing a receiver chain.
///
/// `evicted_physical_device_id` identifies the least-recently-used chain
/// removed to enforce [`MAX_RECEIVER_CHAINS_PER_PEER`]. Callers that persist
/// or surface receiver-chain changes can record the eviction instead of
/// silently losing the chain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReceiverChainInstall {
    pub evicted_physical_device_id: Option<PhysicalDeviceId>,
}

impl Default for SenderKeyState {
    fn default() -> Self {
        Self::new()
    }
}

impl SenderKeyState {
    pub fn new() -> Self {
        SenderKeyState {
            sender: None,
            receivers: HashMap::new(),
        }
    }

    /// Create + install a fresh outgoing sender chain.
    pub fn install_sender(&mut self) -> Result<()> {
        self.sender = Some(SenderChain::new()?);
        Ok(())
    }

    pub fn install_sender_for_physical_device(
        &mut self,
        physical_device_id: PhysicalDeviceId,
    ) -> Result<()> {
        self.sender = Some(SenderChain::new_for_physical_device(physical_device_id)?);
        Ok(())
    }

    /// Borrow the outgoing sender chain (e.g. to inspect its
    /// `rotation_root_bytes()` for distribution).
    pub fn sender_chain(&self) -> Option<&SenderChain> {
        self.sender.as_ref()
    }

    pub fn sender_chain_mut(&mut self) -> Option<&mut SenderChain> {
        self.sender.as_mut()
    }

    pub fn rotate_sender(&mut self) -> Result<()> {
        let s = self
            .sender
            .as_mut()
            .ok_or_else(|| Error::Internal("sender keys: no sender chain to rotate".into()))?;
        s.rotate()
    }

    /// Install an incoming receiver chain for a peer-sender.
    pub fn install_receiver(
        &mut self,
        peer_id: Vec<u8>,
        chain_id: u32,
        rotation_root: &[u8; 32],
        physical_device_id: PhysicalDeviceId,
    ) -> Result<ReceiverChainInstall> {
        let chain = ReceiverChain::install(chain_id, rotation_root, physical_device_id)?;
        let chains = self.receivers.entry(peer_id).or_default();
        if let Some(existing_idx) = chains
            .iter()
            .position(|c| c.physical_device_id == physical_device_id)
        {
            // The vector is ordered least- to most-recently used. A fresh
            // distribution for an existing device refreshes that device.
            chains.remove(existing_idx);
            chains.push(chain);
        } else {
            let evicted_physical_device_id = if chains.len() == MAX_RECEIVER_CHAINS_PER_PEER {
                Some(chains.remove(0).physical_device_id())
            } else {
                None
            };
            chains.push(chain);
            return Ok(ReceiverChainInstall {
                evicted_physical_device_id,
            });
        }
        Ok(ReceiverChainInstall::default())
    }

    /// Rotate an existing peer's receiver chain to a new
    /// `(chain_id, rotation_root)`. The pre-rotation skipped-key
    /// cache is retained.
    pub fn rotate_receiver(
        &mut self,
        peer_id: &[u8],
        chain_id: u32,
        rotation_root: &[u8; 32],
        physical_device_id: PhysicalDeviceId,
    ) -> Result<()> {
        let chains = self.receivers.get_mut(peer_id).ok_or_else(|| {
            Error::Internal("sender keys: no receiver chain for peer physical_device_id".into())
        })?;
        let chain_idx = chains
            .iter()
            .position(|c| c.physical_device_id == physical_device_id)
            .ok_or_else(|| {
                Error::Internal("sender keys: no receiver chain for peer physical_device_id".into())
            })?;
        chains[chain_idx].rotate_to(chain_id, rotation_root)?;
        let recently_used = chains.remove(chain_idx);
        chains.push(recently_used);
        Ok(())
    }

    pub fn receiver_chain(&self, peer_id: &[u8]) -> Option<&ReceiverChain> {
        self.receivers
            .get(peer_id)
            .and_then(|chains| chains.first())
    }

    pub fn receiver_chain_for_physical_device(
        &self,
        peer_id: &[u8],
        physical_device_id: PhysicalDeviceId,
    ) -> Option<&ReceiverChain> {
        self.receivers.get(peer_id).and_then(|chains| {
            chains
                .iter()
                .find(|c| c.physical_device_id == physical_device_id)
        })
    }

    pub fn receiver_chain_mut(&mut self, peer_id: &[u8]) -> Option<&mut ReceiverChain> {
        self.receivers
            .get_mut(peer_id)
            .and_then(|chains| chains.first_mut())
    }

    pub fn encrypt(&mut self, plaintext: &[u8], ctx: &SenderContext) -> Result<EncryptedMessage> {
        let s = self
            .sender
            .as_mut()
            .ok_or_else(|| Error::Internal("sender keys: no sender chain installed".into()))?;
        s.encrypt(plaintext, ctx)
    }

    pub fn decrypt_from(
        &mut self,
        peer_id: &[u8],
        msg: &EncryptedMessage,
        ctx: &SenderContext,
    ) -> Result<Vec<u8>> {
        let chains = self
            .receivers
            .get_mut(peer_id)
            .ok_or_else(|| Error::Internal("sender keys: no receiver chain for peer".into()))?;
        let mut last_err = None;
        for idx in 0..chains.len() {
            match chains[idx].decrypt(msg, ctx) {
                Ok(plaintext) => {
                    // A successful decrypt is a receiver-chain use, so make
                    // it most-recently used before returning.
                    let recently_used = chains.remove(idx);
                    chains.push(recently_used);
                    return Ok(plaintext);
                }
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err
            .unwrap_or_else(|| Error::Internal("sender keys: no receiver chain for peer".into())))
    }
}

// ============================================================
// Phase 9-A3: persistable mirror of sender-keys state
// ============================================================

/// On-disk version byte for [`SenderKeyStateOnDisk`]. Bump on any
/// shape change to force a clean reject of stale-format records.
pub const SENDER_KEY_STATE_ON_DISK_VERSION: u8 = 0x02;

/// Errors raised when reconstructing live state from a persisted
/// [`SenderKeyStateOnDisk`].
#[derive(Debug, thiserror::Error)]
pub enum SenderKeyPersistError {
    #[error("sender-key state on-disk version 0x{got:02x} != supported 0x{want:02x}")]
    UnsupportedVersion { got: u8, want: u8 },
    #[error("sender-key state on-disk: base64 decode {field}: {source}")]
    Base64 {
        field: &'static str,
        #[source]
        source: base64::DecodeError,
    },
    #[error("sender-key state on-disk: {field} length {got} != expected {want}")]
    BadLength {
        field: &'static str,
        got: usize,
        want: usize,
    },
    #[error("sender-key state on-disk: physical_device_id binding absent")]
    MissingPhysicalDeviceId,
    #[error("sender-key state on-disk: peer has more than {max} receiver chains")]
    ReceiverChainLimitExceeded { max: usize },
}

/// Persistable mirror of [`SenderKeyState`]. Inner byte arrays are
/// base64 strings (matching peer_map.json / ratchet_state convention).
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SenderKeyStateOnDisk {
    pub version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<SenderChainOnDisk>,
    /// HashMap encoded as Vec<(key, value)> so the on-disk JSON is a
    /// stable sequence rather than serde_json's unordered object form.
    /// Keys are opaque peer_id byte sequences (base64-encoded for
    /// transport).
    #[serde(default)]
    pub receivers: Vec<(String, ReceiverChainOnDisk)>,
}

impl Default for SenderKeyStateOnDisk {
    fn default() -> Self {
        SenderKeyStateOnDisk {
            version: SENDER_KEY_STATE_ON_DISK_VERSION,
            sender: None,
            receivers: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SenderChainOnDisk {
    #[serde(default)]
    pub physical_device_id_b64: String,
    pub rotation_root_b64: String,
    pub chain_id: u32,
    pub ck_n_b64: String,
    pub n: u32,
    pub prev_chain_length: u32,
    pub chain_started_at: u64,
    /// Base64-encoded opaque member ids (caller-defined; typically
    /// Discord snowflakes as UTF-8 bytes).
    #[serde(default)]
    pub last_known_members_b64: Vec<String>,
    /// Phase 6.2: unix-seconds of the most recent SKDM bundle
    /// emit. `#[serde(default)]` (= 0) on pre-6.2 records, which
    /// the IPC layer interprets as "emit on next send" so old
    /// chains pick up periodic-emit semantics from one self-heal
    /// trigger onward.
    #[serde(default)]
    pub last_skdm_emit_at: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiverChainOnDisk {
    #[serde(default)]
    pub physical_device_id_b64: String,
    pub chain_id: u32,
    pub ck_n_b64: String,
    pub n: u32,
    #[serde(default)]
    pub skipped: Vec<SkippedKeyOnDisk>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedKeyOnDisk {
    pub chain_id: u32,
    pub n: u32,
    pub hk_b64: String,
    pub mk_b64: String,
    pub added_at_unix_secs: u64,
}

impl From<&SenderKeyState> for SenderKeyStateOnDisk {
    fn from(s: &SenderKeyState) -> Self {
        SenderKeyStateOnDisk {
            version: SENDER_KEY_STATE_ON_DISK_VERSION,
            sender: s.sender.as_ref().map(SenderChainOnDisk::from),
            receivers: s
                .receivers
                .iter()
                .flat_map(|(peer_id, chains)| {
                    chains.iter().map(move |chain| {
                        (STANDARD.encode(peer_id), ReceiverChainOnDisk::from(chain))
                    })
                })
                .collect(),
        }
    }
}

impl From<&SenderChain> for SenderChainOnDisk {
    fn from(c: &SenderChain) -> Self {
        SenderChainOnDisk {
            physical_device_id_b64: STANDARD.encode(c.physical_device_id.as_bytes()),
            rotation_root_b64: STANDARD.encode(c.rotation_root.as_bytes()),
            chain_id: c.chain_id,
            ck_n_b64: STANDARD.encode(c.ck_n.as_bytes()),
            n: c.n,
            prev_chain_length: c.prev_chain_length,
            chain_started_at: c.chain_started_at,
            last_known_members_b64: c
                .last_known_members
                .iter()
                .map(|m| STANDARD.encode(m))
                .collect(),
            last_skdm_emit_at: c.last_skdm_emit_at,
        }
    }
}

impl From<&ReceiverChain> for ReceiverChainOnDisk {
    fn from(c: &ReceiverChain) -> Self {
        ReceiverChainOnDisk {
            physical_device_id_b64: STANDARD.encode(c.physical_device_id.as_bytes()),
            chain_id: c.chain_id,
            ck_n_b64: STANDARD.encode(c.ck_n.as_bytes()),
            n: c.n,
            skipped: c
                .skipped
                .keys
                .iter()
                .map(|e| SkippedKeyOnDisk {
                    chain_id: e.chain_id,
                    n: e.n,
                    hk_b64: STANDARD.encode(e.hk.as_bytes()),
                    mk_b64: STANDARD.encode(e.mk.as_bytes()),
                    added_at_unix_secs: e
                        .inserted_at
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                })
                .collect(),
        }
    }
}

impl TryFrom<SenderKeyStateOnDisk> for SenderKeyState {
    type Error = SenderKeyPersistError;

    fn try_from(s: SenderKeyStateOnDisk) -> std::result::Result<Self, Self::Error> {
        if s.version != SENDER_KEY_STATE_ON_DISK_VERSION {
            return Err(SenderKeyPersistError::UnsupportedVersion {
                got: s.version,
                want: SENDER_KEY_STATE_ON_DISK_VERSION,
            });
        }
        let sender = match s.sender {
            Some(disk) => Some(disk.try_into()?),
            None => None,
        };
        let mut receivers: HashMap<Vec<u8>, Vec<ReceiverChain>> = HashMap::new();
        for (peer_b64, chain_disk) in s.receivers {
            let peer_bytes =
                STANDARD
                    .decode(&peer_b64)
                    .map_err(|source| SenderKeyPersistError::Base64 {
                        field: "receivers.peer_id",
                        source,
                    })?;
            let chain: ReceiverChain = chain_disk.try_into()?;
            let chains = receivers.entry(peer_bytes).or_default();
            if chains.len() == MAX_RECEIVER_CHAINS_PER_PEER {
                return Err(SenderKeyPersistError::ReceiverChainLimitExceeded {
                    max: MAX_RECEIVER_CHAINS_PER_PEER,
                });
            }
            chains.push(chain);
        }
        Ok(SenderKeyState { sender, receivers })
    }
}

impl TryFrom<SenderChainOnDisk> for SenderChain {
    type Error = SenderKeyPersistError;

    fn try_from(d: SenderChainOnDisk) -> std::result::Result<Self, Self::Error> {
        let physical_device_id = decode_physical_device_id(&d.physical_device_id_b64)?;
        let rotation_root =
            RotationRoot::from_bytes(decode_32(&d.rotation_root_b64, "rotation_root")?);
        let ck_n = SenderChainKey::from_bytes(decode_32(&d.ck_n_b64, "ck_n")?);
        let mut last_known_members = Vec::with_capacity(d.last_known_members_b64.len());
        for (idx, b64) in d.last_known_members_b64.iter().enumerate() {
            let m = STANDARD
                .decode(b64)
                .map_err(|source| SenderKeyPersistError::Base64 {
                    // Static-str field name; the index is for logging only.
                    field: "last_known_members[?]",
                    source,
                })?;
            let _ = idx;
            last_known_members.push(m);
        }
        Ok(SenderChain {
            physical_device_id,
            rotation_root,
            chain_id: d.chain_id,
            ck_n,
            n: d.n,
            prev_chain_length: d.prev_chain_length,
            chain_started_at: d.chain_started_at,
            last_known_members,
            last_skdm_emit_at: d.last_skdm_emit_at,
        })
    }
}

impl TryFrom<ReceiverChainOnDisk> for ReceiverChain {
    type Error = SenderKeyPersistError;

    fn try_from(d: ReceiverChainOnDisk) -> std::result::Result<Self, Self::Error> {
        let physical_device_id = decode_physical_device_id(&d.physical_device_id_b64)?;
        let ck_n = SenderChainKey::from_bytes(decode_32(&d.ck_n_b64, "receiver.ck_n")?);
        let mut skipped_keys: Vec<SkippedKey> = Vec::with_capacity(d.skipped.len());
        for entry in d.skipped {
            skipped_keys.push(SkippedKey {
                chain_id: entry.chain_id,
                n: entry.n,
                hk: aead::Key::from_bytes(decode_32(&entry.hk_b64, "skipped.hk")?),
                mk: aead::Key::from_bytes(decode_32(&entry.mk_b64, "skipped.mk")?),
                inserted_at: UNIX_EPOCH + Duration::from_secs(entry.added_at_unix_secs),
            });
        }
        Ok(ReceiverChain {
            physical_device_id,
            chain_id: d.chain_id,
            ck_n,
            n: d.n,
            skipped: SkippedKeyCache { keys: skipped_keys },
        })
    }
}

fn decode_physical_device_id(
    s: &str,
) -> std::result::Result<PhysicalDeviceId, SenderKeyPersistError> {
    let bytes = decode_32(s, "physical_device_id")?;
    PhysicalDeviceId::from_bytes(bytes).map_err(|_| SenderKeyPersistError::MissingPhysicalDeviceId)
}

impl fmt::Debug for SenderKeyStateOnDisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderKeyStateOnDisk")
            .field("version", &self.version)
            .field("sender", &self.sender.as_ref().map(|_| "[REDACTED]"))
            .field(
                "receivers",
                &format_args!("[REDACTED; {} entries]", self.receivers.len()),
            )
            .finish()
    }
}

impl fmt::Debug for SenderChainOnDisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenderChainOnDisk")
            .field("physical_device_id_b64", &"[REDACTED]")
            .field("rotation_root_b64", &"[REDACTED]")
            .field("chain_id", &self.chain_id)
            .field("ck_n_b64", &"[REDACTED]")
            .field("n", &self.n)
            .field("prev_chain_length", &self.prev_chain_length)
            .field("chain_started_at", &self.chain_started_at)
            .field(
                "last_known_members_b64",
                &format_args!("[REDACTED; {} entries]", self.last_known_members_b64.len()),
            )
            .field("last_skdm_emit_at", &self.last_skdm_emit_at)
            .finish()
    }
}

impl fmt::Debug for ReceiverChainOnDisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReceiverChainOnDisk")
            .field("physical_device_id_b64", &"[REDACTED]")
            .field("chain_id", &self.chain_id)
            .field("ck_n_b64", &"[REDACTED]")
            .field("n", &self.n)
            .field(
                "skipped",
                &format_args!("[REDACTED; {} entries]", self.skipped.len()),
            )
            .finish()
    }
}

impl fmt::Debug for SkippedKeyOnDisk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SkippedKeyOnDisk")
            .field("chain_id", &self.chain_id)
            .field("n", &self.n)
            .field("hk_b64", &"[REDACTED]")
            .field("mk_b64", &"[REDACTED]")
            .field("added_at_unix_secs", &self.added_at_unix_secs)
            .finish()
    }
}

fn decode_32(s: &str, field: &'static str) -> std::result::Result<[u8; 32], SenderKeyPersistError> {
    let bytes = STANDARD
        .decode(s)
        .map_err(|source| SenderKeyPersistError::Base64 { field, source })?;
    if bytes.len() != 32 {
        return Err(SenderKeyPersistError::BadLength {
            field,
            got: bytes.len(),
            want: 32,
        });
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(seed: u8) -> SenderContext {
        SenderContext {
            sender_ik_x25519_pub: x25519::PublicKey::from_bytes([seed; 32]),
            sender_ik_mlkem_pub: vec![seed; 32],
            group_id: b"sender-keys-device-binding-test".to_vec(),
            session_version: SESSION_VERSION_V1,
        }
    }

    fn device(seed: u8) -> PhysicalDeviceId {
        PhysicalDeviceId::from_bytes([seed; PHYSICAL_DEVICE_ID_BYTES]).unwrap()
    }

    fn skipped_key(chain_id: u32, n: u32) -> SkippedKey {
        SkippedKey {
            chain_id,
            n,
            hk: aead::Key::from_bytes([n as u8; 32]),
            mk: aead::Key::from_bytes([n as u8; 32]),
            inserted_at: UNIX_EPOCH,
        }
    }

    #[test]
    fn skipped_key_cache_deduplicates_replayed_slots() {
        let mut cache = SkippedKeyCache::default();

        // Twenty replays of a 51-slot gap must not consume the 1,000-slot
        // cache or evict a still-live skipped key.
        for _ in 0..20 {
            for n in 0..=50 {
                cache.insert(skipped_key(7, n));
            }
        }

        assert_eq!(cache.len(), 51);
        assert!(cache
            .keys
            .iter()
            .all(|entry| entry.chain_id == 7 && entry.n <= 50));
    }

    #[test]
    fn receiver_refuses_wrong_physical_device_binding() {
        let mut sender = SenderChain::new_for_physical_device(device(0x11)).unwrap();
        let mut receiver = ReceiverChain::install(
            sender.current_chain_id(),
            &sender.rotation_root_bytes(),
            device(0x22),
        )
        .unwrap();
        let c = ctx(0xaa);
        let msg = sender.encrypt(b"bound", &c).unwrap();
        let err = receiver.decrypt(&msg, &c).unwrap_err();
        assert!(format!("{err}").contains("physical_device_id binding mismatch"));
        assert_eq!(receiver.current_n(), 0, "refusal must not advance state");
    }

    #[test]
    fn persisted_sender_chain_without_physical_device_binding_is_refused() {
        let sender = SenderChain::new().unwrap();
        let mut disk = SenderChainOnDisk::from(&sender);
        disk.physical_device_id_b64.clear();
        let err = match SenderChain::try_from(disk) {
            Ok(_) => panic!("missing physical_device_id must be refused"),
            Err(err) => err,
        };
        assert!(matches!(
            err,
            SenderKeyPersistError::BadLength {
                field: "physical_device_id",
                got: 0,
                want: PHYSICAL_DEVICE_ID_BYTES
            }
        ));
    }

    #[test]
    fn sender_key_state_routes_same_peer_by_physical_device_binding() {
        let mut device_a = SenderChain::new_for_physical_device(device(0xa1)).unwrap();
        let mut device_b = SenderChain::new_for_physical_device(device(0xb2)).unwrap();
        let c = ctx(0xcc);
        let mut state = SenderKeyState::new();
        state
            .install_receiver(
                b"same-account".to_vec(),
                device_a.current_chain_id(),
                &device_a.rotation_root_bytes(),
                device_a.physical_device_id(),
            )
            .unwrap();
        state
            .install_receiver(
                b"same-account".to_vec(),
                device_b.current_chain_id(),
                &device_b.rotation_root_bytes(),
                device_b.physical_device_id(),
            )
            .unwrap();

        let msg_b = device_b.encrypt(b"from device b", &c).unwrap();
        assert_eq!(
            state.decrypt_from(b"same-account", &msg_b, &c).unwrap(),
            b"from device b"
        );
        let msg_a = device_a.encrypt(b"from device a", &c).unwrap();
        assert_eq!(
            state.decrypt_from(b"same-account", &msg_a, &c).unwrap(),
            b"from device a"
        );
    }

    #[test]
    fn sender_key_persistence_debug_redacts_keys_and_identifiers() {
        let mut state = SenderKeyState::new();
        state
            .install_sender_for_physical_device(device(0x44))
            .unwrap();
        state
            .sender_chain_mut()
            .unwrap()
            .set_last_known_members(vec![b"member-canary".to_vec()]);
        let chain_id = state.sender_chain().unwrap().current_chain_id();
        let root = state.sender_chain().unwrap().rotation_root_bytes();
        let physical_device_id = state.sender_chain().unwrap().physical_device_id();
        state
            .install_receiver(b"peer-canary".to_vec(), chain_id, &root, physical_device_id)
            .unwrap();

        let disk = SenderKeyStateOnDisk::from(&state);
        let debug = format!("{disk:?}");
        for canary in ["member-canary", "peer-canary"] {
            assert!(!debug.contains(canary), "debug leaked {canary}");
        }
        assert!(!debug.contains(&disk.sender.as_ref().unwrap().rotation_root_b64));
        assert!(!debug.contains(&disk.sender.as_ref().unwrap().ck_n_b64));
        assert!(!debug.contains(&disk.sender.as_ref().unwrap().physical_device_id_b64));
    }
}
