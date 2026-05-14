//! Phase 9-B1 Task 8: burning a scope drops any in-flight Mode 1
//! reassembly buffer for that channel.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::app_preferences::StegoMode;
use ipc::commands::{
    cmd_osl_burn_scope_data, cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2,
    OSL_RESULT_MODE1_INCOMPLETE_PREFIX,
};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity};

const LIAM_DID: &str = "1477008451799482419";
const HENRY_DID: &str = "1502770642930634812";
const DM_CHANNEL_ID: &str = HENRY_DID; // DM scope_id == peer DID == channel_id

fn fresh_state() -> AppState {
    let s = AppState::new();
    *s.identity.lock().unwrap() = Some(generate_identity("liam".into()));
    let mut pm = s.peer_map.lock().unwrap();
    let pe = pm.entry(LIAM_DID.to_string()).or_default();
    pe.is_self = Some(true);
    pe.discord_id = Some(LIAM_DID.to_string());
    drop(pm);

    let henry_id: Identity = generate_identity("henry".into());
    let mut pm = s.peer_map.lock().unwrap();
    let pe = pm.entry(HENRY_DID.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(henry_id.x25519_public.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(&henry_id.mlkem_public_bytes));
    pe.discord_id = Some(HENRY_DID.to_string());
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
    drop(pm);

    let scope = Scope::dm(HENRY_DID);
    let mut ws = s.whitelist_state.lock().unwrap();
    ws.insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
        },
    );
    drop(ws);

    let mut prefs = s.app_preferences.lock().unwrap();
    prefs.stego_mode = StegoMode::Mode1;
    drop(prefs);
    s
}

fn first_chunk_of_mode1_multi(state: &AppState) -> (String, usize) {
    let plaintext: String = "y".repeat(400);
    let out = cmd_osl_encrypt_message_v2(
        state,
        plaintext,
        ScopeInput::from(&Scope::dm(HENRY_DID)),
        vec![HENRY_DID.into()],
        LIAM_DID.into(),
    )
    .unwrap();
    assert!(out.messages.len() >= 2, "test wants multi-chunk session");
    (out.messages[0].clone(), out.messages.len())
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn burn_evicts_in_flight_reassembly_for_channel() {
    let state = fresh_state();
    let (first_cover, total) = first_chunk_of_mode1_multi(&state);
    assert!(total >= 2);

    // Push the first chunk; verify the reassembly buffer carries it.
    let r = cmd_osl_decrypt_message_v2(
        &state,
        None,
        DM_CHANNEL_ID.into(),
        LIAM_DID.into(),
        first_cover,
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
    .unwrap();
    assert!(r.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));
    {
        let bufs = state.mode1_reassembly.lock().unwrap();
        let buf = bufs.get(DM_CHANNEL_ID).expect("channel buffer present");
        assert_eq!(buf.len(), 1, "one in-flight session expected");
    }

    // Burn the DM scope's channel data.
    let res = cmd_osl_burn_scope_data(&state, "dm".into(), HENRY_DID.into(), None);
    assert!(res.is_ok(), "burn should not error: {res:?}");

    // Reassembly entry for this channel should now be gone.
    let bufs = state.mode1_reassembly.lock().unwrap();
    assert!(
        bufs.get(DM_CHANNEL_ID).is_none(),
        "burn must drop the channel's reassembly buffer"
    );
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn burn_only_drops_target_channel_buffer() {
    let state = fresh_state();
    let (first_cover, _) = first_chunk_of_mode1_multi(&state);

    // Push into channel A and into channel B (a different channel
    // id — same scope, but reassembly is keyed by channel id).
    let r1 = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "CHANNEL_X".into(),
        LIAM_DID.into(),
        first_cover.clone(),
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
    .unwrap();
    assert!(r1.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));
    let r2 = cmd_osl_decrypt_message_v2(
        &state,
        None,
        "CHANNEL_Y".into(),
        LIAM_DID.into(),
        first_cover,
        Some(ScopeInput::from(&Scope::dm(HENRY_DID))),
        None,
    )
    .unwrap();
    assert!(r2.starts_with(OSL_RESULT_MODE1_INCOMPLETE_PREFIX));

    // Burn only "CHANNEL_X". Buffer for "CHANNEL_Y" must remain.
    // We call drop_mode1_reassembly_for_channel via the burn entry
    // point — cmd_osl_burn_scope_data resolves channel_id from the
    // scope; for a DM the channel_id IS the scope_id. To target
    // CHANNEL_X specifically we fabricate a scope with that id.
    cmd_osl_burn_scope_data(&state, "dm".into(), "CHANNEL_X".into(), None).unwrap();

    let bufs = state.mode1_reassembly.lock().unwrap();
    assert!(bufs.get("CHANNEL_X").is_none(), "burned channel evicted");
    assert!(
        bufs.get("CHANNEL_Y").is_some(),
        "non-burned channel still has its buffer"
    );
}

#[test]
#[ignore = "Mode 1 disabled in V2; V3 will re-enable"]
fn burn_is_idempotent_when_no_in_flight_sessions() {
    let state = fresh_state();
    // No reassembly state yet; the burn must not panic and must
    // succeed (the row count from the message store will be 0).
    let res = cmd_osl_burn_scope_data(&state, "dm".into(), HENRY_DID.into(), None);
    assert!(res.is_ok());
    let bufs = state.mode1_reassembly.lock().unwrap();
    assert!(bufs.is_empty());
}
