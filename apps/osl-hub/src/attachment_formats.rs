//! What the trusted attachment picker may offer, derived from what the
//! receiving side can actually open.
//!
//! The send and receive halves of the protected-attachment path used to state
//! their supported formats independently: `ipc::attachment_wire`'s MIME table
//! mapped `gif`/`webp`, the native picker offered both extensions, and the
//! receiver refused every image MIME except PNG and JPEG. The result was an
//! attachment the operator could select, encrypt, upload and deliver, and the
//! recipient could never open.
//!
//! So there is exactly one authority here — [`accepted_attachment_mime`] — and
//! the picker's extension filter and every human-readable format list are
//! computed from it. A candidate extension the recipient cannot open is dropped
//! before the dialog ever sees it, so the two halves cannot drift apart again.
//!
//! TASK 6832 added the second receive-side surface this derivation asks about.
//! `gif` used to be filtered out here for a real reason — the only image viewer
//! was the Windows WIC single-frame decoder, which refuses an animated GIF — and
//! the result was a format every surface named and no surface could send. There
//! is now a GIF player ([`crate::gif_message`]), so `gif` is offered because a
//! recipient can actually open it, on exactly the same derived rule as before.
//! `webp` is still dropped, and still for its own reason: static WebP is a
//! Store-delivered codec on Windows 10 and so not guaranteed present.

/// Every extension OSL knows how to name a MIME type for.
///
/// This is deliberately *not* the offered list. It is the candidate set, and it
/// still contains `webp` on purpose: it is what
/// `ipc::attachment_wire::mime_for_filename` maps, and leaving it here keeps the
/// filtering in [`offered_attachment_extensions`] load-bearing rather than
/// decorative.
const CANDIDATE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "mp4", "webm", "mov", "mp3", "m4a", "wav", "flac", "pdf",
    "txt", "md", "csv", "json", "docx", "xlsx", "pptx", "odt", "ods", "odp", "zip", "7z", "rar",
    "tar", "gz",
];

/// The one authority: the MIME type OSL will encrypt this filename as, or
/// `None` when the recipient could not open the result.
///
/// Two gates, in the order the receiver applies them: the wire MIME table (which
/// also refuses executable and script extensions), then the receive-side image
/// surfaces for anything that would be handed to a viewer.
pub fn accepted_attachment_mime(filename: &str) -> Option<&'static str> {
    let mime = ipc::attachment_wire::mime_for_filename(filename)?;
    if mime.starts_with("image/") && !protected_image_is_openable(mime) {
        return None;
    }
    Some(mime)
}

/// Whether *some* OSL-owned surface can open this image MIME type.
///
/// There are two, and they are not interchangeable: the Windows WIC viewer
/// (`peer_attachment_io::supported_protected_image_mime`) decodes exactly one
/// frame, and the GIF player ([`crate::gif_message::playable_gif_mime`]) is the
/// one that can hold an animation. Both the picker filter and the receive-side
/// refusal in `native_attachment_transport` ask this question, so neither can
/// offer a format the other would reject.
pub fn protected_image_is_openable(mime: &str) -> bool {
    crate::peer_attachment_io::supported_protected_image_mime(mime)
        || crate::gif_message::playable_gif_mime(mime)
}

/// The same check for a bare extension, used to build the picker's filter.
fn accepted_extension_mime(extension: &str) -> Option<&'static str> {
    accepted_attachment_mime(&format!("attachment.{extension}"))
}

/// The extension filter the trusted picker offers.
///
/// Derived, never restated: every candidate is put through
/// [`accepted_attachment_mime`], so an extension the recipient cannot open never
/// reaches the dialog even if a later change adds it to `CANDIDATE_EXTENSIONS`.
pub fn offered_attachment_extensions() -> Vec<&'static str> {
    CANDIDATE_EXTENSIONS
        .iter()
        .copied()
        .filter(|extension| accepted_extension_mime(extension).is_some())
        .collect()
}

/// The offered extensions whose MIME type is an image, for messages that need to
/// say which picture formats survive the protected viewer.
pub fn offered_image_extensions() -> Vec<&'static str> {
    CANDIDATE_EXTENSIONS
        .iter()
        .copied()
        .filter(|extension| {
            accepted_extension_mime(extension).is_some_and(|mime| mime.starts_with("image/"))
        })
        .collect()
}

