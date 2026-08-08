//! Preparation boundary for X's protected send choices.
//!
//! Preparing and posting are deliberately separate operations. These handlers
//! bind an already-protected cover to the exact choice the person made; the X
//! placement layer is responsible for placing it later, and no code in this
//! module can press X's Send control.

use core::fmt;

/// The five X choices exposed by the protected composer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XSendChoice {
    Manual,
    DoubleEnter,
    SingleEnter,
    Instant,
    MatchTyping,
}

impl XSendChoice {
    pub const ALL: [Self; 5] = [
        Self::Manual,
        Self::DoubleEnter,
        Self::SingleEnter,
        Self::Instant,
        Self::MatchTyping,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::DoubleEnter => "Double Enter",
            Self::SingleEnter => "Single Enter",
            Self::Instant => "Instant",
            Self::MatchTyping => "Match typing",
        }
    }

    /// Strictly parse the label emitted by the X composer UI.
    ///
    /// Case folding or trimming here could silently turn malformed UI state
    /// into send authority, so only the five exact labels are accepted.
    pub fn from_name(name: &str) -> Result<Self, XCoverPreparationError> {
        match name {
            "Manual" => Ok(Self::Manual),
            "Double Enter" => Ok(Self::DoubleEnter),
            "Single Enter" => Ok(Self::SingleEnter),
            "Instant" => Ok(Self::Instant),
            "Match typing" => Ok(Self::MatchTyping),
            _ => Err(XCoverPreparationError::UnknownChoice(name.to_owned())),
        }
    }
}

/// A cover that is ready for the shared X placement job.
///
/// There is intentionally no `sent` or `posted` state here. Preparation proves
/// only that the exact cover and the exact choice are bound together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedXCover {
    pub choice: XSendChoice,
    pub cover_text: String,
    pub cover_bytes: usize,
}

impl PreparedXCover {
    fn new(choice: XSendChoice, cover_text: &str) -> Self {
        Self {
            choice,
            cover_text: cover_text.to_owned(),
            cover_bytes: cover_text.len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XCoverPreparationError {
    UnknownChoice(String),
    RefusedSendFieldValue(&'static str),
}

impl fmt::Display for XCoverPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChoice(choice) => write!(
                f,
                "unknown X send choice {choice:?}; expected Manual, Double Enter, Single Enter, Instant, or Match typing"
            ),
            Self::RefusedSendFieldValue(value) => {
                write!(f, "refused X send field value {value}")
            }
        }
    }
}

impl std::error::Error for XCoverPreparationError {}

/// Direct backend command used by the X UI to prepare one cover.
pub fn prepare_x_cover(
    choice_name: &str,
    cover_text: &str,
) -> Result<PreparedXCover, XCoverPreparationError> {
    match XSendChoice::from_name(choice_name)? {
        XSendChoice::Manual => prepare_manual_cover(cover_text),
        XSendChoice::DoubleEnter => prepare_double_enter_cover(cover_text),
        XSendChoice::SingleEnter => prepare_single_enter_cover(cover_text),
        XSendChoice::Instant => prepare_instant_cover(cover_text),
        XSendChoice::MatchTyping => prepare_match_typing_cover(cover_text),
    }
}

/// UI-facing state for X's send button.
///
/// Pressing this control only prepares the cover selected in the composer. It
/// deliberately has no website-driver dependency and therefore cannot post to
/// X as a side effect. The separately reviewed placement flow is the only code
/// allowed to put prepared text in a provider composer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XSendButton {
    selected_choice: XSendChoice,
}

/// Fields that must still be present when a selected cover is prepared.
///
/// The private text remains inside OSL. `composer_name` is only the accessible
/// name discovered for the provider composer; this boundary does not write to
/// that composer or press its Send control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct XSendFields<'a> {
    pub private_text: &'a str,
    pub composer_name: Option<&'a str>,
    pub cover_text: &'a str,
}

impl XSendButton {
    /// Connect the button to one of the exact choices shown by the composer.
    pub fn for_selected_choice(choice_name: &str) -> Result<Self, XCoverPreparationError> {
        Ok(Self {
            selected_choice: XSendChoice::from_name(choice_name)?,
        })
    }

