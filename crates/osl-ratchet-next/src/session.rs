//! The session: header-encrypted Double Ratchet with a hybrid
//! (X25519 + fragmented ML-KEM-768) root ratchet.
//!
//! # Wire format, version `0x10`
//!
//! ```text
//! DPC0::base64(
//!     version(1) = 0x10
//!     flags(1)                     bit0 = BOOTSTRAP preamble present
//!   [ preamble                     only when BOOTSTRAP: 1153 bytes ]
//!     header_nonce(12)
//!     header_ct_len(varint) header_ct(..)
//!     body_ct(..)                  to end of buffer
//! )
//! ```
//!
//! The version byte occupies a slot the existing decoders leave free
//! (`wire_v2` defines `0x02`..`0x05`), so a receiver routes on
//! `raw[0]` exactly as it does today and this protocol coexists with
//! the current wire rather than replacing it. See `MIGRATION.md`.
//!
//! ## Encrypted header
//!
//! ```text
//! msg_type(1) dh_pub(32) pn(varint) n(varint)
//! pq_have(varint) mix_epoch(varint)
//! has_fragment(1) [ fragment ]
//! ```
//!
//! Everything a passive observer could otherwise use to link or
//! order messages — the ratchet public key, both counters, the
//! message type, the PQ epoch state — is inside this AEAD. Signal
//! specifies this variant but does not ship it; `wire_v2`'s v=3 path
//! leaves the sender's identity key and message type in the clear.
//!
//! ## Associated data chains the three regions together
//!
//! ```text
//! AD_header = "OSL-RN/v1/header" || version || flags || preamble?
//! AD_body   = "OSL-RN/v1/body"   || version || flags || preamble?
//!                                || header_nonce || header_ct
//! ```
//!
//! So the body is bound to the exact header bytes and the header to
//! the exact preamble. Any single-bit edit anywhere in the blob fails
//! one of the two tags. There is no unauthenticated byte on the wire
//! except the version and flag bytes, and both are covered by both
//! tags.
//!
//! ## Transactional decryption
//!
//! `decrypt` mutates a **clone** of the ratchet state and commits only
//! if the body tag verifies. A tampered ciphertext therefore cannot
//! advance, corrupt, or wedge the session — it is a pure no-op plus an
//! error. The clone is taken only *after* the header AEAD has
//! authenticated the message, so an attacker who cannot forge a header
//! cannot induce the allocation.

use crate::codec::{Reader, Writer};
use crate::error::{Error, Result};
use crate::handshake::{self, HandshakeOutput, LocalPrekeys, PeerBundle, Preamble};
use crate::kdf;
use crate::pq::{Fragment, PqParams, PqRatchet, Role};
use crate::primitives::{
    aead_open, aead_seal, dh, x25519_keypair, Secret32, XPublic, XSecret, AEAD_KEY,
    HEADER_NONCE_WIRE,
};
use crate::skipped::{SkipParams, SkippedKeys};
use rand_core::{CryptoRng, RngCore};

/// Wire version byte for this protocol: **`0x10`**.
///
/// # Why not `0x06`, and why not merely "the next free version"
///
/// This crate originally claimed `0x06`, reasoning that `wire_v2`
/// assigns wire versions `0x02..=0x05` so `0x06` is next. That is
/// correct *within the wire-version namespace*, and it is why `0x06` was
/// never a live protocol conflict: `wire_v2::MSG_TYPE_SKDM_REQUEST =
/// 0x06` lives in a different namespace (the message-type byte) at a
/// different offset.
///
/// It is nonetheless the wrong number, for a reason that only surfaces
/// at integration: **this product inspects raw carrier bytes at fixed
/// offsets without knowing the version.**
/// `wire_v2::is_native_overlay_relay_bundle` and its siblings classify an
/// opaque inbox bundle with
/// `bundle[0] == WIRE_VERSION_V3 && bundle[1] == MSG_TYPE_*`. Byte 0 is a
/// version and byte 1 is a message type, but nothing in the type system
/// says so and both are drawn from small overlapping integer ranges.
///
/// So this protocol takes a value free in **both** namespaces and above
/// everything assigned in either.
///
/// ## Authoritative assignment map (audited across both namespaces)
///
/// | Byte | Name | Status |
/// | --- | --- | --- |
/// | `0x00` | `MSG_TYPE_CONTENT` | live |
/// | `0x01` | `MSG_TYPE_BURN` | live in the legacy client |
/// | `0x02`, `0x03` | retired whitelist invitation / response | **NOT REUSABLE** — see below. Also `WIRE_VERSION_V2` / `WIRE_VERSION_V3`. |
/// | `0x04` | `MSG_TYPE_ATTACHMENT` | live. Also `WIRE_VERSION_V4`. |
/// | `0x05` | `MSG_TYPE_SKDM` | live. Also `WIRE_VERSION_V5`. |
/// | `0x06` | `MSG_TYPE_SKDM_REQUEST` | live |
/// | `0x07` | `MSG_TYPE_SESSION_RESET` | live |
/// | `0x08`, `0x09` | native-overlay relay / ack | live |
/// | `0x0A`..=`0x0F` | free | |
/// | **`0x10`** | **`WIRE_VERSION_RN` (this protocol)** | |
/// | `0x11`..=`0x7F` | free | |
/// | `0x80` | `LOCAL_PROTECTED_MESSAGE_TYPE` | live, but declared in `apps/osl-hub/src/broker.rs:54`, **not** in `wire_v2.rs` |
/// | `0x81`..=`0xFF` | free | |
///
/// ### `0x02` and `0x03` are poison, not free
///
/// Their constants were retired, but the receive dispatcher still
/// hard-matches the literals `0x02 | 0x03` and returns
/// `OSL_RESULT_LEGACY_HANDSHAKE_IGNORED`
/// (`crates/ipc/src/commands.rs:4489`). A new protocol placed there would
/// be **silently swallowed** rather than rejected — the worst possible
/// failure mode for a handshake. They are unusable regardless of the
/// namespace argument.
///
/// ### `0x80` is taken but invisible
///
/// Enumerating free bytes by reading `wire_v2.rs` alone wrongly concludes
/// `0x80` is available; it is declared in `broker.rs`. That declaration
/// belongs in `wire_v2.rs` next to the others. Flagged rather than moved:
/// `broker.rs` is not this work's to edit.
///
/// # The byte-1 hazard this does *not* remove
///
/// In an OSL-RN blob, **byte 1 is `flags`, not a message type** (see
/// [`RN_FLAG_BOOTSTRAP`]), and the real message type lives inside the
/// encrypted header — unavailable until a decrypt succeeds. Any
/// fixed-offset probe reading `bundle[1]` as a message type will read a
/// flags byte on an OSL-RN blob. Such probes must gate on
/// `bundle[0] == WIRE_VERSION_V3` first; the existing ones already do.
///
/// # Unrecognised versions are not uniformly reported
///
/// The v=2/v=3 dispatcher fails closed on an unknown message type
/// (`commands.rs:4540`, `other => Err(..)`), but the native-overlay drain
/// silently `continue`s when a bundle does not classify
/// (`broker.rs:2283`). Because an OSL-RN blob has `bundle[0] == 0x10 !=
/// WIRE_VERSION_V3`, an OSL-RN message reaching an **older build** on the
/// overlay path is dropped with no error and no trace. Do not design on
/// the assumption that a peer will report an unsupported version. See
/// `MIGRATION.md`.
pub const WIRE_VERSION_RN: u8 = 0x10;

