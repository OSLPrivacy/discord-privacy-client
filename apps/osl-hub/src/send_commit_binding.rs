//! The last, fail-closed target check for an external protected-send commit.
//!
//! A reviewed target is not a permission to use whichever conversation happens
//! to be active when a provider write is made.  This module keeps the reviewed
//! provider account, immutable conversation/draft id, complete recipient-id
//! set, and exact cover bytes together until the one irreversible operation.

use std::collections::BTreeSet;

/// The shipping external commit inventory.  Keep this list deliberately
/// explicit: adding a send route requires adding it here and exercising it in
/// the commit-barrier test, rather than inheriting a permissive default.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ShippingCommitRoute {
    DiscordCarrierPlacement,
    TelegramCarrierPlacement,
    SignalCarrierPlacement,
    WhatsAppCarrierPlacement,
    GmailSend,
    GmailReply,
    GmailReplyAll,
    GmailForward,
    OutlookSend,
    OutlookReply,
    OutlookReplyAll,
    OutlookForward,
}

pub const SHIPPING_COMMIT_ROUTES: [ShippingCommitRoute; 12] = [
    ShippingCommitRoute::DiscordCarrierPlacement,
    ShippingCommitRoute::TelegramCarrierPlacement,
    ShippingCommitRoute::SignalCarrierPlacement,
    ShippingCommitRoute::WhatsAppCarrierPlacement,
    ShippingCommitRoute::GmailSend,
    ShippingCommitRoute::GmailReply,
    ShippingCommitRoute::GmailReplyAll,
    ShippingCommitRoute::GmailForward,
    ShippingCommitRoute::OutlookSend,
    ShippingCommitRoute::OutlookReply,
    ShippingCommitRoute::OutlookReplyAll,
    ShippingCommitRoute::OutlookForward,
];

impl ShippingCommitRoute {
    pub const fn label(self) -> &'static str {
        match self {
            Self::DiscordCarrierPlacement => "discord/carrier-placement",
            Self::TelegramCarrierPlacement => "telegram/carrier-placement",
            Self::SignalCarrierPlacement => "signal/carrier-placement",
            Self::WhatsAppCarrierPlacement => "whatsapp/carrier-placement",
            Self::GmailSend => "gmail/send",
            Self::GmailReply => "gmail/reply",
            Self::GmailReplyAll => "gmail/reply-all",
            Self::GmailForward => "gmail/forward",
            Self::OutlookSend => "outlook/send",
            Self::OutlookReply => "outlook/reply",
            Self::OutlookReplyAll => "outlook/reply-all",
            Self::OutlookForward => "outlook/forward",
        }
    }

    pub const fn commit_point(self) -> &'static str {
        match self {
            Self::DiscordCarrierPlacement
            | Self::TelegramCarrierPlacement
            | Self::SignalCarrierPlacement
            | Self::WhatsAppCarrierPlacement => "carrier-placement",
            _ => "provider-send",
        }
    }
}

/// An independently read live provider target.  IDs, rather than visible
/// labels, are commit inputs.  `recipient_ids` contains To, CC and BCC alike.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveCommitTarget {
    pub provider_account_id: String,
    pub immutable_target_id: String,
    pub recipient_ids: BTreeSet<String>,
    pub visible_label: String,
}

impl LiveCommitTarget {
    pub fn new(
        provider_account_id: impl Into<String>,
        immutable_target_id: impl Into<String>,
        recipient_ids: impl IntoIterator<Item = impl Into<String>>,
        visible_label: impl Into<String>,
    ) -> Self {
        Self {
            provider_account_id: provider_account_id.into(),
            immutable_target_id: immutable_target_id.into(),
            recipient_ids: recipient_ids.into_iter().map(Into::into).collect(),
            visible_label: visible_label.into(),
        }
    }
}

/// A non-forgeable-by-accident snapshot of the exact target and bytes the
/// person reviewed.  It has no setters; refresh means prepare a new commit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewedSendCommit {
    route: ShippingCommitRoute,
    target: LiveCommitTarget,
    cover_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SendCommitRefusal {
    EmptyAccount,
    EmptyTarget,
    EmptyRecipients,
    EmptyCover,
    TargetChanged {
        route: ShippingCommitRoute,
        commit_point: &'static str,
    },
}

