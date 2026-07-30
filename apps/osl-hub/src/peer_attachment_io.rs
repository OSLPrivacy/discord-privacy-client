//! Native, bounded-memory attachment staging.
//!
//! This module deliberately has no IPC surface: callers must already hold
//! validated file handles and an OSL-owned local-data root. Plaintext never
//! crosses the renderer boundary or passes through base64.

use crypto::aead;
use crypto::attachment::{StreamDecryptor, StreamEncryptor, ATTACHMENT_CHUNK_SIZE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_PLAINTEXT_BYTES: u64 = ipc::attachment_wire::MAX_STREAMED_ATTACHMENT_BYTES;
pub const MAX_IO_BUFFER_BYTES: usize = ATTACHMENT_CHUNK_SIZE;

const STAGING_DIRECTORY: &str = "peer-attachment-staging";
const MAX_SEALED_BYTES: u64 = ipc::cipher_store_client::MAX_SEALED_ATTACHMENT_BYTES;

/// Bounded, encrypted, append-only record of remote ciphertext OSL has promised
/// to delete. A full outbox is rejected rather than trimmed, per
/// `docs/design/offline-controls-and-opened-receipts.md`.
const DELETION_OUTBOX_FILE: &str = "attachment_deletions.json";
const MAX_DELETION_OUTBOX_BYTES: u64 = 512 * 1024;
pub const MAX_DELETION_OUTBOX_ENTRIES: usize = 512;
/// Longest lifetime the cipher store will honour, so a record can never be
/// admitted claiming to outlive the object it refers to.
const MAX_DELETION_TTL_SECONDS: i64 = 604_800;
const FETCH_TOKEN_HEX_LEN: usize = ipc::cipher_store_client::FETCH_TOKEN_BYTES * 2;
const OBJECT_ID_HEX_LEN: usize = 32;
const PLAINTEXT_REMOVAL_ATTEMPTS: u32 = 5;

/// Fixed refusal used by every attachment surface that cannot render bytes
/// inside an OSL-owned, capture-protected process surface.
///
/// This is a policy result, not an I/O failure. External shell viewers require
/// a filesystem path, and creating that path would violate the standing
/// no-plaintext-at-rest invariant even if a later cleanup normally succeeds.
pub const EXTERNAL_VIEWER_REFUSAL: &str =
    "OSL refused to open this attachment because its viewer would require a plaintext file";

pub fn supported_protected_image_mime(mime: &str) -> bool {
    matches!(mime, "image/png" | "image/jpeg")
}

/// Require an in-process protected viewer before attachment opening can reach
/// its fetch token, durable staging, decryption, replay, reveal, or burn steps.
pub fn require_protected_attachment_viewer(mime_type: &str) -> Result<(), String> {
    if !mime_type.starts_with("image/") {
        return Err(EXTERNAL_VIEWER_REFUSAL.to_owned());
    }
    Ok(())
}

pub struct StagedAttachment {
    root: PathBuf,
    path: PathBuf,
    original_filename: String,
    mime_type: &'static str,
    plaintext_len: u64,
}

impl StagedAttachment {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn original_filename(&self) -> &str {
        &self.original_filename
    }

    pub fn mime_type(&self) -> &'static str {
        self.mime_type
    }

    pub fn plaintext_len(&self) -> u64 {
        self.plaintext_len
    }
}

struct PartialFile(PathBuf);

impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn encrypt_file(
    app_local_data_dir: &Path,
    source: &mut File,
    original_filename: &str,
    declared_mime: &str,
    key: aead::Key,
    content_id: Vec<u8>,
    attachment_index: u32,
) -> Result<StagedAttachment, String> {
    let mime_type = validate_metadata(original_filename, declared_mime)?;
    let plaintext_len = source
        .metadata()
        .map_err(|_| "attachment metadata could not be read".to_owned())?
        .len();
    if plaintext_len > MAX_PLAINTEXT_BYTES {
        return Err("attachment exceeds the 512 MiB limit".to_owned());
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|_| "attachment could not be read".to_owned())?;

    let (mut encryptor, header) =
        StreamEncryptor::new(key, plaintext_len, content_id, attachment_index)
            .map_err(|_| "attachment encryption could not start".to_owned())?;
    let (temporary, final_path, mut output) = create_output(app_local_data_dir, "sealed")?;
    let cleanup = PartialFile(temporary.clone());
    output
        .write_all(&header)
        .map_err(|_| "encrypted attachment could not be written".to_owned())?;

    let mut buffer = [0u8; MAX_IO_BUFFER_BYTES];
    let result: Result<(), String> = (|| {
        let mut consumed = 0u64;
        loop {
            let read = source
                .read(&mut buffer)
                .map_err(|_| "attachment could not be read".to_owned())?;
            if read == 0 {
                break;
            }
            consumed = consumed
                .checked_add(read as u64)
                .ok_or_else(|| "attachment size changed while reading".to_owned())?;
            if consumed > plaintext_len {
                return Err("attachment size changed while reading".to_owned());
            }
            let ciphertext = encryptor
                .write(&buffer[..read])
                .map_err(|_| "attachment encryption failed".to_owned())?;
            output
                .write_all(&ciphertext)
                .map_err(|_| "encrypted attachment could not be written".to_owned())?;
        }
        if consumed != plaintext_len {
            return Err("attachment size changed while reading".to_owned());
        }
        encryptor
            .finalize_into(|chunk| {
                output.write_all(chunk).map_err(|_| {
                    crypto::Error::Internal("encrypted attachment write failed".to_owned())
                })
            })
            .map_err(|_| "attachment encryption failed".to_owned())?;
        output
            .sync_all()
            .map_err(|_| "encrypted attachment could not be synchronized".to_owned())?;
        drop(output);
        std::fs::rename(&temporary, &final_path)
            .map_err(|_| "encrypted attachment could not be committed".to_owned())?;
        Ok(())
    })();
    buffer.zeroize();
    result?;
    std::mem::forget(cleanup);

    Ok(StagedAttachment {
        root: app_local_data_dir.to_owned(),
        path: final_path,
        original_filename: original_filename.to_owned(),
        mime_type,
        plaintext_len,
    })
}

