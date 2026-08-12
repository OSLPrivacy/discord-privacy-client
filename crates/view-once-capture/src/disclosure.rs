//! The exact words shown to the viewer and to the sender, before use.
//!
//! These strings are the canonical copy. `apps/osl-hub-ui` carries a byte-
//! identical copy for rendering and a test asserts the two never drift. The
//! wording is deliberately negative-first: what OSL cannot see comes before
//! what it can, because a reader who stops after one sentence must not walk
//! away believing the feature is a guarantee.

/// Shown on the view-once viewer *before* the content is revealed, so the
/// person about to open it knows what will and will not be reported.
pub const CAPTURE_DISCLOSURE_VIEWER: &str = "\
Before you open this: OSL can detect only the screen-capture paths Windows \
reports to it — the PrintScreen key and the Windows snip, which put a picture \
on the clipboard. If OSL detects one of those while this is open, the sender \
is told once. OSL cannot detect a camera pointed at your screen, an external \
capture device, or every capture tool, and OSL does not stop screenshots.";

/// Shown on the composer *before* view-once is sent, so the sender knows what
/// the absence of a notification does and does not mean.
pub const CAPTURE_DISCLOSURE_SENDER: &str = "\
Before you send this: OSL can detect only the screen-capture paths Windows \
reports to it — the PrintScreen key and the Windows snip, which put a picture \
on the clipboard. You are told once if OSL detects one of those. OSL cannot \
detect a camera pointed at their screen, an external capture device, or every \
capture tool, and OSL does not stop screenshots. No notification does not \
mean no copy was made.";

/// The capture paths OSL cannot see, named in the copy above. These exist as
/// data so a test can assert every one of them is actually disclosed, rather
/// than trusting that someone read the paragraph.
pub const UNSUPPORTED_CAPTURE_PATHS: &[&str] = &[
    "a camera pointed at",
    "an external capture device",
    "every capture tool",
];

/// Phrases that would turn this feature into a promise it cannot keep. Any of
/// these appearing in view-once capture copy is a defect, not a style note.
const ABSOLUTE_CAPTURE_CLAIMS: &[&str] = &[
    "screenshot-proof",
    "screenshot proof",
    "prevents screenshots",
    "prevent screenshots",
    "blocks screenshots",
    "block screenshots",
    "stops screenshots",
    "stops all screenshots",
    "cannot be screenshotted",
    "cannot be captured",
    "impossible to screenshot",
    "detects all screenshots",
    "detects every screenshot",
    "detects any screenshot",
    "all screenshots are detected",
    "every screenshot is detected",
    "you will always know",
    "always notified",
    "guaranteed notification",
    "no one can screenshot",
    "nobody can screenshot",
    "screenshots are impossible",
    "fully protected from capture",
];

/// Return every absolute capture claim present in `text`, lowercased.
///
/// Used by the shipped check to fail the build rather than the user: copy that
/// over-promises is the failure mode this whole task exists to avoid.
pub fn absolute_capture_claims_in(text: &str) -> Vec<&'static str> {
    let haystack = text.to_lowercase();
    ABSOLUTE_CAPTURE_CLAIMS
        .iter()
        .copied()
        .filter(|claim| haystack.contains(claim))
        .collect()
}

/// True when `text` carries the limitation both surfaces must show before use:
/// detection is limited to supported OS paths, the undetectable paths are
/// named, and prevention is disclaimed.
pub fn discloses_capture_limits(text: &str) -> bool {
    let haystack = text.to_lowercase();
    haystack.contains("only the screen-capture paths windows reports")
        && UNSUPPORTED_CAPTURE_PATHS
            .iter()
            .all(|path| haystack.contains(path))
        && haystack.contains("does not stop screenshots")
        && absolute_capture_claims_in(text).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_surfaces_disclose_the_limits_before_use() {
        assert!(discloses_capture_limits(CAPTURE_DISCLOSURE_VIEWER));
        assert!(discloses_capture_limits(CAPTURE_DISCLOSURE_SENDER));
    }

    #[test]
    fn shipped_copy_makes_no_absolute_capture_claim() {
        assert_eq!(
            absolute_capture_claims_in(CAPTURE_DISCLOSURE_VIEWER),
            Vec::<&str>::new()
        );
        assert_eq!(
            absolute_capture_claims_in(CAPTURE_DISCLOSURE_SENDER),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn an_absolute_claim_is_caught_wherever_it_is_written() {
        assert_eq!(
            absolute_capture_claims_in("OSL detects all screenshots of view-once media."),
            vec!["detects all screenshots"]
        );
        assert_eq!(
            absolute_capture_claims_in("This media is Screenshot-Proof."),
            vec!["screenshot-proof"]
        );
        assert!(!discloses_capture_limits(
            "OSL detects every screenshot. A camera pointed at the screen is not detected."
        ));
    }

    #[test]
    fn copy_that_drops_one_unsupported_path_stops_disclosing() {
        let stripped = CAPTURE_DISCLOSURE_VIEWER.replace("an external capture device, or ", "");
        assert!(!discloses_capture_limits(&stripped));
    }

    #[test]
    fn copy_that_drops_the_prevention_disclaimer_stops_disclosing() {
        let stripped = CAPTURE_DISCLOSURE_SENDER.replace(", and OSL does not stop screenshots", "");
        assert!(!discloses_capture_limits(&stripped));
    }
}
