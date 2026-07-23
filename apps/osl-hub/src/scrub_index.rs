//! Encrypted, resumable, local-only foundation for Scrub indexing.
//!
//! This module cannot discover service data, access browser profiles, use the
//! network, or delete messages. Trusted adapters may stream only an explicit
//! user export or OSL-owned data already visible to the user.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use crate::privacy_scan::{
    scan_local_messages, validate_attachment_input_batch, LocalMessageCandidate,
    LocalPrivacyScanResult, MAX_FINDINGS,
};

const VERSION: u8 = 1;
const INDEX_DIR: &str = "scrub-index-v1";
const JOURNAL: &str = "journal.bin";
const CHUNKS: &str = "chunks";
const MAX_JOURNAL_BYTES: u64 = 512 * 1024;
const MAX_INDEX_BYTES: u64 = 50 * 1024 * 1024;
const JOURNAL_RESERVE_BYTES: u64 = 512 * 1024;
const MAX_SELECTIONS: usize = 32;
const MAX_MESSAGES_PER_CHUNK: usize = 256;
// One UI-selected attachment may be up to 8 MiB. Base64 plus the bounded JSON
// envelope fits within 12 MiB while the encrypted index remains globally
// capped at 50 MiB.
const MAX_PLAINTEXT_CHUNK_BYTES: usize = 12 * 1024 * 1024;
const MAX_ENCRYPTED_CHUNK_BYTES: u64 = MAX_PLAINTEXT_CHUNK_BYTES as u64 + 64;
const MAX_CHUNKS: usize = 4_096;
pub const MANUAL_EXPORT_SERVICE_ID: &str = "local_import";
pub const MANUAL_EXPORT_ACCOUNT_ID: &str = "manual-export";

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubAccountSelection {
    pub service_id: String,
    pub account_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubIndexSource {
    ExplicitExport,
    OslVisibleData,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubIndexPhase {
    Running,
    Paused,
    Complete,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubIndexInitializeRequest {
    pub selections: Vec<ScrubAccountSelection>,
    pub source: ScrubIndexSource,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubIndexChunkRequest {
    pub import_id: String,
    pub sequence: u32,
    pub final_chunk: bool,
    pub messages: Vec<LocalMessageCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubIndexStatus {
    pub import_id: String,
    pub phase: ScrubIndexPhase,
    pub source: ScrubIndexSource,
    pub selected_account_count: usize,
    pub messages_indexed: u64,
    pub findings_indexed: u64,
    pub rejected_messages: u64,
    pub completed_chunks: u32,
    pub next_sequence: u32,
    pub bytes_stored: u64,
    pub max_bytes: u64,
    pub analysis_location: &'static str,
    pub persisted_encrypted: bool,
    pub deletion_enabled: bool,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JournalDocument {
    version: u8,
    import_id: String,
    owner_osl_user_id: String,
    source: ScrubIndexSource,
    phase: ScrubIndexPhase,
    selections: Vec<ScrubAccountSelection>,
    messages_indexed: u64,
    findings_indexed: u64,
    rejected_messages: u64,
    next_sequence: u32,
    bytes_stored: u64,
    committed_digests: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredChunk<'a> {
    version: u8,
    import_id: &'a str,
    sequence: u32,
    messages: &'a [LocalMessageCandidate],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoadedChunk {
    version: u8,
    import_id: String,
    sequence: u32,
    messages: Vec<LocalMessageCandidate>,
}

#[derive(Clone, Default)]
pub struct ScrubIndexState {
    transition: Arc<Mutex<()>>,
    #[cfg(test)]
    root_override: Option<PathBuf>,
    #[cfg(test)]
    key_override: Option<[u8; 32]>,
    #[cfg(test)]
    use_key_override: bool,
}

impl ScrubIndexState {
    pub fn initialize(
        &self,
        owner: &str,
        request: ScrubIndexInitializeRequest,
    ) -> Result<ScrubIndexStatus, String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        let selections = validate_selections(request.selections)?;
        validate_source_selections(request.source, &selections)?;
        let root = self.root()?;
        let key = self.key()?;
        ensure_safe_root(&root, true)?;
        if let Some(existing) = load_journal(&root, &key)? {
            require_owner(&existing, owner)?;
            if existing.source == request.source && existing.selections == selections {
                recover_orphans(&root, existing.next_sequence)?;
                return Ok(existing.status());
            }
            return Err("A different Scrub index is already initialized; cancel it first".into());
        }

        let document = JournalDocument {
            version: VERSION,
            import_id: hex(&crypto::random::random_bytes(16)),
            owner_osl_user_id: owner.to_owned(),
            source: request.source,
            phase: ScrubIndexPhase::Running,
            selections,
            messages_indexed: 0,
            findings_indexed: 0,
            rejected_messages: 0,
            next_sequence: 0,
            bytes_stored: 0,
            committed_digests: Vec::new(),
        };
        save_journal(&root, &document, &key)?;
        Ok(document.status())
    }

    pub fn status(&self, owner: &str) -> Result<Option<ScrubIndexStatus>, String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        let root = self.root()?;
        ensure_safe_root(&root, false)?;
        let Some(document) = load_journal(&root, &self.key()?)? else {
            return Ok(None);
        };
        require_owner(&document, owner)?;
        recover_orphans(&root, document.next_sequence)?;
        Ok(Some(document.status()))
    }

    /// Decrypt committed chunks and deterministically rebuild the bounded
    /// review result. No plaintext cache or finding metadata is written.
    pub fn read_scan(
        &self,
        owner: &str,
        import_id: &str,
    ) -> Result<LocalPrivacyScanResult, String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        validate_import_id(import_id)?;
        let root = self.root()?;
        ensure_safe_root(&root, false)?;
        let key = self.key()?;
        let document =
            load_journal(&root, &key)?.ok_or_else(|| "No Scrub index is initialized".to_owned())?;
        require_owner(&document, owner)?;
        require_import(&document, import_id)?;
        recover_orphans(&root, document.next_sequence)?;

        let allowed: HashSet<_> = document.selections.iter().cloned().collect();
        let mut findings = Vec::new();
        let mut messages_scanned = 0usize;
        let mut truncated = false;
        let mut attachments_scanned = 0usize;
        let mut attachment_types_scanned = Vec::new();
        let mut uninspected_attachments = Vec::new();
        for sequence in 0..document.next_sequence {
            let sealed = crate::atomic_file::read_recoverable_bounded(
                &chunk_path(&root, sequence),
                MAX_ENCRYPTED_CHUNK_BYTES,
                "encrypted Scrub chunk",
            )?
            .ok_or_else(|| "A committed Scrub chunk is missing".to_owned())?;
            if !ipc::main_password::has_enc_magic(&sealed) {
                return Err("Scrub chunk is not encrypted".into());
            }
            let mut plaintext = ipc::main_password::decrypt_at_rest(&sealed, &key)
                .map_err(|_| "Scrub chunk authentication failed".to_owned())?;
            let parsed = serde_json::from_slice::<LoadedChunk>(&plaintext)
                .map_err(|_| "Scrub chunk is malformed".to_owned());
            plaintext.zeroize();
            let chunk = parsed?;
            if chunk.version != VERSION
                || chunk.import_id != document.import_id
                || chunk.sequence != sequence
                || chunk.messages.is_empty()
                || chunk.messages.len() > MAX_MESSAGES_PER_CHUNK
                || chunk.messages.iter().any(|message| {
                    !allowed.contains(&ScrubAccountSelection {
                        service_id: message.service_id.clone(),
                        account_id: message.account_id.clone(),
                    })
                })
            {
                return Err("Scrub chunk is inconsistent with its journal".into());
            }
            let expected_digest = document
                .committed_digests
                .get(sequence as usize)
                .ok_or_else(|| "Scrub chunk digest is missing from its journal".to_owned())?;
            let loaded_digest = digest_request(&ScrubIndexChunkRequest {
                import_id: chunk.import_id.clone(),
                sequence,
                final_chunk: sequence + 1 == document.next_sequence
                    && document.phase == ScrubIndexPhase::Complete,
                messages: chunk.messages.clone(),
            })?;
            if &loaded_digest != expected_digest {
                return Err("Scrub chunk content does not match its committed digest".into());
            }
            let chunk_message_count = chunk.messages.len();
            let chunk_scan = scan_local_messages(chunk.messages);
            if chunk_scan.messages_rejected != 0
                || chunk_scan.messages_scanned != chunk_message_count
            {
                return Err("Scrub chunk contains an invalid message".into());
            }
            messages_scanned = messages_scanned.saturating_add(chunk_scan.messages_scanned);
            attachments_scanned =
                attachments_scanned.saturating_add(chunk_scan.attachments_scanned);
            attachment_types_scanned.extend(chunk_scan.attachment_types_scanned);
            uninspected_attachments.extend(chunk_scan.uninspected_attachments);
            let remaining = MAX_FINDINGS.saturating_sub(findings.len());
            truncated |= chunk_scan.truncated || chunk_scan.findings.len() > remaining;
            findings.extend(chunk_scan.findings.into_iter().take(remaining));
        }
        if messages_scanned as u64 != document.messages_indexed
            || findings.len() as u64 != document.findings_indexed
        {
            return Err("Scrub index counters do not match committed chunks".into());
        }
        attachment_types_scanned.sort();
        attachment_types_scanned.dedup();
        let images_checked = attachment_types_scanned.iter().any(|kind| kind == "image")
            && !uninspected_attachments
                .iter()
                .any(|item| item.detected_type == "image");
        let videos_checked = attachment_types_scanned.iter().any(|kind| kind == "video")
            && !uninspected_attachments
                .iter()
                .any(|item| item.detected_type == "video");
        Ok(LocalPrivacyScanResult {
            findings,
            messages_scanned,
            messages_rejected: 0,
            truncated,
            analysis_location: "this_device_only",
            persisted: true,
            attachments_scanned,
            images_checked,
            videos_checked,
            attachment_types_scanned,
            uninspected_attachments,
        })
    }

    pub fn append_chunk(
        &self,
        owner: &str,
        request: ScrubIndexChunkRequest,
    ) -> Result<ScrubIndexStatus, String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        validate_import_id(&request.import_id)?;
        if request.messages.is_empty() || request.messages.len() > MAX_MESSAGES_PER_CHUNK {
            return Err("Scrub chunks must contain 1-256 messages".into());
        }
        validate_attachment_input_batch(&request.messages)?;
        let root = self.root()?;
        ensure_safe_root(&root, false)?;
        let key = self.key()?;
        let mut document = load_journal(&root, &key)?
            .ok_or_else(|| "Initialize Scrub before importing messages".to_owned())?;
        require_owner(&document, owner)?;
        require_import(&document, &request.import_id)?;

        let digest = digest_request(&request)?;
        if request.sequence < document.next_sequence {
            let previous = document
                .committed_digests
                .get(request.sequence as usize)
                .ok_or_else(|| "Scrub retry sequence is outside the journal".to_owned())?;
            return if previous == &digest {
                Ok(document.status())
            } else {
                Err("Scrub retry content differs from the committed chunk".into())
            };
        }
        if request.sequence != document.next_sequence {
            return Err("Scrub chunks must be appended in sequence".into());
        }
        match document.phase {
            ScrubIndexPhase::Running => {}
            ScrubIndexPhase::Paused => {
                return Err("Resume Scrub before importing more messages".into())
            }
            ScrubIndexPhase::Complete => return Err("This Scrub import is complete".into()),
        }

        let allowed: HashSet<_> = document.selections.iter().cloned().collect();
        if request.messages.iter().any(|message| {
            !allowed.contains(&ScrubAccountSelection {
                service_id: message.service_id.clone(),
                account_id: message.account_id.clone(),
            })
        }) {
            return Err("A message does not belong to a selected Scrub account".into());
        }
        let scan = scan_local_messages(request.messages.clone());
        if scan.messages_rejected != 0 || scan.messages_scanned != request.messages.len() {
            return Err(
                "Scrub rejected an invalid message; no part of the chunk was stored".into(),
            );
        }

        let mut plaintext = serde_json::to_vec(&StoredChunk {
            version: VERSION,
            import_id: &request.import_id,
            sequence: request.sequence,
            messages: &request.messages,
        })
        .map_err(|_| "Scrub chunk could not be encoded".to_owned())?;
        if plaintext.len() > MAX_PLAINTEXT_CHUNK_BYTES {
            plaintext.zeroize();
            return Err("Scrub chunk exceeds the local import limit".into());
        }
        let sealed = ipc::main_password::encrypt_at_rest(&plaintext, &key)
            .map_err(|_| "Scrub chunk could not be encrypted".to_owned())?;
        plaintext.zeroize();
        let next_bytes = document.bytes_stored.saturating_add(sealed.len() as u64);
        if next_bytes > MAX_INDEX_BYTES.saturating_sub(JOURNAL_RESERVE_BYTES) {
            return Err("Scrub local index reached its 50 MiB limit".into());
        }
        if document.committed_digests.len() >= MAX_CHUNKS {
            return Err("Scrub import has too many chunks; start a new import".into());
        }

        ensure_safe_chunks_dir(&root)?;
        let path = chunk_path(&root, request.sequence);
        crate::atomic_file::write_recoverable(&path, &sealed, "encrypted Scrub chunk")?;
        document.messages_indexed = document
            .messages_indexed
            .saturating_add(request.messages.len() as u64);
        let remaining_findings = (MAX_FINDINGS as u64).saturating_sub(document.findings_indexed);
        document.findings_indexed = document
            .findings_indexed
            .saturating_add((scan.findings.len() as u64).min(remaining_findings));
        document.rejected_messages = document
            .rejected_messages
            .saturating_add(scan.messages_rejected as u64);
        document.bytes_stored = next_bytes;
        document.next_sequence = document.next_sequence.saturating_add(1);
        document.committed_digests.push(digest);
        if request.final_chunk {
            document.phase = ScrubIndexPhase::Complete;
        }
        if let Err(error) = save_journal(&root, &document, &key) {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(document.status())
    }

    pub fn pause(&self, owner: &str, import_id: &str) -> Result<ScrubIndexStatus, String> {
        self.change_phase(owner, import_id, ScrubIndexPhase::Paused)
    }

    pub fn resume(&self, owner: &str, import_id: &str) -> Result<ScrubIndexStatus, String> {
        self.change_phase(owner, import_id, ScrubIndexPhase::Running)
    }

    pub fn cancel(&self, owner: &str, import_id: &str) -> Result<(), String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        validate_import_id(import_id)?;
        let root = self.root()?;
        ensure_safe_root(&root, false)?;
        let document = load_journal(&root, &self.key()?)?
            .ok_or_else(|| "No Scrub index is initialized".to_owned())?;
        require_owner(&document, owner)?;
        require_import(&document, import_id)?;
        remove_index_tree(&root)
    }

    fn change_phase(
        &self,
        owner: &str,
        import_id: &str,
        phase: ScrubIndexPhase,
    ) -> Result<ScrubIndexStatus, String> {
        let _guard = self.lock()?;
        validate_owner(owner)?;
        validate_import_id(import_id)?;
        let root = self.root()?;
        ensure_safe_root(&root, false)?;
        let key = self.key()?;
        let mut document =
            load_journal(&root, &key)?.ok_or_else(|| "No Scrub index is initialized".to_owned())?;
        require_owner(&document, owner)?;
        require_import(&document, import_id)?;
        if document.phase == ScrubIndexPhase::Complete {
            return Err("A completed Scrub import cannot be paused or resumed".into());
        }
        document.phase = phase;
        save_journal(&root, &document, &key)?;
        Ok(document.status())
    }

    fn root(&self) -> Result<PathBuf, String> {
        #[cfg(test)]
        if let Some(root) = &self.root_override {
            return Ok(root.clone());
        }
        // Fresh single-identity installs intentionally have no active-account
        // override. `osl_config_dir` resolves that valid case to the OSL base
        // directory while still honoring an explicit multi-identity slot.
        scrub_index_root(keystore::osl_config_dir().ok())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.transition
            .lock()
            .map_err(|_| "Scrub index is unavailable".to_owned())
    }

    fn key(&self) -> Result<[u8; 32], String> {
        #[cfg(test)]
        if self.use_key_override {
            return self
                .key_override
                .ok_or_else(|| "Unlock an OSL identity before using Scrub".to_owned());
        }
        file_key()
    }

    #[cfg(test)]
    fn for_test(root: PathBuf) -> Self {
        Self {
            transition: Arc::new(Mutex::new(())),
            root_override: Some(root),
            key_override: Some(KEY_FOR_TESTS),
            use_key_override: true,
        }
    }

    #[cfg(test)]
    fn for_test_locked(root: PathBuf) -> Self {
        Self {
            transition: Arc::new(Mutex::new(())),
            root_override: Some(root),
            key_override: None,
            use_key_override: true,
        }
    }
}

fn validate_source_selections(
    source: ScrubIndexSource,
    selections: &[ScrubAccountSelection],
) -> Result<(), String> {
    if source == ScrubIndexSource::ExplicitExport
        && (selections.len() != 1
            || selections[0].service_id != "local_import"
            || selections[0].account_id != "manual-export")
    {
        return Err("Explicit Scrub exports require the fixed local import scope".into());
    }
    Ok(())
}

fn scrub_index_root(config_dir: Option<PathBuf>) -> Result<PathBuf, String> {
    config_dir
        .map(|path| path.join(INDEX_DIR))
        .ok_or_else(|| "OSL account storage is unavailable for Scrub".to_owned())
}

#[cfg(test)]
const KEY_FOR_TESTS: [u8; 32] = [91; 32];

impl JournalDocument {
    fn status(&self) -> ScrubIndexStatus {
        ScrubIndexStatus {
            import_id: self.import_id.clone(),
            phase: self.phase,
            source: self.source,
            selected_account_count: self.selections.len(),
            messages_indexed: self.messages_indexed,
            findings_indexed: self.findings_indexed,
            rejected_messages: self.rejected_messages,
            completed_chunks: self.next_sequence,
            next_sequence: self.next_sequence,
            bytes_stored: self.bytes_stored,
            max_bytes: MAX_INDEX_BYTES,
            analysis_location: "this_device_only",
            persisted_encrypted: true,
            deletion_enabled: false,
        }
    }
}

fn file_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before using Scrub".to_owned())
}

fn save_journal(root: &Path, document: &JournalDocument, key: &[u8; 32]) -> Result<(), String> {
    let mut plain = serde_json::to_vec(document)
        .map_err(|_| "Scrub journal could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&plain, key)
        .map_err(|_| "Scrub journal could not be encrypted".to_owned())?;
    plain.zeroize();
    if sealed.len() as u64 > MAX_JOURNAL_BYTES {
        return Err("Scrub journal exceeds its local limit".into());
    }
    crate::atomic_file::write_recoverable(&root.join(JOURNAL), &sealed, "Scrub journal")
}

fn load_journal(root: &Path, key: &[u8; 32]) -> Result<Option<JournalDocument>, String> {
    let Some(sealed) = crate::atomic_file::read_recoverable_bounded(
        &root.join(JOURNAL),
        MAX_JOURNAL_BYTES,
        "Scrub journal",
    )?
    else {
        return Ok(None);
    };
    if !ipc::main_password::has_enc_magic(&sealed) {
        return Err("Scrub journal is not encrypted".into());
    }
    let mut plain = ipc::main_password::decrypt_at_rest(&sealed, key)
        .map_err(|_| "Scrub journal authentication failed".to_owned())?;
    let parsed = serde_json::from_slice::<JournalDocument>(&plain)
        .map_err(|_| "Scrub journal is malformed".to_owned());
    plain.zeroize();
    let document = parsed?;
    validate_journal(&document)?;
    Ok(Some(document))
}

fn validate_journal(document: &JournalDocument) -> Result<(), String> {
    if document.version != VERSION
        || document.next_sequence as usize != document.committed_digests.len()
        || document.committed_digests.len() > MAX_CHUNKS
        || document.bytes_stored > MAX_INDEX_BYTES
        || document.findings_indexed > MAX_FINDINGS as u64
    {
        return Err("Scrub journal is inconsistent".into());
    }
    validate_owner(&document.owner_osl_user_id)?;
    validate_import_id(&document.import_id)?;
    if validate_selections(document.selections.clone())? != document.selections {
        return Err("Scrub journal selections are not canonical".into());
    }
    if document
        .committed_digests
        .iter()
        .any(|value| !is_hex(value, 64))
    {
        return Err("Scrub journal contains an invalid digest".into());
    }
    Ok(())
}

fn validate_selections(
    mut selections: Vec<ScrubAccountSelection>,
) -> Result<Vec<ScrubAccountSelection>, String> {
    if selections.is_empty() || selections.len() > MAX_SELECTIONS {
        return Err("Select 1-32 linked accounts for Scrub".into());
    }
    let mut unique = HashSet::new();
    for selection in &selections {
        if !valid_service_id(&selection.service_id) || !valid_account_id(&selection.account_id) {
            return Err("Scrub account selection is invalid".into());
        }
        if !unique.insert(selection.clone()) {
            return Err("Scrub account selections must be unique".into());
        }
    }
    selections.sort_unstable_by(|left, right| {
        left.service_id
            .cmp(&right.service_id)
            .then_with(|| left.account_id.cmp(&right.account_id))
    });
    Ok(selections)
}

/// Manual exports do not represent a linked provider account. This exact
/// sentinel is accepted only for the explicit-export source; provider-backed
/// selections must still be proved by the active identity's registry.
pub fn selection_requires_registry_ownership(
    source: ScrubIndexSource,
    selection: &ScrubAccountSelection,
) -> Result<bool, String> {
    let exact_manual_export = selection.service_id == MANUAL_EXPORT_SERVICE_ID
        && selection.account_id == MANUAL_EXPORT_ACCOUNT_ID;
    let mentions_manual_export = selection.service_id == MANUAL_EXPORT_SERVICE_ID
        || selection.account_id == MANUAL_EXPORT_ACCOUNT_ID;
    if exact_manual_export && source == ScrubIndexSource::ExplicitExport {
        Ok(false)
    } else if mentions_manual_export {
        Err("Scrub manual-export selection is invalid for this source".into())
    } else {
        Ok(true)
    }
}

fn validate_owner(value: &str) -> Result<(), String> {
    if !value.is_empty()
        && value.len() <= 128
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err("Active OSL identity is invalid".into())
    }
}

fn validate_import_id(value: &str) -> Result<(), String> {
    if is_hex(value, 32) {
        Ok(())
    } else {
        Err("Scrub import identifier is invalid".into())
    }
}

fn valid_service_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_account_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn require_owner(document: &JournalDocument, owner: &str) -> Result<(), String> {
    if document.owner_osl_user_id == owner {
        Ok(())
    } else {
        Err("Scrub index belongs to a different OSL identity".into())
    }
}

fn require_import(document: &JournalDocument, import_id: &str) -> Result<(), String> {
    if document.import_id == import_id {
        Ok(())
    } else {
        Err("Scrub import identifier does not match".into())
    }
}

fn digest_request(request: &ScrubIndexChunkRequest) -> Result<String, String> {
    let mut bytes = serde_json::to_vec(request)
        .map_err(|_| "Scrub chunk digest could not be computed".to_owned())?;
    let digest = hex(&Sha256::digest(&bytes));
    bytes.zeroize();
    Ok(digest)
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(TABLE[(byte >> 4) as usize] as char);
        output.push(TABLE[(byte & 15) as usize] as char);
    }
    output
}

