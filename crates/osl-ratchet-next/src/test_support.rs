//! Deterministic two-party harness.
//!
//! Public (not `#[cfg(test)]`) so integration tests and reviewers can
//! reproduce the exact same runs. Everything is driven from a seeded
//! ChaCha20 CSPRNG, so a failure is replayable from its seed alone.
//!
//! Both parties share one RNG. That is fine — and in fact useful —
//! because no security property here depends on the two sides drawing
//! from independent streams; it just makes runs reproducible.

use crate::error::Result;
use crate::handshake::{LocalPrekeys, PeerBundle};
use crate::primitives::{kem_keypair, x25519_keypair, XSecret};
use crate::session::{Opened, Session, SessionParams};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

/// A seeded Alice/Bob pair. Bob's session materialises when he accepts
/// Alice's first message, mirroring the real asynchronous flow.
pub struct Harness {
    pub alice: Session,
    pub bob: Option<Session>,
    pub bob_prekeys: LocalPrekeys,
    pub rng: ChaCha20Rng,
    pub params: SessionParams,
}

impl Harness {
    pub fn new(seed: u64) -> Self {
        Self::with_params(seed, SessionParams::default())
    }

    pub fn with_params(seed: u64, params: SessionParams) -> Self {
        let mut rng = ChaCha20Rng::seed_from_u64(seed);
        let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
        let (alice_ik, _) = x25519_keypair(&mut rng);
        let alice =
            Session::initiate(&alice_ik, &bob_bundle, params, &mut rng).expect("initiate session");
        Harness {
            alice,
            bob: None,
            bob_prekeys,
            rng,
            params,
        }
    }

    /// Drive the handshake to completion so both sessions exist.
    pub fn established(seed: u64) -> Self {
        Self::established_with_params(seed, SessionParams::default())
    }

    pub fn established_with_params(seed: u64, params: SessionParams) -> Self {
        let mut h = Self::with_params(seed, params);
        let w = h.alice_send(b"handshake").expect("alice send");
        h.bob_recv(&w).expect("bob recv");
        let w = h.bob_send(b"handshake ack").expect("bob send");
        h.alice_recv(&w).expect("alice recv");
        h
    }

    pub fn alice_send(&mut self, plaintext: &[u8]) -> Result<String> {
        self.alice.encrypt(0, plaintext, &mut self.rng)
    }

    pub fn bob_send(&mut self, plaintext: &[u8]) -> Result<String> {
        let bob = self.bob.as_mut().expect("bob session not established yet");
        bob.encrypt(0, plaintext, &mut self.rng)
    }

    pub fn alice_recv(&mut self, wire: &str) -> Result<Opened> {
        self.alice.decrypt(wire, &mut self.rng)
    }

    /// Accepts the bootstrap on first use, then decrypts normally.
    pub fn bob_recv(&mut self, wire: &str) -> Result<Opened> {
        match &mut self.bob {
            Some(bob) => bob.decrypt(wire, &mut self.rng),
            None => {
                let (bob, opened) =
                    Session::accept(&self.bob_prekeys, wire, self.params, &mut self.rng)?;
                self.bob = Some(bob);
                Ok(opened)
            }
        }
    }

    pub fn bob_ref(&self) -> &Session {
        self.bob.as_ref().expect("bob session not established yet")
    }

    pub fn bob_mut(&mut self) -> &mut Session {
        self.bob.as_mut().expect("bob session not established yet")
    }
}

/// Two fully-established sessions plus the shared seeded RNG, for
/// tests that want to drive both sides directly.
pub fn established_pair(seed: u64) -> (Session, Session, ChaCha20Rng) {
    established_pair_with_params(seed, SessionParams::default())
}

pub fn established_pair_with_params(
    seed: u64,
    params: SessionParams,
) -> (Session, Session, ChaCha20Rng) {
    let h = Harness::established_with_params(seed, params);
    let bob = h.bob.expect("bob established");
    (h.alice, bob, h.rng)
}

/// A fresh prekey bundle with one one-time prekey (id 7).
pub fn fresh_bundle(rng: &mut ChaCha20Rng) -> (LocalPrekeys, PeerBundle) {
    let (ik_s, ik_p) = x25519_keypair(rng);
    let (spk_s, spk_p) = x25519_keypair(rng);
    let (opk_s, opk_p) = x25519_keypair(rng);
    let (pq_s, pq_p) = kem_keypair(rng);
    (
        LocalPrekeys {
            identity: ik_s,
            signed_prekey: spk_s,
            one_time_prekeys: vec![(7, opk_s)],
            pq_prekey: pq_s,
        },
        PeerBundle {
            identity: ik_p,
            signed_prekey: spk_p,
            one_time_prekey: Some((7, opk_p)),
            pq_prekey: pq_p,
        },
    )
}

/// A seeded RNG, for tests that need one directly.
pub fn seeded_rng(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

/// A deterministic identity key, for tests that need one directly.
pub fn seeded_identity(seed: u64) -> XSecret {
    let mut rng = seeded_rng(seed);
    x25519_keypair(&mut rng).0
}

/// Flip one bit inside the base64 payload of a `DPC0::` blob, at
/// `byte_index` of the *decoded* bytes. Returns `None` if the index is
/// out of range.
pub fn tamper_byte(wire: &str, byte_index: usize, xor: u8) -> Option<String> {
    use base64::Engine as _;
    let body = wire.strip_prefix(crate::session::WIRE_PREFIX)?;
    let mut raw = base64::engine::general_purpose::STANDARD
        .decode(body)
        .ok()?;
    let b = raw.get_mut(byte_index)?;
    *b ^= xor;
    Some(format!(
        "{}{}",
        crate::session::WIRE_PREFIX,
        base64::engine::general_purpose::STANDARD.encode(&raw)
    ))
}

/// Decoded length of a `DPC0::` blob — the real per-message wire cost
/// before base64.
pub fn wire_len(wire: &str) -> usize {
    use base64::Engine as _;
    wire.strip_prefix(crate::session::WIRE_PREFIX)
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
        .map(|v| v.len())
        .unwrap_or(0)
}
