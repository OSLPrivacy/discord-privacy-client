//! A7 acceptance: "locked" must mean the process can no longer decrypt.
//!
//! The audited defect: `lock_main_password_session` had zero non-test callers,
//! and the lock path production actually ran cleared only the file storage
//! key. The identity secret, ratchet state, peer map, whitelist and the open
//! `MessageStore` all stayed live, so a locked OSL kept decrypting.
//!
//! This test refuses to look at source text. It drives the real IPC commands:
//!
//!   1. Alice encrypts, Bob decrypts — SUCCEEDS.
//!   2. Bob's `MessageStore` round-trips a plaintext — SUCCEEDS.
//!   3. Bob's session is locked.
//!   4. The SAME ciphertext, and a fresh one, now FAIL to decrypt.
//!   5. The store is closed and history is gone.
//!   6. The idle path reaches the same lock through the command boundary.
//!   7. Unlock restores identity + files + store, and decrypt SUCCEEDS again.
//!
//! Everything asserted is an observable behaviour of a command, or a count
//! reported by the lock itself.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::x25519;
use ipc::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2_wire};
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::session_lock::{self, SessionLockTrigger};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, Identity, KeyServerClient};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const ALICE_DID: &str = "900000000000000003";
const BOB_DID: &str = "900000000000000001";
const CHANNEL: &str = "channel-a7";

// ---------------------------------------------------------------------------
// Harness (mirrors tests/phase_a2_integration_dr_roundtrip.rs)
// ---------------------------------------------------------------------------

fn fresh_state_for(name: &str) -> AppState {
    let state = AppState::new();
    *state.identity.lock().unwrap() = Some(generate_identity(name.to_string()));
    state
}

fn install_self_entry(state: &AppState, self_did: &str) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(self_did.to_string()).or_default();
    pe.is_self = Some(true);
    pe.discord_id = Some(self_did.to_string());
}

struct Pubkeys {
    x25519_pub: x25519::PublicKey,
    mlkem_pub_bytes: Vec<u8>,
    ratchet_initial_pub: x25519::PublicKey,
}

trait ClonePubkeys {
    fn clone_pubkeys(&self) -> Pubkeys;
}

impl ClonePubkeys for Identity {
    fn clone_pubkeys(&self) -> Pubkeys {
        Pubkeys {
            x25519_pub: self.x25519_public,
            mlkem_pub_bytes: self.mlkem_public_bytes.to_vec(),
            ratchet_initial_pub: self
                .ratchet_initial_pub
                .expect("fresh identity has ratchet pub"),
        }
    }
}

fn install_peer(state: &AppState, peer_did: &str, p: &Pubkeys) {
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.pubkey = Some(STANDARD.encode(p.x25519_pub.as_bytes()));
    pe.ik_mlkem768_pub = Some(STANDARD.encode(&p.mlkem_pub_bytes));
    pe.ik_ratchet_initial_pub = Some(STANDARD.encode(p.ratchet_initial_pub.as_bytes()));
    pe.discord_id = Some(peer_did.to_string());
}

fn install_dm_whitelist(state: &AppState, peer_did: &str) {
    let scope = Scope::dm(peer_did);
    {
        let mut ws = state.whitelist_state.lock().unwrap();
        ws.insert(
            scope.storage_key(),
            ScopeState {
                encrypt_toggle: true,
                auto_enabled: true,
                channel_whitelisted: false,
            },
        );
    }
    let mut pm = state.peer_map.lock().unwrap();
    let pe = pm.entry(peer_did.to_string()).or_default();
    pe.outgoing_whitelists.push(WhitelistEntry::Dm {
        broadened: false,
        enabled_at: None,
    });
}

