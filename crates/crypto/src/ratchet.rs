//! Double Ratchet — full Signal-style construction with encrypted
//! headers + canonical AD encoding (commit 4 of 4 — ratchet
//! complete).
//!
//! Spec: `docs/design/pqxdh-double-ratchet.md` "Layer 2: Double
//! Ratchet" + Signal's published Double Ratchet specification
//! (<https://signal.org/docs/specifications/doubleratchet/>),
//! "Double Ratchet with header encryption" variant.
//!
//! ## What this module currently implements
//!
//! - [`ChainKey`]: 32-byte chain key, one-way HKDF advance,
//!   per-state message-key derivation.
//! - [`DoubleRatchet`]: full Double Ratchet state machine —
//!   - root-key chain advancing via X25519 DH at every direction
//!     change ("DH ratchet step"),
//!   - separate sending and receiving symmetric chains derived from
//!     the root key on each step,
//!   - **encrypted headers**: every wire message's header (current
//!     `DHs.public`, `prev_chain_length`, chain counter, session
//!     version) is AEAD-encrypted under a header-key chain
//!     (`HKs`/`HKr`/`NHKs`/`NHKr`) that rotates alongside the
//!     message chains,
//!   - **canonical AD encoding**: a length-prefixed tuple binding
//!     both sides' identity keys, conversation id, message ordinal,
//!     previous chain length, and session version (per the design
//!     doc),
//!   - skipped-message-key cache keyed by the HKr at time of skip
//!     plus the chain counter — so out-of-order messages survive
//!     across DH ratchet steps,
//!   - FIFO eviction at [`MAX_SKIPPED_PER_CHAIN`] entries (1000),
//!   - 30-day TTL ([`SKIPPED_KEY_TTL`]) per cached entry.
//! - [`DoubleRatchet::encrypt`] / [`DoubleRatchet::decrypt`]:
//!   per-message AEAD with atomic state commit on AEAD success.
//!
//! ## Initialization
//!
//! Per Signal's HE spec, both sides bootstrap symmetric "shared"
//! header keys from the PQXDH session key:
//!
//! - `SHARED_HKA = HKDF(SK, info=init-hka)` — alice's initial sending
//!   header key (and bob's initial NHKr — the next-receive HK he'll
//!   try when the current HKr fails).
//! - `SHARED_NHKB = HKDF(SK, info=init-nhkb)` — bob's initial NHKs
//!   (becomes his actual sending HK on his first DH ratchet step;
//!   also alice's initial NHKr).
//!
//! - **Initiator (Alice)** — [`DoubleRatchet::new_initiator`]:
//!   takes `(SessionKey, peer_initial_ratchet_pub, SessionContext)`.
//!   Generates own DHs, performs initial DH against peer's bootstrap
//!   pub, and derives `(RK, CKs, NHKs)` from `KDF_RK_HE(SK, DH)`.
//!   Sets `HKs := SHARED_HKA`, `NHKr := SHARED_NHKB`. `CKr`/`HKr`
//!   start `None`.
//! - **Responder (Bob)** — [`DoubleRatchet::new_responder`]:
//!   takes `(SessionKey, initial_ratchet_secret, SessionContext)`.
//!   Stores bootstrap as DHs. `RK := SK`. Sets `NHKs := SHARED_NHKB`,
//!   `NHKr := SHARED_HKA`. `CKs`/`CKr`/`HKs`/`HKr` all `None`.
//!   First send is gated on first receive (the receive-side DH step
//!   populates `HKs` and the sending chain).
//!
//! ## DH ratchet step (receive-side, encrypted headers variant)
//!
//! When a header decrypts under the current `HKr`, it's a normal
//! same-chain delivery. When `HKr` fails, the receiver tries `NHKr`;
//! success there means the peer rotated their DHs and a DH ratchet
//! step is needed:
//!
//! 1. Skip remaining keys on the *old* receiving chain (cached with
//!    the *old* `HKr`) up to `header.prev_chain_length`.
//! 2. `HKs := NHKs` (the next sending HK becomes current).
//! 3. `HKr := NHKr` (the next receiving HK becomes current).
//! 4. `(RK, CKr, NHKr) := KDF_RK_HE(RK, DH(local_DHs.secret, peer_new_DHr))`.
//! 5. Generate a fresh local ratchet keypair.
//! 6. `(RK, CKs, NHKs) := KDF_RK_HE(RK, DH(new_local_DHs.secret, peer_new_DHr))`.
//! 7. Skip on the *new* receiving chain (cached with the *new* `HKr`)
//!    up to `header.counter` — these are messages we may receive
//!    out-of-order on the new chain.
//!
//! All of this lands on tentative state; commit is atomic on AEAD
//! success.
//!
//! ## Canonical AD encoding
//!
//! The associated data fed to the *message* AEAD is a length-prefixed
//! tuple per the design doc:
//!
//! ```text
//! AD = LP(sender_ik_x25519_pub) || LP(sender_ik_mlkem_pub)
//!    || LP(recipient_ik_x25519_pub) || LP(recipient_ik_mlkem_pub)
//!    || LP(conversation_id)
//!    || LP(message_ordinal_u32_be) || LP(prev_chain_length_u32_be)
//!    || LP(session_version_u32_be)
//! ```
//!
//! where `LP(x) = u32_be(x.len()) || x`. The library's actual AAD on
//! the message ciphertext is `AD || enc_header`, binding the
//! ciphertext both to the user's identities/version and to the exact
//! header bytes.
//!
//! `message_ordinal` here equals the chain counter from the decoded
//! header (so both sides derive identical AD: sender from state,
//! receiver from `header.counter`).
//!
//! Receivers explicitly reject any inbound message whose decoded
//! `header.session_version` does not match
//! [`SessionContext::session_version`]: this surfaces a distinct
//! error variant ([`Error::Internal`]) and never reaches the
//! message AEAD step.

