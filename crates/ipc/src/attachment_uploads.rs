//! Message-keyed cancellation for attachment uploads that have crossed the
//! native send boundary but have not produced a completed-file receipt yet.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A cancellation signal held by the multipart uploader after its registry
/// entry has been removed by a burn.
#[derive(Clone, Default)]
pub struct AttachmentUploadCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AttachmentUploadCancellation {
    pub fn cancel(&self) -> bool {
        !self.cancelled.swap(true, Ordering::SeqCst)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn same_signal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.cancelled, &other.cancelled)
    }
}

/// In-memory active uploads, keyed by the exact stored message id a burn names.
///
/// Removing an entry cancels only that upload. The uploader keeps its cloned
/// signal long enough to observe the cancellation and delete its remote
/// multipart session before it can ask the store to complete the file.
#[derive(Default)]
pub struct ActiveAttachmentUploads {
    uploads: Mutex<HashMap<String, AttachmentUploadCancellation>>,
}

impl ActiveAttachmentUploads {
    pub fn begin(&self, message_id: &str) -> Result<AttachmentUploadCancellation, String> {
        if message_id.is_empty() || message_id.len() > 512 {
            return Err("OSL attachment upload message id is invalid".to_owned());
        }
        let mut uploads = self
            .uploads
            .lock()
            .map_err(|_| "OSL active attachment uploads are unavailable".to_owned())?;
        if uploads.contains_key(message_id) {
            return Err("OSL attachment upload is already active for this message".to_owned());
        }
        let cancellation = AttachmentUploadCancellation::default();
        uploads.insert(message_id.to_owned(), cancellation.clone());
        Ok(cancellation)
    }

    /// Cancel and remove one exact upload. Returns `true` only for the first
    /// state transition, never for an absent or already-cancelled upload.
    pub fn cancel_exact(&self, message_id: &str) -> Result<bool, String> {
        let cancellation = self
            .uploads
            .lock()
            .map_err(|_| "OSL active attachment uploads are unavailable".to_owned())?
            .remove(message_id);
        Ok(cancellation.is_some_and(|cancellation| cancellation.cancel()))
    }

    /// Remove the same registration after upload termination without allowing
    /// a stale finishing task to remove a newer upload under the same id.
    pub fn finish(
        &self,
        message_id: &str,
        cancellation: &AttachmentUploadCancellation,
    ) -> Result<bool, String> {
        let mut uploads = self
            .uploads
            .lock()
            .map_err(|_| "OSL active attachment uploads are unavailable".to_owned())?;
        let matches = uploads
            .get(message_id)
            .is_some_and(|active| active.same_signal(cancellation));
        if matches {
            uploads.remove(message_id);
        }
        Ok(matches)
    }

    pub fn len(&self) -> Result<usize, String> {
        self.uploads
            .lock()
            .map(|uploads| uploads.len())
            .map_err(|_| "OSL active attachment uploads are unavailable".to_owned())
    }

    pub fn is_empty(&self) -> Result<bool, String> {
        self.len().map(|count| count == 0)
    }
}
