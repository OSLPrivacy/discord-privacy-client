//! Messenger fill-in for the shared marked-message deleter.
//!
//! Messenger communities have an extra boundary beyond the shared author
//! check: deleting an owned message must not unsend it for other community
//! members.  The service seam therefore exposes account-copy removal as a
//! distinct operation, and community targets always take that path.

use std::collections::BTreeMap;

use crate::messenger_message_reader::{SharedMessengerMessage, MESSENGER_SERVICE_ID};
use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

pub const MESSENGER_COMMUNITY_ACCOUNT_ONLY_DETAIL: &str =
    "community message removed only from the signed-in account's copy";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessengerDeletePlaceKind {
    DirectChat,
    GroupChat,
    Community,
}

impl MessengerDeletePlaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectChat => "direct_chat",
            Self::GroupChat => "group_chat",
            Self::Community => "community",
        }
    }
}

/// The one reviewed Messenger row selected for deletion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerMarkedDeleteTarget {
    pub message: SharedMessengerMessage,
}

impl MessengerMarkedDeleteTarget {
    pub fn new(message: SharedMessengerMessage) -> Self {
        Self { message }
    }
}

/// Browser/service operations used by the Messenger fill-in.
///
/// Keeping the two effects separate makes an accidental community-wide
/// unsend observable in review and in tests.  A production browser adapter
/// implements these with Messenger's corresponding controls.
pub trait MessengerMessageDeleteSurface {
    /// Return the live kind for this place. The deletion request cannot supply
    /// or downgrade this security-relevant fact.
    fn place_kind(&self, place_id: &str) -> Result<MessengerDeletePlaceKind, String>;

    fn remove_for_everyone(&mut self, place_id: &str, message_id: &str) -> Result<(), String>;

    fn remove_signed_in_account_copy(
        &mut self,
        signed_in_account_id: &str,
        place_id: &str,
        message_id: &str,
    ) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessengerDeletionScope {
    Everyone,
    SignedInAccountCopyOnly,
}

impl MessengerDeletionScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::SignedInAccountCopyOnly => "signed_in_account_copy_only",
        }
    }
}

/// Service-effect record paired with gate 3010's shared outcome record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerDeletionActionRecord {
    pub signed_in_account_id: String,
    pub place_id: String,
    pub message_id: String,
    pub place_kind: MessengerDeletePlaceKind,
    pub scope: MessengerDeletionScope,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerDeletionRecord {
    pub outcomes: DeletionOutcomeReport,
    pub service_actions: Vec<MessengerDeletionActionRecord>,
}

#[derive(Clone, Debug)]
struct MessengerDeleteMetadata {
    place_id: String,
}

/// Messenger implementation of the service-neutral removal seam.
pub struct MessengerMarkedMessageRemover<'a, S> {
    surface: &'a mut S,
    signed_in_account_id: String,
    targets: BTreeMap<String, MessengerDeleteMetadata>,
    actions: Vec<MessengerDeletionActionRecord>,
}

impl<'a, S> MessengerMarkedMessageRemover<'a, S>
where
    S: MessengerMessageDeleteSurface,
{
    pub fn one(
        surface: &'a mut S,
        signed_in_account_id: impl Into<String>,
        target: &MessengerMarkedDeleteTarget,
    ) -> Self {
        let mut targets = BTreeMap::new();
        targets.insert(
            target.message.message_id.clone(),
            MessengerDeleteMetadata {
                place_id: target.message.place_id.clone(),
            },
        );
        Self {
            surface,
            signed_in_account_id: signed_in_account_id.into(),
            targets,
            actions: Vec::new(),
        }
    }

    pub fn into_actions(self) -> Vec<MessengerDeletionActionRecord> {
        self.actions
    }
}

impl<S> SharedMarkedMessageRemover for MessengerMarkedMessageRemover<'_, S>
where
    S: MessengerMessageDeleteSurface,
{
    fn service_id(&self) -> &str {
        MESSENGER_SERVICE_ID
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        let target = self
            .targets
            .get(&message.message_id)
            .ok_or_else(|| "Messenger delete target was not observed by the reader".to_owned())?
            .clone();
        if target.place_id != message.place {
            return Err("Messenger delete target changed place after review".to_owned());
        }

        let place_kind = self.surface.place_kind(&target.place_id)?;
        let (scope, detail) = if place_kind == MessengerDeletePlaceKind::Community {
            self.surface.remove_signed_in_account_copy(
                &self.signed_in_account_id,
                &target.place_id,
                &message.message_id,
            )?;
            (
                MessengerDeletionScope::SignedInAccountCopyOnly,
                MESSENGER_COMMUNITY_ACCOUNT_ONLY_DETAIL,
            )
        } else {
            self.surface
                .remove_for_everyone(&target.place_id, &message.message_id)?;
            (
                MessengerDeletionScope::Everyone,
                "message removed for everyone",
            )
        };

        self.actions.push(MessengerDeletionActionRecord {
            signed_in_account_id: self.signed_in_account_id.clone(),
            place_id: target.place_id,
            message_id: message.message_id.clone(),
            place_kind,
            scope,
            detail: detail.to_owned(),
        });
        Ok(())
    }
}

/// Delete one marked Messenger row through the shared owner/mark checks and
/// return both the shared outcome report and Messenger's effect-scope record.
pub fn delete_marked_messenger_message_and_record<S>(
    surface: &mut S,
    signed_in_account_id: &str,
    signed_in_author_id: &str,
    target: MessengerMarkedDeleteTarget,
) -> MessengerDeletionRecord
where
    S: MessengerMessageDeleteSurface,
{
    let request = SharedMarkedDeletionRequest::one(
        MESSENGER_SERVICE_ID,
        Some(signed_in_author_id.to_owned()),
        SharedMarkedMessage::new(
            MESSENGER_SERVICE_ID,
            target.message.message_id.clone(),
            target.message.place_id.clone(),
            Some(target.message.author_id.clone()),
            true,
            SharedReviewDecision::MarkedForDeletion,
        ),
    );
    let mut remover = MessengerMarkedMessageRemover::one(surface, signed_in_account_id, &target);
    let outcomes = run_shared_marked_deletion_and_record(&mut remover, request);
    let service_actions = remover.into_actions();
    MessengerDeletionRecord {
        outcomes,
        service_actions,
    }
}
