//! Adversarial tests for the negotiation binding.
//!
//! The property under test is **fail closed, never fall back**: any
//! disagreement about what was negotiated must produce a session that
//! does not establish, rather than one that establishes on weaker terms.
//!
//! No plaintext, key material or session state is printed by any
//! assertion in this file. Failure messages name only the scenario.

use osl_ratchet_next::negotiate::Negotiation;
use osl_ratchet_next::primitives::{x25519_keypair, XPublic};
use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng};
use osl_ratchet_next::{peek_bootstrap_initiator_identity, Error, Session, SessionParams};

const CTX: &[u8] = b"osl-ratchet-next/tests/negotiation/v1";

/// Everything a test needs to drive a bound handshake by hand.
struct Pair {
    alice_ik_secret: osl_ratchet_next::XSecret,
    alice_ik_public: XPublic,
    bob_prekeys: osl_ratchet_next::LocalPrekeys,
    bob_bundle: osl_ratchet_next::PeerBundle,
    /// Bob's published ML-KEM encapsulation key, in the byte form both
    /// sides feed to the digest.
    bob_ek: [u8; osl_ratchet_next::MLKEM_EK],
    rng: rand_chacha::ChaCha20Rng,
}

fn pair(seed: u64) -> Pair {
    let mut rng = seeded_rng(seed);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let (alice_ik_secret, alice_ik_public) = x25519_keypair(&mut rng);
    let bob_ek = bob_bundle.pq_prekey.to_bytes();
    Pair {
        alice_ik_secret,
        alice_ik_public,
        bob_prekeys,
        bob_bundle,
        bob_ek,
        rng,
    }
}

impl Pair {
    /// The digest Alice computes, from the peer record she holds.
    fn alice_digest(&self) -> [u8; 32] {
        Negotiation::for_rn(
            self.bob_bundle.identity.as_bytes(),
            &self.bob_ek,
            self.alice_ik_public.as_bytes(),
            CTX,
        )
        .digest()
        .expect("alice digest")
    }

    /// The digest Bob computes independently, from his own keys plus the
    /// initiator identity he reads out of the preamble. Nothing is
    /// transmitted for this.
    fn bob_digest(&self, wire: &str) -> [u8; 32] {
        let initiator = peek_bootstrap_initiator_identity(wire)
            .expect("parse bootstrap")
            .expect("bootstrap present");
        Negotiation::for_rn(
            self.bob_prekeys.identity.public().as_bytes(),
            &self.bob_ek,
            initiator.as_bytes(),
            CTX,
        )
        .digest()
        .expect("bob digest")
    }
}

