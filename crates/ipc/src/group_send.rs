//! Sender-key group send and receive path.
//!
//! Kept separate from the command surface so subsequent group work has an
//! exclusive module without widening the IPC API.

use crate::commands::{
    now_unix_secs, persist_sender_key_state_now, resolve_pinned_sender_pubkey, EncryptWire,
    SkdmPeerStatus, OSL_RESULT_SKDM_APPLIED, OSL_RESULT_SKDM_REREQUEST_PREFIX,
};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Phase 9-A3: 24-hour rotation timer threshold.
const SENDER_KEY_ROTATE_AFTER_SECS: u64 = 24 * 60 * 60;

/// Phase 6.2: how often (in seconds) we re-emit the SKDM bundle for
/// self-heal purposes when the chain hasn't otherwise changed.
/// Pre-6.2 behaviour was "emit on every send", which produced N extra
/// Discord messages for N user sends in an active GC. Now the bundle
/// is emitted on install, on rotate, OR when this many seconds have
/// elapsed since the last emit. Tradeoff: receivers who miss every
/// SKDM in the interval window will have to wait or trigger the
/// SKDM_REQUEST recovery path; in exchange, active conversations
/// shed ~95% of the noise messages.
const SKDM_PERIODIC_EMIT_INTERVAL_SECS: u64 = 5 * 60;

/// Phase 9-A3: decide whether the sender-keys chain for this scope
/// needs to rotate before the next send. Returns `true` when:
/// - the time since `chain_started_at` exceeds 24 hours, OR
/// - the current channel-member set differs from
///   `last_known_members` (any join/leave).
fn sender_key_needs_rotation(
    sender: &crypto::sender_keys::SenderChain,
    current_members: &[String],
    now: u64,
) -> bool {
    if now.saturating_sub(sender.chain_started_at()) >= SENDER_KEY_ROTATE_AFTER_SECS {
        return true;
    }
    let stored: std::collections::BTreeSet<&[u8]> = sender
        .last_known_members()
        .iter()
        .map(|m| m.as_slice())
        .collect();
    let live: std::collections::BTreeSet<&[u8]> =
        current_members.iter().map(|m| m.as_bytes()).collect();
    stored != live
}

