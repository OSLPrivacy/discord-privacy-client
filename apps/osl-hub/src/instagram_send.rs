//! Preparation boundary for Instagram's protected send choices.
//!
//! Preparing and posting are deliberately separate operations. These handlers
//! bind an already-protected cover to the exact choice the person made; the
//! website placement layer is responsible for placing it later, and no code in
//! this module can press Instagram's Send control.

use core::fmt;

/// The five Instagram choices exposed by the protected composer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstagramSendChoice {
    Manual,
    DoubleEnter,
    SingleEnter,
    Instant,
    MatchTyping,
}

impl InstagramSendChoice {
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

    /// Strictly parse the label emitted by the Instagram composer UI.
    ///
    /// Case folding or trimming here could silently turn malformed UI state
    /// into send authority, so only the five exact labels are accepted.
    pub fn from_name(name: &str) -> Result<Self, InstagramCoverPreparationError> {
        match name {
            "Manual" => Ok(Self::Manual),
            "Double Enter" => Ok(Self::DoubleEnter),
            "Single Enter" => Ok(Self::SingleEnter),
            "Instant" => Ok(Self::Instant),
            "Match typing" => Ok(Self::MatchTyping),
            _ => Err(InstagramCoverPreparationError::UnknownChoice(
                name.to_owned(),
            )),
        }
    }
}

/// A cover that is ready for the shared website placement job.
///
/// There is intentionally no `sent` or `posted` state here. Preparation proves
/// only that the exact cover and the exact choice are bound together.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedInstagramCover {
    pub choice: InstagramSendChoice,
    pub cover_text: String,
    pub cover_bytes: usize,
}

impl PreparedInstagramCover {
    fn new(choice: InstagramSendChoice, cover_text: &str) -> Self {
        Self {
            choice,
            cover_text: cover_text.to_owned(),
            cover_bytes: cover_text.len(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstagramCoverPreparationError {
    UnknownChoice(String),
}

impl fmt::Display for InstagramCoverPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChoice(choice) => write!(
                f,
                "unknown Instagram send choice {choice:?}; expected Manual, Double Enter, Single Enter, Instant, or Match typing"
            ),
        }
    }
}

impl std::error::Error for InstagramCoverPreparationError {}

/// Direct backend command used by the Instagram UI to prepare one cover.
pub fn prepare_instagram_cover(
    choice_name: &str,
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    match InstagramSendChoice::from_name(choice_name)? {
        InstagramSendChoice::Manual => prepare_manual_cover(cover_text),
        InstagramSendChoice::DoubleEnter => prepare_double_enter_cover(cover_text),
        InstagramSendChoice::SingleEnter => prepare_single_enter_cover(cover_text),
        InstagramSendChoice::Instant => prepare_instant_cover(cover_text),
        InstagramSendChoice::MatchTyping => prepare_match_typing_cover(cover_text),
    }
}

/// UI-facing state for Instagram's send button.
///
/// Pressing this control only prepares the cover selected in the composer. It
/// deliberately has no website-driver dependency and therefore cannot post to
/// Instagram as a side effect. The separately reviewed placement flow is the
/// only code allowed to put prepared text in a provider composer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramSendButton {
    selected_choice: InstagramSendChoice,
}

impl InstagramSendButton {
    /// Connect the button to one of the exact choices shown by the composer.
    pub fn for_selected_choice(choice_name: &str) -> Result<Self, InstagramCoverPreparationError> {
        Ok(Self {
            selected_choice: InstagramSendChoice::from_name(choice_name)?,
        })
    }

    /// Prepare the selected cover. This is intentionally not named `send`:
    /// success means preparation only, never a provider post.
    pub fn prepare_selected_cover(
        &self,
        cover_text: &str,
    ) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
        prepare_instagram_cover(self.selected_choice.name(), cover_text)
    }
}

fn prepare_manual_cover(
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    Ok(PreparedInstagramCover::new(
        InstagramSendChoice::Manual,
        cover_text,
    ))
}

fn prepare_double_enter_cover(
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    Ok(PreparedInstagramCover::new(
        InstagramSendChoice::DoubleEnter,
        cover_text,
    ))
}

fn prepare_single_enter_cover(
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    Ok(PreparedInstagramCover::new(
        InstagramSendChoice::SingleEnter,
        cover_text,
    ))
}

fn prepare_instant_cover(
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    Ok(PreparedInstagramCover::new(
        InstagramSendChoice::Instant,
        cover_text,
    ))
}

fn prepare_match_typing_cover(
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    Ok(PreparedInstagramCover::new(
        InstagramSendChoice::MatchTyping,
        cover_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_instagram_cover, InstagramCoverPreparationError, InstagramSendButton,
        InstagramSendChoice,
    };

    #[test]
    fn task_1137_all_five_instagram_choices_prepare_covers_and_unknown_fails() {
        const COVER: &str = "instagram-cover-1137";
        let mut prepared_count = 0usize;

        for expected_choice in InstagramSendChoice::ALL {
            let prepared = prepare_instagram_cover(expected_choice.name(), COVER)
                .expect("each named Instagram choice prepares a cover");
            assert_eq!(prepared.choice, expected_choice);
            assert_eq!(prepared.cover_text, COVER);
            assert_eq!(prepared.cover_bytes, COVER.len());
            prepared_count += 1;
            println!(
                "TASK1137 choice={:?} prepared_cover_bytes={} exact_cover=true",
                expected_choice.name(),
                prepared.cover_bytes
            );
        }

        let unknown_name = "Unexpected sixth choice";
        let unknown = prepare_instagram_cover(unknown_name, COVER)
            .expect_err("an unknown Instagram choice must fail closed");
        assert_eq!(
            unknown,
            InstagramCoverPreparationError::UnknownChoice(unknown_name.to_owned())
        );
        println!("TASK1137 prepared_cover_count={prepared_count}");
        println!("TASK1137 unknown_choice_failures=1 error={unknown}");

        assert_eq!(prepared_count, 5);
    }

    #[test]
    fn task_1138_each_instagram_send_button_choice_prepares_without_posting() {
        const COVER: &str = "instagram-prepared-cover-1138";
        let mut prepared_count = 0usize;

        for choice in InstagramSendChoice::ALL {
            let button = InstagramSendButton::for_selected_choice(choice.name())
                .expect("every rendered choice connects to the send button");
            let prepared = button
                .prepare_selected_cover(COVER)
                .expect("pressing the button prepares the selected cover");

            assert_eq!(prepared.choice, choice);
            assert_eq!(prepared.cover_text, COVER);
            assert_eq!(prepared.cover_bytes, COVER.len());
            // PreparedInstagramCover has no sent/posted state and the button
            // has no provider control or driver, so this path cannot post.
            prepared_count += 1;
            println!(
                "TASK1138 choice={} prepared_cover_bytes={} posted=false",
                choice.name(),
                prepared.cover_bytes
            );
        }

        assert_eq!(prepared_count, InstagramSendChoice::ALL.len());
        println!("TASK1138 prepared_cover_count={prepared_count} posted_count=0");
    }
}
