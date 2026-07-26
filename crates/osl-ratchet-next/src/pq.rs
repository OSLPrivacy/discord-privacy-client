//! The **PQ epoch ratchet**: fragmented ML-KEM-768 rekeying that runs
//! *alongside* the classical DH ratchet without ever blocking it.
//!
//! # The problem this solves
//!
//! Shipped Signal is post-quantum at the handshake (PQXDH) and
//! classical for the rest of the session: the per-message DH ratchet
//! is X25519. A harvest-now-decrypt-later adversary who records a long
//! conversation and later obtains a quantum computer recovers every
//! message after the point where the PQXDH secret's protection is
//! exhausted, because no new PQ entropy is ever introduced.
//!
//! The obvious fix — run ML-KEM at every DH ratchet step — costs 1088
//! bytes of ciphertext plus 1184 bytes of encapsulation key per step.
//! On this product's carrier (steganographic cover text in a chat
//! client) that is not affordable, and worse, it is *fragile*: if the
//! one message carrying the ML-KEM ciphertext is dropped permanently,
//! a naive design stalls forever.
//!
//! # The design
//!
//! PQ rekeying is decoupled from the DH ratchet and runs as its own
//! slow, loss-tolerant, restartable sub-protocol:
//!
//! 1. **Epoch ownership alternates deterministically.** Epoch 0 is the
//!    handshake KEM (owned by the responder, whose PQ prekey it used).
//!    Thereafter epoch `E` is owned by the responder when `E` is even
//!    and by the initiator when `E` is odd. No negotiation is needed.
//!
//! 2. **The owner publishes a fresh encapsulation key**, fragmented
//!    across its outgoing messages, one fragment per message.
//!
//! 3. **The peer assembles it, encapsulates, and fragments the
//!    ciphertext back.**
//!
//! 4. **The owner decapsulates** and now holds `ss_E`. It advertises
//!    `pq_have = E` in every subsequent header. Seeing that, the peer
//!    also promotes to `pq_have = E` (it has held `ss_E` since it
//!    encapsulated, but could not know the owner received the
//!    ciphertext).
//!
//! 5. **Mixing is announced, not inferred.** When a party starts a new
//!    sending chain (a DH ratchet step) it writes
//!    `mix_epoch = min(self.pq_have, last_seen_peer_pq_have)` into the
//!    chain's header — the same value in *every* message of that
//!    chain. The receiver replaying the step folds every epoch secret
//!    in `(already_mixed, mix_epoch]` into the root KDF alongside the
//!    X25519 output.
//!
//! # Why this is loss-tolerant
//!
//! Fragments are retransmitted round-robin until acknowledged, and
//! nothing in the classical ratchet waits on them. A permanently
//! dropped fragment delays PQ healing by one cycle; it never stalls
//! message delivery, never stalls the DH ratchet, and never forces a
//! retransmit of user data. If a fragment stream is hopeless the owner
//! simply starts a later epoch and the stale one is abandoned.
//!
//! Because `mix_epoch` is fixed when a chain is created and repeated
//! in every header of that chain, *any* surviving message of a chain
//! lets the receiver perform the step — exactly the property that
//! makes Signal's DH ratchet tolerate permanent loss. Putting the
//! ML-KEM ciphertext itself in the ratcheting header would have
//! destroyed that property.
//!
//! # Wire cost
//!
//! One full epoch moves 1184 bytes (encapsulation key) in one
//! direction and 1088 bytes (ciphertext) in the other. With the
//! default 128-byte fragment that is 10 and 9 messages respectively,
//! costing ~134 bytes on each of those messages and **zero** on every
//! other message. At the default rekey interval of 64 messages the
//! amortised cost is ~18 bytes per message per direction.
//!
//! # Prior art, stated honestly
//!
//! Chunking an ML-KEM ciphertext across messages to build a PQ ratchet
//! on a constrained carrier is not novel: Signal's own SPQR / Triple
//! Ratchet work and Apple's PQ3 periodic PQ rekey are in the same
//! family, and PQ3 in particular pioneered "periodic PQ rekey rather
//! than per-message". The claim made here is *not* novelty. It is that
//! this is a materially stronger posture than the classical Double
//! Ratchet that ships behind PQXDH, and that the specific
//! ack-then-announce mixing rule below makes the healing point
//! unambiguous for both parties under arbitrary loss and reordering.

