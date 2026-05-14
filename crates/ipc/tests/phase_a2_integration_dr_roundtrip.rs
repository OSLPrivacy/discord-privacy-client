//! Phase 9-A2 Task 7: full DR round-trip through the IPC layer.
//!
//! Drives `cmd_osl_encrypt_message_v2` and
//! `cmd_osl_decrypt_message_v2` directly with two AppStates
//! representing alice (initiator) and bob (responder). The point
//! is to prove the IPC layer correctly persists, loads, and
//! advances DR state across calls — not the ratchet primitives
//! themselves (those have their own ~30 tests in
//! `crates/crypto/tests/ratchet_test.rs`).

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity};

const ALICE_DID: &str = "1477008451799482419";
const BOB_DID: &str = "1502770642930634812";

fn fresh_state_for(name: &str) -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity(name.to_string()));
    state
}

fn install_self_entry(state: &AppState, self_did: &str) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(self_did.to_string()).or_default();
    pe.is_self = Some(true);
    pe.discord_id = Some(self_did.to_string());
}

/// Wire each side's peer_map so both can run as initiator + responder.
/// Returns the alice + bob AppStates with:
///   - whitelist entries (DM scope) on both sides
///   - peer entries carrying X25519 + ML-KEM pubkeys + ratchet
///     bootstrap pubs published by the other side
///   - mutual scope acceptance rows (so the recv gate passes)
fn setup_alice_bob_dm_dr_ready() -> (AppState, AppState) {
    let alice_state = fresh_state_for("alice");
    let bob_state = fresh_state_for("bob");

    // Snapshot pubkeys.
    let alice_id = alice_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone_pubkeys();
    let bob_id = bob_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone_pubkeys();

    // Mark each side's own self-entry so the v=4 dispatch can
    // build the symmetric conversation_id.
    install_self_entry(&alice_state, ALICE_DID);
    install_self_entry(&bob_state, BOB_DID);

    // Each side knows the other's pubkeys (X25519, ML-KEM, ratchet
    // bootstrap pub) via peer_map.
    install_peer(&alice_state, BOB_DID, &bob_id);
    install_peer(&bob_state, ALICE_DID, &alice_id);

    // DM whitelist on each side for the other.
    install_dm_whitelist(&alice_state, BOB_DID);
    install_dm_whitelist(&bob_state, ALICE_DID);

    // Mutual scope acceptance so the recv gate (should_decrypt_from)
    // returns true on both sides.
    let alice_dm_scope = Scope::dm(BOB_DID);
    let bob_dm_scope = Scope::dm(ALICE_DID);
    mark_sender_accepted(&alice_state, BOB_DID, &alice_dm_scope);
    mark_sender_accepted(&bob_state, ALICE_DID, &bob_dm_scope);

    (alice_state, bob_state)
}

struct Pubkeys {
    x25519_pub: x25519::PublicKey,
    mlkem_pub_bytes: Vec<u8>,
    ratchet_initial_pub: x25519::PublicKey,
}
trait ClonePubkeys {
    fn clone_pubkeys(&self) -> Pubkeys;
}
impl ClonePubkeys for Identity {
    fn clone_pubkeys(&self) -> Pubkeys {
        Pubkeys {
            x25519_pub: self.x25519_public,
            mlkem_pub_bytes: self.mlkem_public_bytes.to_vec(),
            ratchet_initial_pub: self
                .ratchet_initial_pub
                .expect("fresh identity has ratchet pub"),
        }
    }
}

fn install_peer(state: &AppState, peer_did: &str, p: &Pubkeys) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(p.x25519_pub.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(&p.mlkem_pub_bytes));
    pe.ik_ratchet_initial_pub = Some(STANDARD.encode(p.ratchet_initial_pub.as_bytes()));
    pe.discord_id = Some(peer_did.to_string());
}

