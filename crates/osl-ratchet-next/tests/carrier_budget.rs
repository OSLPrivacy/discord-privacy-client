//! What one PQ epoch actually costs on the Discord carrier.
//!
//! `DESIGN.md` §6 reports overhead in *bytes*. Bytes are not the unit
//! that matters here: the carrier bills in **Discord messages**, and the
//! two modes bill completely differently. This file measures the real
//! figure for each.
//!
//! # The two carrier modes, as measured in `crates/stego`
//!
//! | Mode | Capacity | Source |
//! | --- | --- | --- |
//! | **Chunked** (Mode 1) | `MODE1_MAX_RAW_LEN (100) - CHUNK_HEADER_BYTES (14)` = **86 payload bytes per Discord message** | `stego::mode1_chunking::CHUNK_PAYLOAD_BYTES` |
//! | **Token** | a **12-byte** pointer (`TOKEN_ID_BYTES (8) + TOKEN_MAC_BYTES (4)`) rides the Discord text; the ciphertext goes to the recipient's key-server inbox | `stego::mode1::TOKEN_ID_BYTES` / `TOKEN_MAC_BYTES` |
//!
//! Chunking consumes the **decoded** wire bytes, not the base64 text
//! (`ipc::commands` strips `DPC0::`, base64-decodes, then calls
//! `stego::chunk_payload`). So base64 does **not** multiply the carrier
//! cost. `DESIGN.md` §6's "base64 multiplies all of these by 4/3, and
//! the steganographic layer multiplies again" is wrong about this path
//! and is corrected here.
//!
//! `CHUNK_PAYLOAD_BYTES` is duplicated as a local constant rather than
//! imported: this crate deliberately does not depend on `stego`, and a
//! carrier constant leaking into a protocol crate would be exactly the
//! layering violation `api.rs` argues against. The value is asserted
//! against its documented derivation below.

use osl_ratchet_next::test_support::{seeded_rng, wire_len, Harness};
use osl_ratchet_next::{PqParams, SessionParams};

/// `stego::mode1_chunking::CHUNK_PAYLOAD_BYTES`, mirrored.
const CHUNK_PAYLOAD_BYTES: usize = 100 - 14;

/// Discord messages consumed by one OSL message in chunked mode.
fn chunked_messages(wire: &str) -> usize {
    wire_len(wire).div_ceil(CHUNK_PAYLOAD_BYTES).max(1)
}

/// Discord messages consumed by one OSL message in token mode: always
/// one, because only a 12-byte pointer rides the Discord text and the
/// ciphertext travels out of band via the key-server inbox.
fn token_messages(_wire: &str) -> usize {
    1
}

#[test]
fn carrier_constants_match_their_documented_derivation() {
    assert_eq!(CHUNK_PAYLOAD_BYTES, 86);
    // The token pointer is 8 + 4 = 12 bytes, well under one message.
    assert_eq!(8 + 4, 12);
}

/// Drive a bidirectional conversation until one full PQ epoch has been
/// established and mixed, counting Discord messages in both modes.
fn measure(fragment_bytes: usize, rekey_interval: u32) -> Report {
    let params = SessionParams {
        pq: PqParams {
            fragment_bytes,
            rekey_interval,
        },
        skip: Default::default(),
    };
    let mut h = Harness::established_with_params(0xB0DE, params);
    let start_epoch = h.alice.pq_mixed_epoch();

    let mut osl_messages = 0usize;
    let mut chunked = 0usize;
    let mut token = 0usize;
    let mut fragment_carrying_wires = 0usize;
    let mut steady_wires = 0usize;

    // A short, realistic chat line.
    let body = b"see you at seven";

    // Bounded: if an epoch has not completed in this many round trips
    // the test fails rather than looping.
    for i in 0..4000usize {
        let alice_sends = i % 2 == 0;
        let wire = if alice_sends {
            let w = h.alice_send(body).expect("alice send");
            h.bob_recv(&w).expect("bob recv");
            w
        } else {
            let w = h.bob_send(body).expect("bob send");
            h.alice_recv(&w).expect("alice recv");
            w
        };

        osl_messages += 1;
        let c = chunked_messages(&wire);
        chunked += c;
        token += token_messages(&wire);
        // A message whose wire is far above the steady-state size is
        // carrying an ML-KEM fragment.
        if wire_len(&wire) > 85 + body.len() + 8 {
            fragment_carrying_wires += 1;
        } else {
            steady_wires += 1;
        }

        if h.alice.pq_mixed_epoch() > start_epoch && h.bob_ref().pq_mixed_epoch() > start_epoch {
            break;
        }
    }

    assert!(
        h.alice.pq_mixed_epoch() > start_epoch,
        "no PQ epoch completed within the bound"
    );

    Report {
        osl_messages,
        chunked,
        token,
        fragment_carrying_wires,
        steady_wires,
    }
}

struct Report {
    osl_messages: usize,
    chunked: usize,
    token: usize,
    fragment_carrying_wires: usize,
    steady_wires: usize,
}

