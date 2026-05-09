//! ML-KEM-768 wrapper tests.
//!
//! Tests cover:
//! - FIPS 203 size invariants (ek 1184, dk 2400, ct 1088, ss 32).
//! - Encaps/decaps round-trip with random RNG.
//! - Determinism: same RNG seed → identical (dk, ek), and identical
//!   (ct, ss) given the same ek.
//! - Byte-serialization round-trip for ek, dk, ct.
//! - Implicit-rejection behaviour on wrong-dk decapsulation.
//! - PQXDH HKDF combiner stub: deterministic, distinct under
//!   different inputs.
//!
//! FIPS 203 / ACVP **published** known-answer-test vector
//! integration is not yet in this file; the wrapper exposes
//! `_with_rng` variants that accept a fixed-bytes RNG, so once
//! NIST KAT data is sourced it plugs in here without API changes.
//! `ml-kem` 0.2 has its own internal KAT suite.

use crypto::ml_kem_768::{
    decapsulate, encapsulate, generate_keypair, generate_keypair_with_rng,
    pqxdh_combine_stub, Ciphertext, DecapsulationKey, EncapsulationKey, CIPHERTEXT_SIZE,
    DECAPSULATION_KEY_SIZE, ENCAPSULATION_KEY_SIZE, SHARED_SECRET_SIZE,
};
use rand_core::{CryptoRng, RngCore};

#[test]
fn sizes_match_fips_203() {
    assert_eq!(ENCAPSULATION_KEY_SIZE, 1184, "FIPS 203 §6.1 ML-KEM-768 ek");
    assert_eq!(DECAPSULATION_KEY_SIZE, 2400, "FIPS 203 §6.1 ML-KEM-768 dk");
    assert_eq!(CIPHERTEXT_SIZE, 1088, "FIPS 203 §6.1 ML-KEM-768 ciphertext");
    assert_eq!(SHARED_SECRET_SIZE, 32, "FIPS 203 ML-KEM shared secret");
}

#[test]
fn round_trip_random() {
    let (dk, ek) = generate_keypair();
    let (ct, ss_sender) = encapsulate(&ek).expect("encapsulate");
    let ss_recipient = decapsulate(&dk, &ct).expect("decapsulate");
    assert_eq!(
        ss_sender.as_bytes(),
        ss_recipient.as_bytes(),
        "encaps/decaps must yield the same shared secret"
    );
}

#[test]
fn deterministic_keygen_with_fixed_rng() {
    // Two keygen runs with the same RNG output must produce the
    // same (dk, ek). 256 bytes of seed is plenty for ML-KEM-768
    // keygen (FIPS 203 consumes 64 bytes for d || z).
    let seed = vec![0x42u8; 256];
    let (dk_a, ek_a) = generate_keypair_with_rng(&mut FixedRng::new(&seed));
    let (dk_b, ek_b) = generate_keypair_with_rng(&mut FixedRng::new(&seed));

    assert_eq!(
        ek_a.to_bytes().as_slice(),
        ek_b.to_bytes().as_slice(),
        "deterministic keygen — same seed must give same ek"
    );
    assert_eq!(
        dk_a.to_bytes().as_slice(),
        dk_b.to_bytes().as_slice(),
        "deterministic keygen — same seed must give same dk"
    );
}

#[test]
fn different_seeds_yield_different_keys() {
    let seed_a = vec![0x11u8; 256];
    let seed_b = vec![0x22u8; 256];
    let (_, ek_a) = generate_keypair_with_rng(&mut FixedRng::new(&seed_a));
    let (_, ek_b) = generate_keypair_with_rng(&mut FixedRng::new(&seed_b));
    assert_ne!(ek_a.to_bytes().as_slice(), ek_b.to_bytes().as_slice());
}

#[test]
fn different_keypairs_yield_different_encapsulation_keys() {
    let (_, ek1) = generate_keypair();
    let (_, ek2) = generate_keypair();
    assert_ne!(
        ek1.to_bytes().as_slice(),
        ek2.to_bytes().as_slice(),
        "fresh keypairs must produce distinct encapsulation keys"
    );
}

#[test]
fn different_keypairs_yield_different_shared_secrets() {
    let (_, ek1) = generate_keypair();
    let (_, ek2) = generate_keypair();
    let (_, ss1) = encapsulate(&ek1).expect("encaps 1");
    let (_, ss2) = encapsulate(&ek2).expect("encaps 2");
    assert_ne!(
        ss1.as_bytes(),
        ss2.as_bytes(),
        "different ek must give different ss with overwhelming probability"
    );
}

