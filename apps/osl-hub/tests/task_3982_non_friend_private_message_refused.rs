#![cfg(feature = "core")]

use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration};

const TEST_MAIN_PASSWORD: &str = "task-3982-non-friend-private-message";
const SCOPE_TTL_SECONDS: u32 = 3600;
const EXACT_WORDS: &str = "task 3982 exact private words";
const STRANGER_PERSON_ID: &str = "task3982-stranger";
const NOT_FRIEND_REFUSAL: &str = "OSL sender is not a friend";

#[derive(Default)]
struct StoreState {
    blobs: HashMap<String, (String, Vec<u8>)>,
    next_id: u64,
    fetches: usize,
}

struct CipherStoreFixture {
    base_url: String,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<StoreState>>,
}

impl CipherStoreFixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        let port = listener.local_addr().expect("cipher store address").port();
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
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
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
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.trim().parse().unwrap_or(0),
            "x-osl-fetch-token" => fetch_token = value.trim().to_owned(),
            "x-osl-ttl-seconds" => ttl_seconds = value.trim().parse().unwrap_or(0),
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
            guard.blobs.insert(id_hex.clone(), (fetch_token, body));
            let expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_secs() as i64
                + ttl_seconds;
            json_response(
                200,
                &json!({ "id": id_hex, "expires_at": expires_at }).to_string(),
            )
        }
        ("GET", path) if path.starts_with("/v1/blob/") => {
            let id_hex = path.trim_start_matches("/v1/blob/").to_owned();
            let mut guard = state.lock().expect("store state");
            guard.fetches += 1;
            match guard.blobs.get(&id_hex) {
                None => status_response(404, "not found"),
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
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    )
    .into_bytes()
    .into_iter()
    .chain(bytes.iter().copied())
    .collect()
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-3982-{}-{}",
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

struct Install {
    dir: PathBuf,
    osl_user_id: String,
    core: osl_privacy_hub::core_bridge::HubCoreState,
    security: osl_privacy_hub::security::HubSecurityState,
    broker: osl_privacy_hub::broker::HubBrokerState,
    invite: String,
}

impl Install {
    fn new(storage: &TestStorage, name: &str, cipher_store_url: &str) -> Self {
        let dir = storage.account(name, cipher_store_url);
        let identity = keystore::generate_identity(format!("osl-task-3982-{name}"));
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
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }
}

struct OpenContext {
    context_token: String,
    sender_person_id: String,
    sender_osl_user_id: String,
}

fn open_chat_from(receiver: &Install, sender: &Install) -> OpenContext {
    receiver.activate();
    let friend = osl_privacy_hub::security::add_friend_code(
        &receiver.core,
        &receiver.security,
        sender.invite.clone(),
        Some(format!("sender {}", sender.osl_user_id)),
    )
    .expect("add sender friend code");
    osl_privacy_hub::security::verify_friend_safety_number(
        &receiver.core,
        &receiver.security,
        friend.person_id.clone(),
        friend.safety_number.clone(),
    )
    .expect("verify sender safety number");
    let binding =
        osl_privacy_hub::security::manual_peer_binding(&receiver.core, friend.person_id.clone())
            .expect("manual peer binding");
    assert_eq!(binding.peer_osl_user_id, sender.osl_user_id);
    let activated = osl_privacy_hub::broker::activate_owned_osl_chat_context(
        &receiver.broker,
        &receiver.osl_user_id,
        binding,
    )
    .expect("activate OSL Chat context");
    osl_privacy_hub::security::set_manual_peer_scope_permission(
        &receiver.core,
        &receiver.security,
        "osl-chat",
        "osl-main",
        activated.person_id.clone(),
        activated.scope.clone(),
        true,
    )
    .expect("approve OSL Chat scope");
    osl_privacy_hub::security::set_scope_security(
        &receiver.security,
        activated.scope,
        SCOPE_TTL_SECONDS,
        true,
    )
    .expect("enable decrypted display");
    OpenContext {
        context_token: activated.lease.context_token,
        sender_person_id: friend.person_id,
        sender_osl_user_id: sender.osl_user_id.clone(),
    }
}

fn approve_sender_to_recipient(sender: &Install, recipient: &Install) {
    sender.activate();
    let friend = osl_privacy_hub::security::add_friend_code(
        &sender.core,
        &sender.security,
        recipient.invite.clone(),
        Some(format!("recipient {}", recipient.osl_user_id)),
    )
    .expect("add recipient friend code");
    osl_privacy_hub::security::verify_friend_safety_number(
        &sender.core,
        &sender.security,
        friend.person_id.clone(),
        friend.safety_number.clone(),
    )
    .expect("verify recipient safety number");
    let binding =
        osl_privacy_hub::security::manual_peer_binding(&sender.core, friend.person_id.clone())
            .expect("manual peer binding");
    let activated = osl_privacy_hub::broker::activate_owned_osl_chat_context(
        &sender.broker,
        &sender.osl_user_id,
        binding,
    )
    .expect("activate OSL Chat context");
    osl_privacy_hub::security::set_manual_peer_scope_permission(
        &sender.core,
        &sender.security,
        "osl-chat",
        "osl-main",
        activated.person_id,
        activated.scope.clone(),
        true,
    )
    .expect("approve OSL Chat scope");
    osl_privacy_hub::security::set_scope_security(
        &sender.security,
        activated.scope,
        SCOPE_TTL_SECONDS,
        true,
    )
    .expect("enable decrypted display");
}

#[test]
fn task_3982_non_friend_private_message_refused_before_fetch() {
    let store = CipherStoreFixture::start();
    let storage = TestStorage::new();
    let alice = Install::new(&storage, "alice", &store.base_url);
    let bob = Install::new(&storage, "bob", &store.base_url);
    let bob_context = open_chat_from(&bob, &alice);
    approve_sender_to_recipient(&alice, &bob);

    let store_client = ipc::cipher_store_client::CipherStoreClient::new(store.base_url.clone())
        .expect("loopback cipher-store client");
    alice.activate();
    let prepared = osl_privacy_hub::broker::prepare_peer_prose_text_with_capture_and_store_client(
        &alice.core,
        &alice.security,
        &alice.broker,
        &alice
            .broker
            .active_osl_chat_context_token()
            .expect("alice active context"),
        EXACT_WORDS.to_owned(),
        false,
        false,
        &store_client,
    )
    .expect("alice prepares a protected pointer");
    assert_eq!(store.state.lock().expect("store state").fetches, 0);

    bob.activate();
    let refused = match osl_privacy_hub::broker::open_peer_prose_text(
        &bob.core,
        &bob.security,
        &bob.broker,
        &bob_context.context_token,
        STRANGER_PERSON_ID.to_owned(),
        prepared.cover_text.clone(),
    ) {
        Ok(opened) => panic!(
            "TASK3982 opened_private_messages=1 stranger_sender={} exact_plaintext=\"{}\"",
            STRANGER_PERSON_ID, opened.plaintext
        ),
        Err(error) => error,
    };
    let refused_store_fetches = store.state.lock().expect("store state").fetches;
    assert_eq!(refused, NOT_FRIEND_REFUSAL);
    assert_eq!(refused_store_fetches, 0);
    println!(
        "TASK3982 refused_opened_private_messages=0 refused_store_fetches={} refusal=\"{}\" stranger_sender={}",
        refused_store_fetches, refused, STRANGER_PERSON_ID
    );

    bob.activate();
    let opened = osl_privacy_hub::broker::open_peer_prose_text(
        &bob.core,
        &bob.security,
        &bob.broker,
        &bob_context.context_token,
        bob_context.sender_person_id.clone(),
        prepared.cover_text,
    )
    .expect("accepted friend opens the same protected pointer");
    let accepted_exact_matches = usize::from(opened.plaintext == EXACT_WORDS);
    assert_eq!(accepted_exact_matches, 1);
    println!(
        "TASK3982 accepted_friend_opened_private_messages=1 accepted_exact_matches={} sender_exact={} exact_plaintext=\"{}\"",
        accepted_exact_matches, bob_context.sender_osl_user_id, opened.plaintext
    );
}
