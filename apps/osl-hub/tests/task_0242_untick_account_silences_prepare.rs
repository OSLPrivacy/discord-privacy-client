#![cfg(feature = "core")]

use osl_privacy_hub::broker::{
    activate_owned_osl_chat_context, prepare_peer_prose_text, HubBrokerState,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::security::{
    add_friend_code, export_friend_code, manual_peer_binding, set_friend_account_reach_choice,
    set_manual_peer_scope_permission, set_scope_security, verify_friend_safety_number,
    HubSecurityState,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

const TEST_MAIN_PASSWORD: &str = "task-0242-untick-account-silences-prepare";
const FIRST_MESSAGE: &str = "TASK0242 first protected draft";
const SECOND_MESSAGE: &str = "TASK0242 second protected draft";
const SCOPE_TTL_SECONDS: u32 = 3600;

#[derive(Default)]
struct StoreState {
    blobs: BTreeMap<String, (String, Vec<u8>)>,
    next_id: u64,
}

struct CipherStoreFixture {
    base_url: String,
    stop: Arc<AtomicBool>,
    state: Arc<Mutex<StoreState>>,
    accepted_uploads: Arc<AtomicUsize>,
}

impl CipherStoreFixture {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
        let port = listener.local_addr().expect("cipher store address").port();
        listener
            .set_nonblocking(true)
            .expect("make cipher store nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(StoreState::default()));
        let accepted_uploads = Arc::new(AtomicUsize::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_state = Arc::clone(&state);
        let thread_uploads = Arc::clone(&accepted_uploads);
        thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&thread_state);
                        let uploads = Arc::clone(&thread_uploads);
                        thread::spawn(move || serve(stream, state, uploads));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
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
            accepted_uploads,
        }
    }

    fn upload_count(&self) -> usize {
        self.accepted_uploads.load(Ordering::SeqCst)
    }

    fn blob_count(&self) -> usize {
        self.state.lock().expect("store state").blobs.len()
    }
}

impl Drop for CipherStoreFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve(mut stream: TcpStream, state: Arc<Mutex<StoreState>>, accepted_uploads: Arc<AtomicUsize>) {
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
    let mut ttl_seconds = 0i64;
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
            guard
                .blobs
                .insert(id_hex.clone(), (fetch_token.clone(), body));
            accepted_uploads.fetch_add(1, Ordering::SeqCst);
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
            let guard = state.lock().expect("store state");
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
    let mut response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        bytes.len()
    )
    .into_bytes();
    response.extend_from_slice(bytes);
    response
}

struct TestStorage {
    root: PathBuf,
}

impl TestStorage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "osl-task-0242-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir(&root).expect("create isolated OSL root");
        keystore::set_base_dir_override(Some(root.clone()));
        ipc::main_password::set_file_storage_key(None);
        ipc::main_password::set_main_password(&root, TEST_MAIN_PASSWORD)
            .expect("set isolated main password");
        Self { root }
    }

    fn account(&self, name: &str, cipher_store_url: &str) -> PathBuf {
        let dir = self.root.join(name);
        fs::create_dir(&dir).expect("create isolated account dir");
        fs::write(
            dir.join("keyserver.json"),
            serde_json::to_vec(&json!({ "cipher_store_url": cipher_store_url })).unwrap(),
        )
        .expect("write cipher-store override");
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
    core: HubCoreState,
    security: HubSecurityState,
    broker: HubBrokerState,
    invite: String,
}

impl Install {
    fn new(storage: &TestStorage, name: &str, cipher_store_url: &str) -> Self {
        let dir = storage.account(name, cipher_store_url);
        let identity = keystore::generate_identity(format!("osl-{name}-task-0242"));
        let osl_user_id = identity.user_id.clone();
        let core = HubCoreState::default();
        *core.osl.identity.lock().unwrap() = Some(identity);
        TestStorage::activate(&dir);
        let invite = export_friend_code(&core)
            .expect("export friend code")
            .friend_code;
        Self {
            dir,
            osl_user_id,
            core,
            security: HubSecurityState::default(),
            broker: HubBrokerState::default(),
            invite,
        }
    }