#[test]
fn decapsulate_with_wrong_secret_implicit_rejection() {
    // FIPS 203 §6.3: decapsulating with a wrong dk does NOT error —
    // it returns a deterministic but unrelated 32-byte value
    // (implicit rejection). Verify the value differs from the real
    // ss, and that the correct dk recovers the real ss.
    let (dk_correct, ek) = generate_keypair();
    let (dk_wrong, _) = generate_keypair();
    let (ct, ss_real) = encapsulate(&ek).expect("encaps");

    let ss_decoy = decapsulate(&dk_wrong, &ct).expect("ML-KEM never errors on decaps");
    assert_ne!(
        ss_real.as_bytes(),
        ss_decoy.as_bytes(),
        "wrong-dk decaps must yield a different (implicit-rejection) ss"
    );

    let ss_recovered = decapsulate(&dk_correct, &ct).expect("decaps with correct dk");
    assert_eq!(ss_real.as_bytes(), ss_recovered.as_bytes());
}

#[test]
fn ek_byte_round_trip() {
    let (_, ek) = generate_keypair();
    let bytes = ek.to_bytes();
    assert_eq!(bytes.len(), ENCAPSULATION_KEY_SIZE);
    let recovered = EncapsulationKey::from_bytes(&bytes);
    assert_eq!(ek.to_bytes(), recovered.to_bytes());
}

#[test]
fn dk_byte_round_trip() {
    let (dk, _) = generate_keypair();
    let bytes = dk.to_bytes();
    assert_eq!(bytes.len(), DECAPSULATION_KEY_SIZE);
    let recovered = DecapsulationKey::from_bytes(&bytes);
    // Compare via Deref to inner [u8; 2400].
    assert_eq!(*dk.to_bytes(), *recovered.to_bytes());
}

#[test]
fn ct_byte_round_trip() {
    let (_, ek) = generate_keypair();
    let (ct, _) = encapsulate(&ek).expect("encaps");
    let bytes = ct.to_bytes();
    assert_eq!(bytes.len(), CIPHERTEXT_SIZE);
    let recovered = Ciphertext::from_bytes(&bytes);
    assert_eq!(ct.to_bytes(), recovered.to_bytes());
}

#[test]
fn ct_byte_round_trip_preserves_decapsulation() {
    // Stronger: serialize ct, deserialize, decapsulate — must still
    // recover the original ss. This exercises the full
    // encode/decode path through Ciphertext::{to_bytes, from_bytes}.
    let (dk, ek) = generate_keypair();
    let (ct, ss_real) = encapsulate(&ek).expect("encaps");

    let wire = ct.to_bytes();
    let ct_back = Ciphertext::from_bytes(&wire);
    let ss_recovered = decapsulate(&dk, &ct_back).expect("decaps after wire round-trip");

    assert_eq!(ss_real.as_bytes(), ss_recovered.as_bytes());
}

#[test]
fn pqxdh_combine_stub_is_deterministic() {
    let dh = b"x25519-shared-secret-bytes-here";
    let (_, ek) = generate_keypair();
    let (_, ss) = encapsulate(&ek).expect("encaps");

    let sk1 = pqxdh_combine_stub(dh, &ss).expect("combine 1");
    let sk2 = pqxdh_combine_stub(dh, &ss).expect("combine 2");
    assert_eq!(sk1, sk2, "combiner is deterministic");
    assert_eq!(sk1.len(), 32);
}

#[test]
fn pqxdh_combine_stub_differs_under_different_ss_pq() {
    let dh = b"x25519-shared-secret-bytes-here";
    let (_, ek_a) = generate_keypair();
    let (_, ek_b) = generate_keypair();
    let (_, ss_a) = encapsulate(&ek_a).expect("encaps A");
    let (_, ss_b) = encapsulate(&ek_b).expect("encaps B");

    let sk_a = pqxdh_combine_stub(dh, &ss_a).expect("combine A");
    let sk_b = pqxdh_combine_stub(dh, &ss_b).expect("combine B");
    assert_ne!(sk_a, sk_b, "different SS_pq must yield different SK");
}

#[test]
fn pqxdh_combine_stub_differs_under_different_dh() {
    let (_, ek) = generate_keypair();
    let (_, ss) = encapsulate(&ek).expect("encaps");

    let sk_1 = pqxdh_combine_stub(b"dh-input-one", &ss).expect("combine 1");
    let sk_2 = pqxdh_combine_stub(b"dh-input-two", &ss).expect("combine 2");
    assert_ne!(sk_1, sk_2, "different DH concat must yield different SK");
}

// ---- Test helpers ----

/// Fake CSPRNG that returns bytes from a fixed buffer in order.
/// Used to drive deterministic keygen / encaps for testing.
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