/// Refusal shown when a selection cannot be sent, before anything is encrypted.
///
/// Names the formats that do work instead of only saying no, and derives both
/// lists so the message can never advertise something the recipient would
/// reject. Contains no filename, path, object id or capability token.
pub fn unsupported_selection_message() -> String {
    format!(
        "OSL cannot send this file type privately, because the recipient could not open it. \
         Protected pictures must be {}; other supported files are {}.",
        offered_image_extensions().join(", "),
        offered_attachment_extensions()
            .into_iter()
            .filter(|extension| {
                !accepted_extension_mime(extension).is_some_and(|mime| mime.starts_with("image/"))
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Refusal shown when an inbound protected image arrives in a format the
/// in-memory viewer cannot decode.
///
/// Inbound filenames come from the sender's wire payload, not from this device's
/// picker, so this case survives the picker fix and must keep its own honest
/// message. The format list comes from the same derivation.
pub fn unsupported_protected_image_message() -> String {
    format!(
        "OSL's protected picture viewer cannot open this image format. \
         It decodes these picture formats: {}.",
        offered_image_extensions().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The candidate set must never contain an extension the wire MIME table
    /// does not know, or the picker would silently shrink for the wrong reason.
    #[test]
    fn every_candidate_extension_has_a_wire_mime_type() {
        for extension in CANDIDATE_EXTENSIONS {
            assert!(
                ipc::attachment_wire::mime_for_filename(&format!("attachment.{extension}"))
                    .is_some(),
                "candidate extension has no wire MIME type: {extension}"
            );
        }
    }

    /// A format is offered exactly when a receive-side surface can open it.
    ///
    /// TASK 6832 built the GIF player, so `gif` moved across this line for the
    /// only reason that may move anything across it. `webp` did not, and the
    /// filter is still load-bearing because of that.
    #[test]
    fn gif_is_offered_because_it_has_a_player_and_webp_still_is_not() {
        assert!(CANDIDATE_EXTENSIONS.contains(&"gif"));
        assert!(CANDIDATE_EXTENSIONS.contains(&"webp"));
        assert_eq!(accepted_attachment_mime("clip.gif"), Some("image/gif"));
        assert_eq!(accepted_attachment_mime("photo.webp"), None);
        let offered = offered_attachment_extensions();
        assert!(offered.contains(&"gif"));
        assert!(!offered.contains(&"webp"));
        // The GIF player, not the single-frame WIC viewer, is what admits it.
        assert!(!crate::peer_attachment_io::supported_protected_image_mime("image/gif"));
        assert!(crate::gif_message::playable_gif_mime("image/gif"));
        assert!(protected_image_is_openable("image/gif"));
        assert!(!protected_image_is_openable("image/webp"));
    }

    /// Every extension the picker offers must be one the receive side accepts.
    /// This is the assertion that keeps the filter and the allowlist in step.
    #[test]
    fn every_offered_extension_is_accepted_by_the_receive_side() {
        let offered = offered_attachment_extensions();
        assert!(!offered.is_empty());
        for extension in &offered {
            let mime = accepted_attachment_mime(&format!("attachment.{extension}"))
                .unwrap_or_else(|| panic!("offered extension is not accepted: {extension}"));
            if mime.starts_with("image/") {
                assert!(
                    protected_image_is_openable(mime),
                    "offered image extension no surface can open: {extension}"
                );
            }
        }
    }

    #[test]
    fn offered_images_are_exactly_png_jpeg_and_gif_extensions() {
        assert_eq!(offered_image_extensions(), vec!["jpg", "jpeg", "png", "gif"]);
    }

    #[test]
    fn non_image_formats_stay_offered() {
        for extension in ["mp4", "pdf", "zip", "txt", "flac", "docx"] {
            assert!(
                offered_attachment_extensions().contains(&extension),
                "non-image format was dropped: {extension}"
            );
        }
    }

    /// The picker must not become a route for the wire table's blocked
    /// executable and script extensions.
    #[test]
    fn executables_and_scripts_are_refused() {
        for filename in ["run.exe", "script.ps1", "payload.svg", "no_extension"] {
            assert_eq!(accepted_attachment_mime(filename), None, "{filename}");
        }
    }

    /// A refusal that names no supported format is not an honest refusal.
    #[test]
    fn refusal_messages_name_the_supported_formats() {
        let selection = unsupported_selection_message();
        assert!(selection.contains("png"));
        assert!(selection.contains("jpg"));
        assert!(selection.contains("mp4"));
        assert!(!selection.contains("gif"));
        assert!(!selection.contains("webp"));
        let inbound = unsupported_protected_image_message();
        assert!(inbound.contains("png"));
        assert!(inbound.contains("jpeg"));
        assert!(!inbound.contains("gif"));
    }
}
