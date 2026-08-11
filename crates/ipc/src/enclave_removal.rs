//! Durable, epoch-bound Enclave member removal.
//!
//! Removal is a cryptographic transition, not a roster edit. A job creates a
//! fresh epoch key, wraps it independently for every remaining member, and only
//! then publishes the successor roster. The durable wraps are also the progress
//! authority: there is no separately mutable counter that can run ahead of the
//! real fan-out.

use crypto::aead::{self, Key, Nonce};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const REMOVAL_DELAY_WARNING: &str = "Removal takes time and is not immediate.";

const STATE_VERSION: u8 = 1;
const WRAP_AAD_DOMAIN: &[u8] = b"OSL/enclave-removal-authority/v1";
const MESSAGE_AAD_DOMAIN: &[u8] = b"OSL/enclave-message-epoch/v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnclaveMemberAuthority {
    member_id: String,
    wrapping_key: [u8; aead::KEY_SIZE],
}

impl EnclaveMemberAuthority {
    pub fn generate(member_id: impl Into<String>) -> Result<Self, EnclaveRemovalError> {
        let member_id = checked_id(member_id.into(), "member")?;
        Ok(Self {
            member_id,
            wrapping_key: random_array()?,
        })
    }

    pub fn member_id(&self) -> &str {
        &self.member_id
    }

    pub fn package_client(
        &self,
        enclave_id: impl Into<String>,
        epoch: u64,
        epoch_key: &EnclaveEpochKey,
    ) -> Result<PackagedEnclaveClient, EnclaveRemovalError> {
        Ok(PackagedEnclaveClient {
            enclave_id: checked_id(enclave_id.into(), "enclave")?,
            member_id: self.member_id.clone(),
            wrapping_key: self.wrapping_key,
            cached_epoch_keys: BTreeMap::from([(epoch, epoch_key.0)]),
            read_message_ids: BTreeSet::new(),
        })
    }
}

#[derive(Clone)]
pub struct EnclaveEpochKey([u8; aead::KEY_SIZE]);

