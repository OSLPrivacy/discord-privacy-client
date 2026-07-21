//! Native, bounded-memory attachment staging.
//!
//! This module deliberately has no IPC surface: callers must already hold
//! validated file handles and an OSL-owned local-data root. Plaintext never
//! crosses the renderer boundary or passes through base64.

use crypto::aead;
use crypto::attachment::{StreamDecryptor, StreamEncryptor, ATTACHMENT_CHUNK_SIZE};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_PLAINTEXT_BYTES: u64 = ipc::attachment_wire::MAX_STREAMED_ATTACHMENT_BYTES;
pub const MAX_IO_BUFFER_BYTES: usize = ATTACHMENT_CHUNK_SIZE;

const STAGING_DIRECTORY: &str = "peer-attachment-staging";
const MAX_SEALED_BYTES: u64 = ipc::cipher_store_client::MAX_SEALED_ATTACHMENT_BYTES;

pub fn supported_protected_image_mime(mime: &str) -> bool {
    matches!(mime, "image/png" | "image/jpeg")
}

#[derive(Debug)]
pub struct StagedAttachment {
    path: PathBuf,
    original_filename: String,
    mime_type: &'static str,
    plaintext_len: u64,
}

impl StagedAttachment {
    pub fn path(&self) -> &Path {
        &self.path
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
        path: final_path,
        original_filename: original_filename.to_owned(),
        mime_type,
        plaintext_len,
    })
}

pub fn decrypt_file(
    app_local_data_dir: &Path,
    sealed: &mut File,
    original_filename: &str,
    declared_mime: &str,
    key: aead::Key,
) -> Result<StagedAttachment, String> {
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

    Ok(StagedAttachment {
        path: final_path,
        original_filename: original_filename.to_owned(),
        mime_type,
        plaintext_len,
    })
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

pub fn remove_staged_file(staged: StagedAttachment) -> Result<(), String> {
    std::fs::remove_file(staged.path)
        .map_err(|_| "staged attachment could not be removed".to_owned())
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

pub fn remove_staging_path(path: &Path) -> Result<(), String> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        != Some(STAGING_DIRECTORY)
        || !(filename.starts_with("download-")
            || filename.starts_with("sealed-")
            || filename.starts_with("opened-"))
        || !(filename.ends_with(".part") || filename.ends_with(".oslatt"))
    {
        return Err("staged attachment path is invalid".to_owned());
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("staged attachment could not be removed".to_owned()),
    }
}

/// Remove every abandoned sealed, download, and plaintext staging file before
/// an identity can unlock. Unknown files or links fail closed rather than
/// being followed or silently retained.
pub fn scavenge_staging_on_startup(app_local_data_dir: &Path) -> Result<(), String> {
    let staging = app_local_data_dir.join(STAGING_DIRECTORY);
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
        remove_staging_path(&entry.path())?;
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
    if !root.is_absolute() || root.parent().is_none() {
        return Err("OSL attachment root is invalid".to_owned());
    }
    let staging = root.join(STAGING_DIRECTORY);
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
        assert_eq!(std::fs::read(&opened.path).unwrap(), bytes);
        remove_staged_file(sealed).unwrap();
        remove_staged_file(opened).unwrap();
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
        remove_staging_path(&path).unwrap();
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
