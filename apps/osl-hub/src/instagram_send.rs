//! Preparation boundary for Instagram protected-send triggers.
//!
//! A trigger describes how a prepared cover leaves OSL. Cover insertion is a
//! separate preference describing how that cover reaches Instagram's composer.
//! Neither choice grants this module authority to press Instagram's Send
//! control.

use core::fmt;

/// The complete, ruled set of Instagram send triggers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstagramSendTrigger {
    Enter,
    EnterX2,
    Clipboard,
}

impl InstagramSendTrigger {
    pub const ALL: [Self; 3] = [Self::Enter, Self::EnterX2, Self::Clipboard];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::EnterX2 => "Enter x2",
            Self::Clipboard => "Clipboard",
        }
    }

    pub fn from_name(name: &str) -> Result<Self, InstagramCoverPreparationError> {
        match name {
            "Enter" => Ok(Self::Enter),
            "Enter x2" => Ok(Self::EnterX2),
            "Clipboard" => Ok(Self::Clipboard),
            _ => Err(InstagramCoverPreparationError::UnknownTrigger(
                name.to_owned(),
            )),
        }
    }
}

/// How a prepared cover is inserted, independently of its send trigger.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstagramCoverInsertion {
    InsertOnSend,
    TypeNaturally,
}

impl InstagramCoverInsertion {
    pub const ALL: [Self; 2] = [Self::InsertOnSend, Self::TypeNaturally];

    pub const fn name(self) -> &'static str {
        match self {
            Self::InsertOnSend => "Insert on send",
            Self::TypeNaturally => "Type naturally",
        }
    }

    pub fn from_name(name: &str) -> Result<Self, InstagramCoverPreparationError> {
        match name {
            "Insert on send" => Ok(Self::InsertOnSend),
            "Type naturally" => Ok(Self::TypeNaturally),
            _ => Err(InstagramCoverPreparationError::UnknownCoverInsertion(
                name.to_owned(),
            )),
        }
    }
}

/// A cover prepared for a trigger and an independently selected insertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedInstagramCover {
    pub trigger: InstagramSendTrigger,
    pub cover_insertion: InstagramCoverInsertion,
    pub cover_text: String,
    pub cover_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstagramCoverPreparationError {
    UnknownTrigger(String),
    UnknownCoverInsertion(String),
}

impl fmt::Display for InstagramCoverPreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTrigger(trigger) => write!(
                f,
                "unknown Instagram send trigger {trigger:?}; expected Enter, Enter x2, or Clipboard"
            ),
            Self::UnknownCoverInsertion(insertion) => write!(
                f,
                "unknown Instagram cover insertion {insertion:?}; expected Insert on send or Type naturally"
            ),
        }
    }
}

impl std::error::Error for InstagramCoverPreparationError {}

/// Prepare a cover without placing or sending it.
pub fn prepare_instagram_cover(
    trigger_name: &str,
    cover_insertion_name: &str,
    cover_text: &str,
) -> Result<PreparedInstagramCover, InstagramCoverPreparationError> {
    let trigger = InstagramSendTrigger::from_name(trigger_name)?;
    let cover_insertion = InstagramCoverInsertion::from_name(cover_insertion_name)?;
    Ok(PreparedInstagramCover {
        trigger,
        cover_insertion,
        cover_text: cover_text.to_owned(),
        cover_bytes: cover_text.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_instagram_cover, InstagramCoverInsertion, InstagramCoverPreparationError,
        InstagramSendTrigger,
    };

    #[test]
    fn task_1137_exact_three_triggers_prepare_and_retired_modes_are_refused_by_name() {
        const COVER: &str = "instagram-cover-1137";
        let mut prepared_trigger_count = 0usize;
        let mut prepared_combination_count = 0usize;

        for trigger in InstagramSendTrigger::ALL {
            for insertion in InstagramCoverInsertion::ALL {
                let prepared = prepare_instagram_cover(trigger.name(), insertion.name(), COVER)
                    .expect("each ruled trigger prepares a cover with either insertion choice");
                assert_eq!(prepared.trigger, trigger);
                assert_eq!(prepared.cover_insertion, insertion);
                assert_eq!(prepared.cover_text, COVER);
                assert_eq!(prepared.cover_bytes, COVER.len());
                prepared_combination_count += 1;
            }
            prepared_trigger_count += 1;
            println!(
                "TASK1137 trigger={:?} prepared_cover_bytes={} insertion_choices_prepared=2 exact_cover=true",
                trigger.name(),
                COVER.len()
            );
        }

        let mut refused_count = 0usize;
        for retired in ["Manual", "Instant", "Match typing"] {
            let error = prepare_instagram_cover(retired, "Insert on send", COVER)
                .expect_err("retired Instagram mode must fail closed by exact name");
            assert_eq!(
                error,
                InstagramCoverPreparationError::UnknownTrigger(retired.to_owned())
            );
            refused_count += 1;
            println!("TASK1137 refused_trigger={retired:?}");
        }

        assert_eq!(prepared_trigger_count, 3);
        assert_eq!(prepared_combination_count, 6);
        assert_eq!(refused_count, 3);
        println!("TASK1137 prepared_trigger_count={prepared_trigger_count}");
        println!("TASK1137 prepared_trigger_insertion_combinations={prepared_combination_count}");
        println!("TASK1137 refused_retired_mode_count={refused_count}");
    }

    #[test]
    fn task_1137_cover_insertion_is_reported_separately_from_trigger() {
        let trigger_names = InstagramSendTrigger::ALL.map(InstagramSendTrigger::name);
        let insertion_names = InstagramCoverInsertion::ALL.map(InstagramCoverInsertion::name);

        assert_eq!(trigger_names, ["Enter", "Enter x2", "Clipboard"]);
        assert_eq!(insertion_names, ["Insert on send", "Type naturally"]);
        assert!(trigger_names
            .iter()
            .all(|trigger| !insertion_names.contains(trigger)));

        println!("TASK1137 trigger_names={trigger_names:?}");
        println!("TASK1137 cover_insertion_names={insertion_names:?}");
        println!("TASK1137 cover_insertion_reported_separately=true");
    }
}
