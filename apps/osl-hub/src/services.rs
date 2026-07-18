use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::{
    DemoConnectionState, EmailProvider, LinkedAccountDemo, LinkedServiceDemo, ServiceCategory,
    ServiceKind, ServiceLaunchState,
};

const REGISTRY_VERSION: u8 = 3;
const MAX_REGISTRY_BYTES: u64 = 64 * 1024;
const MAX_ACCOUNTS_PER_SERVICE: usize = 10;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountRecord {
    service_id: ServiceKind,
    id: String,
    label: String,
    /// Absent only on legacy v1/v2 rows. Those rows stay quarantined: silently
    /// assigning a browser profile to whichever OSL identity opens the new
    /// OSL Privacy first would expose that profile under the wrong cryptographic user.
    #[serde(default)]
    owner_osl_user_id: Option<String>,
    #[serde(default)]
    provider: Option<EmailProvider>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryDocument {
    version: u8,
    accounts: Vec<AccountRecord>,
}

/// Local metadata for isolated service profiles. It intentionally stores no
/// credentials, cookies, tokens, claimed handles, or authentication state.
pub struct ServiceRegistryState {
    path: PathBuf,
    cache: Mutex<RegistryCache>,
    next_id: AtomicU64,
}

#[derive(Default)]
struct RegistryCache {
    loaded: bool,
    accounts: Vec<AccountRecord>,
}

impl ServiceRegistryState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            // The file-storage key does not exist until the trusted password
            // gate is unlocked. Never weaken locked startup by accepting a
            // plaintext ownership registry before then.
            cache: Mutex::new(RegistryCache::default()),
            next_id: AtomicU64::new(1),
        }
    }

    fn locked_cache(&self) -> Result<std::sync::MutexGuard<'_, RegistryCache>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "service registry is unavailable".to_owned())?;
        if !cache.loaded {
            cache.accounts = load_protected_registry(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }

    pub fn list_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<LinkedServiceDemo>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        Ok(service_registry(&cache.accounts, owner_osl_user_id))
    }

    pub fn create_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        label: String,
    ) -> Result<LinkedAccountDemo, String> {
        self.create_with_provider_for_owner(owner_osl_user_id, service_id, label, None)
    }

    pub fn create_with_provider_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        label: String,
        provider: Option<EmailProvider>,
    ) -> Result<LinkedAccountDemo, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let descriptor = service_descriptor(service_id);
        if descriptor.launch_state != ServiceLaunchState::Available {
            return Err("this service is coming soon and cannot create login profiles".to_owned());
        }
        let provider = match (service_id, provider) {
            (ServiceKind::Email, provider) => Some(provider.unwrap_or_default()),
            (_, None) => None,
            (_, Some(_)) => {
                return Err("email provider is valid only for Email profiles".to_owned())
            }
        };
        let label = label.trim();
        if label.is_empty()
            || label.len() > 40
            || label.chars().any(|character| character.is_control())
        {
            return Err("account label must be 1-40 printable characters".to_string());
        }

        let mut cache = self.locked_cache()?;
        if cache
            .accounts
            .iter()
            .filter(|account| {
                account.service_id == service_id
                    && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
            })
            .count()
            >= MAX_ACCOUNTS_PER_SERVICE
        {
            return Err("this service already has the maximum number of profiles".to_string());
        }

        let record = AccountRecord {
            service_id,
            id: new_account_id(&self.next_id),
            label: label.to_string(),
            owner_osl_user_id: Some(owner_osl_user_id.to_owned()),
            provider,
        };
        let mut updated = cache.accounts.clone();
        updated.push(record.clone());
        write_registry(&self.path, &updated)
            .map_err(|_| "could not save the isolated account profile".to_string())?;
        cache.accounts = updated;
        Ok(account_dto(&record))
    }

    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub fn remove_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<bool, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let mut cache = self.locked_cache()?;
        let mut updated = cache.accounts.clone();
        let before = updated.len();
        updated.retain(|account| {
            account.service_id != service_id
                || account.id.as_str() != account_id
                || account.owner_osl_user_id.as_deref() != Some(owner_osl_user_id)
        });
        let removed = updated.len() != before;
        if removed {
            write_registry(&self.path, &updated)
                .map_err(|_| "could not update the isolated account profile".to_string())?;
            cache.accounts = updated;
        }
        Ok(removed)
    }

    /// Authorize a command-boundary operation without revealing whether a
    /// profile exists for a different local OSL identity.
    pub fn require_owned(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<(), String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        if cache.accounts.iter().any(|account| {
            account.service_id == service_id
                && account.id == account_id
                && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
        }) {
            Ok(())
        } else {
            Err("service account is not registered for the active OSL identity".to_owned())
        }
    }

    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub(crate) fn email_provider_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<Option<EmailProvider>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        let account = cache
            .accounts
            .iter()
            .find(|account| {
                account.service_id == service_id
                    && account.id == account_id
                    && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
            })
            .ok_or_else(|| {
                "service account is not registered for the active OSL identity".to_owned()
            })?;
        Ok(account.provider)
    }
}