/// Decrypt to an OSL-owned plaintext staging file, for the non-image path where
/// an external Windows reader needs a real path. The result is an RAII guard:
/// dropping it removes the decrypted file, so no caller can return early and
/// leave plaintext at rest. Images must use [`decrypt_file_to_memory`].
pub fn decrypt_file(
    app_local_data_dir: &Path,
    sealed: &mut File,
    original_filename: &str,
    declared_mime: &str,
    key: aead::Key,
) -> Result<StagedPlaintext, String> {
    let mime_type = validate_metadata(original_filename, declared_mime)?;
    let sealed_len = sealed
        .metadata()
        .map_err(|_| "encrypted attachment metadata could not be read".to_owned())?
        .len();
    if sealed_len == 0 || sealed_len > MAX_SEALED_BYTES {
        return Err("encrypted attachment has an invalid size".to_owned());
    }
    sealed
        .seek(SeekFrom::Start(0))
        .map_err(|_| "encrypted attachment could not be read".to_owned())?;

    let mut input = [0u8; MAX_IO_BUFFER_BYTES];
    let first_len = sealed
        .read(&mut input)
        .map_err(|_| "encrypted attachment could not be read".to_owned())?;
    let (mut decryptor, header_len) = StreamDecryptor::new(key, &input[..first_len])
        .map_err(|_| "encrypted attachment header is invalid".to_owned())?;
    let plaintext_len = decryptor.header().plaintext_len;
    if plaintext_len > MAX_PLAINTEXT_BYTES {
        return Err("encrypted attachment exceeds the plaintext limit".to_owned());
    }

    let (temporary, final_path, mut output) = create_output(app_local_data_dir, "opened")?;
    let cleanup = PartialFile(temporary.clone());
    let result: Result<(), String> = (|| {
        let mut feed = |ciphertext: &[u8]| -> Result<(), String> {
            let mut plaintext = decryptor
                .write(ciphertext)
                .map_err(|_| "encrypted attachment authentication failed".to_owned())?;
            let write_result = output
                .write_all(&plaintext)
                .map_err(|_| "decrypted attachment could not be written".to_owned());
            plaintext.zeroize();
            write_result
        };
        feed(&input[header_len..first_len])?;
        loop {
            let read = sealed
                .read(&mut input)
                .map_err(|_| "encrypted attachment could not be read".to_owned())?;
            if read == 0 {
                break;
            }
            feed(&input[..read])?;
        }
        decryptor
            .finalize()
            .map_err(|_| "encrypted attachment is truncated or invalid".to_owned())?;
        output
            .sync_all()
            .map_err(|_| "decrypted attachment could not be synchronized".to_owned())?;
        drop(output);
        std::fs::rename(&temporary, &final_path)
            .map_err(|_| "decrypted attachment could not be committed".to_owned())?;
        Ok(())
    })();
    input.zeroize();
    result?;
    std::mem::forget(cleanup);

    Ok(StagedPlaintext::new(StagedAttachment {
        root: app_local_data_dir.to_owned(),
        path: final_path,
        original_filename: original_filename.to_owned(),
        mime_type,
        plaintext_len,
    }))
}

/// Authenticate and decrypt an attachment into OSL-owned process memory.
/// This is reserved for the capture-protected native image viewer: no
/// plaintext staging file is created and the returned allocation zeroizes on
/// every exit path.
pub fn decrypt_file_to_memory(
    sealed: &mut File,
    original_filename: &str,
    declared_mime: &str,
    key: aead::Key,
) -> Result<Zeroizing<Vec<u8>>, String> {
    validate_metadata(original_filename, declared_mime)?;
    let sealed_len = sealed
        .metadata()
        .map_err(|_| "encrypted attachment metadata could not be read".to_owned())?
        .len();
    if sealed_len == 0 || sealed_len > MAX_SEALED_BYTES {
        return Err("encrypted attachment has an invalid size".to_owned());
    }
    sealed
        .seek(SeekFrom::Start(0))
        .map_err(|_| "encrypted attachment could not be read".to_owned())?;

    let mut input = [0u8; MAX_IO_BUFFER_BYTES];
    let first_len = sealed
        .read(&mut input)
        .map_err(|_| "encrypted attachment could not be read".to_owned())?;
    let (mut decryptor, header_len) = StreamDecryptor::new(key, &input[..first_len])
        .map_err(|_| "encrypted attachment header is invalid".to_owned())?;
    let plaintext_len = decryptor.header().plaintext_len;
    if plaintext_len > MAX_PLAINTEXT_BYTES || plaintext_len > usize::MAX as u64 {
        input.zeroize();
        return Err("encrypted attachment exceeds the plaintext limit".to_owned());
    }
    let mut output = Zeroizing::new(Vec::new());
    output
        .try_reserve_exact(plaintext_len as usize)
        .map_err(|_| "OSL could not reserve protected image memory".to_owned())?;
    let result: Result<(), String> = (|| {
        let mut feed = |ciphertext: &[u8]| -> Result<(), String> {
            let mut plaintext = decryptor
                .write(ciphertext)
                .map_err(|_| "encrypted attachment authentication failed".to_owned())?;
            let next_len = output
                .len()
                .checked_add(plaintext.len())
                .ok_or_else(|| "decrypted attachment size is invalid".to_owned())?;
            if next_len > plaintext_len as usize {
                plaintext.zeroize();
                return Err("decrypted attachment size is invalid".to_owned());
            }
            output.extend_from_slice(&plaintext);
            plaintext.zeroize();
            Ok(())
        };
        feed(&input[header_len..first_len])?;
        loop {
            let read = sealed
                .read(&mut input)
                .map_err(|_| "encrypted attachment could not be read".to_owned())?;
            if read == 0 {
                break;
            }
            feed(&input[..read])?;
        }
        decryptor
            .finalize()
            .map_err(|_| "encrypted attachment is truncated or invalid".to_owned())?;
        if output.len() != plaintext_len as usize {
            return Err("decrypted attachment size is invalid".to_owned());
        }
        Ok(())
    })();
    input.zeroize();
    result?;
    Ok(output)
}

/// Remove one staged file. The path shape is re-validated and an already
/// absent file is success, so a duplicate removal cannot be mistaken for a
/// staging leak.
pub fn remove_staged_file(staged: StagedAttachment) -> Result<(), String> {
    remove_staging_path_in_root(&staged.root, &staged.path)
}

// ---------------------------------------------------------------------------
// Decrypted plaintext lifetime
// ---------------------------------------------------------------------------

/// Count of decrypted staging files OSL failed to remove since launch.
static UNREMOVED_PLAINTEXT_FILES: AtomicU64 = AtomicU64::new(0);

/// RAII owner of the one decrypted plaintext staging file produced by
/// [`decrypt_file`].
///
/// Non-image attachments must exist as a real path because an external Windows
/// shell handler reads them; images never touch disk and use
/// [`decrypt_file_to_memory`] instead. Every early return in the open path
/// drops this guard, so removal no longer depends on a caller remembering one
/// more `remove_staged_file` call, and a removal that still fails is counted
/// instead of discarded.
pub struct StagedPlaintext {
    staged: Option<StagedAttachment>,
}

impl StagedPlaintext {
    pub fn new(staged: StagedAttachment) -> Self {
        Self {
            staged: Some(staged),
        }
    }

    pub fn plaintext_len(&self) -> u64 {
        self.staged
            .as_ref()
            .map_or(0, StagedAttachment::plaintext_len)
    }

    pub fn original_filename(&self) -> &str {
        self.staged
            .as_ref()
            .map_or("", StagedAttachment::original_filename)
    }

    pub fn path(&self) -> Option<&Path> {
        self.staged.as_ref().map(StagedAttachment::path)
    }

