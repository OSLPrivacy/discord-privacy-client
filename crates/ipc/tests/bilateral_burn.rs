//! Bilateral burn end-to-end at the wire + ledger layer.
//!
//! Two real identities, real `encrypt_v3` envelopes, real ML-KEM keys. What
//! these tests establish that the unit tests in `ipc::revocation` cannot:
//!
//! - a `0x0A` revocation survives a real seal/open round trip and arrives with
//!   its type byte intact, so it cannot be mistaken for content;
//! - a third identity cannot open it, and a peer cannot re-aim one;
//! - `0x0A`/`0x0B` are distinguishable by framing alone before any key is used;
//! - the legacy `0x01` marker still opens, and is honoured as a **bounded**
//!   revocation.
//!
//! Nothing in this file prints or asserts on plaintext, key material or
//! conversation content. The only bytes ever compared are commitments and type
//! bytes.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::{ml_kem_768, x25519};
use ipc::control_messages::{
    deserialize_revocation_ack, deserialize_revocation_notice, serialize_revocation_ack,
    serialize_revocation_notice, RevocationAck, RevocationNotice,
};
use ipc::revocation::{
    accept_content, apply_inbound_revocation, burn_id, legacy_burn_notice, match_scope_commitment,
    record_content_accepted, scope_commit_key, scope_commitment, ContentDecision, InboundDecision,
    RefusalReason, RevocationLedger, SendCounters,
};
use ipc::wire_v2::{
    decrypt_v3, encrypt_v3, is_revocation_ack_bundle, is_revocation_bundle, RecipientV3,
    MSG_TYPE_BURN, MSG_TYPE_CONTENT, MSG_TYPE_REVOCATION, MSG_TYPE_REVOCATION_ACK,
};

const SCOPE: &str = "gc:1502771310428819569";

struct Party {
    ik_sk: x25519::SecretKey,
    ik_pub: x25519::PublicKey,
    mlkem_dk: ml_kem_768::DecapsulationKey,
    mlkem_ek: ml_kem_768::EncapsulationKey,
}

fn party() -> Party {
    let (ik_sk, ik_pub) = x25519::generate_keypair();
    let (mlkem_dk, mlkem_ek) = ml_kem_768::generate_keypair();
    Party {
        ik_sk,
        ik_pub,
        mlkem_dk,
        mlkem_ek,
    }
}

impl Party {
    fn recipient(&self) -> RecipientV3 {
        RecipientV3 {
            x25519_pub: self.ik_pub,
            mlkem_pub: self.mlkem_ek.clone(),
        }
    }
}

/// Seal a revocation notice from `from` to `to` exactly the way the Hub must.
fn seal_notice(from: &Party, to: &Party, notice: &RevocationNotice) -> String {
    let body = serialize_revocation_notice(notice).expect("notice serializes");
    encrypt_v3(
        &from.ik_sk,
        &from.ik_pub,
        &[to.recipient()],
        MSG_TYPE_REVOCATION,
        &body,
    )
    .expect("v=3 seal")
}

fn bundle_bytes(wire: &str) -> Vec<u8> {
    STANDARD
        .decode(wire.strip_prefix("DPC0::").expect("DPC0 prefix"))
        .expect("base64 body")
}

#[test]
fn a_revocation_round_trips_through_a_real_v3_envelope_with_its_type_intact() {
    let alice = party();
    let bob = party();
    let key = scope_commit_key(alice.ik_pub.as_bytes(), bob.ik_pub.as_bytes()).unwrap();
    let commitment = scope_commitment(&key, SCOPE);

    let mut counters = SendCounters::default();
    for _ in 0..12 {
        counters.next_send_seq(&commitment).unwrap();
    }
    let epoch = counters.next_burn_epoch(&commitment).unwrap();
    let upto = counters.current_send_seq(&commitment);
    let notice = RevocationNotice {
        scope_commitment: commitment,
        burn_epoch: epoch,
        burn_upto_seq: upto,
        message_commitments: Vec::new(),
        burn_id: burn_id(&key, &commitment, epoch, upto),
        issued_at: 1_700_000_000,
    };

    let wire = seal_notice(&alice, &bob, &notice);
    // Framing-only classification works before any key is touched, and does not
    // confuse a notice with an ack or with content.
    let raw = bundle_bytes(&wire);
    assert!(is_revocation_bundle(&raw));
    assert!(!is_revocation_ack_bundle(&raw));

    let opened = decrypt_v3(&wire, &bob.ik_sk, &bob.mlkem_dk).expect("bob opens");
    assert_eq!(opened.msg_type, MSG_TYPE_REVOCATION);
    assert_ne!(opened.msg_type, MSG_TYPE_CONTENT);
    let received = deserialize_revocation_notice(&opened.plaintext).unwrap();
    assert_eq!(received, notice);

    // Bob recognises which of *his* conversations this is, by recomputation.
    let bob_key = scope_commit_key(bob.ik_pub.as_bytes(), alice.ik_pub.as_bytes()).unwrap();
    assert_eq!(bob_key, key, "both ends derive the same commitment key");
    assert_eq!(
        match_scope_commitment(
            &bob_key,
            &received.scope_commitment,
            ["dm:someone-else", SCOPE, "gc:unrelated"].into_iter()
        ),
        Some(SCOPE)
    );

    // And applies it.
    let mut ledger = RevocationLedger::default();
    for seq in 1..=12 {
        record_content_accepted(&mut ledger, &received.scope_commitment, seq).unwrap();
    }
    let outcome =
        apply_inbound_revocation(&mut ledger, &bob_key, &received, 1_700_000_001).unwrap();
    assert_eq!(outcome.decision, InboundDecision::Applied);
    assert_eq!(outcome.destroy_upto_seq, 12);
    assert!(outcome.ack.applied);

    // The ack carries the id and one bit, and round trips as 0x0B.
    let ack_wire = encrypt_v3(
        &bob.ik_sk,
        &bob.ik_pub,
        &[alice.recipient()],
        MSG_TYPE_REVOCATION_ACK,
        &serialize_revocation_ack(&outcome.ack).unwrap(),
    )
    .unwrap();
    let ack_raw = bundle_bytes(&ack_wire);
    assert!(is_revocation_ack_bundle(&ack_raw));
    assert!(!is_revocation_bundle(&ack_raw));
    let ack_opened = decrypt_v3(&ack_wire, &alice.ik_sk, &alice.mlkem_dk).unwrap();
    assert_eq!(ack_opened.msg_type, MSG_TYPE_REVOCATION_ACK);
    assert_eq!(
        deserialize_revocation_ack(&ack_opened.plaintext).unwrap(),
        RevocationAck {
            burn_id: notice.burn_id,
            applied: true,
        }
    );
}

