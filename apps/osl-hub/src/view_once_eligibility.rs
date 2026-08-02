//! Compose-time policy for view-once attachments.
//!
//! A view-once payload must stay in OSL's protected viewer for its entire
//! lifetime. Files that need an external viewer cannot be timed, capture
//! protected, or reliably removed, so refusing them is safer than converting
//! the request into an ordinary attachment.

/// Explains why an attachment cannot be sent as view-once.
///
/// The message deliberately identifies the requested MIME type, rather than a
/// filename or local path, so the composer can present a useful reason without
/// exposing local filesystem information.
pub fn view_once_attachment_refusal(mime_type: &str) -> String {
    format!(
        "View-once is unavailable for {mime_type} because OSL cannot display it inside its protected viewer"
    )
}

/// Refuse content that cannot be rendered by OSL's protected in-process
/// attachment viewer.
///
/// This is intentionally an allowlist. A newly supported wire MIME type does
/// not become view-once eligible until the protected viewer explicitly gains
/// support for it.
pub fn require_view_once_attachment_eligibility(mime_type: &str) -> Result<(), String> {
    if matches!(mime_type, "image/png" | "image/jpeg") {
        Ok(())
    } else {
        Err(view_once_attachment_refusal(mime_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_protected_in_process_images_are_view_once_eligible() {
        for mime in ["image/png", "image/jpeg"] {
            assert_eq!(require_view_once_attachment_eligibility(mime), Ok(()));
        }

        for mime in ["video/mp4", "application/pdf", "text/plain", "image/webp"] {
            let refusal = require_view_once_attachment_eligibility(mime)
                .expect_err("external-viewer content must be refused for view-once");
            assert!(refusal.contains(mime));
            assert!(refusal.contains("protected viewer"));
        }
    }
}