    pub fn root(&self) -> Option<&Path> {
        self.staged.as_ref().map(StagedAttachment::root)
    }

    /// Remove the decrypted file now and surface a failure to the caller.
    pub fn remove_now(mut self) -> Result<(), String> {
        match self.staged.take() {
            Some(staged) => remove_plaintext_with_retries(&staged.root, &staged.path),
            None => Ok(()),
        }
    }

    /// Hand the file to an external reader that needs a real path. The caller
    /// takes over removal and must arrange one that does not depend on this
    /// process staying alive.
    pub fn release_to_external_reader(mut self) -> Option<StagedAttachment> {
        self.staged.take()
    }
}

impl Drop for StagedPlaintext {
    fn drop(&mut self) {
        if let Some(staged) = self.staged.take() {
            if remove_plaintext_with_retries(&staged.root, &staged.path).is_err() {
                note_unremoved_plaintext_file();
            }
        }
    }
}

/// Number of decrypted staging files OSL could not remove since launch. A
/// non-zero value means plaintext may remain on disk until the next startup
/// sweep, and callers must say so rather than reporting a clean open.
pub fn unremoved_plaintext_files() -> u64 {
    UNREMOVED_PLAINTEXT_FILES.load(Ordering::Acquire)
}

/// Record that a decrypted staging file outlived the operation that created it.
/// Used by external-reader handoff, where removal happens off the caller's
/// thread and cannot return an error.
pub fn note_unremoved_plaintext_file() {
    UNREMOVED_PLAINTEXT_FILES.fetch_add(1, Ordering::AcqRel);
}

