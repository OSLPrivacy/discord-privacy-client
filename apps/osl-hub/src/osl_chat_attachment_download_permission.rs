//! Recipient-bound permission for downloading a completed Pro attachment.
//!
//! Upload admission belongs to the sender. Once a Pro send has produced a
//! completed-file receipt, the recipient receives a bearer fetch capability
//! bound locally to their OSL user id. Download authorization deliberately does
//! not call the attachment upload-size gate: a Free recipient may receive the
//! full object that the Pro sender was allowed to upload.

use std::fmt;
use std::io::Write;

use ipc::cipher_store_client::{
    CipherStoreClient, CipherStoreError, ProChunkedUploadReport, FETCH_TOKEN_BYTES,
};

use crate::attachment_limits::AttachmentAccountTier;

pub const SHARE_REVOKED_REFUSAL_NAME: &str = "share_revoked";

/// The receiver's authority recorded beside the stored file. Keeping this in
/// the bound record makes a post-upload permission edit fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredReceiverPermission {
    Download,
    None,
}

/// Security-relevant metadata persisted for a completed attachment.
///
/// The download permission snapshots this entire value when it is minted. A
/// later database edit therefore cannot relabel a stored object, move it to a
/// different owner, change its claimed length or kind, or replace the
/// receiver's permission without invalidating the download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRecipientAttachment {
    pub file_id: String,
    pub file_name: String,
    pub byte_length: u64,
    pub kind: String,
    pub owner_osl_user_id: String,
    pub receiver_permission: StoredReceiverPermission,
}

/// The direct request made by the receiving OSL copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientAttachmentDownloadRequest {
    pub recipient_osl_user_id: String,
    pub account_tier: AttachmentAccountTier,
}

/// Proof that the receiver permission admitted one exact transport object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientAttachmentDownloadReceipt {
    pub file_id: String,
    pub byte_length: u64,
    pub recipient_account_tier: AttachmentAccountTier,
}

/// Opaque receiver authority created only from a completed Pro-send receipt.
///
/// The fetch token is intentionally private and this type has no `Debug` or
/// serialization implementation, so the bearer secret cannot accidentally be
/// placed in renderer state or logs.
pub struct RecipientAttachmentDownloadPermission {
    recipient_osl_user_id: String,
    file_id: String,
    expected_byte_length: u64,
    expected_stored_attachment: StoredRecipientAttachment,
    fetch_token: [u8; FETCH_TOKEN_BYTES],
    revoked: bool,
}

impl RecipientAttachmentDownloadPermission {
    pub fn recipient_osl_user_id(&self) -> &str {
        &self.recipient_osl_user_id
    }

    pub fn file_id(&self) -> &str {
        &self.file_id
    }

    pub fn expected_byte_length(&self) -> u64 {
        self.expected_byte_length
    }

    /// Revoke this recipient share. Returns `true` only for the state change.
    pub fn revoke(&mut self) -> bool {
        if self.revoked {
            return false;
        }
        self.revoked = true;
        true
    }
}

#[derive(Debug)]
pub enum RecipientAttachmentDownloadError {
    ShareRevoked,
    RecipientMismatch,
    InvalidProSendReceipt,
    FileNameChanged,
    FileSizeChanged,
    FileKindChanged,
    FileOwnerChanged,
    ReceiverPermissionChanged,
    DownloadLengthMismatch { expected: u64, actual: u64 },
    Transfer(CipherStoreError),
}

impl RecipientAttachmentDownloadError {
    /// Stable refusal name for UI and audit assertions.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::ShareRevoked => SHARE_REVOKED_REFUSAL_NAME,
            Self::RecipientMismatch => "recipient_mismatch",
            Self::InvalidProSendReceipt => "invalid_pro_send_receipt",
            Self::FileNameChanged => "file_name_changed",
            Self::FileSizeChanged => "file_size_changed",
            Self::FileKindChanged => "file_kind_changed",
            Self::FileOwnerChanged => "file_owner_changed",
            Self::ReceiverPermissionChanged => "receiver_permission_changed",
            Self::DownloadLengthMismatch { .. } => "download_length_mismatch",
            Self::Transfer(_) => "attachment_transfer_failed",
        }
    }
}

impl fmt::Display for RecipientAttachmentDownloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShareRevoked => write!(formatter, "{SHARE_REVOKED_REFUSAL_NAME}"),
            Self::RecipientMismatch => write!(formatter, "recipient_mismatch"),
            Self::InvalidProSendReceipt => write!(formatter, "invalid_pro_send_receipt"),
            Self::FileNameChanged => write!(formatter, "file_name_changed"),
            Self::FileSizeChanged => write!(formatter, "file_size_changed"),
            Self::FileKindChanged => write!(formatter, "file_kind_changed"),
            Self::FileOwnerChanged => write!(formatter, "file_owner_changed"),
            Self::ReceiverPermissionChanged => write!(formatter, "receiver_permission_changed"),
            Self::DownloadLengthMismatch { expected, actual } => write!(
                formatter,
                "download_length_mismatch: expected {expected} bytes, received {actual}"
            ),
            Self::Transfer(error) => write!(formatter, "attachment_transfer_failed: {error}"),
        }
    }
}

