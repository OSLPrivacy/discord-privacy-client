//! Whitelist + burn resolution logic.
//!
//! Spec: `docs/phase-7-design.md` §§ 2 (whitelist scopes,
//! precedence, auto-enable) and 3 (burn semantics).
//!
//! Three pure functions form the public API:
//!
//! - [`can_encrypt_to`] — given `(whitelist_state, peer_map,
//!   scope, recipient)`, decide whether outgoing messages to
//!   `recipient` in `scope` should be encrypted.
//! - [`recipients_for_scope`] — given a scope and the channel's
//!   current member list, produce the `Vec<PublicKey>` to wrap K
//!   for. Always includes self.
//! - [`should_decrypt_from`] — given `(peer_map, scope, sender)`,
//!   decide whether incoming messages from `sender` in `scope`
//!   should be decrypted (after AEAD, before render).
//!
//! ## Sources of truth
//!
//! Membership data lives in two places that must stay in sync:
//!
//! - [`crate::whitelist_state::WhitelistState`] — per-scope:
//!   `encrypt_toggle`, full-vs-per-user flag, member list. Drives
//!   "does this scope encrypt at all?" and "who's in this scope?"
//! - [`crate::peer_map::PeerEntry::outgoing_whitelists`] — per-peer
//!   index of scopes that include them. Carries the DM
//!   `broadened` flag (the only per-peer flag the design doc
//!   tracks).
//!
//! Phase 7c UI will write to both atomically via
//! `cmd_osl_set_whitelist`; phase 7b consumes both for the
//! resolution functions below.
//!
//! ## Pubkey resolution
//!
//! [`recipients_for_scope`] needs pubkeys to wrap K. They come
//! from `peer_map[discord_id].pubkey` (base64-decoded). Peers
//! without a pubkey are silently skipped — they'll be re-included
//! once the pubkey is resolved (typically on first observed
//! invitation, when the peer's pubkey rides in the
//! [`crate::control_messages::WhitelistInvitation`] body).

use crate::peer_map::{BurnedScope, PeerEntry, PeerMap, WhitelistEntry};
use crate::scope::{Scope, ScopeKind};
use crate::whitelist_state::WhitelistState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;

/// Can outgoing messages to `recipient_discord_id` in `scope` be
/// encrypted under v=2?
///
/// Logic (per §2.2, "most-permissive wins"):
///
/// 1. If `recipient` is burned in `scope` → false. Burns are
///    absolute and override any whitelist.
/// 2. If `scope` has a whitelist entry with `encrypt_toggle` on
///    AND `recipient` is named in that scope's full or per-user
///    list → true.
/// 3. If `scope.kind != Dm` and `recipient` has a DM whitelist
///    with `broadened = true` → true. DM broaden grants are
///    cross-scope and don't depend on the target scope's toggle
///    (the DM toggle was the user's "I want to encrypt to this
///    peer" decision; broadening makes that decision portable).
/// 4. Otherwise false.
pub fn can_encrypt_to(
    whitelist_state: &WhitelistState,
    peer_map: &PeerMap,
    scope: &Scope,
    recipient_discord_id: &str,
) -> bool {
    if is_burned_in_scope(peer_map, scope, recipient_discord_id) {
        return false;
    }
    if scope_explicit_grant(whitelist_state, scope, recipient_discord_id) {
        return true;
    }
    if scope.kind != ScopeKind::Dm && has_broadened_dm_access(peer_map, recipient_discord_id) {
        return true;
    }
    false
}

/// Pubkeys to wrap K for when encrypting in `scope`.
///
/// Always includes `self_pubkey` (sender self-decrypt — see
/// `wire_v2` docs on the auto-include rationale). Otherwise walks
/// `channel_members` and includes each peer that
/// (a) `can_encrypt_to` returns true for in this scope, AND
/// (b) has a resolvable X25519 pubkey in `peer_map[discord_id].pubkey`.
///
/// Peers with a whitelist hit but no on-disk pubkey are silently
/// skipped. The caller can log this if it cares; the recv path
/// will pick the peer up once their pubkey lands via an observed
/// invitation.
///
/// `self_discord_id` is excluded from the channel-member walk
/// (we add `self_pubkey` ourselves up front) so a self-listed
/// channel doesn't double-include.
pub fn recipients_for_scope(
    whitelist_state: &WhitelistState,
    peer_map: &PeerMap,
    scope: &Scope,
    channel_members: &[String],
    self_discord_id: &str,
    self_pubkey: &x25519::PublicKey,
) -> Vec<x25519::PublicKey> {
    let mut out: Vec<x25519::PublicKey> = Vec::new();
    out.push(*self_pubkey);
    for member in channel_members {
        if member == self_discord_id {
            continue;
        }
        if !can_encrypt_to(whitelist_state, peer_map, scope, member) {
            continue;
        }
        if let Some(pk) = peer_pubkey(peer_map, member) {
            out.push(pk);
        }
    }
    out
}

