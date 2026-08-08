//! Delivery boundary for one prepared Instagram direct-message cover.
//!
//! This module deliberately transports only the cover.  Opening its protected
//! payload belongs to the established OSL message decoder, not to a provider
//! adapter.  Keeping that split means an Instagram receive job cannot learn or
//! manufacture private words while it moves the visible cover between accounts.

use crate::instagram_send::PreparedInstagramCover;
use core::fmt;

/// One cover headed to a specific Instagram direct-message account.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramDirectMessageDelivery {
    pub message_mark: String,
    pub sender_account_id: String,
    pub recipient_account_id: String,
    pub cover: PreparedInstagramCover,
}

impl InstagramDirectMessageDelivery {
    pub fn new(
        message_mark: impl Into<String>,
        sender_account_id: impl Into<String>,
        recipient_account_id: impl Into<String>,
        cover: PreparedInstagramCover,
    ) -> Self {
        Self {
            message_mark: message_mark.into(),
            sender_account_id: sender_account_id.into(),
            recipient_account_id: recipient_account_id.into(),
            cover,
        }
    }
}

/// The reviewed receiving job that makes an arriving provider cover observable
/// to the signed-in Instagram account.
pub trait InstagramDirectMessageReceivingJob {
    fn receive_direct_message_cover(
        &mut self,
        delivery: InstagramDirectMessageDelivery,
    ) -> Result<(), InstagramDirectMessageReceiveError>;
}

/// The receiver-owned inbox state used by the receiving job.
///
/// It records a self-arrival as a failure and does not make that cover
/// available to the opener.  This is intentionally separate from OSL's local
/// private-message history: seeing a cover is not reading private words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramDirectMessageInbox {
    account_id: String,
    received_covers: Vec<InstagramDirectMessageDelivery>,
    failures: Vec<InstagramDirectMessageReceiveError>,
}

impl InstagramDirectMessageInbox {
    pub fn for_account(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            received_covers: Vec::new(),
            failures: Vec::new(),
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn received_covers(&self) -> &[InstagramDirectMessageDelivery] {
        &self.received_covers
    }

    pub fn failures(&self) -> &[InstagramDirectMessageReceiveError] {
        &self.failures
    }

    pub fn receive(
        &mut self,
        delivery: InstagramDirectMessageDelivery,
    ) -> Result<(), InstagramDirectMessageReceiveError> {
        if delivery.recipient_account_id != self.account_id {
            let failure = InstagramDirectMessageReceiveError::WrongRecipient {
                expected_account_id: self.account_id.clone(),
                received_account_id: delivery.recipient_account_id.clone(),
            };
            self.failures.push(failure.clone());
            return Err(failure);
        }
        if delivery.sender_account_id == self.account_id {
            let failure = InstagramDirectMessageReceiveError::SelfArrival {
                account_id: self.account_id.clone(),
                message_mark: delivery.message_mark,
            };
            self.failures.push(failure.clone());
            return Err(failure);
        }

        self.received_covers.push(delivery);
        Ok(())
    }
}

/// Post a prepared cover to the independently reviewed receiving job.
///
/// A successful return only means the job accepted the cover.  Callers that
/// need delivery proof must read the recipient inbox and then invoke the OSL
/// private-message opener; this prevents a no-op job from looking like a sent
/// and read message.
pub fn dispatch_prepared_instagram_direct_message<J>(
    job: &mut J,
    delivery: InstagramDirectMessageDelivery,
) -> Result<(), InstagramDirectMessageReceiveError>
where
    J: InstagramDirectMessageReceivingJob + ?Sized,
{
    job.receive_direct_message_cover(delivery)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstagramDirectMessageReceiveError {
    WrongRecipient {
        expected_account_id: String,
        received_account_id: String,
    },
    SelfArrival {
        account_id: String,
        message_mark: String,
    },
}

impl fmt::Display for InstagramDirectMessageReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongRecipient {
                expected_account_id,
                received_account_id,
            } => write!(
                f,
                "Instagram direct-message cover was delivered to {received_account_id:?}, expected {expected_account_id:?}"
            ),
            Self::SelfArrival {
                account_id,
                message_mark,
            } => write!(
                f,
                "Instagram direct-message cover {message_mark:?} arrived on its own account {account_id:?}"
            ),
        }
    }
}

impl std::error::Error for InstagramDirectMessageReceiveError {}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_prepared_instagram_direct_message, InstagramDirectMessageDelivery,
        InstagramDirectMessageInbox, InstagramDirectMessageReceiveError,
        InstagramDirectMessageReceivingJob,
    };
    use crate::instagram_send::prepare_instagram_cover;

    struct InboxJob<'a>(&'a mut InstagramDirectMessageInbox);

    impl InstagramDirectMessageReceivingJob for InboxJob<'_> {
        fn receive_direct_message_cover(
            &mut self,
            delivery: InstagramDirectMessageDelivery,
        ) -> Result<(), InstagramDirectMessageReceiveError> {
            self.0.receive(delivery)
        }
    }

    #[test]
    fn a_cover_arriving_on_its_own_account_is_recorded_as_a_failure() {
        let account = "instagram-account-self";
        let cover =
            prepare_instagram_cover("Instant", "DPC0::cover").expect("fixture cover prepares");
        let delivery = InstagramDirectMessageDelivery::new("self-cover", account, account, cover);
        let mut inbox = InstagramDirectMessageInbox::for_account(account);

        let error = dispatch_prepared_instagram_direct_message(&mut InboxJob(&mut inbox), delivery)
            .expect_err("self delivery must be refused");

        assert_eq!(inbox.received_covers().len(), 0);
        assert_eq!(inbox.failures(), &[error.clone()]);
        assert_eq!(
            error,
            InstagramDirectMessageReceiveError::SelfArrival {
                account_id: account.to_owned(),
                message_mark: "self-cover".to_owned(),
            }
        );
    }
}