/// Phase 9-A3: v=5 group send. On-demand install/rotate of the
/// outbound `SenderChain`, SKDM dispatch to each non-self peer via
/// v=4, then encode the actual message under v=5.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encrypt_v5_send(
    state: &AppState,
    sender_sk: &crypto::x25519::SecretKey,
    self_pk: &crypto::x25519::PublicKey,
    scope: &crate::scope::Scope,
    self_discord_id: &str,
    // Retained in the signature for call-site symmetry with the v=3
    // path; v=5 derives membership from the resolved recipient set
    // (`non_self_peers`), not the raw roster.
    _channel_members: &[String],
    non_self_peers: &[&(String, crate::wire_v2::RecipientV3)],
    plaintext: &[u8],
) -> Result<EncryptWire, String> {
    use crypto::sender_keys::{SenderContext, SenderKeyState, SenderKeyStateOnDisk};

    let scope_key = scope.storage_key();
    let self_mlkem_pub_bytes: Vec<u8> = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        identity.mlkem_public_bytes.to_vec()
    };
    let now: u64 = now_unix_secs().max(0) as u64;

    // Membership tracking for the sender key is derived from the
    // actual recipient set below (`recipient_ids`, from
    // `non_self_peers`), not the channel roster — see the rotation
    // block. `channel_members` / the gateway snapshot are no longer
    // consulted here: the caller already resolved who is a granted
    // recipient (whitelist + lock tier) into `non_self_peers`, and the
    // SKDM only ever goes to that set.

    // Load (or initialize) the per-scope SenderKeyState. We work on
    // a clone to keep the lock window short; persist back after.
    let mut sks: SenderKeyState = {
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        match g.states.get(&scope_key) {
            Some(disk) => disk
                .clone()
                .try_into()
                .map_err(|e| format!("OSL: v=5 send: load sender_key_state: {e}"))?,
            None => SenderKeyState::new(),
        }
    };

    // Rotation/redistribution must track the ACTUAL recipient set —
    // the granted peers who receive the SKDM (`non_self_peers`) — NOT
    // the raw channel roster (`effective_members`). Tracking the roster
    // was the GC bug: a peer you whitelist AFTER the chain exists is
    // already in the roster, so the roster doesn't change, no rotation
    // fires, and they never get the sender key (until the 5-min
    // periodic re-emit) → permanent "not a recipient" in the meantime.
    // Tracking the recipient set means whitelisting a peer changes the
    // set → rotate → SKDM re-distributed to everyone on the very next
    // send.
    let recipient_ids: Vec<String> = non_self_peers.iter().map(|p| p.0.clone()).collect();

    // Decide install / rotate / continue.
    let needs_install = sks.sender_chain().is_none();
    let needs_rotate = sks
        .sender_chain()
        .map(|c| sender_key_needs_rotation(c, &recipient_ids, now))
        .unwrap_or(false);

    let send_skdm = needs_install || needs_rotate;
    if needs_install {
        sks.install_sender()
            .map_err(|e| format!("OSL: v=5 send: install_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = recipient_ids
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    } else if needs_rotate {
        sks.rotate_sender()
            .map_err(|e| format!("OSL: v=5 send: rotate_sender: {e}"))?;
        let members_bytes: Vec<Vec<u8>> = recipient_ids
            .iter()
            .map(|m| m.as_bytes().to_vec())
            .collect();
        sks.sender_chain_mut()
            .unwrap()
            .set_last_known_members(members_bytes);
    }

    let (chain_id, rotation_root, physical_device_id) = {
        let s = sks
            .sender_chain()
            .ok_or_else(|| "OSL: v=5 send: missing sender chain after install".to_string())?;
        (
            s.current_chain_id(),
            s.rotation_root_bytes(),
            s.physical_device_id(),
        )
    };

    // Self-loopback: install/rotate a self-receiver chain seeded
    // from the same rotation_root so self-decrypts work uniformly.
    if send_skdm {
        let self_bytes = self_discord_id.as_bytes().to_vec();
        if sks
            .receiver_chain_for_physical_device(&self_bytes, physical_device_id)
            .is_some()
        {
            sks.rotate_receiver(&self_bytes, chain_id, &rotation_root, physical_device_id)
                .map_err(|e| format!("OSL: v=5 send: rotate_receiver(self): {e}"))?;
        } else {
            sks.install_receiver(self_bytes, chain_id, &rotation_root, physical_device_id)
                .map_err(|e| format!("OSL: v=5 send: install_receiver(self): {e}"))?;
        }
    }

    // Encrypt the actual message under sender-keys.
    let ctx = SenderContext {
        sender_ik_x25519_pub: *self_pk,
        sender_ik_mlkem_pub: self_mlkem_pub_bytes.clone(),
        group_id: scope_key.clone().into_bytes(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };
    let em = sks
        .encrypt(plaintext, &ctx)
        .map_err(|e| format!("OSL: v=5 send: sender_keys::encrypt: {e}"))?;
    let wire = crate::wire_v2::encrypt_v5(self_pk, crate::wire_v2::MSG_TYPE_CONTENT, 0, &em)
        .map_err(|e| format!("OSL: v=5 send: encrypt_v5: {e}"))?;

    // Persist updated state before any SKDM dispatch — if the SKDM
    // wire goes out but persistence dies between send + crash, we'd
    // be in a position where peers think we've installed and we
    // don't.
    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        g.states
            .insert(scope_key.clone(), SenderKeyStateOnDisk::from(&sks));
        g.version = 1;
    }
    persist_sender_key_state_now(state);

    // Probe-3 Option-2 step 1: dispatch SKDMs as a SINGLE v=3-bundled
    // PQ-hybrid multi-recipient message instead of N separate v=4
    // ratcheted messages.
    //
    // Old design (v=4-per-peer): each SKDM was wrapped via
    // `send_skdm_via_v4` which built a PQXDH bootstrap + advanced the
    // per-peer Double Ratchet. That made SKDM delivery depend on
    // healthy v=4 ratchet state — after any burn / identity rotation
    // / out-of-order key the v=4 ratchet desynced, SKDMs failed to
    // decrypt on the recipient, no receiver chain installed, every
    // v=5 GC message returned "not a recipient", and recovery looped.
    //
    // New design: ONE v=3 wire carrying MSG_TYPE_SENDER_KEY_DISTRIBUTION
    // bundled to all non-self peers via the existing PQ-hybrid
    // multi-recipient wrap. No per-peer ratchet state involved.
    // boot.js posts it as one message; each recipient's v=3 decrypt
    // resolves their slot, the v=2/v=3 dispatcher routes
    // MSG_TYPE_SENDER_KEY_DISTRIBUTION to `apply_skdm_recv` which
    // installs the receiver chain exactly as before.
    // Probe-3 Option-2 step 1 follow-up: previously the bundle emit
    // was gated on `send_skdm` (i.e. ONLY on first install / on
    // rotation). That left the failure mode: if any receiver missed
    // the very first SKDM (offline, app not yet running, broken v=4
    // ratchet in the old design), every subsequent v=5 send produced
    // NO bundle and the receiver stayed permanently locked out
    // (`not a recipient` forever, no in-band recovery signal).
    // Now: always emit on every send while there are peers. The cost
    // is one extra ~2KB v=3 message per v=5 send; the win is that
    // receivers self-heal from any v=5 message they observe, without
    // needing the SKDM_REQUEST/recovery round-trip. apply_skdm_recv
    // is idempotent (existing receiver chain → rotate_receiver to
    // the same chain_id is a no-op; absent → install_receiver).
    let mut skdm_wires: Vec<String> = Vec::new();
    let mut skdm_peer_status: Vec<SkdmPeerStatus> = Vec::new();
    let _ = send_skdm; // retained for self-receiver gate above; no longer gates bundle emit
                       // Phase 6.2: gate the periodic self-heal emit. Always emit on
                       // install/rotate (the chain is fresh, every receiver needs it).
                       // Otherwise emit only if SKDM_PERIODIC_EMIT_INTERVAL_SECS have
                       // elapsed since the last emit. The 0-default on legacy on-disk
                       // records (pre-6.2 SenderChainOnDisk) forces a one-time emit on
                       // load, which is the right thing -- it primes receivers who
                       // might have missed the prior chain.
    let last_emit_at = sks
        .sender_chain()
        .map(|c| c.last_skdm_emit_at())
        .unwrap_or(0);
    let periodic_due = now.saturating_sub(last_emit_at) >= SKDM_PERIODIC_EMIT_INTERVAL_SECS;
    let should_emit_bundle = needs_install || needs_rotate || periodic_due;
    if !non_self_peers.is_empty() && should_emit_bundle {
        // Include self as a recipient so the sender's own DOM
        // (which sees the SKDM bundle round-tripped through Discord
        // like any other channel message) decodes it cleanly instead
        // of raising "not a recipient" for every own send. The
        // self-slot decode hits apply_skdm_recv which is idempotent
        // for an already-installed receiver chain.
        let self_mlkem = {
            let id_guard = state.identity.lock().expect("identity mutex poisoned");
            id_guard
                .as_ref()
                .ok_or_else(|| "OSL: identity not loaded".to_string())?
                .mlkem_encapsulation_key()
        };
        let mut recipients_v3: Vec<crate::wire_v2::RecipientV3> =
            Vec::with_capacity(non_self_peers.len() + 1);
        recipients_v3.push(crate::wire_v2::RecipientV3 {
            x25519_pub: *self_pk,
            mlkem_pub: self_mlkem,
        });
        for (_, r) in non_self_peers.iter() {
            recipients_v3.push(r.clone());
        }
        match send_skdm_via_v3_bundle(
            sender_sk,
            self_pk,
            &recipients_v3,
            &scope_key,
            chain_id,
            &rotation_root,
            physical_device_id.as_bytes(),
        ) {
            Ok(skdm_wire) => {
                skdm_wires.push(skdm_wire);
                // Phase 6.2: stamp the periodic emit clock so the
                // next send within SKDM_PERIODIC_EMIT_INTERVAL_SECS
                // skips bundle emission unless install/rotate
                // independently triggers it.
                if let Some(chain) = sks.sender_chain_mut() {
                    chain.mark_skdm_emitted_at(now);
                }
                for pair in non_self_peers.iter() {
                    skdm_peer_status.push(SkdmPeerStatus {
                        peer_discord_id: pair.0.clone(),
                        ok: true,
                        error: None,
                    });
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    recipients = non_self_peers.len(),
                    "[OSL] v=5 SKDM bundle dispatch failed (best-effort; \
                     recipients will request via SKDM_REQUEST if they get \
                     a v=5 message before they install)"
                );
                for pair in non_self_peers.iter() {
                    skdm_peer_status.push(SkdmPeerStatus {
                        peer_discord_id: pair.0.clone(),
                        ok: false,
                        error: Some(e.clone()),
                    });
                }
            }
        }
    }

    Ok(EncryptWire {
        content: wire,
        control_messages: skdm_wires,
        skdm_peer_status,
    })
}

/// Probe-3 Option-2 step 1: ship a SenderKeyDistribution payload to
/// ALL non-self peers in ONE v=3-bundled PQ-hybrid multi-recipient
/// wire (formerly: N separate v=4 ratcheted wires via the now-removed
/// `send_skdm_via_v4`).
///
/// The body is the same `SenderKeyDistribution{scope_storage_key,
/// chain_id, rotation_root, sent_at}` payload that the recipient-side
/// `apply_skdm_recv` already understands. The transport switches from
/// v=4 (ratcheted, single-peer, broke whenever the v=4 DR was
/// desynced) to v=3 (PQXDH per slot + ML-KEM hybrid, no ratchet
/// state). One wire instead of N; no dependency on per-peer ratchet
/// liveness.
///
/// Receive: the existing v=3 decrypt dispatcher in
/// `cmd_osl_decrypt_message_v2` recovers the `DecryptedV2` and the
/// `match recovered.msg_type` block routes
/// `MSG_TYPE_SENDER_KEY_DISTRIBUTION` to `apply_skdm_recv`. No JS
/// changes; the existing boot.js code already POSTs every
/// `control_messages[]` entry as its own Discord message and
/// suppresses the `OSL_RESULT_SKDM_APPLIED` sentinel.
fn send_skdm_via_v3_bundle(
    sender_sk: &crypto::x25519::SecretKey,
    self_pk: &crypto::x25519::PublicKey,
    recipients: &[crate::wire_v2::RecipientV3],
    scope_storage_key: &str,
    chain_id: u32,
    rotation_root: &[u8; 32],
    physical_device_id: &[u8; 32],
) -> Result<String, String> {
    let payload = crate::control_messages::SenderKeyDistribution {
        scope_storage_key: scope_storage_key.to_string(),
        chain_id,
        rotation_root: *rotation_root,
        physical_device_id: *physical_device_id,
        sent_at: now_unix_secs(),
    };
    let body = crate::control_messages::serialize_sender_key_distribution(&payload)
        .map_err(|e| format!("OSL: v=5 SKDM bundle: serialize: {e}"))?;
    crate::wire_v2::encrypt_v3(
        sender_sk,
        self_pk,
        recipients,
        crate::wire_v2::MSG_TYPE_SENDER_KEY_DISTRIBUTION,
        &body,
    )
    .map_err(|e| format!("OSL: v=5 SKDM bundle: encrypt_v3: {e}"))
}

/// Phase 9-A2: symmetric DM conversation_id for the DR session
/// context. Each side derives the same string by sorting the two
/// discord_ids — without this, alice's `Scope::dm(bob).storage_key()
/// = "dm:bob"` and bob's `Scope::dm(alice).storage_key() = "dm:alice"`
/// would mismatch on the DR's canonical AD.

pub const OSL_RESULT_RECOVERY_IGNORED: &str = "__OSL_CONTROL_RECOVERY_IGNORED__";

/// Auto-recovery inbound handler for `MSG_TYPE_SKDM_REQUEST` (0x06):
/// a peer says it never received our sender-key for `scope`. If we
/// genuinely have a sender chain for that scope and the request
/// passes the throttle/replay/staleness guards, re-emit ONE SKDM
/// (v=4-wrapped) addressed to that requester only, bypassing the
/// normal "already installed" short-circuit. Returns the SKDM wire
/// behind [`OSL_RESULT_SKDM_REREQUEST_PREFIX`] for boot.js to POST,
/// or [`OSL_RESULT_RECOVERY_IGNORED`] for any no-op path.
pub(crate) fn apply_skdm_request_recv(
    state: &AppState,
    requester_discord_id: &str,
    payload_bytes: &[u8],
) -> Result<String, String> {
    let req = crate::control_messages::deserialize_skdm_request(payload_bytes)
        .map_err(|e| format!("OSL: SKDM_REQUEST: deserialize: {e}"))?;
    let now = now_unix_secs();

    {
        let mut g = state
            .recovery_guard
            .lock()
            .expect("recovery_guard mutex poisoned");
        if !g.accept_inbound(
            requester_discord_id,
            crate::recovery::RecoveryKind::SkdmRequest,
            &req.nonce,
            req.requested_at,
            now,
        ) {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                reason = "recovery_guard_rejected",
                requested_at = req.requested_at,
                now = now,
                "OSL: SKDM_REQUEST IGNORED — guard rejected (replay/staleness/throttle)"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        }
    }

    let scope = match crate::scope::Scope::parse(&req.scope_storage_key) {
        Some(s) => s,
        None => {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                scope_storage_key = %crate::log_id::log_id(&req.scope_storage_key),
                reason = "invalid_scope",
                "OSL: SKDM_REQUEST IGNORED — scope_storage_key didn't parse"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        }
    };
    let scope_key = scope.storage_key();

    // We can only redistribute a chain we actually own. If there is
    // no sender chain for this scope we are not a v=5 sender here —
    // benign no-op (the requester is asking the wrong peer, or the
    // scope was never keyed).
    let (chain_id, rotation_root, physical_device_id) = {
        use crypto::sender_keys::SenderKeyState;
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        let Some(disk) = g.states.get(&scope_key) else {
            tracing::warn!(
                requester = %crate::log_id::log_id(requester_discord_id),
                scope = %crate::log_id::log_id(&scope_key),
                reason = "no_sender_key_state",
                known_scopes = g.states.len(),
                "OSL: SKDM_REQUEST IGNORED — no sender_key_state for scope \
                 (we never sent a v=5 message here; nothing to redistribute)"
            );
            return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
        };
        let sks: SenderKeyState = disk
            .clone()
            .try_into()
            .map_err(|e| format!("OSL: SKDM_REQUEST: load sender_key_state: {e}"))?;
        match sks.sender_chain() {
            Some(c) => (
                c.current_chain_id(),
                c.rotation_root_bytes(),
                c.physical_device_id(),
            ),
            None => {
                tracing::warn!(
                    requester = %crate::log_id::log_id(requester_discord_id),
                    scope = %crate::log_id::log_id(&scope_key),
                    reason = "no_sender_chain",
                    "OSL: SKDM_REQUEST IGNORED — sender_key_state exists \
                     but has no sender_chain (we never bootstrapped one)"
                );
                return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
            }
        }
    };

    // Self identity material for the v=4 wrap.
    let (sender_sk, self_pk, self_mlkem_pub, self_discord_id) = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: SKDM_REQUEST: identity not loaded".to_string())?;
        let self_did = {
            let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm.iter()
                .find_map(|(did, pe)| (pe.is_self == Some(true)).then(|| did.clone()))
        };
        (
            identity.x25519_secret.clone(),
            identity.x25519_public,
            identity.mlkem_encapsulation_key(),
            self_did,
        )
    };
    let Some(self_discord_id) = self_discord_id else {
        return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
    };

    // Resolve the requester's RecipientV3 via the same vetted path
    // the send loop uses (handles key resolution consistently). The
    // channel-member list for resolution is the gateway snapshot for
    // this scope's channel (DM/GC: scope.id; server: scope.channel_id).
    let channel_members: Vec<String> = {
        let cm = state
            .channel_members
            .lock()
            .expect("channel_members mutex poisoned");
        let cache_key = scope.channel_id.clone().unwrap_or_else(|| scope.id.clone());
        cm.get(&cache_key).cloned().unwrap_or_default()
    };
    let recipients = {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        let ws_guard = state
            .whitelist_state
            .lock()
            .expect("whitelist_state mutex poisoned");
        let sd_guard = state
            .server_defaults
            .lock()
            .expect("server_defaults mutex poisoned");
        let mem_guard = state
            .scope_membership
            .lock()
            .expect("scope_membership mutex poisoned");
        let auth_ctx = crate::whitelist::ScopeAuthCtx {
            whitelist_state: &ws_guard,
            server_defaults: &sd_guard,
            membership: &mem_guard,
        };
        crate::whitelist::recipients_for_scope_v3(
            &pm,
            &auth_ctx,
            &scope,
            &channel_members,
            &self_discord_id,
            &self_pk,
            &self_mlkem_pub,
        )
        .map_err(|e| format!("OSL: SKDM_REQUEST: resolve recipients: {e}"))?
    };
    // Probe-3 follow-up: after a relaunch the `channel_members`
    // cache is empty until the gateway feed populates it, and
    // Discord doesn't always re-ship CHANNEL_CREATE/UPDATE for
    // every GC on reconnect. That left both sides stuck in a
    // mutual-IGNORED loop: each one's SKDM_REQUEST arrived at the
    // other, but `recipients_for_scope_v3` returned only `[self]`
    // (no members in the cache), so the requester wasn't found
    // and we IGNORED. Fall back to a direct peer_map lookup when
    // the whitelist resolver doesn't surface the requester. Safe:
    // the requester already proved themselves by getting their
    // v=2-wrapped request through our v=2 decrypt (mutual ECDH
    // auth), and providing them with the sender key only lets
    // them decode messages they were already able to receive —
    // no new content is exposed.
    let direct_recipient_owned: Option<crate::wire_v2::RecipientV3> = if recipients
        .iter()
        .any(|(did, _)| did.as_str() == requester_discord_id)
    {
        None
    } else {
        let pm = state.peer_map.lock().expect("peer_map mutex poisoned");
        pm.get(requester_discord_id).and_then(|entry| {
            let x_b64 = entry.pubkey.as_ref()?;
            let mlkem_b64 = entry.ik_mlkem768_pub.as_ref()?;
            let x_bytes = STANDARD.decode(x_b64).ok()?;
            if x_bytes.len() != crypto::x25519::PUBLIC_KEY_SIZE {
                return None;
            }
            let mlkem_bytes = STANDARD.decode(mlkem_b64).ok()?;
            if mlkem_bytes.len() != crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE {
                return None;
            }
            let mut x_arr = [0u8; crypto::x25519::PUBLIC_KEY_SIZE];
            x_arr.copy_from_slice(&x_bytes);
            let mut mlkem_arr = [0u8; crypto::ml_kem_768::ENCAPSULATION_KEY_SIZE];
            mlkem_arr.copy_from_slice(&mlkem_bytes);
            Some(crate::wire_v2::RecipientV3 {
                x25519_pub: crypto::x25519::PublicKey::from_bytes(x_arr),
                mlkem_pub: crypto::ml_kem_768::EncapsulationKey::from_bytes(&mlkem_arr),
            })
        })
    };
    let peer_recipient: &crate::wire_v2::RecipientV3 = match recipients
        .iter()
        .find(|(did, _)| did.as_str() == requester_discord_id)
    {
        Some((_, r)) => r,
        None => match direct_recipient_owned.as_ref() {
            Some(r) => {
                tracing::info!(
                    requester = %crate::log_id::log_id(requester_discord_id),
                    scope = %crate::log_id::log_id(&scope_key),
                    "OSL: SKDM_REQUEST: requester not in channel_members \
                     (gateway cache cold); falling back to direct peer_map \
                     RecipientV3 lookup so the request doesn't IGNORE-loop"
                );
                r
            }
            None => {
                // Requester not whitelisted-resolvable AND not
                // directly known by peer_map — genuinely nothing
                // we can send them.
                return Ok(OSL_RESULT_RECOVERY_IGNORED.to_string());
            }
        },
    };

    // Probe-3 Option-2 step 1: emit the recovery SKDM through the
    // same v=3 bundle path as the main send loop (single-recipient
    // slice). Transport no longer depends on v=4 ratchet liveness.
    let wire = send_skdm_via_v3_bundle(
        &sender_sk,
        &self_pk,
        std::slice::from_ref(peer_recipient),
        &scope_key,
        chain_id,
        &rotation_root,
        physical_device_id.as_bytes(),
    )?;
    tracing::info!(
        requester = %crate::log_id::log_id(requester_discord_id),
        scope = %crate::log_id::log_id(&scope_key),
        "OSL: SKDM_REQUEST honored — re-emitting v=3-bundled SKDM"
    );
    Ok(format!("{OSL_RESULT_SKDM_REREQUEST_PREFIX}{wire}"))
}