use crate::aead;
use crate::error::{Error, Result};
use crate::hkdf;
use crate::pqxdh::SessionKey;
use crate::random;
use crate::x25519;
use std::time::{Duration, SystemTime};
use zeroize::ZeroizeOnDrop;

/// HKDF info labels — domain-separated for v1.
const SHARED_HKA_INFO: &[u8] = b"discord-privacy-client/ratchet/init-hka/v1";
const SHARED_NHKB_INFO: &[u8] = b"discord-privacy-client/ratchet/init-nhkb/v1";
const ROOT_KDF_HE_INFO: &[u8] = b"discord-privacy-client/ratchet/root-kdf-he/v1";
const CHAIN_ADVANCE_INFO: &[u8] = b"discord-privacy-client/ratchet/chain-step/v1";
const MESSAGE_KEY_INFO: &[u8] = b"discord-privacy-client/ratchet/msg-key/v1";

/// Wire protocol version. Embedded in every header and AD; receivers
/// reject mismatches.
pub const SESSION_VERSION_V1: u32 = 1;

/// Hard cap on cached skipped message keys (global across DH chains).
pub const MAX_SKIPPED_PER_CHAIN: usize = 1000;

/// TTL for cached skipped message keys.
pub const SKIPPED_KEY_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Plaintext header layout: 32-byte `dh_pub` || u32_be(`PN`) ||
/// u32_be(`N`) || u32_be(`session_version`).
const HEADER_BYTES: usize = 32 + 4 + 4 + 4;

/// 32-byte symmetric ratchet chain key. Advances one-way via HKDF.
#[derive(Clone, ZeroizeOnDrop)]
pub struct ChainKey([u8; 32]);

impl ChainKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ChainKey(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive the message key for the current chain state, **without
    /// advancing**. Repeated calls without `advance` produce the
    /// same message key.
    pub fn message_key(&self) -> Result<aead::Key> {
        let bytes = hkdf::derive_32(&[], &self.0, MESSAGE_KEY_INFO)?;
        Ok(aead::Key::from_bytes(bytes))
    }

    /// Advance the chain to its next state via one HKDF step.
    pub fn advance(&mut self) -> Result<()> {
        let next = hkdf::derive_32(&[], &self.0, CHAIN_ADVANCE_INFO)?;
        self.0 = next;
        Ok(())
    }
}

/// 32-byte root key. Zeroizes on drop. Internal use only.
#[derive(Clone, ZeroizeOnDrop)]
struct RootKey([u8; 32]);

