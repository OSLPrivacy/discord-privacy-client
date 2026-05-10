//! PQXDH handshake tests.
//!
//! Coverage:
//! - Round-trip Alice/Bob with one-time prekey present.
//! - Round-trip Alice/Bob without one-time prekey (no-OPK fallback).
//! - With identical RNG seed and identity material, the OPK and
//!   no-OPK paths produce **distinct** session keys (the OPK
//!   transcript bit actually affects the HKDF input).
//! - Determinism of `initiate_with_rng` under a fixed-bytes RNG.
//! - Distinct recipient identity material yields distinct session
//!   keys.
//! - Responder rejects handshakes whose `no_opk` flag doesn't agree
//!   with the `opk_id` it carries and the OPK secret the caller
//!   supplies (both directions of mismatch).
//! - Handshake metadata round-trips correctly (`opk_id` populated
//!   iff `no_opk == false`).
//!
//! Cross-implementation KAT vectors against libsignal's reference
//! implementation in test mode are deferred to a separate commit
//! (per the design doc's review-gate test-vector item).

use crypto::ml_kem_768;
use crypto::pqxdh::{derive_sk, initiate, initiate_with_rng, respond};
use crypto::x25519;
use rand_core::{CryptoRng, RngCore};

#[test]
fn round_trip_with_opk() {
    let bob = bob_keys();
    let (alice_ik_secret, alice_ik_pub) = x25519::generate_keypair();
    let (alice_sk, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((42, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .expect("alice initiate");

    assert!(!handshake.no_opk);
    assert_eq!(handshake.opk_id, Some(42));

    let bob_sk = respond(
        &bob.ik_secret,
        &bob.spk_secret,
        Some(&bob.opk_secret),
        &bob.mlkem_dk,
        &alice_ik_pub,
        &handshake,
    )
    .expect("bob respond");

    assert_eq!(
        alice_sk.as_bytes(),
        bob_sk.as_bytes(),
        "Alice and Bob must derive the same SK (with OPK)"
    );
}

#[test]
fn round_trip_without_opk() {
    let bob = bob_keys();
    let (alice_ik_secret, alice_ik_pub) = x25519::generate_keypair();
    let (alice_sk, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        None,
        &bob.mlkem_ek,
    )
    .expect("alice initiate no-opk");

    assert!(handshake.no_opk);
    assert!(handshake.opk_id.is_none());

    let bob_sk = respond(
        &bob.ik_secret,
        &bob.spk_secret,
        None,
        &bob.mlkem_dk,
        &alice_ik_pub,
        &handshake,
    )
    .expect("bob respond no-opk");

    assert_eq!(
        alice_sk.as_bytes(),
        bob_sk.as_bytes(),
        "Alice and Bob must derive the same SK (no-OPK fallback)"
    );
}

#[test]
fn opk_vs_no_opk_yield_distinct_sk_with_fixed_rng() {
    // Same Alice IK, same Bob keys, same RNG seed for the ephemeral
    // X25519 keygen and ML-KEM encapsulation. The only difference
    // between the two runs is whether DH4 (= X25519(EK_A, OPK_B))
    // gets folded into the HKDF transcript. The session keys must
    // differ.
    let bob = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();
    let seed = vec![0xA5u8; 1024];

    let (sk_with_opk, _) = initiate_with_rng(
        &mut FixedRng::new(&seed),
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((1, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .expect("initiate with-opk");

    let (sk_no_opk, _) = initiate_with_rng(
        &mut FixedRng::new(&seed),
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        None,
        &bob.mlkem_ek,
    )
    .expect("initiate no-opk");

    assert_ne!(
        sk_with_opk.as_bytes(),
        sk_no_opk.as_bytes(),
        "OPK presence must affect SK even when EK and ML-KEM are fixed"
    );
}

#[test]
fn initiate_with_rng_is_deterministic() {
    let bob = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();
    let seed = vec![0x33u8; 1024];

    let (sk1, h1) = initiate_with_rng(
        &mut FixedRng::new(&seed),
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((7, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .expect("initiate run 1");

    let (sk2, h2) = initiate_with_rng(
        &mut FixedRng::new(&seed),
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((7, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .expect("initiate run 2");

    assert_eq!(sk1.as_bytes(), sk2.as_bytes(), "deterministic SK");
    assert_eq!(
        h1.ek_x25519_pub.as_bytes(),
        h2.ek_x25519_pub.as_bytes(),
        "deterministic ephemeral X25519 public"
    );
    assert_eq!(
        h1.mlkem_ciphertext.to_bytes(),
        h2.mlkem_ciphertext.to_bytes(),
        "deterministic ML-KEM ciphertext"
    );
    assert_eq!(h1.no_opk, h2.no_opk);
    assert_eq!(h1.opk_id, h2.opk_id);
}

#[test]
fn distinct_recipients_yield_distinct_sk() {
    let bob_a = bob_keys();
    let bob_b = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();

    let (sk_a, _) = initiate(
        &alice_ik_secret,
        &bob_a.ik_pub,
        &bob_a.spk_pub,
        Some((1, &bob_a.opk_pub)),
        &bob_a.mlkem_ek,
    )
    .unwrap();

    let (sk_b, _) = initiate(
        &alice_ik_secret,
        &bob_b.ik_pub,
        &bob_b.spk_pub,
        Some((1, &bob_b.opk_pub)),
        &bob_b.mlkem_ek,
    )
    .unwrap();

    assert_ne!(
        sk_a.as_bytes(),
        sk_b.as_bytes(),
        "different Bob keys must yield different SK"
    );
}

#[test]
fn distinct_initiators_yield_distinct_sk() {
    let bob = bob_keys();
    let (ik_secret_1, _) = x25519::generate_keypair();
    let (ik_secret_2, _) = x25519::generate_keypair();

    let (sk1, _) = initiate(&ik_secret_1, &bob.ik_pub, &bob.spk_pub, None, &bob.mlkem_ek).unwrap();
    let (sk2, _) = initiate(&ik_secret_2, &bob.ik_pub, &bob.spk_pub, None, &bob.mlkem_ek).unwrap();

    assert_ne!(
        sk1.as_bytes(),
        sk2.as_bytes(),
        "different Alice IK must yield different SK"
    );
}

#[test]
fn respond_rejects_no_opk_true_with_supplied_opk_secret() {
    let bob = bob_keys();
    let (alice_ik_secret, alice_ik_pub) = x25519::generate_keypair();
    let (_, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        None,
        &bob.mlkem_ek,
    )
    .unwrap();
    assert!(handshake.no_opk);

    // Caller mistakenly supplies an OPK secret despite no_opk=true.
    let result = respond(
        &bob.ik_secret,
        &bob.spk_secret,
        Some(&bob.opk_secret),
        &bob.mlkem_dk,
        &alice_ik_pub,
        &handshake,
    );
    assert!(result.is_err(), "respond must reject this inconsistency");
}

#[test]
fn respond_rejects_no_opk_false_without_opk_secret() {
    let bob = bob_keys();
    let (alice_ik_secret, alice_ik_pub) = x25519::generate_keypair();
    let (_, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((9, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .unwrap();
    assert!(!handshake.no_opk);

    // Caller forgets to supply the OPK secret.
    let result = respond(
        &bob.ik_secret,
        &bob.spk_secret,
        None,
        &bob.mlkem_dk,
        &alice_ik_pub,
        &handshake,
    );
    assert!(result.is_err(), "respond must reject this inconsistency");
}

#[test]
fn handshake_metadata_for_opk() {
    let bob = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();
    let (_, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((123_456, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .unwrap();
    assert_eq!(handshake.opk_id, Some(123_456));
    assert!(!handshake.no_opk);
}

#[test]
fn handshake_metadata_for_no_opk() {
    let bob = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();
    let (_, handshake) = initiate(
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        None,
        &bob.mlkem_ek,
    )
    .unwrap();
    assert!(handshake.no_opk);
    assert!(handshake.opk_id.is_none());
}

#[test]
fn derive_sk_matches_initiate_when_inputs_align() {
    // Lower-level sanity: feed derive_sk the DH outputs that
    // Alice would compute, plus the ML-KEM SS, and verify the SK
    // matches the high-level initiate() output. This guards against
    // the combiner being called with a different concat order on
    // either side.
    let bob = bob_keys();
    let (alice_ik_secret, _) = x25519::generate_keypair();
    let seed = vec![0x77u8; 1024];

    let (sk_initiate, handshake) = initiate_with_rng(
        &mut FixedRng::new(&seed),
        &alice_ik_secret,
        &bob.ik_pub,
        &bob.spk_pub,
        Some((1, &bob.opk_pub)),
        &bob.mlkem_ek,
    )
    .unwrap();

    // Recreate the same EK by replaying the same seed prefix
    // (32 bytes for x25519 ephemeral keygen).
    let (ek_secret, _) = x25519::generate_keypair_with_rng(&mut FixedRng::new(&seed[..32]));

    let dh1 = x25519::diffie_hellman(&alice_ik_secret, &bob.spk_pub).unwrap();
    let dh2 = x25519::diffie_hellman(&ek_secret, &bob.ik_pub).unwrap();
    let dh3 = x25519::diffie_hellman(&ek_secret, &bob.spk_pub).unwrap();
    let dh4 = x25519::diffie_hellman(&ek_secret, &bob.opk_pub).unwrap();

    // Decapsulate the handshake's ML-KEM ciphertext to get SS_pq
    // (Bob would do this in respond()).
    let ss_pq = ml_kem_768::decapsulate(&bob.mlkem_dk, &handshake.mlkem_ciphertext).unwrap();

    let sk_derived = derive_sk(&dh1, &dh2, &dh3, Some(&dh4), &ss_pq).unwrap();

    assert_eq!(
        sk_initiate.as_bytes(),
        sk_derived.as_bytes(),
        "derive_sk on the same inputs must match initiate's SK"
    );
}

// ---- Test fixtures ----

struct BobKeys {
    ik_secret: x25519::SecretKey,
    ik_pub: x25519::PublicKey,
    spk_secret: x25519::SecretKey,
    spk_pub: x25519::PublicKey,
    opk_secret: x25519::SecretKey,
    opk_pub: x25519::PublicKey,
    mlkem_dk: ml_kem_768::DecapsulationKey,
    mlkem_ek: ml_kem_768::EncapsulationKey,
}

fn bob_keys() -> BobKeys {
    let (ik_secret, ik_pub) = x25519::generate_keypair();
    let (spk_secret, spk_pub) = x25519::generate_keypair();
    let (opk_secret, opk_pub) = x25519::generate_keypair();
    let (mlkem_dk, mlkem_ek) = ml_kem_768::generate_keypair();
    BobKeys {
        ik_secret,
        ik_pub,
        spk_secret,
        spk_pub,
        opk_secret,
        opk_pub,
        mlkem_dk,
        mlkem_ek,
    }
}

/// Fake CSPRNG that returns bytes from a fixed buffer in order.
/// Used to drive deterministic ephemeral X25519 keygen + ML-KEM
/// encapsulation in the PQXDH initiator.
struct FixedRng {
    data: Vec<u8>,
    pos: usize,
}

impl FixedRng {
    fn new(data: &[u8]) -> Self {
        FixedRng {
            data: data.to_vec(),
            pos: 0,
        }
    }
}

impl RngCore for FixedRng {
    fn next_u32(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        self.fill_bytes(&mut buf);
        u32::from_le_bytes(buf)
    }

    fn next_u64(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        self.fill_bytes(&mut buf);
        u64::from_le_bytes(buf)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        let len = dst.len();
        assert!(
            self.pos + len <= self.data.len(),
            "FixedRng exhausted: needed {} bytes at pos {}, have {} total",
            len,
            self.pos,
            self.data.len()
        );
        dst.copy_from_slice(&self.data[self.pos..self.pos + len]);
        self.pos += len;
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dst);
        Ok(())
    }
}

impl CryptoRng for FixedRng {}
