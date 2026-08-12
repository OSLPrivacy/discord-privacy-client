//! TASK 6844 — telling a sender, honestly, that their view-once media was
//! screenshotted.
//!
//! # What this can and cannot do
//!
//! Windows exposes a small number of capture paths that a normal user-mode
//! process can *observe*. This crate integrates those and only those:
//!
//! * the PrintScreen / Alt+PrintScreen key, seen through a low-level keyboard
//!   hook, and
//! * a bitmap arriving on the clipboard, seen through
//!   `AddClipboardFormatListener`, which is how PrintScreen, Alt+PrintScreen
//!   and the Windows snip (Win+Shift+S) all deliver their result.
//!
//! Everything else is invisible to us and this crate says so in its shipped
//! copy rather than in a comment. A phone camera pointed at the monitor, a
//! capture card, a virtual display driver, a screen recorder that BitBlts the
//! desktop into its own process and never touches the clipboard — none of
//! these produce a signal we can see. There is no Windows API that reports
//! them, so no amount of implementation effort here would change that.
//!
//! This crate also does not *prevent* anything. Prevention is
//! `SetWindowDisplayAffinity`, it lives elsewhere, it is best-effort, and it
//! is a separate mechanism from the detection here. Nothing in this crate may
//! be described as making content screenshot-proof.
//!
//! # Why the event is signed and bound
//!
//! A "someone screenshotted your message" notification is an accusation. If
//! any peer could mint one, or replay one, or point one at a message the
//! viewer never received, the notification would be worth nothing and worse
//! than nothing. So a capture event is:
//!
//! * **bound** — it names the exact message, sender, viewer, viewer device and
//!   the nonce of the one viewer open session it happened during;
//! * **signed** — Ed25519, by the viewer device key, over a
//!   domain-separated, length-prefixed encoding of that binding, so no field
//!   can be slid into another;
//! * **deduplicated** — the sender's ledger records the open nonce and raises
//!   exactly one notification for it, however many times the event arrives.
//!
//! # Why the event is durable
//!
//! Capture happens while the viewer is looking at the screen, which is exactly
//! when their network may be down and exactly when the app may be killed. The
//! signed event is written to a durable outbox *before* any delivery attempt,
//! and is cleared only once the sender has acknowledged it, so a reconnect or
//! a restart delivers it rather than losing it.

/// The source tree this library was compiled from.
///
/// TASK 6845 builds this crate from throwaway copies and runs the 6844 check
/// against them. Two copies of the crate produce artifacts with the same file
/// name, so a shared target directory lets one build's library end up linked
/// into another build's binary while cargo still reports both as fresh. That
/// happened, silently, and a mutated library answered for the shipping one.
/// The check prints this constant so the library that actually answered can be
/// named, rather than assumed.
pub const BUILT_FROM: &str = env!("CARGO_MANIFEST_DIR");

pub mod disclosure;
pub mod event;
pub mod notifier;
pub mod outbox;
pub mod sig;

#[cfg(windows)]
pub mod windows_watch;

pub use disclosure::{
    absolute_capture_claims_in, CAPTURE_DISCLOSURE_SENDER, CAPTURE_DISCLOSURE_VIEWER,
    UNSUPPORTED_CAPTURE_PATHS,
};
pub use event::{
    sign_capture_event, verify_capture_event, CaptureEvidence, CaptureRealness, SignedCaptureEvent,
    SupportedCapturePath, UnsupportedCapturePath, ViewOnceOpenBinding, EVENT_DOMAIN,
    MIN_DISTINCT_SAMPLED_COLORS, MIN_LIVE_MATCH_PPM,
};
pub use notifier::{AcceptOutcome, SenderCaptureNotifier, SentViewOnceRecord};
pub use outbox::{CaptureOutbox, OutboxRecord};