impl RootKey {
    fn from_bytes(b: [u8; 32]) -> Self {
        RootKey(b)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn replace(&mut self, b: [u8; 32]) {
        self.0 = b;
    }
}

/// `KDF_RK_HE` per Signal's encrypted-headers spec — HKDF-SHA256
/// expand to 96 bytes, split into
/// `(new_root_key, new_chain_key, next_header_key)`.
fn kdf_rk_he(
    root_key: &[u8; 32],
    dh_out: &[u8; 32],
) -> Result<([u8; 32], [u8; 32], [u8; 32])> {
    let bytes = hkdf::derive(root_key, dh_out, ROOT_KDF_HE_INFO, 96)?;
    let mut new_rk = [0u8; 32];
    let mut new_ck = [0u8; 32];
    let mut new_nhk = [0u8; 32];
    new_rk.copy_from_slice(&bytes[..32]);
    new_ck.copy_from_slice(&bytes[32..64]);
    new_nhk.copy_from_slice(&bytes[64..]);
    Ok((new_rk, new_ck, new_nhk))
}

/// Plaintext header carried (encrypted) on every wire message.
#[derive(Clone, Debug)]
struct Header {
    dh_pub: x25519::PublicKey,
    prev_chain_length: u32,
    counter: u32,
    session_version: u32,
}

impl Header {
    fn serialize(&self) -> [u8; HEADER_BYTES] {
        let mut out = [0u8; HEADER_BYTES];
        out[..32].copy_from_slice(self.dh_pub.as_bytes());
        out[32..36].copy_from_slice(&self.prev_chain_length.to_be_bytes());
        out[36..40].copy_from_slice(&self.counter.to_be_bytes());
        out[40..44].copy_from_slice(&self.session_version.to_be_bytes());
        out
    }

