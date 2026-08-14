//! Safe, stable names for attachment save-to-folder operations.
//!
//! Attachment names originate on another machine and cannot be joined to a
//! receiver-selected directory as paths. This boundary turns every untrusted
//! name into one ASCII filesystem component only at the save boundary. The
//! authenticated logical name remains byte-for-byte identical in sender and
//! receiver UI, while the filesystem leaf is explicit and unambiguous.

use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Leaves room below the 255-byte component limit used by Windows and common
/// Unix filesystems. The transport itself permits a larger metadata field, but
/// a display name that cannot be saved is not useful.
pub const MAX_SAFE_ATTACHMENT_FILENAME_BYTES: usize = 200;
const DIGEST_HEX_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedReceivedAttachment {
    pub display_name: String,
    pub path: PathBuf,
    pub byte_length: u64,
}

/// Return the exact single-component name the receiver may safely reserve below
/// a chosen folder. This is deliberately distinct from the authenticated
/// logical name shown by both participants.
///
/// The readable prefix contains ASCII bytes only. Bytes with path, bidi,
/// normalization, control, or Windows-device-name significance are rendered as
/// `_XX`; a digest of the complete original name prevents truncated or
/// lookalike inputs from collapsing onto the same visible component.
pub fn safe_attachment_output_name(untrusted_name: &str) -> Result<String, String> {
    if untrusted_name.is_empty() || untrusted_name.contains('\0') {
        return Err("attachment filename is empty or contains NUL".to_owned());
    }

    let (stem, extension) = split_supported_extension(untrusted_name)?;
    let digest = hex_lower(&Sha256::digest(untrusted_name.as_bytes()));
    let suffix = format!("--{}.{}", &digest[..DIGEST_HEX_BYTES], extension);
    let prefix_budget = MAX_SAFE_ATTACHMENT_FILENAME_BYTES
        .checked_sub("osl-".len() + suffix.len())
        .ok_or_else(|| "attachment filename extension is too long".to_owned())?;

    let mut escaped = String::new();
    for byte in stem.as_bytes() {
        let piece = if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            char::from(*byte).to_string()
        } else {
            format!("_{byte:02X}")
        };
        if escaped.len() + piece.len() > prefix_budget {
            break;
        }
        escaped.push_str(&piece);
    }
    if escaped.is_empty() {
        escaped.push_str("file");
    }

    let safe = format!("osl-{escaped}{suffix}");
    if !is_canonical_safe_name(&safe) {
        return Err("attachment filename could not be made filesystem-safe".to_owned());
    }
    Ok(safe)
}

/// Atomically reserve and write one received attachment inside an existing,
/// real directory. Existing files are never opened for writing.
pub fn save_received_attachment_bytes(
    chosen_folder: &Path,
    sender_display_name: &str,
    bytes: &[u8],
) -> Result<SavedReceivedAttachment, String> {
    let metadata = std::fs::symlink_metadata(chosen_folder)
        .map_err(|error| format!("chosen attachment folder is unavailable: {error}"))?;
    if !metadata.file_type().is_dir() {
        return Err("chosen attachment folder is not a real directory".to_owned());
    }

    let output_name = safe_attachment_output_name(sender_display_name)?;
    let path = chosen_folder.join(&output_name);
    if path.parent() != Some(chosen_folder) {
        return Err("attachment output escaped the chosen folder".to_owned());
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("attachment output could not be reserved: {error}"))?;
    let mut cleanup = PartialOutput::new(path.clone());
    let write_result = (|| {
        output
            .write_all(bytes)
            .map_err(|error| format!("attachment output could not be written: {error}"))?;
        output
            .sync_all()
            .map_err(|error| format!("attachment output could not be synchronized: {error}"))
    })();
    drop(output);
    write_result?;
    cleanup.keep();

    Ok(SavedReceivedAttachment {
        display_name: sender_display_name.to_owned(),
        path,
        byte_length: bytes.len() as u64,
    })
}

fn split_supported_extension(name: &str) -> Result<(&str, String), String> {
    let (stem, extension) = name
        .rsplit_once('.')
        .ok_or_else(|| "attachment filename has no supported extension".to_owned())?;
    if stem.is_empty()
        || extension.is_empty()
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        || extension.len() > 10
    {
        return Err("attachment filename has no supported extension".to_owned());
    }
    let extension = extension.to_ascii_lowercase();
    if crate::attachment_formats::accepted_attachment_mime(&format!("attachment.{extension}"))
        .is_none()
    {
        return Err("attachment filename has no supported extension".to_owned());
    }
    Ok((stem, extension))
}

fn is_canonical_safe_name(name: &str) -> bool {
    if name.len() > MAX_SAFE_ATTACHMENT_FILENAME_BYTES
        || !name.is_ascii()
        || !name.starts_with("osl-")
        || name.contains(['/', '\\', ':'])
        || name.ends_with([' ', '.'])
    {
        return false;
    }
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    if crate::attachment_formats::accepted_attachment_mime(&format!("attachment.{extension}"))
        .is_none()
    {
        return false;
    }
    let Some((prefix, digest)) = stem.rsplit_once("--") else {
        return false;
    };
    !prefix.is_empty()
        && digest.len() == DIGEST_HEX_BYTES
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[(byte >> 4) as usize]));
        output.push(char::from(HEX[(byte & 0x0f) as usize]));
    }
    output
}

struct PartialOutput {
    path: PathBuf,
    keep: bool,
}

impl PartialOutput {
    fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }

    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for PartialOutput {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
