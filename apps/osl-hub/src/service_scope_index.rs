//! Encrypted, authenticated write-ahead index for service-account burn.
//!
//! This index is the authority for enumerating Hub cryptographic scopes by
//! OSL identity + service + service account. It never owns or deletes browser
//! profiles, cookies, native-app state, or login sessions.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ipc::scope::{Scope, ScopeInput};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INDEX_VERSION: u8 = 1;
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 128;
const MAX_SCOPES_PER_ACCOUNT: usize = 8_192;
const MAX_CHANNELS_PER_SCOPE: usize = 512;
const MAX_BURN_JOURNAL: usize = 8_192;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    LegacyIncomplete,
    CleanPostIndex,
    TrustedEnumeration,
}

impl CoverageState {
    pub fn complete(self) -> bool {
        matches!(self, Self::CleanPostIndex | Self::TrustedEnumeration)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceScopeRegistration {
    pub owner_osl_user_id: String,
    pub service_id: String,
    pub account_id: String,
    pub scope: ScopeInput,
    pub canonical_channel_ids: Vec<String>,
    pub local_context_binding_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_peer_person_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedServiceScope {
    pub storage_key: String,
    pub scope: ScopeInput,
    pub canonical_channel_ids: Vec<String>,
    pub local_context_binding_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_peer_person_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImmutableServiceBurnManifest {
    pub owner_osl_user_id: String,
    pub service_id: String,
    pub account_id: String,
    pub generation: u64,
    pub burn_id: [u8; 32],
    pub manifest_digest: [u8; 32],
    pub scopes: Vec<IndexedServiceScope>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BurnStepState {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BurnJournalRecord {
    manifest_digest: [u8; 32],
    steps: BTreeMap<String, BurnStepState>,
    complete: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AccountScopeIndex {
    owner_osl_user_id: String,
    service_id: String,
    account_id: String,
    generation: u64,
    frozen: bool,
    coverage: CoverageState,
    scopes: BTreeMap<String, IndexedServiceScope>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IndexDocument {
    version: u8,
    accounts: BTreeMap<String, AccountScopeIndex>,
    burn_journal: BTreeMap<String, BurnJournalRecord>,
}

#[derive(Default)]
struct IndexCache {
    loaded: bool,
    document: IndexDocument,
}

pub struct ServiceScopeIndexState {
    path: PathBuf,
    inner: Mutex<IndexCache>,
}

impl ServiceScopeIndexState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            inner: Mutex::new(IndexCache::default()),
        }
    }

    /// Must run before a newly-created service profile can perform any OSL
    /// scope write. Existing profiles must never call this migration shortcut.
    pub fn initialize_clean_account(
        &self,
        owner: &str,
        service: &str,
        account: &str,
    ) -> Result<(), String> {
        validate_account_binding(owner, service, account)?;
        let mut cache = self.lock_loaded()?;
        let key = account_key(owner, service, account);
        if cache.document.accounts.contains_key(&key) {
            return Err("OSL service scope coverage was already initialized".to_owned());
        }
        if cache.document.accounts.len() >= MAX_ACCOUNTS {
            return Err("OSL service scope index account limit reached".to_owned());
        }
        cache.document.accounts.insert(
            key,
            AccountScopeIndex {
                owner_osl_user_id: owner.to_owned(),
                service_id: service.to_owned(),
                account_id: account.to_owned(),
                generation: 1,
                frozen: false,
                coverage: CoverageState::CleanPostIndex,
                scopes: BTreeMap::new(),
            },
        );
        persist(&self.path, &cache.document)
    }

    /// Records the scope durably before executing `write`. Holding the index
    /// transition lock across the callback prevents app-burn from freezing an
    /// incomplete concurrent mutation.
    pub fn with_registered_write<T>(
        &self,
        registration: ServiceScopeRegistration,
        write: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        validate_registration(&registration)?;
        let mut cache = self.lock_loaded()?;
        let key = account_key(
            &registration.owner_osl_user_id,
            &registration.service_id,
            &registration.account_id,
        );
        if !cache.document.accounts.contains_key(&key) {
            if cache.document.accounts.len() >= MAX_ACCOUNTS {
                return Err("OSL service scope index account limit reached".to_owned());
            }
            // This account predates an explicit clean initialization. Even if
            // the first observed write is indexed, prior one-way-hashed scopes
            // cannot be disproved, so service burn remains unavailable.
            cache.document.accounts.insert(
                key.clone(),
                AccountScopeIndex {
                    owner_osl_user_id: registration.owner_osl_user_id.clone(),
                    service_id: registration.service_id.clone(),
                    account_id: registration.account_id.clone(),
                    generation: 1,
                    frozen: false,
                    coverage: CoverageState::LegacyIncomplete,
                    scopes: BTreeMap::new(),
                },
            );
        }
        let account = cache
            .document
            .accounts
            .get_mut(&key)
            .ok_or_else(|| "OSL service scope index account disappeared".to_owned())?;
        if account.frozen {
            return Err("OSL service account is frozen for cryptographic burn".to_owned());
        }
        let indexed = indexed_scope(&registration)?;
        if !account.scopes.contains_key(&indexed.storage_key)
            && account.scopes.len() >= MAX_SCOPES_PER_ACCOUNT
        {
            return Err("OSL service scope index scope limit reached".to_owned());
        }
        account.scopes.insert(indexed.storage_key.clone(), indexed);
        account.generation = account
            .generation
            .checked_add(1)
            .ok_or_else(|| "OSL service scope generation exhausted".to_owned())?;
        // Write-ahead durability: the callback is not entered unless the
        // encrypted, authenticated index has committed.
        persist(&self.path, &cache.document)?;
        let result = write();
        drop(cache);
        result
    }

    pub fn coverage(
        &self,
        owner: &str,
        service: &str,
        account: &str,
    ) -> Result<CoverageState, String> {
        let cache = self.lock_loaded()?;
        cache
            .document
            .accounts
            .get(&account_key(owner, service, account))
            .map(|entry| entry.coverage)
            .ok_or_else(|| "OSL service scope coverage is incomplete".to_owned())
    }

    pub fn remove_identity(&self, owner: &str) -> Result<usize, String> {
        if !valid_opaque(owner, 128) {
            return Err("OSL service scope identity binding is invalid".to_owned());
        }
        let mut cache = self.lock_loaded()?;
        if cache
            .document
            .accounts
            .values()
            .any(|account| account.owner_osl_user_id == owner && account.frozen)
        {
            return Err("OSL identity has a service burn in progress".to_owned());
        }
        let before = cache.document.accounts.len();
        cache
            .document
            .accounts
            .retain(|_, account| account.owner_osl_user_id != owner);
        let removed = before.saturating_sub(cache.document.accounts.len());
        if removed > 0 {
            persist(&self.path, &cache.document)?;
        }
        Ok(removed)
    }

    /// Freeze writes and return an immutable exact manifest. Incomplete legacy
    /// coverage fails before changing the frozen state.
    pub fn preview_complete_manifest(
        &self,
        owner: &str,
        service: &str,
        account: &str,
    ) -> Result<ImmutableServiceBurnManifest, String> {
        validate_account_binding(owner, service, account)?;
        let cache = self.lock_loaded()?;
        let indexed = cache
            .document
            .accounts
            .get(&account_key(owner, service, account))
            .ok_or_else(|| "OSL service scope coverage is incomplete".to_owned())?;
        if !indexed.coverage.complete() {
            return Err(
                "OSL cannot prove complete scope coverage for this legacy service account"
                    .to_owned(),
            );
        }
        if indexed.frozen {
            return build_manifest(indexed, indexed.generation);
        }
        let generation = indexed
            .generation
            .checked_add(1)
            .ok_or_else(|| "OSL service scope generation exhausted".to_owned())?;
        build_manifest(indexed, generation)
    }

    pub fn freeze_complete_manifest(
        &self,
        owner: &str,
        service: &str,
        account: &str,
    ) -> Result<ImmutableServiceBurnManifest, String> {
        validate_account_binding(owner, service, account)?;
        let mut cache = self.lock_loaded()?;
        let key = account_key(owner, service, account);
        let indexed = cache
            .document
            .accounts
            .get_mut(&key)
            .ok_or_else(|| "OSL service scope coverage is incomplete".to_owned())?;
        if !indexed.coverage.complete() {
            return Err(
                "OSL cannot prove complete scope coverage for this legacy service account"
                    .to_owned(),
            );
        }
        if indexed.frozen {
            let manifest = build_manifest(indexed, indexed.generation)?;
            if cache
                .document
                .burn_journal
                .contains_key(&hex(&manifest.burn_id))
            {
                return Ok(manifest);
            }
            return Err("OSL service account is frozen without a valid burn journal".to_owned());
        }
        let manifest = build_manifest(
            indexed,
            indexed
                .generation
                .checked_add(1)
                .ok_or_else(|| "OSL service scope generation exhausted".to_owned())?,
        )?;
        indexed.frozen = true;
        indexed.generation = manifest.generation;
        let scopes = &manifest.scopes;
        let manifest_digest = manifest.manifest_digest;
        let burn_id = manifest.burn_id;
        let journal_key = hex(&burn_id);
        if cache.document.burn_journal.len() >= MAX_BURN_JOURNAL
            && !cache.document.burn_journal.contains_key(&journal_key)
        {
            return Err("OSL service burn journal is full".to_owned());
        }
        let steps = scopes
            .iter()
            .map(|scope| (scope.storage_key.clone(), BurnStepState::Pending))
            .collect();
        cache
            .document
            .burn_journal
            .entry(journal_key)
            .or_insert(BurnJournalRecord {
                manifest_digest,
                steps,
                complete: false,
            });
        persist(&self.path, &cache.document)?;
        Ok(manifest)
    }

    pub fn pending_scopes(
        &self,
        manifest: &ImmutableServiceBurnManifest,
    ) -> Result<Vec<IndexedServiceScope>, String> {
        verify_manifest(manifest)?;
        let cache = self.lock_loaded()?;
        let journal = cache
            .document
            .burn_journal
            .get(&hex(&manifest.burn_id))
            .ok_or_else(|| "OSL service burn journal is missing".to_owned())?;
        if journal.manifest_digest != manifest.manifest_digest {
            return Err("OSL service burn manifest changed".to_owned());
        }
        Ok(manifest
            .scopes
            .iter()
            .filter(|scope| journal.steps.get(&scope.storage_key) != Some(&BurnStepState::Complete))
            .cloned()
            .collect())
    }

    pub fn mark_scope_burned(
        &self,
        manifest: &ImmutableServiceBurnManifest,
        storage_key: &str,
    ) -> Result<(), String> {
        verify_manifest(manifest)?;
        let mut cache = self.lock_loaded()?;
        let journal = cache
            .document
            .burn_journal
            .get_mut(&hex(&manifest.burn_id))
            .ok_or_else(|| "OSL service burn journal is missing".to_owned())?;
        let step = journal
            .steps
            .get_mut(storage_key)
            .ok_or_else(|| "OSL service burn scope is outside its manifest".to_owned())?;
        *step = BurnStepState::Complete;
        persist(&self.path, &cache.document)
    }

    /// Complete local cryptographic burn while deliberately retaining the
    /// service-account registry and its external login profile.
    pub fn finish_burn(&self, manifest: &ImmutableServiceBurnManifest) -> Result<(), String> {
        verify_manifest(manifest)?;
        let mut cache = self.lock_loaded()?;
        let journal_key = hex(&manifest.burn_id);
        let journal = cache
            .document
            .burn_journal
            .get_mut(&journal_key)
            .ok_or_else(|| "OSL service burn journal is missing".to_owned())?;
        if journal
            .steps
            .values()
            .any(|step| *step != BurnStepState::Complete)
        {
            return Err("OSL service burn still has pending scopes".to_owned());
        }
        journal.complete = true;
        let account = cache
            .document
            .accounts
            .get_mut(&account_key(
                &manifest.owner_osl_user_id,
                &manifest.service_id,
                &manifest.account_id,
            ))
            .ok_or_else(|| "OSL service burn account disappeared".to_owned())?;
        account.scopes.clear();
        account.frozen = false;
        account.generation = account
            .generation
            .checked_add(1)
            .ok_or_else(|| "OSL service scope generation exhausted".to_owned())?;
        // Coverage remains complete: the write barrier covered the entire
        // lifecycle and future writes must register again before mutation.
        persist(&self.path, &cache.document)
    }

    fn lock_loaded(&self) -> Result<std::sync::MutexGuard<'_, IndexCache>, String> {
        let mut cache = self
            .inner
            .lock()
            .map_err(|_| "OSL service scope index is unavailable".to_owned())?;
        if !cache.loaded {
            cache.document = load_document(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }
}

fn indexed_scope(registration: &ServiceScopeRegistration) -> Result<IndexedServiceScope, String> {
    let scope: Scope = registration
        .scope
        .clone()
        .try_into()
        .map_err(|_| "OSL service scope registration is invalid".to_owned())?;
    let mut channels = BTreeSet::new();
    channels.extend(registration.canonical_channel_ids.iter().cloned());
    Ok(IndexedServiceScope {
        storage_key: scope.storage_key(),
        scope: registration.scope.clone(),
        canonical_channel_ids: channels.into_iter().collect(),
        local_context_binding_sha256: registration.local_context_binding_sha256.clone(),
        manual_peer_person_id: registration.manual_peer_person_id.clone(),
    })
}

fn validate_registration(value: &ServiceScopeRegistration) -> Result<(), String> {
    validate_account_binding(
        &value.owner_osl_user_id,
        &value.service_id,
        &value.account_id,
    )?;
    if value.canonical_channel_ids.is_empty()
        || value.canonical_channel_ids.len() > MAX_CHANNELS_PER_SCOPE
        || value
            .canonical_channel_ids
            .iter()
            .any(|id| !valid_opaque(id, 160))
        || !valid_hex(&value.local_context_binding_sha256, 64)
    {
        return Err("OSL service scope registration is incomplete".to_owned());
    }
    let indexed = indexed_scope(value)?;
    validate_indexed_scope(&indexed, &value.service_id, &value.account_id)
}

fn validate_indexed_scope(
    indexed: &IndexedServiceScope,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    let scope: Scope = indexed
        .scope
        .clone()
        .try_into()
        .map_err(|_| "OSL indexed service scope is invalid".to_owned())?;
    if indexed.storage_key != scope.storage_key()
        || indexed.canonical_channel_ids.is_empty()
        || indexed.canonical_channel_ids.len() > MAX_CHANNELS_PER_SCOPE
        || indexed
            .canonical_channel_ids
            .iter()
            .any(|id| !valid_opaque(id, 160))
        || !valid_hex(&indexed.local_context_binding_sha256, 64)
    {
        return Err("OSL indexed service scope metadata is invalid".to_owned());
    }
    if let Some(person_id) = indexed.manual_peer_person_id.as_deref() {
        let channel_id = indexed
            .scope
            .channel_id
            .as_deref()
            .ok_or_else(|| "OSL indexed manual scope has no channel binding".to_owned())?;
        let expected_id = crate::security::manual_peer_scope_id(service_id, account_id, person_id)?;
        if indexed.scope.kind != ipc::scope::ScopeKind::Dm
            || indexed.scope.server_id.is_some()
            || indexed.scope.id != expected_id
            || channel_id == indexed.scope.id
            || indexed.canonical_channel_ids != [channel_id]
        {
            return Err("OSL indexed manual scope metadata does not match its binding".to_owned());
        }
    } else if indexed.scope.kind == ipc::scope::ScopeKind::Dm
        && indexed.scope.id.starts_with("manual-scope-")
    {
        return Err(
            "OSL indexed manual scope is missing its authenticated discriminator".to_owned(),
        );
    }
    Ok(())
}

fn validate_account_binding(owner: &str, service: &str, account: &str) -> Result<(), String> {
    if !valid_opaque(owner, 128) || !valid_opaque(service, 32) || !valid_opaque(account, 64) {
        return Err("OSL service account binding is invalid".to_owned());
    }
    Ok(())
}

fn valid_opaque(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_hex(value: &str, exact: usize) -> bool {
    value.len() == exact
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn account_key(owner: &str, service: &str, account: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL/service-scope-account/v1");
    for value in [owner, service, account] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    hex(&hash.finalize())
}

fn manifest_digest(
    owner: &str,
    service: &str,
    account: &str,
    generation: u64,
    scopes: &[IndexedServiceScope],
) -> Result<[u8; 32], String> {
    let encoded = serde_json::to_vec(&(owner, service, account, generation, scopes))
        .map_err(|_| "OSL service burn manifest could not be encoded".to_owned())?;
    Ok(Sha256::digest(encoded).into())
}

fn build_manifest(
    indexed: &AccountScopeIndex,
    generation: u64,
) -> Result<ImmutableServiceBurnManifest, String> {
    let scopes: Vec<_> = indexed.scopes.values().cloned().collect();
    for scope in &scopes {
        validate_indexed_scope(scope, &indexed.service_id, &indexed.account_id)?;
    }
    let manifest_digest = manifest_digest(
        &indexed.owner_osl_user_id,
        &indexed.service_id,
        &indexed.account_id,
        generation,
        &scopes,
    )?;
    let burn_id: [u8; 32] = Sha256::digest(
        [
            b"OSL/service-burn/v1".as_slice(),
            manifest_digest.as_slice(),
        ]
        .concat(),
    )
    .into();
    Ok(ImmutableServiceBurnManifest {
        owner_osl_user_id: indexed.owner_osl_user_id.clone(),
        service_id: indexed.service_id.clone(),
        account_id: indexed.account_id.clone(),
        generation,
        burn_id,
        manifest_digest,
        scopes,
    })
}

fn verify_manifest(manifest: &ImmutableServiceBurnManifest) -> Result<(), String> {
    for scope in &manifest.scopes {
        validate_indexed_scope(scope, &manifest.service_id, &manifest.account_id)?;
    }
    let expected = manifest_digest(
        &manifest.owner_osl_user_id,
        &manifest.service_id,
        &manifest.account_id,
        manifest.generation,
        &manifest.scopes,
    )?;
    if expected != manifest.manifest_digest
        || Sha256::digest([b"OSL/service-burn/v1".as_slice(), expected.as_slice()].concat())
            .as_slice()
            != manifest.burn_id
    {
        return Err("OSL service burn manifest authentication failed".to_owned());
    }
    Ok(())
}

fn load_document(path: &Path) -> Result<IndexDocument, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before accessing service scope coverage".to_owned())?;
    let Some(bytes) =
        crate::atomic_file::read_recoverable_bounded(path, MAX_INDEX_BYTES, "service scope index")
            .map_err(|_| "OSL service scope index could not be read".to_owned())?
    else {
        return Ok(IndexDocument {
            version: INDEX_VERSION,
            ..IndexDocument::default()
        });
    };
    let plaintext = ipc::main_password::decrypt_at_rest(&bytes, &key)
        .map_err(|_| "OSL service scope index authentication failed".to_owned())?;
    let document: IndexDocument = serde_json::from_slice(&plaintext)
        .map_err(|_| "OSL service scope index is malformed".to_owned())?;
    if document.version != INDEX_VERSION
        || document.accounts.len() > MAX_ACCOUNTS
        || document.burn_journal.len() > MAX_BURN_JOURNAL
        || document
            .accounts
            .values()
            .any(|account| account.scopes.len() > MAX_SCOPES_PER_ACCOUNT)
    {
        return Err("OSL service scope index has unsupported or unbounded data".to_owned());
    }
    Ok(document)
}

fn persist(path: &Path, document: &IndexDocument) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before updating service scope coverage".to_owned())?;
    let plaintext = serde_json::to_vec(document)
        .map_err(|_| "OSL service scope index could not be encoded".to_owned())?;
    if plaintext.len() as u64 > MAX_INDEX_BYTES {
        return Err("OSL service scope index exceeds its hard limit".to_owned());
    }
    let encrypted = ipc::main_password::encrypt_at_rest(&plaintext, &key)
        .map_err(|_| "OSL service scope index could not be encrypted".to_owned())?;
    crate::atomic_file::write_recoverable(path, &encrypted, "service scope index")
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
    use super::*;
    use ipc::scope::ScopeKind;
    use std::time::{SystemTime, UNIX_EPOCH};

    const KEY: [u8; 32] = [0x4d; 32];

    fn state() -> (ServiceScopeIndexState, PathBuf) {
        ipc::main_password::set_file_storage_key(Some(KEY));
        let path = std::env::temp_dir().join(format!(
            "osl-service-index-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (ServiceScopeIndexState::load(path.clone()), path)
    }

    fn registration() -> ServiceScopeRegistration {
        ServiceScopeRegistration {
            owner_osl_user_id: "identity-test-1".to_owned(),
            service_id: "discord".to_owned(),
            account_id: "account-test-1".to_owned(),
            scope: ScopeInput {
                kind: ScopeKind::Dm,
                id: "hub-aabbccdd".to_owned(),
                server_id: None,
                channel_id: Some("hub-aabbccdd".to_owned()),
            },
            canonical_channel_ids: vec!["hub-aabbccdd".to_owned()],
            local_context_binding_sha256: "a".repeat(64),
            manual_peer_person_id: None,
        }
    }

    fn manual_registration() -> ServiceScopeRegistration {
        let mut value = registration();
        value.scope = ScopeInput {
            kind: ScopeKind::Dm,
            id: crate::security::manual_peer_scope_id(
                "discord",
                "account-test-1",
                "hub-person-peer-1",
            )
            .unwrap(),
            server_id: None,
            channel_id: Some("manual-dm-shared-1".to_owned()),
        };
        value.canonical_channel_ids = vec!["manual-dm-shared-1".to_owned()];
        value.manual_peer_person_id = Some("hub-person-peer-1".to_owned());
        value
    }

    #[test]
    fn clean_account_registration_is_write_ahead_and_encrypted() {
        let (index_state, path) = state();
        index_state
            .initialize_clean_account("identity-test-1", "discord", "account-test-1")
            .unwrap();
        let observed = std::sync::atomic::AtomicBool::new(false);
        index_state
            .with_registered_write(registration(), || {
                let bytes = std::fs::read(&path).unwrap();
                assert!(ipc::main_password::has_enc_magic(&bytes));
                assert!(!bytes.windows(7).any(|window| window == b"discord"));
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .unwrap();
        assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            index_state
                .coverage("identity-test-1", "discord", "account-test-1")
                .unwrap(),
            CoverageState::CleanPostIndex
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("bak"));
    }

    #[test]
    fn uninitialized_legacy_account_indexes_write_but_cannot_claim_complete_burn() {
        let (state, path) = state();
        state
            .with_registered_write(registration(), || Ok(()))
            .unwrap();
        assert_eq!(
            state
                .coverage("identity-test-1", "discord", "account-test-1")
                .unwrap(),
            CoverageState::LegacyIncomplete
        );
        assert!(state
            .freeze_complete_manifest("identity-test-1", "discord", "account-test-1")
            .is_err());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("bak"));
    }

    #[test]
    fn immutable_manifest_freezes_writes_and_journal_makes_retry_idempotent() {
        let (state, path) = state();
        state
            .initialize_clean_account("identity-test-1", "discord", "account-test-1")
            .unwrap();
        state
            .with_registered_write(registration(), || Ok(()))
            .unwrap();
        let manifest = state
            .freeze_complete_manifest("identity-test-1", "discord", "account-test-1")
            .unwrap();
        assert_eq!(manifest.scopes.len(), 1);
        assert!(state
            .with_registered_write(registration(), || Ok(()))
            .is_err());
        assert_eq!(state.pending_scopes(&manifest).unwrap().len(), 1);
        state
            .mark_scope_burned(&manifest, &manifest.scopes[0].storage_key)
            .unwrap();
        state
            .mark_scope_burned(&manifest, &manifest.scopes[0].storage_key)
            .unwrap();
        assert!(state.pending_scopes(&manifest).unwrap().is_empty());
        state.finish_burn(&manifest).unwrap();
        assert!(state
            .with_registered_write(registration(), || Ok(()))
            .is_ok());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("bak"));
    }

    #[test]
    fn manual_discriminator_is_authenticated_and_mismatch_fails_closed() {
        let (index_state, path) = state();
        index_state
            .initialize_clean_account("identity-test-1", "discord", "account-test-1")
            .unwrap();
        index_state
            .with_registered_write(manual_registration(), || Ok(()))
            .unwrap();
        let manifest = index_state
            .freeze_complete_manifest("identity-test-1", "discord", "account-test-1")
            .unwrap();
        assert_eq!(
            manifest.scopes[0].manual_peer_person_id.as_deref(),
            Some("hub-person-peer-1")
        );

        let mut removed_discriminator = manifest.clone();
        removed_discriminator.scopes[0].manual_peer_person_id = None;
        assert!(verify_manifest(&removed_discriminator).is_err());

        let mut mismatched_person = manifest.clone();
        mismatched_person.scopes[0].manual_peer_person_id = Some("hub-person-other".to_owned());
        assert!(verify_manifest(&mismatched_person).is_err());

        let generic = registration();
        assert!(validate_registration(&generic).is_ok());
        assert!(indexed_scope(&generic)
            .unwrap()
            .manual_peer_person_id
            .is_none());

        let mut missing_discriminator = manual_registration();
        missing_discriminator.manual_peer_person_id = None;
        assert!(validate_registration(&missing_discriminator).is_err());
        let (rejected_state, rejected_path) = state();
        rejected_state
            .initialize_clean_account("identity-test-1", "discord", "account-test-1")
            .unwrap();
        assert!(rejected_state
            .with_registered_write(missing_discriminator, || Ok(()))
            .is_err());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("bak"));
        let _ = std::fs::remove_file(&rejected_path);
        let _ = std::fs::remove_file(rejected_path.with_extension("bak"));
    }

    #[test]
    fn module_has_no_profile_deletion_surface() {
        let source = include_str!("service_scope_index.rs");
        assert!(!source.contains(&["service", "-profiles-v2"].concat()));
        assert!(!source.contains(&["remove", "_dir_all"].concat()));
    }
}
