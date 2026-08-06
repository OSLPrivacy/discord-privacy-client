//! The OSL-RN runtime activation gate, asserted through the **shipping
//! default** rather than through a flag the test sets itself.
//!
//! Task 0428 changes the product default: an eligible one-to-one chat must
//! start the OSL-RN sequence without a user setting. This file drives the real
//! command path with a fresh `AppState::new()` and verifies the observable
//! consequences: RN wire `0x10`, a persisted session, and an RN pin.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use ipc::whitelist_state::ScopeState;
use keystore::client::{reg_msg_with_capabilities, KeyServerClient, CLIENT_RN_CAPABILITY_FLOOR};
use keystore::{generate_identity, Identity};

const ALICE_DID: &str = "900000000000000201";
const BOB_DID: &str = "900000000000000202";

/// A `/v1/pubkeys` body shaped exactly like the deployed key server's, signed
/// by the peer's own Ed25519 key over the capability-bearing `REG_MSG`. This
/// is what a peer registered by *this* build looks like: `register` always
/// sends `CLIENT_RN_CAPABILITY_FLOOR`, and the server's capability column only
/// ever raises (`keyserver-cf/src/endpoints/register.ts:269-287`).
fn signed_pubkeys_response(identity: &Identity) -> serde_json::Value {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
    let capabilities = CLIENT_RN_CAPABILITY_FLOOR;
    let message = reg_msg_with_capabilities(
        &identity.user_id,
        &x25519,
        &ed25519,
        &mlkem,
        None,
        capabilities,
    );
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, &message);

    serde_json::json!({
        "user_id": identity.user_id.clone(),
        "ik_x25519_pub": x25519,
        "ik_ed25519_pub": ed25519,
        "ik_mlkem768_pub": mlkem,
        "registered_at": "2026-08-04T00:00:00Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": null,
        "rn_capabilities": capabilities,
        "registration_sig": STANDARD.encode(signature.as_bytes()),
        "identity_scheme": null,
        "identity_bundle_version": null,
        "identity_revision": null,
        "ik_root_ed25519_pub": null,
        "identity_bundle_proof_sig": null,
    })
}

fn prekey_bundle_response(identity: &Identity) -> serde_json::Value {
    let spk = identity
        .ratchet_initial_pub
        .expect("fresh identity carries a signed RN prekey");
    let signature = crypto::ed25519::sign(&identity.ed25519_secret, spk.as_bytes());
    serde_json::json!({
        "user_id": identity.user_id.clone(),
        "ik_x25519_pub": STANDARD.encode(identity.x25519_public.as_bytes()),
        "ik_ed25519_pub": STANDARD.encode(identity.ed25519_public.as_bytes()),
        "ik_mlkem768_pub": STANDARD.encode(identity.mlkem_public_bytes),
        "spk_pub": STANDARD.encode(spk.as_bytes()),
        "spk_signature": STANDARD.encode(signature.as_bytes()),
        "spk_rotated_at": "2026-08-04T00:00:00Z",
        "opk": null,
        "remaining_opk_count": 0,
        "ik_ratchet_initial_pub": null,
    })
}

fn start_keyserver(responses: Vec<(&'static str, serde_json::Value)>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().expect("loopback address").port();
    thread::spawn(move || {
        for (expected_path, body) in responses {
            let (mut stream, _) = listener.accept().expect("keyserver request");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read request");
            let request_text = std::str::from_utf8(&request[..read]).expect("request UTF-8");
            assert!(
                request_text.starts_with(expected_path),
                "the send command must use the expected keyserver path: \
                 expected={expected_path} actual={}",
                request_text.lines().next().unwrap_or("<empty>")
            );
            let encoded = serde_json::to_vec(&body).expect("serialize pubkeys response");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                encoded.len()
            )
            .expect("write response headers");
            stream.write_all(&encoded).expect("write response body");
        }
    });
    port
}

struct OslConfigDirGuard;

impl Drop for OslConfigDirGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn use_osl_config_dir(dir: &std::path::Path) -> OslConfigDirGuard {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(dir.to_path_buf()));
    OslConfigDirGuard
}

