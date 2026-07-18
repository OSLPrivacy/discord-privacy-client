//! DM round-trip through the IPC layer.
//!
//! Drives `cmd_osl_encrypt_message_v2` and
//! `cmd_osl_decrypt_message_v2` directly with two AppStates
//! representing alice (initiator) and bob (responder). The point
//! is to prove the IPC layer correctly persists, loads, and
//! follows the current stateless v=3 DM policy. The dormant v=4
//! ratchet primitives have their own focused crypto tests.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity, KeyServerClient};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

const ALICE_DID: &str = "900000000000000003";
const BOB_DID: &str = "900000000000000001";

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

    // Commit 0889049 made the v=4 send path fail-closed: it forces a
    // refresh_peer_pubkeys_from_keyserver before encrypting and
    // returns Err if no keyserver is installed. This harness has the
    // peer keys wired directly, so we serve those SAME keys back from
    // a loopback fake keyserver — the forced refresh succeeds,
    // populate_peer_from_fetch_response is idempotent (identical
    // keys), and the production fail-closed guard stays fully intact
    // (it genuinely succeeds against a real HTTP round-trip). The
    // ik_ed25519_pub returned is each identity's REAL key, so the
    // refresh's TOFU classify is FirstUse→Unchanged (NOT Changed —
    // a different ed25519 would trip 4033e82 and clear ratchet_state
    // mid-suite, masking the DR behavior under test).
    let alice_json = pubkeys_json_for(&alice_state, ALICE_DID);
    let bob_json = pubkeys_json_for(&bob_state, BOB_DID);
    let addr = spawn_fake_keyserver(vec![
        (ALICE_DID.to_string(), alice_json),
        (BOB_DID.to_string(), bob_json),
    ]);
    let base = format!("http://{addr}");
    *alice_state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&base).expect("fake keyserver client (alice)"));
    *bob_state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&base).expect("fake keyserver client (bob)"));

    (alice_state, bob_state)
}