fn pubkeys_json_for(state: &AppState, user_id: &str) -> String {
    let g = state.identity.lock().unwrap();
    let id = g.as_ref().expect("identity loaded");
    assert!(id.user_id == user_id, "fixture route must match identity");
    let req = keystore::client::KeyServerClient::build_register_request(id);
    serde_json::to_string(&serde_json::json!({
        "user_id": req.user_id,
        "ik_x25519_pub": req.ik_x25519_pub,
        "ik_ed25519_pub": req.ik_ed25519_pub,
        "ik_mlkem768_pub": req.ik_mlkem768_pub,
        "registered_at": "2026-01-01T00:00:00Z",
        "last_rotated_at": null,
        "ik_ratchet_initial_pub": req.ik_ratchet_initial_pub,
        "registration_sig": req.registration_sig,
    }))
    .expect("pubkeys response json")
}

/// Hand-rolled loopback keyserver: the v=3 send path forces a pubkey refresh
/// before encrypting, so the fixture serves back the same keys it wired in.
fn spawn_fake_keyserver(routes: Vec<(String, String)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local_addr").to_string();
    thread::spawn(move || {
        for conn in listener.incoming() {
            let mut stream = match conn {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut buf = Vec::new();
            let mut tmp = [0u8; 1024];
            loop {
                let n = match stream.read(&mut tmp) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if buf.len() > 64 * 1024 {
                    break;
                }
            }
            let head = String::from_utf8_lossy(&buf);
            let req_line = head.lines().next().unwrap_or("");
            let mut parts = req_line.split_whitespace();
            let method = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("");

            let (status, body): (&str, String) =
                if method == "GET" && path.starts_with("/v1/pubkeys/") {
                    let id = path.trim_start_matches("/v1/pubkeys/");
                    match routes.iter().find(|(u, _)| u == id) {
                        Some((_, json)) => ("200 OK", json.clone()),
                        None => ("404 Not Found", String::new()),
                    }
                } else if method == "POST" && path == "/v1/register" {
                    (
                        "200 OK",
                        "{\"user_id\":\"test\",\"status\":\"noop\"}".to_string(),
                    )
                } else {
                    ("404 Not Found", String::new())
                };

            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

fn setup_alice_bob_dm() -> (AppState, AppState) {
    let alice_state = fresh_state_for(ALICE_DID);
    let bob_state = fresh_state_for(BOB_DID);

    let alice_id = alice_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone_pubkeys();
    let bob_id = bob_state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .clone_pubkeys();

    install_self_entry(&alice_state, ALICE_DID);
    install_self_entry(&bob_state, BOB_DID);
    install_peer(&alice_state, BOB_DID, &bob_id);
    install_peer(&bob_state, ALICE_DID, &alice_id);
    install_dm_whitelist(&alice_state, BOB_DID);
    install_dm_whitelist(&bob_state, ALICE_DID);

    let alice_json = pubkeys_json_for(&alice_state, ALICE_DID);
    let bob_json = pubkeys_json_for(&bob_state, BOB_DID);
    let addr = spawn_fake_keyserver(vec![
        (ALICE_DID.to_string(), alice_json),
        (BOB_DID.to_string(), bob_json),
    ]);
    let base = format!("http://{addr}");
    *alice_state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&base).expect("fake keyserver client (alice)"));
    *bob_state.keyserver.lock().unwrap() =
        Some(KeyServerClient::new(&base).expect("fake keyserver client (bob)"));

    (alice_state, bob_state)
}

fn alice_sends(alice_state: &AppState, plaintext: &str) -> Result<String, String> {
    cmd_osl_encrypt_message_v2_wire(
        alice_state,
        plaintext.to_string(),
        ScopeInput::from(&Scope::dm(BOB_DID)),
        vec![ALICE_DID.to_string(), BOB_DID.to_string()],
        ALICE_DID.to_string(),
    )
    .map(|w| w.content)
}

fn bob_decrypts(bob_state: &AppState, wire: &str, msg_id: &str) -> Result<String, String> {
    cmd_osl_decrypt_message_v2(
        bob_state,
        Some(msg_id.to_string()),
        CHANNEL.to_string(),
        ALICE_DID.to_string(),
        wire.to_string(),
        Some(ScopeInput::from(&Scope::dm(ALICE_DID))),
        None,
    )
}

/// A fabricated per-peer ratchet record. The point is not that it is a valid
/// session — it is that `lock_session` must take it out of memory and let the
/// `ZeroizeOnDrop` wipe run, and must report having done so.
fn fabricated_ratchet_state() -> crypto::ratchet::RatchetStateOnDisk {
    crypto::ratchet::RatchetStateOnDisk {
        version: 1,
        root_key_b64: STANDARD.encode([0x11u8; 32]),
        dhs_secret_b64: STANDARD.encode([0x22u8; 32]),
        dhs_pub_b64: STANDARD.encode([0x33u8; 32]),
        dhr_b64: None,
        sending_chain_b64: Some(STANDARD.encode([0x44u8; 32])),
        sending_counter: 7,
        receiving_chain_b64: Some(STANDARD.encode([0x55u8; 32])),
        receiving_counter: 3,
        prev_sending_count: 0,
        hks_b64: None,
        hkr_b64: None,
        nhks_b64: STANDARD.encode([0x66u8; 32]),
        nhkr_b64: STANDARD.encode([0x77u8; 32]),
        skipped: Vec::new(),
        ctx: crypto::ratchet::SessionContextOnDisk {
            local_ik_x25519_pub_b64: STANDARD.encode([0x01u8; 32]),
            local_ik_mlkem_pub_b64: STANDARD.encode([0x02u8; 32]),
            peer_ik_x25519_pub_b64: STANDARD.encode([0x03u8; 32]),
            peer_ik_mlkem_pub_b64: STANDARD.encode([0x04u8; 32]),
            conversation_id_b64: STANDARD.encode(b"a7-session-lock"),
            session_version: 1,
        },
    }
}

fn persist_bob_account(bob_state: &AppState, account_dir: &Path) {
    std::fs::create_dir_all(account_dir).expect("account dir");
    let sealer = keystore::select_best_sealer();
    {
        let guard = bob_state.identity.lock().unwrap();
        let identity = guard.as_ref().expect("bob identity");
        keystore::save_identity(
            &account_dir.join("identity.json"),
            identity,
            sealer.as_ref(),
        )
        .expect("seal bob identity to disk");
    }
    {
        let pm = bob_state.peer_map.lock().unwrap();
        ipc::peer_map::write_peer_map(&account_dir.join("peer_map.json"), &pm)
            .expect("persist bob peer_map");
    }
    {
        let ws = bob_state.whitelist_state.lock().unwrap();
        let sd = bob_state.server_defaults.lock().unwrap();
        ipc::whitelist_state::write_whitelist_state_file(
            &account_dir.join("whitelist_state.json"),
            &ipc::whitelist_state::WhitelistStateFile {
                migrated_c1: true,
                scopes: ws.clone(),
                server_defaults: sd.clone(),
            },
        )
        .expect("persist bob whitelist");
    }
}

/// Both tests drive the same process-global slots (file storage key, config
/// dir overrides, the idle clock), and integration tests in one binary run on
/// parallel threads. Serialize them rather than pretend the globals are
/// per-test.
static PROCESS_GLOBALS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize_process_globals() -> std::sync::MutexGuard<'static, ()> {
    PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
}

fn reset_process_globals() {
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
    ipc::main_password::set_file_storage_key(None);
    session_lock::disarm_idle_lock();
}

fn open_bob_store(bob_state: &AppState, account_dir: &Path) {
    let secret: [u8; 32] = {
        let guard = bob_state.identity.lock().unwrap();
        *guard.as_ref().unwrap().x25519_secret.as_bytes()
    };
    let store =
        store::MessageStore::open(&account_dir.join("store"), &secret).expect("open bob store");
    *bob_state.message_store.lock().unwrap() = Some(store);
}

// ---------------------------------------------------------------------------
// The acceptance
// ---------------------------------------------------------------------------

#[test]
fn locking_the_session_stops_decryption_and_unlocking_restores_it() {
    let _globals = serialize_process_globals();
    reset_process_globals();
    let tmp = TempDir::new().expect("tempdir");
    let account_dir = tmp.path().join("account");
    std::fs::create_dir_all(&account_dir).unwrap();
    keystore::set_base_dir_override(Some(account_dir.clone()));
    keystore::set_active_account_dir(Some(account_dir.clone()));

    // Stand in for a successful main-password verify: the derived file storage
    // key lands in the process slot and the gate's own timer arms.
    let file_key = [0xA7u8; 32];
    ipc::main_password::set_file_storage_key_after_main_password_unlock(file_key);
    session_lock::arm_idle_lock();

    let (alice_state, bob_state) = setup_alice_bob_dm();

    // A live per-peer ratchet record, so the lock has secret ratchet material
    // to zeroize and can be measured doing it.
    {
        let mut pm = bob_state.peer_map.lock().unwrap();
        pm.get_mut(ALICE_DID).unwrap().ratchet_state = Some(fabricated_ratchet_state());
    }

    persist_bob_account(&bob_state, &account_dir);
    open_bob_store(&bob_state, &account_dir);

    // ---- 1. Baseline: Bob can decrypt. -----------------------------------
    let wire_before = alice_sends(&alice_state, "message sent before the lock").unwrap();
    assert_eq!(
        bob_decrypts(&bob_state, &wire_before, "a7-msg-1").unwrap(),
        "message sent before the lock",
        "baseline: an unlocked Bob must decrypt"
    );

    // ---- 2. Baseline: Bob's at-rest store round-trips. --------------------
    let history_before =
        ipc::commands::cmd_osl_load_channel_history(&bob_state, CHANNEL.to_string(), None).unwrap();
    assert!(
        !history_before.is_empty(),
        "baseline: the decrypt above must have persisted into the open store"
    );

    // A ciphertext produced BEFORE the lock, decrypted only after it. This is
    // the one that proves the lock removed a capability Bob already had.
    let wire_after = alice_sends(&alice_state, "message Bob must not read while locked").unwrap();

    // ---- 3. Lock. ---------------------------------------------------------
    let report = session_lock::lock_session(&bob_state, SessionLockTrigger::Manual);

    // ---- 4. Decrypt must now FAIL. This is the load-bearing assertion, and
    //         it is checked BEFORE any bookkeeping so that a lock which only
    //         clears the file key (the shipped defect) fails HERE, on
    //         behaviour, rather than on a counter.
    let same_ciphertext = bob_decrypts(&bob_state, &wire_before, "a7-msg-2");
    assert!(
        same_ciphertext.is_err(),
        "a locked session decrypted a message it had already decrypted once: {same_ciphertext:?}"
    );
    let fresh_ciphertext = bob_decrypts(&bob_state, &wire_after, "a7-msg-3");
    assert!(
        fresh_ciphertext.is_err(),
        "a locked session decrypted a fresh message: {fresh_ciphertext:?}"
    );

    // ---- 4b. Now the bookkeeping, which says WHAT was removed. ------------
    assert!(!report.trigger_was_noop, "the lock must be a real transition");
    assert!(report.identity_cleared, "identity secret must be dropped");
    assert!(report.message_store_closed, "MessageStore must be closed");
    assert!(
        report.file_storage_key_cleared,
        "file storage key must be cleared"
    );
    assert_eq!(
        report.peer_ratchet_states_zeroized, 1,
        "the live per-peer ratchet record must be taken out and zeroized"
    );
    assert!(
        report.peer_entries_cleared >= 2,
        "the peer map (trust root) must be emptied, got {}",
        report.peer_entries_cleared
    );
    assert!(
        report.whitelist_scopes_cleared >= 1,
        "the whitelist must be emptied"
    );

    // ---- 5. The rest of the live surface is gone too. ---------------------
    assert!(
        bob_state.message_store.lock().unwrap().is_none(),
        "the store handle must not survive the lock"
    );
    assert!(
        ipc::main_password::get_file_storage_key().is_none(),
        "the file storage key must not survive the lock"
    );
    assert!(
        bob_state.peer_map.lock().unwrap().is_empty(),
        "the peer map must not survive the lock"
    );
    assert!(
        bob_state.whitelist_state.lock().unwrap().is_empty(),
        "the whitelist must not survive the lock"
    );
    let history_after =
        ipc::commands::cmd_osl_load_channel_history(&bob_state, CHANNEL.to_string(), None).unwrap();
    assert!(
        history_after.is_empty(),
        "a locked session still served decrypted history: {} rows",
        history_after.len()
    );

    // ---- 6. Unlock restores the session. ---------------------------------
    ipc::main_password::set_file_storage_key_after_main_password_unlock(file_key);
    let unlock = session_lock::unlock_session(&bob_state, &account_dir).expect("unlock");
    assert!(
        unlock.reload.errors.is_empty(),
        "unlock reported reload errors: {:?}",
        unlock.reload.errors
    );
    assert!(unlock.identity_reloaded, "identity must come back from disk");
    assert!(
        unlock.message_store_reopened,
        "the message store must reopen"
    );
    assert!(
        unlock.reload.peer_map_entries >= 2,
        "the peer map must come back from disk"
    );

    assert_eq!(
        bob_decrypts(&bob_state, &wire_after, "a7-msg-4").unwrap(),
        "message Bob must not read while locked",
        "unlock must restore decryption, not brick the session"
    );

    reset_process_globals();
}

/// The idle trigger, end to end through the command boundary.
///
/// Before this fix the inactivity path's only consequence was clearing the
/// file storage key — decrypt kept working. Now an expired idle window makes
/// the command itself run the real lock and refuse.
#[test]
fn an_expired_idle_window_locks_the_session_at_the_command_boundary() {
    let _globals = serialize_process_globals();
    reset_process_globals();
    let tmp = TempDir::new().expect("tempdir");
    let account_dir = tmp.path().join("account");
    std::fs::create_dir_all(&account_dir).unwrap();
    keystore::set_base_dir_override(Some(account_dir.clone()));
    keystore::set_active_account_dir(Some(account_dir.clone()));

    ipc::main_password::set_file_storage_key_after_main_password_unlock([0xA7u8; 32]);
    let (alice_state, bob_state) = setup_alice_bob_dm();
    let wire = alice_sends(&alice_state, "idle-window message").unwrap();

    // Armed and fresh: the command boundary lets the decrypt through.
    session_lock::arm_idle_lock();
    assert_eq!(
        bob_decrypts(&bob_state, &wire, "a7-idle-1").unwrap(),
        "idle-window message"
    );

    // Push last-activity back past the window. `Instant` cannot go before the
    // process/boot epoch, so skip rather than panic on a just-booted host.
    let stale = Instant::now()
        .checked_sub(Duration::from_secs(session_lock::SESSION_IDLE_LOCK_SECONDS + 60));
    let Some(stale) = stale else {
        eprintln!("skipping: host clock cannot express a stale Instant");
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        session_lock::disarm_idle_lock();
        return;
    };
    session_lock::arm_idle_lock_at(stale);

    let refused = bob_decrypts(&bob_state, &wire, "a7-idle-2");
    assert_eq!(
        refused.as_ref().err().map(String::as_str),
        Some(session_lock::SESSION_LOCKED_ERROR),
        "an idle session must refuse the decrypt, got {refused:?}"
    );
    // The refusal above is a gate; this is the proof that the gate was backed
    // by an actual wipe. `lock_session` disarms the idle clock, so the guard
    // no longer fires — a second decrypt now has to fail because the secrets
    // are gone, not because a flag said so.
    let still_locked = bob_decrypts(&bob_state, &wire, "a7-idle-3");
    assert!(
        still_locked.is_err(),
        "the idle lock left the session able to decrypt: {still_locked:?}"
    );
    assert_ne!(
        still_locked.as_ref().err().map(String::as_str),
        Some(session_lock::SESSION_LOCKED_ERROR),
        "the second refusal must come from missing secrets, not the idle gate"
    );
    assert!(
        !bob_state.has_identity(),
        "the idle path must run the real lock, not just clear the file key"
    );
    assert!(
        ipc::main_password::get_file_storage_key().is_none(),
        "the idle lock must clear the file storage key too"
    );

    reset_process_globals();
}