pub fn service_kind_from_id(service_id: &str) -> Option<ServiceKind> {
    Some(match service_id {
        "discord" => ServiceKind::Discord,
        "telegram" => ServiceKind::Telegram,
        "whatsapp" => ServiceKind::WhatsApp,
        "instagram" => ServiceKind::Instagram,
        "snapchat" => ServiceKind::Snapchat,
        "email" => ServiceKind::Email,
        "x" => ServiceKind::X,
        "signal" => ServiceKind::Signal,
        "slack" => ServiceKind::Slack,
        "linkedin" => ServiceKind::Linkedin,
        "teams" => ServiceKind::Teams,
        "messenger" => ServiceKind::Messenger,
        _ => return None,
    })
}

fn new_account_id(counter: &AtomicU64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = counter.fetch_add(1, Ordering::Relaxed);
    format!("acct-{now:x}-{sequence:x}")
}

fn sanitize_accounts(accounts: Vec<AccountRecord>) -> Vec<AccountRecord> {
    let mut clean = Vec::new();
    for mut account in accounts {
        account.provider = match account.service_id {
            ServiceKind::Email => Some(account.provider.unwrap_or_default()),
            _ => None,
        };
        if clean.len() >= 100
            || !valid_account_id(&account.id)
            || account.label.trim().is_empty()
            || account.label.len() > 40
            || account
                .label
                .chars()
                .any(|character| character.is_control())
            || account
                .owner_osl_user_id
                .as_deref()
                .is_some_and(|owner| validate_owner_osl_user_id(owner).is_err())
            || clean.iter().any(|existing: &AccountRecord| {
                existing.service_id == account.service_id && existing.id == account.id
            })
            || clean
                .iter()
                .filter(|existing| {
                    existing.service_id == account.service_id
                        && existing.owner_osl_user_id == account.owner_osl_user_id
                })
                .count()
                >= MAX_ACCOUNTS_PER_SERVICE
        {
            continue;
        }
        clean.push(account);
    }
    clean
}

fn validate_owner_osl_user_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("active OSL identity is invalid".to_owned());
    }
    Ok(())
}

