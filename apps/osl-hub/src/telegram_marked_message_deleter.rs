//! Telegram Desktop's fill-in of the shared marked-message deleter.
//!
//! Gate 3017's snapshot supplies both the selected row and Telegram's
//! provider-observed `yours` signal. Review payloads never supply a sender.
//! Deletion scope is fail-safe: an omitted scope means delete only for the
//! signed-in account, while delete-for-everyone requires an explicit review
//! choice.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::services::{TelegramDesktopMachine, TelegramDesktopMessage, TelegramDesktopPlaceKind};
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedDeletionError, SharedMarkedDeletionReport,
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

pub const TELEGRAM_SERVICE_ID: &str = "telegram";

// Internal owner-check identities derived only from Telegram's observed bit.
const SIGNED_IN_TELEGRAM_ACCOUNT: &str = "telegram:provider:signed-in-account";
const OTHER_TELEGRAM_ACCOUNT: &str = "telegram:provider:other-account";

/// Which Telegram affordance the review explicitly authorized.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelegramDeletionScope {
    #[default]
    DeleteForMe,
    DeleteForEveryone,
}

impl TelegramDeletionScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeleteForMe => "delete_for_me",
            Self::DeleteForEveryone => "delete_for_everyone",
        }
    }
}

/// One Telegram row and the decision saved by review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramReviewSelection {
    pub place_id: String,
    pub message_id: String,
    pub reviewed: bool,
    pub decision: SharedReviewDecision,
    /// Backward-compatible and fail-safe: older reviews omitted this field and
    /// therefore authorize local deletion only.
    #[serde(default)]
    pub deletion_scope: TelegramDeletionScope,
}

impl TelegramReviewSelection {
    pub fn marked(place_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
            reviewed: true,
            decision: SharedReviewDecision::MarkedForDeletion,
            deletion_scope: TelegramDeletionScope::DeleteForMe,
        }
    }

    pub fn marked_for_everyone(place_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            deletion_scope: TelegramDeletionScope::DeleteForEveryone,
            ..Self::marked(place_id, message_id)
        }
    }
}

/// One removal the Telegram fill-in actually executed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramRemoval {
    pub place_id: String,
    pub message_id: String,
    pub scope: TelegramDeletionScope,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramMarkedDeletionReport {
    pub shared: SharedMarkedDeletionReport,
    pub removals: Vec<TelegramRemoval>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramRecordedDeletionReport {
    pub outcomes: DeletionOutcomeReport,
    pub removals: Vec<TelegramRemoval>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum TelegramMarkedDeletionError {
    PlaceNotFound { place_id: String },
    DuplicatePlace { place_id: String },
    MessageNotFound { message_id: String },
    DuplicateMessage { message_id: String },
    DuplicateSelection { message_id: String },
    Shared(SharedMarkedDeletionError),
}

impl TelegramMarkedDeletionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlaceNotFound { .. } => "place_not_found",
            Self::DuplicatePlace { .. } => "duplicate_place",
            Self::MessageNotFound { .. } => "message_not_found",
            Self::DuplicateMessage { .. } | Self::DuplicateSelection { .. } => "duplicate_message",
            Self::Shared(error) => error.code(),
        }
    }
}

impl std::fmt::Display for TelegramMarkedDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceNotFound { place_id } => {
                write!(f, "Telegram place {place_id} was not found")
            }
            Self::DuplicatePlace { place_id } => {
                write!(f, "Telegram place {place_id} is duplicated")
            }
            Self::MessageNotFound { message_id } => {
                write!(f, "Telegram message {message_id} was not found")
            }
            Self::DuplicateMessage { message_id } => {
                write!(f, "Telegram message {message_id} is duplicated")
            }
            Self::DuplicateSelection { message_id } => {
                write!(f, "Telegram message {message_id} was selected twice")
            }
            Self::Shared(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for TelegramMarkedDeletionError {}

impl From<SharedMarkedDeletionError> for TelegramMarkedDeletionError {
    fn from(error: SharedMarkedDeletionError) -> Self {
        Self::Shared(error)
    }
}

type MessageKey = (String, String);

struct TelegramMarkedMessageRemover<'a> {
    desktop: &'a mut TelegramDesktopMachine,
    scopes: BTreeMap<MessageKey, TelegramDeletionScope>,
    removals: Vec<TelegramRemoval>,
}

