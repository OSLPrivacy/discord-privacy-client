//! Phase 9-A2 Task 2: PeerEntry carries optional ratchet state.
//!
//! Verifies the new fields don't break legacy on-disk records and
//! round-trip cleanly via serde for both DM (with ratchet) and
//! GC/server (without ratchet) peers.

use crypto::pqxdh::SessionKey;
use crypto::ratchet::{DoubleRatchet, RatchetStateOnDisk, SessionContext, SESSION_VERSION_V1};
use crypto::{ml_kem_768, pqxdh, x25519};
use ipc::peer_map::PeerEntry;
use std::fs;
use tempfile::tempdir;

fn build_ratchet_state() -> RatchetStateOnDisk {
    let (alice_ik_sk, alice_ik_pub) = x25519::generate_keypair();
    let (bob_ik_sk, bob_ik_pub) = x25519::generate_keypair();
    let (bob_spk_sk, bob_spk_pub) = x25519::generate_keypair();
    let (bob_mlkem_dk, bob_mlkem_ek) = ml_kem_768::generate_keypair();
    let (alice_sk, hs) =
        pqxdh::initiate(&alice_ik_sk, &bob_ik_pub, &bob_spk_pub, None, &bob_mlkem_ek).unwrap();
    let _bob_sk: SessionKey = pqxdh::respond(
        &bob_ik_sk,
        &bob_spk_sk,
        None,
        &bob_mlkem_dk,
        &alice_ik_pub,
        &hs,
    )
    .unwrap();
    let ctx = SessionContext {
        local_ik_x25519_pub: alice_ik_pub,
        local_ik_mlkem_pub: vec![0xaa; 1184],
        peer_ik_x25519_pub: bob_ik_pub,
        peer_ik_mlkem_pub: vec![0xbb; 1184],
        conversation_id: b"phase-a2-peer-map-test".to_vec(),
        session_version: SESSION_VERSION_V1,
    };
    let alice = DoubleRatchet::new_initiator(&alice_sk, &bob_spk_pub, ctx).unwrap();
    RatchetStateOnDisk::from(&alice)
}

#[test]
fn peer_entry_with_ratchet_state_serde_roundtrip() {
    let e = PeerEntry {
        pubkey: Some(
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gIQ==".to_string(), // dummy 32B b64
        ),
        discord_id: Some("900000000000000001".to_string()),
        ik_ratchet_initial_pub: Some("ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=".to_string()),
        ratchet_state: Some(build_ratchet_state()),
        ..Default::default()
    };

    let json = serde_json::to_string(&e).expect("serialize");
    let back: PeerEntry = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, e);
    assert!(back.ratchet_state.is_some(), "ratchet state survives");
    assert_eq!(
        back.ik_ratchet_initial_pub.as_deref(),
        Some("ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8=")
    );
}

#[test]
fn peer_entry_without_ratchet_state_loads_as_legacy() {
    // Pre-9-A2 record on disk: no `ratchet_state`, no
    // `ik_ratchet_initial_pub`. Must still load cleanly and produce
    // None for both new fields.
    let json = r#"{
        "osl_user_id": "henry",
        "pubkey": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gIQ==",
        "discord_id": "900000000000000001",
        "first_seen": "2026-05-09T12:00:00Z",
        "incoming_decrypt_accepted": {},
        "outgoing_whitelists": [],
        "burned_scopes": []
    }"#;
    let e: PeerEntry = serde_json::from_str(json).expect("legacy parses");
    assert!(e.ratchet_state.is_none());
    assert!(e.ik_ratchet_initial_pub.is_none());
    // Re-serialize: the new fields must NOT appear when None
    // (skip_serializing_if keeps the file clean for non-DM peers).
    let out = serde_json::to_string(&e).unwrap();
    assert!(
        !out.contains("ratchet_state"),
        "ratchet_state must be omitted when None, got: {out}"
    );
    assert!(
        !out.contains("ik_ratchet_initial_pub"),
        "ik_ratchet_initial_pub must be omitted when None, got: {out}"
    );
}

#[test]
fn peer_map_file_roundtrip_with_mixed_dm_and_gc_peers() {
    // Two peers: one DM (with ratchet), one GC member (no ratchet).
    // Confirm both load+save through the encrypted-at-rest pipeline
    // (no password installed → plain JSON path) without corrupting
    // either record.
    let dir = tempdir().unwrap();
    let path = dir.path().join("peer_map.json");

    let dm_peer = PeerEntry {
        discord_id: Some("DM_PEER".to_string()),
        ratchet_state: Some(build_ratchet_state()),
        ..Default::default()
    };

    let gc_peer = PeerEntry {
        discord_id: Some("GC_PEER".to_string()),
        osl_user_id: Some("alice".to_string()),
        ..Default::default()
    };

    let mut map = std::collections::HashMap::<String, PeerEntry>::new();
    map.insert("DM_PEER".to_string(), dm_peer.clone());
    map.insert("GC_PEER".to_string(), gc_peer.clone());

    ipc::peer_map::write_peer_map(&path, &map).expect("write");
    let raw = fs::read_to_string(&path).expect("read raw");
    assert!(
        raw.contains("ratchet_state"),
        "DM peer's ratchet_state should serialize"
    );

    let reloaded = ipc::peer_map::load_peer_map_from_path(&path).expect("reload");
    assert_eq!(reloaded.get("DM_PEER"), Some(&dm_peer));
    assert_eq!(reloaded.get("GC_PEER"), Some(&gc_peer));
    assert!(
        reloaded.get("GC_PEER").unwrap().ratchet_state.is_none(),
        "GC peer must round-trip with ratchet_state still None"
    );
}
