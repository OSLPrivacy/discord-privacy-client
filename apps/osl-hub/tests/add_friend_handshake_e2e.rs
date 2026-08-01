//! Headless proof of the **only working add-a-friend path** on the shipping
//! app: manual invite-code exchange in BOTH directions, safety-number
//! verification on each side, and per-friend chat approval on each side.
//!
//! Companion document: `/home/liamw/osl-plan/ADD-FRIEND-PROCEDURE.md`. Every
//! numbered step in that document that is marked EXECUTED is executed here.
//!
//! What is driven, in the order the shipping commands drive it:
//!
//! ```text
//! security::export_friend_code            <- "Copy invite"
//! security::add_friend_code               <- "Add person"        (add_hub_friend)
//! security::verify_friend_safety_number   <- "Accept"            (verify_hub_friend_safety_number)
//! security::manual_peer_binding           <- open the chat
//! broker::activate_owned_osl_chat_context <- activate_osl_chat_context
//! security::set_manual_peer_scope_permission <- "Enable"         (set_active_hub_friend_permission)
//! security::set_scope_security            <- decrypted display   (set_active_hub_context_security)
//! broker::prepare_peer_prose_text         <- send                (prepare_peer_prose_text)
//! broker::open_peer_prose_text            <- receive             (open_peer_prose_text)
//! ```
//!
//! The one thing that is NOT the real binary is the cipher store: the send leg
//! uploads ciphertext bytes and the receive leg fetches them back. A loopback
//! server mirroring exactly the two endpoints
//! `crates/ipc/src/cipher_store_client.rs` calls (`POST /v1/blob`,
//! `GET /v1/blob/{id}`) stands in for `cipher-store-cf`. Everything above it —
//! trust, key material, scope approval, encryption and decryption — is the
//! product's own code.
//!
//! ## Why this test exists
//!
//! The handshake is symmetric and nothing said so. `peer_attachment_network_e2e.rs`
//! sets up the same handshake but only as scaffolding for attachments: it never
//! asserts that half a handshake fails, so every gate in it could rot open
//! without a red test. This file asserts the refusals, not just the successes.
//!
//! Gated behind `#[ignore]` **and** `OSL_ADD_FRIEND_PROOF=1` in the style of the
//! repo's other real-profile tests: it binds a loopback TCP port and writes two
//! real encrypted account profiles to a temp root.
//!
//! ## Discipline
//!
//! Plaintext is compared with `assert!(a == b, "<static message>")`, never
//! `assert_eq!`, so a failure cannot print message content.

#![cfg(feature = "core")]

use serde_json::json;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, thread};

const TEST_MAIN_PASSWORD: &str = "add-friend-handshake-proof-password";
const ENV_GATE: &str = "OSL_ADD_FRIEND_PROOF";

/// One of the four TTLs `is_valid_ttl` accepts in the cipher-store client.
const SCOPE_TTL_SECONDS: u32 = 3600;

const ALICE_TO_BOB: &str = "handshake proof: first message, alice to bob";
const BOB_TO_ALICE: &str = "handshake proof: reply, bob to alice";

// ---------------------------------------------------------------------------
// Loopback cipher store. Mirrors only the two endpoints the prose-token send
// and receive legs actually call.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct StoreState {
    /// id_hex -> (fetch token hex, ciphertext bytes)
    blobs: HashMap<String, (String, Vec<u8>)>,
    next_id: u64,
}

struct CipherStoreFixture {
    base_url: String,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<StoreState>>,
}

impl CipherStoreFixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        let port = listener.local_addr().expect("cipher store addr").port();
        listener
            .set_nonblocking(true)
            .expect("cipher store nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(StoreState::default()));
        let thread_stop = Arc::clone(&stop);
        let thread_state = Arc::clone(&state);
        thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let state = Arc::clone(&thread_state);
                        thread::spawn(move || serve(stream, state));
                    }
                    Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            stop,
            state,
        }
    }

    fn blob_count(&self) -> usize {
        self.state.lock().expect("store state").blobs.len()
    }

    /// Delete every stored blob, standing in for TTL expiry or a burn. Used to
    /// prove the receive leg really reads the store rather than a local copy.
    fn wipe(&self) {
        self.state.lock().expect("store state").blobs.clear();
    }
}