fn valid_account_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn load_protected_registry(path: &Path) -> Result<Vec<AccountRecord>, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())?;
    let backup = path.with_extension("bak");
    let primary = read_bounded(path)?;
    let fallback = read_bounded(&backup)?;

    for (bytes, recover) in [(primary.as_deref(), false), (fallback.as_deref(), true)] {
        let Some(bytes) = bytes else { continue };
        if !ipc::main_password::has_enc_magic(bytes) {
            continue;
        }
        if let Ok(document) = decode_registry(bytes, &key) {
            if recover {
                fs::copy(&backup, path)
                    .map_err(|_| "service registry backup could not be recovered".to_owned())?;
            }
            if document.version != REGISTRY_VERSION {
                return Err("service registry version is unsupported".to_owned());
            }
            return Ok(sanitize_accounts(document.accounts));
        }
    }

    let encrypted_present = primary
        .as_deref()
        .is_some_and(ipc::main_password::has_enc_magic)
        || fallback
            .as_deref()
            .is_some_and(ipc::main_password::has_enc_magic);
    if encrypted_present {
        return Err("service registry authentication failed".to_owned());
    }
    // Previous builds stored owner rows as unauthenticated JSON. Never leave
    // those account labels or owner identifiers on disk in plaintext. Keep a
    // sealed migration archive for explicit local recovery, but never
    // auto-claim or trust its contents.
    quarantine_plaintext(path, primary.as_deref(), &key)?;
    quarantine_plaintext(&backup, fallback.as_deref(), &key)?;
    Ok(Vec::new())
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_REGISTRY_BYTES => {
            fs::read(path)
                .map(Some)
                .map_err(|_| "service registry could not be read".to_owned())
        }
        Ok(_) => Err("service registry is not a bounded regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("service registry metadata could not be read".to_owned()),
    }
}

fn decode_registry(bytes: &[u8], key: &[u8; 32]) -> Result<RegistryDocument, String> {
    let plain = ipc::main_password::decrypt_at_rest(bytes, key)
        .map_err(|_| "service registry decrypt failed".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "service registry is malformed".to_owned())
}

fn quarantine_plaintext(
    path: &Path,
    plaintext: Option<&[u8]>,
    key: &[u8; 32],
) -> Result<(), String> {
    let Some(plaintext) = plaintext else {
        return Ok(());
    };
    let mut quarantine_name = path.as_os_str().to_os_string();
    // Keep a final extension so `atomic_file`'s `.bak`/`.tmp` companions can
    // never collide with the live registry's own backup path.
    quarantine_name.push(".legacy-encrypted.bin");
    let quarantine = PathBuf::from(quarantine_name);
    let sealed = ipc::main_password::encrypt_at_rest(plaintext, key)
        .map_err(|_| "legacy service registry could not be sealed".to_owned())?;
    crate::atomic_file::write_recoverable(
        &quarantine,
        &sealed,
        "legacy encrypted service registry",
    )?;
    fs::remove_file(path)
        .map_err(|_| "legacy plaintext service registry could not be removed".to_owned())
}

fn write_registry(path: &Path, accounts: &[AccountRecord]) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())?;
    let bytes = serde_json::to_vec(&RegistryDocument {
        version: REGISTRY_VERSION,
        accounts: accounts.to_vec(),
    })
    .map_err(|_| "service registry could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_REGISTRY_BYTES {
        return Err("service registry exceeds limit".to_owned());
    }
    let sealed = ipc::main_password::encrypt_at_rest(&bytes, &key)
        .map_err(|_| "service registry could not be encrypted".to_owned())?;
    crate::atomic_file::write_recoverable(path, &sealed, "service registry")
}

fn account_dto(account: &AccountRecord) -> LinkedAccountDemo {
    LinkedAccountDemo {
        id: account.id.clone(),
        label: account.label.clone(),
        display_handle: "Sign in on the service".to_string(),
        state: DemoConnectionState::NotLinked,
        provider: account.provider,
    }
}

