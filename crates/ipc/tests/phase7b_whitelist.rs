//! Phase 7b whitelist resolution tests.
//!
//! Covers `whitelist::can_encrypt_to`, `recipients_for_scope`,
//! `should_decrypt_from` against the scope hierarchy +
//! broadened-DM + burn semantics from
//! `docs/phase-7-design.md` §§ 2, 3, 7.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::peer_map::{BurnedScope, PeerEntry, PeerMap, WhitelistEntry};
use ipc::scope::Scope;
use ipc::whitelist::{can_encrypt_to, recipients_for_scope, should_decrypt_from};
use ipc::whitelist_state::{ScopeState, WhitelistState};

// ---- fixtures ----

const HENRY_DID: &str = "1502770642930634812";
const ALICE_DID: &str = "1602770642930634812";
const LIAM_DID: &str = "1477008451799482419";
const GC_ID: &str = "1234567890";
const SERVER_ID: &str = "9876";
const CHANNEL_ID: &str = "5432";

/// Build a PeerEntry with a fresh pubkey and the given discord_id
/// recorded inline. Most tests don't need the pubkey but
/// `recipients_for_scope` does.
fn peer_entry_with_pubkey(discord_id: &str) -> (PeerEntry, x25519::PublicKey) {
    let (_sk, pk) = x25519::generate_keypair();
    let entry = PeerEntry {
        osl_user_id: None,
        pubkey: Some(STANDARD.encode(pk.as_bytes())),
        discord_id: Some(discord_id.to_string()),
        first_seen: None,
        incoming_decrypt_accepted: Default::default(),
        outgoing_whitelists: vec![],
        burned_scopes: vec![],
        outgoing_whitelist_responses: Default::default(),
        is_self: None,
    };
    (entry, pk)
}

/// Register a DM whitelist for `peer` (both maps).
fn add_dm_whitelist(
    state: &mut WhitelistState,
    peer_map: &mut PeerMap,
    peer_discord: &str,
    broadened: bool,
) {
    let scope = Scope::dm(peer_discord);
    state.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            full_whitelist: false,
            members: vec![],
            whitelisted_users: vec![],
        },
    );
    let entry = peer_map.entry(peer_discord.to_string()).or_default();
    entry.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened,
        enabled_at: None,
    });
}

/// Register a GC full whitelist with the named members.
fn add_gc_full_whitelist(
    state: &mut WhitelistState,
    peer_map: &mut PeerMap,
    gc_id: &str,
    members: &[&str],
) {
    let scope = Scope::gc(gc_id);
    state.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            full_whitelist: true,
            members: members.iter().map(|s| s.to_string()).collect(),
            whitelisted_users: vec![],
        },
    );
    for m in members {
        let entry = peer_map.entry((*m).to_string()).or_default();
        entry.outgoing_whitelists.push(WhitelistEntry::Gc {
            id: gc_id.to_string(),
            user_specific: false,
        });
    }
}

/// Register a GC per-user whitelist with the named subset.
fn add_gc_per_user_whitelist(
    state: &mut WhitelistState,
    peer_map: &mut PeerMap,
    gc_id: &str,
    whitelisted: &[&str],
) {
    let scope = Scope::gc(gc_id);
    state.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            full_whitelist: false,
            members: vec![],
            whitelisted_users: whitelisted.iter().map(|s| s.to_string()).collect(),
        },
    );
    for u in whitelisted {
        let entry = peer_map.entry((*u).to_string()).or_default();
        entry.outgoing_whitelists.push(WhitelistEntry::Gc {
            id: gc_id.to_string(),
            user_specific: true,
        });
    }
}

