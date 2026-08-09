//! WhatsApp fill-in for the shared marked-message deleter.
//!
//! WhatsApp offers two distinct effects: remove a message from the signed-in
//! account's copy, or ask WhatsApp to remove it for everyone.  The safer
//! account-only effect is the default.  An explicit remove-for-everyone choice
//! is honored only while the message is inside WhatsApp's 60-hour window; an
//! older or future-dated row falls back to delete-for-me and records that
//! narrower effect.

use std::collections::BTreeMap;

use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};
use crate::whatsapp_message_reader::{SharedWhatsAppMessage, WHATSAPP_SERVICE_ID};

/// WhatsApp's recorded remove-for-everyone window: 60 hours.
pub const WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS: i64 = 60 * 60 * 60;
pub const WHATSAPP_DELETE_FOR_EVERYONE_FALLBACK_DETAIL: &str =
    "requested delete-for-everyone exceeded WhatsApp's 60-hour limit; fell back to delete-for-me";

/// The effect selected during review. Delete-for-me is deliberately the
/// constructor default; delete-for-everyone requires the explicit constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppDeleteChoice {
    DeleteForMe,
    DeleteForEveryone,
}

impl WhatsAppDeleteChoice {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeleteForMe => "delete_for_me",
            Self::DeleteForEveryone => "delete_for_everyone",
        }
    }
}

/// The effect the WhatsApp surface actually applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppDeletionScope {
    DeleteForMe,
    DeleteForEveryone,
}

impl WhatsAppDeletionScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeleteForMe => "delete_for_me",
            Self::DeleteForEveryone => "delete_for_everyone",
        }
    }
}

/// One reader-produced WhatsApp row selected during review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppMarkedDeleteTarget {
    pub message: SharedWhatsAppMessage,
    pub choice: WhatsAppDeleteChoice,
}

impl WhatsAppMarkedDeleteTarget {
    /// The normal and safest choice: remove only the signed-in account's copy.
    pub fn new(message: SharedWhatsAppMessage) -> Self {
        Self {
            message,
            choice: WhatsAppDeleteChoice::DeleteForMe,
        }
    }

    /// The explicit review choice to ask WhatsApp to remove the message for
    /// everyone, subject to WhatsApp's own time limit at execution time.
    pub fn delete_for_everyone(message: SharedWhatsAppMessage) -> Self {
        Self {
            message,
            choice: WhatsAppDeleteChoice::DeleteForEveryone,
        }
    }
}

/// The two distinct operations supplied by a live WhatsApp adapter.
pub trait WhatsAppMessageDeleteSurface {
    fn delete_for_me(
        &mut self,
        signed_in_account_id: &str,
        place_id: &str,
        message_id: &str,
    ) -> Result<(), String>;

    fn delete_for_everyone(&mut self, place_id: &str, message_id: &str) -> Result<(), String>;
}

/// WhatsApp-specific effect detail paired with task 3010's shared record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppDeletionActionRecord {
    pub signed_in_account_id: String,
    pub place_id: String,
    pub message_id: String,
    pub requested_scope: WhatsAppDeleteChoice,
    pub scope: WhatsAppDeletionScope,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppDeletionRecord {
    pub outcomes: DeletionOutcomeReport,
    pub service_actions: Vec<WhatsAppDeletionActionRecord>,
}

#[derive(Clone, Debug)]
struct WhatsAppDeleteMetadata {
    place_id: String,
    sent_at_unix_seconds: i64,
    choice: WhatsAppDeleteChoice,
}

/// WhatsApp implementation of the service-neutral removal seam.
pub struct WhatsAppMarkedMessageRemover<'a, S> {
    surface: &'a mut S,
    signed_in_account_id: String,
    now_unix_seconds: i64,
    targets: BTreeMap<String, WhatsAppDeleteMetadata>,
    actions: Vec<WhatsAppDeletionActionRecord>,
}

