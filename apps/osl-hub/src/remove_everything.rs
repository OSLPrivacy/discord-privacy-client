use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::core_bridge::HubCoreState;
use crate::models::ServiceKind;
use crate::security::{self, HubSecurityState};
use crate::service_scope_index::{ImmutableServiceBurnManifest, ServiceScopeIndexState};
use crate::services::{service_kind_from_id, ServiceRegistryState};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveEverythingResult {
    pub burn_id: String,
    pub key_count_removed: usize,
    pub message_count_removed: usize,
    pub setting_count_removed: usize,
    pub service_message_count_removed: usize,
    pub file_count_removed: usize,
    pub session_count_removed: usize,
    pub registry_removed: bool,
    pub profile_cleanup_pending: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveEverythingReadiness {
    pub burn_id: String,
    pub manifest_digest: String,
    pub indexed_scopes: usize,
}

pub fn current_account_remove_everything_readiness(
    core: &HubCoreState,
    registry: &ServiceRegistryState,
    index: &ServiceScopeIndexState,
    service_id: &str,
    account_id: &str,
) -> Result<RemoveEverythingReadiness, String> {
    let owner = active_owner(core)?;
    require_owned(registry, &owner, service_id, account_id)?;
    let manifest = index.preview_complete_manifest(&owner, service_id, account_id)?;
    Ok(RemoveEverythingReadiness {
        burn_id: hex(&manifest.burn_id),
        manifest_digest: hex(&manifest.manifest_digest),
        indexed_scopes: manifest.scopes.len(),
    })
}

pub fn current_account_remove_everything(
    core: &HubCoreState,
    security: &HubSecurityState,
    registry: &ServiceRegistryState,
    index: &ServiceScopeIndexState,
    service_profiles_root: &Path,
    service_id: &str,
    account_id: &str,
    confirmed_burn_id: &str,
) -> Result<RemoveEverythingResult, String> {
    let owner = active_owner(core)?;
    require_owned(registry, &owner, service_id, account_id)?;
    let preview = index.preview_complete_manifest(&owner, service_id, account_id)?;
    if confirmed_burn_id != hex(&preview.burn_id) {
        return Err("The remove-everything scope changed; review and confirm again".to_owned());
    }
    let manifest = index.freeze_complete_manifest(&owner, service_id, account_id)?;
    if manifest.burn_id != preview.burn_id {
        return Err("The remove-everything scope changed before it could be frozen".to_owned());
    }

    let profile_root = account_profile_root(service_profiles_root, &owner)?;
    let profile_path = crate::service_host::profile_path(&profile_root, service_id, account_id)
        .map_err(|_| "The isolated account profile path is invalid".to_owned())?;
    let before_file_count = direct_child_count(&profile_path.join("files"))?;
    let before_session_count = direct_child_count(&profile_path.join("sessions"))?;

    let BurnTotals {
        key_count_removed,
        message_count_removed,
        setting_count_removed,
        service_message_count_removed,
    } = burn_manifest(core, security, index, &manifest)?;

    let profile =
        crate::service_host::tombstone_profile_directory(&profile_root, service_id, account_id)
            .map_err(|_| "The isolated account profile could not be removed safely".to_owned())?;
    let service_kind =
        service_kind_from_id(service_id).ok_or_else(|| "unknown service".to_owned())?;
    let registry_removed = registry.remove_for_owner(&owner, service_kind, account_id)?;
    let setting_count_removed = setting_count_removed.saturating_add(usize::from(
        index.remove_account(&owner, service_id, account_id)?,
    ));

    Ok(RemoveEverythingResult {
        burn_id: hex(&manifest.burn_id),
        key_count_removed,
        message_count_removed,
        setting_count_removed,
        service_message_count_removed,
        file_count_removed: if profile.profile_existed {
            before_file_count
        } else {
            0
        },
        session_count_removed: if profile.profile_existed {
            before_session_count
        } else {
            0
        },
        registry_removed,
        profile_cleanup_pending: profile.cleanup_pending,
    })
}

struct BurnTotals {
    key_count_removed: usize,
    message_count_removed: usize,
    setting_count_removed: usize,
    service_message_count_removed: usize,
}

fn burn_manifest(
    core: &HubCoreState,
    security: &HubSecurityState,
    index: &ServiceScopeIndexState,
    manifest: &ImmutableServiceBurnManifest,
) -> Result<BurnTotals, String> {
    let mut totals = BurnTotals {
        key_count_removed: 0,
        message_count_removed: 0,
        setting_count_removed: 0,
        service_message_count_removed: 0,
    };
    for indexed in index.pending_scopes(manifest)? {
        let result = if let Some(person_id) = indexed.manual_peer_person_id.as_deref() {
            security::burn_manual_peer_scope(
                core,
                security,
                &manifest.service_id,
                &manifest.account_id,
                person_id,
                indexed.scope.clone(),
            )?
        } else {
            security::burn_scope(
                core,
                security,
                indexed.scope.clone(),
                indexed.canonical_channel_ids.clone(),
                true,
                Vec::new(),
            )?
        };
        totals.key_count_removed = totals.key_count_removed.saturating_add(
            crate::broker::burn_indexed_local_protected_binding(
                core,
                &indexed.local_context_binding_sha256,
            )?,
        );
        index.mark_scope_burned(manifest, &indexed.storage_key)?;
        totals.message_count_removed = totals
            .message_count_removed
            .saturating_add(result.rows_destroyed);
        totals.service_message_count_removed = totals
            .service_message_count_removed
            .saturating_add(result.remote_blobs_deleted);
    }
    index.finish_burn(manifest)?;
    Ok(totals)
}

fn active_owner(core: &HubCoreState) -> Result<String, String> {
    core.osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .as_ref()
        .map(|identity| identity.user_id.clone())
        .ok_or_else(|| "OSL identity is not loaded".to_owned())
}

fn require_owned(
    registry: &ServiceRegistryState,
    owner: &str,
    service_id: &str,
    account_id: &str,
) -> Result<ServiceKind, String> {
    let kind = service_kind_from_id(service_id).ok_or_else(|| "unknown service".to_owned())?;
    registry.require_owned(owner, kind, account_id)?;
    Ok(kind)
}

fn account_profile_root(service_profiles_root: &Path, owner: &str) -> Result<PathBuf, String> {
    crate::service_host::owner_profile_namespace(owner)
        .map(|namespace| service_profiles_root.join(namespace))
        .map_err(|_| "The isolated account profile owner is invalid".to_owned())
}

fn direct_child_count(path: &Path) -> Result<usize, String> {
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err("The isolated account profile could not be read".to_owned()),
    };
    let mut count = 0usize;
    for entry in entries {
        entry.map_err(|_| "The isolated account profile could not be read".to_owned())?;
        count = count.saturating_add(1);
    }
    Ok(count)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::thread;

    use ipc::scope::Scope;
    use serde::Serialize;

    use super::*;
    use crate::broker::{self, HubBrokerState};
    use crate::models::ServiceKind;
    use crate::service_host::{self, ServiceHostState};

    const OWNER: &str = "osl-owner-task-3710";
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
        survivor: AccountFixture,
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
            let _ = std::fs::remove_dir_all(std::env::temp_dir().join("task3710"));
            let tmp = tempfile::tempdir().expect("tempdir");
            let config = tmp.path().join("account");
            std::fs::create_dir_all(&config).expect("config dir");
            keystore::set_base_dir_override(Some(config.clone()));
            keystore::set_active_account_dir(Some(config.clone()));
            ipc::main_password::set_main_password(&config, "task-3710-password")
                .expect("main password");

            let server = start_cipher_store();
            std::fs::write(
                config.join("keyserver.json"),
                format!(r#"{{"cipher_store_url":"{}"}}"#, server.0),
            )
            .expect("write cipher store override");

            let core = HubCoreState::default();
            *core.osl.identity.lock().expect("identity lock") = Some(
                keystore::identity_from_entropy([0x37; 16], OWNER.to_owned()),
            );
            let message_store =
                store::MessageStore::open(&tmp.path().join("message-store"), STORE_KEY)
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
                "current",
                "task3710-current-channel",
                "1111111111111111",
                "aaaaaaaaaaaaaaaa",
                "0123456789abcdef0123456789abcdef",
            );
            let survivor = seed_account(
                &core,
                &security,
                &registry,
                &index,
                &broker,
                &host,
                &message_store,
                &profile_root,
                "survivor",
                "task3710-survivor-channel",
                "2222222222222222",
                "bbbbbbbbbbbbbbbb",
                "fedcba9876543210fedcba9876543210",
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
                survivor,
                server: Some(server.1),
            }
        }

        fn counts(&self, account: &AccountFixture) -> DataCounts {
            DataCounts {
                key_count: broker::test_local_protected_binding_count(&account.context_binding)
                    .expect("local protected count"),
                message_count: self
                    .message_store
                    .list_by_channel(&account.channel_id, 10)
                    .expect("message count")
                    .len(),
                setting_count: usize::from(
                    self.index
                        .coverage(OWNER, SERVICE, &account.account_id)
                        .is_ok(),
                ),
                service_message_count: scope_blob_count(&account.storage_key),
                file_count: direct_child_count(
                    &account_profile_path(&self.profile_root, &account.account_id).join("files"),
                )
                .expect("file count"),
                session_count: direct_child_count(
                    &account_profile_path(&self.profile_root, &account.account_id).join("sessions"),
                )
                .expect("session count"),
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
    fn one_confirmed_remove_everything_action_zeros_only_current_account_counts() {
        let _serial = crate::global_keystore_test_lock();
        let fixture = Fixture::new();

        let current_before = fixture.counts(&fixture.current);
        let survivor_before = fixture.counts(&fixture.survivor);
        assert_eq!(current_before, one_each());
        assert_eq!(survivor_before, one_each());

        let readiness = current_account_remove_everything_readiness(
            &fixture.core,
            &fixture.registry,
            &fixture.index,
            SERVICE,
            &fixture.current.account_id,
        )
        .expect("readiness");
        let stale = current_account_remove_everything(
            &fixture.core,
            &fixture.security,
            &fixture.registry,
            &fixture.index,
            &fixture.profile_root,
            SERVICE,
            &fixture.current.account_id,
            &"0".repeat(64),
        )
        .expect_err("stale confirmation must fail");
        assert!(
            stale.contains("review and confirm"),
            "unexpected stale-confirmation error: {stale}"
        );
        assert_eq!(fixture.counts(&fixture.current), one_each());

        fixture.install_store();
        let result = current_account_remove_everything(
            &fixture.core,
            &fixture.security,
            &fixture.registry,
            &fixture.index,
            &fixture.profile_root,
            SERVICE,
            &fixture.current.account_id,
            &readiness.burn_id,
        )
        .expect("confirmed remove everything");
        assert_eq!(result.key_count_removed, 1);
        assert_eq!(result.message_count_removed, 1);
        assert_eq!(result.setting_count_removed, 1);
        assert_eq!(result.service_message_count_removed, 1);
        assert_eq!(result.file_count_removed, 1);
        assert_eq!(result.session_count_removed, 1);

        let current_after = fixture.counts(&fixture.current);
        let survivor_after = fixture.counts(&fixture.survivor);
        assert_eq!(current_after, zero_each());
        assert_eq!(survivor_after, one_each());

        println!(
            "TASK3710 action=current_account_remove_everything stale_confirmation_error={:?} current_before={} current_after={} survivor_before={} survivor_after={} result_removed={}",
            stale,
            serde_json::to_string(&current_before).unwrap(),
            serde_json::to_string(&current_after).unwrap(),
            serde_json::to_string(&survivor_before).unwrap(),
            serde_json::to_string(&survivor_after).unwrap(),
            serde_json::to_string(&result).unwrap()
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
        label: &str,
        channel_id: &str,
        blob_id: &str,
        context_binding_fallback: &str,
        burn_capability: &str,
    ) -> AccountFixture {
        let account = registry
            .create_for_owner(OWNER, ServiceKind::Email, format!("task3710 {label}"))
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
            channel_id.to_owned(),
        )
        .expect("activate loopback context");
        broker::prepare_local_protected_text(
            core,
            broker,
            &lease.context_token,
            format!("task3710 protected {label}"),
        )
        .expect("prepare local protected text");
        let registration = broker
            .service_scope_registration(&lease.context_token)
            .expect("service scope registration");
        let context_binding = registration.local_context_binding_sha256.clone();
        assert_ne!(
            context_binding, context_binding_fallback,
            "fallback literal must not accidentally be the context hash"
        );
        index
            .with_registered_write(registration.clone(), || Ok(()))
            .expect("register scope write");
        let scope = Scope::try_from(registration.scope.clone()).expect("registered scope");
        let burn_channel_id = scope.id.clone();
        message_store
            .put(&store::StoredMessage {
                discord_message_id: format!("task3710-message-{label}"),
                channel_id: burn_channel_id.clone(),
                sender_discord_id: format!("task3710-sender-{label}"),
                sender_osl_user_id: OWNER.to_owned(),
                plaintext: format!("task3710 plaintext {label}"),
                decrypted_at: 1_903_710_000,
                burned: false,
            })
            .expect("seed message row");
        security::record_peer_prose_blob(
            security,
            registration.scope.clone(),
            blob_id.to_owned(),
            Some(burn_capability.to_owned()),
        )
        .expect("record remote blob");
        let profile = account_profile_path(profile_root, &account.id);
        std::fs::create_dir_all(profile.join("files")).expect("profile files dir");
        std::fs::create_dir_all(profile.join("sessions")).expect("profile sessions dir");
        std::fs::write(profile.join("files").join(format!("{label}.bin")), b"file")
            .expect("profile file");
        std::fs::write(
            profile.join("sessions").join(format!("{label}.json")),
            b"session",
        )
        .expect("profile session");

        AccountFixture {
            account_id: account.id,
            channel_id: burn_channel_id,
            context_binding,
            storage_key: scope.storage_key(),
        }
    }

    fn account_profile_path(service_profiles_root: &Path, account_id: &str) -> PathBuf {
        let owner_namespace =
            service_host::owner_profile_namespace(OWNER).expect("owner profile namespace");
        let owner_root = service_profiles_root.join(owner_namespace);
        service_host::profile_path(&owner_root, SERVICE, account_id).expect("service profile path")
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
            let mut requests = Vec::new();
            for _ in 0..1 {
                let (mut stream, _) = listener.accept().expect("accept request");
                let raw = read_http_request(&mut stream);
                requests.push(String::from_utf8_lossy(&raw).to_ascii_lowercase());
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
                    .expect("write response");
            }
            requests
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
}