/// Register a server-channel per-user whitelist.
fn add_server_channel_per_user_whitelist(
    state: &mut WhitelistState,
    peer_map: &mut PeerMap,
    server_id: &str,
    channel_id: &str,
    whitelisted: &[&str],
) {
    let scope = Scope::server_channel(server_id, channel_id);
    state.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            full_whitelist: false,
            members: vec![],
            whitelisted_users: whitelisted.iter().map(|s| s.to_string()).collect(),
        },
    );
    for u in whitelisted {
        let entry = peer_map.entry((*u).to_string()).or_default();
        entry
            .outgoing_whitelists
            .push(WhitelistEntry::ServerChannel {
                server_id: server_id.to_string(),
                channel_id: channel_id.to_string(),
                user_specific: true,
            });
    }
}

// ---- 1. dm whitelisted ----

#[test]
fn test_can_encrypt_to_dm_whitelisted() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    add_dm_whitelist(&mut ws, &mut pm, HENRY_DID, /*broadened*/ false);

    let scope = Scope::dm(HENRY_DID);
    assert!(can_encrypt_to(&ws, &pm, &scope, HENRY_DID));
}

// ---- 2. dm not whitelisted ----

#[test]
fn test_can_encrypt_to_dm_not_whitelisted() {
    let ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);

    let scope = Scope::dm(HENRY_DID);
    assert!(!can_encrypt_to(&ws, &pm, &scope, HENRY_DID));
}

// ---- 3. dm broaden grants gc access ----

#[test]
fn test_dm_broaden_grants_gc_access() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    add_dm_whitelist(&mut ws, &mut pm, HENRY_DID, /*broadened*/ true);

    let gc_scope = Scope::gc(GC_ID);
    assert!(
        can_encrypt_to(&ws, &pm, &gc_scope, HENRY_DID),
        "DM broaden should grant GC access without explicit GC whitelist"
    );
}

// ---- 4. dm no broaden, no gc access ----

#[test]
fn test_dm_no_broaden_no_gc_access() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    add_dm_whitelist(&mut ws, &mut pm, HENRY_DID, /*broadened*/ false);

    let gc_scope = Scope::gc(GC_ID);
    assert!(
        !can_encrypt_to(&ws, &pm, &gc_scope, HENRY_DID),
        "DM whitelist without broaden must not grant GC access"
    );
}

// ---- 5. gc per-user whitelist ----

#[test]
fn test_gc_per_user_whitelist() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    pm.insert(ALICE_DID.to_string(), peer_entry_with_pubkey(ALICE_DID).0);
    add_gc_per_user_whitelist(&mut ws, &mut pm, GC_ID, &[HENRY_DID]);

    let scope = Scope::gc(GC_ID);
    assert!(can_encrypt_to(&ws, &pm, &scope, HENRY_DID));
    assert!(!can_encrypt_to(&ws, &pm, &scope, ALICE_DID));
}

// ---- 6. gc full whitelist ----

#[test]
fn test_gc_full_whitelist() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    pm.insert(ALICE_DID.to_string(), peer_entry_with_pubkey(ALICE_DID).0);
    add_gc_full_whitelist(&mut ws, &mut pm, GC_ID, &[HENRY_DID, ALICE_DID]);

    let scope = Scope::gc(GC_ID);
    assert!(can_encrypt_to(&ws, &pm, &scope, HENRY_DID));
    assert!(can_encrypt_to(&ws, &pm, &scope, ALICE_DID));
}

// ---- 7. server-channel isolated from server-full ----

#[test]
fn test_server_channel_isolated_from_server_full() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    pm.insert(HENRY_DID.to_string(), peer_entry_with_pubkey(HENRY_DID).0);
    add_server_channel_per_user_whitelist(&mut ws, &mut pm, SERVER_ID, CHANNEL_ID, &[HENRY_DID]);

    let server_full = Scope::server_full(SERVER_ID);
    assert!(
        !can_encrypt_to(&ws, &pm, &server_full, HENRY_DID),
        "channel whitelist must not broaden to server-full"
    );
}

// ---- 8. recipients_for_scope includes self ----

