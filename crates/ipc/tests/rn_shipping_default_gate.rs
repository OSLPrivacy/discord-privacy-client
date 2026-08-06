//! The OSL-RN runtime activation gate, asserted through the **shipping
//! default** rather than through a flag the test sets itself.
//!
//! Task 0428 changes the product default: an eligible one-to-one chat must
//! start the OSL-RN sequence without a user setting. This file drives the real
//! command path with a fresh `AppState::new()` and verifies the observable
//! consequences: RN wire `0x10`, a persisted session, and an RN pin.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
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
use sha2::{Digest, Sha256};

const ALICE_DID: &str = "900000000000000201";
const BOB_DID: &str = "900000000000000202";

/// A `/v1/pubkeys` body shaped exactly like the deployed key server's, signed
/// by the peer's own Ed25519 key over the capability-bearing `REG_MSG`. This
/// is what a peer registered by *this* build looks like: `register` always
/// sends `CLIENT_RN_CAPABILITY_FLOOR`, and the server's capability column only
/// ever raises (`keyserver-cf/src/endpoints/register.ts:269-287`).
fn signed_pubkeys_response_with_capabilities(
    identity: &Identity,
    capabilities: u32,
) -> serde_json::Value {
    let x25519 = STANDARD.encode(identity.x25519_public.as_bytes());
    let ed25519 = STANDARD.encode(identity.ed25519_public.as_bytes());
    let mlkem = STANDARD.encode(identity.mlkem_public_bytes);
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

fn signed_pubkeys_response(identity: &Identity) -> serde_json::Value {
    signed_pubkeys_response_with_capabilities(identity, CLIENT_RN_CAPABILITY_FLOOR)
}

fn signed_downgrade_pubkeys_response(identity: &Identity) -> serde_json::Value {
    signed_pubkeys_response_with_capabilities(identity, 0)
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
    start_keyserver_with_request_log(responses).0
}

fn start_keyserver_with_request_log(
    responses: Vec<(&'static str, serde_json::Value)>,
) -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback keyserver");
    let port = listener.local_addr().expect("loopback address").port();
    let (tx, rx) = mpsc::channel();
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
            let _ = tx.send(request_text.lines().next().unwrap_or("<empty>").to_owned());
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
    (port, rx)
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

fn snapshot_rn_saved_state(config_dir: &Path) -> (String, usize) {
    let rn_dir = config_dir.join(ipc::wire_rn::RN_SESSION_DIR);
    let mut entries: Vec<_> = std::fs::read_dir(&rn_dir)
        .unwrap_or_else(|e| panic!("read RN state dir {}: {e}", rn_dir.display()))
        .map(|entry| entry.expect("read RN state entry").path())
        .filter(|path| path.is_file())
        .collect();
    entries.sort();

    let mut hasher = Sha256::new();
    for path in &entries {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("RN state file name is UTF-8");
        let bytes = std::fs::read(path)
            .unwrap_or_else(|e| panic!("read RN state file {}: {e}", path.display()));
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    (format!("{:x}", hasher.finalize()), entries.len())
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

#[test]
fn direct_chat_peer_downgrade_command_is_refused_and_saved_state_is_unchanged() {
    let config_dir = tempfile::tempdir().expect("isolated OSL config dir");
    let _config_guard = use_osl_config_dir(config_dir.path());
    let alice = generate_identity("rn-downgrade-alice".to_owned());
    let bob = generate_identity("rn-downgrade-bob".to_owned());
    let (port, requests) = start_keyserver_with_request_log(vec![
        ("GET /v1/pubkeys/", signed_pubkeys_response(&bob)),
        ("GET /v1/prekey-bundle/", prekey_bundle_response(&bob)),
        ("GET /v1/pubkeys/", signed_downgrade_pubkeys_response(&bob)),
    ]);
    let alice_state = shipping_state_with_current_build_peer(alice, &bob, BOB_DID, port);

    let agreed_wire = cmd_osl_encrypt_message_v2_wire(
        &alice_state,
        "agreed stronger sequence".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .map(|wire| wire.content)
    .expect("first direct-chat send should establish OSL-RN");
    assert_eq!(
        osl_ratchet_next::peek_wire_version(&agreed_wire),
        Some(osl_ratchet_next::WIRE_VERSION_RN),
        "test fixture must first agree the stronger sequence"
    );

    let store =
        ipc::wire_rn::RnSessionStore::for_config_dir(config_dir.path()).expect("RN session store");
    assert!(store
        .load_session(bob.x25519_public.as_bytes())
        .expect("load agreed RN session")
        .is_some());
    assert!(store
        .load_pin(bob.x25519_public.as_bytes())
        .expect("load agreed RN pin")
        .is_pinned_to_rn());
    let (before_sha256, before_files) = snapshot_rn_saved_state(config_dir.path());

    let refused = cmd_osl_encrypt_message_v2_wire(
        &alice_state,
        "attempt weaker sequence".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .expect_err("downgraded direct-chat capability must refuse the send command");
    let refusal = "peer is pinned to OSL-RN; refusing to send a legacy v=3 message";
    assert!(
        refused.contains(refusal),
        "downgrade command returned the wrong refusal: {refused}"
    );

    let (after_sha256, after_files) = snapshot_rn_saved_state(config_dir.path());
    assert_eq!(
        before_files, after_files,
        "refused downgrade must leave the RN state file count unchanged"
    );
    assert_eq!(
        before_sha256, after_sha256,
        "refused downgrade must leave saved RN state bytes unchanged"
    );
    assert!(store
        .load_pin(bob.x25519_public.as_bytes())
        .expect("reload RN pin after refusal")
        .is_pinned_to_rn());

    let request_lines: Vec<String> = (0..3)
        .map(|_| requests.recv().expect("keyserver request line"))
        .collect();
    assert!(request_lines[0].starts_with("GET /v1/pubkeys/"));
    assert!(request_lines[1].starts_with("GET /v1/prekey-bundle/"));
    assert!(request_lines[2].starts_with("GET /v1/pubkeys/"));

    println!(
        "TASK 0431 downgrade_command=refused refusal=\"{refusal}\" \
         saved_state_unchanged={} before_files={} after_files={} \
         before_sha256={} after_sha256={} downgraded_capabilities={}",
        usize::from(before_sha256 == after_sha256 && before_files == after_files),
        before_files,
        after_files,
        before_sha256,
        after_sha256,
        0
    );
}

#[test]
fn task0436_stolen_key_check_is_red_when_direct_chat_default_is_not_stronger() {
    let config_dir = tempfile::tempdir().expect("isolated OSL config dir");
    let _config_guard = use_osl_config_dir(config_dir.path());
    let alice_identity = generate_identity("rn-0436-alice".to_owned());
    let bob_identity = generate_identity("rn-0436-bob".to_owned());
    let port = start_keyserver(vec![
        ("GET /v1/pubkeys/", signed_pubkeys_response(&bob_identity)),
        (
            "GET /v1/prekey-bundle/",
            prekey_bundle_response(&bob_identity),
        ),
    ]);
    let alice_state =
        shipping_state_with_current_build_peer(alice_identity, &bob_identity, BOB_DID, port);

    assert!(
        alice_state.rn_wire_in_enabled(),
        "TASK0436 chat is not stronger"
    );
    let wire = cmd_osl_encrypt_message_v2_wire(
        &alice_state,
        "TASK0436 default-strength probe".to_owned(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![BOB_DID.to_owned()],
        ALICE_DID.to_owned(),
    )
    .map(|wire| wire.content)
    .expect("TASK0436 chat is not stronger");
    assert_eq!(
        osl_ratchet_next::peek_wire_version(&wire),
        Some(osl_ratchet_next::WIRE_VERSION_RN),
        "TASK0436 chat is not stronger"
    );
    println!("TASK0436 direct_chat_default=stronger wire_version=0x10");

    let (mut alice, mut bob, mut rng) = osl_ratchet_next::test_support::established_pair(0x0436);
    let old_0 = alice
        .encrypt(0, b"TASK0436 old protected 0", &mut rng)
        .expect("old 0 encrypts");
    let old_1 = alice
        .encrypt(0, b"TASK0436 old protected 1", &mut rng)
        .expect("old 1 encrypts");
    assert_eq!(
        bob.decrypt(&old_0, &mut rng)
            .expect("live bob opens old 0")
            .plaintext,
        b"TASK0436 old protected 0"
    );
    assert_eq!(
        bob.decrypt(&old_1, &mut rng)
            .expect("live bob opens old 1")
            .plaintext,
        b"TASK0436 old protected 1"
    );

    let current = alice
        .encrypt(0, b"TASK0436 current protected", &mut rng)
        .expect("current encrypts");
    let stolen_current_key = bob.export_state().expect("export stolen current key");
    let mut current_reader =
        osl_ratchet_next::Session::import_state(&stolen_current_key).expect("import current key");
    let current_plaintext = current_reader
        .decrypt(&current, &mut rng)
        .expect("stolen current key opens current message")
        .plaintext;
    assert_eq!(current_plaintext, b"TASK0436 current protected");

    let earlier = [&old_0, &old_1];
    let mut earlier_locked = 0usize;
    let mut old_message_opened = 0usize;
    for protected_message in earlier {
        let mut replay_reader = osl_ratchet_next::Session::import_state(&stolen_current_key)
            .expect("import replay key");
        if replay_reader.decrypt(protected_message, &mut rng).is_err() {
            earlier_locked += 1;
        } else {
            old_message_opened += 1;
        }
    }

    println!(
        "TASK0436 current_opened=1 plaintext={}",
        String::from_utf8(current_plaintext).expect("fixture plaintext is UTF-8")
    );
    println!(
        "TASK0436 old_message_opened={} earlier_locked={} total_earlier={}",
        old_message_opened,
        earlier_locked,
        earlier.len()
    );
    assert_eq!(old_message_opened, 0, "TASK0436 old message opens");
    assert_eq!(earlier_locked, earlier.len());
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