/// Build the `GET /v1/pubkeys/:id` JSON body (the
/// `keystore::client::PubkeysResponse` shape) from a state's loaded
/// identity. ed25519 is the identity's REAL key (TOFU stays
/// FirstUse→Unchanged).
fn pubkeys_json_for(state: &AppState, user_id: &str) -> String {
    let g = state.identity.lock().unwrap();
    let id = g.as_ref().expect("identity loaded");
    let x = STANDARD.encode(id.x25519_public.as_bytes());
    let ed = STANDARD.encode(id.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(id.mlkem_public_bytes);
    let ratchet = STANDARD.encode(
        id.ratchet_initial_pub
            .expect("fresh identity has ratchet pub")
            .as_bytes(),
    );
    format!(
        "{{\"user_id\":\"{user_id}\",\"ik_x25519_pub\":\"{x}\",\
         \"ik_ed25519_pub\":\"{ed}\",\"ik_mlkem768_pub\":\"{mlkem}\",\
         \"registered_at\":\"2026-01-01T00:00:00Z\",\
         \"last_rotated_at\":null,\
         \"ik_ratchet_initial_pub\":\"{ratchet}\"}}"
    )
}

/// Hand-rolled loopback HTTP server (no dependency). Serves
/// `GET /v1/pubkeys/<user_id>` from `routes` and `POST /v1/register`
/// with a minimal RegisterResponse. Sequential, Connection: close,
/// one daemon thread. Returns the bound `127.0.0.1:PORT`.
fn spawn_fake_keyserver(routes: Vec<(String, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local_addr").to_string();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let mut stream = match conn {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Read until end of headers.
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 64 * 1024 {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf);
            let req_line = head.lines().next().unwrap_or("");
            let mut parts = req_line.split_whitespace();
            let method = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("");

            let (status, body): (&str, String) =
                if method == "GET" && path.starts_with("/v1/pubkeys/") {
                    let id = path.trim_start_matches("/v1/pubkeys/");
                    match routes.iter().find(|(u, _)| u == id) {
                        Some((_, json)) => ("200 OK", json.clone()),
                        None => ("404 Not Found", String::new()),
                    }
                } else if method == "POST" && path == "/v1/register" {
                    (
                        "200 OK",
                        "{\"user_id\":\"test\",\"status\":\"noop\"}".to_string(),
                    )
                } else {
                    ("404 Not Found", String::new())
                };

            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
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
                channel_whitelisted: false,
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
    .map(|w| w.content)
}

fn bob_sends(bob_state: &AppState, plaintext: &str) -> Result<String, String> {
    cmd_osl_encrypt_message_v2_wire(
        bob_state,
        plaintext.to_string(),
        ScopeInput::from(&Scope::dm(ALICE_DID)),
        vec![ALICE_DID.to_string(), BOB_DID.to_string()],
        BOB_DID.to_string(),
    )
    .map(|w| w.content)
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
fn alice_encrypt_v3_bob_decrypts_without_ratchet_state() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    assert!(!peer_ratchet_state_is_set(&alice_state, BOB_DID));
    let wire = alice_sends(&alice_state, "hello bob").unwrap();
    assert!(
        wire.starts_with("DPC0::"),
        "wire must carry the DPC0:: prefix"
    );
    // OPTION B deliberately routes DMs through stateless v=3 to avoid
    // the historical v=4 ratchet desynchronization failure class.
    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    assert_eq!(raw[0], ipc::wire_v2::WIRE_VERSION_V3);
    assert!(
        !peer_ratchet_state_is_set(&alice_state, BOB_DID),
        "stateless v=3 send must not create dormant v=4 ratchet state"
    );
    let plain = bob_decrypts(&bob_state, &wire).unwrap();
    assert_eq!(plain, "hello bob");
    assert!(
        !peer_ratchet_state_is_set(&bob_state, ALICE_DID),
        "stateless v=3 receive must not create dormant v=4 ratchet state"
    );
}

#[test]
fn bob_replies_v3_alice_decrypts_statelessly() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let w1 = alice_sends(&alice_state, "ping").unwrap();
    bob_decrypts(&bob_state, &w1).unwrap();
    // Bob's reply uses the same stateless v=3 DM policy.
    let w2 = bob_sends(&bob_state, "pong").unwrap();
    let raw = STANDARD.decode(w2.strip_prefix("DPC0::").unwrap()).unwrap();
    assert_eq!(raw[0], ipc::wire_v2::WIRE_VERSION_V3);
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
    // Stateless v=3 messages are independently decryptable, so arrival
    // order cannot desynchronize a session.
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let w0 = alice_sends(&alice_state, "a0").unwrap();
    let w1 = alice_sends(&alice_state, "a1").unwrap();
    let w2 = alice_sends(&alice_state, "a2").unwrap();
    let w3 = alice_sends(&alice_state, "a3").unwrap();
    let w4 = alice_sends(&alice_state, "a4").unwrap();
    // Deliberately deliver out of order.
    assert_eq!(bob_decrypts(&bob_state, &w0).unwrap(), "a0");
    assert_eq!(bob_decrypts(&bob_state, &w2).unwrap(), "a2");
    assert_eq!(bob_decrypts(&bob_state, &w4).unwrap(), "a4");
    assert_eq!(bob_decrypts(&bob_state, &w1).unwrap(), "a1");
    assert_eq!(bob_decrypts(&bob_state, &w3).unwrap(), "a3");
}

#[test]
fn repeated_bidirectional_v3_messages_decrypt() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    // Round trip 1.
    let m1 = alice_sends(&alice_state, "m1").unwrap();
    bob_decrypts(&bob_state, &m1).unwrap();
    let r1 = bob_sends(&bob_state, "r1").unwrap();
    alice_decrypts(&alice_state, &r1).unwrap();
    // Repeated replies remain independent and decryptable.
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
fn burst_after_bidirectional_exchange_stays_decryptable() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    // Initial bidirectional exchange.
    let m1 = alice_sends(&alice_state, "m1").unwrap();
    bob_decrypts(&bob_state, &m1).unwrap();
    let r1 = bob_sends(&bob_state, "r1").unwrap();
    alice_decrypts(&alice_state, &r1).unwrap();
    // A later five-message burst must all decrypt cleanly on Bob.
    for i in 0..5 {
        let m = alice_sends(&alice_state, &format!("post-{i}")).unwrap();
        assert_eq!(bob_decrypts(&bob_state, &m).unwrap(), format!("post-{i}"));
    }
}

#[test]
fn tampered_v3_ciphertext_rejected() {
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
fn tampered_v3_header_rejected() {
    let (alice_state, bob_state) = setup_alice_bob_dm_dr_ready();
    let wire = alice_sends(&alice_state, "header tamper").unwrap();
    let mut raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").unwrap())
        .unwrap();
    // Flip the msg_type byte (global header byte 1). The wrap AAD
    // binds the whole global header so this should fail with a
    // clear AEAD error before content decryption.
    raw[1] ^= 0xFF;
    let tampered = format!("DPC0::{}", STANDARD.encode(&raw));
    assert!(bob_decrypts(&bob_state, &tampered).is_err());
}
