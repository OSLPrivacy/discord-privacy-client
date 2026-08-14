//! Pro gate for the two shared cover-writing buttons.
//!
//! Every app and story/post composer uses the same two controls, so their
//! backend reaches this single boundary.  The entitlement is read from the
//! cached, keyserver-derived [`ipc::AppState`] record at the last moment; a
//! renderer cannot grant itself Pro by supplying a boolean or plan name.
//!
//! This gate applies only when the person explicitly chooses `Covertext` or
//! `AI Covertext`.  Protected sends with no chosen writer continue through
//! [`write_ordinary_cover_message`] for Free and Pro identities alike.

use ipc::AppState;

pub const COVER_WRITING_PRO_REFUSAL: &str = "cover_writing_requires_pro";

/// The two choices exposed by the shared composer control from TASK 3518.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverWritingButton {
    Covertext,
    AiCovertext,
}

impl CoverWritingButton {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Covertext => "Covertext",
            Self::AiCovertext => "AI Covertext",
        }
    }
}

/// A message returned only after the writer actually produced non-empty cover.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WrittenCoverMessage {
    pub text: String,
}

/// Backend seam shared by the selected and ordinary cover routes.
///
/// Implementations may be the word-bank writer, the local model writer, or the
/// ordinary carrier.  Keeping both operations on one seam lets the gate's test
/// prove that a Free button press never reaches a chosen writer while an
/// ordinary Free send still reaches its own writer once.
pub trait CoverMessageWriter {
    fn write_chosen_cover(&mut self, button: CoverWritingButton) -> Result<String, String>;

    fn write_ordinary_cover(&mut self) -> Result<String, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverWritingError {
    ProRequired {
        refusal: &'static str,
        button: CoverWritingButton,
        raw_license_state: String,
    },
    WriterFailed(String),
    EmptyCoverMessage,
}

impl CoverWritingError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ProRequired { refusal, .. } => refusal,
            Self::WriterFailed(_) => "cover_writer_failed",
            Self::EmptyCoverMessage => "cover_writer_returned_empty_message",
        }
    }
}

impl std::fmt::Display for CoverWritingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProRequired {
                refusal,
                button,
                raw_license_state,
            } => write!(
                formatter,
                "{refusal}: {} is a Pro cover-writing choice (raw_license_state={raw_license_state})",
                button.label(),
            ),
            Self::WriterFailed(message) => write!(formatter, "cover writer failed: {message}"),
            Self::EmptyCoverMessage => formatter.write_str("cover writer returned an empty message"),
        }
    }
}

impl std::error::Error for CoverWritingError {}

fn written(text: String) -> Result<WrittenCoverMessage, CoverWritingError> {
    if text.trim().is_empty() {
        Err(CoverWritingError::EmptyCoverMessage)
    } else {
        Ok(WrittenCoverMessage { text })
    }
}

/// Press one of the two cover-writing buttons.
///
/// The entitlement check deliberately precedes the writer call.  Free,
/// expired, revoked, and unconfigured records therefore cannot generate a
/// cover through a chosen setting, even if the caller can invoke this function
/// directly.  Paid offline grace remains Pro-equivalent under the existing
/// tier contract.
pub fn press_cover_writing_button<W: CoverMessageWriter>(
    state: &AppState,
    button: CoverWritingButton,
    writer: &mut W,
) -> Result<WrittenCoverMessage, CoverWritingError> {
    if !ipc::tier_gate::is_paid_equivalent(state) {
        let raw_license_state = state
            .license_state
            .lock()
            .expect("license_state mutex poisoned")
            .raw_status
            .clone();
        return Err(CoverWritingError::ProRequired {
            refusal: COVER_WRITING_PRO_REFUSAL,
            button,
            raw_license_state,
        });
    }

    writer
        .write_chosen_cover(button)
        .map_err(CoverWritingError::WriterFailed)
        .and_then(written)
}

/// Write the normal carrier used when neither Pro button was chosen.
///
/// This route intentionally has no entitlement input or check.  Cover is part
/// of ordinary protected delivery; only choosing how it is written is Pro.
pub fn write_ordinary_cover_message<W: CoverMessageWriter>(
    writer: &mut W,
) -> Result<WrittenCoverMessage, CoverWritingError> {
    writer
        .write_ordinary_cover()
        .map_err(CoverWritingError::WriterFailed)
        .and_then(written)
}