/// The notice body carries no plaintext scope identifier. Belt and braces on
/// top of the encryption: even the *serialized body* contains none of the
/// conversation's identifying bytes.
#[test]
fn a_notice_body_contains_no_plaintext_scope_identifier() {
    let alice = party();
    let bob = party();
    let key = scope_commit_key(alice.ik_pub.as_bytes(), bob.ik_pub.as_bytes()).unwrap();
    let commitment = scope_commitment(&key, SCOPE);
    let notice = RevocationNotice {
        scope_commitment: commitment,
        burn_epoch: 1,
        burn_upto_seq: 1,
        message_commitments: vec![ipc::revocation::message_commitment(
            &key,
            &commitment,
            "1502771310428819570",
        )],
        burn_id: burn_id(&key, &commitment, 1, 1),
        issued_at: 1,
    };
    let body = serialize_revocation_notice(&notice).unwrap();
    let haystack = String::from_utf8_lossy(&body);
    for needle in [SCOPE, "1502771310428819569", "1502771310428819570", "gc:"] {
        assert!(
            !haystack.contains(needle),
            "burn-contract.md forbids plaintext scope/message identifiers in a notice"
        );
        assert!(!body.windows(needle.len()).any(|w| w == needle.as_bytes()));
    }
    // Contrast: the legacy 0x01 body does carry them, which is why it is not the
    // wire type new code sends.
    let legacy = ipc::control_messages::serialize_burn_marker(&ipc::control_messages::BurnMarker {
        scope: ipc::scope::Scope::gc("1502771310428819569"),
        burned_at: 1,
    })
    .unwrap();
    assert!(String::from_utf8_lossy(&legacy).contains("1502771310428819569"));
}

/// A third identity cannot open a revocation, and cannot re-aim one it captured:
/// the commitment key is pair-specific, so the `burn_id` does not verify under
/// anyone else's key.
#[test]
fn a_captured_revocation_is_useless_to_a_third_identity() {
    let alice = party();
    let bob = party();
    let eve = party();
    let ab = scope_commit_key(alice.ik_pub.as_bytes(), bob.ik_pub.as_bytes()).unwrap();
    let ae = scope_commit_key(alice.ik_pub.as_bytes(), eve.ik_pub.as_bytes()).unwrap();
    let c_ab = scope_commitment(&ab, SCOPE);
    let notice = RevocationNotice {
        scope_commitment: c_ab,
        burn_epoch: 1,
        burn_upto_seq: 5,
        message_commitments: Vec::new(),
        burn_id: burn_id(&ab, &c_ab, 1, 5),
        issued_at: 1,
    };
    let wire = seal_notice(&alice, &bob, &notice);
    assert!(
        decrypt_v3(&wire, &eve.ik_sk, &eve.mlkem_dk).is_err(),
        "a v=3 envelope addressed to bob must not open for eve"
    );

    // Even handed the cleartext notice, Eve's ledger refuses it: under her own
    // key the burn id is wrong.
    let mut eve_ledger = RevocationLedger::default();
    let out = apply_inbound_revocation(&mut eve_ledger, &ae, &notice, 100).unwrap();
    assert_eq!(
        out.decision,
        InboundDecision::Refused(RefusalReason::BurnIdMismatch)
    );
    assert!(eve_ledger.scopes.is_empty());
}