use crate::codec::{Reader, Writer};
use crate::error::{Error, Result};
use crate::primitives::{
    kem_decapsulate, kem_encapsulate, kem_keypair, KemPublic, KemSecret, Secret32, MLKEM_CT,
    MLKEM_EK,
};
use rand_core::{CryptoRng, RngCore};
use std::collections::BTreeMap;

/// Fragment carries part of an ML-KEM encapsulation key.
pub const FRAG_KIND_EK: u8 = 1;
/// Fragment carries part of an ML-KEM ciphertext.
pub const FRAG_KIND_CT: u8 = 2;

/// Hard cap on fragments per epoch stream. Bounds assembler memory to
/// `MAX_FRAGMENTS * MAX_FRAGMENT_BYTES` and bounds the work an
/// authenticated-but-buggy peer can induce.
pub const MAX_FRAGMENTS: u32 = 64;
/// Hard cap on a single fragment's payload.
pub const MAX_FRAGMENT_BYTES: usize = 512;
/// How far ahead of `pq_have` a peer may legitimately start an epoch.
pub const MAX_EPOCH_LOOKAHEAD: u32 = 2;
/// Retained-but-unmixed epoch secrets. Exceeding this stops new epochs
/// rather than growing the map.
pub const MAX_RETAINED_EPOCHS: usize = 8;

/// Tunables. Defaults are the ones documented in `DESIGN.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PqParams {
    /// Payload bytes per fragment.
    pub fragment_bytes: usize,
    /// Messages sent after an epoch completes before the next epoch's
    /// owner starts a new one.
    pub rekey_interval: u32,
}

impl Default for PqParams {
    fn default() -> Self {
        PqParams {
            fragment_bytes: 128,
            rekey_interval: 64,
        }
    }
}

impl PqParams {
    fn validated(self) -> Result<Self> {
        if self.fragment_bytes == 0 || self.fragment_bytes > MAX_FRAGMENT_BYTES {
            return Err(Error::PolicyBound("fragment_bytes out of range"));
        }
        // Must be able to carry the larger of the two payloads.
        let needed = MLKEM_EK.div_ceil(self.fragment_bytes);
        if needed as u32 > MAX_FRAGMENTS {
            return Err(Error::PolicyBound("fragment_bytes too small"));
        }
        Ok(self)
    }
}

/// A fragment as it appears inside the encrypted header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fragment {
    pub kind: u8,
    pub epoch: u32,
    pub index: u32,
    pub total: u32,
    pub data: Vec<u8>,
}

impl Fragment {
    pub fn encode(&self, w: &mut Writer) -> Result<()> {
        w.u8(self.kind);
        w.varint(self.epoch);
        w.varint(self.index);
        w.varint(self.total);
        w.var_bytes(&self.data)?;
        Ok(())
    }

    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let kind = r.u8()?;
        if kind != FRAG_KIND_EK && kind != FRAG_KIND_CT {
            return Err(Error::Malformed("unknown fragment kind"));
        }
        let epoch = r.varint()?;
        let index = r.varint()?;
        let total = r.varint()?;
        if total == 0 || total > MAX_FRAGMENTS {
            return Err(Error::PolicyBound("fragment total out of range"));
        }
        if index >= total {
            return Err(Error::Malformed("fragment index >= total"));
        }
        let data = r.var_bytes()?;
        if data.is_empty() || data.len() > MAX_FRAGMENT_BYTES {
            return Err(Error::PolicyBound("fragment payload out of range"));
        }
        Ok(Fragment {
            kind,
            epoch,
            index,
            total,
            data: data.to_vec(),
        })
    }
}

/// Outgoing fragment stream: a byte payload plus a round-robin cursor.
#[derive(Clone, Debug)]
struct Outbox {
    kind: u8,
    epoch: u32,
    payload: Vec<u8>,
    chunk: usize,
    cursor: u32,
}

impl Outbox {
    fn total(&self) -> u32 {
        // `chunk` is validated non-zero by `PqParams::validated`.
        self.payload.len().div_ceil(self.chunk).max(1) as u32
    }

    /// Next fragment to transmit.
    ///
    /// The cursor is *rotated by one extra step on every pass* rather
    /// than being a plain `cursor % total`. This matters: a plain
    /// round-robin has period exactly `total`, so a carrier that drops
    /// messages on any period dividing `total` — every other message,
    /// every third — aliases perfectly and the same fragment indices
    /// are lost forever. The epoch then never completes even though
    /// half the messages get through. Rotating each pass makes the
    /// emission sequence period `total * total`, so no fixed periodic
    /// drop pattern can starve a fragment indefinitely. There is a
    /// regression test for exactly this.
    fn next(&mut self) -> Fragment {
        let total = self.total();
        let pass = self.cursor / total;
        let idx = (self.cursor.wrapping_add(pass)) % total;
        self.cursor = self.cursor.wrapping_add(1);
        let start = (idx as usize).saturating_mul(self.chunk);
        let end = start.saturating_add(self.chunk).min(self.payload.len());
        let data = self
            .payload
            .get(start..end)
            .unwrap_or(&[])
            .to_vec();
        Fragment {
            kind: self.kind,
            epoch: self.epoch,
            index: idx,
            total,
            data,
        }
    }
}