fn chunk_path(root: &Path, sequence: u32) -> PathBuf {
    root.join(CHUNKS).join(format!("chunk-{sequence:08}.bin"))
}

fn ensure_safe_root(root: &Path, create: bool) -> Result<(), String> {
    match fs::symlink_metadata(root) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            Err("Scrub index root is not a private directory".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            fs::create_dir_all(root)
                .map_err(|_| "Scrub index directory could not be created".into())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Scrub index directory could not be inspected".into()),
    }
}

fn ensure_safe_chunks_dir(root: &Path) -> Result<(), String> {
    let path = root.join(CHUNKS);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            Err("Scrub chunk directory is unsafe".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|_| "Scrub chunk directory could not be created".into())
        }
        Err(_) => Err("Scrub chunk directory could not be inspected".into()),
    }
}

fn recover_orphans(root: &Path, next_sequence: u32) -> Result<(), String> {
    let path = root.join(CHUNKS);
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Scrub chunk directory could not be read".into()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| "Scrub chunk entry could not be read".to_owned())?;
        let meta = fs::symlink_metadata(entry.path())
            .map_err(|_| "Scrub chunk metadata could not be read".to_owned())?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err("Scrub chunk directory contains an unsafe entry".into());
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let sequence = name
            .strip_prefix("chunk-")
            .and_then(|value| {
                value
                    .strip_suffix(".bin")
                    .or_else(|| value.strip_suffix(".tmp"))
                    .or_else(|| value.strip_suffix(".bak"))
            })
            .filter(|value| value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| "Scrub chunk directory contains an unknown file".to_owned())?;
        if sequence >= next_sequence || name.ends_with(".tmp") {
            fs::remove_file(entry.path())
                .map_err(|_| "Incomplete Scrub chunk could not be removed".to_owned())?;
        }
    }
    Ok(())
}