/// The load-bearing property, end to end: replaying a genuine, correctly signed
/// revocation cannot reach content sent after it was issued.
#[test]
fn replaying_a_genuine_revocation_cannot_reach_later_content() {
    let alice = party();
    let bob = party();
    let key = scope_commit_key(alice.ik_pub.as_bytes(), bob.ik_pub.as_bytes()).unwrap();
    let commitment = scope_commitment(&key, SCOPE);

    let mut ledger = RevocationLedger::default();
    for seq in 1..=4 {
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }
    let notice = RevocationNotice {
        scope_commitment: commitment,
        burn_epoch: 1,
        burn_upto_seq: 4,
        message_commitments: Vec::new(),
        burn_id: burn_id(&key, &commitment, 1, 4),
        issued_at: 1,
    };
    let captured = seal_notice(&alice, &bob, &notice);

    let first = decrypt_v3(&captured, &bob.ik_sk, &bob.mlkem_dk).unwrap();
    assert_eq!(
        apply_inbound_revocation(
            &mut ledger,
            &key,
            &deserialize_revocation_notice(&first.plaintext).unwrap(),
            10
        )
        .unwrap()
        .decision,
        InboundDecision::Applied
    );

    for seq in 5..=9 {
        assert_eq!(
            accept_content(&ledger, &commitment, seq),
            ContentDecision::Accept
        );
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }

    // The same bytes, injected again days later. Still a valid envelope; still
    // inert.
    for now in [20, 30, 40] {
        let again = decrypt_v3(&captured, &bob.ik_sk, &bob.mlkem_dk).unwrap();
        let out = apply_inbound_revocation(
            &mut ledger,
            &key,
            &deserialize_revocation_notice(&again.plaintext).unwrap(),
            now,
        )
        .unwrap();
        assert_eq!(out.decision, InboundDecision::AlreadyApplied);
        assert_eq!(out.destroy_upto_seq, 0);
        assert!(
            out.ack.applied,
            "an ack must not distinguish already-applied"
        );
    }
    for seq in 5..=9 {
        assert_eq!(
            accept_content(&ledger, &commitment, seq),
            ContentDecision::Accept
        );
    }
}

/// Legacy interop: an old peer's `0x01` still opens on a real envelope, and is
/// honoured as a bounded revocation rather than a permanent scope flag.
#[test]
fn a_legacy_burn_marker_is_honoured_but_never_permanent() {
    let alice = party();
    let bob = party();
    let key = scope_commit_key(alice.ik_pub.as_bytes(), bob.ik_pub.as_bytes()).unwrap();
    let commitment = scope_commitment(&key, SCOPE);

    let marker_body =
        ipc::control_messages::serialize_burn_marker(&ipc::control_messages::BurnMarker {
            scope: ipc::scope::Scope::gc("1502771310428819569"),
            burned_at: 1_700_000_000,
        })
        .unwrap();
    let wire = encrypt_v3(
        &alice.ik_sk,
        &alice.ik_pub,
        &[bob.recipient()],
        MSG_TYPE_BURN,
        &marker_body,
    )
    .unwrap();
    let opened = decrypt_v3(&wire, &bob.ik_sk, &bob.mlkem_dk).unwrap();
    assert_eq!(opened.msg_type, MSG_TYPE_BURN);

    let mut ledger = RevocationLedger::default();
    for seq in 1..=3 {
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }
    let converted = legacy_burn_notice(&ledger, &key, &commitment, 1_700_000_001);
    assert_eq!(
        converted.burn_upto_seq, 3,
        "bounded at what we currently hold"
    );
    let out = apply_inbound_revocation(&mut ledger, &key, &converted, 1_700_000_001).unwrap();
    assert_eq!(out.decision, InboundDecision::Applied);
    assert_eq!(out.destroy_upto_seq, 3);

    // The conversation is NOT dead. This is the legacy defect, and its absence
    // is the whole point of converting rather than flagging.
    for seq in 4..=8 {
        assert_eq!(
            accept_content(&ledger, &commitment, seq),
            ContentDecision::Accept
        );
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }
}

/// A revocation must not be accepted on a retired type byte. `0x02`/`0x03` are
/// swallowed by the dispatcher's legacy-handshake arm, so framing must never
/// classify them as a burn.
#[test]
fn retired_type_bytes_are_never_classified_as_a_revocation() {
    for msg_type in [
        0x00u8, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x80,
    ] {
        let raw = [0x03u8, msg_type, 0, 0];
        assert!(!is_revocation_bundle(&raw), "0x{msg_type:02X} is not 0x0A");
        assert!(
            !is_revocation_ack_bundle(&raw),
            "0x{msg_type:02X} is not 0x0B"
        );
    }
    assert!(is_revocation_bundle(&[0x03, MSG_TYPE_REVOCATION]));
    assert!(is_revocation_ack_bundle(&[0x03, MSG_TYPE_REVOCATION_ACK]));
    // Wrong wire version, right type byte: not a revocation bundle.
    assert!(!is_revocation_bundle(&[0x02, MSG_TYPE_REVOCATION]));
    assert!(!is_revocation_bundle(&[MSG_TYPE_REVOCATION]));
    assert!(!is_revocation_bundle(&[]));
}