#[test]
fn a_bound_handshake_succeeds_when_both_sides_agree() {
    let mut p = pair(1);
    let binding = p.alice_digest();
    let mut alice = Session::initiate_bound(
        &p.alice_ik_secret,
        &p.bob_bundle,
        Some(&binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate");

    let wire = alice.encrypt(0, b"agreed", &mut p.rng).expect("encrypt");
    let bob_binding = p.bob_digest(&wire);
    assert_eq!(
        binding, bob_binding,
        "both sides must derive the same digest with no extra wire bytes"
    );

    let (mut bob, opened) = Session::accept_bound(
        &p.bob_prekeys,
        &wire,
        Some(&bob_binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("accept");
    assert_eq!(opened.plaintext, b"agreed");

    // And the session keeps working in both directions.
    let back = bob.encrypt(0, b"ack", &mut p.rng).expect("encrypt");
    assert_eq!(
        alice.decrypt(&back, &mut p.rng).expect("decrypt").plaintext,
        b"ack"
    );
}

/// The core downgrade case reachable *inside* the protocol: the two
/// sides disagree about the negotiation. Alice binds the OSL-RN floor;
/// a peer that bound anything else must fail to establish, not
/// establish on the weaker terms.
#[test]
fn a_floor_disagreement_fails_closed() {
    let mut p = pair(2);
    let alice_binding = p.alice_digest();
    let mut alice = Session::initiate_bound(
        &p.alice_ik_secret,
        &p.bob_bundle,
        Some(&alice_binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate");
    let wire = alice.encrypt(0, b"x", &mut p.rng).expect("encrypt");

    // A floor below the selected version cannot even be hashed: the
    // digest refuses it rather than producing a usable binding. That is
    // the first line of defence — a downgraded floor is not a different
    // session, it is no session.
    let bob_identity = p.bob_prekeys.identity.public();
    let mut floor_below = Negotiation::for_rn(
        bob_identity.as_bytes(),
        &p.bob_ek,
        p.alice_ik_public.as_bytes(),
        CTX,
    );
    floor_below.min_acceptable_version = 0x03;
    assert!(
        floor_below.digest().is_err(),
        "a v=3 floor must be unrepresentable, not merely different"
    );

    // And any *other* disagreement about the negotiation yields a
    // different digest, hence a different SK.
    let perturbed = Negotiation::for_rn(
        p.bob_prekeys.identity.public().as_bytes(),
        &p.bob_ek,
        p.alice_ik_public.as_bytes(),
        b"osl-ratchet-next/tests/negotiation/DOWNGRADED",
    )
    .digest()
    .expect("digest");
    assert_ne!(alice_binding, perturbed);

    let outcome = Session::accept_bound(
        &p.bob_prekeys,
        &wire,
        Some(&perturbed),
        SessionParams::default(),
        &mut p.rng,
    );
    assert!(
        matches!(outcome, Err(Error::AuthFailed)),
        "a negotiation mismatch must fail closed"
    );
}

/// An attacker who substitutes their own ML-KEM encapsulation key into
/// the peer record — the "strip post-quantum protection" move — must not
/// be able to establish a session, and must not be able to make Bob
/// establish one either.
#[test]
fn substituting_the_responder_kem_key_fails_closed() {
    let mut p = pair(3);

    // Alice is served an attacker's ML-KEM key in place of Bob's.
    let (_attacker_dk, attacker_ek) = osl_ratchet_next::primitives::kem_keypair(&mut p.rng);
    let mut tampered_bundle = p.bob_bundle.clone();
    tampered_bundle.pq_prekey = attacker_ek;

    let attacker_ek_bytes = tampered_bundle.pq_prekey.to_bytes();
    let alice_binding = Negotiation::for_rn(
        tampered_bundle.identity.as_bytes(),
        &attacker_ek_bytes,
        p.alice_ik_public.as_bytes(),
        CTX,
    )
    .digest()
    .expect("digest");

    let mut alice = Session::initiate_bound(
        &p.alice_ik_secret,
        &tampered_bundle,
        Some(&alice_binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate");
    let wire = alice.encrypt(0, b"x", &mut p.rng).expect("encrypt");

    // Bob binds his *real* key, as he always does.
    let bob_binding = p.bob_digest(&wire);
    assert_ne!(
        alice_binding, bob_binding,
        "the binding must notice the substituted KEM key"
    );
    assert!(matches!(
        Session::accept_bound(
            &p.bob_prekeys,
            &wire,
            Some(&bob_binding),
            SessionParams::default(),
            &mut p.rng,
        ),
        Err(Error::AuthFailed)
    ));
}

/// A bound session and an unbound one over the *same* static keys must
/// not be interchangeable. This is the cross-version key-separation
/// property: v=3 and v=6 share the identity and ML-KEM keys, and a v=6
/// session must be cryptographically distinct from anything derivable
/// without the version binding.
#[test]
fn bound_and_unbound_sessions_are_not_interchangeable() {
    let mut p = pair(4);
    let binding = p.alice_digest();

    let mut bound = Session::initiate_bound(
        &p.alice_ik_secret,
        &p.bob_bundle,
        Some(&binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate bound");
    let bound_wire = bound.encrypt(0, b"x", &mut p.rng).expect("encrypt");

    // An unbound responder cannot open a bound initiator's message.
    assert!(matches!(
        Session::accept(
            &p.bob_prekeys,
            &bound_wire,
            SessionParams::default(),
            &mut p.rng,
        ),
        Err(Error::AuthFailed)
    ));

    // And symmetrically.
    let mut unbound = Session::initiate(
        &p.alice_ik_secret,
        &p.bob_bundle,
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate unbound");
    let unbound_wire = unbound.encrypt(0, b"x", &mut p.rng).expect("encrypt");
    assert!(matches!(
        Session::accept_bound(
            &p.bob_prekeys,
            &unbound_wire,
            Some(&binding),
            SessionParams::default(),
            &mut p.rng,
        ),
        Err(Error::AuthFailed)
    ));
}

/// A rewritten `initiator_identity` in the preamble must not survive.
/// It is covered by the AEAD's associated data already; the binding
/// makes it a key-derivation failure as well, so removing the AD
/// coverage in some future refactor could not silently open this hole.
#[test]
fn a_rewritten_initiator_identity_fails_closed() {
    let mut p = pair(5);
    let binding = p.alice_digest();
    let mut alice = Session::initiate_bound(
        &p.alice_ik_secret,
        &p.bob_bundle,
        Some(&binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate");
    let wire = alice.encrypt(0, b"x", &mut p.rng).expect("encrypt");

    // Bob binds an identity that is not the one in the preamble.
    let (_, other_pub) = x25519_keypair(&mut p.rng);
    let wrong = Negotiation::for_rn(
        p.bob_prekeys.identity.public().as_bytes(),
        &p.bob_ek,
        other_pub.as_bytes(),
        CTX,
    )
    .digest()
    .expect("digest");

    assert!(matches!(
        Session::accept_bound(
            &p.bob_prekeys,
            &wire,
            Some(&wrong),
            SessionParams::default(),
            &mut p.rng,
        ),
        Err(Error::AuthFailed)
    ));
}

/// The peek helper must never panic and must not claim a bootstrap
/// where there is none.
#[test]
fn peeking_the_initiator_identity_is_total() {
    let mut p = pair(6);
    let binding = p.alice_digest();
    let mut alice = Session::initiate_bound(
        &p.alice_ik_secret,
        &p.bob_bundle,
        Some(&binding),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("initiate");
    let bootstrap = alice.encrypt(0, b"x", &mut p.rng).expect("encrypt");
    assert!(peek_bootstrap_initiator_identity(&bootstrap)
        .expect("parse")
        .is_some());

    // Non-v6 and malformed inputs must be errors, never panics.
    assert!(peek_bootstrap_initiator_identity("not a wire").is_err());
    assert!(peek_bootstrap_initiator_identity("DPC0::").is_err());
    assert!(peek_bootstrap_initiator_identity("DPC0::AwAA").is_err());

    // Every truncation of a real blob must be handled.
    for cut in 0..bootstrap.len() {
        let _ = peek_bootstrap_initiator_identity(&bootstrap[..cut]);
    }

    // After the responder replies, Alice's later messages carry no
    // preamble, and the peek must say so rather than inventing one.
    let (mut bob, _) = Session::accept_bound(
        &p.bob_prekeys,
        &bootstrap,
        Some(&p.bob_digest(&bootstrap)),
        SessionParams::default(),
        &mut p.rng,
    )
    .expect("accept");
    let back = bob.encrypt(0, b"ack", &mut p.rng).expect("encrypt");
    alice.decrypt(&back, &mut p.rng).expect("decrypt");
    let later = alice.encrypt(0, b"y", &mut p.rng).expect("encrypt");
    assert_eq!(
        peek_bootstrap_initiator_identity(&later).expect("parse"),
        None
    );
}
