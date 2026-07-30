//! Known-answer / determinism vectors.
//!
//! # What these are, honestly
//!
//! These are **self-generated regression vectors**, not third-party
//! known-answer tests. No independent implementation of this protocol
//! exists, so nothing here proves interoperability with anyone. What
//! they do prove is that the protocol is fully deterministic given its
//! randomness, and that no refactor silently changes the bytes on the
//! wire or the key schedule.
//!
//! Genuine external KATs — RFC 5869 for HKDF and RFC 7748 for X25519 —
//! live in `src/primitives.rs` and *do* pin the primitive wiring
//! against published vectors. ML-KEM-768 is covered by the upstream
//! `ml-kem` crate's own FIPS 203 test suite.
//!
//! # Regenerating
//!
//! Run with `--nocapture`; each vector prints alongside its assertion,
//! so a deliberate wire-format change is a matter of copying the new
//! values in — and bumping `WIRE_VERSION_RN`'s documentation if the
//! change is not backwards compatible.

use osl_ratchet_next::test_support::{fresh_bundle, seeded_rng, wire_len};
use osl_ratchet_next::{Session, SessionParams};
use sha2::{Digest, Sha256};

const KAT_SEED: u64 = 0x0512_3456_789A_BCDE;

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

fn hexs(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministically replay a fixed script and return
/// `(session_id, per-message wire blobs, final exported states)`.
fn replay() -> ([u8; 16], Vec<String>, Vec<u8>, Vec<u8>) {
    let params = SessionParams::default();
    let mut rng = seeded_rng(KAT_SEED);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let (alice_ik, _) = osl_ratchet_next::primitives::x25519_keypair(&mut rng);
    let mut alice =
        Session::initiate(&alice_ik, &bob_bundle, params, &mut rng).expect("initiate");

    let mut wires = Vec::new();
    let w = alice.encrypt(0, b"one", &mut rng).expect("encrypt");
    wires.push(w.clone());
    let (mut bob, opened) =
        Session::accept(&bob_prekeys, &w, params, &mut rng).expect("accept");
    assert_eq!(opened.plaintext, b"one");

    // Alternate directions so the vectors cover both a same-chain
    // message and a DH-ratchet step in each direction.
    let script: [&[u8]; 5] = [b"two", b"three", b"four", b"five", b"six"];
    for (i, body) in script.iter().enumerate() {
        let bob_sends = i % 2 == 0;
        let (w, got) = if bob_sends {
            let w = bob.encrypt(7, body, &mut rng).expect("bob encrypt");
            let got = alice.decrypt(&w, &mut rng).expect("alice decrypt");
            (w, got)
        } else {
            let w = alice.encrypt(7, body, &mut rng).expect("alice encrypt");
            let got = bob.decrypt(&w, &mut rng).expect("bob decrypt");
            (w, got)
        };
        assert_eq!(&got.plaintext, body);
        assert_eq!(got.msg_type, 7);
        wires.push(w);
    }

    (
        alice.session_id(),
        wires,
        alice.export_state().expect("export"),
        bob.export_state().expect("export"),
    )
}

#[test]
fn protocol_is_fully_deterministic_given_its_randomness() {
    let a = replay();
    let b = replay();
    assert_eq!(a.0, b.0, "session id is not deterministic");
    assert_eq!(a.1, b.1, "wire bytes are not deterministic");
    assert_eq!(a.2, b.2, "alice state export is not deterministic");
    assert_eq!(a.3, b.3, "bob state export is not deterministic");
}

#[test]
fn known_answer_vectors() {
    let (session_id, wires, alice_state, bob_state) = replay();

    println!("session_id      = {}", hexs(&session_id));
    for (i, w) in wires.iter().enumerate() {
        println!(
            "wire[{i}] len={:<5} sha256={}",
            wire_len(w),
            sha(w.as_bytes())
        );
    }
    println!("wire[1] full    = {}", wires.get(1).expect("wire 1"));
    println!("alice_state     = {}", sha(&alice_state));
    println!("bob_state       = {}", sha(&bob_state));

    assert_eq!(hexs(&session_id), KAT_SESSION_ID);
    assert_eq!(wires.len(), KAT_WIRE_SHA.len());
    for (i, expected) in KAT_WIRE_SHA.iter().enumerate() {
        let w = wires.get(i).expect("wire");
        assert_eq!(&sha(w.as_bytes()), expected, "wire[{i}] changed");
    }
    assert_eq!(wires.get(1).map(String::as_str), Some(KAT_WIRE_1));
    assert_eq!(sha(&alice_state), KAT_ALICE_STATE);
    assert_eq!(sha(&bob_state), KAT_BOB_STATE);
}

#[test]
fn message_sizes_match_the_documented_budget() {
    let (_, wires, _, _) = replay();
    let first = wire_len(wires.first().expect("wire 0"));
    let steady = wire_len(wires.get(1).expect("wire 1"));

    println!("bootstrap message = {first} bytes, steady-state = {steady} bytes");
    // The bootstrap message carries the 1088-byte ML-KEM ciphertext and
    // two 32-byte public keys, exactly as a Signal PreKeyMessage does.
    assert!(
        (1230..=1260).contains(&first),
        "bootstrap size drifted: {first}"
    );
    // Steady state: 2 framing + 12 nonce + 1 length + (37 header + 16
    // tag) + (plaintext + 16 tag).
    assert!((80..=95).contains(&steady), "steady size drifted: {steady}");
}

#[test]
fn a_frozen_wire_blob_still_decrypts() {
    // Rebuild the responder from the same seed and open the frozen
    // blob. This is the vector that would catch a key-schedule change
    // that happened to leave message *sizes* unchanged.
    let params = SessionParams::default();
    let mut rng = seeded_rng(KAT_SEED);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let (alice_ik, _) = osl_ratchet_next::primitives::x25519_keypair(&mut rng);
    let mut alice =
        Session::initiate(&alice_ik, &bob_bundle, params, &mut rng).expect("initiate");
    let w0 = alice.encrypt(0, b"one", &mut rng).expect("encrypt");
    assert_eq!(KAT_WIRE_SHA.first().copied(), Some(sha(w0.as_bytes()).as_str()));

    let (_bob, opened) = Session::accept(&bob_prekeys, &w0, params, &mut rng).expect("accept");
    assert_eq!(opened.plaintext, b"one");
    assert_eq!(opened.msg_type, 0);
}

// ---------------------------------------------------------------
// Frozen vectors. Regenerate with `--nocapture` (see module docs).
// ---------------------------------------------------------------

const KAT_SESSION_ID: &str = "a68c91059d80bf7542e12a973d470c84";
const KAT_WIRE_1: &str = "DPC0::EABOP9LM54g9q6Pm+Eg2+Bho7qO3rNMrxUETLLQjNeT5AFkMGLDrI3lk0a3BBNeqG1JbhwD8HAggD1eeXvj5Jt2IYJ9BajbM1ElZXqW/0pqjQo+KWMvzog==";
const KAT_ALICE_STATE: &str = "0f25f92169c2a79371ed24f1da8cb5eea8b309fbdd0d0442e982b3116ffa3916";
const KAT_BOB_STATE: &str = "593208856da5c977343b6c26898a4589b177b62171e4d301cbda605ba053e718";
// Regenerated when the wire version byte moved from 0x06 to 0x10
// (`WIRE_VERSION_RN`). Only these wire hashes changed: the version byte
// is both a wire byte and AEAD associated data, so every tag moved.
// `KAT_SESSION_ID`, `KAT_ALICE_STATE` and `KAT_BOB_STATE` are
// *unchanged*, which is the evidence that the renumber touched the
// framing only and left the key schedule alone.
const KAT_WIRE_SHA: [&str; 6] = [
    "8a6ac8f04dd193744aa6ec3ab460046370f4db707bd86291a6ca87164b3d88a8",
    "25c45eba0c523775eec748460659dc4c7cbf6354cc52a3242c66870e13e27e74",
    "10e6e859b4c75e4c4434acc413db8bb8024bdb22df9b7978e22382e412035530",
    "2c272f73f918af7fde5f2d60f141da763020a51230f1f9cec0d0f80a1eec1686",
    "dced1e2d0f817bd64ca5d189c666660e02818ab6ccad7ca8c4707997585f442d",
    "366368b9c8cb4a704cb5fa552f4a133b737d311f706d7a12889b34fe8fe066fc",
];