impl Drop for CipherStoreFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve(mut stream: TcpStream, state: Arc<Mutex<StoreState>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone cipher store stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();

    let mut content_length = 0usize;
    let mut fetch_token = String::new();
    let mut ttl_seconds: i64 = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        let (name, value) = match trimmed.split_once(':') {
            Some((name, value)) => (name.trim().to_ascii_lowercase(), value.trim().to_owned()),
            None => continue,
        };
        match name.as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "x-osl-fetch-token" => fetch_token = value,
            "x-osl-ttl-seconds" => ttl_seconds = value.parse().unwrap_or(0),
            _ => {}
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    let response = match (method.as_str(), path.as_str()) {
        ("POST", "/v1/blob") => {
            let mut guard = state.lock().expect("store state");
            guard.next_id += 1;
            let id_hex = format!("{:016x}", guard.next_id);
            guard
                .blobs
                .insert(id_hex.clone(), (fetch_token.clone(), body));
            let expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64
                + ttl_seconds;
            json_response(200, &json!({ "id": id_hex, "expires_at": expires_at }).to_string())
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id_hex = path.trim_start_matches("/v1/blob/").to_owned();
            let guard = state.lock().expect("store state");
            match guard.blobs.get(&id_hex) {
                None => status_response(404, "not found"),
                // Same capability rule the worker enforces: a mismatched or
                // absent token is 403, never a body.
                Some((token, _)) if *token != fetch_token => status_response(403, "forbidden"),
                Some((_, bytes)) => bytes_response(bytes),
            }
        }
        ("DELETE", path) if path.starts_with("/v1/blob/") => {
            let id_hex = path.trim_start_matches("/v1/blob/").to_owned();
            state.lock().expect("store state").blobs.remove(&id_hex);
            status_response(200, "ok")
        }
        _ => status_response(404, "not found"),
    };
    let _ = stream.write_all(&response);
    let _ = stream.flush();
}

fn json_response(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn status_response(status: u16, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} X\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn bytes_response(bytes: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    )
    .into_bytes();
    response.extend_from_slice(bytes);
    response
}

// ---------------------------------------------------------------------------
// Two isolated installs in one temp root.
// ---------------------------------------------------------------------------

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-add-friend-handshake-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated OSL test root");
        keystore::set_base_dir_override(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated OSL main password");
        Self { root }
    }

    fn account(&self, name: &str, cipher_store_url: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir(&dir).expect("create isolated OSL account dir");
        fs::write(
            dir.join("keyserver.json"),
            serde_json::to_vec(&json!({ "cipher_store_url": cipher_store_url })).unwrap(),
        )
        .expect("write isolated cipher-store configuration");
        dir
    }

    fn activate(dir: &Path) {
        keystore::set_active_account_dir(Some(dir.to_owned()));
    }
}