    /// Prepare the selected cover. This is intentionally not named `send`:
    /// success means preparation only, never a provider post.
    pub fn prepare_selected_cover(
        &self,
        fields: XSendFields<'_>,
    ) -> Result<PreparedXCover, XCoverPreparationError> {
        if fields.private_text.trim().is_empty() {
            return Err(XCoverPreparationError::RefusedSendFieldValue("empty-text"));
        }
        if fields
            .composer_name
            .filter(|name| !name.trim().is_empty())
            .is_none()
        {
            return Err(XCoverPreparationError::RefusedSendFieldValue(
                "missing-composer",
            ));
        }
        prepare_x_cover(self.selected_choice.name(), fields.cover_text)
    }
}

fn prepare_manual_cover(cover_text: &str) -> Result<PreparedXCover, XCoverPreparationError> {
    Ok(PreparedXCover::new(XSendChoice::Manual, cover_text))
}

fn prepare_double_enter_cover(cover_text: &str) -> Result<PreparedXCover, XCoverPreparationError> {
    Ok(PreparedXCover::new(XSendChoice::DoubleEnter, cover_text))
}

fn prepare_single_enter_cover(cover_text: &str) -> Result<PreparedXCover, XCoverPreparationError> {
    Ok(PreparedXCover::new(XSendChoice::SingleEnter, cover_text))
}

fn prepare_instant_cover(cover_text: &str) -> Result<PreparedXCover, XCoverPreparationError> {
    Ok(PreparedXCover::new(XSendChoice::Instant, cover_text))
}

fn prepare_match_typing_cover(cover_text: &str) -> Result<PreparedXCover, XCoverPreparationError> {
    Ok(PreparedXCover::new(XSendChoice::MatchTyping, cover_text))
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_x_cover, XCoverPreparationError, XSendButton, XSendChoice, XSendFields,
    };

    #[test]
    fn task_1107_all_five_x_choices_prepare_covers_and_unknown_fails() {
        const COVER: &str = "x-cover-1107";
        let mut prepared_count = 0usize;

        for expected_choice in XSendChoice::ALL {
            let prepared = prepare_x_cover(expected_choice.name(), COVER)
                .expect("each named X choice prepares a cover");
            assert_eq!(prepared.choice, expected_choice);
            assert_eq!(prepared.cover_text, COVER);
            assert_eq!(prepared.cover_bytes, COVER.len());
            prepared_count += 1;
            println!(
                "TASK1107 choice={:?} prepared_cover_bytes={} exact_cover=true",
                expected_choice.name(),
                prepared.cover_bytes
            );
        }

        let unknown_name = "Unexpected sixth choice";
        let unknown =
            prepare_x_cover(unknown_name, COVER).expect_err("an unknown X choice must fail closed");
        assert_eq!(
            unknown,
            XCoverPreparationError::UnknownChoice(unknown_name.to_owned())
        );
        println!("TASK1107 prepared_cover_count={prepared_count}");
        println!("TASK1107 unknown_choice_failures=1 error={unknown}");

        assert_eq!(prepared_count, 5);
    }

    #[test]
    fn task_1108_each_x_send_button_choice_prepares_without_posting() {
        const COVER: &str = "x-prepared-cover-1108";
        let mut prepared_count = 0usize;

        for choice in XSendChoice::ALL {
            let button = XSendButton::for_selected_choice(choice.name())
                .expect("every rendered choice connects to the send button");
            let prepared = button
                .prepare_selected_cover(XSendFields {
                    private_text: "private text stays in OSL",
                    composer_name: Some("Post"),
                    cover_text: COVER,
                })
                .expect("pressing the button prepares the selected cover");

            assert_eq!(prepared.choice, choice);
            assert_eq!(prepared.cover_text, COVER);
            assert_eq!(prepared.cover_bytes, COVER.len());
            // PreparedXCover has no sent/posted state and the button has no
            // provider control or driver, so this path cannot post.
            prepared_count += 1;
            println!(
                "TASK1108 choice={} prepared_cover_bytes={} posted=false",
                choice.name(),
                prepared.cover_bytes
            );
        }

        assert_eq!(prepared_count, XSendChoice::ALL.len());
        println!("TASK1108 prepared_cover_count={prepared_count} posted_count=0");
    }
}

