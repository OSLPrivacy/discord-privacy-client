//! Native OSL Chat boundary for Pro multipart attachment sends.
//!
//! The renderer may name a tray item, but it never chooses the file handed to
//! the uploader.  This boundary re-admits the live tray and checks that the
//! requested item still names the same path and size before it opens a
//! separately sealed file for the cipher-store client.

use std::fs::{self, File};
use std::path::PathBuf;

use ipc::cipher_store_client::{CipherStoreClient, ProChunkedUploadReport, FETCH_TOKEN_BYTES};

use crate::attachment_limits::{
    check_attachment_count, check_attachment_size, AttachmentAccountTier,
};
use crate::osl_chat_drag_drop::OslChatAttachmentTray;

/// Data selected at the native OSL Chat send boundary.
///
/// `sealed_path` is deliberately distinct from the tray path: tray files are
/// plaintext intake files and must never be passed to the network uploader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OslChatProAttachmentSendRequest {
    pub tray_id: String,
    pub tray_path: PathBuf,
    pub tray_size_bytes: u64,
    pub sealed_path: PathBuf,
    pub ttl_seconds: u32,
    pub fetch_token: [u8; FETCH_TOKEN_BYTES],
}

/// Re-admit a Pro tray selection and upload only its already-sealed file.
///
/// Admission and live-tray validation intentionally finish before `File::open`
/// is called for `sealed_path` and before the uploader is invoked.
pub fn send_pro_attachment_from_osl_chat_tray(
    tray: &OslChatAttachmentTray,
    request: &OslChatProAttachmentSendRequest,
    client: &CipherStoreClient,
) -> Result<ProChunkedUploadReport, String> {
    let sealed = open_admitted_sealed_file(tray, request)?;
    client
        .upload_attachment_file_pro_chunked(sealed, request.ttl_seconds, &request.fetch_token)
        .map_err(|error| format!("OSL Chat Pro attachment upload failed: {error}"))
}

/// Register the upload under its exact message id before the first multipart
/// request. Burning that message removes the registration and flips the signal
/// checked around every accepted part and before completion.
pub fn send_registered_pro_attachment_from_osl_chat_tray(
    state: &ipc::state::AppState,
    message_id: &str,
    tray: &OslChatAttachmentTray,
    request: &OslChatProAttachmentSendRequest,
    client: &CipherStoreClient,
) -> Result<ProChunkedUploadReport, String> {
    let sealed = open_admitted_sealed_file(tray, request)?;
    let cancellation = state.active_attachment_uploads.begin(message_id)?;
    let upload = client.upload_attachment_file_pro_chunked_cancellable(
        sealed,
        request.ttl_seconds,
        &request.fetch_token,
        &cancellation,
    );
    let finished_registration = state
        .active_attachment_uploads
        .finish(message_id, &cancellation);

    match (upload, finished_registration) {
        (Ok(report), Err(error)) => {
            let _ = client.delete_attachment(&report.completed_file.file_id, &request.fetch_token);
            Err(error)
        }
        (Err(_), Err(error)) => Err(error),
        (Ok(report), Ok(removed_by_finisher))
            if cancellation.is_cancelled() || !removed_by_finisher =>
        {
            let _ = client.delete_attachment(&report.completed_file.file_id, &request.fetch_token);
            Err("OSL Chat Pro attachment upload cancelled".to_owned())
        }
        (Ok(report), Ok(_)) => Ok(report),
        (
            Err(ipc::cipher_store_client::CipherStoreError::AttachmentUploadCancelled),
            Ok(_),
        ) => Err("OSL Chat Pro attachment upload cancelled".to_owned()),
        (Err(error), Ok(_)) => Err(format!("OSL Chat Pro attachment upload failed: {error}")),
    }
}

fn open_admitted_sealed_file(
    tray: &OslChatAttachmentTray,
    request: &OslChatProAttachmentSendRequest,
) -> Result<File, String> {
    check_attachment_count(tray.attachments().len())
        .map_err(|_| "OSL Chat attachment count is no longer allowed for Pro".to_owned())?;
    for attachment in tray.attachments() {
        check_attachment_size(attachment.size_bytes, AttachmentAccountTier::Pro)
            .map_err(|_| "OSL Chat attachment size is no longer allowed for Pro".to_owned())?;
    }

    let attachment = tray
        .attachments()
        .iter()
        .find(|attachment| attachment.tray_id == request.tray_id)
        .ok_or_else(|| "OSL Chat attachment tray record no longer exists".to_owned())?;
    if attachment.path != request.tray_path || attachment.size_bytes != request.tray_size_bytes {
        return Err(
            "OSL Chat attachment tray record no longer matches the send request".to_owned(),
        );
    }
    let metadata = fs::metadata(&attachment.path)
        .map_err(|_| "OSL Chat attachment tray file could not be checked".to_owned())?;
    if !metadata.is_file() || metadata.len() != attachment.size_bytes {
        return Err("OSL Chat attachment tray file no longer matches its record".to_owned());
    }
    if same_file_path(&attachment.path, &request.sealed_path) {
        return Err("OSL Chat attachment must be sealed before upload".to_owned());
    }

    File::open(&request.sealed_path)
        .map_err(|_| "OSL Chat sealed attachment could not be opened".to_owned())
}

fn same_file_path(first: &std::path::Path, second: &std::path::Path) -> bool {
    first == second
        || match (fs::canonicalize(first), fs::canonicalize(second)) {
            (Ok(first), Ok(second)) => first == second,
            _ => false,
        }
}
