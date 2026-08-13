//! TASK 6804 — the native side of "Upload recovery kit".
//!
//! The renderer may not name a file, may not read one, and may not learn what
//! is on this disk. It asks for a picker; the operating system's own dialog
//! answers; and what comes back to JavaScript is one already-decided selection:
//! a path Rust resolved, a length Rust measured and a digest Rust took, all of
//! the same file, all at the same moment. That triple is the freeze.
//!
//! The freeze exists because the gap between "the dialog closed" and "the bytes
//! were validated" is writable by anything else on the machine. Without it, a
//! kit could be shown to the picker and a different file imported. With it, the
//! renderer re-digests the bytes it is about to parse and refuses unless they
//! hash to what was frozen, so the file that was chosen is provably the file
//! that is read.
//!
//! Nothing here parses a recovery kit. The bytes are handed over intact and the
//! validation happens once, in `recovery-kit-file-6804.ts`. A second parser in
//! Rust would be a second place for the format to drift and a second place to
//! leak a phrase into a log line.

use std::path::Path;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use osl_privacy_hub::core_bridge::HubCoreState;
use sha2::{Digest, Sha256};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// The extension the dialog filters on. Mirrors `RECOVERY_KIT_FILE_EXTENSION`.
const RECOVERY_KIT_EXTENSION: &str = "oslkit";
/// Mirrors `RECOVERY_KIT_MAX_FILE_BYTES`. A kit is roughly 400 bytes; anything
/// larger is refused by length before a byte of it is read into memory.
const RECOVERY_KIT_MAX_FILE_BYTES: u64 = 8192;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PickedRecoveryKitFileDto {
    path: String,
    sha256: String,
    size_bytes: u64,
    bytes_base64: String,
    /// The account this device already holds, or `None` when it holds none —
    /// which is the ordinary state of the Restore Account screen.
    local_user_id: Option<String>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    let digest = hash.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest.iter() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

/// The account this device holds, if it has loaded one.
///
/// Deliberately not gated on the main password being unlocked: the Forgot
/// Password screen is reached *because* the account is locked, and the value
/// here is the account's own public routing label, not anything the lock
/// protects. Returning `None` while locked would have made the identity
/// comparison silent on the one screen that most needs it.
fn local_osl_user_id(core: &HubCoreState) -> Option<String> {
    core.osl
        .identity
        .lock()
        .ok()
        .and_then(|identity| identity.as_ref().map(|value| value.user_id.clone()))
}

fn read_frozen_selection(path: &Path) -> Result<(u64, Vec<u8>), String> {
    let metadata =
        std::fs::metadata(path).map_err(|_| "The chosen file could not be read".to_owned())?;
    if !metadata.is_file() {
        return Err("The chosen file is not a regular file".to_owned());
    }
    if metadata.len() == 0 {
        return Err("The chosen file is empty".to_owned());
    }
    if metadata.len() > RECOVERY_KIT_MAX_FILE_BYTES {
        return Err("The chosen file is too large to be an OSL recovery kit".to_owned());
    }
    let bytes = std::fs::read(path).map_err(|_| "The chosen file could not be read".to_owned())?;
    // Length is re-read from the bytes actually obtained rather than trusted
    // from the metadata call: a file that grew between `metadata` and `read`
    // must not be described by the smaller number the renderer would then
    // compare against.
    let length = u64::try_from(bytes.len())
        .map_err(|_| "The chosen file could not be measured".to_owned())?;
    if length > RECOVERY_KIT_MAX_FILE_BYTES {
        return Err("The chosen file is too large to be an OSL recovery kit".to_owned());
    }
    Ok((length, bytes))
}

/// Open the installed picker and freeze whatever is chosen.
///
/// `Ok(None)` is cancellation, and cancellation is not an error: it must reach
/// the renderer as "nothing happened", never as a message that a person could
/// read as a failure of their recovery kit.
pub(crate) fn pick_recovery_kit_file(
    app: &tauri::AppHandle,
    core: &HubCoreState,
) -> Result<Option<PickedRecoveryKitFileDto>, String> {
    let parent = app
        .get_webview_window("main")
        .ok_or_else(|| "The trusted recovery-kit picker is unavailable".to_owned())?;
    let selected = app
        .dialog()
        .file()
        .set_parent(&parent)
        .set_title("Choose your OSL recovery kit")
        .add_filter("OSL recovery kit", &[RECOVERY_KIT_EXTENSION])
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "The chosen file path is unavailable".to_owned())?;
    let (size_bytes, bytes) = read_frozen_selection(&path)?;
    let sha256 = sha256_hex(&bytes);
    Ok(Some(PickedRecoveryKitFileDto {
        path: path.to_string_lossy().into_owned(),
        sha256,
        size_bytes,
        bytes_base64: STANDARD.encode(&bytes),
        local_user_id: local_osl_user_id(core),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_sha256_of_the_exact_bytes() {
        // The published SHA-256 of the empty string and of "abc". If the freeze
        // ever stops being SHA-256 the renderer's re-digest would disagree with
        // every real selection, so this is pinned rather than round-tripped.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn an_oversized_file_is_refused_before_its_bytes_are_returned() {
        let dir = std::env::temp_dir().join(format!(
            "osl-6804-oversize-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("big.oslkit");
        std::fs::write(&path, vec![b'x'; (RECOVERY_KIT_MAX_FILE_BYTES + 1) as usize])
            .expect("write oversize");
        let refusal = read_frozen_selection(&path).expect_err("oversize must be refused");
        assert_eq!(
            refusal,
            "The chosen file is too large to be an OSL recovery kit"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_file_is_refused() {
        let dir = std::env::temp_dir().join(format!("osl-6804-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("empty.oslkit");
        std::fs::write(&path, b"").expect("write empty");
        assert_eq!(
            read_frozen_selection(&path).expect_err("empty must be refused"),
            "The chosen file is empty"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
