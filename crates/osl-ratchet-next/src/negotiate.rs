//! Version negotiation binding.
//!
//! # The problem, stated honestly
//!
//! TLS-style downgrade protection works because version negotiation is
//! **in band**: both parties see the same ClientHello/ServerHello bytes
//! and hash them into the key schedule, so tampering is detected. OSL
//! has no in-band negotiation. A sender picks a wire version
//! unilaterally, from a peer record it obtained out of band (a friend
//! code, or the key server). Nothing that happens *inside* the OSL-RN
//! handshake can stop an attacker who removes "supports OSL-RN" from that
//! record before the sender ever reaches this code — because in that
//! case no OSL-RN handshake runs at all.
//!
//! So downgrade protection here is three layers, and only two of them
//! live in this crate:
//!
//! | Layer | Where | What it stops |
//! | --- | --- | --- |
//! | **L1** Authenticated capability advertisement | key server (**spec only, not implemented**) | Stripping `OSL-RN` from a peer record in transit. Requires the advertisement to be inside the Ed25519-signed identity record. |
//! | **L2** Sticky monotone version pin | `ipc::wire_rn` | Falling back to v=3 for a peer already known to speak OSL-RN. Once raised, the pin makes a v=3 send to that peer **unreachable code**, not a discouraged path. |
//! | **L3** Negotiation binding | *this module* | Two parties silently proceeding while disagreeing about what was negotiated, and any reuse of v=3 key material in a OSL-RN session. Failure is closed: a mismatch yields a different `SK`, so the first message simply does not authenticate. |
//!
//! **There is deliberately no "try OSL-RN, fall back on error" anywhere.**
//! An authentication failure on a OSL-RN send or receive never causes a
//! v=3 retry: the pin is consulted *before* a version is chosen, and it
//! only ever moves upward.
//!
//! # What the binding covers, and why each field is safe to bind
//!
//! Every input below is a value **both parties know exactly**, with no
//! dependency on build version, wall clock, or anything an upgrade
//! could change out from under one side. That property is what stops
//! the binding from becoming a liveness cliff.
//!
//! ```text
//! digest = SHA-256(
//!       LP("OSL-RN/v1/negotiation/v1")
//!    || u8(selected_version)          both sides: 0x06, by construction
//!    || u8(min_acceptable_version)    both sides: the OSL-RN floor
//!    || LP(responder_identity_x25519) initiator: from the pinned peer record
//!                                     responder: its own identity
//!    || LP(responder_mlkem768_ek)     same
//!    || LP(initiator_identity_x25519) initiator: its own
//!                                     responder: from the bootstrap preamble
//!    || LP(context)                   a fixed per-call-path constant
//! )
//! LP(x) = u32_be(x.len()) || x
//! ```
//!
//! The digest is mixed into the PQXDH `SK` derivation as HKDF `info`
//! (see [`crate::handshake::initiate_bound`]). It is never transmitted.
//!
//! ## What this genuinely buys
//!
//! - **Cross-version key separation.** The same static X25519 identity
//!   key and the same ML-KEM-768 encapsulation key are used by the
//!   shipping `v=3` wrap and by `OSL-RN`. Binding the selected version
//!   into the key schedule guarantees no OSL-RN secret can coincide with,
//!   or be substituted by, material from a v=3 exchange over the same
//!   static keys. This is the same reason TLS 1.3 binds its version
//!   into the key schedule.
//! - **A floor that is cryptographic, not advisory.** `min_acceptable_version`
//!   is an input to `SK`. A peer that ran this handshake while believing
//!   a *lower* floor applied derives a different `SK` and fails closed,
//!   rather than completing a session under weaker assumptions than the
//!   other side thinks.
//! - **Initiator identity binding.** The initiator's identity key enters
//!   `SK` directly rather than only through `DH2`, so a preamble whose
//!   `initiator_identity` field was rewritten fails at key derivation.
//!
//! ## What it does not buy — say this out loud
//!
//! It **cannot** stop a first-contact downgrade, where the attacker
//! controls the peer record and the initiator therefore never selects
//! OSL-RN. That is L1 (key-server work, specified but not implemented) and
//! L2 (implemented). Anyone reading this module as "downgrade is solved"
//! is reading it wrong.

use crate::error::{Error, Result};
use crate::primitives::{MLKEM_EK, X25519_PUB};
use crate::session::WIRE_VERSION_RN;
use sha2::{Digest, Sha256};