/// Remove a decrypted staging file with bounded retries. A Windows reader that
/// still holds the file briefly must not turn into a permanent plaintext leak.
pub fn remove_plaintext_with_retries(root: &Path, path: &Path) -> Result<(), String> {
    let mut attempt = 0u32;
    loop {
        match remove_staging_path_in_root(root, path) {
            Ok(()) => return Ok(()),
            Err(error) => {
                attempt = attempt.saturating_add(1);
                if attempt >= PLAINTEXT_REMOVAL_ATTEMPTS {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(
                    25u64.saturating_mul(u64::from(attempt)),
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Distinguishable transport outcomes
// ---------------------------------------------------------------------------

/// One honest outcome per cipher-store failure. A rejected capability, an
/// expired object, a rate limit, an oversize body, an unsupported lifetime and
/// an unreachable network are separate facts: collapsing them lets a 403
/// capability mismatch read to the user as "expired".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportOutcome {
    Unreachable,
    TimedOut,
    RateLimited,
    CapabilityRejected,
    Gone,
    TooLarge,
    UnsupportedLifetime,
    ServerFault,
    MalformedResponse,
    LocalIo,
    Refused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportPhase {
    Upload,
    Fetch,
    Delete,
}

pub fn classify_cipher_store_error(
    error: &ipc::cipher_store_client::CipherStoreError,
) -> TransportOutcome {
    use ipc::cipher_store_client::CipherStoreError as Raw;
    match error {
        Raw::BadTtl(_) => TransportOutcome::UnsupportedLifetime,
        Raw::BlobTooLarge { .. } => TransportOutcome::TooLarge,
        Raw::NotFound => TransportOutcome::Gone,
        Raw::RateLimited => TransportOutcome::RateLimited,
        Raw::ParseError(_) => TransportOutcome::MalformedResponse,
        Raw::Io(_) => TransportOutcome::LocalIo,
        Raw::Status { status, .. } => match *status {
            401 | 403 => TransportOutcome::CapabilityRejected,
            404 | 410 => TransportOutcome::Gone,
            413 => TransportOutcome::TooLarge,
            429 => TransportOutcome::RateLimited,
            500..=599 => TransportOutcome::ServerFault,
            _ => TransportOutcome::Refused,
        },
        Raw::Network(network) => {
            if network.is_timeout() {
                TransportOutcome::TimedOut
            } else if network.is_decode() {
                TransportOutcome::MalformedResponse
            } else {
                TransportOutcome::Unreachable
            }
        }
    }
}

/// Render one outcome for the user. Server-supplied response bodies are never
/// included, so no remote text can reach a toast.
pub fn describe_transport_outcome(outcome: TransportOutcome, phase: TransportPhase) -> String {
    let subject = match phase {
        TransportPhase::Upload => "This private attachment could not be uploaded",
        TransportPhase::Fetch => "This private attachment could not be retrieved",
        TransportPhase::Delete => "OSL could not delete this private attachment from its storage",
    };
    let reason = match outcome {
        TransportOutcome::Unreachable => "OSL could not reach its encrypted attachment storage",
        TransportOutcome::TimedOut => "the encrypted attachment storage did not answer in time",
        TransportOutcome::RateLimited => {
            "the encrypted attachment storage is rate limiting this device, so wait and retry"
        }
        TransportOutcome::CapabilityRejected => {
            "the encrypted attachment storage rejected this device's capability for it"
        }
        TransportOutcome::Gone => "it has already expired or been burned",
        TransportOutcome::TooLarge => "it exceeds the encrypted attachment size limit",
        TransportOutcome::UnsupportedLifetime => {
            "its requested lifetime is not one OSL storage accepts"
        }
        TransportOutcome::ServerFault => {
            "the encrypted attachment storage reported a fault on its side"
        }
        TransportOutcome::MalformedResponse => {
            "the encrypted attachment storage returned an unexpected response"
        }
        TransportOutcome::LocalIo => "OSL could not read the sealed copy on this device",
        TransportOutcome::Refused => "the encrypted attachment storage refused the request",
    };
    format!("{subject}: {reason}.")
}

pub fn describe_cipher_store_error(
    error: &ipc::cipher_store_client::CipherStoreError,
    phase: TransportPhase,
) -> String {
    describe_transport_outcome(classify_cipher_store_error(error), phase)
}

// ---------------------------------------------------------------------------
// Durable deletion outbox
// ---------------------------------------------------------------------------

#[derive(Default, Deserialize, Serialize)]
struct DeletionOutbox {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    entries: Vec<DeletionRecord>,
}

#[derive(Clone, Deserialize, Serialize)]
struct DeletionRecord {
    object_id: String,
    fetch_token: String,
    expires_at: i64,
    #[serde(default)]
    attempts: u32,
    #[serde(default)]
    view_once: bool,
}

impl Drop for DeletionRecord {
    fn drop(&mut self) {
        self.fetch_token.zeroize();
    }
}

/// One outstanding remote deletion handed to the caller for a retry attempt.
pub struct PendingDeletion {
    pub object_id: String,
    pub fetch_token: [u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES],
    pub view_once: bool,
    pub attempts: u32,
}

impl Drop for PendingDeletion {
    fn drop(&mut self) {
        self.fetch_token.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeletionAttempt {
    /// The remote object is confirmed gone; drop the record.
    Deleted,
    /// The store says the object no longer exists; drop the record.
    AlreadyGone,
    /// Keep the record and try again later.
    Retry,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeletionDrainReport {
    pub deleted: usize,
    pub already_gone: usize,
    pub retained: usize,
    /// Records whose server TTL had already elapsed, so the object is gone
    /// whether or not OSL ever reached the store. Removing these is not
    /// eviction of a live record.
    pub expired: usize,
}

fn canonical_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn decode_fetch_token(
    value: &str,
) -> Option<[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES]> {
    if !canonical_lower_hex(value, FETCH_TOKEN_HEX_LEN) {
        return None;
    }
    let mut output = [0u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).ok()?;
        output[index] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(output)
}

fn load_deletion_outbox(path: &Path, key: &[u8; 32]) -> Result<DeletionOutbox, String> {
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_DELETION_OUTBOX_BYTES,
        "OSL attachment deletion outbox",
    )?
    else {
        return Ok(DeletionOutbox::default());
    };
    if !ipc::main_password::has_enc_magic(&bytes) {
        return Err("OSL attachment deletion outbox is not encrypted".to_owned());
    }
    let plain = Zeroizing::new(
        ipc::main_password::decrypt_at_rest(&bytes, key)
            .map_err(|_| "OSL attachment deletion outbox could not be opened".to_owned())?,
    );
    let outbox: DeletionOutbox = serde_json::from_slice(&plain)
        .map_err(|_| "OSL attachment deletion outbox is malformed".to_owned())?;
    if !matches!(outbox.version, 0 | 1) || outbox.entries.len() > MAX_DELETION_OUTBOX_ENTRIES {
        return Err("OSL attachment deletion outbox is malformed".to_owned());
    }
    // A record OSL cannot act on must fail closed rather than sit in the
    // outbox pretending a deletion is still pending.
    for entry in &outbox.entries {
        if !canonical_lower_hex(&entry.object_id, OBJECT_ID_HEX_LEN)
            || !canonical_lower_hex(&entry.fetch_token, FETCH_TOKEN_HEX_LEN)
        {
            return Err("OSL attachment deletion outbox is malformed".to_owned());
        }
    }
    Ok(outbox)
}

fn store_deletion_outbox(
    path: &Path,
    outbox: &DeletionOutbox,
    key: &[u8; 32],
) -> Result<(), String> {
    let mut body = serde_json::to_vec(outbox)
        .map_err(|_| "OSL attachment deletion outbox could not be encoded".to_owned())?;
    let sealed = ipc::main_password::encrypt_at_rest(&body, key)
        .map_err(|_| "OSL attachment deletion outbox could not be encrypted".to_owned());
    body.zeroize();
    let sealed = sealed?;
    if sealed.len() as u64 > MAX_DELETION_OUTBOX_BYTES {
        return Err("OSL attachment deletion outbox exceeds its storage limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &sealed, "OSL attachment deletion outbox")
}

/// Durably record one remote object OSL must delete. Called only after an
/// inline delete failed, so a failed rollback can never leave ciphertext in
/// remote storage for the full TTL without a retry owner.
pub fn enqueue_attachment_deletion_at_path(
    path: &Path,
    key: &[u8; 32],
    object_id: &str,
    fetch_token: &str,
    expires_at: i64,
    now: i64,
    view_once: bool,
) -> Result<(), String> {
    if !canonical_lower_hex(object_id, OBJECT_ID_HEX_LEN)
        || !canonical_lower_hex(fetch_token, FETCH_TOKEN_HEX_LEN)
        || expires_at <= now
        || expires_at > now.saturating_add(MAX_DELETION_TTL_SECONDS)
    {
        return Err("OSL attachment deletion record is invalid".to_owned());
    }
    let mut outbox = load_deletion_outbox(path, key)?;
    outbox.entries.retain(|entry| entry.expires_at > now);
    if let Some(existing) = outbox
        .entries
        .iter_mut()
        .find(|entry| entry.object_id == object_id)
    {
        existing.fetch_token.zeroize();
        existing.fetch_token = fetch_token.to_owned();
        existing.expires_at = expires_at;
        existing.view_once = existing.view_once || view_once;
    } else {
        // Bounded and append-only. A full outbox is rejected so a live
        // deletion promise is never silently evicted to make room.
        if outbox.entries.len() >= MAX_DELETION_OUTBOX_ENTRIES {
            return Err("OSL attachment deletion outbox is full".to_owned());
        }
        outbox.entries.push(DeletionRecord {
            object_id: object_id.to_owned(),
            fetch_token: fetch_token.to_owned(),
            expires_at,
            attempts: 0,
            view_once,
        });
    }
    outbox.version = 1;
    store_deletion_outbox(path, &outbox, key)
}

/// Retry every outstanding deletion. Records survive an unlimited number of
/// failed attempts and leave only on confirmed deletion or after their server
/// TTL has elapsed.
pub fn drain_attachment_deletions_at_path<F>(
    path: &Path,
    key: &[u8; 32],
    now: i64,
    mut attempt: F,
) -> Result<DeletionDrainReport, String>
where
    F: FnMut(&PendingDeletion) -> DeletionAttempt,
{
    let mut outbox = load_deletion_outbox(path, key)?;
    let entries = std::mem::take(&mut outbox.entries);
    if entries.is_empty() {
        // Nothing owed, so do not create or rewrite the outbox on a routine pass.
        return Ok(DeletionDrainReport::default());
    }
    let mut report = DeletionDrainReport::default();
    let mut retained: Vec<DeletionRecord> = Vec::new();
    for entry in entries {
        if entry.expires_at <= now {
            report.expired += 1;
            continue;
        }
        let Some(fetch_token) = decode_fetch_token(&entry.fetch_token) else {
            return Err("OSL attachment deletion outbox is malformed".to_owned());
        };
        let pending = PendingDeletion {
            object_id: entry.object_id.clone(),
            fetch_token,
            view_once: entry.view_once,
            attempts: entry.attempts,
        };
        match attempt(&pending) {
            DeletionAttempt::Deleted => report.deleted += 1,
            DeletionAttempt::AlreadyGone => report.already_gone += 1,
            DeletionAttempt::Retry => {
                let mut kept = entry.clone();
                kept.attempts = kept.attempts.saturating_add(1);
                retained.push(kept);
                report.retained += 1;
            }
        }
    }
    outbox.entries = retained;
    outbox.version = 1;
    store_deletion_outbox(path, &outbox, key)?;
    Ok(report)
}

pub fn pending_attachment_deletions_at_path(
    path: &Path,
    key: &[u8; 32],
) -> Result<usize, String> {
    Ok(load_deletion_outbox(path, key)?.entries.len())
}

fn deletion_outbox_path() -> Result<PathBuf, String> {
    Ok(keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?
        .join(DELETION_OUTBOX_FILE))
}

/// Durably record a remote deletion OSL owes. Fails loudly when the outbox
/// cannot be written, so the caller reports an orphaned object rather than
/// discarding the promise.
pub fn enqueue_attachment_deletion(
    object_id: &str,
    fetch_token: &str,
    expires_at: i64,
    view_once: bool,
) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL must be unlocked to record an attachment deletion".to_owned())?;
    enqueue_attachment_deletion_at_path(
        &deletion_outbox_path()?,
        &key,
        object_id,
        fetch_token,
        expires_at,
        ipc::main_password::now_unix_secs_pub(),
        view_once,
    )
}

/// Retry outstanding deletions against live storage. While locked there is
/// nothing readable to retry; the enqueue path is the one that fails loudly.
pub fn drain_attachment_deletions<F>(attempt: F) -> Result<DeletionDrainReport, String>
where
    F: FnMut(&PendingDeletion) -> DeletionAttempt,
{
    let Some(key) = ipc::main_password::get_file_storage_key() else {
        return Ok(DeletionDrainReport::default());
    };
    drain_attachment_deletions_at_path(
        &deletion_outbox_path()?,
        &key,
        ipc::main_password::now_unix_secs_pub(),
        attempt,
    )
}

pub fn sha256_file(path: &Path) -> Result<([u8; 32], u64), String> {
    let mut file =
        File::open(path).map_err(|_| "staged attachment could not be read".to_owned())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; MAX_IO_BUFFER_BYTES];
    let mut total = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "staged attachment could not be read".to_owned())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "staged attachment size is invalid".to_owned())?;
        if total > MAX_SEALED_BYTES {
            buffer.zeroize();
            return Err("staged attachment exceeds the sealed limit".to_owned());
        }
        hash.update(&buffer[..read]);
    }
    buffer.zeroize();
    Ok((hash.finalize().into(), total))
}

/// Create one OSL-owned partial file for a bounded streaming download. The
/// caller must remove the returned path on every failure and after decrypting.
pub fn create_download_file(app_local_data_dir: &Path) -> Result<(PathBuf, File), String> {
    let (temporary, _unused_final, file) = create_output(app_local_data_dir, "download")?;
    Ok((temporary, file))
}

pub fn remove_staging_path_in_root(root: &Path, path: &Path) -> Result<(), String> {
    validate_staging_path_in_root(root, path)?;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("staged attachment could not be removed".to_owned()),
    }
}

fn validate_staging_path_in_root(root: &Path, path: &Path) -> Result<(), String> {
    let staging = staging_directory_for_root(root)?;
    if path.parent() != Some(staging.as_path()) {
        return Err("staged attachment path is invalid".to_owned());
    }
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if !(filename.starts_with("download-")
        || filename.starts_with("sealed-")
        || filename.starts_with("opened-"))
        || !(filename.ends_with(".part") || filename.ends_with(".oslatt"))
    {
        return Err("staged attachment path is invalid".to_owned());
    }
    Ok(())
}

/// Remove every abandoned sealed, download, and plaintext staging file before
/// an identity can unlock. Unknown files or links fail closed rather than
/// being followed or silently retained.
pub fn scavenge_staging_on_startup(app_local_data_dir: &Path) -> Result<(), String> {
    let staging = staging_directory_for_root(app_local_data_dir)?;
    let metadata = match std::fs::symlink_metadata(&staging) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("OSL attachment staging could not be checked".to_owned()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("OSL attachment staging is unsafe".to_owned());
    }
    for entry in std::fs::read_dir(&staging)
        .map_err(|_| "OSL attachment staging could not be read".to_owned())?
    {
        let entry = entry.map_err(|_| "OSL attachment staging could not be read".to_owned())?;
        let file_type = entry
            .file_type()
            .map_err(|_| "OSL attachment staging entry could not be checked".to_owned())?;
        if !file_type.is_file() {
            return Err("OSL attachment staging contains an unsafe entry".to_owned());
        }
        remove_staging_path_in_root(app_local_data_dir, &entry.path())?;
    }
    Ok(())
}

fn validate_metadata(filename: &str, declared_mime: &str) -> Result<&'static str, String> {
    if filename.is_empty()
        || filename.len() > ipc::attachment_wire::MAX_FILENAME_LEN
        || filename.contains(['/', '\\', ':'])
        || filename
            != Path::new(filename)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
        || filename.chars().any(char::is_control)
    {
        return Err("attachment filename is invalid".to_owned());
    }
    let expected = ipc::attachment_wire::mime_for_filename(filename)
        .ok_or_else(|| "attachment type is not supported".to_owned())?;
    if declared_mime != expected {
        return Err("attachment MIME type does not match its filename".to_owned());
    }
    Ok(expected)
}

fn create_output(root: &Path, kind: &str) -> Result<(PathBuf, PathBuf, File), String> {
    let staging = staging_directory_for_root(root)?;
    std::fs::create_dir_all(&staging)
        .map_err(|_| "OSL attachment staging directory could not be created".to_owned())?;
    let metadata = std::fs::symlink_metadata(&staging)
        .map_err(|_| "OSL attachment staging directory could not be checked".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("OSL attachment staging directory is unsafe".to_owned());
    }

    for _ in 0..8 {
        let token = crypto::random::random_bytes(16);
        let suffix: String = token.iter().map(|byte| format!("{byte:02x}")).collect();
        let temporary = staging.join(format!("{kind}-{suffix}.part"));
        let final_path = staging.join(format!("{kind}-{suffix}.oslatt"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, final_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("OSL attachment staging file could not be created".to_owned()),
        }
    }
    Err("OSL attachment staging name could not be allocated".to_owned())
}

fn staging_directory_for_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute()
        || root.parent().is_none()
        || root
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("OSL attachment root is invalid".to_owned());
    }
    Ok(root.join(STAGING_DIRECTORY))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "osl-peer-attachment-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn key() -> aead::Key {
        aead::Key::from_bytes([7u8; aead::KEY_SIZE])
    }

    fn round_trip(label: &str, bytes: &[u8]) {
        let root = root(label);
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        std::fs::write(&source_path, bytes).unwrap();
        let mut source = File::open(&source_path).unwrap();
        let sealed = encrypt_file(
            &root,
            &mut source,
            "photo.png",
            "image/png",
            key(),
            vec![3u8; 16],
            0,
        )
        .unwrap();
        let mut sealed_file = File::open(&sealed.path).unwrap();
        let opened =
            decrypt_file(&root, &mut sealed_file, "photo.png", "image/png", key()).unwrap();
        let opened_path = opened.path().unwrap().to_owned();
        assert_eq!(std::fs::read(&opened_path).unwrap(), bytes);
        remove_staged_file(sealed).unwrap();
        opened.remove_now().unwrap();
        assert!(!opened_path.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn zero_small_and_multichunk_files_round_trip() {
        round_trip("zero", &[]);
        round_trip("small", b"private attachment");
        round_trip("multi", &vec![0x5a; ATTACHMENT_CHUNK_SIZE * 3 + 117]);
    }

    #[test]
    fn streamed_plaintext_bound_is_512_mib_without_allocating_it() {
        let root = root("maximum-bound");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        let source = File::create(&source_path).unwrap();
        source.set_len(MAX_PLAINTEXT_BYTES).unwrap();
        assert_eq!(source.metadata().unwrap().len(), 512 * 1024 * 1024);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn oversize_input_is_rejected_without_staging_output() {
        let root = root("oversize");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        let source = File::create(&source_path).unwrap();
        source.set_len(MAX_PLAINTEXT_BYTES + 1).unwrap();
        drop(source);
        let mut source = File::open(&source_path).unwrap();
        assert!(encrypt_file(
            &root,
            &mut source,
            "photo.png",
            "image/png",
            key(),
            vec![1u8; 16],
            0,
        )
        .is_err());
        assert!(!root.join(STAGING_DIRECTORY).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn truncated_ciphertext_fails_and_removes_partial_plaintext() {
        let root = root("truncated");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        std::fs::write(&source_path, b"secret").unwrap();
        let mut source = File::open(&source_path).unwrap();
        let sealed = encrypt_file(
            &root,
            &mut source,
            "photo.png",
            "image/png",
            key(),
            vec![2u8; 16],
            0,
        )
        .unwrap();
        let file = OpenOptions::new().write(true).open(&sealed.path).unwrap();
        file.set_len(file.metadata().unwrap().len() - 1).unwrap();
        drop(file);
        let mut sealed_file = File::open(&sealed.path).unwrap();
        assert!(decrypt_file(&root, &mut sealed_file, "photo.png", "image/png", key(),).is_err());
        let staging = root.join(STAGING_DIRECTORY);
        assert_eq!(
            std::fs::read_dir(staging)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.path().extension().and_then(|v| v.to_str()) == Some("part"))
                .count(),
            0
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn protected_image_decrypts_only_to_zeroizing_memory() {
        let root = root("in-memory-image");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        let bytes = b"private image bytes";
        std::fs::write(&source_path, bytes).unwrap();
        let mut source = File::open(&source_path).unwrap();
        let sealed = encrypt_file(
            &root,
            &mut source,
            "photo.png",
            "image/png",
            key(),
            vec![9u8; 16],
            0,
        )
        .unwrap();
        let mut sealed_file = File::open(sealed.path()).unwrap();
        let opened =
            decrypt_file_to_memory(&mut sealed_file, "photo.png", "image/png", key()).unwrap();
        assert_eq!(opened.as_slice(), bytes);
        assert_eq!(
            std::fs::read_dir(root.join(STAGING_DIRECTORY))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("opened-"))
                .count(),
            0
        );
        remove_staged_file(sealed).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_and_buffer_bounds_are_fixed() {
        assert_eq!(MAX_IO_BUFFER_BYTES, 16 * 1024);
        assert_eq!(MAX_PLAINTEXT_BYTES, 512 * 1024 * 1024);
        assert!(validate_metadata("../photo.png", "image/png").is_err());
        assert!(validate_metadata("photo.png", "video/mp4").is_err());
        assert_eq!(
            validate_metadata("notes.txt", "text/plain").unwrap(),
            "text/plain"
        );
        assert!(validate_metadata("installer.exe", "application/octet-stream").is_err());
        assert!(validate_metadata("script.ps1", "text/plain").is_err());
        assert!(supported_protected_image_mime("image/png"));
        assert!(supported_protected_image_mime("image/jpeg"));
        assert!(!supported_protected_image_mime("image/gif"));
        assert!(!supported_protected_image_mime("image/webp"));
        assert!(!supported_protected_image_mime("application/pdf"));
    }

    #[test]
    fn streaming_hash_and_download_staging_are_bounded() {
        let root = root("download");
        std::fs::create_dir_all(&root).unwrap();
        let (path, mut file) = create_download_file(&root).unwrap();
        file.write_all(b"sealed bytes").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let (hash, size) = sha256_file(&path).unwrap();
        assert_eq!(size, 12);
        assert_ne!(hash, [0u8; 32]);
        remove_staging_path_in_root(&root, &path).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn staging_cleanup_combines_caller_root_with_the_staging_directory() {
        let caller_root = root("caller-root");
        let foreign_root = root("foreign-root");
        let caller_staging = caller_root.join(STAGING_DIRECTORY);
        let foreign_staging = foreign_root.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&caller_staging).unwrap();
        std::fs::create_dir_all(&foreign_staging).unwrap();
        let foreign_path = foreign_staging.join("opened-00112233445566778899aabbccddeeff.oslatt");
        std::fs::write(&foreign_path, b"not this caller's staging file").unwrap();

        assert_eq!(
            remove_staging_path_in_root(&caller_root, &foreign_path),
            Err("staged attachment path is invalid".to_owned())
        );
        assert!(
            foreign_path.exists(),
            "a matching staging filename under a different caller root is refused, not removed"
        );

        let caller_path = caller_staging.join("opened-ffeeddccbbaa99887766554433221100.oslatt");
        std::fs::write(&caller_path, b"this caller's staging file").unwrap();
        remove_staging_path_in_root(&caller_root, &caller_path).unwrap();
        assert!(!caller_path.exists());

        let dotted_root = caller_root.join("..").join(
            caller_root
                .file_name()
                .expect("test root has a final component"),
        );
        assert_eq!(
            create_download_file(&dotted_root).map(|_| ()),
            Err("OSL attachment root is invalid".to_owned())
        );

        let _ = std::fs::remove_dir_all(caller_root);
        let _ = std::fs::remove_dir_all(foreign_root);
    }

    #[test]
    fn staging_and_caller_root_must_match_before_plaintext_open() {
        let caller_root = root("plaintext-caller-root");
        let foreign_root = root("plaintext-foreign-root");
        let caller_staging = caller_root.join(STAGING_DIRECTORY);
        let foreign_staging = foreign_root.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&caller_staging).unwrap();
        std::fs::create_dir_all(&foreign_staging).unwrap();

        let foreign_path = foreign_staging.join("opened-00112233445566778899aabbccddeeff.oslatt");
        std::fs::write(&foreign_path, b"foreign plaintext").unwrap();
        let refused = StagedPlaintext::new(StagedAttachment {
            root: caller_root.clone(),
            path: foreign_path.clone(),
            original_filename: "notes.txt".to_owned(),
            mime_type: "text/plain",
            plaintext_len: 17,
        })
        .remove_now();
        assert_eq!(
            refused,
            Err("staged attachment path is invalid".to_owned()),
            "a caller-root mismatch must refuse before treating a foreign path as opened plaintext"
        );
        assert!(
            foreign_path.exists(),
            "the foreign staged plaintext must not be removed through the caller root"
        );

        let caller_path = caller_staging.join("opened-ffeeddccbbaa99887766554433221100.oslatt");
        std::fs::write(&caller_path, b"caller plaintext").unwrap();
        StagedPlaintext::new(StagedAttachment {
            root: caller_root.clone(),
            path: caller_path.clone(),
            original_filename: "notes.txt".to_owned(),
            mime_type: "text/plain",
            plaintext_len: 16,
        })
        .remove_now()
        .unwrap();
        assert!(!caller_path.exists());

        let _ = std::fs::remove_dir_all(caller_root);
        let _ = std::fs::remove_dir_all(foreign_root);
    }

    fn outbox_key() -> [u8; 32] {
        [0x31u8; 32]
    }

    /// 32 lowercase hex characters, unique per index, matching the shape the
    /// cipher store assigns.
    fn object_id(index: usize) -> String {
        format!("{index:032x}")
    }

    fn fetch_token_hex(seed: usize) -> String {
        assert_eq!(FETCH_TOKEN_HEX_LEN, 32);
        format!("{:032x}", seed.wrapping_add(0x5a5a))
    }

    #[test]
    fn non_image_open_policy_refuses_without_creating_plaintext() {
        let root = root("non-image-refusal");
        std::fs::create_dir_all(&root).unwrap();
        let durable_plaintext = root.join("would-be-plaintext.txt");
        let download_reached = std::cell::Cell::new(false);
        let decrypt_reached = std::cell::Cell::new(false);

        let refusal = require_protected_attachment_viewer("application/pdf").and_then(|_| {
            download_reached.set(true);
            let (_download_path, _download) = create_download_file(&root)?;
            decrypt_reached.set(true);
            std::fs::write(&durable_plaintext, b"plaintext")
                .map_err(|_| "test durable plaintext write failed".to_owned())?;
            Ok(())
        });

        assert!(
            !download_reached.get(),
            "non-image refusal must happen before download"
        );
        assert!(
            !decrypt_reached.get(),
            "non-image refusal must happen before decrypt"
        );
        assert!(
            !root.join(STAGING_DIRECTORY).exists(),
            "non-image refusal must not create download staging"
        );
        assert!(
            !durable_plaintext.exists(),
            "non-image refusal must not create durable plaintext"
        );
        assert_eq!(refusal, Err(EXTERNAL_VIEWER_REFUSAL.to_owned()));

        let image_open_reached = std::cell::Cell::new(false);
        let positive = require_protected_attachment_viewer("image/png").map(|_| {
            image_open_reached.set(true);
            "protected viewer reached"
        });
        assert_eq!(positive.unwrap(), "protected viewer reached");
        assert!(image_open_reached.get());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dropping_a_decrypted_file_removes_the_plaintext_without_a_caller_call() {
        let root = root("guard-drop");
        std::fs::create_dir_all(&root).unwrap();
        let source_path = root.join("source");
        std::fs::write(&source_path, b"private document bytes").unwrap();
        let mut source = File::open(&source_path).unwrap();
        let sealed = encrypt_file(
            &root,
            &mut source,
            "notes.txt",
            "text/plain",
            key(),
            vec![4u8; 16],
            0,
        )
        .unwrap();
        let mut sealed_file = File::open(sealed.path()).unwrap();
        let opened_path = {
            let opened =
                decrypt_file(&root, &mut sealed_file, "notes.txt", "text/plain", key()).unwrap();
            let path = opened.path().unwrap().to_owned();
            assert!(path.exists());
            path
        };
        assert!(!opened_path.exists());
        assert_eq!(
            std::fs::read_dir(root.join(STAGING_DIRECTORY))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().starts_with("opened-"))
                .count(),
            0
        );
        remove_staged_file(sealed).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_plaintext_that_cannot_be_removed_is_counted_not_discarded() {
        let root = root("guard-failure");
        let staging = root.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&staging).unwrap();
        // A directory at the staging path makes removal fail deterministically
        // on every platform without depending on file permissions.
        let blocked = staging.join("opened-00112233445566778899aabbccddeeff.oslatt");
        std::fs::create_dir(&blocked).unwrap();
        let before = unremoved_plaintext_files();
        drop(StagedPlaintext::new(StagedAttachment {
            root: root.clone(),
            path: blocked.clone(),
            original_filename: "notes.txt".to_owned(),
            mime_type: "text/plain",
            plaintext_len: 0,
        }));
        // A global counter under a parallel test runner can only be asserted
        // monotonically.
        assert!(unremoved_plaintext_files() >= before + 1);
        assert!(blocked.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn releasing_to_an_external_reader_transfers_removal_and_leaves_the_file() {
        let root = root("guard-release");
        let staging = root.join(STAGING_DIRECTORY);
        std::fs::create_dir_all(&staging).unwrap();
        let path = staging.join("opened-ffeeddccbbaa99887766554433221100.oslatt");
        std::fs::write(&path, b"external reader bytes").unwrap();
        let guard = StagedPlaintext::new(StagedAttachment {
            root: root.clone(),
            path: path.clone(),
            original_filename: "notes.txt".to_owned(),
            mime_type: "text/plain",
            plaintext_len: 21,
        });
        let released = guard.release_to_external_reader().unwrap();
        assert!(path.exists());
        remove_staged_file(released).unwrap();
        assert!(!path.exists());
        // A second removal of the same staging path is success, not a leak.
        assert!(remove_staging_path_in_root(&root, &path).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transport_failures_stay_distinguishable_and_403_never_reads_as_expired() {
        use ipc::cipher_store_client::CipherStoreError as Raw;
        let cases = [
            (
                Raw::Status {
                    status: 403,
                    body: "forbidden".to_owned(),
                },
                TransportOutcome::CapabilityRejected,
            ),
            (
                Raw::Status {
                    status: 401,
                    body: String::new(),
                },
                TransportOutcome::CapabilityRejected,
            ),
            (
                Raw::Status {
                    status: 404,
                    body: String::new(),
                },
                TransportOutcome::Gone,
            ),
            (Raw::NotFound, TransportOutcome::Gone),
            (
                Raw::Status {
                    status: 413,
                    body: String::new(),
                },
                TransportOutcome::TooLarge,
            ),
            (
                Raw::Status {
                    status: 429,
                    body: String::new(),
                },
                TransportOutcome::RateLimited,
            ),
            (Raw::RateLimited, TransportOutcome::RateLimited),
            (
                Raw::Status {
                    status: 503,
                    body: String::new(),
                },
                TransportOutcome::ServerFault,
            ),
            (
                Raw::Status {
                    status: 418,
                    body: String::new(),
                },
                TransportOutcome::Refused,
            ),
            (Raw::BadTtl(7), TransportOutcome::UnsupportedLifetime),
            (
                Raw::BlobTooLarge {
                    got: 2,
                    max: 1,
                },
                TransportOutcome::TooLarge,
            ),
            (
                Raw::ParseError("shape".to_owned()),
                TransportOutcome::MalformedResponse,
            ),
            (
                Raw::Io(std::io::Error::other("local")),
                TransportOutcome::LocalIo,
            ),
        ];
        for (error, expected) in &cases {
            assert_eq!(classify_cipher_store_error(error), *expected);
        }

        let rejected = describe_cipher_store_error(
            &Raw::Status {
                status: 403,
                body: "forbidden".to_owned(),
            },
            TransportPhase::Fetch,
        );
        let expired = describe_cipher_store_error(&Raw::NotFound, TransportPhase::Fetch);
        assert_ne!(rejected, expired);
        assert!(!rejected.contains("expired"));
        // A server-supplied body must never reach a user-facing string.
        assert!(!rejected.contains("forbidden"));

        let all = [
            TransportOutcome::Unreachable,
            TransportOutcome::TimedOut,
            TransportOutcome::RateLimited,
            TransportOutcome::CapabilityRejected,
            TransportOutcome::Gone,
            TransportOutcome::TooLarge,
            TransportOutcome::UnsupportedLifetime,
            TransportOutcome::ServerFault,
            TransportOutcome::MalformedResponse,
            TransportOutcome::LocalIo,
            TransportOutcome::Refused,
        ];
        for phase in [
            TransportPhase::Upload,
            TransportPhase::Fetch,
            TransportPhase::Delete,
        ] {
            let mut rendered: Vec<String> = all
                .iter()
                .map(|outcome| describe_transport_outcome(*outcome, phase))
                .collect();
            let total = rendered.len();
            rendered.sort();
            rendered.dedup();
            assert_eq!(rendered.len(), total);
        }
    }

    #[test]
    fn deletion_outbox_is_encrypted_bounded_and_rejects_instead_of_evicting() {
        let root = root("outbox-bound");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("deletions.json");
        let key = outbox_key();
        let now = 1_000_000i64;
        for index in 0..MAX_DELETION_OUTBOX_ENTRIES {
            enqueue_attachment_deletion_at_path(
                &path,
                &key,
                &object_id(index),
                &fetch_token_hex(index),
                now + 3_600,
                now,
                index % 2 == 0,
            )
            .unwrap();
        }
        assert_eq!(
            pending_attachment_deletions_at_path(&path, &key).unwrap(),
            MAX_DELETION_OUTBOX_ENTRIES
        );
        let overflow = enqueue_attachment_deletion_at_path(
            &path,
            &key,
            "aaaabbbbccccddddeeeeffff00001111",
            &fetch_token_hex(9),
            now + 3_600,
            now,
            true,
        );
        assert!(overflow.is_err());
        // Rejected, never trimmed: every live record survives the refusal.
        assert_eq!(
            pending_attachment_deletions_at_path(&path, &key).unwrap(),
            MAX_DELETION_OUTBOX_ENTRIES
        );

        let bytes = std::fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&bytes));
        let ciphertext = String::from_utf8_lossy(&bytes);
        assert!(!ciphertext.contains(&object_id(0)));
        assert!(!ciphertext.contains(&fetch_token_hex(0)));
        let mut foreign = key;
        foreign[0] ^= 0xff;
        assert!(pending_attachment_deletions_at_path(&path, &foreign).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deletion_outbox_retries_forever_and_only_leaves_on_success_or_ttl() {
        let root = root("outbox-drain");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("deletions.json");
        let key = outbox_key();
        let now = 2_000_000i64;
        enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(1),
            &fetch_token_hex(1),
            now + 3_600,
            now,
            true,
        )
        .unwrap();
        // Same object twice is one record, not a duplicate promise.
        enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(1),
            &fetch_token_hex(2),
            now + 7_200,
            now,
            false,
        )
        .unwrap();
        enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(2),
            &fetch_token_hex(3),
            now + 60,
            now,
            false,
        )
        .unwrap();
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 2);

        let mut seen_attempts = Vec::new();
        let report = drain_attachment_deletions_at_path(&path, &key, now, |pending| {
            seen_attempts.push((pending.object_id.clone(), pending.attempts));
            DeletionAttempt::Retry
        })
        .unwrap();
        assert_eq!(report.retained, 2);
        assert_eq!(report.deleted, 0);
        assert_eq!(report.expired, 0);
        assert!(seen_attempts.iter().all(|(_, attempts)| *attempts == 0));
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 2);

        // A repeated failure increments the attempt count and still keeps the
        // record: the promise is never abandoned after N tries.
        let mut attempts_second_round = Vec::new();
        drain_attachment_deletions_at_path(&path, &key, now, |pending| {
            attempts_second_round.push(pending.attempts);
            DeletionAttempt::Retry
        })
        .unwrap();
        assert_eq!(attempts_second_round, vec![1, 1]);
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 2);

        // The view-once flag survives a merge and reaches the retry closure.
        let mut view_once_flags = Vec::new();
        let report = drain_attachment_deletions_at_path(&path, &key, now + 120, |pending| {
            view_once_flags.push(pending.view_once);
            DeletionAttempt::Deleted
        })
        .unwrap();
        // The 60-second record's TTL passed, so its object is gone regardless.
        assert_eq!(report.expired, 1);
        assert_eq!(report.deleted, 1);
        assert_eq!(view_once_flags, vec![true]);
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 0);

        // AlreadyGone clears the record too.
        enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(3),
            &fetch_token_hex(4),
            now + 3_600,
            now,
            false,
        )
        .unwrap();
        let report =
            drain_attachment_deletions_at_path(&path, &key, now, |_| DeletionAttempt::AlreadyGone)
                .unwrap();
        assert_eq!(report.already_gone, 1);
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deletion_outbox_refuses_records_it_could_never_act_on() {
        let root = root("outbox-validate");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("deletions.json");
        let key = outbox_key();
        let now = 3_000_000i64;
        // Short object id, non-hex token, already expired, and beyond the
        // longest lifetime the store honours.
        assert!(enqueue_attachment_deletion_at_path(
            &path, &key, "abcd", &fetch_token_hex(1), now + 60, now, false
        )
        .is_err());
        assert!(enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(1),
            "not-hex-at-all-not-hex-at-all-xy",
            now + 60,
            now,
            false
        )
        .is_err());
        assert!(enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(1),
            &fetch_token_hex(1),
            now,
            now,
            false
        )
        .is_err());
        assert!(enqueue_attachment_deletion_at_path(
            &path,
            &key,
            &object_id(1),
            &fetch_token_hex(1),
            now + MAX_DELETION_TTL_SECONDS + 1,
            now,
            false
        )
        .is_err());
        assert_eq!(pending_attachment_deletions_at_path(&path, &key).unwrap(), 0);

        // A plaintext or corrupted outbox fails closed instead of being read.
        std::fs::write(&path, b"{\"version\":1,\"entries\":[]}").unwrap();
        assert!(pending_attachment_deletions_at_path(&path, &key).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn startup_scavenges_only_known_regular_staging_files() {
        let root = root("scavenge");
        std::fs::create_dir_all(&root).unwrap();
        let (download, _file) = create_download_file(&root).unwrap();
        scavenge_staging_on_startup(&root).unwrap();
        assert!(!download.exists());

        let staging = root.join(STAGING_DIRECTORY);
        std::fs::write(staging.join("unknown.bin"), b"private").unwrap();
        assert!(scavenge_staging_on_startup(&root).is_err());
        assert!(staging.join("unknown.bin").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
