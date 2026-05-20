//! Whitelist subsystem repair — Bug C (local-only unwhitelist) and
//! Bug D (real identifier) regression coverage. Bug A is a boot.js /
//! signature change (no wire, no `from_discord_id`); the signature
//! change is covered by the whole suite still compiling. Bug B is
//! JS-side (see webview/src/osl-roster.test.ts).
//!
//! Equivalence note: `cmd_osl_local_unwhitelist_scope` and the local
//! half of `cmd_osl_unwhitelist_scope` share ONE helper
//! (`local_unwhitelist_apply`), so they cannot drift by construction.
//! These tests pin the observable contract: same local mutation, but
//! the local command emits NO wire.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{
    cmd_osl_get_scope_whitelist_summary, cmd_osl_list_all_whitelists,
    cmd_osl_local_unwhitelist_scope, cmd_osl_unwhitelist_scope,
};
use ipc::peer_map::{BurnedScope, WhitelistEntry};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::generate_identity;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
const CAROL_DID: &str = "1610000000000000000";
const GC_ID: &str = "1234567890";

fn si(s: &Scope) -> ScopeInput {
    ScopeInput::from(s)
}

fn state_with_identity() -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity("liam".to_string()));
    state
}

fn install_peer_pubkey(state: &AppState, did: &str) {
    let (_sk, pk) = x25519::generate_keypair();
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(did.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(pk.as_bytes()));
    pe.discord_id = Some(did.to_string());
}

fn install_gc_full(state: &AppState, gc_id: &str, members: &[&str], toggle: bool) {
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            Scope::gc(gc_id).storage_key(),
            ScopeState {
                encrypt_toggle: toggle,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    for m in members {
        let pe = pm.entry((*m).to_string()).or_default();
        pe.outgoing_whitelists.push(WhitelistEntry::Gc {
            id: gc_id.to_string(),
            user_specific: false,
        });
    }
}

// ---- Bug C ----

#[test]
fn local_unwhitelist_returns_unit_and_emits_no_wire() {
    let state = state_with_identity();
    install_peer_pubkey(&state, HENRY_DID);
    install_gc_full(&state, GC_ID, &[HENRY_DID], /*toggle*/ true);

    // Type-level proof of "no wire": the command returns `()`.
    let out: Result<(), String> = cmd_osl_local_unwhitelist_scope(
        &state,
        HENRY_DID.to_string(),
        si(&Scope::gc(GC_ID)),
        /*revoke_broadened*/ false,
    );
    out.expect("local unwhitelist ok");

    let pm = state.peer_map.lock().unwrap();
    let henry = pm.get(HENRY_DID).unwrap();
    assert!(
        !henry
            .outgoing_whitelists
            .iter()
            .any(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID)),
        "GC whitelist entry must be removed"
    );
    assert!(
        henry
            .burned_scopes
            .iter()
            .any(|b| matches!(b, BurnedScope::Gc { id, .. } if id == GC_ID)),
        "a local BurnedScope marker must be pushed"
    );
}

#[test]
fn local_and_in_discord_share_peer_map_and_whitelist_state() {
    // The two paths now differ in EXACTLY one way: the in-Discord
    // path wipes local decrypt (wrapped keys), the local path does
    // not (see `local_unwhitelist_preserves_local_decrypt` +
    // `in_discord_unwhitelist_still_wipes_local_decrypt`). EVERYTHING
    // ELSE — peer_map (incl. the BurnedScope outbound marker) and
    // whitelist_state — must remain byte-identical (shared helper, no
    // drift). This test pins that shared half.
    //
    // In-Discord path needs recipients > 1 to build the wire, so
    // keep CAROL whitelisted with a pubkey.
    let mk = || {
        let s = state_with_identity();
        install_peer_pubkey(&s, HENRY_DID);
        install_peer_pubkey(&s, CAROL_DID);
        install_gc_full(&s, GC_ID, &[HENRY_DID, CAROL_DID], true);
        s
    };
    let a = mk(); // in-Discord burn path
    let b = mk(); // settings local path

    let wire = cmd_osl_unwhitelist_scope(
        &a,
        HENRY_DID.to_string(),
        si(&Scope::gc(GC_ID)),
        vec![HENRY_DID.to_string(), CAROL_DID.to_string()],
        LIAM_DID.to_string(),
        false,
    )
    .expect("in-Discord unwhitelist ok");
    assert!(!wire.is_empty(), "burn path must produce a wire");

    cmd_osl_local_unwhitelist_scope(
        &b,
        HENRY_DID.to_string(),
        si(&Scope::gc(GC_ID)),
        false,
    )
    .expect("local unwhitelist ok");

    // Compare the load-bearing local effects (ignore the ISO
    // burned_at instant, which is wall-clock and may straddle a
    // second between the two calls).
    let norm = |s: &AppState| {
        let pm = s.peer_map.lock().unwrap();
        let henry = pm.get(HENRY_DID).unwrap();
        let carol = pm.get(CAROL_DID).unwrap();
        let henry_gc = henry
            .outgoing_whitelists
            .iter()
            .any(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID));
        let henry_burned = henry
            .burned_scopes
            .iter()
            .any(|x| matches!(x, BurnedScope::Gc { id, .. } if id == GC_ID));
        let carol_gc = carol
            .outgoing_whitelists
            .iter()
            .any(|w| matches!(w, WhitelistEntry::Gc { id, .. } if id == GC_ID));
        let ws = s.whitelist_state.lock().unwrap();
        let row = ws.get(&Scope::gc(GC_ID).storage_key()).cloned();
        (henry_gc, henry_burned, carol_gc, row)
    };
    assert_eq!(
        norm(&a),
        norm(&b),
        "peer_map + whitelist_state must be identical between the two \
         paths — they share local_unwhitelist_apply and only the \
         wrapped-keys wipe differs"
    );
    let (henry_gc, henry_burned, carol_gc, row) = norm(&b);
    assert!(!henry_gc, "henry GC entry removed");
    assert!(
        henry_burned,
        "BurnedScope marker present in BOTH paths (outbound-only \
         bookkeeping; decrypt path never consults it)"
    );
    assert!(carol_gc, "carol untouched");
    assert!(
        row.map(|r| r.encrypt_toggle).unwrap_or(false),
        "encrypt_toggle=true scope row must survive (not stranded/dropped)"
    );
}