/// Incoming fragment reassembly for exactly one (kind, epoch) stream.
#[derive(Clone, Debug)]
struct Assembler {
    kind: u8,
    epoch: u32,
    total: u32,
    chunks: Vec<Option<Vec<u8>>>,
    have: u32,
}

impl Assembler {
    fn new(kind: u8, epoch: u32, total: u32) -> Self {
        Assembler {
            kind,
            epoch,
            total,
            chunks: vec![None; total as usize],
            have: 0,
        }
    }

    fn accept(&mut self, f: &Fragment) -> Result<Option<Vec<u8>>> {
        if f.total != self.total {
            return Err(Error::Malformed("fragment total changed mid-stream"));
        }
        let slot = self
            .chunks
            .get_mut(f.index as usize)
            .ok_or(Error::Malformed("fragment index out of range"))?;
        if slot.is_none() {
            *slot = Some(f.data.clone());
            self.have = self.have.saturating_add(1);
        }
        if self.have < self.total {
            return Ok(None);
        }
        let mut out = Vec::with_capacity(self.total as usize * f.data.len());
        for c in &self.chunks {
            match c {
                Some(bytes) => out.extend_from_slice(bytes),
                // Unreachable given `have == total`, but returned as an
                // error rather than unwrapped.
                None => return Err(Error::Internal("assembler hole")),
            }
        }
        Ok(Some(out))
    }
}

/// Which side we are. Determines epoch ownership parity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Initiator,
    Responder,
}

impl Role {
    /// Epoch `e` is owned by the responder when `e` is even.
    pub fn owns(self, epoch: u32) -> bool {
        match self {
            Role::Responder => epoch % 2 == 0,
            Role::Initiator => epoch % 2 == 1,
        }
    }
}

/// State of the PQ epoch ratchet for one session.
#[derive(Clone)]
pub struct PqRatchet {
    params: PqParams,
    role: Role,
    /// Highest epoch whose secret we hold *and* may announce.
    pq_have: u32,
    /// Highest epoch already folded into the root key.
    mixed: u32,
    /// Highest `pq_have` the peer has advertised to us.
    peer_have: u32,
    /// Epoch secrets we hold, keyed by epoch. Pruned to
    /// `(mixed, ..]` plus the current one.
    secrets: BTreeMap<u32, Secret32>,
    /// Decapsulation key for an epoch we own and have published.
    own_kem: Option<(u32, KemSecret)>,
    /// Highest epoch we have already answered as the *non-owner*.
    ///
    /// Load-bearing for correctness, not an optimisation. Encapsulation
    /// is randomised, so answering the same encapsulation key twice
    /// produces a *different* shared secret. On a carrier that
    /// duplicates and reorders, a late copy of an already-assembled
    /// fragment stream would otherwise re-complete the assembler and
    /// silently fork the epoch secret: the owner keeps the secret from
    /// the first ciphertext, we replace ours with the second, and the
    /// next root step diverges into an unrecoverable AEAD failure.
    /// There is a regression test for exactly this.
    answered: Option<u32>,
    outbox: Option<Outbox>,
    inbox: Option<Assembler>,
    /// Messages sent since the last epoch completed. Drives rekey.
    since_complete: u32,
}

impl PqRatchet {
    /// Bootstrap from the handshake. `handshake_secret` becomes epoch
    /// 0's secret and is already folded into the initial root key by
    /// the handshake itself, so `mixed` starts at 0 too.
    pub fn new(role: Role, params: PqParams, handshake_secret: Secret32) -> Result<Self> {
        let params = params.validated()?;
        let mut secrets = BTreeMap::new();
        secrets.insert(0u32, handshake_secret);
        Ok(PqRatchet {
            params,
            role,
            pq_have: 0,
            mixed: 0,
            peer_have: 0,
            secrets,
            own_kem: None,
            answered: None,
            outbox: None,
            inbox: None,
            since_complete: 0,
        })
    }

