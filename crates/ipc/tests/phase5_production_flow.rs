//! Phase 5 v1: end-to-end production-flow integration test.
//!
//! Mirrors what actually happens when liam types in Discord:
//!
//! 1. liam's IPC layer calls `encrypt_osl_phase4_to_pubkeys` with
//!    his identity secret + henry's pubkey. The encoder
//!    auto-includes liam himself as a final slot for sender
//!    self-decrypt UX.
//! 2. The wire bytes are wrapped in `DPC0::<base64>` and sent.
//! 3. Discord bounces them back as a `MESSAGE_CREATE`, the DOM
//!    observer in boot.js pulls liam's discord_id from the message
//!    DOM and calls `cmd_osl_decrypt_message` (Tauri command).
//! 4. The cmd resolves discord_id → OSL user_id via `peer_map`,
//!    fetches the sender's pubkey from the cache (or keyserver
//!    on miss), then hands the cover string to the pure decoder.
//!
//! What the existing `osl_phase5_decrypt.rs` covers: the pure
//! decoder against direct `(recipient_secret, sender_pub, cover)`
//! tuples. What it doesn't: the multi-step `cmd_osl_decrypt_message`
//! orchestration with `AppState`, peer_map resolution, and a
//! pre-warmed pubkey cache.
//!
//! Pre-warming the cache lets these tests stay zero-keyserver
//! (the cache-hit branch never hits the network). The cache-miss
//! → keyserver-fetch path is exercised by `keystore` crate
//! integration tests against a real Node keyserver subprocess.
//!
//! ## What we're trying to catch
//!
//! Phase 5's first dogfood run on Windows showed a "first message
//! decrypts, every subsequent fails with NoMatchingSlot" pattern.
//! Static-static ECDH is symmetric: if liam's keyserver pub
//! matches his local pub (which the user verified), the math
//! cannot fail "sometimes." So either:
//!
//! - The bug is NOT in the crypto or `cmd_osl_decrypt_message`
//!   orchestration → these tests pass and we look upstream
//!   (boot.js extraction picking up extra characters, React
//!   re-render quirks, etc.).
//! - There's a state-keeping bug in the IPC layer that only
//!   surfaces after a successful decrypt (cache poisoning,
//!   identity mutex held wrong) → these tests catch it.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_osl_decrypt_message, encrypt_osl_phase4_to_pubkeys, OSL_PHASE4_WIRE_VERSION,
};
use ipc::state::AppState;
use keystore::{generate_identity, Identity};

/// Install both peers' public keys into the cache, keyed by
/// each identity's own `user_id`. Order-insensitive — pass
/// either peer in either slot. (The earlier positional
/// `liam_pub` / `henry_pub` naming was a footgun: passing
/// `(henry, liam)` silently stored each pubkey under the
/// wrong key, so cache lookups returned the OTHER peer's
/// pubkey and decrypt failed with NoMatchingSlot.)
fn warm_cache(state: &AppState, a: &Identity, b: &Identity) {
    state
        .sender_pubkey_cache
        .insert(a.user_id.clone(), a.x25519_public.clone());
    state
        .sender_pubkey_cache
        .insert(b.user_id.clone(), b.x25519_public.clone());
}

/// Install the liam/henry peer_map both peers would have on disk.
fn install_peer_map(state: &AppState) {
    let mut pm = state.peer_map.lock().unwrap();
    pm.insert("1477008451799482419".to_string(), "liam".to_string());
    pm.insert("1502770642930634812".to_string(), "henry".to_string());
}

/// Build a fresh `AppState` with `loaded` as the loaded identity,
/// peer_map populated, and the cache pre-warmed with the
/// counterpart's pubkey (plus the loaded identity's own pubkey
/// for sender-self-decrypt).
fn fresh_state(loaded: Identity, counterpart: &Identity) -> AppState {
    let state = AppState::new();
    warm_cache(&state, &loaded, counterpart);
    install_peer_map(&state);
    *state.identity.lock().unwrap() = Some(loaded);
    state
}

