//! Clipboard writes for protected-message placement.
//!
//! The private draft is never an argument to this module. Callers must finish
//! encryption/cover generation first, then hand over only the public text that
//! is safe for the host clipboard and its history.

use crate::invite_clipboard;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishedCoverText(String);

impl FinishedCoverText {
    pub fn new(value: String) -> Result<Self, String> {
        if value.trim().is_empty() {
            return Err("protected clipboard cover text is empty".to_owned());
        }
        if value.len() > 2_000 {
            return Err("protected clipboard cover text exceeds the placement limit".to_owned());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Copy a completed protected-message carrier. This function deliberately does
/// not accept a plaintext/draft parameter.
pub fn write_finished_cover_text_to_clipboard(
    cover_text: &FinishedCoverText,
) -> Result<(), String> {
    invite_clipboard::write_desktop_clipboard_text(cover_text.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_cover_text_refuses_empty_clipboard_payloads() {
        assert!(FinishedCoverText::new("".to_owned()).is_err());
        assert!(FinishedCoverText::new(" \n\t ".to_owned()).is_err());
    }

    #[test]
    fn finished_cover_text_bounds_clipboard_payloads_to_one_message() {
        assert!(FinishedCoverText::new("a".repeat(2_000)).is_ok());
        assert!(FinishedCoverText::new("a".repeat(2_001)).is_err());
    }
}