/// The initiator's bootstrap preamble is attached.
pub const RN_FLAG_BOOTSTRAP: u8 = 0x01;
/// Any other bit set means a newer minor revision we do not
/// understand; the decoder refuses rather than guessing.
pub const RN_FLAG_RESERVED_MASK: u8 = 0xFE;

/// The `DPC0::` prefix the existing carrier already uses.
pub const WIRE_PREFIX: &str = "DPC0::";

/// Refuse absurd blobs before doing any work.
pub const MAX_WIRE_BYTES: usize = 64 * 1024;
/// The encrypted header is small and bounded; a larger one is junk.
pub const MAX_HEADER_CT_BYTES: usize = 1024;

/// Plaintext plus metadata recovered from a OSL-RN blob. Field-compatible
/// with `ipc::wire_v2::DecryptedV2` so call sites do not change shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub msg_type: u8,
    pub plaintext: Vec<u8>,
}

/// Session tunables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SessionParams {
    pub pq: PqParams,
    pub skip: SkipParams,
}

/// The ratchet header, as it appears inside the header AEAD.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatchetHeader {
    pub msg_type: u8,
    pub dh_pub: XPublic,
    pub pn: u32,
    pub n: u32,
    pub pq_have: u32,
    pub mix_epoch: u32,
    pub fragment: Option<Fragment>,
}

impl RatchetHeader {
    fn encode(&self) -> Result<Vec<u8>> {
        let mut w = Writer::with_capacity(64);
        w.u8(self.msg_type);
        w.bytes(self.dh_pub.as_bytes());
        w.varint(self.pn);
        w.varint(self.n);
        w.varint(self.pq_have);
        w.varint(self.mix_epoch);
        match &self.fragment {
            Some(f) => {
                w.u8(1);
                f.encode(&mut w)?;
            }
            None => {
                w.u8(0);
            }
        }
        Ok(w.into_vec())
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        let msg_type = r.u8()?;
        let dh_pub = XPublic::from_bytes(r.array::<32>()?);
        let pn = r.varint()?;
        let n = r.varint()?;
        let pq_have = r.varint()?;
        let mix_epoch = r.varint()?;
        let fragment = match r.u8()? {
            0 => None,
            1 => Some(Fragment::decode(&mut r)?),
            _ => return Err(Error::Malformed("bad fragment flag")),
        };
        r.finish()?;
        Ok(RatchetHeader {
            msg_type,
            dh_pub,
            pn,
            n,
            pq_have,
            mix_epoch,
            fragment,
        })
    }
}

/// One end of a secure session.
#[derive(Clone)]
pub struct Session {
    role: Role,
    session_id: [u8; 16],

    root: Secret32,
    dhs: XSecret,
    dhs_pub: XPublic,
    dhr: Option<XPublic>,

    cks: Option<Secret32>,
    hks: Option<[u8; AEAD_KEY]>,
    nhks: [u8; AEAD_KEY],

    ckr: Option<Secret32>,
    hkr: Option<[u8; AEAD_KEY]>,
    nhkr: [u8; AEAD_KEY],

    ns: u32,
    nr: u32,
    pn: u32,
    /// The mix epoch our current sending chain announces. Fixed when
    /// the chain is created and repeated in every header of that
    /// chain, so any surviving message of the chain lets the peer
    /// replay the step.
    send_mix_epoch: u32,

    pq: PqRatchet,
    skipped: SkippedKeys,

    /// Present on the initiator until it hears back. Repeated on every
    /// message until then, exactly as Signal repeats a PreKeyMessage.
    bootstrap: Option<Preamble>,
}