fn remove_index_tree(root: &Path) -> Result<(), String> {
    ensure_safe_root(root, false)?;
    if !root.exists() {
        return Ok(());
    }
    let chunks = root.join(CHUNKS);
    if chunks.exists() {
        ensure_safe_chunks_dir(root)?;
        for entry in fs::read_dir(&chunks)
            .map_err(|_| "Scrub chunk directory could not be read".to_owned())?
        {
            let entry = entry.map_err(|_| "Scrub chunk entry could not be read".to_owned())?;
            let meta = fs::symlink_metadata(entry.path())
                .map_err(|_| "Scrub chunk metadata could not be read".to_owned())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if meta.file_type().is_symlink()
                || !meta.is_file()
                || !name.starts_with("chunk-")
                || !(name.ends_with(".bin") || name.ends_with(".tmp") || name.ends_with(".bak"))
            {
                return Err("Scrub chunk directory contains an unsafe entry".into());
            }
            fs::remove_file(entry.path())
                .map_err(|_| "Scrub chunk could not be removed".to_owned())?;
        }
        fs::remove_dir(chunks)
            .map_err(|_| "Scrub chunk directory could not be removed".to_owned())?;
    }
    for name in [JOURNAL, "journal.tmp", "journal.bak"] {
        match fs::remove_file(root.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Scrub journal could not be removed".into()),
        }
    }
    fs::remove_dir(root).map_err(|_| "Scrub index directory could not be removed".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const OWNER: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("osl-scrub-{label}-{}-{nonce}", std::process::id()))
    }

    fn selection() -> ScrubAccountSelection {
        ScrubAccountSelection {
            service_id: "discord".into(),
            account_id: "account-1".into(),
        }
    }

    fn message(text: &str) -> LocalMessageCandidate {
        LocalMessageCandidate {
            service_id: "discord".into(),
            account_id: "account-1".into(),
            conversation_id: "chat-1".into(),
            message_locator: "message-1".into(),
            authored_by_self: true,
            created_at_unix_ms: Some(1_700_000_000_000),
            text: text.into(),
            attachments: Vec::new(),
        }
    }

    #[test]
    fn encrypted_lifecycle_is_resumable_and_identity_scoped() {
        let root = root("lifecycle");
        let state = ScrubIndexState::for_test(root.clone());
        let initial = state
            .initialize(
                OWNER,
                ScrubIndexInitializeRequest {
                    selections: vec![selection()],
                    source: ScrubIndexSource::OslVisibleData,
                },
            )
            .unwrap();
        assert!(!initial.deletion_enabled);
        let chunk = ScrubIndexChunkRequest {
            import_id: initial.import_id.clone(),
            sequence: 0,
            final_chunk: false,
            messages: vec![message("token=ghp_abcdefghijklmnop")],
        };
        let indexed = state.append_chunk(OWNER, chunk.clone()).unwrap();
        assert_eq!(indexed.messages_indexed, 1);
        assert!(indexed.findings_indexed >= 1);
        assert!(indexed.persisted_encrypted);
        assert!(!indexed.deletion_enabled);
        assert_eq!(state.append_chunk(OWNER, chunk).unwrap(), indexed);
        for bytes in [
            fs::read(root.join(JOURNAL)).unwrap(),
            fs::read(chunk_path(&root, 0)).unwrap(),
        ] {
            assert!(ipc::main_password::has_enc_magic(&bytes));
            assert!(!String::from_utf8_lossy(&bytes).contains("ghp_abcdefghijklmnop"));
            assert!(!String::from_utf8_lossy(&bytes).contains("message-1"));
            assert!(!String::from_utf8_lossy(&bytes).contains("account-1"));
        }
        let restored = ScrubIndexState::for_test(root.clone());
        let persisted_scan = restored.read_scan(OWNER, &initial.import_id).unwrap();
        assert!(persisted_scan.persisted);
        assert_eq!(persisted_scan.messages_scanned, 1);
        assert_eq!(
            persisted_scan.findings.len() as u64,
            indexed.findings_indexed
        );
        assert_eq!(persisted_scan.findings[0].message_locator, "message-1");
        state.pause(OWNER, &initial.import_id).unwrap();
        let paused = state.append_chunk(
            OWNER,
            ScrubIndexChunkRequest {
                import_id: initial.import_id.clone(),
                sequence: 1,
                final_chunk: true,
                messages: vec![message("normal message")],
            },
        );
        assert!(paused.unwrap_err().contains("Resume"));
        state.resume(OWNER, &initial.import_id).unwrap();
        let complete = state
            .append_chunk(
                OWNER,
                ScrubIndexChunkRequest {
                    import_id: initial.import_id.clone(),
                    sequence: 1,
                    final_chunk: true,
                    messages: vec![message("normal message")],
                },
            )
            .unwrap();
        assert_eq!(complete.phase, ScrubIndexPhase::Complete);
        let restored = ScrubIndexState::for_test(root.clone());
        assert_eq!(restored.status(OWNER).unwrap().unwrap(), complete);
        let complete_scan = restored.read_scan(OWNER, &initial.import_id).unwrap();
        assert!(complete_scan.persisted);
        assert_eq!(complete_scan.messages_scanned, 2);
        assert_eq!(
            complete_scan.findings.len() as u64,
            complete.findings_indexed
        );
        assert!(state.status(&"f".repeat(64)).is_err());
        state.cancel(OWNER, &initial.import_id).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn wrong_account_chunk_is_rejected_before_write() {
        let root = root("reject");
        let state = ScrubIndexState::for_test(root.clone());
        let initial = state
            .initialize(
                OWNER,
                ScrubIndexInitializeRequest {
                    selections: vec![selection()],
                    source: ScrubIndexSource::OslVisibleData,
                },
            )
            .unwrap();
        let mut wrong = message("secret");
        wrong.account_id = "other-account".into();
        let result = state.append_chunk(
            OWNER,
            ScrubIndexChunkRequest {
                import_id: initial.import_id.clone(),
                sequence: 0,
                final_chunk: false,
                messages: vec![wrong],
            },
        );
        assert!(result.unwrap_err().contains("selected"));
        assert!(!chunk_path(&root, 0).exists());
        state.cancel(OWNER, &initial.import_id).unwrap();
    }

    #[test]
    fn persisted_review_rejects_authenticated_chunk_swap_against_journal_digest() {
        let root = root("digest-swap");
        let state = ScrubIndexState::for_test(root.clone());
        let initial = state
            .initialize(
                OWNER,
                ScrubIndexInitializeRequest {
                    selections: vec![selection()],
                    source: ScrubIndexSource::ExplicitExport,
                },
            )
            .unwrap();
        state
            .append_chunk(
                OWNER,
                ScrubIndexChunkRequest {
                    import_id: initial.import_id.clone(),
                    sequence: 0,
                    final_chunk: true,
                    messages: vec![message("password: original")],
                },
            )
            .unwrap();

        let replacement = vec![message("password: swapped")];
        let plaintext = serde_json::to_vec(&StoredChunk {
            version: VERSION,
            import_id: &initial.import_id,
            sequence: 0,
            messages: &replacement,
        })
        .unwrap();
        let sealed = ipc::main_password::encrypt_at_rest(&plaintext, &KEY_FOR_TESTS).unwrap();
        fs::write(chunk_path(&root, 0), sealed).unwrap();

        let error = match state.read_scan(OWNER, &initial.import_id) {
            Ok(_) => panic!("swapped chunk unexpectedly passed its committed digest"),
            Err(error) => error,
        };
        assert!(error.contains("committed digest"));
        state.cancel(OWNER, &initial.import_id).unwrap();
    }

    #[test]
    fn persisted_review_and_status_share_the_global_finding_bound() {
        let root = root("finding-bound");
        let state = ScrubIndexState::for_test(root.clone());
        let initial = state
            .initialize(
                OWNER,
                ScrubIndexInitializeRequest {
                    selections: vec![selection()],
                    source: ScrubIndexSource::ExplicitExport,
                },
            )
            .unwrap();
        let dense = "password: sample; recovery phrase; card 4242 4242 4242 4242; passport number; my address is 123 Main; fuck; explicit photo; my diagnosis; cocaine; commit fraud; internal only";
        let indexed = state
            .append_chunk(
                OWNER,
                ScrubIndexChunkRequest {
                    import_id: initial.import_id.clone(),
                    sequence: 0,
                    final_chunk: true,
                    messages: (0..100).map(|_| message(dense)).collect(),
                },
            )
            .unwrap();
        assert_eq!(indexed.messages_indexed, 100);
        assert_eq!(indexed.findings_indexed, MAX_FINDINGS as u64);
        let review = ScrubIndexState::for_test(root.clone())
            .read_scan(OWNER, &initial.import_id)
            .unwrap();
        assert_eq!(review.messages_scanned, 100);
        assert_eq!(review.findings.len(), MAX_FINDINGS);
        assert!(review.truncated);
        assert!(review.persisted);
        state.cancel(OWNER, &initial.import_id).unwrap();
    }

    #[test]
    fn locked_identity_cannot_create_even_an_empty_index_directory() {
        let root = root("locked");
        let state = ScrubIndexState::for_test_locked(root.clone());
        let result = state.initialize(
            OWNER,
            ScrubIndexInitializeRequest {
                selections: vec![selection()],
                source: ScrubIndexSource::OslVisibleData,
            },
        );
        assert!(result.unwrap_err().contains("Unlock"));
        assert!(!root.exists());
    }

    #[test]
    fn only_the_exact_explicit_export_sentinel_skips_registry_ownership() {
        let manual = ScrubAccountSelection {
            service_id: MANUAL_EXPORT_SERVICE_ID.into(),
            account_id: MANUAL_EXPORT_ACCOUNT_ID.into(),
        };
        assert!(
            !selection_requires_registry_ownership(ScrubIndexSource::ExplicitExport, &manual,)
                .unwrap()
        );
        assert!(
            selection_requires_registry_ownership(ScrubIndexSource::OslVisibleData, &manual,)
                .is_err()
        );
        let provider = selection();
        assert!(
            selection_requires_registry_ownership(ScrubIndexSource::ExplicitExport, &provider,)
                .unwrap()
        );
        let partial_sentinel = ScrubAccountSelection {
            service_id: MANUAL_EXPORT_SERVICE_ID.into(),
            account_id: "provider-account".into(),
        };
        assert!(selection_requires_registry_ownership(
            ScrubIndexSource::ExplicitExport,
            &partial_sentinel,
        )
        .is_err());
    }

    #[test]
    fn index_root_accepts_the_resolved_single_identity_base_directory() {
        let base = root("single-identity");
        assert_eq!(
            scrub_index_root(Some(base.clone())).unwrap(),
            base.join(INDEX_DIR)
        );
        assert!(scrub_index_root(None).unwrap_err().contains("unavailable"));
    }

    #[cfg(unix)]
    #[test]
    fn index_root_symlink_fails_closed_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let parent = root("symlink");
        let target = parent.join("target");
        let linked = parent.join("linked");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep"), b"safe").unwrap();
        symlink(&target, &linked).unwrap();
        let state = ScrubIndexState::for_test(linked);
        let result = state.initialize(
            OWNER,
            ScrubIndexInitializeRequest {
                selections: vec![selection()],
                source: ScrubIndexSource::OslVisibleData,
            },
        );
        assert!(result.unwrap_err().contains("not a private directory"));
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"safe");
        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn explicit_exports_are_confined_to_the_fixed_local_scope() {
        let root = root("explicit-scope");
        let state = ScrubIndexState::for_test(root.clone());
        let rejected = state.initialize(
            OWNER,
            ScrubIndexInitializeRequest {
                selections: vec![selection()],
                source: ScrubIndexSource::ExplicitExport,
            },
        );
        assert!(rejected.unwrap_err().contains("fixed local import scope"));
        assert!(!root.exists());

        let accepted = state
            .initialize(
                OWNER,
                ScrubIndexInitializeRequest {
                    selections: vec![ScrubAccountSelection {
                        service_id: "local_import".into(),
                        account_id: "manual-export".into(),
                    }],
                    source: ScrubIndexSource::ExplicitExport,
                },
            )
            .unwrap();
        state.cancel(OWNER, &accepted.import_id).unwrap();
    }
}