impl Drop for TestStorage {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// One fresh install: its own account directory, its own identity, its own
/// People/preferences files, its own broker.
struct Install {
    dir: PathBuf,
    osl_user_id: String,
    core: osl_privacy_hub::core_bridge::HubCoreState,
    security: osl_privacy_hub::security::HubSecurityState,
    broker: osl_privacy_hub::broker::HubBrokerState,
    /// What "Copy invite" puts on the clipboard.
    invite: String,
    /// The number `export_friend_code` derives from this install's OWN bundle.
    /// This is the value the OTHER side must type. See finding F2.
    own_safety_number: String,
}

impl Install {
    fn new(storage: &TestStorage, name: &str, cipher_store_url: &str) -> Self {
        let dir = storage.account(name, cipher_store_url);
        let identity = keystore::generate_identity(format!("osl-{name}-add-friend-proof"));
        let osl_user_id = identity.user_id.clone();
        let core = osl_privacy_hub::core_bridge::HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(identity);
        TestStorage::activate(&dir);
        let exported =
            osl_privacy_hub::security::export_friend_code(&core).expect("export friend code");
        Self {
            dir,
            osl_user_id,
            core,
            security: osl_privacy_hub::security::HubSecurityState::default(),
            broker: osl_privacy_hub::broker::HubBrokerState::default(),
            invite: exported.friend_code,
            own_safety_number: exported.safety_number,
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }

    fn people(&self) -> Vec<osl_privacy_hub::security::PersonDto> {
        self.activate();
        osl_privacy_hub::security::list_people(&self.core).expect("list people")
    }
}

fn gate_enabled() -> bool {
    std::env::var(ENV_GATE).as_deref() == Ok("1")
}

// ---------------------------------------------------------------------------
// The proof.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "real-profile handshake proof: writes two encrypted profiles and binds a loopback port; set OSL_ADD_FRIEND_PROOF=1"]
fn two_fresh_installs_reach_encrypted_messaging_only_after_a_symmetric_handshake() {
    assert!(
        gate_enabled(),
        "set {ENV_GATE}=1 to run the add-friend handshake proof"
    );

    let store = CipherStoreFixture::start();
    let storage = TestStorage::new();

    // -----------------------------------------------------------------------
    // STEP 1-2 — two fresh installs, each with an identity and nothing else.
    // -----------------------------------------------------------------------
    let alice = Install::new(&storage, "alice", &store.base_url);
    let bob = Install::new(&storage, "bob", &store.base_url);

    assert!(
        alice.people().is_empty(),
        "a fresh install must know nobody"
    );
    assert!(bob.people().is_empty(), "a fresh install must know nobody");
    assert_ne!(
        alice.osl_user_id, bob.osl_user_id,
        "the two installs must be different identities"
    );

    // -----------------------------------------------------------------------
    // STEP 3 — "Copy invite" produces a signed OSLFR1 code.
    // -----------------------------------------------------------------------
    assert!(
        alice.invite.starts_with("OSLFR1."),
        "the invite must be an OSLFR1 friend code"
    );
    assert!(
        bob.invite.starts_with("OSLFR1."),
        "the invite must be an OSLFR1 friend code"
    );
    assert_ne!(
        alice.invite, bob.invite,
        "two installs must not share an invite"
    );

    // FINDING F2, pinned as an assertion: the safety number is derived from ONE
    // bundle, so the two devices do NOT show the same number. Whoever changes
    // this to the symmetric two-party derivation will land here first and must
    // update the procedure document at the same time.
    assert_ne!(
        alice.own_safety_number, bob.own_safety_number,
        "v2 safety numbers are per-bundle, so the two sides differ; if this now \
         matches, the derivation became two-party and ADD-FRIEND-PROCEDURE.md \
         step 7 must be rewritten"
    );

    // -----------------------------------------------------------------------
    // STEP 4 — Alice pastes Bob's invite. Adding alone grants nothing.
    // -----------------------------------------------------------------------
    alice.activate();
    let bob_on_alice = osl_privacy_hub::security::add_friend_code(
        &alice.core,
        &alice.security,
        bob.invite.clone(),
        Some("Bob".to_owned()),
    )
    .expect("alice adds bob's invite");
    assert!(
        matches!(
            bob_on_alice.disposition,
            osl_privacy_hub::security::AddFriendDisposition::Added
        ),
        "a first add must report Added"
    );
    assert!(
        bob_on_alice.code_signature_valid,
        "the invite must be self-signature-verified on add"
    );
    assert!(
        !bob_on_alice.safety_number_verified,
        "adding an invite must never mark a friend verified"
    );
    assert_eq!(
        bob_on_alice.osl_user_id, bob.osl_user_id,
        "the added record must carry the sender's real OSL id"
    );
    // The number Alice's device shows for Bob is derived from BOB's bundle, so
    // it equals the number Bob's own install exported for itself.
    assert_eq!(
        bob_on_alice.safety_number, bob.own_safety_number,
        "the friend's number on this device is derived from THEIR bundle"
    );

    // Gate 1: an added-but-unverified friend cannot be opened at all.
    let unverified = osl_privacy_hub::security::manual_peer_binding(
        &alice.core,
        bob_on_alice.person_id.clone(),
    );
    assert!(
        unverified.is_err(),
        "an unverified friend must not produce a messaging binding"
    );

    // -----------------------------------------------------------------------
    // STEP 5 — the wrong number is refused.
    //
    // This is the assertion that catches the on-screen instruction being wrong
    // (finding F2): the code Bob would "read back" from HIS screen is the one
    // his device derives for ALICE, and typing that here must fail.
    // -----------------------------------------------------------------------
    bob.activate();
    let alice_on_bob_preview = osl_privacy_hub::security::add_friend_code(
        &bob.core,
        &bob.security,
        alice.invite.clone(),
        Some("Alice".to_owned()),
    )
    .expect("bob adds alice's invite");
    let number_bobs_screen_shows = alice_on_bob_preview.safety_number.clone();

    alice.activate();
    let wrong = osl_privacy_hub::security::verify_friend_safety_number(
        &alice.core,
        &alice.security,
        bob_on_alice.person_id.clone(),
        number_bobs_screen_shows.clone(),
    );
    assert!(
        wrong.is_err(),
        "verification must refuse a number derived from the other side's bundle"
    );
    assert!(
        !alice
            .people()
            .iter()
            .any(|person| person.safety_number_verified),
        "a refused verification must not mark anyone verified"
    );

    // -----------------------------------------------------------------------
    // STEP 6-7 — Alice verifies Bob with the number Bob's install exported.
    // -----------------------------------------------------------------------
    alice.activate();
    let verified = osl_privacy_hub::security::verify_friend_safety_number(
        &alice.core,
        &alice.security,
        bob_on_alice.person_id.clone(),
        bob.own_safety_number.clone(),
    )
    .expect("alice verifies bob");
    assert!(
        verified.safety_number_verified,
        "verification must set the persisted flag"
    );

    // -----------------------------------------------------------------------
    // STEP 8 — open the chat, and prove the send is still refused until the
    // per-friend chat approval is given.
    // -----------------------------------------------------------------------
    let binding = osl_privacy_hub::security::manual_peer_binding(
        &alice.core,
        bob_on_alice.person_id.clone(),
    )
    .expect("alice binds a messaging context to bob");
    assert_eq!(
        binding.peer_osl_user_id, bob.osl_user_id,
        "the binding must encrypt to the real peer identity"
    );
    let alice_context = osl_privacy_hub::broker::activate_owned_osl_chat_context(
        &alice.broker,
        &alice.osl_user_id,
        binding,
    )
    .expect("alice opens an OSL chat with bob");
    assert_eq!(alice_context.lease.service_id, "osl-chat");

    assert!(
        !osl_privacy_hub::security::manual_peer_scope_approved(
            &alice.core,
            &alice_context.lease.service_id,
            &alice_context.lease.account_id,
            alice_context.person_id.clone(),
            alice_context.scope.clone(),
        )
        .expect("read scope approval"),
        "opening a chat must not approve it"
    );

    // Gate 2: no approval, no send.
    let refused = osl_privacy_hub::broker::prepare_peer_prose_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        &alice_context.lease.context_token,
        ALICE_TO_BOB.to_owned(),
        false,
    );
    assert!(
        refused.is_err(),
        "a send must be refused while the chat is unapproved"
    );
    assert_eq!(
        store.blob_count(),
        0,
        "a refused send must not upload ciphertext"
    );