    fn deserialize(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != HEADER_BYTES {
            return Err(Error::Internal(format!(
                "ratchet header: wrong length (got {}, want {})",
                bytes.len(),
                HEADER_BYTES
            )));
        }
        let mut dh_bytes = [0u8; 32];
        dh_bytes.copy_from_slice(&bytes[..32]);
        let prev_chain_length = u32::from_be_bytes(bytes[32..36].try_into().unwrap());
        let counter = u32::from_be_bytes(bytes[36..40].try_into().unwrap());
        let session_version = u32::from_be_bytes(bytes[40..44].try_into().unwrap());
        Ok(Header {
            dh_pub: x25519::PublicKey::from_bytes(dh_bytes),
            prev_chain_length,
            counter,
            session_version,
        })
    }
}

/// Canonical length-prefixed AD encoding per the design doc:
///
/// ```text
/// AD = LP(sender_ik_x25519_pub) || LP(sender_ik_mlkem_pub)
///    || LP(recipient_ik_x25519_pub) || LP(recipient_ik_mlkem_pub)
///    || LP(conversation_id)
///    || LP(u32_be(message_ordinal)) || LP(u32_be(prev_chain_length))
///    || LP(u32_be(session_version))
/// ```
///
/// where `LP(x) = u32_be(x.len()) || x`. Deterministic for fixed
/// inputs.
pub fn canonical_ad(
    sender_ik_x25519: &[u8; 32],
    sender_ik_mlkem: &[u8],
    recipient_ik_x25519: &[u8; 32],
    recipient_ik_mlkem: &[u8],
    conversation_id: &[u8],
    message_ordinal: u32,
    prev_chain_length: u32,
    session_version: u32,
) -> Vec<u8> {
    let mut buf = Vec::new();
    write_lp(&mut buf, sender_ik_x25519);
    write_lp(&mut buf, sender_ik_mlkem);
    write_lp(&mut buf, recipient_ik_x25519);
    write_lp(&mut buf, recipient_ik_mlkem);
    write_lp(&mut buf, conversation_id);
    write_lp(&mut buf, &message_ordinal.to_be_bytes());
    write_lp(&mut buf, &prev_chain_length.to_be_bytes());
    write_lp(&mut buf, &session_version.to_be_bytes());
    buf
}

fn write_lp(buf: &mut Vec<u8>, bytes: &[u8]) {
    buf.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    buf.extend_from_slice(bytes);
}

/// Identity + conversation parameters used for the canonical AD
/// encoding on every encrypt/decrypt call. The library stores one of
/// these per ratchet (the local view: own identity is "local", peer's
/// is "peer"); the AD render swaps sender/recipient depending on
/// direction.
#[derive(Clone)]
pub struct SessionContext {
    pub local_ik_x25519_pub: x25519::PublicKey,
    pub local_ik_mlkem_pub: Vec<u8>,
    pub peer_ik_x25519_pub: x25519::PublicKey,
    pub peer_ik_mlkem_pub: Vec<u8>,
    pub conversation_id: Vec<u8>,
    pub session_version: u32,
}

/// One cached skipped (header-key, message-key) pair.
///
/// The header key (`hk`) is what the sender used for the wire header
/// at the time the slot was skipped. On lookup, the receiver tries
/// each cached `hk` against the current message's `enc_header`; the
/// matching entry's counter and `mk` are the ones to use.
#[derive(Clone)]
struct SkippedKey {
    hk: aead::Key,
    counter: u32,
    mk: aead::Key,
    inserted_at: SystemTime,
}

#[derive(Clone, Default)]
struct SkippedKeyCache {
    keys: Vec<SkippedKey>,
}

impl SkippedKeyCache {
    fn sweep_expired(&mut self, now: SystemTime) {
        self.keys.retain(|k| match now.duration_since(k.inserted_at) {
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

/// Full Signal-style Double Ratchet with encrypted headers.
#[derive(Clone)]
pub struct DoubleRatchet {
    root_key: RootKey,
    dhs_secret: x25519::SecretKey,
    dhs_pub: x25519::PublicKey,
    dhr: Option<x25519::PublicKey>,
    sending_chain: Option<ChainKey>,
    sending_counter: u32,
    receiving_chain: Option<ChainKey>,
    receiving_counter: u32,
    prev_sending_count: u32,

    /// Current sending-side header key. `None` for a freshly-initialized
    /// responder; populated on his first DH ratchet step.
    hks: Option<aead::Key>,
    /// Current receiving-side header key. `None` until the first DH
    /// ratchet step on receive populates it.
    hkr: Option<aead::Key>,
    /// Next sending-side header key — promoted to `hks` on each DH
    /// ratchet step.
    nhks: aead::Key,
    /// Next receiving-side header key — promoted to `hkr` on each DH
    /// ratchet step. Used as the fallback header-decryption key when
    /// `hkr` fails.
    nhkr: aead::Key,

    skipped: SkippedKeyCache,

    ctx: SessionContext,
}

impl DoubleRatchet {
    /// Initialize the initiator side.
    pub fn new_initiator(
        sk: &SessionKey,
        peer_initial_ratchet_pub: &x25519::PublicKey,
        ctx: SessionContext,
    ) -> Result<Self> {
        let shared_hka_bytes = hkdf::derive_32(&[], sk.as_bytes(), SHARED_HKA_INFO)?;
        let shared_nhkb_bytes = hkdf::derive_32(&[], sk.as_bytes(), SHARED_NHKB_INFO)?;
        let (dhs_secret, dhs_pub) = x25519::generate_keypair();
        let dh = x25519::diffie_hellman(&dhs_secret, peer_initial_ratchet_pub)?;
        let (rk, cks_bytes, nhks_bytes) = kdf_rk_he(sk.as_bytes(), dh.as_bytes())?;
        Ok(DoubleRatchet {
            root_key: RootKey::from_bytes(rk),
            dhs_secret,
            dhs_pub,
            dhr: Some(*peer_initial_ratchet_pub),
            sending_chain: Some(ChainKey::from_bytes(cks_bytes)),
            sending_counter: 0,
            receiving_chain: None,
            receiving_counter: 0,
            prev_sending_count: 0,
            hks: Some(aead::Key::from_bytes(shared_hka_bytes)),
            hkr: None,
            nhks: aead::Key::from_bytes(nhks_bytes),
            nhkr: aead::Key::from_bytes(shared_nhkb_bytes),
            skipped: SkippedKeyCache::default(),
            ctx,
        })
    }

    /// Initialize the responder side. The responder cannot send before
    /// receiving the initiator's first message.
    pub fn new_responder(
        sk: &SessionKey,
        initial_ratchet_secret: &x25519::SecretKey,
        ctx: SessionContext,
    ) -> Result<Self> {
        let shared_hka_bytes = hkdf::derive_32(&[], sk.as_bytes(), SHARED_HKA_INFO)?;
        let shared_nhkb_bytes = hkdf::derive_32(&[], sk.as_bytes(), SHARED_NHKB_INFO)?;
        let dhs_pub = x25519::derive_public(initial_ratchet_secret);
        Ok(DoubleRatchet {
            root_key: RootKey::from_bytes(*sk.as_bytes()),
            dhs_secret: initial_ratchet_secret.clone(),
            dhs_pub,
            dhr: None,
            sending_chain: None,
            sending_counter: 0,
            receiving_chain: None,
            receiving_counter: 0,
            prev_sending_count: 0,
            hks: None,
            hkr: None,
            nhks: aead::Key::from_bytes(shared_nhkb_bytes),
            nhkr: aead::Key::from_bytes(shared_hka_bytes),
            skipped: SkippedKeyCache::default(),
            ctx,
        })
    }

    pub fn sending_counter(&self) -> u32 {
        self.sending_counter
    }

    pub fn receiving_counter(&self) -> u32 {
        self.receiving_counter
    }

    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    /// Test/diagnostic-only: current sending header key bytes, if any.
    /// Exposed so integration tests can directly verify HK rotation
    /// behavior across DH ratchet steps.
    #[doc(hidden)]
    pub fn dbg_hks_bytes(&self) -> Option<[u8; 32]> {
        self.hks.as_ref().map(|k| *k.as_bytes())
    }

    /// Test/diagnostic-only: current receiving header key bytes, if any.
    #[doc(hidden)]
    pub fn dbg_hkr_bytes(&self) -> Option<[u8; 32]> {
        self.hkr.as_ref().map(|k| *k.as_bytes())
    }

    /// Encrypt `plaintext` under the current sending message and
    /// header keys. The header (DHs.public, PN, N, session_version)
    /// is AEAD-encrypted with `HKs` and a fresh nonce; the message
    /// is AEAD-encrypted with `MK` and an AAD that binds the
    /// canonical AD encoding plus the encrypted header bytes.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedMessage> {
        let cks = self.sending_chain.as_mut().ok_or_else(|| {
            Error::Internal(
                "ratchet encrypt: no sending chain (responder must receive at least one \
                 message before sending)"
                    .into(),
            )
        })?;
        let hks = self.hks.as_ref().ok_or_else(|| {
            Error::Internal(
                "ratchet encrypt: no sending header key (responder must receive at least \
                 one message before sending)"
                    .into(),
            )
        })?;

        let mk = cks.message_key()?;
        cks.advance()?;
        let counter = self.sending_counter;
        self.sending_counter = self
            .sending_counter
            .checked_add(1)
            .ok_or_else(|| Error::Internal("ratchet: sending counter overflow".into()))?;

        let header = Header {
            dh_pub: self.dhs_pub,
            prev_chain_length: self.prev_sending_count,
            counter,
            session_version: self.ctx.session_version,
        };
        let header_bytes = header.serialize();
        let header_nonce = random::random_nonce();
        let enc_header = aead::seal(hks, &header_nonce, b"", &header_bytes)?;

        let message_nonce = random::random_nonce();
        let mut full_ad = render_outbound_ad(&self.ctx, &header);
        full_ad.extend_from_slice(&enc_header);
        let ciphertext = aead::seal(&mk, &message_nonce, &full_ad, plaintext)?;

        Ok(EncryptedMessage {
            header_nonce,
            enc_header,
            message_nonce,
            ciphertext,
        })
    }

    /// Decrypt `msg` using the system clock for cache TTL.
    pub fn decrypt(&mut self, msg: &EncryptedMessage) -> Result<Vec<u8>> {
        self.decrypt_at(msg, SystemTime::now())
    }

    /// Decrypt `msg` with an explicit `now` for cache-TTL expiry.
    /// Tests use this to simulate the passage of time.
    ///
    /// Order of operations:
    ///
    /// 1. Sweep expired entries from the skipped-key cache.
    /// 2. Try the cache: for each cached entry, attempt to decrypt
    ///    the wire `enc_header` with the entry's `hk`. If success
    ///    and the decoded header counter matches the entry counter,
    ///    use the cached `mk` to decrypt the ciphertext (consume on
    ///    success).
    /// 3. Try the current `HKr` to decrypt the header. Success
    ///    means same-chain delivery; bound-check, gap-fill skipped
    ///    keys on the current chain, decrypt.
    /// 4. Else try `NHKr` to decrypt the header. Success here
    ///    triggers a DH ratchet step.
    /// 5. Validate `header.session_version` against
    ///    `self.ctx.session_version`. Mismatch → reject before any
    ///    state mutation.
    /// 6. Run the DH ratchet step (or same-chain gap-fill) on
    ///    tentative state. AEAD decrypt the ciphertext; on success
    ///    commit; on failure, no state changes.
    pub fn decrypt_at(&mut self, msg: &EncryptedMessage, now: SystemTime) -> Result<Vec<u8>> {
        self.skipped.sweep_expired(now);

        // 1) Try skipped cache.
        if let Some(plaintext) = self.try_decrypt_skipped(msg)? {
            return Ok(plaintext);
        }

        // 2) Try current HKr (same chain).
        let header_via_hkr = self.hkr.as_ref().and_then(|hkr| {
            aead::open(hkr, &msg.header_nonce, b"", &msg.enc_header)
                .ok()
                .and_then(|bytes| Header::deserialize(&bytes).ok())
        });

        // 3) Else try NHKr (DH ratchet step).
        let (header, is_dh_step) = match header_via_hkr {
            Some(h) => (h, false),
            None => {
                let bytes =
                    aead::open(&self.nhkr, &msg.header_nonce, b"", &msg.enc_header).map_err(
                        |_| {
                            Error::Internal(
                                "ratchet decrypt: header AEAD failed (neither HKr nor NHKr \
                             could open the wire header)"
                                    .into(),
                            )
                        },
                    )?;
                let h = Header::deserialize(&bytes)?;
                (h, true)
            }
        };

        // 4) Reject session_version mismatches before touching state.
        if header.session_version != self.ctx.session_version {
            return Err(Error::Internal(format!(
                "ratchet decrypt: session_version mismatch (header={}, expected={})",
                header.session_version, self.ctx.session_version
            )));
        }

        if is_dh_step {
            self.decrypt_dh_step(msg, &header, now)
        } else {
            self.decrypt_same_chain(msg, &header, now)
        }
    }

    /// Linear scan over cached entries: for each, try to decrypt the
    /// wire `enc_header` with the entry's `hk`; if the decoded header
    /// counter matches, attempt the message AEAD with the cached
    /// `mk`. Returns `Some(plaintext)` on a full match (cache entry
    /// is consumed); `None` when nothing in the cache decrypts the
    /// header.
    fn try_decrypt_skipped(&mut self, msg: &EncryptedMessage) -> Result<Option<Vec<u8>>> {
        let mut matched_idx: Option<usize> = None;
        let mut matched_header: Option<Header> = None;
        for (idx, entry) in self.skipped.keys.iter().enumerate() {
            if let Ok(header_bytes) =
                aead::open(&entry.hk, &msg.header_nonce, b"", &msg.enc_header)
            {
                if let Ok(header) = Header::deserialize(&header_bytes) {
                    if header.counter == entry.counter {
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

        if header.session_version != self.ctx.session_version {
            return Err(Error::Internal(format!(
                "ratchet decrypt: session_version mismatch (header={}, expected={})",
                header.session_version, self.ctx.session_version
            )));
        }

        let mk = self.skipped.keys[idx].mk.clone();
        let mut full_ad = render_inbound_ad(&self.ctx, &header);
        full_ad.extend_from_slice(&msg.enc_header);
        let plaintext = aead::open(&mk, &msg.message_nonce, &full_ad, &msg.ciphertext)?;
        self.skipped.keys.remove(idx);
        Ok(Some(plaintext))
    }

    fn decrypt_same_chain(
        &mut self,
        msg: &EncryptedMessage,
        header: &Header,
        now: SystemTime,
    ) -> Result<Vec<u8>> {
        if header.counter < self.receiving_counter {
            return Err(Error::Internal(format!(
                "ratchet decrypt: counter {} < receiving_counter {} \
                 (replay or evicted/expired skipped key)",
                header.counter, self.receiving_counter
            )));
        }
        let gap = header.counter - self.receiving_counter;
        if (gap as usize) > MAX_SKIPPED_PER_CHAIN {
            return Err(Error::Internal(format!(
                "ratchet decrypt: gap of {} skipped messages exceeds cap {}",
                gap, MAX_SKIPPED_PER_CHAIN
            )));
        }

        let recv_chain = self.receiving_chain.as_ref().ok_or_else(|| {
            Error::Internal(
                "ratchet decrypt: same-chain path with no receiving chain (impossible)"
                    .into(),
            )
        })?;
        let hkr = self.hkr.as_ref().ok_or_else(|| {
            Error::Internal(
                "ratchet decrypt: same-chain path with no HKr (impossible)".into(),
            )
        })?;
        let mut tentative_chain = recv_chain.clone();
        let mut tentative_counter = self.receiving_counter;
        let mut new_skipped: Vec<SkippedKey> = Vec::with_capacity(gap as usize);
        for _ in 0..gap {
            let mk = tentative_chain.message_key()?;
            new_skipped.push(SkippedKey {
                hk: hkr.clone(),
                counter: tentative_counter,
                mk,
                inserted_at: now,
            });
            tentative_chain.advance()?;
            tentative_counter = tentative_counter
                .checked_add(1)
                .ok_or_else(|| Error::Internal("ratchet: receiving counter overflow".into()))?;
        }

        let mk = tentative_chain.message_key()?;
        let mut full_ad = render_inbound_ad(&self.ctx, header);
        full_ad.extend_from_slice(&msg.enc_header);
        let plaintext = aead::open(&mk, &msg.message_nonce, &full_ad, &msg.ciphertext)?;
        tentative_chain.advance()?;
        tentative_counter = tentative_counter
            .checked_add(1)
            .ok_or_else(|| Error::Internal("ratchet: receiving counter overflow".into()))?;

        self.receiving_chain = Some(tentative_chain);
        self.receiving_counter = tentative_counter;
        for entry in new_skipped {
            self.skipped.insert(entry);
        }
        Ok(plaintext)
    }

    fn decrypt_dh_step(
        &mut self,
        msg: &EncryptedMessage,
        header: &Header,
        now: SystemTime,
    ) -> Result<Vec<u8>> {
        // Pre-bound checks — reject before any state mutation.
        if self.receiving_chain.is_some() {
            let old_skip = header
                .prev_chain_length
                .saturating_sub(self.receiving_counter);
            if (old_skip as usize) > MAX_SKIPPED_PER_CHAIN {
                return Err(Error::Internal(format!(
                    "ratchet decrypt: would skip {} keys on previous chain (cap {})",
                    old_skip, MAX_SKIPPED_PER_CHAIN
                )));
            }
        }
        if (header.counter as usize) > MAX_SKIPPED_PER_CHAIN {
            return Err(Error::Internal(format!(
                "ratchet decrypt: would skip {} keys on new chain (cap {})",
                header.counter, MAX_SKIPPED_PER_CHAIN
            )));
        }

        // Tentative state: nothing below mutates `self` until commit.
        let mut tentative_old_chain = self.receiving_chain.clone();
        let mut tentative_old_counter = self.receiving_counter;
        let mut new_skipped: Vec<SkippedKey> = Vec::new();

        // Phase 1: skip remaining keys on the OLD receiving chain
        // (cached with the OLD HKr).
        if let (Some(chain), Some(hkr)) =
            (tentative_old_chain.as_mut(), self.hkr.as_ref())
        {
            while tentative_old_counter < header.prev_chain_length {
                let mk = chain.message_key()?;
                new_skipped.push(SkippedKey {
                    hk: hkr.clone(),
                    counter: tentative_old_counter,
                    mk,
                    inserted_at: now,
                });
                chain.advance()?;
                tentative_old_counter = tentative_old_counter
                    .checked_add(1)
                    .ok_or_else(|| Error::Internal("ratchet: receiving counter overflow".into()))?;
            }
        }

        // Phase 2: tentative DH ratchet step.
        let new_dhr = header.dh_pub;
        let dh1 = x25519::diffie_hellman(&self.dhs_secret, &new_dhr)?;
        let (rk1, ckr_bytes, new_nhkr_bytes) =
            kdf_rk_he(self.root_key.as_bytes(), dh1.as_bytes())?;
        let (new_dhs_secret, new_dhs_pub) = x25519::generate_keypair();
        let dh2 = x25519::diffie_hellman(&new_dhs_secret, &new_dhr)?;
        let (rk2, cks_bytes, new_nhks_bytes) = kdf_rk_he(&rk1, dh2.as_bytes())?;

        // Per Signal's HE spec, on DH step:
        //   HKs := old NHKs
        //   HKr := old NHKr
        //   NHKs, NHKr := newly derived above.
        // The "new HKr" used for caching skipped keys on the new
        // chain is therefore the old NHKr — captured via reference
        // here, cloned once per skipped slot below.
        let new_hkr_for_caching = self.nhkr.clone();

        // Phase 3: skip on NEW receiving chain (cached with the new
        // HKr) and decrypt the current message — all on tentative.
        let mut tentative_new_recv = ChainKey::from_bytes(ckr_bytes);
        let mut tentative_new_recv_counter: u32 = 0;
        for slot in 0..header.counter {
            let mk = tentative_new_recv.message_key()?;
            new_skipped.push(SkippedKey {
                hk: new_hkr_for_caching.clone(),
                counter: slot,
                mk,
                inserted_at: now,
            });
            tentative_new_recv.advance()?;
            tentative_new_recv_counter = tentative_new_recv_counter
                .checked_add(1)
                .ok_or_else(|| Error::Internal("ratchet: receiving counter overflow".into()))?;
        }

        let mk = tentative_new_recv.message_key()?;
        let mut full_ad = render_inbound_ad(&self.ctx, header);
        full_ad.extend_from_slice(&msg.enc_header);
        let plaintext = aead::open(&mk, &msg.message_nonce, &full_ad, &msg.ciphertext)?;
        tentative_new_recv.advance()?;
        tentative_new_recv_counter = tentative_new_recv_counter
            .checked_add(1)
            .ok_or_else(|| Error::Internal("ratchet: receiving counter overflow".into()))?;

        // COMMIT — atomic.
        self.prev_sending_count = self.sending_counter;
        self.sending_counter = 0;
        self.receiving_counter = tentative_new_recv_counter;
        self.dhr = Some(new_dhr);
        self.root_key.replace(rk2);
        self.dhs_secret = new_dhs_secret;
        self.dhs_pub = new_dhs_pub;
        self.sending_chain = Some(ChainKey::from_bytes(cks_bytes));
        self.receiving_chain = Some(tentative_new_recv);

        // Promote NHKs/NHKr → HKs/HKr; install fresh NHKs/NHKr.
        let promoted_hks = std::mem::replace(
            &mut self.nhks,
            aead::Key::from_bytes(new_nhks_bytes),
        );
        let promoted_hkr = std::mem::replace(
            &mut self.nhkr,
            aead::Key::from_bytes(new_nhkr_bytes),
        );
        self.hks = Some(promoted_hks);
        self.hkr = Some(promoted_hkr);

        for entry in new_skipped {
            self.skipped.insert(entry);
        }
        Ok(plaintext)
    }
}

/// Sender-side AD: local identity is the sender, peer is the recipient.
fn render_outbound_ad(ctx: &SessionContext, header: &Header) -> Vec<u8> {
    canonical_ad(
        ctx.local_ik_x25519_pub.as_bytes(),
        &ctx.local_ik_mlkem_pub,
        ctx.peer_ik_x25519_pub.as_bytes(),
        &ctx.peer_ik_mlkem_pub,
        &ctx.conversation_id,
        header.counter,
        header.prev_chain_length,
        header.session_version,
    )
}

/// Receiver-side AD: peer identity is the sender, local is the recipient.
fn render_inbound_ad(ctx: &SessionContext, header: &Header) -> Vec<u8> {
    canonical_ad(
        ctx.peer_ik_x25519_pub.as_bytes(),
        &ctx.peer_ik_mlkem_pub,
        ctx.local_ik_x25519_pub.as_bytes(),
        &ctx.local_ik_mlkem_pub,
        &ctx.conversation_id,
        header.counter,
        header.prev_chain_length,
        header.session_version,
    )
}

/// On-the-wire encrypted ratchet message.
///
/// The header (DHs.public, PN, N, session_version) is AEAD-encrypted
/// under HKs and rides as opaque ciphertext alongside its random
/// nonce. The receiver tries HKr first, then NHKr (which signals a
/// DH ratchet step). Skipped-key cache entries also store the HKr at
/// time-of-skip so old-chain headers can still be opened.
#[derive(Clone, Debug)]
pub struct EncryptedMessage {
    /// 24-byte XChaCha20-Poly1305 nonce used for the header AEAD.
    pub header_nonce: aead::Nonce,
    /// AEAD ciphertext of the serialized [`Header`] (HKs, no AAD).
    pub enc_header: Vec<u8>,
    /// 24-byte XChaCha20-Poly1305 nonce used for the message AEAD.
    pub message_nonce: aead::Nonce,
    /// AEAD ciphertext of the plaintext payload (MK, AAD = canonical
    /// AD || `enc_header`).
    pub ciphertext: Vec<u8>,
}
