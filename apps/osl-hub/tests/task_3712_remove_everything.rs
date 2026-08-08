use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use ipc::scope::Scope;
use osl_privacy_hub::broker::{self, HubBrokerState};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::models::ServiceKind;
use osl_privacy_hub::remove_everything::{
    current_account_remove_everything, current_account_remove_everything_readiness,
};
use osl_privacy_hub::security::{self, HubSecurityState};
use osl_privacy_hub::service_host::{self, ServiceHostState};
use osl_privacy_hub::service_scope_index::ServiceScopeIndexState;
use osl_privacy_hub::services::ServiceRegistryState;
use serde::Serialize;
use sha2::{Digest, Sha256};

const OWNER: &str = "osl-owner-task-3712";
const SERVICE: &str = "email";
const STORE_KEY: &[u8; 32] = &[0x37; 32];

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
struct DataCounts {
    key_count: usize,
    message_count: usize,
    setting_count: usize,
    service_message_count: usize,
    file_count: usize,
    session_count: usize,
}

struct Fixture {
    _tmp: tempfile::TempDir,
    core: HubCoreState,
    security: HubSecurityState,
    registry: ServiceRegistryState,
    index: ServiceScopeIndexState,
    profile_root: PathBuf,
    message_store: store::MessageStore,
    current: AccountFixture,
    server: Option<thread::JoinHandle<Vec<String>>>,
}

struct AccountFixture {
    account_id: String,
    channel_id: String,
    context_binding: String,
    storage_key: String,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = tmp.path().join("account");
        std::fs::create_dir_all(&config).expect("config dir");
        keystore::set_base_dir_override(Some(config.clone()));
        keystore::set_active_account_dir(Some(config.clone()));
        ipc::main_password::set_main_password(&config, "task-3712-password")
            .expect("main password");

        let server = start_cipher_store();
        std::fs::write(
            config.join("keyserver.json"),
            format!(r#"{{"cipher_store_url":"{}"}}"#, server.0),
        )
        .expect("write cipher store override");

        let core = HubCoreState::default();
        *core.osl.identity.lock().expect("identity lock") = Some(keystore::identity_from_entropy(
            [0x37; 16],
            OWNER.to_owned(),
        ));
        let message_store = store::MessageStore::open(&tmp.path().join("message-store"), STORE_KEY)
            .expect("message store");
        let registry = ServiceRegistryState::load(config.join("service-registry.json"));
        let index = ServiceScopeIndexState::load(config.join("service-scope-index.json"));
        let broker = HubBrokerState::default();
        let host = ServiceHostState::default();
        let security = HubSecurityState::default();
        let profile_root = service_host::service_profiles_root(&tmp.path().join("app-local"));

        let current = seed_account(
            &core,
            &security,
            &registry,
            &index,
            &broker,
            &host,
            &message_store,
            &profile_root,
        );
        drop(broker);
        *core.osl.message_store.lock().expect("message store slot") = Some(message_store);
        let message_store = core
            .osl
            .message_store
            .lock()
            .expect("message store slot")
            .take()
            .expect("message store round trip");

        Self {
            _tmp: tmp,
            core,
            security,
            registry,
            index,
            profile_root,
            message_store,
            current,
            server: Some(server.1),
        }
    }

    fn counts(&self) -> DataCounts {
        DataCounts {
            key_count: broker::test_local_protected_binding_count(&self.current.context_binding)
                .expect("local protected count"),
            message_count: self
                .message_store
                .list_by_channel(&self.current.channel_id, 10)
                .expect("message count")
                .len(),
            setting_count: usize::from(
                self.index
                    .coverage(OWNER, SERVICE, &self.current.account_id)
                    .is_ok(),
            ),
            service_message_count: scope_blob_count(&self.current.storage_key),
            file_count: direct_child_count(
                &account_profile_path(&self.profile_root, &self.current.account_id).join("files"),
            ),
            session_count: direct_child_count(
                &account_profile_path(&self.profile_root, &self.current.account_id)
                    .join("sessions"),
            ),
        }
    }