    // -----------------------------------------------------------------------
    // STEP 9-10 — approve the chat and turn on decrypted display.
    // -----------------------------------------------------------------------
    osl_privacy_hub::security::set_manual_peer_scope_permission(
        &alice.core,
        &alice.security,
        &alice_context.lease.service_id,
        &alice_context.lease.account_id,
        alice_context.person_id.clone(),
        alice_context.scope.clone(),
        true,
    )
    .expect("alice approves the chat with bob");
    assert!(
        osl_privacy_hub::security::manual_peer_scope_approved(
            &alice.core,
            &alice_context.lease.service_id,
            &alice_context.lease.account_id,
            alice_context.person_id.clone(),
            alice_context.scope.clone(),
        )
        .expect("read scope approval"),
        "approval must be readable back through the product's own predicate"
    );
    osl_privacy_hub::security::set_scope_security(
        &alice.security,
        alice_context.scope.clone(),
        SCOPE_TTL_SECONDS,
        true,
    )
    .expect("alice enables decrypted display for this chat");

    // -----------------------------------------------------------------------
    // STEP 11 — Alice can now send. Bob still cannot receive: he has done
    // nothing on his side beyond adding her, so the handshake is not complete.
    // -----------------------------------------------------------------------
    let sent = osl_privacy_hub::broker::prepare_peer_prose_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        &alice_context.lease.context_token,
        ALICE_TO_BOB.to_owned(),
        false,
    )
    .expect("alice sends an encrypted message to bob");
    assert!(
        sent.person_to_person_e2ee,
        "the send must report person-to-person end-to-end encryption"
    );
    assert!(
        !sent.cover_text.contains(ALICE_TO_BOB),
        "the carrier text must not contain the plaintext"
    );
    assert_eq!(
        store.blob_count(),
        1,
        "the send must place exactly one ciphertext blob in the store"
    );