impl EnclaveEpochKey {
    pub fn generate() -> Result<Self, EnclaveRemovalError> {
        Ok(Self(random_array()?))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RestartBoundary {
    Client,
    Worker,
    Machine,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RemovalStage {
    AwaitingConfirmation,
    Rekeying,
    Succeeded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalConfirmation {
    pub member_count: usize,
    pub measured_progress_n: usize,
    pub warning: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemovalProgress {
    pub stage: RemovalStage,
    pub completed: usize,
    pub remaining: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct WrappedSuccessorAuthority {
    member_id: String,
    nonce: [u8; aead::NONCE_SIZE],
    ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct RemovalState {
    version: u8,
    enclave_id: String,
    removed_member_id: String,
    original_member_count: usize,
    measured_progress_n: usize,
    predecessor_epoch: u64,
    successor_epoch: u64,
    stage: RemovalStage,
    remaining_members: Vec<EnclaveMemberAuthority>,
    active_member_ids: BTreeSet<String>,
    successor_epoch_key: Option<[u8; aead::KEY_SIZE]>,
    wraps: Vec<WrappedSuccessorAuthority>,
    restart_history: Vec<RestartBoundary>,
}

pub struct EnclaveRemovalJob {
    path: PathBuf,
    state: RemovalState,
}

impl EnclaveRemovalJob {
    pub fn begin(
        path: impl Into<PathBuf>,
        enclave_id: impl Into<String>,
        predecessor_epoch: u64,
        members: Vec<EnclaveMemberAuthority>,
        removed_member_id: impl Into<String>,
        measured_progress_n: usize,
    ) -> Result<Self, EnclaveRemovalError> {
        if measured_progress_n == 0 {
            return Err(EnclaveRemovalError::InvalidMeasuredThreshold);
        }
        let enclave_id = checked_id(enclave_id.into(), "enclave")?;
        let removed_member_id = checked_id(removed_member_id.into(), "removed member")?;
        let successor_epoch = predecessor_epoch
            .checked_add(1)
            .ok_or(EnclaveRemovalError::EpochExhausted)?;
        let mut seen = BTreeSet::new();
        let mut removed_present = false;
        let mut remaining_members = Vec::with_capacity(members.len().saturating_sub(1));
        for member in members {
            checked_id(member.member_id.clone(), "member")?;
            if !seen.insert(member.member_id.clone()) {
                return Err(EnclaveRemovalError::DuplicateMember(member.member_id));
            }
            if member.member_id == removed_member_id {
                removed_present = true;
            } else {
                remaining_members.push(member);
            }
        }
        if !removed_present {
            return Err(EnclaveRemovalError::RemovedMemberAbsent);
        }
        let original_member_count = seen.len();
        let active_member_ids = seen;
        let mut job = Self {
            path: path.into(),
            state: RemovalState {
                version: STATE_VERSION,
                enclave_id,
                removed_member_id,
                original_member_count,
                measured_progress_n,
                predecessor_epoch,
                successor_epoch,
                stage: RemovalStage::AwaitingConfirmation,
                remaining_members,
                active_member_ids,
                successor_epoch_key: None,
                wraps: Vec::new(),
                restart_history: Vec::new(),
            },
        };
        job.persist()?;
        Ok(job)
    }

    pub fn reopen(
        path: impl Into<PathBuf>,
        boundary: RestartBoundary,
    ) -> Result<Self, EnclaveRemovalError> {
        let path = path.into();
        let bytes = read_durable(&path)?;
        let mut state: RemovalState = serde_json::from_slice(&bytes)
            .map_err(|error| EnclaveRemovalError::MalformedState(error.to_string()))?;
        validate_state(&state)?;
        state.restart_history.push(boundary);
        let mut job = Self { path, state };
        job.persist()?;
        Ok(job)
    }

    pub fn confirmation(&self) -> RemovalConfirmation {
        RemovalConfirmation {
            member_count: self.state.original_member_count,
            measured_progress_n: self.state.measured_progress_n,
            warning: (self.state.original_member_count >= self.state.measured_progress_n)
                .then_some(REMOVAL_DELAY_WARNING),
        }
    }

    pub fn confirm(&mut self) -> Result<RemovalProgress, EnclaveRemovalError> {
        if self.state.stage != RemovalStage::AwaitingConfirmation {
            return Err(EnclaveRemovalError::WrongStage);
        }
        self.state.successor_epoch_key = Some(random_array()?);
        self.state.stage = RemovalStage::Rekeying;
        self.finish_if_complete();
        self.persist()?;
        self.observed_progress()
    }

    /// Complete at most `work_limit` real per-member authority wraps.
    pub fn step(&mut self, work_limit: usize) -> Result<RemovalProgress, EnclaveRemovalError> {
        if work_limit == 0 {
            return Err(EnclaveRemovalError::ZeroWorkLimit);
        }
        if self.state.stage == RemovalStage::Succeeded {
            return self.observed_progress();
        }
        if self.state.stage != RemovalStage::Rekeying {
            return Err(EnclaveRemovalError::WrongStage);
        }
        let epoch_key = self
            .state
            .successor_epoch_key
            .ok_or(EnclaveRemovalError::MissingSuccessorAuthority)?;
        let completed: BTreeSet<_> = self
            .state
            .wraps
            .iter()
            .map(|wrapped| wrapped.member_id.as_str())
            .collect();
        let pending: Vec<_> = self
            .state
            .remaining_members
            .iter()
            .filter(|member| !completed.contains(member.member_id.as_str()))
            .take(work_limit)
            .cloned()
            .collect();
        for member in pending {
            self.state.wraps.push(wrap_successor_authority(
                &self.state.enclave_id,
                self.state.successor_epoch,
                &member,
                &epoch_key,
            )?);
        }
        self.finish_if_complete();
        self.persist()?;
        self.observed_progress()
    }

    /// Progress is cryptographically observed from stored per-member wraps.
    /// Corrupt, fabricated, duplicated, or wrong-epoch wraps make the job fail
    /// closed instead of advancing a cosmetic counter.
    pub fn observed_progress(&self) -> Result<RemovalProgress, EnclaveRemovalError> {
        let total = self.state.remaining_members.len();
        let completed = if let Some(epoch_key) = self.state.successor_epoch_key {
            let mut observed = BTreeSet::new();
            for wrapped in &self.state.wraps {
                let member = self
                    .state
                    .remaining_members
                    .iter()
                    .find(|candidate| candidate.member_id == wrapped.member_id)
                    .ok_or(EnclaveRemovalError::FabricatedProgress)?;
                if !observed.insert(wrapped.member_id.as_str()) {
                    return Err(EnclaveRemovalError::FabricatedProgress);
                }
                let opened = open_successor_authority(
                    &self.state.enclave_id,
                    self.state.successor_epoch,
                    member,
                    wrapped,
                )?;
                if opened != epoch_key {
                    return Err(EnclaveRemovalError::FabricatedProgress);
                }
            }
            observed.len()
        } else {
            if !self.state.wraps.is_empty() {
                return Err(EnclaveRemovalError::FabricatedProgress);
            }
            0
        };
        if completed > total {
            return Err(EnclaveRemovalError::FabricatedProgress);
        }
        if self.state.stage == RemovalStage::Succeeded
            && (completed != total
                || self
                    .state
                    .active_member_ids
                    .contains(&self.state.removed_member_id))
        {
            return Err(EnclaveRemovalError::PrematureSuccess);
        }
        Ok(RemovalProgress {
            stage: self.state.stage,
            completed,
            remaining: total - completed,
            total,
        })
    }

    pub fn active_member_ids(&self) -> &BTreeSet<String> {
        &self.state.active_member_ids
    }

    pub fn successor_epoch(&self) -> Option<u64> {
        (self.state.stage == RemovalStage::Succeeded).then_some(self.state.successor_epoch)
    }

    pub fn restart_history(&self) -> &[RestartBoundary] {
        &self.state.restart_history
    }

    pub fn install_successor_authority(
        &self,
        client: &mut PackagedEnclaveClient,
    ) -> Result<(), EnclaveRemovalError> {
        self.require_successful_member(client)?;
        let member = self
            .state
            .remaining_members
            .iter()
            .find(|member| member.member_id == client.member_id)
            .ok_or(EnclaveRemovalError::ReadRefused)?;
        let wrapped = self
            .state
            .wraps
            .iter()
            .find(|wrapped| wrapped.member_id == client.member_id)
            .ok_or(EnclaveRemovalError::ReadRefused)?;
        // The packaged client must prove possession of its own wrapping key.
        if member.wrapping_key != client.wrapping_key {
            return Err(EnclaveRemovalError::ReadRefused);
        }
        let epoch_key = open_successor_authority(
            &self.state.enclave_id,
            self.state.successor_epoch,
            member,
            wrapped,
        )?;
        client
            .cached_epoch_keys
            .insert(self.state.successor_epoch, epoch_key);
        Ok(())
    }

    pub fn encrypt_new_message(
        &self,
        plaintext: &[u8],
    ) -> Result<EnclaveEpochMessage, EnclaveRemovalError> {
        if self.state.stage != RemovalStage::Succeeded || plaintext.is_empty() {
            return Err(EnclaveRemovalError::WrongStage);
        }
        self.observed_progress()?;
        let epoch_key = self
            .state
            .successor_epoch_key
            .ok_or(EnclaveRemovalError::MissingSuccessorAuthority)?;
        let message_id: [u8; 16] = random_array()?;
        let nonce: [u8; aead::NONCE_SIZE] = random_array()?;
        let ciphertext = aead::seal(
            &Key::from_bytes(epoch_key),
            &Nonce::from_bytes(nonce),
            &message_aad(
                &self.state.enclave_id,
                self.state.successor_epoch,
                &message_id,
            ),
            plaintext,
        )
        .map_err(|_| EnclaveRemovalError::MessageAuthentication)?;
        Ok(EnclaveEpochMessage {
            enclave_id: self.state.enclave_id.clone(),
            epoch: self.state.successor_epoch,
            message_id,
            nonce,
            ciphertext,
        })
    }

    /// Direct service path: membership is checked before authority installation
    /// or plaintext access, independently of what a packaged client cached.
    pub fn direct_service_read(
        &self,
        client: &mut PackagedEnclaveClient,
        message: &EnclaveEpochMessage,
    ) -> Result<Option<Vec<u8>>, EnclaveRemovalError> {
        self.require_successful_member(client)?;
        self.install_successor_authority(client)?;
        client.read_cached(message)
    }

    pub fn discard(self) -> Result<(), EnclaveRemovalError> {
        remove_if_present(&self.path)?;
        remove_if_present(&self.path.with_extension("bak"))?;
        remove_if_present(&self.path.with_extension("tmp"))?;
        Ok(())
    }

    fn require_successful_member(
        &self,
        client: &PackagedEnclaveClient,
    ) -> Result<(), EnclaveRemovalError> {
        if self.state.stage != RemovalStage::Succeeded
            || client.enclave_id != self.state.enclave_id
            || !self.state.active_member_ids.contains(&client.member_id)
            || client.member_id == self.state.removed_member_id
        {
            return Err(EnclaveRemovalError::ReadRefused);
        }
        Ok(())
    }

    fn finish_if_complete(&mut self) {
        if self.state.stage == RemovalStage::Rekeying
            && self.state.wraps.len() == self.state.remaining_members.len()
        {
            self.state.active_member_ids = self
                .state
                .remaining_members
                .iter()
                .map(|member| member.member_id.clone())
                .collect();
            self.state.stage = RemovalStage::Succeeded;
        }
    }

    fn persist(&mut self) -> Result<(), EnclaveRemovalError> {
        validate_state(&self.state)?;
        let plaintext = serde_json::to_vec(&self.state)
            .map_err(|error| EnclaveRemovalError::MalformedState(error.to_string()))?;
        let key = crate::main_password::get_file_storage_key()
            .ok_or(EnclaveRemovalError::StorageKeyUnavailable)?;
        let sealed = crate::main_password::encrypt_at_rest(&plaintext, &key)
            .map_err(EnclaveRemovalError::Persistence)?;
        crate::recoverable_file::write_recoverable(&self.path, &sealed)
            .map_err(|error| EnclaveRemovalError::Persistence(error.to_string()))
    }
}

pub struct PackagedEnclaveClient {
    enclave_id: String,
    member_id: String,
    wrapping_key: [u8; aead::KEY_SIZE],
    cached_epoch_keys: BTreeMap<u64, [u8; aead::KEY_SIZE]>,
    read_message_ids: BTreeSet<[u8; 16]>,
}

impl PackagedEnclaveClient {
    pub fn member_id(&self) -> &str {
        &self.member_id
    }

    /// Packaged/cache-only path. It has no service membership shortcut: a
    /// removed client owns only predecessor keys and cannot authenticate a
    /// successor-epoch ciphertext.
    pub fn read_cached(
        &mut self,
        message: &EnclaveEpochMessage,
    ) -> Result<Option<Vec<u8>>, EnclaveRemovalError> {
        if message.enclave_id != self.enclave_id {
            return Err(EnclaveRemovalError::ReadRefused);
        }
        if self.read_message_ids.contains(&message.message_id) {
            return Ok(None);
        }
        let epoch_key = self
            .cached_epoch_keys
            .get(&message.epoch)
            .copied()
            .ok_or(EnclaveRemovalError::ReadRefused)?;
        let plaintext = aead::open(
            &Key::from_bytes(epoch_key),
            &Nonce::from_bytes(message.nonce),
            &message_aad(&message.enclave_id, message.epoch, &message.message_id),
            &message.ciphertext,
        )
        .map_err(|_| EnclaveRemovalError::MessageAuthentication)?;
        self.read_message_ids.insert(message.message_id);
        Ok(Some(plaintext))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EnclaveEpochMessage {
    enclave_id: String,
    epoch: u64,
    message_id: [u8; 16],
    nonce: [u8; aead::NONCE_SIZE],
    ciphertext: Vec<u8>,
}

fn wrap_successor_authority(
    enclave_id: &str,
    epoch: u64,
    member: &EnclaveMemberAuthority,
    epoch_key: &[u8; aead::KEY_SIZE],
) -> Result<WrappedSuccessorAuthority, EnclaveRemovalError> {
    let nonce: [u8; aead::NONCE_SIZE] = random_array()?;
    let ciphertext = aead::seal(
        &Key::from_bytes(member.wrapping_key),
        &Nonce::from_bytes(nonce),
        &wrap_aad(enclave_id, epoch, &member.member_id),
        epoch_key,
    )
    .map_err(|_| EnclaveRemovalError::AuthorityAuthentication)?;
    Ok(WrappedSuccessorAuthority {
        member_id: member.member_id.clone(),
        nonce,
        ciphertext,
    })
}

fn open_successor_authority(
    enclave_id: &str,
    epoch: u64,
    member: &EnclaveMemberAuthority,
    wrapped: &WrappedSuccessorAuthority,
) -> Result<[u8; aead::KEY_SIZE], EnclaveRemovalError> {
    let plaintext = aead::open(
        &Key::from_bytes(member.wrapping_key),
        &Nonce::from_bytes(wrapped.nonce),
        &wrap_aad(enclave_id, epoch, &member.member_id),
        &wrapped.ciphertext,
    )
    .map_err(|_| EnclaveRemovalError::AuthorityAuthentication)?;
    plaintext
        .try_into()
        .map_err(|_| EnclaveRemovalError::AuthorityAuthentication)
}

fn wrap_aad(enclave_id: &str, epoch: u64, member_id: &str) -> Vec<u8> {
    framed_aad(WRAP_AAD_DOMAIN, enclave_id, epoch, member_id.as_bytes())
}

fn message_aad(enclave_id: &str, epoch: u64, message_id: &[u8; 16]) -> Vec<u8> {
    framed_aad(MESSAGE_AAD_DOMAIN, enclave_id, epoch, message_id)
}

fn framed_aad(domain: &[u8], enclave_id: &str, epoch: u64, tail: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(domain.len() + enclave_id.len() + tail.len() + 24);
    aad.extend_from_slice(domain);
    aad.extend_from_slice(&(enclave_id.len() as u64).to_be_bytes());
    aad.extend_from_slice(enclave_id.as_bytes());
    aad.extend_from_slice(&epoch.to_be_bytes());
    aad.extend_from_slice(&(tail.len() as u64).to_be_bytes());
    aad.extend_from_slice(tail);
    aad
}

fn random_array<const N: usize>() -> Result<[u8; N], EnclaveRemovalError> {
    crypto::random::random_bytes(N)
        .try_into()
        .map_err(|_| EnclaveRemovalError::Randomness)
}

fn checked_id(value: String, field: &'static str) -> Result<String, EnclaveRemovalError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed != value || value.chars().any(char::is_control) {
        return Err(EnclaveRemovalError::InvalidId(field));
    }
    Ok(value)
}

fn validate_state(state: &RemovalState) -> Result<(), EnclaveRemovalError> {
    if state.version != STATE_VERSION
        || state.original_member_count != state.remaining_members.len().saturating_add(1)
        || state.measured_progress_n == 0
        || state.successor_epoch != state.predecessor_epoch.checked_add(1).unwrap_or(0)
    {
        return Err(EnclaveRemovalError::MalformedState(
            "invalid removal state header".to_owned(),
        ));
    }
    let targets: BTreeSet<_> = state
        .remaining_members
        .iter()
        .map(|member| member.member_id.as_str())
        .collect();
    if targets.len() != state.remaining_members.len()
        || targets.contains(state.removed_member_id.as_str())
        || state.wraps.len() > state.remaining_members.len()
    {
        return Err(EnclaveRemovalError::MalformedState(
            "invalid removal member set".to_owned(),
        ));
    }
    match state.stage {
        RemovalStage::AwaitingConfirmation => {
            if state.successor_epoch_key.is_some() || !state.wraps.is_empty() {
                return Err(EnclaveRemovalError::MalformedState(
                    "unconfirmed removal has successor work".to_owned(),
                ));
            }
        }
        RemovalStage::Rekeying => {
            if state.successor_epoch_key.is_none()
                || !state.active_member_ids.contains(&state.removed_member_id)
            {
                return Err(EnclaveRemovalError::MalformedState(
                    "running removal state is inconsistent".to_owned(),
                ));
            }
        }
        RemovalStage::Succeeded => {
            if state.successor_epoch_key.is_none()
                || state.wraps.len() != state.remaining_members.len()
                || state.active_member_ids != targets.into_iter().map(str::to_owned).collect()
            {
                return Err(EnclaveRemovalError::PrematureSuccess);
            }
        }
    }
    Ok(())
}

fn read_durable(path: &Path) -> Result<Vec<u8>, EnclaveRemovalError> {
    let key = crate::main_password::get_file_storage_key()
        .ok_or(EnclaveRemovalError::StorageKeyUnavailable)?;
    let primary = std::fs::read(path).ok();
    let backup_path = path.with_extension("bak");
    let backup = std::fs::read(&backup_path).ok();
    for candidate in [primary, backup] {
        let Some(candidate) = candidate else { continue };
        if !crate::main_password::has_enc_magic(&candidate) {
            continue;
        }
        if let Ok(plaintext) = crate::main_password::decrypt_at_rest(&candidate, &key) {
            return Ok(plaintext);
        }
    }
    Err(EnclaveRemovalError::Persistence(
        "durable enclave removal state is unavailable".to_owned(),
    ))
}

fn remove_if_present(path: &Path) -> Result<(), EnclaveRemovalError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(EnclaveRemovalError::Persistence(error.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EnclaveRemovalError {
    #[error("invalid {0} id")]
    InvalidId(&'static str),
    #[error("measured progress threshold N must be positive")]
    InvalidMeasuredThreshold,
    #[error("duplicate enclave member {0}")]
    DuplicateMember(String),
    #[error("removed member is absent from the enclave")]
    RemovedMemberAbsent,
    #[error("enclave epoch is exhausted")]
    EpochExhausted,
    #[error("removal job is in the wrong stage")]
    WrongStage,
    #[error("removal work limit must be positive")]
    ZeroWorkLimit,
    #[error("successor authority is missing")]
    MissingSuccessorAuthority,
    #[error("removal progress is fabricated or corrupt")]
    FabricatedProgress,
    #[error("removal reported success before the last re-key")]
    PrematureSuccess,
    #[error("successor authority authentication failed")]
    AuthorityAuthentication,
    #[error("new-message authentication failed")]
    MessageAuthentication,
    #[error("new-message read refused")]
    ReadRefused,
    #[error("file storage key is unavailable")]
    StorageKeyUnavailable,
    #[error("removal state is malformed: {0}")]
    MalformedState(String),
    #[error("removal persistence failed: {0}")]
    Persistence(String),
    #[error("secure randomness failed")]
    Randomness,
}