impl std::error::Error for RecipientAttachmentDownloadError {}

impl From<CipherStoreError> for RecipientAttachmentDownloadError {
    fn from(error: CipherStoreError) -> Self {
        Self::Transfer(error)
    }
}

/// Mint one recipient permission from the authoritative completion receipt of
/// the Pro uploader introduced at task 0642.
pub fn grant_recipient_download_from_pro_send(
    report: &ProChunkedUploadReport,
    stored_attachment: &StoredRecipientAttachment,
    recipient_osl_user_id: impl Into<String>,
    fetch_token: [u8; FETCH_TOKEN_BYTES],
) -> Result<RecipientAttachmentDownloadPermission, RecipientAttachmentDownloadError> {
    let recipient_osl_user_id = recipient_osl_user_id.into();
    let completed = &report.completed_file;
    let receipts_are_ordered = report
        .finished_pieces
        .iter()
        .enumerate()
        .all(|(index, piece)| {
            piece.upload_id == completed.file_id
                && piece.piece_number == u32::try_from(index + 1).unwrap_or(u32::MAX)
        });
    let recorded_bytes = report
        .finished_pieces
        .iter()
        .try_fold(0_u64, |total, piece| total.checked_add(piece.size_bytes));
    if recipient_osl_user_id.is_empty()
        || completed.file_id.len() != 32
        || !completed
            .file_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || completed.total_size_bytes == 0
        || stored_attachment.file_id != completed.file_id
        || stored_attachment.byte_length != completed.total_size_bytes
        || stored_attachment.file_name.is_empty()
        || stored_attachment.kind.is_empty()
        || stored_attachment.owner_osl_user_id.is_empty()
        || stored_attachment.receiver_permission != StoredReceiverPermission::Download
        || completed.piece_count as usize != report.finished_pieces.len()
        || !receipts_are_ordered
        || recorded_bytes != Some(completed.total_size_bytes)
    {
        return Err(RecipientAttachmentDownloadError::InvalidProSendReceipt);
    }

    Ok(RecipientAttachmentDownloadPermission {
        recipient_osl_user_id,
        file_id: completed.file_id.clone(),
        expected_byte_length: completed.total_size_bytes,
        expected_stored_attachment: stored_attachment.clone(),
        fetch_token,
        revoked: false,
    })
}

/// Authorize and execute a direct receiver download.
///
/// `request.account_tier` is retained in the receipt as evidence of who made
/// the request. It is intentionally not passed through `check_attachment_size`:
/// upload limits constrain senders, not recipients.
pub fn download_pro_attachment_for_recipient(
    permission: &RecipientAttachmentDownloadPermission,
    stored_attachment: &StoredRecipientAttachment,
    request: &RecipientAttachmentDownloadRequest,
    client: &CipherStoreClient,
    output: &mut impl Write,
) -> Result<RecipientAttachmentDownloadReceipt, RecipientAttachmentDownloadError> {
    if permission.revoked {
        return Err(RecipientAttachmentDownloadError::ShareRevoked);
    }
    if request.recipient_osl_user_id != permission.recipient_osl_user_id {
        return Err(RecipientAttachmentDownloadError::RecipientMismatch);
    }
    verify_stored_attachment(permission, stored_attachment)?;

    let byte_length =
        client.fetch_attachment_to_writer(&permission.file_id, &permission.fetch_token, output)?;
    if byte_length != permission.expected_byte_length {
        return Err(RecipientAttachmentDownloadError::DownloadLengthMismatch {
            expected: permission.expected_byte_length,
            actual: byte_length,
        });
    }

    Ok(RecipientAttachmentDownloadReceipt {
        file_id: permission.file_id.clone(),
        byte_length,
        recipient_account_tier: request.account_tier,
    })
}

fn verify_stored_attachment(
    permission: &RecipientAttachmentDownloadPermission,
    candidate: &StoredRecipientAttachment,
) -> Result<(), RecipientAttachmentDownloadError> {
    let expected = &permission.expected_stored_attachment;

    // The transport object id is deliberately not reported as a sixth mutable
    // detail: it is already the lookup key and remains private in the
    // permission. Treat replacing it as an invalid receipt-shaped record.
    if candidate.file_id != expected.file_id {
        return Err(RecipientAttachmentDownloadError::InvalidProSendReceipt);
    }
    if candidate.file_name != expected.file_name {
        return Err(RecipientAttachmentDownloadError::FileNameChanged);
    }
    if candidate.byte_length != expected.byte_length {
        return Err(RecipientAttachmentDownloadError::FileSizeChanged);
    }
    if candidate.kind != expected.kind {
        return Err(RecipientAttachmentDownloadError::FileKindChanged);
    }
    if candidate.owner_osl_user_id != expected.owner_osl_user_id {
        return Err(RecipientAttachmentDownloadError::FileOwnerChanged);
    }
    if candidate.receiver_permission != expected.receiver_permission {
        return Err(RecipientAttachmentDownloadError::ReceiverPermissionChanged);
    }
    Ok(())
}
