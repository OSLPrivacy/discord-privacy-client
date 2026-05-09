use crypto::ed25519;
use keystore::{
    canonical_replenish_bytes, generate_identity, iso_8601_from_unix_seconds,
    load_prekey_state, save_prekey_state, sign_replenish_batch, MemorySealer,
    NoOpSealer, PrekeyConfig, PrekeyState, ReplenishOpk, ReplenishSpk,
    SPK_ROTATION_INTERVAL_SECONDS,
};
use tempfile::TempDir;

const T0: u64 = 1_700_000_000;

// ---- generation + targets ----

#[test]
fn new_state_generates_target_pool_and_initial_spk() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    assert_eq!(state.opk_pool.len(), 100);
    assert_eq!(state.config.opk_pool_target, 100);
    assert_eq!(state.config.opk_replenish_threshold, 25);
    assert_eq!(state.next_opk_id, 100);
    assert_eq!(state.current_spk.rotated_at_unix_seconds, T0);
    assert!(state.previous_spk.is_none());
}

#[test]
fn opk_ids_are_unique_and_monotonic() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    let ids: Vec<u32> = state.opk_pool.iter().map(|o| o.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "ids must be unique");
    for i in 0..ids.len() {
        assert_eq!(ids[i] as usize, i);
    }
}

#[test]
fn spk_signature_verifies_against_identity_ed25519_pub() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    let sig = ed25519::Signature::from_bytes(state.current_spk.signature);
    assert!(ed25519::verify(&id.ed25519_public, &state.current_spk.public, &sig).unwrap());
}

#[test]
fn add_opk_batch_extends_pool_with_fresh_keys() {
    let id = generate_identity("alice".to_string());
    let mut state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    let prev_len = state.opk_pool.len();
    let added = state.add_opk_batch(20);
    assert_eq!(added.len(), 20);
    let new_len = state.opk_pool.len();
    assert_eq!(new_len, prev_len + 20);
    // Pull added IDs once before reborrowing state mutably.
    let added_ids: Vec<u32> = state.opk_pool[prev_len..].iter().map(|o| o.id).collect();
    assert_eq!(added_ids[0], 100);
    assert_eq!(added_ids[19], 119);
    assert_eq!(state.next_opk_id, 120);
}

#[test]
fn should_replenish_at_or_below_threshold() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    assert!(state.should_replenish(0));
    assert!(state.should_replenish(25));
    assert!(!state.should_replenish(26));
    assert!(!state.should_replenish(100));
}

#[test]
fn replenish_count_to_target_caps_at_zero() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    assert_eq!(state.replenish_count_to_target(0), 100);
    assert_eq!(state.replenish_count_to_target(25), 75);
    assert_eq!(state.replenish_count_to_target(100), 0);
    // Server reports MORE than target — clamps to 0, doesn't underflow.
    assert_eq!(state.replenish_count_to_target(200), 0);
}

#[test]
fn consume_opk_removes_only_matching_id() {
    let id = generate_identity("alice".to_string());
    let mut state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    let before = state.opk_pool.len();
    assert!(state.consume_opk(0));
    assert_eq!(state.opk_pool.len(), before - 1);
    // Idempotent: re-consuming the same id is a no-op.
    assert!(!state.consume_opk(0));
    assert_eq!(state.opk_pool.len(), before - 1);
}

// ---- SPK rotation ----

#[test]
fn should_rotate_spk_after_interval() {
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    assert!(!state.should_rotate_spk(T0));
    assert!(!state.should_rotate_spk(T0 + SPK_ROTATION_INTERVAL_SECONDS - 1));
    assert!(state.should_rotate_spk(T0 + SPK_ROTATION_INTERVAL_SECONDS));
    assert!(state.should_rotate_spk(T0 + SPK_ROTATION_INTERVAL_SECONDS * 2));
}

#[test]
fn rotate_spk_moves_current_to_previous() {
    let id = generate_identity("alice".to_string());
    let mut state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    let original_pub = state.current_spk.public;
    let new_t = T0 + SPK_ROTATION_INTERVAL_SECONDS;
    state.rotate_spk(&id, new_t);
    let new_spk_pub = state.current_spk.public;
    assert_ne!(new_spk_pub, original_pub);
    assert_eq!(state.current_spk.rotated_at_unix_seconds, new_t);
    let prev = state.previous_spk.as_ref().unwrap();
    assert_eq!(prev.public, original_pub);
    assert_eq!(prev.rotated_at_unix_seconds, T0);
    // New SPK signature verifies.
    let sig = ed25519::Signature::from_bytes(state.current_spk.signature);
    assert!(ed25519::verify(&id.ed25519_public, &state.current_spk.public, &sig).unwrap());
}

// ---- canonical encoding ----

#[test]
fn canonical_bytes_deterministic_for_identical_input() {
    let opks = vec![
        ReplenishOpk { id: 0, pub_b64: "AAA=".into() },
        ReplenishOpk { id: 1, pub_b64: "BBB=".into() },
    ];
    let spk = ReplenishSpk {
        pub_b64: "spk-pub".into(),
        signature_b64: "spk-sig".into(),
        rotated_at: "2026-05-08T12:00:00Z".into(),
    };
    let a = canonical_replenish_bytes("alice", Some(&spk), &opks);
    let b = canonical_replenish_bytes("alice", Some(&spk), &opks);
    assert_eq!(a, b);
}