    pub fn pq_have(&self) -> u32 {
        self.pq_have
    }
    pub fn mixed(&self) -> u32 {
        self.mixed
    }
    pub fn peer_have(&self) -> u32 {
        self.peer_have
    }
    pub fn params(&self) -> PqParams {
        self.params
    }
    pub fn retained_secrets(&self) -> usize {
        self.secrets.len()
    }
    pub fn has_outbox(&self) -> bool {
        self.outbox.is_some()
    }

    /// The epoch a new sending chain should announce as mixed.
    ///
    /// Both sides must be able to perform the mix, so this is the
    /// minimum of what we hold and what the peer has told us it holds.
    pub fn mix_target(&self) -> u32 {
        self.pq_have.min(self.peer_have)
    }

    /// Called by the session at each outgoing message. Returns the
    /// fragment to attach, if any.
    pub fn on_send<R: RngCore + CryptoRng>(&mut self, rng: &mut R) -> Result<Option<Fragment>> {
        self.since_complete = self.since_complete.saturating_add(1);
        self.maybe_start_epoch(rng)?;
        Ok(self.outbox.as_mut().map(Outbox::next))
    }

    /// Owner-side epoch initiation. Deterministic: only the owner of
    /// `pq_have + 1` ever starts it, only when nothing else is in
    /// flight and the rekey interval has elapsed.
    fn maybe_start_epoch<R: RngCore + CryptoRng>(&mut self, rng: &mut R) -> Result<()> {
        if self.outbox.is_some() || self.own_kem.is_some() {
            return Ok(());
        }
        if self.since_complete < self.params.rekey_interval {
            return Ok(());
        }
        let next = self.pq_have.saturating_add(1);
        if next == self.pq_have {
            // u32 saturation: refuse to wrap the epoch counter.
            return Ok(());
        }
        if !self.role.owns(next) {
            return Ok(());
        }
        // Back-pressure: never let the unmixed window grow unbounded.
        if self.secrets.len() >= MAX_RETAINED_EPOCHS {
            return Ok(());
        }
        let (dk, ek) = kem_keypair(rng);
        self.own_kem = Some((next, dk));
        self.outbox = Some(Outbox {
            kind: FRAG_KIND_EK,
            epoch: next,
            payload: ek.to_bytes().to_vec(),
            chunk: self.params.fragment_bytes,
            cursor: 0,
        });
        Ok(())
    }

    /// Called with the `pq_have` a peer advertised in an authenticated
    /// header. Monotone; stale values are ignored.
    pub fn on_peer_have(&mut self, peer_have: u32) {
        if peer_have <= self.peer_have {
            return;
        }
        self.peer_have = peer_have;
        // Non-owner promotion: we have held the secret since we
        // encapsulated, but only now know the owner received it.
        if peer_have > self.pq_have && self.secrets.contains_key(&peer_have) {
            self.pq_have = peer_have;
            self.since_complete = 0;
        }
        // Our ciphertext has been acknowledged; stop retransmitting.
        if let Some(ob) = &self.outbox {
            if ob.kind == FRAG_KIND_CT && ob.epoch <= peer_have {
                self.outbox = None;
            }
        }
    }

    /// Process an authenticated inbound fragment.
    ///
    /// Fragments arrive inside the AEAD-authenticated header, so only
    /// the real peer can reach this code. The validation below still
    /// treats the peer as untrusted: a buggy or malicious *peer* must
    /// not be able to grow our memory or spin our CPU.
    pub fn on_fragment<R: RngCore + CryptoRng>(
        &mut self,
        f: &Fragment,
        rng: &mut R,
    ) -> Result<()> {
        // Ownership sanity: only the owner publishes an EK, only the
        // non-owner replies with a CT.
        match f.kind {
            FRAG_KIND_EK => {
                if self.role.owns(f.epoch) {
                    return Err(Error::Malformed("EK fragment for an epoch we own"));
                }
            }
            FRAG_KIND_CT => {
                if !self.role.owns(f.epoch) {
                    return Err(Error::Malformed("CT fragment for an epoch we do not own"));
                }
            }
            _ => return Err(Error::Malformed("unknown fragment kind")),
        }

        // Stale or absurdly-ahead epochs are dropped silently: a
        // retransmission that races an epoch completion is normal.
        if f.epoch <= self.pq_have {
            return Ok(());
        }
        // Idempotence for the non-owner: never answer the same
        // encapsulation key twice (see `answered`).
        if f.kind == FRAG_KIND_EK {
            if let Some(answered) = self.answered {
                if f.epoch <= answered {
                    return Ok(());
                }
            }
        }
        if f.epoch > self.pq_have.saturating_add(MAX_EPOCH_LOOKAHEAD) {
            return Err(Error::PolicyBound("PQ epoch too far ahead"));
        }

        // A different stream supersedes any partial assembly.
        let restart = match &self.inbox {
            Some(a) => a.epoch != f.epoch || a.kind != f.kind,
            None => true,
        };
        if restart {
            self.inbox = Some(Assembler::new(f.kind, f.epoch, f.total));
        }
        let asm = self
            .inbox
            .as_mut()
            .ok_or(Error::Internal("assembler missing"))?;
        let complete = asm.accept(f)?;
        let Some(payload) = complete else {
            return Ok(());
        };
        let (kind, epoch) = (asm.kind, asm.epoch);
        self.inbox = None;

        match kind {
            FRAG_KIND_EK => self.on_ek_complete(epoch, &payload, rng),
            FRAG_KIND_CT => self.on_ct_complete(epoch, &payload),
            _ => Err(Error::Internal("unreachable fragment kind")),
        }
    }