impl core::fmt::Display for SendCommitRefusal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyAccount => f.write_str("provider account is empty"),
            Self::EmptyTarget => f.write_str("immutable target id is empty"),
            Self::EmptyRecipients => f.write_str("recipient id set is empty"),
            Self::EmptyCover => f.write_str("cover bytes are empty"),
            Self::TargetChanged { route, commit_point } => write!(
                f,
                "{} refused at {} because the live target changed",
                route.label(),
                commit_point
            ),
        }
    }
}

impl std::error::Error for SendCommitRefusal {}

/// Freeze the independently-read target after the last production validation.
/// The caller must call [`commit_reviewed_send`] immediately before its first
/// provider side effect, using a fresh independent provider read.
pub fn prepare_reviewed_send(
    route: ShippingCommitRoute,
    target: LiveCommitTarget,
    cover_bytes: impl AsRef<[u8]>,
) -> Result<ReviewedSendCommit, SendCommitRefusal> {
    if target.provider_account_id.is_empty() {
        return Err(SendCommitRefusal::EmptyAccount);
    }
    if target.immutable_target_id.is_empty() {
        return Err(SendCommitRefusal::EmptyTarget);
    }
    if target.recipient_ids.is_empty() {
        return Err(SendCommitRefusal::EmptyRecipients);
    }
    let cover_bytes = cover_bytes.as_ref();
    if cover_bytes.is_empty() {
        return Err(SendCommitRefusal::EmptyCover);
    }
    Ok(ReviewedSendCommit {
        route,
        target,
        cover_bytes: cover_bytes.to_vec(),
    })
}

impl ReviewedSendCommit {
    pub fn route(&self) -> ShippingCommitRoute {
        self.route
    }

    pub fn target(&self) -> &LiveCommitTarget {
        &self.target
    }

    pub fn cover_bytes(&self) -> &[u8] {
        &self.cover_bytes
    }
}

/// This is the irreversible-send gate.  Compare every target component before
/// invoking `emit`; therefore a stale successful validation cannot emit a
/// placement, mutate a draft, issue a request, or write one provider byte.
pub fn commit_reviewed_send<T>(
    reviewed: &ReviewedSendCommit,
    independently_read_live_target: &LiveCommitTarget,
    emit: impl FnOnce(&LiveCommitTarget, &[u8]) -> T,
) -> Result<T, SendCommitRefusal> {
    if reviewed.target != *independently_read_live_target {
        return Err(SendCommitRefusal::TargetChanged {
            route: reviewed.route,
            commit_point: reviewed.route.commit_point(),
        });
    }
    Ok(emit(&reviewed.target, &reviewed.cover_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> LiveCommitTarget {
        LiveCommitTarget::new("account-a", "thread-a", ["to-a", "cc-a", "bcc-a"], "Same label")
    }

    #[test]
    fn changed_account_target_or_bcc_refuses_before_emission() {
        let expected = target();
        let reviewed = prepare_reviewed_send(
            ShippingCommitRoute::GmailReplyAll,
            expected.clone(),
            b"cover\0bytes",
        )
        .unwrap();
        for changed in [
            LiveCommitTarget::new("account-b", "thread-a", ["to-a", "cc-a", "bcc-a"], "Same label"),
            LiveCommitTarget::new("account-a", "thread-b", ["to-a", "cc-a", "bcc-a"], "Same label"),
            LiveCommitTarget::new("account-a", "thread-a", ["to-a", "cc-a", "bcc-b"], "Same label"),
        ] {
            let mut emissions = 0;
            assert!(matches!(
                commit_reviewed_send(&reviewed, &changed, |_, _| emissions += 1),
                Err(SendCommitRefusal::TargetChanged { .. })
            ));
            assert_eq!(emissions, 0);
        }
        let mut emissions = 0;
        commit_reviewed_send(&reviewed, &expected, |live, bytes| {
            assert_eq!(live, &expected);
            assert_eq!(bytes, b"cover\0bytes");
            emissions += 1;
        })
        .unwrap();
        assert_eq!(emissions, 1);
    }
}
