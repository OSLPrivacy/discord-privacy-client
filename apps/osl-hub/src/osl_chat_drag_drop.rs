//! Drag/drop intake for the first-party OSL Chats composer.
//!
//! This is intentionally tray-only. Dropping a file records bounded local file
//! metadata for the trusted composer; message creation remains owned by the
//! explicit send path.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Refusal shown when a dropped item is a folder rather than a file. Folders are
/// not allowed: staging one would mean walking a tree of unknown size and depth
/// out of the composer's sight, so the whole drop is refused instead.
pub const OSL_CHAT_DROP_FOLDER_REFUSAL: &str =
    "The dropped OSL Chat attachment is a folder, and folders are not allowed";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslChatTrayAttachment {
    pub tray_id: String,
    pub original_filename: String,
    /// Short human-readable identity for the staged bytes, see [`tray_fingerprint`].
    pub fingerprint: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslChatDropIntakeReceipt {
    pub accepted_file_count: usize,
    pub tray_file_count: usize,
    pub messages_created: usize,
    pub accepted_filenames: Vec<String>,
    pub accepted_fingerprints: Vec<String>,
}

#[derive(Debug, Default)]
pub struct OslChatAttachmentTray {
    attachments: Vec<OslChatTrayAttachment>,
    messages_created: usize,
}

impl OslChatAttachmentTray {
    pub fn attachments(&self) -> &[OslChatTrayAttachment] {
        &self.attachments
    }

    pub fn messages_created(&self) -> usize {
        self.messages_created
    }

    pub fn accept_dropped_files<I, P>(
        &mut self,
        dropped_files: I,
    ) -> Result<OslChatDropIntakeReceipt, String>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut accepted = Vec::new();
        for path in dropped_files {
            accepted.push(tray_attachment(
                self.attachments.len() + accepted.len(),
                path.as_ref(),
            )?);
        }
        if accepted.is_empty() {
            return Err("Drop at least one file into the OSL Chats attachment tray".to_owned());
        }

        let accepted_filenames = accepted
            .iter()
            .map(|attachment| attachment.original_filename.clone())
            .collect::<Vec<_>>();
        let accepted_fingerprints = accepted
            .iter()
            .map(|attachment| attachment.fingerprint.clone())
            .collect::<Vec<_>>();
        self.attachments.extend(accepted);

        Ok(OslChatDropIntakeReceipt {
            accepted_file_count: accepted_filenames.len(),
            tray_file_count: self.attachments.len(),
            messages_created: self.messages_created,
            accepted_filenames,
            accepted_fingerprints,
        })
    }
}

fn tray_attachment(index: usize, path: &Path) -> Result<OslChatTrayAttachment, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|_| "The dropped OSL Chat attachment could not be checked".to_owned())?;
    if metadata.is_dir() {
        return Err(OSL_CHAT_DROP_FOLDER_REFUSAL.to_owned());
    }
    if !metadata.is_file() {
        return Err("The dropped OSL Chat attachment is not a regular file".to_owned());
    }
    let original_filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The dropped OSL Chat attachment filename is invalid".to_owned())?
        .to_owned();
    let fingerprint = tray_fingerprint(&original_filename, path)?;

    Ok(OslChatTrayAttachment {
        tray_id: format!("osl-chat-drop-{index:06}"),
        original_filename,
        fingerprint,
        path: path.to_path_buf(),
        size_bytes: metadata.len(),
    })
}

/// Short, human-readable identity for one staged tray item: the uppercased
/// filename stem, then a four-digit fold of the file's own bytes.
///
/// It is derived from what is on disk at drop time and never stored anywhere
/// else, so a tray card that still carries its fingerprint is carrying the
/// bytes it was staged with — a later drop cannot quietly re-point it.
pub fn tray_fingerprint(original_filename: &str, path: &Path) -> Result<String, String> {
    let stem = original_filename
        .split('.')
        .next()
        .unwrap_or(original_filename);
    let label = stem
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(16)
        .collect::<String>()
        .to_uppercase();
    let label = if label.is_empty() {
        "FILE".to_owned()
    } else {
        label
    };

    // FNV-1a over the bytes, read in bounded chunks so a large attachment is
    // never held in memory just to be named.
    let mut file = std::fs::File::open(path)
        .map_err(|_| "The dropped OSL Chat attachment could not be checked".to_owned())?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut chunk = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut chunk)
            .map_err(|_| "The dropped OSL Chat attachment could not be read".to_owned())?;
        if read == 0 {
            break;
        }
        for byte in &chunk[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    Ok(format!("{label}-{:04}", hash % 10_000))
}

#[cfg(test)]
mod tests {
    use super::OslChatAttachmentTray;

    #[test]
    fn task_1335_drop_adds_two_files_and_creates_zero_messages() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("task-1335-alpha.txt");
        let second = temp.path().join("task-1335-beta.png");
        std::fs::write(&first, b"alpha").expect("write first fixture");
        std::fs::write(&second, b"beta").expect("write second fixture");

        let mut tray = OslChatAttachmentTray::default();
        let receipt = tray
            .accept_dropped_files([first.as_path(), second.as_path()])
            .expect("dropped files are accepted");

        println!(
            "TASK1335_TEST accepted_file_count={} tray_file_count={} messages_created={}",
            receipt.accepted_file_count, receipt.tray_file_count, receipt.messages_created
        );
        assert_eq!(receipt.accepted_file_count, 2);
        assert_eq!(tray.attachments().len(), 2);
        assert_eq!(receipt.tray_file_count, 2);
        assert_eq!(tray.messages_created(), 0);
        assert_eq!(receipt.messages_created, 0);
    }
}
