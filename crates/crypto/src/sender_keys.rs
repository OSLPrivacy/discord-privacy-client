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
//! header_plaintext = u32_be(chain_id)
//!                 || u32_be(n)
//!                 || u32_be(prev_chain_length)
//!                 || u32_be(session_version)        // 16 bytes
//! enc_header       = AEAD-encrypt(HK_n, header_nonce, "", header_plaintext)
//! AD               = canonical_ad_sender_keys(sender_ik..., group_id, ...)
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
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use zeroize::ZeroizeOnDrop;

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

/// TTL for cached skipped message keys.
pub const SKIPPED_KEY_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Plaintext header layout: u32_be(chain_id) || u32_be(n) ||
/// u32_be(prev_chain_length) || u32_be(session_version).
pub const HEADER_BYTES: usize = 16;

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
    pub chain_id: u32,
    pub n: u32,
    pub prev_chain_length: u32,
    pub session_version: u32,
}

impl Header {
    /// Fixed 16-byte serialization:
    /// `u32_be(chain_id) || u32_be(n) || u32_be(prev_chain_length)
    ///  || u32_be(session_version)`.
    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut out = [0u8; HEADER_BYTES];
        out[..4].copy_from_slice(&self.chain_id.to_be_bytes());
        out[4..8].copy_from_slice(&self.n.to_be_bytes());
        out[8..12].copy_from_slice(&self.prev_chain_length.to_be_bytes());
        out[12..16].copy_from_slice(&self.session_version.to_be_bytes());
        out
    }

    /// Parse 16 fixed bytes back into a [`Header`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != HEADER_BYTES {
            return Err(Error::Internal(format!(
                "sender keys header: wrong length (got {}, want {})",
                bytes.len(),
                HEADER_BYTES
            )));
        }
        Ok(Header {
            chain_id: u32::from_be_bytes(bytes[..4].try_into().unwrap()),
            n: u32::from_be_bytes(bytes[4..8].try_into().unwrap()),
            prev_chain_length: u32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            session_version: u32::from_be_bytes(bytes[12..16].try_into().unwrap()),
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
///    || LP(group_id)
///    || LP(u32_be(chain_id)) || LP(u32_be(n))
///    || LP(u32_be(prev_chain_length))
///    || LP(u32_be(session_version))
/// ```
///
/// where `LP(x) = u32_be(x.len()) || x`. Deterministic for fixed
/// inputs.
pub fn canonical_ad_sender_keys(
    sender_ik_x25519_pub: &[u8; 32],
    sender_ik_mlkem_pub: &[u8],
    group_id: &[u8],
    chain_id: u32,
    n: u32,
    prev_chain_length: u32,
    session_version: u32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, sender_ik_x25519_pub);
    write_lp(&mut buf, sender_ik_mlkem_pub);
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
    rotation_root: RotationRoot,
    chain_id: u32,
    ck_n: SenderChainKey,
    n: u32,
    prev_chain_length: u32,
}

impl SenderChain {
    /// Create a fresh sender chain at `chain_id = 0` with a CSPRNG
    /// rotation root.
    pub fn new() -> Result<Self> {
        let rotation_root = RotationRoot::random();
        let ck_0 = derive_ck_0(&rotation_root, 0)?;
        Ok(SenderChain {
            rotation_root,
            chain_id: 0,
            ck_n: ck_0,
            n: 0,
            prev_chain_length: 0,
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
        self.rotation_root = RotationRoot::random();
        self.chain_id = new_chain_id;
        self.ck_n = derive_ck_0(&self.rotation_root, self.chain_id)?;
        self.n = 0;
        self.prev_chain_length = prev;
        Ok(())
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
    chain_id: u32,
    ck_n: SenderChainKey,
    n: u32,
    skipped: SkippedKeyCache,
}

impl ReceiverChain {
    /// Install a fresh chain for a peer-sender. The
    /// `(chain_id, rotation_root)` tuple comes from the pairwise
    /// ratchet (delivery is out-of-scope for this crate).
    pub fn install(chain_id: u32, rotation_root: &[u8; 32]) -> Result<Self> {
        let root = RotationRoot::from_bytes(*rotation_root);
        let ck_0 = derive_ck_0(&root, chain_id)?;
        Ok(ReceiverChain {
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
                "sender keys decrypt: no matching slot within MAX_SKIPPED_PER_CHAIN ({}) \
                 iterations (replay, oversized gap, unknown chain, or tampered header)",
                MAX_SKIPPED_PER_CHAIN
            ))
        })?;

        // Decrypt the message ciphertext.
        let mut full_ad = canonical_ad_sender_keys(
            ctx.sender_ik_x25519_pub.as_bytes(),
            &ctx.sender_ik_mlkem_pub,
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

        let mk = self.skipped.keys[idx].mk.clone();
        let mut full_ad = canonical_ad_sender_keys(
            ctx.sender_ik_x25519_pub.as_bytes(),
            &ctx.sender_ik_mlkem_pub,
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
/// `peer_id → ReceiverChain` for incoming senders.
pub struct SenderKeyState {
    sender: Option<SenderChain>,
    receivers: HashMap<Vec<u8>, ReceiverChain>,
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
    ) -> Result<()> {
        let chain = ReceiverChain::install(chain_id, rotation_root)?;
        self.receivers.insert(peer_id, chain);
        Ok(())
    }

    /// Rotate an existing peer's receiver chain to a new
    /// `(chain_id, rotation_root)`. The pre-rotation skipped-key
    /// cache is retained.
    pub fn rotate_receiver(
        &mut self,
        peer_id: &[u8],
        chain_id: u32,
        rotation_root: &[u8; 32],
    ) -> Result<()> {
        let chain = self
            .receivers
            .get_mut(peer_id)
            .ok_or_else(|| Error::Internal("sender keys: no receiver chain for peer".into()))?;
        chain.rotate_to(chain_id, rotation_root)
    }

    pub fn receiver_chain(&self, peer_id: &[u8]) -> Option<&ReceiverChain> {
        self.receivers.get(peer_id)
    }

    pub fn receiver_chain_mut(&mut self, peer_id: &[u8]) -> Option<&mut ReceiverChain> {
        self.receivers.get_mut(peer_id)
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
        let chain = self
            .receivers
            .get_mut(peer_id)
            .ok_or_else(|| Error::Internal("sender keys: no receiver chain for peer".into()))?;
        chain.decrypt(msg, ctx)
    }
}