    fn install_store(&self) {
        let reopened =
            store::MessageStore::open(&self._tmp.path().join("message-store"), STORE_KEY)
                .expect("reopen message store");
        *self
            .core
            .osl
            .message_store
            .lock()
            .expect("message store slot") = Some(reopened);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        *self
            .core
            .osl
            .message_store
            .lock()
            .expect("message store slot") = None;
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

#[test]
fn task3712_open_confirm_remove_everything_and_query_both_sides() {
    let fixture = Fixture::new();
    let before = fixture.counts();
    assert_eq!(before, one_each());
    let readiness = current_account_remove_everything_readiness(
        &fixture.core,
        &fixture.registry,
        &fixture.index,
        SERVICE,
        &fixture.current.account_id,
    )
    .expect("readiness before opening Remove everything");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("osl-hub has a repository root")
        .to_path_buf();
    let image_path =
        repo_root.join("apps/osl-hub-ui/screenshots/evidence/task-3712-remove-everything.png");
    let capture = Command::new("node")
        .arg(repo_root.join("apps/osl-hub-ui/screenshots/capture-task-3712-remove-everything.mjs"))
        .arg("--output")
        .arg(&image_path)
        .current_dir(&repo_root)
        .output()
        .expect("launch Remove everything screen capture");
    let capture_stdout = String::from_utf8_lossy(&capture.stdout);
    let capture_stderr = String::from_utf8_lossy(&capture.stderr);
    assert!(
        capture.status.success(),
        "Remove everything screen capture failed: stdout={capture_stdout}; stderr={capture_stderr}"
    );
    assert!(capture_stdout.contains("TASK3712_SCREEN_TREE_TITLE=Remove everything"));
    for control in ["local data", "service data", "Remove everything", "Cancel"] {
        assert!(capture_stdout.contains(&format!("TASK3712_SCREEN_TREE_CONTROL={control}")));
        assert!(capture_stdout.contains(&format!("TASK3712_IMAGE_LABEL={control}")));
    }
    assert!(capture_stdout.contains("blank=false"));
    assert!(capture_stdout.contains("TASK3712_UI_CONFIRM=Remove everything confirmed=true"));
    print!("{capture_stdout}");
    eprint!("{capture_stderr}");

    fixture.install_store();
    let removed = current_account_remove_everything(
        &fixture.core,
        &fixture.security,
        &fixture.registry,
        &fixture.index,
        &fixture.profile_root,
        SERVICE,
        &fixture.current.account_id,
        &readiness.burn_id,
    )
    .expect("confirmed Remove everything action");
    let after = fixture.counts();
    assert_eq!(after, zero_each());
    assert_eq!(removed.key_count_removed, 1);
    assert_eq!(removed.message_count_removed, 1);
    assert_eq!(removed.setting_count_removed, 1);
    assert_eq!(removed.service_message_count_removed, 1);
    assert_eq!(removed.file_count_removed, 1);
    assert_eq!(removed.session_count_removed, 1);

    println!(
        "TASK3712_COUNTS_BEFORE key={} message={} setting={} service_message={} file={} session={}",
        before.key_count,
        before.message_count,
        before.setting_count,
        before.service_message_count,
        before.file_count,
        before.session_count,
    );
    println!(
        "TASK3712_COUNTS_AFTER key={} message={} setting={} service_message={} file={} session={}",
        after.key_count,
        after.message_count,
        after.setting_count,
        after.service_message_count,
        after.file_count,
        after.session_count,
    );
}

fn seed_account(
    core: &HubCoreState,
    security: &HubSecurityState,
    registry: &ServiceRegistryState,
    index: &ServiceScopeIndexState,
    broker: &HubBrokerState,
    host: &ServiceHostState,
    message_store: &store::MessageStore,
    profile_root: &Path,
) -> AccountFixture {
    let account = registry
        .create_for_owner(OWNER, ServiceKind::Email, "task3712 current".to_owned())
        .expect("create service account");
    index
        .initialize_clean_account(OWNER, SERVICE, &account.id)
        .expect("initialize scope index");
    let lease = broker::activate_owned_local_loopback_context(
        broker,
        registry,
        host,
        OWNER,
        SERVICE,
        &account.id,
        "task3712-current-channel".to_owned(),
    )
    .expect("activate loopback context");
    broker::prepare_local_protected_text(
        core,
        broker,
        &lease.context_token,
        "protected".to_owned(),
    )
    .expect("prepare local protected text");
    let registration = broker
        .service_scope_registration(&lease.context_token)
        .expect("service scope registration");
    let context_binding = registration.local_context_binding_sha256.clone();
    index
        .with_registered_write(registration.clone(), || Ok(()))
        .expect("register scope write");
    let scope = Scope::try_from(registration.scope.clone()).expect("registered scope");
    let channel_id = scope.id.clone();
    message_store
        .put(&store::StoredMessage {
            discord_message_id: "task3712-message".to_owned(),
            channel_id: channel_id.clone(),
            sender_discord_id: "task3712-sender".to_owned(),
            sender_osl_user_id: OWNER.to_owned(),
            plaintext: "task3712 plaintext".to_owned(),
            decrypted_at: 1_903_712_000,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        })
        .expect("seed message row");
    security::record_peer_prose_blob(
        security,
        registration.scope,
        "3712371237123712".to_owned(),
        Some("0123456789abcdef0123456789abcdef".to_owned()),
    )
    .expect("record remote blob");
    let profile = account_profile_path(profile_root, &account.id);
    std::fs::create_dir_all(profile.join("files")).expect("profile files dir");
    std::fs::create_dir_all(profile.join("sessions")).expect("profile sessions dir");
    std::fs::write(profile.join("files/current.bin"), b"file").expect("profile file");
    std::fs::write(profile.join("sessions/current.json"), b"session").expect("profile session");

    AccountFixture {
        account_id: account.id,
        channel_id,
        context_binding,
        storage_key: scope.storage_key(),
    }
}

fn owner_namespace(owner: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-HUB/service-profile-owner/v1");
    hash.update(owner.as_bytes());
    let digest = hash.finalize();
    let mut namespace = String::from("owner-");
    for byte in &digest[..24] {
        use std::fmt::Write as _;
        write!(namespace, "{byte:02x}").expect("write owner namespace");
    }
    namespace
}

fn account_profile_path(service_profiles_root: &Path, account_id: &str) -> PathBuf {
    service_host::profile_path(
        &service_profiles_root.join(owner_namespace(OWNER)),
        SERVICE,
        account_id,
    )
    .expect("service profile path")
}

fn direct_child_count(path: &Path) -> usize {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.count(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => panic!("read profile directory: {error}"),
    }
}

fn scope_blob_count(storage_key: &str) -> usize {
    let path = keystore::osl_config_dir()
        .expect("config dir")
        .join("scope_blobs.json");
    ipc::scope_blobs_file::count_for(&ipc::scope_blobs_file::load(&path), storage_key)
}

fn one_each() -> DataCounts {
    DataCounts {
        key_count: 1,
        message_count: 1,
        setting_count: 1,
        service_message_count: 1,
        file_count: 1,
        session_count: 1,
    }
}

fn zero_each() -> DataCounts {
    DataCounts {
        key_count: 0,
        message_count: 0,
        setting_count: 0,
        service_message_count: 0,
        file_count: 0,
        session_count: 0,
    }
}

fn start_cipher_store() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback cipher store");
    let address = listener.local_addr().expect("cipher store address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let raw = read_http_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .expect("write response");
        vec![String::from_utf8_lossy(&raw).to_ascii_lowercase()]
    });
    (format!("http://{address}"), handle)
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let count = stream.read(&mut chunk).expect("read request");
        raw.extend_from_slice(&chunk[..count]);
        let Some(headers_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&raw[..headers_end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= headers_end + 4 + length {
            return raw;
        }
    }
}