/// Redacted `Debug`. Deliberately hand-written rather than derived:
/// a derived impl would print root keys, chain keys, header keys and
/// message keys into any log line or panic message that formats a
/// session. Nothing secret appears below.
impl core::fmt::Debug for Session {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Session")
            .field("role", &self.role)
            .field("session_id", &hex16(&self.session_id))
            .field("ns", &self.ns)
            .field("nr", &self.nr)
            .field("pn", &self.pn)
            .field("send_mix_epoch", &self.send_mix_epoch)
            .field("pq_have", &self.pq.pq_have())
            .field("pq_mixed", &self.pq.mixed())
            .field("skipped_keys", &self.skipped.total_keys())
            .field("skipped_chains", &self.skipped.chain_count())
            .field("bootstrap_pending", &self.bootstrap.is_some())
            .finish_non_exhaustive()
    }
}

fn hex16(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Which candidate header key opened a message.
enum Opener {
    Current,
    Next,
    Stored([u8; AEAD_KEY]),
}

impl Session {
    /// Stable, non-secret session identifier. Both sides compute the
    /// same value; safe to use as a storage key.
    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    pub fn role(&self) -> Role {
        self.role
    }
    pub fn sending_counter(&self) -> u32 {
        self.ns
    }
    pub fn receiving_counter(&self) -> u32 {
        self.nr
    }
    pub fn skipped_key_count(&self) -> usize {
        self.skipped.total_keys()
    }
    pub fn skipped_chain_count(&self) -> usize {
        self.skipped.chain_count()
    }
    /// The skipped-key storage policy this session is running under.
    ///
    /// Exposed so a persistence layer can enforce its *own* ceiling on
    /// an imported state blob: `import_state` restores the caps that
    /// were exported, so a blob that was written under looser params
    /// (a future build, a hand-edited file, a rolled-back profile)
    /// would otherwise silently raise this session's memory ceiling.
    /// The caller can compare against a policy maximum and refuse.
    pub fn skip_params(&self) -> crate::skipped::SkipParams {
        self.skipped.params()
    }
    /// Highest PQ epoch whose secret is folded into the root key.
    pub fn pq_mixed_epoch(&self) -> u32 {
        self.pq.mixed()
    }
    /// Highest PQ epoch whose secret we hold.
    pub fn pq_have_epoch(&self) -> u32 {
        self.pq.pq_have()
    }
    pub fn has_bootstrap_pending(&self) -> bool {
        self.bootstrap.is_some()
    }

    // -----------------------------------------------------------
    // Construction
    // -----------------------------------------------------------

    /// Initiator side: build a session from a peer's published bundle.
    ///
    /// **Protocol-only form: no negotiated version is bound into `SK`.**
    /// Application code must call [`Session::initiate_bound`]; see
    /// [`crate::negotiate`] for why.
    pub fn initiate<R: RngCore + CryptoRng>(
        local_identity: &XSecret,
        peer: &PeerBundle,
        params: SessionParams,
        rng: &mut R,
    ) -> Result<Self> {
        Self::initiate_bound(local_identity, peer, None, params, rng)
    }

    /// Initiator side with the negotiated version bound into `SK`.
    ///
    /// `binding` comes from [`crate::negotiate::Negotiation::digest`].
    /// A responder that computes a different digest derives a different
    /// `SK`, so its first decrypt returns [`Error::AuthFailed`] and the
    /// session never establishes — there is no fallback path.
    pub fn initiate_bound<R: RngCore + CryptoRng>(
        local_identity: &XSecret,
        peer: &PeerBundle,
        binding: Option<&[u8; 32]>,
        params: SessionParams,
        rng: &mut R,
    ) -> Result<Self> {
        let (hs, preamble, peer_ratchet) =
            handshake::initiate_bound(local_identity, peer, binding, rng)?;
        let (dhs, dhs_pub) = x25519_keypair(rng);
        let mut pq = PqRatchet::new(Role::Initiator, params.pq, hs.pq_epoch0.clone())?;
        let skipped = SkippedKeys::new(params.skip)?;

        let mix = pq.mix_target();
        let pq_secrets = pq.peek_mix(mix)?;
        let dh_out = dh(&dhs, &peer_ratchet)?;
        let step = kdf::root_step(&hs.root, &dh_out, &pq_secrets, mix)?;
        pq.commit_mix(mix);

        Ok(Session {
            role: Role::Initiator,
            session_id: hs.session_id,
            root: step.root_key,
            dhs,
            dhs_pub,
            dhr: Some(peer_ratchet),
            cks: Some(step.chain_key),
            hks: Some(hs.initiator_header_key),
            nhks: step.header_key,
            ckr: None,
            hkr: None,
            nhkr: hs.responder_header_key,
            ns: 0,
            nr: 0,
            pn: 0,
            send_mix_epoch: mix,
            pq,
            skipped,
            bootstrap: Some(preamble),
        })
    }

    fn from_handshake_responder(
        hs: HandshakeOutput,
        signed_prekey: &XSecret,
        params: SessionParams,
    ) -> Result<Self> {
        let pq = PqRatchet::new(Role::Responder, params.pq, hs.pq_epoch0.clone())?;
        let skipped = SkippedKeys::new(params.skip)?;
        Ok(Session {
            role: Role::Responder,
            session_id: hs.session_id,
            root: hs.root,
            dhs: signed_prekey.clone(),
            dhs_pub: signed_prekey.public(),
            dhr: None,
            cks: None,
            hks: None,
            nhks: hs.responder_header_key,
            ckr: None,
            hkr: None,
            nhkr: hs.initiator_header_key,
            ns: 0,
            nr: 0,
            pn: 0,
            send_mix_epoch: 0,
            pq,
            skipped,
            bootstrap: None,
        })
    }

    /// Responder side: build a session from an incoming bootstrap
    /// message and open it in one step.
    ///
    /// Mirrors `wire_v2::decrypt_v4`'s shape, which likewise
    /// reconstructs the receiving state from the wire.
    ///
    /// **Protocol-only form: no negotiated version is bound into `SK`.**
    /// Application code must call [`Session::accept_bound`].
    pub fn accept<R: RngCore + CryptoRng>(
        local: &LocalPrekeys,
        wire: &str,
        params: SessionParams,
        rng: &mut R,
    ) -> Result<(Self, Opened)> {
        Self::accept_bound(local, wire, None, params, rng)
    }

    /// Responder side with the negotiated version bound into `SK`.
    ///
    /// The responder computes its own digest from *its own* identity and
    /// ML-KEM key plus the initiator identity carried in the preamble,
    /// so the two sides never need to exchange the digest. Disagreement
    /// surfaces as [`Error::AuthFailed`] on this call.
    pub fn accept_bound<R: RngCore + CryptoRng>(
        local: &LocalPrekeys,
        wire: &str,
        binding: Option<&[u8; 32]>,
        params: SessionParams,
        rng: &mut R,
    ) -> Result<(Self, Opened)> {
        let raw = decode_wire(wire)?;
        let parsed = ParsedWire::parse(&raw)?;
        let preamble = parsed
            .preamble
            .as_ref()
            .ok_or(Error::Malformed("accept requires a bootstrap message"))?;
        let hs = handshake::respond_bound(local, preamble, binding)?;
        let mut session = Self::from_handshake_responder(hs, &local.signed_prekey, params)?;
        let opened = session.open_parsed(&parsed, rng)?;
        Ok((session, opened))
    }

    // -----------------------------------------------------------
    // Encrypt
    // -----------------------------------------------------------

    /// Encrypt one message. Returns a `DPC0::`-prefixed wire blob.
    pub fn encrypt<R: RngCore + CryptoRng>(
        &mut self,
        msg_type: u8,
        plaintext: &[u8],
        rng: &mut R,
    ) -> Result<String> {
        let cks = self
            .cks
            .as_ref()
            .ok_or(Error::Internal("no sending chain yet"))?
            .clone();
        let hks = self.hks.ok_or(Error::Internal("no sending header key"))?;
        if self.ns == u32::MAX {
            return Err(Error::PolicyBound("sending counter exhausted"));
        }

        let fragment = self.pq.on_send(rng)?;
        let (mk, next_ck) = kdf::chain_step(&cks)?;

        let header = RatchetHeader {
            msg_type,
            dh_pub: self.dhs_pub,
            pn: self.pn,
            n: self.ns,
            pq_have: self.pq.pq_have(),
            mix_epoch: self.send_mix_epoch,
            fragment,
        };
        let header_pt = header.encode()?;

        let mut prefix = Writer::with_capacity(2 + handshake::PREAMBLE_MIN_BYTES);
        let flags = if self.bootstrap.is_some() {
            RN_FLAG_BOOTSTRAP
        } else {
            0
        };
        prefix.u8(WIRE_VERSION_RN).u8(flags);
        if let Some(p) = &self.bootstrap {
            p.encode(&mut prefix);
        }
        let prefix = prefix.into_vec();

        let mut nonce_wire = [0u8; HEADER_NONCE_WIRE];
        rng.fill_bytes(&mut nonce_wire);

        let mut ad_h = Vec::with_capacity(kdf::AD_HEADER.len() + prefix.len());
        ad_h.extend_from_slice(kdf::AD_HEADER);
        ad_h.extend_from_slice(&prefix);
        let header_ct = aead_seal(&hks, &kdf::header_nonce(&nonce_wire), &ad_h, &header_pt)?;
        if header_ct.len() > MAX_HEADER_CT_BYTES {
            return Err(Error::PolicyBound("header too large"));
        }

        let mut out = Writer::with_capacity(prefix.len() + header_ct.len() + plaintext.len() + 64);
        out.bytes(&prefix);
        out.bytes(&nonce_wire);
        out.var_bytes(&header_ct)?;

        let mut ad_b = Vec::with_capacity(kdf::AD_BODY.len() + out.len());
        ad_b.extend_from_slice(kdf::AD_BODY);
        ad_b.extend_from_slice(out.as_slice());
        let body_ct = aead_seal(mk.as_bytes(), &kdf::body_nonce(&mk)?, &ad_b, plaintext)?;
        out.bytes(&body_ct);

        // Commit only once every fallible step has succeeded.
        self.cks = Some(next_ck);
        self.ns = self.ns.saturating_add(1);

        let raw = out.into_vec();
        if raw.len() > MAX_WIRE_BYTES {
            return Err(Error::PolicyBound("message exceeds wire budget"));
        }
        Ok(format!("{WIRE_PREFIX}{}", base64_encode(&raw)))
    }

    // -----------------------------------------------------------
    // Decrypt
    // -----------------------------------------------------------

    /// Decrypt one message. On any failure the session is left
    /// bit-for-bit unchanged.
    pub fn decrypt<R: RngCore + CryptoRng>(&mut self, wire: &str, rng: &mut R) -> Result<Opened> {
        let raw = decode_wire(wire)?;
        let parsed = ParsedWire::parse(&raw)?;
        self.open_parsed(&parsed, rng)
    }

    fn open_parsed<R: RngCore + CryptoRng>(
        &mut self,
        parsed: &ParsedWire<'_>,
        rng: &mut R,
    ) -> Result<Opened> {
        let mut ad_h = Vec::with_capacity(kdf::AD_HEADER.len() + parsed.prefix.len());
        ad_h.extend_from_slice(kdf::AD_HEADER);
        ad_h.extend_from_slice(parsed.prefix);
        let nonce = kdf::header_nonce(&parsed.header_nonce);

        // Bounded trial decryption: current chain, next chain, then at
        // most `max_chains` retained chains. Nothing here mutates
        // state, so an unauthenticated blob costs only these opens.
        let mut candidates: Vec<(Opener, [u8; AEAD_KEY])> = Vec::with_capacity(2 + 8);
        if let Some(hk) = self.hkr {
            candidates.push((Opener::Current, hk));
        }
        candidates.push((Opener::Next, self.nhkr));
        for hk in self.skipped.header_keys() {
            if Some(hk) != self.hkr {
                candidates.push((Opener::Stored(hk), hk));
            }
        }

        let mut opened: Option<(Opener, RatchetHeader)> = None;
        for (which, hk) in candidates {
            if let Ok(pt) = aead_open(&hk, &nonce, &ad_h, parsed.header_ct) {
                // A header that authenticates but does not parse is a
                // peer bug, not an attack; surface it rather than
                // silently trying the next key.
                let header = RatchetHeader::decode(&pt)?;
                opened = Some((which, header));
                break;
            }
        }
        let Some((which, header)) = opened else {
            return Err(Error::AuthFailed);
        };

        // From here on we work on a clone and commit atomically.
        let mut trial = self.clone();
        let mk = trial.advance_for(&which, &header, rng)?;

        let mut ad_b = Vec::with_capacity(kdf::AD_BODY.len() + parsed.pre_body.len());
        ad_b.extend_from_slice(kdf::AD_BODY);
        ad_b.extend_from_slice(parsed.pre_body);
        let plaintext = aead_open(mk.as_bytes(), &kdf::body_nonce(&mk)?, &ad_b, parsed.body_ct)?;

        trial.skipped.tick();
        trial.bootstrap = None;
        *self = trial;
        Ok(Opened {
            msg_type: header.msg_type,
            plaintext,
        })
    }

    /// Advance the (cloned) ratchet to the message key for `header`.
    fn advance_for<R: RngCore + CryptoRng>(
        &mut self,
        which: &Opener,
        header: &RatchetHeader,
        rng: &mut R,
    ) -> Result<Secret32> {
        // The peer is authenticated at this point, so its advertised
        // PQ state and any attached fragment can be absorbed. Doing it
        // before the ratchet keeps the mix target as fresh as
        // possible.
        self.pq.on_peer_have(header.pq_have);
        if let Some(f) = &header.fragment {
            self.pq.on_fragment(f, rng)?;
        }

        match which {
            Opener::Stored(hk) => self
                .skipped
                .take(hk, header.n)
                .ok_or(Error::AuthFailed),

            Opener::Current => {
                let hk = self.hkr.ok_or(Error::Internal("current chain without key"))?;
                self.message_key_in_current_chain(&hk, header.n)
            }

            Opener::Next => {
                self.dh_ratchet(header, rng)?;
                let hk = self.hkr.ok_or(Error::Internal("ratchet left no header key"))?;
                self.message_key_in_current_chain(&hk, header.n)
            }
        }
    }

    /// Derive (or recover) the message key for counter `n` in the
    /// current receiving chain, caching anything skipped.
    fn message_key_in_current_chain(
        &mut self,
        hk: &[u8; AEAD_KEY],
        n: u32,
    ) -> Result<Secret32> {
        if n < self.nr {
            // Older than the chain head: it can only be a cached skip.
            // A replay lands here too, and fails, because taking a
            // skipped key removes it.
            return self.skipped.take(hk, n).ok_or(Error::AuthFailed);
        }

        let gap = u64::from(n) - u64::from(self.nr);
        // Checked before any derivation, so an over-large gap costs
        // nothing and leaves the trial state untouched.
        self.skipped.check_skip(gap)?;

        let mut ck = self
            .ckr
            .as_ref()
            .ok_or(Error::Internal("no receiving chain"))?
            .clone();
        let mut counter = self.nr;
        for _ in 0..gap {
            let (mk, next) = kdf::chain_step(&ck)?;
            self.skipped.insert(hk, counter, mk);
            ck = next;
            counter = counter.saturating_add(1);
        }
        let (mk, next) = kdf::chain_step(&ck)?;
        self.ckr = Some(next);
        self.nr = counter.saturating_add(1);
        Ok(mk)
    }

    /// One DH ratchet step, following the header-encrypted Double
    /// Ratchet, with the announced PQ epoch secrets folded into the
    /// first root step.
    fn dh_ratchet<R: RngCore + CryptoRng>(
        &mut self,
        header: &RatchetHeader,
        rng: &mut R,
    ) -> Result<()> {
        // Bound the work of finishing the outgoing chain *before*
        // touching anything.
        if self.ckr.is_some() {
            let remaining = u64::from(header.pn).saturating_sub(u64::from(self.nr));
            self.skipped.check_skip(remaining)?;
        }
        self.skipped.check_skip(u64::from(header.n))?;

        // Drain what is left of the chain we are leaving.
        if let (Some(ck), Some(hk)) = (self.ckr.clone(), self.hkr) {
            let mut ck = ck;
            let mut counter = self.nr;
            while counter < header.pn {
                let (mk, next) = kdf::chain_step(&ck)?;
                self.skipped.insert(&hk, counter, mk);
                ck = next;
                counter = counter.saturating_add(1);
            }
        }

        let pq_secrets = self.pq.take_mix(header.mix_epoch)?;

        self.pn = self.ns;
        self.ns = 0;
        self.nr = 0;
        self.hks = Some(self.nhks);
        self.hkr = Some(self.nhkr);
        self.dhr = Some(header.dh_pub);

        // Step 1: the peer's new sending chain (our receiving chain).
        let dh1 = dh(&self.dhs, &header.dh_pub)?;
        let s1 = kdf::root_step(&self.root, &dh1, &pq_secrets, header.mix_epoch)?;
        self.root = s1.root_key;
        self.ckr = Some(s1.chain_key);
        self.nhkr = s1.header_key;

        // Step 2: our new sending chain.
        let (dhs, dhs_pub) = x25519_keypair(rng);
        self.dhs = dhs;
        self.dhs_pub = dhs_pub;
        let mix = self.pq.mix_target();
        let pq2 = self.pq.peek_mix(mix)?;
        let dh2 = dh(&self.dhs, &header.dh_pub)?;
        let s2 = kdf::root_step(&self.root, &dh2, &pq2, mix)?;
        self.pq.commit_mix(mix);
        self.root = s2.root_key;
        self.cks = Some(s2.chain_key);
        self.nhks = s2.header_key;
        self.send_mix_epoch = mix;
        Ok(())
    }

    // -----------------------------------------------------------
    // State export / import
    // -----------------------------------------------------------

    /// Serialize the full session. **The output contains every secret
    /// the session holds** and must be sealed at rest by the caller.
    pub fn export_state(&self) -> Result<Vec<u8>> {
        let mut w = Writer::with_capacity(4096);
        w.u8(STATE_FORMAT_VERSION);
        w.u8(match self.role {
            Role::Initiator => 0,
            Role::Responder => 1,
        });
        w.bytes(&self.session_id);
        w.bytes(self.root.as_bytes());
        w.bytes(self.dhs.as_bytes());
        match &self.dhr {
            Some(p) => {
                w.u8(1);
                w.bytes(p.as_bytes());
            }
            None => {
                w.u8(0);
            }
        }
        write_opt_secret(&mut w, self.cks.as_ref());
        write_opt_key(&mut w, self.hks.as_ref());
        w.bytes(&self.nhks);
        write_opt_secret(&mut w, self.ckr.as_ref());
        write_opt_key(&mut w, self.hkr.as_ref());
        w.bytes(&self.nhkr);
        w.varint(self.ns);
        w.varint(self.nr);
        w.varint(self.pn);
        w.varint(self.send_mix_epoch);
        self.pq.export(&mut w)?;
        self.skipped.export(&mut w)?;
        match &self.bootstrap {
            Some(p) => {
                w.u8(1);
                p.encode(&mut w);
            }
            None => {
                w.u8(0);
            }
        }
        Ok(w.into_vec())
    }

    /// Restore a session from [`Self::export_state`].
    pub fn import_state(bytes: &[u8]) -> Result<Self> {
        let mut r = Reader::new(bytes);
        if r.u8()? != STATE_FORMAT_VERSION {
            return Err(Error::BadStateFormat);
        }
        let role = match r.u8()? {
            0 => Role::Initiator,
            1 => Role::Responder,
            _ => return Err(Error::BadStateFormat),
        };
        let session_id = r.array::<16>()?;
        let root = Secret32::from_bytes(r.array::<32>()?);
        let dhs = XSecret::from_bytes(r.array::<32>()?);
        let dhs_pub = dhs.public();
        let dhr = if r.u8()? == 1 {
            Some(XPublic::from_bytes(r.array::<32>()?))
        } else {
            None
        };
        let cks = read_opt_secret(&mut r)?;
        let hks = read_opt_key(&mut r)?;
        let nhks = r.array::<AEAD_KEY>()?;
        let ckr = read_opt_secret(&mut r)?;
        let hkr = read_opt_key(&mut r)?;
        let nhkr = r.array::<AEAD_KEY>()?;
        let ns = r.varint()?;
        let nr = r.varint()?;
        let pn = r.varint()?;
        let send_mix_epoch = r.varint()?;
        let pq = PqRatchet::import(&mut r)?;
        let skipped = SkippedKeys::import(&mut r)?;
        let bootstrap = if r.u8()? == 1 {
            Some(Preamble::decode(&mut r)?)
        } else {
            None
        };
        r.finish().map_err(|_| Error::BadStateFormat)?;
        Ok(Session {
            role,
            session_id,
            root,
            dhs,
            dhs_pub,
            dhr,
            cks,
            hks,
            nhks,
            ckr,
            hkr,
            nhkr,
            ns,
            nr,
            pn,
            send_mix_epoch,
            pq,
            skipped,
            bootstrap,
        })
    }
}

/// Bumped whenever the serialized layout changes. Import refuses any
/// other value rather than mis-parsing.
pub const STATE_FORMAT_VERSION: u8 = 1;

fn write_opt_secret(w: &mut Writer, s: Option<&Secret32>) {
    match s {
        Some(v) => {
            w.u8(1);
            w.bytes(v.as_bytes());
        }
        None => {
            w.u8(0);
        }
    }
}

fn read_opt_secret(r: &mut Reader<'_>) -> Result<Option<Secret32>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(Secret32::from_bytes(r.array::<32>()?))),
        _ => Err(Error::BadStateFormat),
    }
}