/// Sender encrypts; returns the `DPC0::<base64>` cover string the
/// JS hook would put on the wire. `recipients_user_ids` simulates
/// the channels.json lookup result on the sender's box (sorted
/// upstream by `cmd_osl_encrypt_message`; we sort manually here
/// to match).
fn encrypt_as(sender: &Identity, others: &[&Identity], plaintext: &str) -> String {
    let mut sorted: Vec<&Identity> = others.to_vec();
    sorted.sort_by_key(|i| i.user_id.clone());
    let pubs: Vec<_> = sorted.iter().map(|i| i.x25519_public.clone()).collect();
    encrypt_osl_phase4_to_pubkeys(&sender.x25519_secret, &pubs, plaintext)
        .expect("encrypt should succeed for valid inputs")
}

// ---- Sender self-decrypt: liam encrypts to [henry], reads own bounce ----

#[test]
fn liam_decrypts_own_message_via_cmd() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    let cover = encrypt_as(&liam, &[&henry], "hello phase 5");

    let state = fresh_state(liam, &henry);
    let plaintext = cmd_osl_decrypt_message(
        &state,
        "channel-id".to_string(),
        "1477008451799482419".to_string(), // liam's discord id (sender)
        cover,
    )
    .expect("liam should decrypt his own bounced message");
    assert_eq!(plaintext, "hello phase 5");
}

#[test]
fn liam_decrypts_consecutive_own_messages() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    // Pre-build all covers (need liam by ref). Then move liam
    // into the state for the decrypt phase.
    let plaintexts = ["hello phase 5", "decrypt test", "another message"];
    let covers: Vec<String> = plaintexts
        .iter()
        .map(|p| encrypt_as(&liam, &[&henry], p))
        .collect();

    let state = fresh_state(liam, &henry);

    // Three messages back-to-back, all liam-as-sender bounced
    // through Discord. The user reported message 1 succeeding
    // then 2+ failing — this is the regression test for that.
    for (plaintext, cover) in plaintexts.iter().zip(covers.into_iter()) {
        let recovered = cmd_osl_decrypt_message(
            &state,
            "channel".to_string(),
            "1477008451799482419".to_string(),
            cover,
        )
        .unwrap_or_else(|e| panic!("decrypt of {plaintext:?} failed: {e}"));
        assert_eq!(&recovered, plaintext);
    }
}

// ---- Cross-peer decrypt: henry encrypts to [liam], liam reads ----

#[test]
fn liam_decrypts_henrys_message_via_cmd() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    let cover = encrypt_as(&henry, &[&liam], "from henry");
    let state = fresh_state(liam, &henry);

    let plaintext = cmd_osl_decrypt_message(
        &state,
        "channel".to_string(),
        "1502770642930634812".to_string(), // henry's discord id (sender)
        cover,
    )
    .expect("liam should decrypt henry's message");
    assert_eq!(plaintext, "from henry");
}

#[test]
fn henry_decrypts_liams_message_via_cmd() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    let cover = encrypt_as(&liam, &[&henry], "to henry");
    let state = fresh_state(henry, &liam);

    let plaintext = cmd_osl_decrypt_message(
        &state,
        "channel".to_string(),
        "1477008451799482419".to_string(), // liam's discord id (sender)
        cover,
    )
    .expect("henry should decrypt liam's message");
    assert_eq!(plaintext, "to henry");
}

// ---- Mixed sender sequence (mirrors a real conversation) ----

