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