    fn on_ek_complete<R: RngCore + CryptoRng>(
        &mut self,
        epoch: u32,
        payload: &[u8],
        rng: &mut R,
    ) -> Result<()> {
        if payload.len() != MLKEM_EK {
            return Err(Error::Malformed("reassembled EK wrong length"));
        }
        let ek = KemPublic::from_slice(payload)?;
        let (ct, ss) = kem_encapsulate(&ek, rng)?;
        self.insert_secret(epoch, ss)?;
        self.answered = Some(epoch);
        // Reply with the ciphertext; retransmit until the owner acks
        // by advertising pq_have >= epoch.
        self.outbox = Some(Outbox {
            kind: FRAG_KIND_CT,
            epoch,
            payload: ct.to_vec(),
            chunk: self.params.fragment_bytes,
            cursor: 0,
        });
        Ok(())
    }

    fn on_ct_complete(&mut self, epoch: u32, payload: &[u8]) -> Result<()> {
        let Some((own_epoch, dk)) = &self.own_kem else {
            // No outstanding published key: a late duplicate. Ignore.
            return Ok(());
        };
        if *own_epoch != epoch {
            return Ok(());
        }
        let ct: [u8; MLKEM_CT] = payload
            .try_into()
            .map_err(|_| Error::Malformed("reassembled CT wrong length"))?;
        // Implicit rejection means this cannot fail; a wrong secret
        // simply makes the next root step diverge and surfaces as an
        // AEAD failure, never as an oracle here.
        let ss = kem_decapsulate(dk, &ct)?;
        self.insert_secret(epoch, ss)?;
        self.own_kem = None;
        self.outbox = None;
        self.pq_have = epoch;
        self.since_complete = 0;
        Ok(())
    }

    fn insert_secret(&mut self, epoch: u32, ss: Secret32) -> Result<()> {
        if self.secrets.len() >= MAX_RETAINED_EPOCHS && !self.secrets.contains_key(&epoch) {
            return Err(Error::PolicyBound("too many unmixed PQ epochs"));
        }
        self.secrets.insert(epoch, ss);
        Ok(())
    }

    /// Collect the secrets a root step must fold in, and advance
    /// `mixed`. Returns them in ascending epoch order.
    ///
    /// Fails closed with [`Error::MissingPqEpoch`] rather than
    /// silently mixing nothing: a missing secret means the two sides
    /// would derive different roots, and diverging silently is worse
    /// than refusing the message.
    pub fn take_mix(&mut self, mix_epoch: u32) -> Result<Vec<Secret32>> {
        if mix_epoch <= self.mixed {
            return Ok(Vec::new());
        }
        if mix_epoch > self.pq_have {
            return Err(Error::MissingPqEpoch { epoch: mix_epoch });
        }
        let mut out = Vec::new();
        for e in (self.mixed + 1)..=mix_epoch {
            let s = self
                .secrets
                .get(&e)
                .cloned()
                .ok_or(Error::MissingPqEpoch { epoch: e })?;
            out.push(s);
        }
        self.mixed = mix_epoch;
        self.prune();
        Ok(out)
    }