#[test]
fn liam_decrypts_mixed_sender_sequence() {
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    // Sequence: (sender_user_id_str, sender_did, plaintext).
    let cases: &[(&str, &str, &str)] = &[
        ("liam", "1477008451799482419", "liam first"),
        ("henry", "1502770642930634812", "henry replies"),
        ("liam", "1477008451799482419", "liam continues"),
        ("henry", "1502770642930634812", "henry again"),
        ("liam", "1477008451799482419", "liam closes"),
    ];
    let covers: Vec<(String, String)> = cases
        .iter()
        .map(|(sender_uid, _, plaintext)| {
            // From sender's perspective, encrypt to the OTHER
            // peer. Encoder auto-includes sender.
            let cover = if *sender_uid == "liam" {
                encrypt_as(&liam, &[&henry], plaintext)
            } else {
                encrypt_as(&henry, &[&liam], plaintext)
            };
            (cover, plaintext.to_string())
        })
        .collect();

    let state = fresh_state(liam, &henry);
    for ((cover, plaintext), (_, did, _)) in covers.into_iter().zip(cases.iter()) {
        let recovered =
            cmd_osl_decrypt_message(&state, "channel".to_string(), (*did).to_string(), cover)
                .unwrap_or_else(|e| panic!("decrypt of {plaintext:?} failed: {e}"));
        assert_eq!(&recovered, &plaintext);
    }
}

// ---- Cache-keyed-by-osl-id invariant ----

#[test]
fn cache_remains_valid_across_consecutive_decrypts() {
    // After one decrypt, the cache holds the sender's pubkey.
    // Subsequent decrypts must hit the cache and produce the
    // SAME shared secret. If something corrupts the cached
    // PublicKey value between calls, this test fails.
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());

    // Pre-build covers (henry sends, liam receives).
    let plaintexts = ["a", "ab", "abc", "abcd"];
    let covers: Vec<String> = plaintexts
        .iter()
        .map(|p| encrypt_as(&henry, &[&liam], p))
        .collect();

    let state = fresh_state(liam, &henry);

    // Snapshot the pubkey we put in the cache so we can compare
    // post-loop and detect silent eviction or replacement.
    let henry_pub_before = state
        .sender_pubkey_cache
        .get("henry")
        .expect("warm-cache should have inserted henry")
        .as_bytes()
        .to_vec();

    for (plaintext, cover) in plaintexts.iter().zip(covers.into_iter()) {
        let recovered = cmd_osl_decrypt_message(
            &state,
            "ch".to_string(),
            "1502770642930634812".to_string(),
            cover,
        )
        .unwrap_or_else(|e| panic!("decrypt of {plaintext:?} failed: {e}"));
        assert_eq!(&recovered, plaintext);
    }

    let henry_pub_after = state
        .sender_pubkey_cache
        .get("henry")
        .expect("henry cached after loop")
        .as_bytes()
        .to_vec();
    assert_eq!(
        henry_pub_after, henry_pub_before,
        "cached pubkey changed across consecutive decrypts (cache poisoning?)"
    );
}

// ---- Wire-shape invariants (smoke checks) ----

#[test]
fn wire_version_byte_stable_across_messages() {
    // If something accidentally bumped the wire version between
    // encoder and decoder, every message past the first would
    // hit UnsupportedVersion. Smoke-check that version is still
    // 0x01 in the produced cover.
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());
    let cover = encrypt_as(&liam, &[&henry], "x");
    let body = cover.strip_prefix("DPC0::").expect("DPC0 prefix");
    let raw = STANDARD.decode(body).expect("base64 decode");
    assert!(raw.len() >= 2);
    assert_eq!(raw[0], OSL_PHASE4_WIRE_VERSION);
}

#[test]
fn wire_recipient_count_includes_auto_sender() {
    // Encoder auto-includes sender. recipients=[henry] → N=2
    // (henry + liam-auto-include).
    let liam = generate_identity("liam".to_string());
    let henry = generate_identity("henry".to_string());
    let cover = encrypt_as(&liam, &[&henry], "x");
    let body = cover.strip_prefix("DPC0::").expect("DPC0 prefix");
    let raw = STANDARD.decode(body).expect("base64 decode");
    assert!(raw.len() >= 2);
    assert_eq!(raw[1], 2, "expected N=2 (henry + liam-auto)");
}

// ---- Post-rotation regression ----

/// Pull the slot pubkey hints out of an encrypted cover string.
/// Returns the per-slot first-byte values in wire order.
fn slot_hints(cover: &str) -> Vec<u8> {
    let body = cover.strip_prefix("DPC0::").expect("DPC0 prefix");
    let raw = STANDARD.decode(body).expect("base64 decode");
    let n = raw[1] as usize;
    let slot_size = 1 + 24 + (32 + 16); // hint + nonce_k + (key + tag)
    (0..n).map(|i| raw[2 + i * slot_size]).collect()
}