/// Should an incoming AEAD-decrypted message from `sender` in
/// `scope` be surfaced as plaintext, or held as cover until the
/// user explicitly accepts?
///
/// Default: **false** (cover stays in place). The user must
/// accept the sender's whitelist invitation for the scope before
/// any of their messages decrypt — see §7.3.
///
/// Lookup: `peer_map[sender].incoming_decrypt_accepted[scope.storage_key()]`.
/// Returns the stored value if present, else false.
pub fn should_decrypt_from(peer_map: &PeerMap, scope: &Scope, sender_discord_id: &str) -> bool {
    peer_map
        .get(sender_discord_id)
        .and_then(|e| e.incoming_decrypt_accepted.get(&scope.storage_key()))
        .copied()
        .unwrap_or(false)
}

// ---- helpers ----

/// Is `recipient` named in a [`BurnedScope`] entry that matches
/// `scope`? See [`BurnedScope`] variants for the per-kind match.
fn is_burned_in_scope(peer_map: &PeerMap, scope: &Scope, recipient_discord_id: &str) -> bool {
    let Some(entry) = peer_map.get(recipient_discord_id) else {
        return false;
    };
    entry.burned_scopes.iter().any(|b| burned_matches(b, scope))
}

fn burned_matches(b: &BurnedScope, s: &Scope) -> bool {
    match (b, &s.kind) {
        (BurnedScope::Dm { .. }, ScopeKind::Dm) => true,
        (BurnedScope::Gc { id, .. }, ScopeKind::Gc) => id == &s.id,
        (
            BurnedScope::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            ScopeKind::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (BurnedScope::ServerFull { server_id, .. }, ScopeKind::ServerFull) => {
            Some(server_id) == s.server_id.as_ref()
        }
        _ => false,
    }
}

/// Explicit grant: `scope` has a `WhitelistState` entry with
/// `encrypt_toggle` on, and `recipient` is in the appropriate
/// list (full-whitelist members for full scope, or
/// `whitelisted_users` for per-user scope). DMs are a special
/// case — the scope id IS the recipient's discord_id, so the
/// recipient match is implicit.
fn scope_explicit_grant(
    whitelist_state: &WhitelistState,
    scope: &Scope,
    recipient_discord_id: &str,
) -> bool {
    let Some(state) = whitelist_state.get(&scope.storage_key()) else {
        return false;
    };
    if !state.encrypt_toggle {
        return false;
    }
    match scope.kind {
        ScopeKind::Dm => recipient_discord_id == scope.id,
        ScopeKind::Gc | ScopeKind::ServerChannel | ScopeKind::ServerFull => {
            if state.full_whitelist {
                state.members.iter().any(|m| m == recipient_discord_id)
            } else {
                state
                    .whitelisted_users
                    .iter()
                    .any(|u| u == recipient_discord_id)
            }
        }
    }
}

/// Does `recipient` carry a [`WhitelistEntry::Dm`] with
/// `broadened = true`? Used by the cross-scope grant path.
fn has_broadened_dm_access(peer_map: &PeerMap, recipient_discord_id: &str) -> bool {
    let Some(entry) = peer_map.get(recipient_discord_id) else {
        return false;
    };
    entry
        .outgoing_whitelists
        .iter()
        .any(|w| matches!(w, WhitelistEntry::Dm { broadened, .. } if *broadened))
}

/// Decode the on-disk base64 X25519 pubkey for a peer. None for
/// unknown peers and for entries whose `pubkey` field is unset or
/// malformed.
fn peer_pubkey(peer_map: &PeerMap, discord_id: &str) -> Option<x25519::PublicKey> {
    let entry: &PeerEntry = peer_map.get(discord_id)?;
    let b64 = entry.pubkey.as_deref()?;
    let bytes = STANDARD.decode(b64).ok()?;
    if bytes.len() != x25519::PUBLIC_KEY_SIZE {
        return None;
    }
    let mut arr = [0u8; x25519::PUBLIC_KEY_SIZE];
    arr.copy_from_slice(&bytes);
    Some(x25519::PublicKey::from_bytes(arr))
}
