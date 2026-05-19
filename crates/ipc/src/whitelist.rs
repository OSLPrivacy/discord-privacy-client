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

use crate::membership::ScopeMembership;
use crate::peer_map::{BurnedScope, PeerEntry, PeerMap, WhitelistEntry};
use crate::scope::{Scope, ScopeKind};
use crate::whitelist_state::{ServerDefaults, WhitelistState};
use crate::wire_v2::RecipientV3;
use std::collections::HashMap;
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

/// W1 (Option B): read-only inputs the dynamic precedence resolver
/// needs beyond `peer_map`. Borrowed so the caller holds the locks.
pub struct ScopeAuthCtx<'a> {
    /// Per-scope `ScopeState` map (carries `channel_whitelisted`).
    pub whitelist_state: &'a WhitelistState,
    /// Per-server defaults (carries `server_header_whitelisted`).
    pub server_defaults: &'a HashMap<String, ServerDefaults>,
    /// The membership oracle (who is an OSL member of what).
    pub membership: &'a ScopeMembership,
}

/// W1 canonical authorization: should an outgoing message in `scope`
/// be encrypted to `recipient`? Implements the locked precedence
///
///   DM-whitelist  >  server-header  >  per-channel
///
/// with the server header REPLACING per-channel and the DM bleed
/// member-gated (decision #2). Supersedes [`can_encrypt_to`] for the
/// recipient walk: it still honors burns + the existing per-peer
/// explicit `outgoing_whitelists` entries (so shipped DM/GC behavior
/// is unchanged), then adds the scope-flag + membership rules.
///
/// Precedence, top wins:
/// 1. Burned in scope → never (burns override everything).
/// 2. Explicit per-peer `outgoing_whitelists` entry for this scope →
///    yes (covers DM whitelist, GC "whitelist-all", legacy grants).
/// 3. ServerChannel / ServerFull: if the server header is on and the
///    peer is an OSL member of the server → yes. Else (header off)
///    if the channel flag is on and the peer is an OSL member of the
///    channel → yes. Header-on makes the channel flag inert.
/// 4. DM bleed: a peer with a broadened DM whitelist may read any
///    NON-DM scope they are actually a member of (decision #2).
/// 5. Otherwise no.
pub fn should_encrypt_to(
    peer_map: &PeerMap,
    ctx: &ScopeAuthCtx,
    scope: &Scope,
    recipient_discord_id: &str,
) -> bool {
    // 1. Burns override all.
    if is_burned_in_scope(peer_map, scope, recipient_discord_id) {
        return false;
    }
    // 2. Existing per-peer explicit grant (DM/GC/legacy). Preserves
    //    all shipped non-server behavior byte-for-byte.
    if has_explicit_whitelist_entry(peer_map, scope, recipient_discord_id) {
        return true;
    }
    // 3. Scope-level server flags + dynamic membership.
    match scope.kind {
        ScopeKind::ServerChannel => {
            if let (Some(srv), Some(chan)) = (&scope.server_id, &scope.channel_id) {
                let header_on = ctx
                    .server_defaults
                    .get(srv)
                    .map(|d| d.server_header_whitelisted)
                    .unwrap_or(false);
                if header_on {
                    if ctx.membership.is_server_member(srv, recipient_discord_id) {
                        return true;
                    }
                } else {
                    let chan_on = ctx
                        .whitelist_state
                        .get(&scope.storage_key())
                        .map(|s| s.channel_whitelisted)
                        .unwrap_or(false);
                    if chan_on
                        && ctx
                            .membership
                            .is_channel_member(srv, chan, recipient_discord_id)
                    {
                        return true;
                    }
                }
            }
        }
        ScopeKind::ServerFull => {
            if let Some(srv) = &scope.server_id {
                let header_on = ctx
                    .server_defaults
                    .get(srv)
                    .map(|d| d.server_header_whitelisted)
                    .unwrap_or(false);
                if header_on && ctx.membership.is_server_member(srv, recipient_discord_id) {
                    return true;
                }
            }
        }
        ScopeKind::Dm | ScopeKind::Gc => {}
    }
    // 4. DM bleed — broadened DM peer reads any NON-DM scope they are
    //    actually a member of (decision #2: member-gated).
    if scope.kind != ScopeKind::Dm
        && has_broadened_dm_access(peer_map, recipient_discord_id)
        && ctx
            .membership
            .is_member_of_scope(scope, recipient_discord_id)
    {
        return true;
    }
    // 5. Default deny.
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
    ctx: &ScopeAuthCtx,
    scope: &Scope,
    channel_members: &[String],
    self_discord_id: &str,
    self_x25519_pub: &x25519::PublicKey,
    self_mlkem_pub: &ml_kem_768::EncapsulationKey,
) -> Result<Vec<(String, RecipientV3)>, RecipientsV3Error> {
    // Each entry is (discord_id, keys). Pairing the id WITH the keys
    // here — atomically, at resolution time — is what lets the v=5
    // SKDM loop address each SKDM to the right peer. The previous
    // keys-only Vec forced the SKDM loop to reconstruct ids by
    // positional indexing into the UNFILTERED roster, which
    // misaligned the moment any non-OSL member was filtered out.
    let mut out: Vec<(String, RecipientV3)> = Vec::new();
    out.push((
        self_discord_id.to_string(),
        RecipientV3 {
            x25519_pub: *self_x25519_pub,
            mlkem_pub: self_mlkem_pub.clone(),
        },
    ));
    // GC Step 1 (decision (a)): a group DM encrypts only to the
    // members who are OSL/keyserver-resolvable; non-OSL members
    // seeing raw DPC0:: is acceptable. So for `gc:` scopes a
    // whitelisted member that lacks usable keys is SKIPPED, not a
    // hard error. Bug 3 (c): ServerChannel now shares this
    // best-effort behavior — server-channel sends are
    // roster-independent and may include non-OSL members, who see
    // DPC0::. DM stays strict fail-closed (a single keyless
    // recipient is a surfaced error so the caller can
    // keyserver-refresh + retry); ServerFull untouched.
    let gc_best_effort =
        matches!(scope.kind, ScopeKind::Gc | ScopeKind::ServerChannel);

    // W1 (Option B): server-channel recipients are roster-independent
    // — Discord never hands us a server member list. The candidate
    // set is derived from the dynamic membership oracle: the whole
    // server's observed members when the server header is on, else
    // this channel's observed members. Unioned with any peer carrying
    // a per-peer explicit entry or a broadened DM (so legacy grants +
    // the DM bleed are still considered), then `should_encrypt_to`
    // applies the full locked precedence per candidate. Gc / Dm /
    // ServerFull keep the `channel_members` walk; only the authority
    // gate changes (can_encrypt_to → should_encrypt_to) so the DM
    // bleed is member-gated per decision #2 and server flags apply.
    let server_channel_members: Vec<String> = if scope.kind == ScopeKind::ServerChannel {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        if let (Some(srv), Some(chan)) = (&scope.server_id, &scope.channel_id) {
            let header_on = ctx
                .server_defaults
                .get(srv)
                .map(|d| d.server_header_whitelisted)
                .unwrap_or(false);
            for did in ctx
                .membership
                .server_channel_candidates(srv, chan, header_on)
            {
                set.insert(did);
            }
        }
        // Fold in peers with a legacy explicit entry or a broadened
        // DM so neither path is lost if membership hasn't observed
        // them yet; should_encrypt_to still gates each one.
        for (did, entry) in peer_map.iter() {
            if did.as_str() == self_discord_id || entry.is_self == Some(true) {
                continue;
            }
            if has_explicit_whitelist_entry(peer_map, scope, did)
                || has_broadened_dm_access(peer_map, did)
            {
                set.insert(did.clone());
            }
        }
        set.into_iter().collect()
    } else {
        Vec::new()
    };
    let members: &[String] = if scope.kind == ScopeKind::ServerChannel {
        &server_channel_members
    } else {
        channel_members
    };
    for member in members {
        if member == self_discord_id {
            continue;
        }
        if !should_encrypt_to(peer_map, ctx, scope, member) {
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
        out.push((
            member.clone(),
            RecipientV3 {
                x25519_pub,
                mlkem_pub,
            },
        ));
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

#[cfg(test)]
mod should_encrypt_to_tests {
    use super::*;
    use crate::membership::ScopeMembership;
    use crate::peer_map::{BurnedScope, PeerEntry, WhitelistEntry};
    use crate::whitelist_state::{ScopeState, ServerDefaults};

    const SRV: &str = "900000000000000001";
    const CH: &str = "900000000000000010";
    const CH2: &str = "900000000000000011";
    const GC: &str = "900000000000000099";
    const PEER: &str = "111111111111111111";

    fn peer(wl: Vec<WhitelistEntry>, burns: Vec<BurnedScope>) -> PeerEntry {
        PeerEntry {
            outgoing_whitelists: wl,
            burned_scopes: burns,
            ..PeerEntry::default()
        }
    }

    fn pm(entry: PeerEntry) -> PeerMap {
        let mut m = PeerMap::new();
        m.insert(PEER.to_string(), entry);
        m
    }

    /// Build a ctx; `header` toggles server_header_whitelisted for
    /// SRV, `chan` toggles channel_whitelisted for server_channel
    /// SRV:CH, `members` are noted as channel members of SRV:CH.
    struct Built {
        ws: WhitelistState,
        sd: HashMap<String, ServerDefaults>,
        mem: ScopeMembership,
    }
    fn build(header: bool, chan: bool, members: &[&str]) -> Built {
        let mut ws = WhitelistState::new();
        if chan {
            ws.insert(
                format!("server_channel:{SRV}:{CH}"),
                ScopeState {
                    channel_whitelisted: true,
                    ..ScopeState::default()
                },
            );
        }
        let mut sd = HashMap::new();
        sd.insert(
            SRV.to_string(),
            ServerDefaults {
                server_header_whitelisted: header,
                ..ServerDefaults::default()
            },
        );
        let mut mem = ScopeMembership::new();
        for m in members {
            mem.note_server_channel_member(SRV, CH, m);
        }
        Built { ws, sd, mem }
    }
    fn ctx<'a>(b: &'a Built) -> ScopeAuthCtx<'a> {
        ScopeAuthCtx {
            whitelist_state: &b.ws,
            server_defaults: &b.sd,
            membership: &b.mem,
        }
    }

    #[test]
    fn burn_overrides_everything() {
        // Even with an explicit DM whitelist + broadened, a burn in
        // the scope denies.
        let p = pm(peer(
            vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: None,
            }],
            vec![BurnedScope::Gc {
                id: GC.to_string(),
                burned_at: "x".into(),
            }],
        ));
        let b = build(false, false, &[PEER]);
        assert!(!should_encrypt_to(&p, &ctx(&b), &Scope::gc(GC), PEER));
    }

    #[test]
    fn explicit_dm_entry_grants_dm_scope() {
        let p = pm(peer(
            vec![WhitelistEntry::Dm {
                broadened: false,
                enabled_at: None,
            }],
            vec![],
        ));
        let b = build(false, false, &[]);
        assert!(should_encrypt_to(&p, &ctx(&b), &Scope::dm(PEER), PEER));
    }

    #[test]
    fn explicit_gc_entry_grants_gc_scope() {
        let p = pm(peer(
            vec![WhitelistEntry::Gc {
                id: GC.to_string(),
                user_specific: false,
            }],
            vec![],
        ));
        let b = build(false, false, &[]);
        assert!(should_encrypt_to(&p, &ctx(&b), &Scope::gc(GC), PEER));
    }

    #[test]
    fn server_header_on_grants_server_member_replaces_channel() {
        // Header ON, channel flag OFF, peer is a server member →
        // granted (header replaces channel; channel flag irrelevant).
        let p = pm(peer(vec![], vec![]));
        let b = build(true, false, &[PEER]);
        assert!(should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn server_header_on_but_not_a_member_denied() {
        let p = pm(peer(vec![], vec![]));
        let b = build(true, false, &[]); // PEER not noted as member
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn header_off_channel_on_member_granted() {
        let p = pm(peer(vec![], vec![]));
        let b = build(false, true, &[PEER]);
        assert!(should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn header_off_channel_off_denied_even_if_member() {
        let p = pm(peer(vec![], vec![]));
        let b = build(false, false, &[PEER]);
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn header_off_channel_on_but_not_channel_member_denied() {
        let p = pm(peer(vec![], vec![]));
        // Member of CH2, channel-whitelist is on CH → not a member of
        // the whitelisted channel.
        let mut b = build(false, true, &[]);
        b.mem.note_server_channel_member(SRV, CH2, PEER);
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn dm_bleed_member_gated_grants_when_member() {
        // Broadened DM, no server flags; peer IS a channel member →
        // decision #2 bleed grants the server channel.
        let p = pm(peer(
            vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: None,
            }],
            vec![],
        ));
        let b = build(false, false, &[PEER]);
        assert!(should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn dm_bleed_denied_when_not_a_member() {
        // Broadened DM but NOT a member of this server/channel →
        // decision #2 denies (member-gated, unlike old can_encrypt_to).
        let p = pm(peer(
            vec![WhitelistEntry::Dm {
                broadened: true,
                enabled_at: None,
            }],
            vec![],
        ));
        let b = build(false, false, &[]); // not noted as member
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn server_full_header_on_member_granted() {
        let p = pm(peer(vec![], vec![]));
        let b = build(true, false, &[PEER]); // server roll-up has PEER
        assert!(should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_full(SRV),
            PEER
        ));
    }

    #[test]
    fn nothing_grants_nothing() {
        let p = pm(peer(vec![], vec![]));
        let b = build(false, false, &[PEER]);
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
        assert!(!should_encrypt_to(&p, &ctx(&b), &Scope::gc(GC), PEER));
        assert!(!should_encrypt_to(&p, &ctx(&b), &Scope::dm(PEER), PEER));
    }

    #[test]
    fn authorization_is_independent_of_peer_map_presence() {
        // should_encrypt_to is AUTHORIZATION only; key availability is
        // enforced later in recipients_for_scope_v3 (keyless peers are
        // skipped best-effort). A server member under header-on is
        // authorized even with no peer_map entry yet — they simply
        // won't be keyed until their keys resolve.
        let p = PeerMap::new();
        let b = build(true, false, &[PEER]); // PEER is a server member
        assert!(should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }

    #[test]
    fn truly_unknown_peer_denied() {
        // No peer_map entry, no membership, no flags → genuine deny.
        let p = PeerMap::new();
        let b = build(true, true, &[]); // PEER not a member of anything
        assert!(!should_encrypt_to(
            &p,
            &ctx(&b),
            &Scope::server_channel(SRV, CH),
            PEER
        ));
    }
}