/// Domain separator for the negotiation digest. Distinct from every
/// label in [`crate::kdf`] (asserted by a test there and here).
pub const NEGOTIATION_DOMAIN: &[u8] = b"OSL-RN/v1/negotiation/v1";

/// Upper bound on the caller-supplied context string. Keeps the digest
/// input bounded and the failure mode obvious.
pub const MAX_CONTEXT_BYTES: usize = 256;

/// The parameters of a version decision, as the two sides each compute
/// them.
///
/// Construct with [`Negotiation::for_rn`] rather than by hand where
/// possible — it fills in the two version bytes with the only values
/// this crate accepts, which is precisely what makes both sides agree.
#[derive(Clone, Copy)]
pub struct Negotiation<'a> {
    /// The wire version actually being used. Must be [`WIRE_VERSION_RN`].
    pub selected_version: u8,
    /// The lowest wire version the *initiator* would have accepted.
    /// Must be `>= WIRE_VERSION_RN`: this crate refuses to participate
    /// in a session whose floor admits a non-ratcheting version.
    pub min_acceptable_version: u8,
    /// Responder's long-term X25519 identity public key.
    pub responder_identity: &'a [u8; X25519_PUB],
    /// Responder's published ML-KEM-768 encapsulation key.
    pub responder_mlkem_ek: &'a [u8],
    /// Initiator's long-term X25519 identity public key.
    pub initiator_identity: &'a [u8; X25519_PUB],
    /// A fixed constant identifying the application call path, so a
    /// digest from one path can never be replayed into another.
    pub context: &'a [u8],
}

/// Deliberately redacted: the inputs are all public, but printing them
/// into a log line is still needless metadata leakage (it names exactly
/// who is talking to whom).
impl core::fmt::Debug for Negotiation<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Negotiation")
            .field("selected_version", &self.selected_version)
            .field("min_acceptable_version", &self.min_acceptable_version)
            .field("responder_identity", &"<redacted>")
            .field("responder_mlkem_ek", &"<redacted>")
            .field("initiator_identity", &"<redacted>")
            .field("context", &"<redacted>")
            .finish()
    }
}

impl<'a> Negotiation<'a> {
    /// The only shape this crate accepts today: OSL-RN selected, OSL-RN floor.
    ///
    /// Both sides call this, so both sides agree on the two version
    /// bytes by construction — there is no build-version skew hazard.
    pub fn for_rn(
        responder_identity: &'a [u8; X25519_PUB],
        responder_mlkem_ek: &'a [u8],
        initiator_identity: &'a [u8; X25519_PUB],
        context: &'a [u8],
    ) -> Self {
        Negotiation {
            selected_version: WIRE_VERSION_RN,
            min_acceptable_version: WIRE_VERSION_RN,
            responder_identity,
            responder_mlkem_ek,
            initiator_identity,
            context,
        }
    }

    /// Compute the binding digest.
    ///
    /// Fails closed on any out-of-policy input rather than hashing it:
    /// a caller that hands us a downgraded floor gets an error, not a
    /// session.
    pub fn digest(&self) -> Result<[u8; 32]> {
        if self.selected_version != WIRE_VERSION_RN {
            return Err(Error::PolicyBound(
                "negotiation: selected version is not OSL-RN",
            ));
        }
        if self.min_acceptable_version < WIRE_VERSION_RN {
            return Err(Error::PolicyBound(
                "negotiation: minimum acceptable version is below OSL-RN",
            ));
        }
        if self.min_acceptable_version > self.selected_version {
            return Err(Error::PolicyBound(
                "negotiation: floor exceeds the selected version",
            ));
        }
        if self.responder_mlkem_ek.len() != MLKEM_EK {
            return Err(Error::PolicyBound(
                "negotiation: responder ML-KEM encapsulation key has the wrong length",
            ));
        }
        if self.context.is_empty() || self.context.len() > MAX_CONTEXT_BYTES {
            return Err(Error::PolicyBound(
                "negotiation: context length out of range",
            ));
        }

        let mut h = Sha256::new();
        length_prefixed(&mut h, NEGOTIATION_DOMAIN);
        h.update([self.selected_version]);
        h.update([self.min_acceptable_version]);
        length_prefixed(&mut h, self.responder_identity);
        length_prefixed(&mut h, self.responder_mlkem_ek);
        length_prefixed(&mut h, self.initiator_identity);
        length_prefixed(&mut h, self.context);
        Ok(h.finalize().into())
    }
}