impl<'a, S> WhatsAppMarkedMessageRemover<'a, S>
where
    S: WhatsAppMessageDeleteSurface,
{
    pub fn one(
        surface: &'a mut S,
        signed_in_account_id: impl Into<String>,
        now_unix_seconds: i64,
        target: &WhatsAppMarkedDeleteTarget,
    ) -> Self {
        let mut targets = BTreeMap::new();
        targets.insert(
            target.message.message_id.clone(),
            WhatsAppDeleteMetadata {
                place_id: target.message.place_id.clone(),
                sent_at_unix_seconds: target.message.time,
                choice: target.choice,
            },
        );
        Self {
            surface,
            signed_in_account_id: signed_in_account_id.into(),
            now_unix_seconds,
            targets,
            actions: Vec::new(),
        }
    }

    pub fn into_actions(self) -> Vec<WhatsAppDeletionActionRecord> {
        self.actions
    }
}

impl<S> SharedMarkedMessageRemover for WhatsAppMarkedMessageRemover<'_, S>
where
    S: WhatsAppMessageDeleteSurface,
{
    fn service_id(&self) -> &str {
        WHATSAPP_SERVICE_ID
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        let target = self
            .targets
            .get(&message.message_id)
            .ok_or_else(|| "WhatsApp delete target was not observed by the reader".to_owned())?
            .clone();
        if target.place_id != message.place {
            return Err("WhatsApp delete target changed place after review".to_owned());
        }

        let age_seconds = self
            .now_unix_seconds
            .checked_sub(target.sent_at_unix_seconds);
        let everyone_is_still_allowed = matches!(
            age_seconds,
            Some(age) if (0..=WHATSAPP_DELETE_FOR_EVERYONE_LIMIT_SECONDS).contains(&age)
        );

        let (scope, detail) = match target.choice {
            WhatsAppDeleteChoice::DeleteForEveryone if everyone_is_still_allowed => {
                self.surface
                    .delete_for_everyone(&target.place_id, &message.message_id)?;
                (
                    WhatsAppDeletionScope::DeleteForEveryone,
                    "message deleted for everyone",
                )
            }
            WhatsAppDeleteChoice::DeleteForEveryone => {
                self.surface.delete_for_me(
                    &self.signed_in_account_id,
                    &target.place_id,
                    &message.message_id,
                )?;
                (
                    WhatsAppDeletionScope::DeleteForMe,
                    WHATSAPP_DELETE_FOR_EVERYONE_FALLBACK_DETAIL,
                )
            }
            WhatsAppDeleteChoice::DeleteForMe => {
                self.surface.delete_for_me(
                    &self.signed_in_account_id,
                    &target.place_id,
                    &message.message_id,
                )?;
                (WhatsAppDeletionScope::DeleteForMe, "message deleted for me")
            }
        };

        self.actions.push(WhatsAppDeletionActionRecord {
            signed_in_account_id: self.signed_in_account_id.clone(),
            place_id: target.place_id,
            message_id: message.message_id.clone(),
            requested_scope: target.choice,
            scope,
            detail: detail.to_owned(),
        });
        Ok(())
    }
}

/// Delete one reviewed WhatsApp row through the common mark/owner checks and
/// return both the shared outcome and the exact WhatsApp effect that occurred.
pub fn delete_marked_whatsapp_message_and_record<S>(
    surface: &mut S,
    signed_in_account_id: &str,
    signed_in_author_id: &str,
    now_unix_seconds: i64,
    target: WhatsAppMarkedDeleteTarget,
) -> WhatsAppDeletionRecord
where
    S: WhatsAppMessageDeleteSurface,
{
    let request = SharedMarkedDeletionRequest::one(
        WHATSAPP_SERVICE_ID,
        Some(signed_in_author_id.to_owned()),
        SharedMarkedMessage::new(
            WHATSAPP_SERVICE_ID,
            target.message.message_id.clone(),
            target.message.place_id.clone(),
            Some(target.message.author_id.clone()),
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
    );
    let mut remover =
        WhatsAppMarkedMessageRemover::one(surface, signed_in_account_id, now_unix_seconds, &target);
    let outcomes = run_shared_marked_deletion_and_record(&mut remover, request);
    let service_actions = remover.into_actions();
    WhatsAppDeletionRecord {
        outcomes,
        service_actions,
    }
}