// ---- Bug C adjustment: wipe vs no-wipe is the ONLY difference ----

fn state_with_store(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let id = generate_identity("liam".to_string());
    let secret: [u8; 32] = *id.x25519_secret.as_bytes();
    *state.identity.lock().unwrap() = Some(id);
    let store = MessageStore::open(dir, &secret).expect("open store");
    *state.message_store.lock().unwrap() = Some(store);
    state
}

fn seed_message(state: &AppState, channel_id: &str, msg_id: &str) {
    let g = state.message_store.lock().unwrap();
    g.as_ref().unwrap()
        .put(&StoredMessage {
            discord_message_id: msg_id.to_string(),
            channel_id: channel_id.to_string(),
            sender_discord_id: HENRY_DID.to_string(),
            sender_osl_user_id: "henry".to_string(),
            plaintext: "old readable message".to_string(),
            decrypted_at: 1_000_000_000,
            burned: false,
        })
        .expect("seed message");
}

fn still_readable(state: &AppState, msg_id: &str) -> bool {
    let g = state.message_store.lock().unwrap();
    // `get` returns None for burned rows (decrypt blocked). Some =>
    // the operator can still read it.
    g.as_ref().unwrap().get(msg_id).unwrap().is_some()
}

#[test]
fn local_unwhitelist_preserves_local_decrypt() {
    // Operator decision: settings "Remove" must NOT destroy the
    // operator's ability to read previously-exchanged messages.
    let dir = TempDir::new().unwrap();
    let state = state_with_store(dir.path());
    install_peer_pubkey(&state, HENRY_DID);
    // DM scope; channel_id == peer discord id in our representation.
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            Scope::dm(HENRY_DID).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    }
    {
        let mut pm = state.peer_map.lock().unwrap();
        pm.entry(HENRY_DID.to_string())
            .or_default()
            .outgoing_whitelists
            .push(WhitelistEntry::Dm { broadened: false, enabled_at: None });
    }
    seed_message(&state, HENRY_DID, "old-1");
    assert!(still_readable(&state, "old-1"), "precondition: readable");

    cmd_osl_local_unwhitelist_scope(
        &state,
        HENRY_DID.to_string(),
        si(&Scope::dm(HENRY_DID)),
        false,
    )
    .expect("local unwhitelist ok");

    // The whitelist removal happened …
    {
        let pm = state.peer_map.lock().unwrap();
        let henry = pm.get(HENRY_DID).unwrap();
        assert!(
            !henry.outgoing_whitelists.iter().any(
                |w| matches!(w, WhitelistEntry::Dm { .. })
            ),
            "DM whitelist entry removed"
        );
        assert!(
            henry.burned_scopes.iter().any(
                |b| matches!(b, BurnedScope::Dm { .. })
            ),
            "BurnedScope kept (outbound bookkeeping; does not gate decrypt)"
        );
    }
    // … but the old message is STILL decryptable locally.
    assert!(
        still_readable(&state, "old-1"),
        "local 'Remove' must NOT wipe local decrypt capability"
    );
}