/// Auto-recovery inbound handler for `MSG_TYPE_SESSION_RESET` (0x07):
/// the sender says our shared v=4 ratchet is desynced and they have
/// dropped their side. Honor it when it passes the
/// staleness/replay/honor-throttle guards.
///
/// Act-on-symptom DOWNGRADE (one-directional-desync fix): a
/// SESSION_RESET only reaches this function after it has been
/// successfully `wire_v2`-decrypted — i.e. it was PQ-hybrid wrapped to
/// our identity using the peer's identity secret. A third party who
/// can merely post into the channel cannot forge one that decrypts, so
/// the original "could be spammed by anyone" threat is already closed
/// by that authentication for SESSION_RESET specifically. Requiring an
/// *additional* local decrypt failure before honoring it broke the
/// common real case: a one-directional ratchet desync (peer→us fails,
/// us→peer still works) leaves the side that must reset with no local
/// symptom, so the reset was ignored forever and the session never
/// healed without two console commands. We now honor an authenticated,
/// non-replayed, non-throttled reset regardless of corroboration; the
/// symptom is still recorded and logged (`corroborated`) for forensics
/// but is no longer a gate. Residual risk: a peer holding valid keys
/// can induce at most one (idempotent, cheap) re-handshake per
/// `RECOVERY_MIN_INTERVAL_SECS` — a throttled self-inflicted nuisance,
/// not a third-party DoS, no secret exposure, no MITM gain. On honor,
/// drop our `ratchet_state` for the peer so the next v=4 send
/// re-handshakes.

