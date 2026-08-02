//! Independent, label-aware verification for attended IMAP deletion.
//!
//! A successful delete request is not a receipt.  In particular, Gmail labels
//! are folders from IMAP's point of view: removing a message from one label is
//! not evidence that the message has left All Mail or Trash.  This module keeps
//! the verification surface explicit so callers cannot accidentally reuse the
//! mutated label view as their proof.

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapVerificationResult {
    VerifiedGone,
    StillPresent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapSearchResult {
    Present,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapVerificationSurface {
    AllMail,
    Trash,
}

/// A deliberately narrow independent readback interface.  Implementations
/// must resolve the stable message id in the named mailbox; they must not
/// answer from the label/folder that the delete operation just mutated.
pub trait IndependentImapSearch {
    fn find_message(
        &mut self,
        surface: ImapVerificationSurface,
        stable_message_id: &str,
    ) -> Result<ImapSearchResult, ImapVerifyError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImapVerifyError {
    InvalidMessageId,
    Transport,
    AuthenticationChanged,
    AmbiguousMessageId,
}

impl fmt::Display for ImapVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidMessageId => "IMAP verification needs a stable message id",
            Self::Transport => "IMAP verification could not complete",
            Self::AuthenticationChanged => "IMAP authentication changed during verification",
            Self::AmbiguousMessageId => "IMAP message id is ambiguous during verification",
        })
    }
}

impl std::error::Error for ImapVerifyError {}

/// Re-resolve a message on the independent Gmail-compatible surfaces.
///
/// Any error is `Unknown`: a receipt may never turn a failed readback into a
/// deletion claim.  A message in either All Mail or Trash is still present.
pub fn verify_independently(
    search: &mut dyn IndependentImapSearch,
    stable_message_id: &str,
) -> ImapVerificationResult {
    if stable_message_id.trim().is_empty() {
        return ImapVerificationResult::Unknown;
    }
    let all_mail = search.find_message(ImapVerificationSurface::AllMail, stable_message_id);
    let trash = search.find_message(ImapVerificationSurface::Trash, stable_message_id);
    match (all_mail, trash) {
        (Ok(ImapSearchResult::Absent), Ok(ImapSearchResult::Absent)) => {
            ImapVerificationResult::VerifiedGone
        }
        (Ok(ImapSearchResult::Present), _) | (_, Ok(ImapSearchResult::Present)) => {
            ImapVerificationResult::StillPresent
        }
        _ => ImapVerificationResult::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        all_mail: Result<ImapSearchResult, ImapVerifyError>,
        trash: Result<ImapSearchResult, ImapVerifyError>,
    }

    impl IndependentImapSearch for Fixture {
        fn find_message(
            &mut self,
            surface: ImapVerificationSurface,
            _: &str,
        ) -> Result<ImapSearchResult, ImapVerifyError> {
            match surface {
                ImapVerificationSurface::AllMail => self.all_mail,
                ImapVerificationSurface::Trash => self.trash,
            }
        }
    }

    #[test]
    fn scr_i5_label_removal_is_never_a_verified_gone_receipt() {
        // The deleted label is intentionally not a verification surface.  This
        // fixture represents a message removed from that label but still in
        // All Mail, which must remain visibly unresolved/present.
        let mut label_mode = Fixture {
            all_mail: Ok(ImapSearchResult::Present),
            trash: Ok(ImapSearchResult::Absent),
        };
        assert_eq!(
            verify_independently(&mut label_mode, "<message@example>"),
            ImapVerificationResult::StillPresent
        );

        let mut gone = Fixture {
            all_mail: Ok(ImapSearchResult::Absent),
            trash: Ok(ImapSearchResult::Absent),
        };
        assert_eq!(
            verify_independently(&mut gone, "<message@example>"),
            ImapVerificationResult::VerifiedGone
        );

        let mut dropped = Fixture {
            all_mail: Err(ImapVerifyError::Transport),
            trash: Ok(ImapSearchResult::Absent),
        };
        assert_eq!(
            verify_independently(&mut dropped, "<message@example>"),
            ImapVerificationResult::Unknown
        );
    }
}