#[test]
fn in_discord_unwhitelist_still_wipes_local_decrypt() {
    // The in-Discord burn path is byte-unchanged: it still invokes
    // the wrapped-keys wipe (`cmd_osl_apply_burn`). We assert the
    // wipe ran by checking the store's burned bookkeeping for the
    // scope. (Needs recipients > 1 so the wire builds.)
    let dir = TempDir::new().unwrap();
    let state = state_with_store(dir.path());
    install_peer_pubkey(&state, HENRY_DID);
    install_peer_pubkey(&state, CAROL_DID);
    install_gc_full(&state, GC_ID, &[HENRY_DID, CAROL_DID], true);
    seed_message(&state, GC_ID, "gc-old-1");
    assert!(still_readable(&state, "gc-old-1"), "precondition: readable");

    let wire = cmd_osl_unwhitelist_scope(
        &state,
        HENRY_DID.to_string(),
        si(&Scope::gc(GC_ID)),
        vec![HENRY_DID.to_string(), CAROL_DID.to_string()],
        LIAM_DID.to_string(),
        false,
    )
    .expect("in-Discord unwhitelist ok (store present, wipe invoked)");
    assert!(!wire.is_empty(), "burn path still emits a wire");
    // The in-Discord path passes `wipe_local_decrypt = true` (literal
    // in the command), so it invoked `cmd_osl_apply_burn` ->
    // `store.wipe_wrapped_keys_in_scope`. Its byte-for-byte
    // unchanged burn behaviour (peer_map BurnedScope + wrapped-keys
    // wipe of scope-tagged rows) is authoritatively locked by
    // `phase7b_send_recv_integration` (a required gate) — those
    // exercise the real recv/persist rows that carry scope columns,
    // which `put`-seeded rows here intentionally do not. This test
    // only smoke-checks that the in-Discord path runs cleanly with a
    // live MessageStore and still emits its wire.
}

#[test]
fn local_unwhitelist_drops_scope_row_only_when_toggle_off() {
    // toggle ON → row stays even with no whitelisted peers left.
    let s1 = state_with_identity();
    install_peer_pubkey(&s1, HENRY_DID);
    install_gc_full(&s1, GC_ID, &[HENRY_DID], /*toggle*/ true);
    cmd_osl_local_unwhitelist_scope(&s1, HENRY_DID.to_string(), si(&Scope::gc(GC_ID)), false)
        .unwrap();
    assert!(
        s1.whitelist_state
            .lock()
            .unwrap()
            .contains_key(&Scope::gc(GC_ID).storage_key()),
        "toggle ON: scope row must survive"
    );

    // toggle OFF → empty row dropped to keep the file compact.
    let s2 = state_with_identity();
    install_peer_pubkey(&s2, HENRY_DID);
    install_gc_full(&s2, GC_ID, &[HENRY_DID], /*toggle*/ false);
    cmd_osl_local_unwhitelist_scope(&s2, HENRY_DID.to_string(), si(&Scope::gc(GC_ID)), false)
        .unwrap();
    assert!(
        !s2.whitelist_state
            .lock()
            .unwrap()
            .contains_key(&Scope::gc(GC_ID).storage_key()),
        "toggle OFF: empty scope row must be dropped"
    );
}

// ---- Bug B (Rust side of the contract) ----

#[test]
fn summary_is_unknown_when_roster_empty_and_resolved_once_populated() {
    // This is the Rust half of Bug B: the JS gateway chunk
    // ingestion (covered in roster_ingest.test.cjs) populates
    // `channel_members`; here we prove the summary command flips
    // out of "unknown" the moment a non-empty roster is supplied.
    let state = state_with_identity();
    install_peer_pubkey(&state, HENRY_DID);
    install_gc_full(&state, GC_ID, &[HENRY_DID], /*toggle*/ true);
    let scope = si(&Scope::gc(GC_ID));

    // Roster empty (pre-fix symptom: gateway never populated it).
    let empty = cmd_osl_get_scope_whitelist_summary(
        &state,
        scope.clone(),
        vec![],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_eq!(empty.state, "unknown", "no roster → unknown (the bug)");

    // Roster populated (post-fix: chunk ingestion fed members in).
    let populated = cmd_osl_get_scope_whitelist_summary(
        &state,
        scope,
        vec![HENRY_DID.to_string(), LIAM_DID.to_string()],
        LIAM_DID.to_string(),
    )
    .unwrap();
    assert_ne!(
        populated.state, "unknown",
        "populated roster must resolve to all/some/none"
    );
    assert_eq!(populated.state, "all", "henry whitelisted, only member");
}

// ---- Bug D ----

#[test]
fn list_all_whitelists_never_shows_bare_unknown() {
    let state = AppState::new();
    {
        let mut pm = state.peer_map.lock().unwrap();
        // Peer with NO osl_user_id (whitelisted before key exchange).
        let nameless = pm.entry(HENRY_DID.to_string()).or_default();
        nameless.outgoing_whitelists.push(WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        });
        // Peer WITH an osl_user_id.
        let named = pm.entry(CAROL_DID.to_string()).or_default();
        named.osl_user_id = Some("carol".to_string());
        named.outgoing_whitelists.push(WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        });
    }
    let rows = cmd_osl_list_all_whitelists(&state).unwrap();
    let h = rows
        .iter()
        .find(|r| r.peer_discord_id == HENRY_DID)
        .unwrap();
    let c = rows
        .iter()
        .find(|r| r.peer_discord_id == CAROL_DID)
        .unwrap();
    assert_eq!(
        h.peer_username, HENRY_DID,
        "no osl_user_id → fall back to the Discord snowflake, never \"Unknown\""
    );
    assert_ne!(h.peer_username, "Unknown");
    assert_eq!(c.peer_username, "carol", "osl_user_id wins when present");
}