/// Phase 9-A3: install or rotate a peer's `ReceiverChain` for the
/// scope named in the SKDM payload. Persists `sender_key_state.json`
/// after mutation. Returns the sentinel string so the dispatcher
/// surfaces "control handled, no user content" to the JS layer.
pub(crate) fn apply_skdm_recv(
    state: &AppState,
    sender_discord_id: &str,
    payload_bytes: &[u8],
) -> Result<String, String> {
    use crypto::sender_keys::{PhysicalDeviceId, SenderKeyState, SenderKeyStateOnDisk};
    let payload = crate::control_messages::deserialize_sender_key_distribution(payload_bytes)
        .map_err(|e| format!("OSL: SKDM: deserialize: {e}"))?;

    let scope_key = payload.scope_storage_key.clone();
    // Reject SKDMs for an unknown scope_kind (defensive — boot.js
    // should never produce one). `Scope::parse` returns None for
    // malformed storage keys.
    if crate::scope::Scope::parse(&scope_key).is_none() {
        return Err(format!(
            "OSL: SKDM: payload.scope_storage_key '{scope_key}' is not a valid scope",
            scope_key = crate::log_id::log_id(&scope_key)
        ));
    }

    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        let entry = g.states.entry(scope_key.clone()).or_default();
        let mut live: SenderKeyState = entry
            .clone()
            .try_into()
            .map_err(|e| format!("OSL: SKDM: load existing state: {e}"))?;
        let peer_bytes = sender_discord_id.as_bytes().to_vec();
        let physical_device_id = PhysicalDeviceId::from_bytes(payload.physical_device_id)
            .map_err(|e| format!("OSL: SKDM: physical_device_id binding invalid or absent: {e}"))?;
        if live
            .receiver_chain_for_physical_device(&peer_bytes, physical_device_id)
            .is_some()
        {
            live.rotate_receiver(
                &peer_bytes,
                payload.chain_id,
                &payload.rotation_root,
                physical_device_id,
            )
            .map_err(|e| format!("OSL: SKDM: rotate_receiver: {e}"))?;
        } else {
            live.install_receiver(
                peer_bytes,
                payload.chain_id,
                &payload.rotation_root,
                physical_device_id,
            )
            .map_err(|e| format!("OSL: SKDM: install_receiver: {e}"))?;
        }
        *entry = SenderKeyStateOnDisk::from(&live);
        g.version = 1;
    }
    persist_sender_key_state_now(state);
    tracing::info!(
        sender = %crate::log_id::log_id(sender_discord_id),
        scope = %crate::log_id::log_id(&scope_key),
        chain_id = payload.chain_id,
        "[OSL] SKDM applied: receiver chain installed/rotated"
    );
    Ok(OSL_RESULT_SKDM_APPLIED.to_string())
}

