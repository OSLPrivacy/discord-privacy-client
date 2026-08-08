//! The shared mail owner check (TASK 3044).
//!
//! A visible mail message counts as the signed-in account's only when its
//! readable sender address matches the signed-in address. A sender address that
//! cannot be read fails closed rather than being guessed.
//!
//! This lived in `service_connections.rs` and still reads from there through a
//! `pub use`, so every existing `service_connections::…` path is unchanged. It
//! moved because the shared mail deleter (TASK 3045) needs exactly this check
//! and nothing else that mail-website plumbing pulls in.

use crate::row_who_wrote_it::SharedRowWhoWroteIt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VisibleMailMessage {
    pub message_id: String,
    pub mailbox: String,
    pub sender_address: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MailOwnerCheckError {
    SenderAddressUnreadable,
}

impl MailOwnerCheckError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::SenderAddressUnreadable => "OSL: sender address cannot be read",
        }
    }
}

pub fn mail_message_is_owned_by_signed_in_address(
    signed_in_address: &str,
    message: &VisibleMailMessage,
) -> Result<bool, MailOwnerCheckError> {
    let sender = message
        .sender_address
        .as_deref()
        .map(str::trim)
        .filter(|sender| !sender.is_empty())
        .ok_or(MailOwnerCheckError::SenderAddressUnreadable)?;
    Ok(sender.eq_ignore_ascii_case(signed_in_address.trim()))
}

pub fn mail_message_who_wrote_it(
    signed_in_address: &str,
    message: &VisibleMailMessage,
) -> Result<SharedRowWhoWroteIt, MailOwnerCheckError> {
    Ok(
        if mail_message_is_owned_by_signed_in_address(signed_in_address, message)? {
            SharedRowWhoWroteIt::Yours
        } else {
            SharedRowWhoWroteIt::Theirs
        },
    )
}