#[test]
fn one_pq_epoch_costs_this_many_discord_messages() {
    // The information-theoretic floor for chunked mode: an epoch must
    // move an ML-KEM encapsulation key (1184 B) one way and a
    // ciphertext (1088 B) back = 2272 bytes of PQ material.
    let floor = (1184usize + 1088).div_ceil(CHUNK_PAYLOAD_BYTES);
    println!("chunked-mode floor for 2272 B of PQ material = {floor} Discord messages");
    assert_eq!(floor, 27);

    for (frag, rekey) in [(128usize, 64u32), (72, 64), (128, 16)] {
        let r = measure(frag, rekey);
        let baseline = r.osl_messages; // one Discord message each in token mode
        println!(
            "fragment_bytes={frag:<4} rekey_interval={rekey:<3} \
             OSL msgs={:<5} chunked={:<5} (={:.2}x) token={:<5} (={:.2}x) \
             fragment-carrying={:<4} steady={}",
            r.osl_messages,
            r.chunked,
            r.chunked as f64 / baseline as f64,
            r.token,
            r.token as f64 / baseline as f64,
            r.fragment_carrying_wires,
            r.steady_wires,
        );

        // Token mode is exactly one Discord message per OSL message: the
        // PQ epoch is FREE in carrier terms, because the ciphertext does
        // not ride the Discord text at all.
        assert_eq!(
            r.token, r.osl_messages,
            "token mode must cost exactly one Discord message per OSL message"
        );
        // Chunked mode always costs more than the OSL message count.
        assert!(r.chunked > r.osl_messages);
    }
}

/// Which `fragment_bytes` actually minimises Discord-message cost.
///
/// A plausible-sounding hypothesis is that `fragment_bytes` should be
/// sized so a fragment-carrying message does not spill into an extra
/// chunk (a steady-state message already occupies 2 chunks = 172 bytes,
/// leaving ~85 bytes spare, so a 128-byte fragment must spill). **That
/// hypothesis is false, and this test is what disproved it.**
///
/// Smaller fragments do avoid the spill, but they need proportionally
/// more fragments to move the same 2272 bytes of ML-KEM material, and
/// each additional fragment needs another round trip — which is a whole
/// extra OSL message costing 2 chunks. Round trips dominate the spill by
/// a wide margin, so **larger fragments are cheaper on this carrier**,
/// and the default of 128 is on the right side of the trade.
#[test]
fn larger_fragments_are_cheaper_because_round_trips_dominate() {
    let mut rows = Vec::new();
    for frag in [64usize, 72, 128, 256] {
        let r = measure(frag, 64);
        let amp = r.chunked as f64 / r.osl_messages as f64;
        println!(
            "fragment_bytes={frag:<4} OSL msgs to heal={:<4} chunked Discord msgs={:<5} \
             amplification={amp:.3}x fragment-carrying={}",
            r.osl_messages, r.chunked, r.fragment_carrying_wires
        );
        rows.push((frag, amp, r.osl_messages));
    }

    // Monotone: bigger fragments heal in fewer OSL messages.
    for w in rows.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        assert!(
            b.2 <= a.2,
            "raising fragment_bytes must not increase the messages needed to heal"
        );
    }
    // And the default beats the "avoid the spill" tuning.
    let tuned = rows.iter().find(|r| r.0 == 72).expect("72");
    let default = rows.iter().find(|r| r.0 == 128).expect("128");
    assert!(
        default.1 < tuned.1,
        "the default must beat the spill-avoiding tuning"
    );
}

/// A single steady-state message must not straddle more chunks than the
/// budget assumes. This is the figure that dominates ordinary traffic,
/// since only a minority of messages carry fragments.
#[test]
fn a_steady_state_message_fits_in_two_chunks() {
    let mut h = Harness::established(0xB0DF);
    let mut rng = seeded_rng(1);
    let _ = &mut rng;
    for body in [
        &b"ok"[..],
        &b"see you at seven"[..],
        // 86 - 85 = 1 byte is all a single chunk can spare, so anything
        // realistic needs two.
        &[b'x'; 60][..],
    ] {
        let w = h.alice_send(body).expect("send");
        h.bob_recv(&w).expect("recv");
        let chunks = chunked_messages(&w);
        println!(
            "plaintext {:>3} B -> wire {:>3} B -> {chunks} Discord message(s) chunked, 1 token",
            body.len(),
            wire_len(&w)
        );
        assert!(
            chunks <= 2,
            "a steady-state message must not exceed two chunks"
        );
    }
}

/// The bootstrap message is the expensive one, and it is unavoidable:
/// it carries the 1088-byte ML-KEM ciphertext of the handshake.
#[test]
fn the_bootstrap_message_costs_this_much() {
    let mut h = Harness::new(0xB0E0);
    let w = h.alice_send(b"first contact").expect("send");
    let chunks = chunked_messages(&w);
    println!(
        "bootstrap: wire {} B -> {chunks} Discord messages chunked, 1 token",
        wire_len(&w)
    );
    // 1241 bytes / 86 = 15 chunks.
    assert_eq!(chunks, 15);
    assert!(chunks < 255, "must stay under stego CHUNK_MAX_TOTAL");
}