/// Phase 9-A3: v=5 receive dispatch. Parses the wire, applies the
/// kill-list gate (same as v=4), looks up the matching
/// `SenderKeyState` + `ReceiverChain`, runs the sender-keys
/// `decrypt`, persists, returns plaintext.
pub(crate) fn decrypt_v5_recv(
    state: &AppState,
    sender_discord_id: String,
    content: String,
    scope_opt: Option<crate::scope::Scope>,
) -> Result<String, String> {
    use crypto::sender_keys::{SenderContext, SenderKeyState, SenderKeyStateOnDisk};

    let parsed =
        crate::wire_v2::decrypt_v5(&content).map_err(|e| format!("OSL: v=5 decode: {e}"))?;
    verify_v5_sender_discord_binding(state, &sender_discord_id, &parsed.sender_ik_pub)?;

    let scope = scope_opt
        .ok_or_else(|| "OSL: v=5 decode: scope required for sender-keys lookup".to_string())?;
    let scope_key = scope.storage_key();

    // Sender ML-KEM pub for AD binding. Two cases:
    //   - Sender is the local user (self-decrypt loopback): pull
    //     bytes from `identity.mlkem_public_bytes` so AD matches the
    //     sender-side encrypt path exactly.
    //   - Sender is a peer: pull bytes from peer_map. Empty Vec if
    //     not yet known — AD will then differ and AEAD fails with
    //     a clear error.
    let sender_mlkem_pub_bytes: Vec<u8> = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        let is_self = parsed.sender_ik_pub.as_bytes() == identity.x25519_public.as_bytes();
        if is_self {
            identity.mlkem_public_bytes.to_vec()
        } else {
            drop(id_guard);
            let pm_guard = state.peer_map.lock().expect("peer_map mutex poisoned");
            pm_guard
                .get(&sender_discord_id)
                .and_then(|pe| pe.ik_mlkem768_pub.as_deref())
                .and_then(|b64| STANDARD.decode(b64).ok())
                .unwrap_or_default()
        }
    };

    let ctx = SenderContext {
        sender_ik_x25519_pub: parsed.sender_ik_pub,
        sender_ik_mlkem_pub: sender_mlkem_pub_bytes,
        group_id: scope_key.clone().into_bytes(),
        session_version: crypto::sender_keys::SESSION_VERSION_V1,
    };

    // Load the per-scope SenderKeyState. If absent → no SKDM has
    // arrived yet → return a clear retry-worthy error.
    let mut sks: SenderKeyState = {
        let g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        match g.states.get(&scope_key) {
            Some(disk) => disk.clone().try_into().map_err(|e| {
                format!(
                    "OSL: v=5 decode: load sender_key_state for scope {scope_key}: {e}",
                    scope_key = crate::log_id::log_id(&scope_key)
                )
            })?,
            None => {
                return Err(format!(
                    "OSL: v=5 decode: no installed sender-key state for peer \
                     {sender_discord_id} in scope {scope_key} — awaiting SKDM",
                    sender_discord_id = crate::log_id::log_id(&sender_discord_id),
                    scope_key = crate::log_id::log_id(&scope_key)
                ));
            }
        }
    };

    let peer_bytes = sender_discord_id.as_bytes().to_vec();
    if sks.receiver_chain(&peer_bytes).is_none() {
        return Err(format!(
            "OSL: v=5 decode: no installed sender-key state for peer \
             {sender_discord_id} in scope {scope_key} — awaiting SKDM",
            sender_discord_id = crate::log_id::log_id(&sender_discord_id),
            scope_key = crate::log_id::log_id(&scope_key)
        ));
    }

    let em = crypto::sender_keys::EncryptedMessage {
        header_nonce: parsed.header_nonce,
        enc_header: parsed.enc_header,
        message_nonce: parsed.message_nonce,
        ciphertext: parsed.ciphertext,
    };
    let plaintext_bytes = sks
        .decrypt_from(&peer_bytes, &em, &ctx)
        .map_err(|e| format!("OSL: v=5 decode: decrypt_from: {e}"))?;

    // Persist updated state.
    {
        let mut g = state
            .sender_key_state
            .lock()
            .expect("sender_key_state mutex poisoned");
        g.states
            .insert(scope_key.clone(), SenderKeyStateOnDisk::from(&sks));
        g.version = 1;
    }
    persist_sender_key_state_now(state);

    // 9-C1: permissive decrypt — no per-scope accept gate. The
    // self-sender pubkey-comparison bypass that previously guarded
    // the gate is also gone (the gate is gone, so the bypass is
    // moot).
    let _ = (scope, sender_discord_id);

    String::from_utf8(plaintext_bytes)
        .map_err(|_| "OSL: v=5 decrypted plaintext is not valid UTF-8".to_string())
}

fn verify_v5_sender_discord_binding(
    state: &AppState,
    sender_discord_id: &str,
    wire_sender_pub: &crypto::x25519::PublicKey,
) -> Result<(), String> {
    let self_expected = {
        let id_guard = state.identity.lock().expect("identity mutex poisoned");
        let identity = id_guard
            .as_ref()
            .ok_or_else(|| "OSL: identity not loaded".to_string())?;
        let self_claim = identity.discord_snowflake.as_deref() == Some(sender_discord_id)
            || identity.user_id.as_str() == sender_discord_id;
        self_claim.then_some(identity.x25519_public)
    };

    let expected_sender = match self_expected {
        Some(pk) => pk,
        None => resolve_pinned_sender_pubkey(state, sender_discord_id)
            .map_err(|_| "OSL: v5 sender identity is not pinned".to_string())?,
    };

    if wire_sender_pub != &expected_sender {
        return Err("OSL: v5 authenticated sender refused".to_string());
    }

    Ok(())
}