impl SharedMarkedMessageRemover for TelegramMarkedMessageRemover<'_> {
    fn service_id(&self) -> &str {
        TELEGRAM_SERVICE_ID
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        let matching = self
            .desktop
            .messages
            .iter()
            .enumerate()
            .filter(|(_, held)| {
                held.place_id == message.place && held.message_id == message.message_id
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let index = match matching.as_slice() {
            [index] => *index,
            [] => {
                return Err(format!(
                    "Telegram message {} is not in place {}",
                    message.message_id, message.place
                ))
            }
            _ => {
                return Err(format!(
                    "Telegram message {} is duplicated in place {}",
                    message.message_id, message.place
                ))
            }
        };

        // Missing scope can only narrow the action. It can never silently
        // widen a review to delete-for-everyone.
        let scope = self
            .scopes
            .get(&(message.place.clone(), message.message_id.clone()))
            .copied()
            .unwrap_or_default();
        self.desktop.messages.remove(index);
        self.removals.push(TelegramRemoval {
            place_id: message.place.clone(),
            message_id: message.message_id.clone(),
            scope,
        });
        Ok(())
    }
}

fn selected_message(
    desktop: &TelegramDesktopMachine,
    selection: &TelegramReviewSelection,
) -> Result<SharedMarkedMessage, TelegramMarkedDeletionError> {
    let places = desktop
        .places
        .iter()
        .filter(|place| place.place_id == selection.place_id)
        .collect::<Vec<_>>();
    let place = match places.as_slice() {
        [place] => *place,
        [] => {
            return Err(TelegramMarkedDeletionError::PlaceNotFound {
                place_id: selection.place_id.clone(),
            })
        }
        _ => {
            return Err(TelegramMarkedDeletionError::DuplicatePlace {
                place_id: selection.place_id.clone(),
            })
        }
    };

    // Exhaustive so adding a Telegram place kind requires an explicit
    // deletion decision before the fill-in compiles.
    match place.kind {
        TelegramDesktopPlaceKind::DirectChat
        | TelegramDesktopPlaceKind::Group
        | TelegramDesktopPlaceKind::Channel => {}
    }

    let rows = desktop
        .messages
        .iter()
        .filter(|message| {
            message.place_id == selection.place_id && message.message_id == selection.message_id
        })
        .collect::<Vec<&TelegramDesktopMessage>>();
    let row = match rows.as_slice() {
        [row] => *row,
        [] => {
            return Err(TelegramMarkedDeletionError::MessageNotFound {
                message_id: selection.message_id.clone(),
            })
        }
        _ => {
            return Err(TelegramMarkedDeletionError::DuplicateMessage {
                message_id: selection.message_id.clone(),
            })
        }
    };
    let provider_sender = if row.yours {
        SIGNED_IN_TELEGRAM_ACCOUNT
    } else {
        OTHER_TELEGRAM_ACCOUNT
    };

    Ok(SharedMarkedMessage::new(
        TELEGRAM_SERVICE_ID,
        row.message_id.clone(),
        row.place_id.clone(),
        Some(provider_sender.to_owned()),
        selection.reviewed,
        selection.decision,
    ))
}

fn scope_map(
    selections: &[TelegramReviewSelection],
) -> BTreeMap<MessageKey, TelegramDeletionScope> {
    selections
        .iter()
        .map(|selection| {
            (
                (selection.place_id.clone(), selection.message_id.clone()),
                selection.deletion_scope,
            )
        })
        .collect()
}

/// Direct one-row command. It preserves the shared `not_yours` refusal code.
pub fn delete_marked_telegram_message(
    desktop: &mut TelegramDesktopMachine,
    selection: &TelegramReviewSelection,
) -> Result<TelegramMarkedDeletionReport, TelegramMarkedDeletionError> {
    let message = selected_message(desktop, selection)?;
    let mut remover = TelegramMarkedMessageRemover {
        desktop,
        scopes: scope_map(std::slice::from_ref(selection)),
        removals: Vec::new(),
    };
    let shared = delete_marked_message(
        &mut remover,
        TELEGRAM_SERVICE_ID,
        Some(SIGNED_IN_TELEGRAM_ACCOUNT.to_owned()),
        message,
    )?;
    Ok(TelegramMarkedDeletionReport {
        shared,
        removals: remover.removals,
    })
}

/// Normal Scrub run: resolve selections against gate 3017, run the shared
/// safety boundary, and file gate 3010's outcomes.
pub fn delete_marked_telegram_messages_and_record(
    desktop: &mut TelegramDesktopMachine,
    selections: impl IntoIterator<Item = TelegramReviewSelection>,
) -> Result<TelegramRecordedDeletionReport, TelegramMarkedDeletionError> {
    let selections = selections.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    for selection in &selections {
        if !seen.insert((selection.place_id.clone(), selection.message_id.clone())) {
            return Err(TelegramMarkedDeletionError::DuplicateSelection {
                message_id: selection.message_id.clone(),
            });
        }
    }
    let messages = selections
        .iter()
        .map(|selection| selected_message(desktop, selection))
        .collect::<Result<Vec<_>, _>>()?;
    let request = SharedMarkedDeletionRequest::new(
        TELEGRAM_SERVICE_ID,
        Some(SIGNED_IN_TELEGRAM_ACCOUNT.to_owned()),
        messages,
    );
    let mut remover = TelegramMarkedMessageRemover {
        desktop,
        scopes: scope_map(&selections),
        removals: Vec::new(),
    };
    let outcomes = run_shared_marked_deletion_and_record(&mut remover, request);
    Ok(TelegramRecordedDeletionReport {
        outcomes,
        removals: remover.removals,
    })
}