fn write_opt_key(w: &mut Writer, k: Option<&[u8; AEAD_KEY]>) {
    match k {
        Some(v) => {
            w.u8(1);
            w.bytes(v);
        }
        None => {
            w.u8(0);
        }
    }
}

fn read_opt_key(r: &mut Reader<'_>) -> Result<Option<[u8; AEAD_KEY]>> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.array::<AEAD_KEY>()?)),
        _ => Err(Error::BadStateFormat),
    }
}

/// Read the initiator's identity key out of a `OSL-RN` **bootstrap** blob
/// without deriving anything.
///
/// A responder needs this before it can build its own
/// [`crate::negotiate::Negotiation`], because the digest binds the
/// initiator's identity and the responder learns that identity only
/// from the preamble.
///
/// This is a *parse*, not an authentication: the value returned is
/// attacker-controlled until [`Session::accept_bound`] succeeds. It is
/// safe to use as a lookup key and as a digest input — a wrong value
/// simply yields a `SK` that does not match, i.e. `AuthFailed`. It must
/// **not** be treated as proof of who sent the message.
///
/// Returns `Ok(None)` for a well-formed `OSL-RN` blob that carries no
/// bootstrap preamble (an ordinary in-session message).
pub fn peek_bootstrap_initiator_identity(wire: &str) -> Result<Option<XPublic>> {
    let raw = decode_wire(wire)?;
    let parsed = ParsedWire::parse(&raw)?;
    Ok(parsed.preamble.as_ref().map(|p| p.initiator_identity))
}

