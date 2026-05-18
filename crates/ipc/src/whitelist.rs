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
//!
//! 9-C1 removed the `should_decrypt_from` per-scope accept gate.
//! Decrypt is permissive — Discord's block feature is the trust
//! boundary.
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
use crate::wire_v2::RecipientV3;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ml_kem_768, x25519};

/// Can outgoing messages to `recipient_discord_id` in `scope` be
/// encrypted?
///
/// 9-C1 logic (per-peer source of truth):
///
/// 1. If `recipient` is burned in `scope` → false. Burns override.
/// 2. If `peer_map[recipient].outgoing_whitelists` has a matching
///    entry for `scope` → true.
/// 3. If `scope.kind != Dm` and `recipient` has a DM whitelist
///    with `broadened = true` → true. Cross-scope grant.
/// 4. Otherwise false.
pub fn can_encrypt_to(peer_map: &PeerMap, scope: &Scope, recipient_discord_id: &str) -> bool {
    if is_burned_in_scope(peer_map, scope, recipient_discord_id) {
        return false;
    }
    if has_explicit_whitelist_entry(peer_map, scope, recipient_discord_id) {
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
        if !can_encrypt_to(peer_map, scope, member) {
            continue;
        }
        if let Some(pk) = peer_pubkey(peer_map, member) {
            out.push(pk);
        }
    }
    out
}

/// Phase 9-A1: PQ-hybrid recipient resolution for v=3 sends.
///
/// Same gate as [`recipients_for_scope`] (whitelist + per-member
/// `can_encrypt_to`), but every member of the output **must** carry
/// both an X25519 ik pubkey AND an ML-KEM-768 encap key in
/// `peer_map`. If any whitelisted member lacks ML-KEM, the function
/// returns [`RecipientsV3Error::PeerMissingMlkemPubkey`] with that
/// peer's `discord_id` — no automatic v=2 fallback (locked policy).
///
/// Self is always the first entry (we have both keys on our own
/// Identity). The caller supplies them so this fn stays free of a
/// keystore dep.
pub fn recipients_for_scope_v3(
    peer_map: &PeerMap,
    scope: &Scope,
    channel_members: &[String],
    self_discord_id: &str,
    self_x25519_pub: &x25519::PublicKey,
    self_mlkem_pub: &ml_kem_768::EncapsulationKey,
) -> Result<Vec<RecipientV3>, RecipientsV3Error> {
    let mut out: Vec<RecipientV3> = Vec::new();
    out.push(RecipientV3 {
        x25519_pub: *self_x25519_pub,
        mlkem_pub: self_mlkem_pub.clone(),
    });
    // GC Step 1 (decision (a)): a group DM encrypts only to the
    // members who are OSL/keyserver-resolvable; non-OSL members
    // seeing raw DPC0:: is acceptable. So for `gc:` scopes a
    // whitelisted member that lacks usable keys is SKIPPED, not a
    // hard error. DM and server-channel scopes keep the strict
    // fail-closed behavior (a single keyless recipient there is a
    // surfaced error so the caller can keyserver-refresh + retry).
    let gc_best_effort = matches!(scope.kind, ScopeKind::Gc);
    for member in channel_members {
        if member == self_discord_id {
            continue;
        }
        if !can_encrypt_to(peer_map, scope, member) {
            continue;
        }
        let entry = match peer_map.get(member) {
            Some(e) => e,
            None => continue,
        };
        // REGISTER-FIX (cross-machine decrypt): a whitelisted peer
        // with no X25519 pubkey on file used to be SILENTLY skipped
        // (`continue`), which made the whole send collapse to
        // encrypt-to-self-only (the peer was never a recipient slot)
        // — the message then arrived as raw ciphertext on the peer.
        // That is now a recoverable, surfaced error so the caller
        // (`cmd_osl_encrypt_message_v2_wire`) can do an inline
        // keyserver fetch for this peer and retry — option 4(a),
        // "it just works". NEVER silently drop a whitelisted peer.
        let x25519_pub = match peer_pubkey(peer_map, member) {
            Some(pk) => pk,
            None => {
                if gc_best_effort {
                    continue;
                }
                return Err(RecipientsV3Error::PeerMissingKeys {
                    discord_id: member.clone(),
                });
            }
        };
        // ML-KEM is REQUIRED for v=3 recipients; missing is a
        // hard fail surfaced to the caller (not a silent skip)
        // so the user is told to ask the peer to regen — EXCEPT
        // for gc: scopes (decision (a): skip non-OSL members).
        let mlkem_b64 = match entry.ik_mlkem768_pub.as_ref() {
            Some(b) => b,
            None => {
                if gc_best_effort {
                    continue;
                }
                return Err(RecipientsV3Error::PeerMissingMlkemPubkey {
                    discord_id: member.clone(),
                });
            }
        };
        let mlkem_bytes = match STANDARD.decode(mlkem_b64) {
            Ok(b) => b,
            Err(e) => {
                if gc_best_effort {
                    continue;
                }
                return Err(RecipientsV3Error::PeerMlkemPubkeyDecode {
                    discord_id: member.clone(),
                    reason: e.to_string(),
                });
            }
        };
        if mlkem_bytes.len() != ml_kem_768::ENCAPSULATION_KEY_SIZE {
            if gc_best_effort {
                continue;
            }
            return Err(RecipientsV3Error::PeerMlkemPubkeyDecode {
                discord_id: member.clone(),
                reason: format!(
                    "expected {} bytes, got {}",
                    ml_kem_768::ENCAPSULATION_KEY_SIZE,
                    mlkem_bytes.len()
                ),
            });
        }
        let mut mlkem_arr = [0u8; ml_kem_768::ENCAPSULATION_KEY_SIZE];
        mlkem_arr.copy_from_slice(&mlkem_bytes);
        let mlkem_pub = ml_kem_768::EncapsulationKey::from_bytes(&mlkem_arr);
        out.push(RecipientV3 {
            x25519_pub,
            mlkem_pub,
        });
    }
    Ok(out)
}