    /// Read-only variant used when a *sender* builds a chain: the
    /// sender must mix exactly what the receiver will mix.
    pub fn peek_mix(&self, mix_epoch: u32) -> Result<Vec<Secret32>> {
        if mix_epoch <= self.mixed {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for e in (self.mixed + 1)..=mix_epoch {
            let s = self
                .secrets
                .get(&e)
                .cloned()
                .ok_or(Error::MissingPqEpoch { epoch: e })?;
            out.push(s);
        }
        Ok(out)
    }

    /// Commit a mix the local sender already performed.
    pub fn commit_mix(&mut self, mix_epoch: u32) {
        if mix_epoch > self.mixed {
            self.mixed = mix_epoch;
            self.prune();
        }
    }

    /// Drop secrets that can never be needed again. Forward secrecy:
    /// a mixed epoch secret is gone from memory (and zeroized by
    /// `Secret32`'s `Drop`) once the root has absorbed it.
    fn prune(&mut self) {
        let mixed = self.mixed;
        self.secrets.retain(|e, _| *e > mixed);
    }

    // -- state export (see `crate::state`) --

    pub(crate) fn export(&self, w: &mut Writer) -> Result<()> {
        w.varint(self.params.fragment_bytes as u32);
        w.varint(self.params.rekey_interval);
        w.u8(match self.role {
            Role::Initiator => 0,
            Role::Responder => 1,
        });
        w.varint(self.pq_have);
        w.varint(self.mixed);
        w.varint(self.peer_have);
        w.varint(self.since_complete);
        w.varint(self.secrets.len() as u32);
        for (e, s) in &self.secrets {
            w.varint(*e);
            w.bytes(s.as_bytes());
        }
        match &self.own_kem {
            Some((e, dk)) => {
                w.u8(1);
                w.varint(*e);
                w.bytes(&dk.to_bytes());
            }
            None => {
                w.u8(0);
            }
        }
        match self.answered {
            Some(e) => {
                w.u8(1);
                w.varint(e);
            }
            None => {
                w.u8(0);
            }
        }
        match &self.outbox {
            Some(ob) => {
                w.u8(1);
                w.u8(ob.kind);
                w.varint(ob.epoch);
                w.varint(ob.cursor);
                w.var_bytes(&ob.payload)?;
            }
            None => {
                w.u8(0);
            }
        }
        match &self.inbox {
            Some(asm) => {
                w.u8(1);
                w.u8(asm.kind);
                w.varint(asm.epoch);
                w.varint(asm.total);
                for c in &asm.chunks {
                    match c {
                        Some(bytes) => {
                            w.u8(1);
                            w.var_bytes(bytes)?;
                        }
                        None => {
                            w.u8(0);
                        }
                    }
                }
            }
            None => {
                w.u8(0);
            }
        }
        Ok(())
    }

    pub(crate) fn import(r: &mut Reader<'_>) -> Result<Self> {
        let fragment_bytes = r.varint()? as usize;
        let rekey_interval = r.varint()?;
        let params = PqParams {
            fragment_bytes,
            rekey_interval,
        }
        .validated()?;
        let role = match r.u8()? {
            0 => Role::Initiator,
            1 => Role::Responder,
            _ => return Err(Error::BadStateFormat),
        };
        let pq_have = r.varint()?;
        let mixed = r.varint()?;
        let peer_have = r.varint()?;
        let since_complete = r.varint()?;
        let n = r.varint()?;
        if n as usize > MAX_RETAINED_EPOCHS {
            return Err(Error::BadStateFormat);
        }
        let mut secrets = BTreeMap::new();
        for _ in 0..n {
            let e = r.varint()?;
            let s = r.array::<32>()?;
            secrets.insert(e, Secret32::from_bytes(s));
        }
        let own_kem = if r.u8()? == 1 {
            let e = r.varint()?;
            let dk = r.array::<{ crate::primitives::MLKEM_DK }>()?;
            Some((e, KemSecret::from_bytes(&dk)?))
        } else {
            None
        };
        let answered = match r.u8()? {
            0 => None,
            1 => Some(r.varint()?),
            _ => return Err(Error::BadStateFormat),
        };
        let outbox = if r.u8()? == 1 {
            let kind = r.u8()?;
            if kind != FRAG_KIND_EK && kind != FRAG_KIND_CT {
                return Err(Error::BadStateFormat);
            }
            let epoch = r.varint()?;
            let cursor = r.varint()?;
            let payload = r.var_bytes()?.to_vec();
            if payload.len() != MLKEM_EK && payload.len() != MLKEM_CT {
                return Err(Error::BadStateFormat);
            }
            Some(Outbox {
                kind,
                epoch,
                payload,
                chunk: fragment_bytes,
                cursor,
            })
        } else {
            None
        };
        let inbox = if r.u8()? == 1 {
            let kind = r.u8()?;
            if kind != FRAG_KIND_EK && kind != FRAG_KIND_CT {
                return Err(Error::BadStateFormat);
            }
            let epoch = r.varint()?;
            let total = r.varint()?;
            if total == 0 || total > MAX_FRAGMENTS {
                return Err(Error::BadStateFormat);
            }
            let mut chunks = Vec::with_capacity(total as usize);
            let mut have = 0u32;
            for _ in 0..total {
                if r.u8()? == 1 {
                    let bytes = r.var_bytes()?;
                    if bytes.len() > MAX_FRAGMENT_BYTES {
                        return Err(Error::BadStateFormat);
                    }
                    chunks.push(Some(bytes.to_vec()));
                    have += 1;
                } else {
                    chunks.push(None);
                }
            }
            Some(Assembler {
                kind,
                epoch,
                total,
                chunks,
                have,
            })
        } else {
            None
        };
        Ok(PqRatchet {
            params,
            role,
            pq_have,
            mixed,
            peer_have,
            secrets,
            own_kem,
            answered,
            outbox,
            inbox,
            since_complete,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    fn rng(seed: u64) -> ChaCha20Rng {
        ChaCha20Rng::seed_from_u64(seed)
    }

    #[test]
    fn ownership_alternates_and_is_complementary() {
        for e in 0..8u32 {
            assert_ne!(
                Role::Initiator.owns(e),
                Role::Responder.owns(e),
                "epoch {e} must have exactly one owner"
            );
        }
        assert!(Role::Responder.owns(0));
        assert!(Role::Initiator.owns(1));
    }

    /// Drive a full epoch to completion with no loss.
    #[test]
    fn full_epoch_completes() {
        let mut r = rng(1);
        let params = PqParams {
            fragment_bytes: 128,
            rekey_interval: 1,
        };
        let hs = Secret32::from_bytes([9u8; 32]);
        let mut init = PqRatchet::new(Role::Initiator, params, hs.clone()).expect("init");
        let mut resp = PqRatchet::new(Role::Responder, params, hs).expect("resp");

        // Initiator owns epoch 1 and starts publishing its EK.
        for _ in 0..64 {
            if let Some(f) = init.on_send(&mut r).expect("send") {
                resp.on_fragment(&f, &mut r).expect("frag");
            }
            resp.on_peer_have(init.pq_have());
            if let Some(f) = resp.on_send(&mut r).expect("send") {
                init.on_fragment(&f, &mut r).expect("frag");
            }
            init.on_peer_have(resp.pq_have());
            if init.pq_have() >= 1 && resp.pq_have() >= 1 {
                break;
            }
        }
        assert_eq!(init.pq_have(), 1);
        assert_eq!(resp.pq_have(), 1);
        // Both derived the same secret.
        assert_eq!(
            init.peek_mix(1).expect("peek"),
            resp.peek_mix(1).expect("peek")
        );
    }

    /// Every second fragment is dropped forever. The epoch must still
    /// complete, just later — that is the loss-tolerance claim.
    #[test]
    fn epoch_completes_under_heavy_fragment_loss() {
        let mut r = rng(2);
        let params = PqParams {
            fragment_bytes: 128,
            rekey_interval: 1,
        };
        let hs = Secret32::from_bytes([4u8; 32]);
        let mut init = PqRatchet::new(Role::Initiator, params, hs.clone()).expect("init");
        let mut resp = PqRatchet::new(Role::Responder, params, hs).expect("resp");

        let mut tick = 0u32;
        for _ in 0..400 {
            tick += 1;
            if let Some(f) = init.on_send(&mut r).expect("send") {
                if tick % 2 == 0 {
                    resp.on_fragment(&f, &mut r).expect("frag");
                }
            }
            resp.on_peer_have(init.pq_have());
            if let Some(f) = resp.on_send(&mut r).expect("send") {
                if tick % 3 != 0 {
                    init.on_fragment(&f, &mut r).expect("frag");
                }
            }
            init.on_peer_have(resp.pq_have());
            if init.pq_have() >= 1 && resp.pq_have() >= 1 {
                break;
            }
        }
        assert_eq!(init.pq_have(), 1, "epoch stalled under loss");
        assert_eq!(resp.pq_have(), 1);
    }

    /// Regression: a duplicated encapsulation-key fragment stream must
    /// not make the non-owner encapsulate a second time. Encapsulation
    /// is randomised, so a second answer silently forks the epoch
    /// secret and the two sides' root keys diverge forever.
    #[test]
    fn duplicate_ek_stream_does_not_fork_the_epoch_secret() {
        let mut r = rng(21);
        let params = PqParams {
            fragment_bytes: 128,
            rekey_interval: 1,
        };
        let hs = Secret32::from_bytes([6u8; 32]);
        let mut init = PqRatchet::new(Role::Initiator, params, hs.clone()).expect("init");
        let mut resp = PqRatchet::new(Role::Responder, params, hs).expect("resp");

        // Capture one full pass of the initiator's EK fragments.
        let mut ek_frags = Vec::new();
        for _ in 0..10 {
            if let Some(f) = init.on_send(&mut r).expect("send") {
                ek_frags.push(f);
            }
        }
        for f in &ek_frags {
            resp.on_fragment(f, &mut r).expect("frag");
        }
        let first = resp.peek_mix(1).expect("secret after first answer");
        assert_eq!(first.len(), 1);

        // Replay the whole stream. The answer must be unchanged.
        for f in &ek_frags {
            resp.on_fragment(f, &mut r).expect("duplicate frag");
        }
        assert_eq!(
            resp.peek_mix(1).expect("secret after replay"),
            first,
            "duplicate EK stream forked the epoch secret"
        );
    }

    #[test]
    fn wrong_direction_fragments_are_rejected() {
        let mut r = rng(3);
        let mut init = PqRatchet::new(
            Role::Initiator,
            PqParams::default(),
            Secret32::from_bytes([0u8; 32]),
        )
        .expect("init");
        // Initiator owns epoch 1, so an EK for epoch 1 is illegal.
        let f = Fragment {
            kind: FRAG_KIND_EK,
            epoch: 1,
            index: 0,
            total: 1,
            data: vec![0u8; 8],
        };
        assert!(init.on_fragment(&f, &mut r).is_err());
        // Epoch 2 is the responder's, so a CT for epoch 2 is illegal.
        let f = Fragment {
            kind: FRAG_KIND_CT,
            epoch: 2,
            index: 0,
            total: 1,
            data: vec![0u8; 8],
        };
        assert!(init.on_fragment(&f, &mut r).is_err());
    }

    #[test]
    fn far_future_epoch_is_refused() {
        let mut r = rng(4);
        let mut init = PqRatchet::new(
            Role::Initiator,
            PqParams::default(),
            Secret32::from_bytes([0u8; 32]),
        )
        .expect("init");
        let f = Fragment {
            kind: FRAG_KIND_CT,
            epoch: 999,
            index: 0,
            total: 1,
            data: vec![0u8; 8],
        };
        assert_eq!(
            init.on_fragment(&f, &mut r),
            Err(Error::PolicyBound("PQ epoch too far ahead"))
        );
    }

    #[test]
    fn fragment_decode_rejects_out_of_range() {
        let mut w = Writer::default();
        Fragment {
            kind: FRAG_KIND_EK,
            epoch: 1,
            index: 0,
            total: MAX_FRAGMENTS + 1,
            data: vec![1, 2, 3],
        }
        .encode(&mut w)
        .expect("encode");
        assert!(Fragment::decode(&mut Reader::new(w.as_slice())).is_err());

        let mut w = Writer::default();
        Fragment {
            kind: 7,
            epoch: 1,
            index: 0,
            total: 1,
            data: vec![1],
        }
        .encode(&mut w)
        .expect("encode");
        assert!(Fragment::decode(&mut Reader::new(w.as_slice())).is_err());
    }

    #[test]
    fn take_mix_fails_closed_for_unheld_epoch() {
        let mut p = PqRatchet::new(
            Role::Initiator,
            PqParams::default(),
            Secret32::from_bytes([1u8; 32]),
        )
        .expect("new");
        assert_eq!(p.take_mix(5), Err(Error::MissingPqEpoch { epoch: 5 }));
        // Epoch 0 is already mixed by construction.
        assert!(p.take_mix(0).expect("mix").is_empty());
    }

    #[test]
    fn mixed_secrets_are_pruned() {
        let mut p = PqRatchet::new(
            Role::Initiator,
            PqParams::default(),
            Secret32::from_bytes([1u8; 32]),
        )
        .expect("new");
        p.insert_secret(1, Secret32::from_bytes([2u8; 32]))
            .expect("insert");
        p.pq_have = 1;
        assert_eq!(p.retained_secrets(), 2);
        let mixed = p.take_mix(1).expect("mix");
        assert_eq!(mixed.len(), 1);
        assert_eq!(p.retained_secrets(), 0);
    }
}