    bob.activate();
    let bob_unverified_binding = osl_privacy_hub::security::manual_peer_binding(
        &bob.core,
        alice_on_bob_preview.person_id.clone(),
    );
    assert!(
        bob_unverified_binding.is_err(),
        "bob has not verified alice, so he cannot open a context to her"
    );

    // -----------------------------------------------------------------------
    // STEP 12-15 — Bob completes the mirror-image half of the handshake.
    // -----------------------------------------------------------------------
    let alice_on_bob = osl_privacy_hub::security::verify_friend_safety_number(
        &bob.core,
        &bob.security,
        alice_on_bob_preview.person_id.clone(),
        alice.own_safety_number.clone(),
    )
    .expect("bob verifies alice");
    assert!(alice_on_bob.safety_number_verified);

    let bob_binding = osl_privacy_hub::security::manual_peer_binding(
        &bob.core,
        alice_on_bob_preview.person_id.clone(),
    )
    .expect("bob binds a messaging context to alice");
    assert_eq!(
        bob_binding.peer_osl_user_id, alice.osl_user_id,
        "bob's binding must resolve to alice's real identity"
    );
    let bob_context = osl_privacy_hub::broker::activate_owned_osl_chat_context(
        &bob.broker,
        &bob.osl_user_id,
        bob_binding,
    )
    .expect("bob opens an OSL chat with alice");
    osl_privacy_hub::security::set_manual_peer_scope_permission(
        &bob.core,
        &bob.security,
        &bob_context.lease.service_id,
        &bob_context.lease.account_id,
        bob_context.person_id.clone(),
        bob_context.scope.clone(),
        true,
    )
    .expect("bob approves the chat with alice");
    osl_privacy_hub::security::set_scope_security(
        &bob.security,
        bob_context.scope.clone(),
        SCOPE_TTL_SECONDS,
        true,
    )
    .expect("bob enables decrypted display for this chat");

    // -----------------------------------------------------------------------
    // STEP 16 — the message Alice already sent now opens on Bob's install.
    // -----------------------------------------------------------------------
    let opened = osl_privacy_hub::broker::open_peer_prose_text(
        &bob.core,
        &bob.security,
        &bob.broker,
        &bob_context.lease.context_token,
        alice_on_bob_preview.person_id.clone(),
        sent.cover_text.clone(),
    )
    .expect("bob opens alice's encrypted message");
    assert!(
        opened.plaintext == ALICE_TO_BOB,
        "bob must recover alice's exact plaintext"
    );
    assert!(
        opened.person_to_person_e2ee && opened.context_verified,
        "the opened message must report a verified person-to-person context"
    );

    // -----------------------------------------------------------------------
    // STEP 17 — the reverse direction works too, on the same approvals.
    // -----------------------------------------------------------------------
    let reply = osl_privacy_hub::broker::prepare_peer_prose_text(
        &bob.core,
        &bob.security,
        &bob.broker,
        &bob_context.lease.context_token,
        BOB_TO_ALICE.to_owned(),
        false,
    )
    .expect("bob replies to alice");
    assert_eq!(
        store.blob_count(),
        2,
        "the reply must place its own ciphertext blob"
    );

    alice.activate();
    let opened_reply = osl_privacy_hub::broker::open_peer_prose_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        &alice_context.lease.context_token,
        bob_on_alice.person_id.clone(),
        reply.cover_text.clone(),
    )
    .expect("alice opens bob's reply");
    assert!(
        opened_reply.plaintext == BOB_TO_ALICE,
        "alice must recover bob's exact plaintext"
    );

    // -----------------------------------------------------------------------
    // STEP 18 — the carrier really is a pointer: with the store emptied, the
    // same cover text no longer opens. This proves the receive leg is reading
    // ciphertext the sender uploaded, not a local echo.
    // -----------------------------------------------------------------------
    store.wipe();
    bob.activate();
    let gone = osl_privacy_hub::broker::open_peer_prose_text(
        &bob.core,
        &bob.security,
        &bob.broker,
        &bob_context.lease.context_token,
        alice_on_bob_preview.person_id.clone(),
        sent.cover_text.clone(),
    );
    assert!(
        gone.is_err(),
        "with the ciphertext gone, the carrier alone must not open anything"
    );
}