/// Phase 9-A1: v=3 capability-check failures. Surfaced to the user
/// with the offending peer's discord_id so they know who needs to
/// regenerate identity.
#[derive(Debug, thiserror::Error)]
pub enum RecipientsV3Error {
    #[error(
        "peer {discord_id} has no ML-KEM-768 pubkey on file; ask them \
         to regenerate identity (no v=2 fallback in production)"
    )]
    PeerMissingMlkemPubkey { discord_id: String },
    #[error("peer {discord_id} ML-KEM pubkey decode failed: {reason}")]
    PeerMlkemPubkeyDecode { discord_id: String, reason: String },
    /// REGISTER-FIX: whitelisted peer has no X25519 pubkey on file
    /// yet (e.g. peer_map was wiped + re-whitelisted; keys never
    /// fetched). Recoverable — the caller does an inline keyserver
    /// fetch keyed by the peer's Discord snowflake and retries.
    #[error("peer {discord_id} has no keys on file yet (will fetch from keyserver)")]
    PeerMissingKeys { discord_id: String },
}

// 9-C1: `should_decrypt_from` removed. Decrypt is permissive — if
// we have keys, we surface plaintext. Discord's native block feature
// is the user-facing trust boundary.

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

/// 9-C1: does `peer_map[recipient].outgoing_whitelists` carry a
/// [`WhitelistEntry`] matching `scope`? This replaces the prior
/// `scope_explicit_grant` which consulted the per-scope membership
/// tables in `WhitelistState`. Membership now lives only on
/// `PeerEntry`.
fn has_explicit_whitelist_entry(
    peer_map: &PeerMap,
    scope: &Scope,
    recipient_discord_id: &str,
) -> bool {
    let Some(entry) = peer_map.get(recipient_discord_id) else {
        return false;
    };
    entry
        .outgoing_whitelists
        .iter()
        .any(|w| whitelist_entry_matches_scope(w, scope))
}

fn whitelist_entry_matches_scope(w: &WhitelistEntry, s: &Scope) -> bool {
    match (w, &s.kind) {
        (WhitelistEntry::Dm { .. }, ScopeKind::Dm) => {
            // The DM whitelist is on the *peer's* entry, which
            // already keys by recipient_discord_id. So if a Dm
            // variant is present in the peer's outgoing list, it
            // applies to a DM scope with that peer.
            true
        }
        (WhitelistEntry::Gc { id, .. }, ScopeKind::Gc) => id == &s.id,
        (
            WhitelistEntry::ServerChannel {
                server_id,
                channel_id,
                ..
            },
            ScopeKind::ServerChannel,
        ) => Some(server_id) == s.server_id.as_ref() && Some(channel_id) == s.channel_id.as_ref(),
        (WhitelistEntry::ServerFull { server_id, .. }, ScopeKind::ServerFull) => {
            Some(server_id) == s.server_id.as_ref()
        }
        _ => false,
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