/// `u32_be(len) || bytes`. Unambiguous framing: no two distinct input
/// tuples can produce the same byte stream, so the digest cannot be
/// collided by shifting bytes between adjacent fields.
fn length_prefixed(h: &mut Sha256, bytes: &[u8]) {
    // Every caller passes a fixed-size array or a length-checked slice,
    // all far below u32::MAX; the saturating cast is unreachable in
    // practice and is written explicitly rather than via `as`.
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    h.update(len.to_be_bytes());
    h.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    const EK: [u8; MLKEM_EK] = [7u8; MLKEM_EK];
    const CTX: &[u8] = b"osl/test/v1";

    fn base<'a>(r: &'a [u8; 32], i: &'a [u8; 32]) -> Negotiation<'a> {
        Negotiation::for_rn(r, &EK, i, CTX)
    }

    #[test]
    fn digest_is_deterministic_and_input_sensitive() {
        let r = [1u8; 32];
        let i = [2u8; 32];
        let a = base(&r, &i).digest().expect("digest");
        assert_eq!(a, base(&r, &i).digest().expect("digest"));

        // Swapping the two identities must change the digest: the
        // construction is not symmetric in the parties.
        let swapped = Negotiation::for_rn(&i, &EK, &r, CTX)
            .digest()
            .expect("digest");
        assert_ne!(a, swapped);

        // A different context is a different digest.
        let other_ctx = Negotiation::for_rn(&r, &EK, &i, b"osl/other/v1")
            .digest()
            .expect("digest");
        assert_ne!(a, other_ctx);

        // A different responder KEM key is a different digest.
        let other_ek = [8u8; MLKEM_EK];
        let swapped_ek = Negotiation::for_rn(&r, &other_ek, &i, CTX)
            .digest()
            .expect("digest");
        assert_ne!(a, swapped_ek);
    }

    #[test]
    fn a_downgraded_floor_is_refused_not_hashed() {
        let r = [1u8; 32];
        let i = [2u8; 32];
        for floor in 0u8..WIRE_VERSION_RN {
            let mut n = base(&r, &i);
            n.min_acceptable_version = floor;
            assert!(
                matches!(n.digest(), Err(Error::PolicyBound(_))),
                "floor {floor} must be refused"
            );
        }
    }

    #[test]
    fn a_non_v6_selection_is_refused() {
        let r = [1u8; 32];
        let i = [2u8; 32];
        for v in [0x02u8, 0x03, 0x04, 0x05, 0x07, 0xFF] {
            let mut n = base(&r, &i);
            n.selected_version = v;
            assert!(matches!(n.digest(), Err(Error::PolicyBound(_))));
        }
    }

    #[test]
    fn malformed_lengths_are_refused() {
        let r = [1u8; 32];
        let i = [2u8; 32];

        let short_ek = [0u8; 16];
        let mut n = base(&r, &i);
        n.responder_mlkem_ek = &short_ek;
        assert!(matches!(n.digest(), Err(Error::PolicyBound(_))));

        let mut n = base(&r, &i);
        n.context = b"";
        assert!(matches!(n.digest(), Err(Error::PolicyBound(_))));

        let long = [b'x'; MAX_CONTEXT_BYTES + 1];
        let mut n = base(&r, &i);
        n.context = &long;
        assert!(matches!(n.digest(), Err(Error::PolicyBound(_))));
    }

    #[test]
    fn length_prefixing_prevents_field_boundary_collisions() {
        // Without length prefixes, moving a byte from the end of the
        // responder identity to the start of the KEM key would collide.
        // With them, it cannot. Construct the closest analogue we can
        // with fixed-size fields: two contexts whose concatenation with
        // the preceding field would otherwise be equal.
        let r = [0u8; 32];
        let i = [0u8; 32];
        let a = Negotiation::for_rn(&r, &EK, &i, b"ab").digest().expect("d");
        let b = Negotiation::for_rn(&r, &EK, &i, b"a").digest().expect("d");
        assert_ne!(a, b);
    }

    #[test]
    fn negotiation_domain_is_distinct_from_every_kdf_label() {
        for label in [
            crate::kdf::LABEL_HANDSHAKE,
            crate::kdf::LABEL_ROOT,
            crate::kdf::LABEL_MK,
            crate::kdf::LABEL_CK,
            crate::kdf::LABEL_BODY_NONCE,
            crate::kdf::LABEL_HEADER_INIT,
            crate::kdf::LABEL_SESSION_ID,
            crate::kdf::LABEL_STATE_KEY,
            crate::kdf::AD_HEADER,
            crate::kdf::AD_BODY,
        ] {
            assert_ne!(label, NEGOTIATION_DOMAIN);
        }
    }
}