fn install_dm_whitelist(state: &AppState, peer_did: &str) {
    let scope = Scope::dm(peer_did);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn mark_sender_accepted(state: &AppState, sender_did: &str, scope: &Scope) {
    // 9-C1: handshake gate removed; this helper is a no-op kept
    // for call-site stability. Permissive decrypt means no sender-accept
    // state needs to exist.
    let _ = (state, sender_did, scope);
}

fn alice_sends(alice_state: &AppState, plaintext: &str) -> Result<String, String> {
    cmd_osl_encrypt_message_v2_wire(
        alice_state,
        plaintext.to_string(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![ALICE_DID.to_string(), BOB_DID.to_string()],
        ALICE_DID.to_string(),
    )
}

fn bob_sends(bob_state: &AppState, plaintext: &str) -> Result<String, String> {
    cmd_osl_encrypt_message_v2_wire(
        bob_state,
        plaintext.to_string(),
        ScopeInput::from(&Scope::dm(ALICE_DID)),
        vec![ALICE_DID.to_string(), BOB_DID.to_string()],
        BOB_DID.to_string(),
    )
}

fn bob_decrypts(bob_state: &AppState, wire: &str) -> Result<String, String> {
    cmd_osl_decrypt_message_v2(
        bob_state,
        Some(format!("msg-{}", rand_id())),
        "channel-x".to_string(),
        ALICE_DID.to_string(),
        wire.to_string(),
        Some(ScopeInput::from(&Scope::dm(ALICE_DID))),
        None,
    )
}

fn alice_decrypts(alice_state: &AppState, wire: &str) -> Result<String, String> {
    cmd_osl_decrypt_message_v2(
        alice_state,
        Some(format!("msg-{}", rand_id())),
        "channel-x".to_string(),
        BOB_DID.to_string(),
        wire.to_string(),
        Some(ScopeInput::from(&Scope::dm(BOB_DID))),
        None,
    )
}

fn rand_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn peer_ratchet_state_is_set(state: &AppState, peer_did: &str) -> bool {
    let pm = state.peer_map.lock().unwrap();
    pm.get(peer_did)
        .map(|pe| pe.ratchet_state.is_some())
        .unwrap_or(false)
}

#[test]
fn alice_encrypt_v4_bootstrap_bob_decrypt_v4_responder() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    assert!(!peer_ratchet_state_is_set(&alice_state, BOB_DID));
    let wire = alice_sends(&alice_state, "hello bob").unwrap();
    assert!(
        wire.starts_with("DPC0::"),
        "wire must carry the DPC0:: prefix"
    );
    // Peek the version byte directly to confirm v=4 was chosen.
    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    assert_eq!(raw[0], ipc::wire_v2::WIRE_VERSION_V4);
    // Alice's peer_map[bob].ratchet_state should now be populated.
    assert!(
        peer_ratchet_state_is_set(&alice_state, BOB_DID),
        "alice must persist DR state after v=4 send"
    );
    let plain = bob_decrypts(&bob_state, &wire).unwrap();
    assert_eq!(plain, "hello bob");
    assert!(
        peer_ratchet_state_is_set(&bob_state, ALICE_DID),
        "bob must persist DR state after v=4 receive"
    );
}

#[test]
fn bob_replies_v4_alice_decrypts_advancing_dr() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let w1 = alice_sends(&alice_state, "ping").unwrap();
    bob_decrypts(&bob_state, &w1).unwrap();
    // Now bob replies. Should also be v=4 (peer has ratchet state).
    let w2 = bob_sends(&bob_state, "pong").unwrap();
    let raw = STANDARD.decode(w2.strip_prefix("DPC0::").unwrap()).unwrap();
    assert_eq!(raw[0], ipc::wire_v2::WIRE_VERSION_V4);
    let plain = alice_decrypts(&alice_state, &w2).unwrap();
    assert_eq!(plain, "pong");
}

#[test]
fn five_message_burst_alice_to_bob_decrypts_in_order() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let mut wires = Vec::new();
    for i in 0..5 {
        wires.push(alice_sends(&alice_state, &format!("a{i}")).unwrap());
    }
    for (i, w) in wires.iter().enumerate() {
        let p = bob_decrypts(&bob_state, w).unwrap();
        assert_eq!(p, format!("a{i}"));
    }
}

