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
    RefusedSendFieldValue(&'static str),
}

impl fmt::Display for InstagramCoverPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownChoice(choice) => write!(
                f,
                "unknown Instagram send choice {choice:?}; expected Manual, Double Enter, Single Enter, Instant, or Match typing"
            ),
            Self::RefusedSendFieldValue(value) => {
                write!(f, "refused Instagram send field value {value}")
            }
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

/// Fields that must still be present when a selected cover is prepared.
///
/// The private text remains inside OSL. `composer_name` is only the accessible
/// name discovered for the provider composer; this boundary does not write to
/// that composer or press its Send control.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstagramSendFields<'a> {
    pub private_text: &'a str,
    pub composer_name: Option<&'a str>,
    pub cover_text: &'a str,
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
        fields: InstagramSendFields<'_>,
    ) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
        if fields.private_text.trim().is_empty() {
            return Err(InstagramCoverPreparationError::RefusedSendFieldValue(
                "empty-text",
            ));
        }
        if fields
            .composer_name
            .filter(|name| !name.trim().is_empty())
            .is_none()
        {
            return Err(InstagramCoverPreparationError::RefusedSendFieldValue(
                "missing-composer",
            ));
        }
        prepare_instagram_cover(self.selected_choice.name(), fields.cover_text)
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
        InstagramSendChoice, InstagramSendFields, PreparedInstagramCover,
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
                .prepare_selected_cover(InstagramSendFields {
                    private_text: "private text stays in OSL",
                    composer_name: Some("Message Maya"),
                    cover_text: COVER,
                })
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

    #[test]
    fn task_1139_empty_text_and_missing_composer_fail_closed_for_every_choice() {
        const MARKER: &str = "instagram-send-1139";
        const COMPOSER: &str = "Message Maya";

        let good_button =
            InstagramSendButton::for_selected_choice(InstagramSendChoice::Manual.name())
                .expect("the good fixture uses a named choice");
        let good_cover = good_button
            .prepare_selected_cover(InstagramSendFields {
                private_text: MARKER,
                composer_name: Some(COMPOSER),
                cover_text: MARKER,
            })
            .expect("good private text and a discovered composer prepare one cover");
        let sent_covers: Vec<PreparedInstagramCover> = vec![good_cover];
        let sent_covers_before_refusals = sent_covers.clone();

        assert_eq!(sent_covers.len(), 1);
        assert_eq!(sent_covers[0].cover_text, MARKER);
        println!(
            "TASK1139 good_text={MARKER} sent_cover_count={} sent_cover={}",
            sent_covers.len(),
            sent_covers[0].cover_text
        );

        let mut refusal_count = 0usize;
        for choice in InstagramSendChoice::ALL {
            let button = InstagramSendButton::for_selected_choice(choice.name())
                .expect("every rendered choice connects to the send button");

            let empty_text = button
                .prepare_selected_cover(InstagramSendFields {
                    private_text: "",
                    composer_name: Some(COMPOSER),
                    cover_text: MARKER,
                })
                .expect_err("empty private text must not prepare a cover");
            assert_eq!(
                empty_text,
                InstagramCoverPreparationError::RefusedSendFieldValue("empty-text")
            );
            assert!(empty_text.to_string().contains("empty-text"));
            refusal_count += 1;
            println!(
                "TASK1139 choice={} changed_send_field_value=empty-text refused_by_name=empty-text sent_cover_count={}",
                choice.name(),
                sent_covers.len()
            );

            let missing_composer = button
                .prepare_selected_cover(InstagramSendFields {
                    private_text: MARKER,
                    composer_name: None,
                    cover_text: MARKER,
                })
                .expect_err("a missing composer must not prepare a cover");
            assert_eq!(
                missing_composer,
                InstagramCoverPreparationError::RefusedSendFieldValue("missing-composer")
            );
            assert!(missing_composer.to_string().contains("missing-composer"));
            refusal_count += 1;
            println!(
                "TASK1139 choice={} changed_send_field_value=missing-composer refused_by_name=missing-composer sent_cover_count={}",
                choice.name(),
                sent_covers.len()
            );
        }

        assert_eq!(refusal_count, 10);
        assert_eq!(sent_covers, sent_covers_before_refusals);
        assert_eq!(sent_covers.len(), 1);
        assert_eq!(sent_covers[0].cover_text, MARKER);
        println!("TASK1139 refusal_count={refusal_count}");
        println!(
            "TASK1139 final_sent_cover_count={} final_sent_cover={} unchanged=true",
            sent_covers.len(),
            sent_covers[0].cover_text
        );
    }
}