#[test]
fn canonical_bytes_change_with_user_id() {
    let opks = vec![ReplenishOpk { id: 0, pub_b64: "AAA=".into() }];
    let a = canonical_replenish_bytes("alice", None, &opks);
    let b = canonical_replenish_bytes("bob", None, &opks);
    assert_ne!(a, b);
}

#[test]
fn canonical_bytes_change_with_spk_presence() {
    let opks = vec![ReplenishOpk { id: 0, pub_b64: "AAA=".into() }];
    let with_spk = canonical_replenish_bytes(
        "alice",
        Some(&ReplenishSpk {
            pub_b64: "p".into(),
            signature_b64: "s".into(),
            rotated_at: "t".into(),
        }),
        &opks,
    );
    let without = canonical_replenish_bytes("alice", None, &opks);
    assert_ne!(with_spk, without);
}

#[test]
fn canonical_bytes_change_with_opk_order() {
    let a_opks = vec![
        ReplenishOpk { id: 0, pub_b64: "A".into() },
        ReplenishOpk { id: 1, pub_b64: "B".into() },
    ];
    let b_opks = vec![
        ReplenishOpk { id: 1, pub_b64: "B".into() },
        ReplenishOpk { id: 0, pub_b64: "A".into() },
    ];
    let a = canonical_replenish_bytes("alice", None, &a_opks);
    let b = canonical_replenish_bytes("alice", None, &b_opks);
    assert_ne!(a, b);
}

// ---- signature ----

#[test]
fn batch_signature_verifies_with_identity_ed25519_pub() {
    let id = generate_identity("alice".to_string());
    let opks = vec![ReplenishOpk { id: 0, pub_b64: "AAA".into() }];
    let sig = sign_replenish_batch(&id, "alice", None, &opks);
    let bytes = canonical_replenish_bytes("alice", None, &opks);
    assert!(ed25519::verify(&id.ed25519_public, &bytes, &sig).unwrap());
}

#[test]
fn batch_signature_invalid_with_other_identity() {
    let alice = generate_identity("alice".to_string());
    let bob = generate_identity("bob".to_string());
    let opks = vec![ReplenishOpk { id: 0, pub_b64: "AAA".into() }];
    let sig = sign_replenish_batch(&alice, "alice", None, &opks);
    let bytes = canonical_replenish_bytes("alice", None, &opks);
    assert!(!ed25519::verify(&bob.ed25519_public, &bytes, &sig).unwrap());
}

// ---- iso_8601 helper ----

#[test]
fn iso_8601_round_trips_for_known_unix_seconds() {
    // 2024-01-01T00:00:00Z = 1704067200.
    assert_eq!(
        iso_8601_from_unix_seconds(1_704_067_200),
        "2024-01-01T00:00:00.000Z"
    );
    // 1970-01-01T00:00:00Z = 0.
    assert_eq!(iso_8601_from_unix_seconds(0), "1970-01-01T00:00:00.000Z");
    // 2000-02-29 (leap year) at midnight = 951782400.
    assert_eq!(
        iso_8601_from_unix_seconds(951_782_400),
        "2000-02-29T00:00:00.000Z"
    );
}

// ---- persistence ----

#[test]
fn save_load_round_trip_with_memory_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("prekeys.json");
    let sealer = MemorySealer::new();
    let id = generate_identity("alice".to_string());
    let original = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    save_prekey_state(&path, &original, &sealer).unwrap();
    let loaded = load_prekey_state(&path, &sealer).unwrap();
    assert_eq!(loaded.config, original.config);
    assert_eq!(loaded.opk_pool.len(), original.opk_pool.len());
    assert_eq!(
        loaded.current_spk.public,
        original.current_spk.public
    );
    assert_eq!(
        loaded.current_spk.signature,
        original.current_spk.signature
    );
    // SPK signature still verifies after round-trip.
    let sig = ed25519::Signature::from_bytes(loaded.current_spk.signature);
    assert!(ed25519::verify(
        &id.ed25519_public,
        &loaded.current_spk.public,
        &sig
    )
    .unwrap());
}

#[test]
fn save_with_noop_sealer_writes_insecure_banner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("prekeys.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    save_prekey_state(&path, &state, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("INSECURE prototype storage"));
}

#[test]
fn save_with_memory_sealer_does_not_leak_secrets() {
    // The OPK secrets must not appear in plaintext on disk when
    // sealed.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("prekeys.json");
    let sealer = MemorySealer::new();
    let id = generate_identity("alice".to_string());
    let state = PrekeyState::new(&id, PrekeyConfig::default(), T0);
    save_prekey_state(&path, &state, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    // First OPK secret encoded as base64 should NOT appear in the
    // sealed file.
    let secret_b64 = STANDARD.encode(state.opk_pool[0].secret);
    assert!(
        !raw.contains(&secret_b64),
        "sealed prekey state must not leak OPK secret bytes"
    );
}