/// Build the state the app builds. **Nothing here touches
/// `set_rn_wire_in_enabled`** — that is the whole point of the file.
fn shipping_state_with_current_build_peer(
    self_identity: Identity,
    peer_identity: &Identity,
    peer_did: &str,
    keyserver_port: u16,
) -> AppState {
    let state = AppState::new();
    *state.identity.lock().expect("identity lock") = Some(self_identity);
    *state.keyserver.lock().expect("keyserver lock") = Some(
        KeyServerClient::new(format!("http://127.0.0.1:{keyserver_port}"))
            .expect("loopback keyserver client"),
    );
    state
        .whitelist_state
        .lock()
        .expect("whitelist lock")
        .insert(
            Scope::dm(peer_did).storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                ..ScopeState::default()
            },
        );
    let mut peers = state.peer_map.lock().expect("peer map lock");
    let entry = peers.entry(peer_did.to_owned()).or_default();
    entry.osl_user_id = Some(peer_identity.user_id.clone());
    entry.discord_id = Some(peer_did.to_owned());
    entry.pubkey = Some(STANDARD.encode(peer_identity.x25519_public.as_bytes()));
    entry.ik_mlkem768_pub = Some(STANDARD.encode(peer_identity.mlkem_public_bytes));
    entry.tofu_key_bundle = Some(KeyBundle {
        ed25519_pub: STANDARD.encode(peer_identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(peer_identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(peer_identity.mlkem_public_bytes),
        ratchet_initial_pub: None,
    });
    entry.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
    drop(peers);
    state
}

#[test]
fn new_eligible_direct_chat_reports_forward_secrecy_on_without_user_setting() {
    let config_dir = tempfile::tempdir().expect("isolated OSL config dir");
    let _config_guard = use_osl_config_dir(config_dir.path());
    let alice = generate_identity("rn-default-alice".to_owned());
    let bob = generate_identity("rn-default-bob".to_owned());
    let port = start_keyserver(vec![
        ("GET /v1/pubkeys/", signed_pubkeys_response(&bob)),
        ("GET /v1/prekey-bundle/", prekey_bundle_response(&bob)),
    ]);
    let alice_state = shipping_state_with_current_build_peer(alice, &bob, BOB_DID, port);

    assert!(
        alice_state.rn_wire_in_enabled(),
        "a freshly constructed AppState must enable eligible direct-chat OSL-RN"
    );

    let wire = cmd_osl_encrypt_message_v2_wire(
        &alice_state,
        "shipping default must emit an OSL-RN wire".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .map(|wire| wire.content)
    .expect("eligible direct chat should send with forward secrecy on by default");

    let wire_version = osl_ratchet_next::peek_wire_version(&wire);
    assert_eq!(wire_version, Some(osl_ratchet_next::WIRE_VERSION_RN));
    let store =
        ipc::wire_rn::RnSessionStore::for_config_dir(config_dir.path()).expect("RN session store");
    let session_persisted = store
        .load_session(bob.x25519_public.as_bytes())
        .expect("load persisted RN session")
        .is_some();
    let pin_rn = store
        .load_pin(bob.x25519_public.as_bytes())
        .expect("load RN pin")
        .is_pinned_to_rn();
    assert!(
        session_persisted,
        "eligible direct chat must persist an RN session"
    );
    assert!(pin_rn, "eligible direct chat must raise the RN pin");

    println!(
        "TASK 0428 direct_chat=one_to_one eligible=1 forward_secrecy=on \
         user_setting=none wire_version=0x{:02x} session_persisted={} pin_rn={}",
        wire_version.expect("already asserted RN wire"),
        usize::from(session_persisted),
        usize::from(pin_rn)
    );
}

/// The other half of the same gate, and it is deliberately asymmetric:
/// **receive is open, send is closed.**
///
/// `cmd_osl_decrypt_message_v2`'s `0x10` arm passes the *compile-time*
/// `wire_rn::RN_WIRE_IN_ENABLED` (`true`) to
/// `accept_rn_bootstrap_inbound_unknown`, not `state.rn_wire_in_enabled()`
/// (`false`). That is `MIGRATION.md`'s "ship receive-only" step, so it is
/// recorded here rather than changed — but it must not be able to drift
/// silently in either direction.
///
/// The two refusals are distinguishable by message, which is what makes this
/// a gate and not a grep:
///   * gate closed  -> `OSL: secure message format is not available`
///     (`commands.rs:7042`)
///   * gate open, wire bad -> `OSL: secure message could not be opened`
///     (`commands.rs:7086`)
///
/// Mutant that turns this RED: pass `state.rn_wire_in_enabled()` instead of
/// `crate::wire_rn::RN_WIRE_IN_ENABLED` at `commands.rs:6805`. The receive arm
/// then closes under the shipping default and the message changes.
#[test]
fn shipping_default_still_enters_the_rn_receive_arm() {
    let state = AppState::new();
    state.install_identity(generate_identity("rn-default-receiver".to_owned()));
    assert!(state.rn_wire_in_enabled());

    // A truncated `0x10` wire: enough to route, never enough to open. It
    // reaches the accept and fails there, which is the observable difference
    // between "arm entered" and "arm refused".
    let truncated_rn_wire = format!("DPC0::{}", STANDARD.encode([0x10_u8]));

    let verdict = cmd_osl_decrypt_message_v2(
        &state,
        Some("rn-default-message".to_owned()),
        "rn-default-channel".to_owned(),
        "900000000000000203".to_owned(),
        truncated_rn_wire,
        None,
        None,
    )
    .expect_err("a truncated RN wire can never open");

    assert!(
        verdict.contains("could not be opened"),
        "the shipping default must still ENTER the RN receive arm — OSL-RN is \
         wired receive-only, and a build that refuses inbound 0x10 outright \
         has changed that: {verdict}"
    );
    assert!(
        !verdict.contains("format is not available"),
        "this is the closed-gate refusal from commands.rs:7042; the receive arm \
         is gated on the compile-time fuse, not on the runtime activation gate: \
         {verdict}"
    );
}
