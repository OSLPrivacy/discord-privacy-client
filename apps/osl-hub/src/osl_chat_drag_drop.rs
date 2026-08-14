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

/// A single outbound OSL Chat message may carry at most this many files.
/// Keep this at the tray boundary so every intake route shares the rule.
pub const MAX_OSL_CHAT_TRAY_FILES: usize = 16;

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
        self.accept_files(
            dropped_files,
            "Drop at least one file into the OSL Chats attachment tray",
        )
    }

    /// Add files returned by the trusted file picker.
    pub fn accept_picked_files<I, P>(
        &mut self,
        picked_files: I,
    ) -> Result<OslChatDropIntakeReceipt, String>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.accept_files(
            picked_files,
            "Choose at least one file for the OSL Chats attachment tray",
        )
    }

    /// Add the file staged from a clipboard image. Clipboard images become a
    /// local temporary file before this boundary; they must not bypass its
    /// per-message count check.
    pub fn accept_clipboard_image_file(
        &mut self,
        clipboard_image_file: impl AsRef<Path>,
    ) -> Result<OslChatDropIntakeReceipt, String> {
        self.accept_files(
            [clipboard_image_file],
            "Paste an image into the OSL Chats attachment tray",
        )
    }

    fn accept_files<I, P>(
        &mut self,
        files: I,
        empty_message: &str,
    ) -> Result<OslChatDropIntakeReceipt, String>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let files = files.into_iter().collect::<Vec<_>>();
        if files.is_empty() {
            return Err(empty_message.to_owned());
        }
        let requested_count = files.len();
        let resulting_count = self.attachments.len().saturating_add(requested_count);
        if resulting_count > MAX_OSL_CHAT_TRAY_FILES {
            return Err(tray_file_limit_rejection());
        }

        let mut accepted = Vec::with_capacity(requested_count);
        for path in files {
            accepted.push(tray_attachment(
                self.attachments.len() + accepted.len(),
                path.as_ref(),
            )?);
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

fn tray_file_limit_rejection() -> String {
    format!("An OSL Chat attachment tray can contain at most {MAX_OSL_CHAT_TRAY_FILES} files")
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
    use super::{OslChatAttachmentTray, MAX_OSL_CHAT_TRAY_FILES};

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

    #[test]
    fn task_0618_picker_drop_and_clipboard_refuse_the_same_seventeenth_file() {
        const REQUIRED_LIMIT: usize = 16;
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = (1..=REQUIRED_LIMIT + 1)
            .map(|number| {
                let path = temp.path().join(format!("task-0618-{number}.png"));
                std::fs::write(&path, b"fixture").expect("write fixture");
                path
            })
            .collect::<Vec<_>>();

        let mut picker_tray = OslChatAttachmentTray::default();
        let mut drop_tray = OslChatAttachmentTray::default();
        let mut clipboard_tray = OslChatAttachmentTray::default();
        for path in paths.iter().take(REQUIRED_LIMIT) {
            picker_tray
                .accept_picked_files([path.as_path()])
                .expect("picker accepts the first sixteen files");
            drop_tray
                .accept_dropped_files([path.as_path()])
                .expect("drop accepts the first sixteen files");
            clipboard_tray
                .accept_clipboard_image_file(path)
                .expect("clipboard accepts the first sixteen files");
        }

        let picker_error = picker_tray
            .accept_picked_files([paths[REQUIRED_LIMIT].as_path()])
            .expect_err("picker must refuse the seventeenth file");
        let drop_error = drop_tray
            .accept_dropped_files([paths[REQUIRED_LIMIT].as_path()])
            .expect_err("drop must refuse the seventeenth file");
        let clipboard_error = clipboard_tray
            .accept_clipboard_image_file(&paths[REQUIRED_LIMIT])
            .expect_err("clipboard must refuse the seventeenth file");
        let expected = "An OSL Chat attachment tray can contain at most 16 files";

        println!(
            "TASK0618 picker_adds=16 drop_adds=16 clipboard_image_adds=16 picker_add17={picker_error:?} drop_add17={drop_error:?} clipboard_add17={clipboard_error:?}"
        );
        assert_eq!(MAX_OSL_CHAT_TRAY_FILES, REQUIRED_LIMIT);
        assert_eq!(picker_error, expected);
        assert_eq!(drop_error, expected);
        assert_eq!(clipboard_error, expected);
        assert_eq!(picker_tray.attachments().len(), REQUIRED_LIMIT);
        assert_eq!(drop_tray.attachments().len(), REQUIRED_LIMIT);
        assert_eq!(clipboard_tray.attachments().len(), REQUIRED_LIMIT);
    }
}