/// The parsed outer framing. Borrows the decoded buffer.
struct ParsedWire<'a> {
    /// Version, flags and (if present) the preamble — the associated
    /// data for the header AEAD.
    prefix: &'a [u8],
    preamble: Option<Preamble>,
    header_nonce: [u8; HEADER_NONCE_WIRE],
    header_ct: &'a [u8],
    /// Everything up to and including `header_ct` — the associated
    /// data for the body AEAD.
    pre_body: &'a [u8],
    body_ct: &'a [u8],
}

impl<'a> ParsedWire<'a> {
    fn parse(raw: &'a [u8]) -> Result<Self> {
        // Version first, before any length reasoning: a v=3 blob must
        // report WrongVersion, not TooShort.
        let version = *raw.first().ok_or(Error::Malformed("empty blob"))?;
        if version != WIRE_VERSION_RN {
            return Err(Error::WrongVersion {
                got: version,
                expected: WIRE_VERSION_RN,
            });
        }
        let mut r = Reader::new(raw);
        let _ = r.u8()?;
        let flags = r.u8()?;
        if flags & RN_FLAG_RESERVED_MASK != 0 {
            return Err(Error::Malformed("reserved OSL-RN flag bits set"));
        }
        let preamble = if flags & RN_FLAG_BOOTSTRAP != 0 {
            Some(Preamble::decode(&mut r)?)
        } else {
            None
        };
        let prefix_len = raw.len().saturating_sub(r.remaining());
        let prefix = raw
            .get(..prefix_len)
            .ok_or(Error::Internal("prefix slice"))?;

        let header_nonce = r.array::<HEADER_NONCE_WIRE>()?;
        let header_ct_len = r.varint()? as usize;
        if header_ct_len > MAX_HEADER_CT_BYTES {
            return Err(Error::PolicyBound("header ciphertext too large"));
        }
        let header_ct = r.take(header_ct_len)?;

        let pre_body_len = raw.len().saturating_sub(r.remaining());
        let pre_body = raw
            .get(..pre_body_len)
            .ok_or(Error::Internal("pre-body slice"))?;
        let remaining = r.remaining();
        let body_ct = r.take(remaining)?;
        if body_ct.len() < crate::primitives::AEAD_TAG {
            return Err(Error::Malformed("body shorter than an AEAD tag"));
        }
        r.finish()?;

        Ok(ParsedWire {
            prefix,
            preamble,
            header_nonce,
            header_ct,
            pre_body,
            body_ct,
        })
    }
}