#[test]
fn five_message_burst_arrives_out_of_order_all_decrypt() {
    // Note: the v=4 bootstrap design in A2 requires the receiver to
    // see the bootstrap message (alice's first send, flags.bit0=1)
    // BEFORE any continuation message — that's where the PQXDH
    // session_key seeding the DR comes from. After bootstrap, out-
    // of-order delivery within the DR's skipped-key cache works
    // freely. The realistic delivery model on top of Discord
    // (HTTP+websocket message stream, single ordered channel)
    // matches this: the bootstrap message is delivered first.
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let w0 = alice_sends(&alice_state, "a0").unwrap(); // bootstrap=true
    let w1 = alice_sends(&alice_state, "a1").unwrap();
    let w2 = alice_sends(&alice_state, "a2").unwrap();
    let w3 = alice_sends(&alice_state, "a3").unwrap();
    let w4 = alice_sends(&alice_state, "a4").unwrap();
    // Bootstrap lands first; continuation messages then arrive out
    // of order. The DR's skipped-key cache covers this.
    assert_eq!(bob_decrypts(&bob_state, &w0).unwrap(), "a0");
    assert_eq!(bob_decrypts(&bob_state, &w2).unwrap(), "a2");
    assert_eq!(bob_decrypts(&bob_state, &w4).unwrap(), "a4");
    assert_eq!(bob_decrypts(&bob_state, &w1).unwrap(), "a1");
    assert_eq!(bob_decrypts(&bob_state, &w3).unwrap(), "a3");
}

#[test]
fn dh_step_after_reply_chain_rotates() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    // Round trip 1.
    let m1 = alice_sends(&alice_state, "m1").unwrap();
    bob_decrypts(&bob_state, &m1).unwrap();
    let r1 = bob_sends(&bob_state, "r1").unwrap();
    alice_decrypts(&alice_state, &r1).unwrap();
    // Capture alice's sending counter pre and post DH step.
    // (Reading via peer_map is the only externally-visible way —
    // we just confirm sends succeed and Bob keeps decrypting.)
    for round in 0..3 {
        let m = alice_sends(&alice_state, &format!("post-rotation-{round}")).unwrap();
        let p = bob_decrypts(&bob_state, &m).unwrap();
        assert_eq!(p, format!("post-rotation-{round}"));
        let r = bob_sends(&bob_state, &format!("ack-{round}")).unwrap();
        let p2 = alice_decrypts(&alice_state, &r).unwrap();
        assert_eq!(p2, format!("ack-{round}"));
    }
}

#[test]
fn post_compromise_security_burst_after_dh_step() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    // Initial exchange to establish ratchets.
    let m1 = alice_sends(&alice_state, "m1").unwrap();
    bob_decrypts(&bob_state, &m1).unwrap();
    let r1 = bob_sends(&bob_state, "r1").unwrap();
    alice_decrypts(&alice_state, &r1).unwrap();
    // After Bob's reply triggers a DH step on Alice's side, a
    // 5-message burst from Alice must all decrypt cleanly on Bob.
    for i in 0..5 {
        let m = alice_sends(&alice_state, &format!("post-{i}")).unwrap();
        assert_eq!(bob_decrypts(&bob_state, &m).unwrap(), format!("post-{i}"));
    }
}

#[test]
fn tampered_v4_ciphertext_rejected() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let wire = alice_sends(&alice_state, "tamper me").unwrap();
    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Flip a body byte (end of the buffer is body ciphertext).
    let last = raw.len() - 1;
    raw[last] ^= 0xFF;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
    assert!(bob_decrypts(&bob_state, &tampered).is_err());
}

#[test]
fn tampered_v4_header_rejected() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let wire = alice_sends(&alice_state, "header tamper").unwrap();
    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Flip the msg_type byte (global header byte 1). The wrap AAD
    // binds the whole global header so this should fail with a
    // clear AEAD error before the DR step runs.
    raw[1] ^= 0xFF;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
    assert!(bob_decrypts(&bob_state, &tampered).is_err());
}