fn service_registry(accounts: &[AccountRecord], owner_osl_user_id: &str) -> Vec<LinkedServiceDemo> {
    service_descriptors()
        .into_iter()
        .map(|descriptor| LinkedServiceDemo {
            id: descriptor.id,
            display_name: descriptor.display_name.to_string(),
            sidebar_glyph: descriptor.sidebar_glyph.to_string(),
            sidebar_order: descriptor.sidebar_order,
            category: descriptor.category,
            launch_state: descriptor.launch_state,
            supports_native_preview: descriptor.launch_state == ServiceLaunchState::Available,
            supports_protected_preview: descriptor.launch_state == ServiceLaunchState::Available,
            accounts: accounts
                .iter()
                .filter(|account| {
                    account.service_id == descriptor.id
                        && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
                })
                .map(account_dto)
                .collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceDescriptor {
    pub id: ServiceKind,
    pub display_name: &'static str,
    pub sidebar_glyph: &'static str,
    pub sidebar_order: u8,
    pub category: ServiceCategory,
    pub launch_state: ServiceLaunchState,
}

pub fn service_descriptor(id: ServiceKind) -> ServiceDescriptor {
    service_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.id == id)
        .expect("every ServiceKind has a descriptor")
}

fn service_descriptors() -> [ServiceDescriptor; 12] {
    use ServiceCategory::{Consumer, Enterprise};
    use ServiceLaunchState::{Available, ComingSoon};
    [
        descriptor(
            ServiceKind::Discord,
            "Discord",
            "DC",
            10,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::Telegram,
            "Telegram",
            "TG",
            20,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::WhatsApp,
            "WhatsApp",
            "WA",
            25,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::Instagram,
            "Instagram",
            "IG",
            30,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::Messenger,
            "Facebook Messenger",
            "MS",
            40,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::Snapchat,
            "Snapchat",
            "SC",
            50,
            Consumer,
            Available,
        ),
        descriptor(ServiceKind::X, "X", "X", 60, Consumer, Available),
        descriptor(ServiceKind::Email, "Email", "EM", 70, Consumer, Available),
        // Signal has no first-party web messaging client. Listing it truthfully
        // is safer than embedding an unofficial client under the Signal name.
        descriptor(
            ServiceKind::Signal,
            "Signal",
            "SG",
            80,
            Consumer,
            ComingSoon,
        ),
        descriptor(
            ServiceKind::Slack,
            "Slack",
            "SL",
            90,
            Enterprise,
            ComingSoon,
        ),
        descriptor(
            ServiceKind::Linkedin,
            "LinkedIn messaging",
            "LI",
            95,
            Enterprise,
            ComingSoon,
        ),
        descriptor(
            ServiceKind::Teams,
            "Microsoft Teams",
            "TM",
            100,
            Enterprise,
            ComingSoon,
        ),
    ]
}

const fn descriptor(
    id: ServiceKind,
    display_name: &'static str,
    sidebar_glyph: &'static str,
    sidebar_order: u8,
    category: ServiceCategory,
    launch_state: ServiceLaunchState,
) -> ServiceDescriptor {
    ServiceDescriptor {
        id,
        display_name,
        sidebar_glyph,
        sidebar_order,
        category,
        launch_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER_A: &str = "osl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWNER_B: &str = "osl_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TEST_KEY: [u8; 32] = [0x5a; 32];

    fn temporary_registry() -> PathBuf {
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));
        std::env::temp_dir().join(format!(
            "osl-hub-service-registry-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn new_registry_has_twelve_services_and_no_fake_accounts() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let services = state.list_for_owner(OWNER_A).unwrap();
        assert_eq!(services.len(), 12);
        assert!(services.iter().all(|service| service.accounts.is_empty()));
        let signal = services
            .iter()
            .find(|service| service.id == ServiceKind::Signal)
            .unwrap();
        assert_eq!(signal.launch_state, ServiceLaunchState::ComingSoon);
        assert!(!signal.supports_native_preview);
        assert!(services
            .windows(2)
            .all(|pair| pair[0].sidebar_order < pair[1].sidebar_order));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn created_profiles_persist_without_credentials_or_claimed_login() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .unwrap();
        assert_eq!(account.state, DemoConnectionState::NotLinked);
        assert_eq!(account.display_handle, "Sign in on the service");
        assert_eq!(account.provider, None);
        let bytes = fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&bytes));
        assert!(!bytes
            .windows("Personal".len())
            .any(|window| window == b"Personal"));
        assert_eq!(
            ServiceRegistryState::load(path.clone())
                .list_for_owner(OWNER_A)
                .unwrap()
                .into_iter()
                .find(|service| service.id == ServiceKind::Discord)
                .unwrap()
                .accounts
                .len(),
            1
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_is_scoped_to_service_and_account() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .unwrap();
        assert!(!state
            .remove_for_owner(OWNER_A, ServiceKind::Instagram, &account.id)
            .unwrap());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_ok());
        assert!(state
            .remove_for_owner(OWNER_A, ServiceKind::Discord, &account.id)
            .unwrap());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn string_service_ids_map_exactly_without_aliases() {
        assert_eq!(service_kind_from_id("discord"), Some(ServiceKind::Discord));
        assert_eq!(
            service_kind_from_id("whatsapp"),
            Some(ServiceKind::WhatsApp)
        );
        assert_eq!(service_kind_from_id("x"), Some(ServiceKind::X));
        assert_eq!(service_kind_from_id("signal"), Some(ServiceKind::Signal));
        assert_eq!(
            service_kind_from_id("linkedin"),
            Some(ServiceKind::Linkedin)
        );
        assert_eq!(service_kind_from_id("Discord"), None);
        assert_eq!(service_kind_from_id("discord.com"), None);
        assert_eq!(service_kind_from_id("../discord"), None);
    }

    #[test]
    fn failed_create_never_leaves_a_phantom_in_memory_account() {
        let parent_file = temporary_registry().with_extension("blocked-parent");
        fs::write(&parent_file, b"not a directory").unwrap();
        let state = ServiceRegistryState::load(parent_file.join("registry.json"));
        assert!(state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .is_err());
        assert!(state.list_for_owner(OWNER_A).is_err());
        let _ = fs::remove_file(parent_file);
    }

    #[test]
    fn multiple_email_profiles_preserve_fixed_providers_across_restart() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let gmail = state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Email,
                "Personal Gmail".to_owned(),
                Some(EmailProvider::Gmail),
            )
            .unwrap();
        let proton = state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Email,
                "Private Proton".to_owned(),
                Some(EmailProvider::Proton),
            )
            .unwrap();
        assert_ne!(gmail.id, proton.id);
        assert_eq!(gmail.provider, Some(EmailProvider::Gmail));
        assert_eq!(proton.provider, Some(EmailProvider::Proton));

        let reloaded = ServiceRegistryState::load(path.clone());
        assert_eq!(
            reloaded
                .email_provider_for_owner(OWNER_A, ServiceKind::Email, &gmail.id)
                .unwrap(),
            Some(EmailProvider::Gmail)
        );
        assert_eq!(
            reloaded
                .email_provider_for_owner(OWNER_A, ServiceKind::Email, &proton.id)
                .unwrap(),
            Some(EmailProvider::Proton)
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn arbitrary_provider_binding_and_coming_soon_profiles_fail_closed() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Discord,
                "Bad binding".to_owned(),
                Some(EmailProvider::Tuta),
            )
            .is_err());
        for unavailable in [
            ServiceKind::Signal,
            ServiceKind::Slack,
            ServiceKind::Linkedin,
            ServiceKind::Teams,
        ] {
            assert!(state
                .create_for_owner(OWNER_A, unavailable, "Unavailable".to_owned())
                .is_err());
        }
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .iter()
            .all(|service| service.accounts.is_empty()));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_unowned_account_is_quarantined_during_version_three_migration() {
        let path = temporary_registry();
        fs::write(
            &path,
            br#"{
              "version": 1,
              "accounts": [{
                "serviceId": "email",
                "id": "acct-existing",
                "label": "Existing email"
              }]
            }"#,
        )
        .unwrap();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .into_iter()
            .all(|service| service.accounts.is_empty()));
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Email, "acct-existing")
            .is_err());
        state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owned".to_owned())
            .unwrap();
        let archive = PathBuf::from(format!("{}.legacy-encrypted.bin", path.display()));
        let archived = fs::read(&archive).unwrap();
        assert!(ipc::main_password::has_enc_magic(&archived));
        assert!(String::from_utf8(
            ipc::main_password::decrypt_at_rest(&archived, &TEST_KEY).unwrap()
        )
        .unwrap()
        .contains("acct-existing"));
        assert!(!PathBuf::from(format!("{}.legacy-untrusted", path.display())).exists());
        let sealed = fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&sealed));
        let migrated: serde_json::Value = serde_json::from_slice(
            &ipc::main_password::decrypt_at_rest(&sealed, &TEST_KEY).unwrap(),
        )
        .unwrap();
        assert_eq!(migrated["version"], REGISTRY_VERSION);
        assert!(migrated["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["id"] != "acct-existing"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn complete_backup_recovers_a_crash_between_registry_replacements() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_owned())
            .unwrap();
        let backup = path.with_extension("bak");
        fs::rename(&path, &backup).unwrap();

        let recovered = ServiceRegistryState::load(path.clone());
        assert!(recovered
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_ok());
        assert!(path.exists());
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(backup);
    }

    #[test]
    fn two_owners_cannot_cross_list_or_authorize_open() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account_a = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owner A".to_owned())
            .unwrap();
        let account_b = state
            .create_for_owner(OWNER_B, ServiceKind::Discord, "Owner B".to_owned())
            .unwrap();

        let listed_a = state
            .list_for_owner(OWNER_A)
            .unwrap()
            .into_iter()
            .find(|service| service.id == ServiceKind::Discord)
            .unwrap()
            .accounts;
        let listed_b = state
            .list_for_owner(OWNER_B)
            .unwrap()
            .into_iter()
            .find(|service| service.id == ServiceKind::Discord)
            .unwrap()
            .accounts;
        assert_eq!(
            listed_a.iter().map(|row| &row.id).collect::<Vec<_>>(),
            vec![&account_a.id]
        );
        assert_eq!(
            listed_b.iter().map(|row| &row.id).collect::<Vec<_>>(),
            vec![&account_b.id]
        );

        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account_a.id)
            .is_ok());
        assert!(state
            .require_owned(OWNER_B, ServiceKind::Discord, &account_b.id)
            .is_ok());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account_b.id)
            .is_err());
        assert!(state
            .require_owned(OWNER_B, ServiceKind::Discord, &account_a.id)
            .is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reload_preserves_the_per_owner_profile_limit() {
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        for index in 0..MAX_ACCOUNTS_PER_SERVICE {
            state
                .create_for_owner(OWNER_A, ServiceKind::Discord, format!("Owner A {index}"))
                .unwrap();
            state
                .create_for_owner(OWNER_B, ServiceKind::Discord, format!("Owner B {index}"))
                .unwrap();
        }

        let reloaded = ServiceRegistryState::load(path.clone());
        for owner in [OWNER_A, OWNER_B] {
            let profiles = reloaded
                .list_for_owner(owner)
                .unwrap()
                .into_iter()
                .find(|service| service.id == ServiceKind::Discord)
                .unwrap()
                .accounts;
            assert_eq!(profiles.len(), MAX_ACCOUNTS_PER_SERVICE);
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tampered_encrypted_registry_fails_closed() {
        let path = temporary_registry();
        ServiceRegistryState::load(path.clone())
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owner A".to_owned())
            .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        fs::write(&path, bytes).unwrap();
        let reloaded = ServiceRegistryState::load(path.clone());
        assert!(reloaded.list_for_owner(OWNER_A).is_err());
        assert!(path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn plaintext_primary_and_backup_get_distinct_encrypted_archive_paths() {
        let path = temporary_registry();
        let backup = path.with_extension("bak");
        fs::write(&path, br#"{"version":2,"accounts":[]}"#).unwrap();
        fs::write(&backup, br#"{"version":2,"accounts":[]}"#).unwrap();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .iter()
            .all(|row| row.accounts.is_empty()));
        for original in [&path, &backup] {
            assert!(!original.exists());
            let archive = PathBuf::from(format!("{}.legacy-encrypted.bin", original.display()));
            let bytes = fs::read(archive).unwrap();
            assert!(ipc::main_password::has_enc_magic(&bytes));
            assert!(ipc::main_password::decrypt_at_rest(&bytes, &TEST_KEY).is_ok());
        }
    }
}