    fn activate(&self) {
        TestStorage::activate(&self.dir);
    }
}

#[test]
fn task_0242_unticking_account_makes_osl_silent_on_next_prepare() {
    let store = CipherStoreFixture::start();
    let storage = TestStorage::new();
    let alice = Install::new(&storage, "alice", &store.base_url);
    let bob = Install::new(&storage, "bob", &store.base_url);

    alice.activate();
    let bob_on_alice = add_friend_code(
        &alice.core,
        &alice.security,
        bob.invite.clone(),
        Some("Bob 0242".to_owned()),
    )
    .expect("alice adds bob");
    let verified = verify_friend_safety_number(
        &alice.core,
        &alice.security,
        bob_on_alice.person_id.clone(),
        bob_on_alice.safety_number.clone(),
    )
    .expect("alice verifies bob");
    assert!(verified.safety_number_verified);

    let binding = manual_peer_binding(&alice.core, bob_on_alice.person_id.clone())
        .expect("bind verified friend");
    assert_eq!(binding.peer_osl_user_id, bob.osl_user_id);
    let context = activate_owned_osl_chat_context(&alice.broker, &alice.osl_user_id, binding)
        .expect("activate OSL chat context");
    set_manual_peer_scope_permission(
        &alice.core,
        &alice.security,
        &context.lease.service_id,
        &context.lease.account_id,
        context.person_id.clone(),
        context.scope.clone(),
        true,
    )
    .expect("approve the manual peer scope");
    set_scope_security(
        &alice.security,
        context.scope.clone(),
        SCOPE_TTL_SECONDS,
        true,
    )
    .expect("set a valid scope TTL");

    let before_first = store.upload_count();
    let ticked = set_friend_account_reach_choice(
        &alice.security,
        context.person_id.clone(),
        context.lease.service_id.clone(),
        context.lease.account_id.clone(),
        true,
    )
    .expect("tick the account");
    assert!(ticked.broadened);
    let first = prepare_peer_prose_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        &context.lease.context_token,
        FIRST_MESSAGE.to_owned(),
        false,
    )
    .expect("first prepare succeeds while account is ticked");
    let first_actions = store.upload_count().saturating_sub(before_first);
    assert!(first.person_to_person_e2ee);
    assert!(!first.cover_text.contains(FIRST_MESSAGE));
    assert_eq!(first_actions, 1);
    assert_eq!(store.blob_count(), 1);

    let before_second = store.upload_count();
    let unticked = set_friend_account_reach_choice(
        &alice.security,
        context.person_id.clone(),
        context.lease.service_id.clone(),
        context.lease.account_id.clone(),
        false,
    )
    .expect("untick the same account");
    assert!(!unticked.broadened);
    let second = prepare_peer_prose_text(
        &alice.core,
        &alice.security,
        &alice.broker,
        &context.lease.context_token,
        SECOND_MESSAGE.to_owned(),
        false,
    );
    let second_actions = store.upload_count().saturating_sub(before_second);
    let second_refusal = match second {
        Ok(_) => panic!("second prepare is refused after untick"),
        Err(error) => error,
    };
    assert_eq!(
        second_refusal,
        "Approve encryption for this friend before continuing"
    );
    assert_eq!(second_actions, 0);
    assert_eq!(store.blob_count(), 1);

    println!("TASK0242_TICKED_ACCOUNT={}", ticked.account_id);
    println!("TASK0242_FIRST_PREPARES={}", first.person_to_person_e2ee);
    println!("TASK0242_FIRST_OSL_ACTIONS={first_actions}");
    println!("TASK0242_UNTICKED_ACCOUNT={}", unticked.account_id);
    println!("TASK0242_SECOND_REFUSAL={second_refusal}");
    println!("TASK0242_SECOND_OSL_ACTIONS={second_actions}");
    println!("TASK0242_STORE_BLOBS_AFTER_SECOND={}", store.blob_count());
}