fn decode_wire(wire: &str) -> Result<Vec<u8>> {
    let body = wire.strip_prefix(WIRE_PREFIX).ok_or(Error::BadPrefix)?;
    if body.len() > MAX_WIRE_BYTES * 2 {
        return Err(Error::PolicyBound("wire blob exceeds budget"));
    }
    let raw = base64_decode(body)?;
    if raw.len() > MAX_WIRE_BYTES {
        return Err(Error::PolicyBound("wire blob exceeds budget"));
    }
    Ok(raw)
}

fn base64_encode(raw: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(raw)
}

fn base64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| Error::Base64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;

    #[test]
    fn header_roundtrips() {
        let h = RatchetHeader {
            msg_type: 3,
            dh_pub: XPublic::from_bytes([7u8; 32]),
            pn: 300,
            n: 5,
            pq_have: 2,
            mix_epoch: 1,
            fragment: Some(Fragment {
                kind: crate::pq::FRAG_KIND_EK,
                epoch: 3,
                index: 1,
                total: 10,
                data: vec![1, 2, 3, 4],
            }),
        };
        let bytes = h.encode().expect("encode");
        assert_eq!(RatchetHeader::decode(&bytes).expect("decode"), h);
        // Truncation at any point must error, never panic.
        for cut in 0..bytes.len() {
            assert!(RatchetHeader::decode(&bytes[..cut]).is_err(), "cut {cut}");
        }
        // Trailing bytes are rejected.
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(RatchetHeader::decode(&extra).is_err());
    }

    #[test]
    fn wrong_version_is_distinguishable() {
        let raw = [0x03u8, 0, 0, 0];
        assert_eq!(
            ParsedWire::parse(&raw).err(),
            Some(Error::WrongVersion {
                got: 0x03,
                expected: WIRE_VERSION_RN,
            })
        );
        // Every other assigned wire version must also be reported as a
        // version mismatch, not as corruption, so a router can fall
        // through to the v=2..v=5 decoders.
        for v in [0x02u8, 0x04, 0x05] {
            let raw = [v, 0, 0, 0];
            assert_eq!(
                ParsedWire::parse(&raw).err(),
                Some(Error::WrongVersion {
                    got: v,
                    expected: WIRE_VERSION_RN,
                })
            );
        }
    }

    #[test]
    fn reserved_flag_bits_are_refused() {
        let raw = [WIRE_VERSION_RN, 0x80, 0, 0];
        assert_eq!(
            ParsedWire::parse(&raw).err(),
            Some(Error::Malformed("reserved OSL-RN flag bits set"))
        );
    }

    #[test]
    fn parse_never_panics_on_arbitrary_input() {
        let mut seed = 0x12345678u32;
        for len in 0..300usize {
            let mut buf = vec![0u8; len];
            for b in buf.iter_mut() {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *b = (seed >> 16) as u8;
            }
            if let Some(first) = buf.first_mut() {
                *first = WIRE_VERSION_RN;
            }
            let _ = ParsedWire::parse(&buf);
        }
    }

    #[test]
    fn basic_roundtrip_both_directions() {
        let mut h = Harness::new(1);
        let wire = h.alice_send(b"hello bob").expect("send");
        assert_eq!(h.bob_recv(&wire).expect("recv").plaintext, b"hello bob");
        let wire = h.bob_send(b"hi alice").expect("send");
        assert_eq!(h.alice_recv(&wire).expect("recv").plaintext, b"hi alice");
    }

    #[test]
    fn session_ids_match() {
        let h = Harness::established(2);
        assert_eq!(h.alice.session_id(), h.bob_ref().session_id());
        assert_ne!(h.alice.session_id(), [0u8; 16]);
    }

    #[test]
    fn state_export_import_survives_an_interleaved_run() {
        let skip = SkipParams {
            max_skip_per_message: 3,
            max_keys_per_chain: 3,
            max_total_keys: 3,
            max_chains: 2,
            max_age: 100,
        };
        let params = SessionParams {
            skip,
            ..SessionParams::default()
        };
        let (mut alice, mut bob, mut rng) = established_pair_with_params(44, params);

        let a0 = alice.encrypt(0, b"alice gap 0", &mut rng).expect("a0");
        let a1 = alice.encrypt(0, b"alice gap 1", &mut rng).expect("a1");
        let a2 = alice.encrypt(0, b"alice gap 2", &mut rng).expect("a2");
        let a3 = alice.encrypt(0, b"alice gap 3", &mut rng).expect("a3");

        assert_eq!(
            bob.decrypt(&a3, &mut rng)
                .expect("deliver newest")
                .plaintext,
            b"alice gap 3"
        );
        assert_eq!(bob.skipped_key_count(), 3);

        let interleaved = bob
            .encrypt(7, b"bob interleaved reply", &mut rng)
            .expect("bob reply");
        let opened = alice
            .decrypt(&interleaved, &mut rng)
            .expect("alice receives interleaved reply");
        assert_eq!(opened.msg_type, 7);
        assert_eq!(opened.plaintext, b"bob interleaved reply");

        let alice_session_id = alice.session_id();
        let bob_session_id = bob.session_id();
        let alice_sending_counter = alice.sending_counter();
        let alice_receiving_counter = alice.receiving_counter();
        let bob_sending_counter = bob.sending_counter();
        let bob_receiving_counter = bob.receiving_counter();
        let bob_skip_params = bob.skip_params();
        let alice_state = alice.export_state().expect("export alice");
        let bob_state = bob.export_state().expect("export bob");
        alice = Session::import_state(&alice_state).expect("import alice");
        bob = Session::import_state(&bob_state).expect("import bob");
        assert_eq!(alice.session_id(), alice_session_id);
        assert_eq!(bob.session_id(), bob_session_id);
        assert_eq!(alice.sending_counter(), alice_sending_counter);
        assert_eq!(alice.receiving_counter(), alice_receiving_counter);
        assert_eq!(bob.sending_counter(), bob_sending_counter);
        assert_eq!(bob.receiving_counter(), bob_receiving_counter);
        assert_eq!(bob.skip_params(), skip);
        assert_eq!(bob.skip_params(), bob_skip_params);
        assert_eq!(bob.skipped_key_count(), 3);
        assert!(bob.skipped_key_count() <= bob.skip_params().max_total_keys);

        assert_eq!(
            bob.decrypt(&a1, &mut rng)
                .expect("deliver skipped 1")
                .plaintext,
            b"alice gap 1"
        );
        assert_eq!(bob.skipped_key_count(), 2);
        assert!(
            bob.decrypt(&a1, &mut rng).is_err(),
            "a restored skipped key must be consumed exactly once"
        );
        assert_eq!(
            bob.decrypt(&a0, &mut rng)
                .expect("deliver skipped 0")
                .plaintext,
            b"alice gap 0"
        );
        assert_eq!(bob.skipped_key_count(), 1);
        assert_eq!(
            bob.decrypt(&a2, &mut rng)
                .expect("deliver skipped 2")
                .plaintext,
            b"alice gap 2"
        );
        assert_eq!(bob.skipped_key_count(), 0);

        let after = alice
            .encrypt(9, b"alice after restored gaps", &mut rng)
            .expect("alice after restore");
        let opened = bob.decrypt(&after, &mut rng).expect("bob after restore");
        assert_eq!(opened.msg_type, 9);
        assert_eq!(opened.plaintext, b"alice after restored gaps");

        let bob_after = bob
            .encrypt(10, b"bob after restored gaps", &mut rng)
            .expect("bob after restore");
        let opened = alice
            .decrypt(&bob_after, &mut rng)
            .expect("alice after restore");
        assert_eq!(opened.msg_type, 10);
        assert_eq!(opened.plaintext, b"bob after restored gaps");
    }
}