#[test]
fn test_recipients_for_scope_includes_self() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    let (henry_entry, _henry_pk) = peer_entry_with_pubkey(HENRY_DID);
    let (alice_entry, _alice_pk) = peer_entry_with_pubkey(ALICE_DID);
    pm.insert(HENRY_DID.to_string(), henry_entry);
    pm.insert(ALICE_DID.to_string(), alice_entry);
    add_gc_full_whitelist(&mut ws, &mut pm, GC_ID, &[HENRY_DID, ALICE_DID]);

    let (_, self_pk) = x25519::generate_keypair();
    let scope = Scope::gc(GC_ID);
    let recipients = recipients_for_scope(
        &ws,
        &pm,
        &scope,
        &[
            LIAM_DID.to_string(),
            HENRY_DID.to_string(),
            ALICE_DID.to_string(),
        ],
        LIAM_DID,
        &self_pk,
    );

    assert!(
        recipients.contains(&self_pk),
        "self pubkey must be in recipients"
    );
    // 1 self + henry + alice = 3
    assert_eq!(recipients.len(), 3);
}

// ---- 9. burned scope excludes recipient ----

#[test]
fn test_burned_scope_excludes_recipient() {
    let mut ws = WhitelistState::new();
    let mut pm = PeerMap::new();
    let (henry_entry, _) = peer_entry_with_pubkey(HENRY_DID);
    pm.insert(HENRY_DID.to_string(), henry_entry);
    add_gc_full_whitelist(&mut ws, &mut pm, GC_ID, &[HENRY_DID]);

    // Now mark Henry burned for the GC.
    pm.get_mut(HENRY_DID)
        .unwrap()
        .burned_scopes
        .push(BurnedScope::Gc {
            id: GC_ID.to_string(),
            burned_at: "2026-05-09T13:00:00Z".to_string(),
        });

    let scope = Scope::gc(GC_ID);
    assert!(
        !can_encrypt_to(&ws, &pm, &scope, HENRY_DID),
        "burned recipient must not be encrypted-to"
    );

    let (_, self_pk) = x25519::generate_keypair();
    let recipients = recipients_for_scope(
        &ws,
        &pm,
        &scope,
        &[LIAM_DID.to_string(), HENRY_DID.to_string()],
        LIAM_DID,
        &self_pk,
    );
    // Only self should remain.
    assert_eq!(recipients, vec![self_pk]);
}

// ---- 10. should_decrypt_from accepted ----

#[test]
fn test_should_decrypt_from_accepted() {
    let mut pm = PeerMap::new();
    let mut entry = peer_entry_with_pubkey(LIAM_DID).0;
    let scope = Scope::dm(LIAM_DID);
    entry
        .incoming_decrypt_accepted
        .insert(scope.storage_key(), true);
    pm.insert(LIAM_DID.to_string(), entry);

    assert!(should_decrypt_from(&pm, &scope, LIAM_DID));
}

// ---- 11. should_decrypt_from declined ----

#[test]
fn test_should_decrypt_from_declined() {
    let mut pm = PeerMap::new();
    let mut entry = peer_entry_with_pubkey(LIAM_DID).0;
    let scope = Scope::dm(LIAM_DID);
    entry
        .incoming_decrypt_accepted
        .insert(scope.storage_key(), false);
    pm.insert(LIAM_DID.to_string(), entry);

    assert!(!should_decrypt_from(&pm, &scope, LIAM_DID));
}

// ---- 12. should_decrypt_from default (no entry) ----

#[test]
fn test_should_decrypt_from_not_yet_responded() {
    let mut pm = PeerMap::new();
    let entry = peer_entry_with_pubkey(LIAM_DID).0;
    pm.insert(LIAM_DID.to_string(), entry);

    let scope = Scope::dm(LIAM_DID);
    assert!(
        !should_decrypt_from(&pm, &scope, LIAM_DID),
        "default must be false until explicit accept"
    );
}