#[test]
fn rotation_simulated_via_state_replacement_uses_new_pub() {
    // The user-reported bug pattern: encrypt produces slot hints
    // for a STALE pubkey, while decrypt's our_hint reflects the
    // CURRENT secret. Symptom: NoMatchingSlot. This test simulates
    // a rotation by replacing `state.identity` mid-state and
    // verifies that the next encrypt's slot hints match the
    // CURRENT identity's derived pub — never the prior identity's.

    let old_id = generate_identity("liam".to_string());
    let new_id = generate_identity("liam".to_string());
    // Different secrets → different derived pubs.
    assert_ne!(
        old_id.x25519_public.as_bytes(),
        new_id.x25519_public.as_bytes(),
        "two fresh identities should not collide"
    );

    let henry = generate_identity("henry".to_string());

    // Pre-rotation encrypt with old identity.
    let pre = encrypt_osl_phase4_to_pubkeys(
        &old_id.x25519_secret,
        &[henry.x25519_public.clone()],
        "msg before rotation",
    )
    .unwrap();
    let pre_hints = slot_hints(&pre);
    assert_eq!(pre_hints.len(), 2, "henry + old-liam-auto-include");
    assert!(
        pre_hints.contains(&old_id.x25519_public.as_bytes()[0]),
        "pre-rotation wire must include old liam's hint"
    );

    // Rotation: state.identity = new_id. Cmd_osl_encrypt_message
    // would lock state.identity, take .x25519_secret, hand to
    // the encoder. Mirror that here.
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(new_id);
    let post_secret = state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_secret
        .clone();
    let post_pub = crypto::x25519::derive_public(&post_secret);
    let post = encrypt_osl_phase4_to_pubkeys(
        &post_secret,
        &[henry.x25519_public.clone()],
        "msg after rotation",
    )
    .unwrap();
    let post_hints = slot_hints(&post);
    assert_eq!(post_hints.len(), 2, "henry + new-liam-auto-include");
    assert!(
        post_hints.contains(&post_pub.as_bytes()[0]),
        "post-rotation wire must include NEW liam's hint, got {post_hints:?}"
    );
    assert!(
        !post_hints.contains(&old_id.x25519_public.as_bytes()[0])
            || old_id.x25519_public.as_bytes()[0] == post_pub.as_bytes()[0],
        "post-rotation wire must NOT include old liam's hint (unless coincident first byte)"
    );
}

#[test]
fn encoder_always_derives_sender_pub_from_secret() {
    // Defense against any future caller passing a stale
    // sender_pub via a side channel: the encoder takes only a
    // SecretKey and derives the public itself. Verify this is
    // true: change the secret, and the wire's auto-include hint
    // changes correspondingly.
    let id_a = generate_identity("a".to_string());
    let id_b = generate_identity("b".to_string());
    let henry = generate_identity("henry".to_string());

    let wire_a =
        encrypt_osl_phase4_to_pubkeys(&id_a.x25519_secret, &[henry.x25519_public.clone()], "x")
            .unwrap();
    let wire_b =
        encrypt_osl_phase4_to_pubkeys(&id_b.x25519_secret, &[henry.x25519_public.clone()], "x")
            .unwrap();
    let hints_a = slot_hints(&wire_a);
    let hints_b = slot_hints(&wire_b);

    // Both wires include henry's hint (same recipient pubkey).
    assert!(hints_a.contains(&henry.x25519_public.as_bytes()[0]));
    assert!(hints_b.contains(&henry.x25519_public.as_bytes()[0]));
    // Each wire ALSO includes its sender's auto-included hint.
    assert!(hints_a.contains(&id_a.x25519_public.as_bytes()[0]));
    assert!(hints_b.contains(&id_b.x25519_public.as_bytes()[0]));
}
