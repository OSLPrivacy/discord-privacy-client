//! Signal Desktop's fill-in of the shared marked-message deleter.
//!
//! Gate 1030 permits Signal to be driven, so this adapter executes a bounded
//! removal instead of returning the plan's fallback refusal. The provider row
//! from gate 3021 supplies the sender id; review selections never get to claim
//! ownership for themselves.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedDeletionError, SharedMarkedDeletionReport,
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};
use crate::signal_message_reader::{SignalOpenScreenMessage, SignalOpenScreenSnapshot};

pub const SIGNAL_SERVICE_ID: &str = "signal";

/// One Signal row and the decision saved by review.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignalReviewSelection {
    pub place_id: String,
    pub message_id: String,
    pub reviewed: bool,
    pub decision: SharedReviewDecision,
}

impl SignalReviewSelection {
    pub fn marked(place_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
            reviewed: true,
            decision: SharedReviewDecision::MarkedForDeletion,
        }
    }
}

/// One exact Signal removal the service fill-in executed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignalRemoval {
    pub place_id: String,
    pub message_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalMarkedDeletionReport {
    pub shared: SharedMarkedDeletionReport,
    pub removals: Vec<SignalRemoval>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalRecordedDeletionReport {
    pub outcomes: DeletionOutcomeReport,
    pub removals: Vec<SignalRemoval>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum SignalMarkedDeletionError {
    PlaceNotOpen { place_id: String },
    MessageNotFound { message_id: String },
    DuplicateMessage { message_id: String },
    DuplicateSelection { message_id: String },
    Shared(SharedMarkedDeletionError),
}

impl SignalMarkedDeletionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlaceNotOpen { .. } => "place_not_open",
            Self::MessageNotFound { .. } => "message_not_found",
            Self::DuplicateMessage { .. } | Self::DuplicateSelection { .. } => "duplicate_message",
            Self::Shared(error) => error.code(),
        }
    }
}

impl std::fmt::Display for SignalMarkedDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceNotOpen { place_id } => {
                write!(f, "Signal place {place_id} is not the open conversation")
            }
            Self::MessageNotFound { message_id } => {
                write!(f, "Signal message {message_id} was not found")
            }
            Self::DuplicateMessage { message_id } => {
                write!(f, "Signal message {message_id} is duplicated")
            }
            Self::DuplicateSelection { message_id } => {
                write!(f, "Signal message {message_id} was selected twice")
            }
            Self::Shared(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SignalMarkedDeletionError {}

impl From<SharedMarkedDeletionError> for SignalMarkedDeletionError {
    fn from(error: SharedMarkedDeletionError) -> Self {
        Self::Shared(error)
    }
}

struct SignalMarkedMessageRemover<'a> {
    screen: &'a mut SignalOpenScreenSnapshot,
    removals: Vec<SignalRemoval>,
}

impl SharedMarkedMessageRemover for SignalMarkedMessageRemover<'_> {
    fn service_id(&self) -> &str {
        SIGNAL_SERVICE_ID
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        let matching = matching_message_indexes(&self.screen.messages, &message.message_id);
        let index = match matching.as_slice() {
            [index] if self.screen.place_id == message.place => *index,
            [] => {
                return Err(format!(
                    "Signal message {} is not in open place {}",
                    message.message_id, message.place
                ))
            }
            [_] => {
                return Err(format!(
                    "Signal open place {} does not match {}",
                    self.screen.place_id, message.place
                ))
            }
            _ => {
                return Err(format!(
                    "Signal message {} is duplicated in open place {}",
                    message.message_id, message.place
                ))
            }
        };

        self.screen.messages.remove(index);
        self.removals.push(SignalRemoval {
            place_id: message.place.clone(),
            message_id: message.message_id.clone(),
        });
        Ok(())
    }
}

fn matching_message_indexes(messages: &[SignalOpenScreenMessage], message_id: &str) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.message_id == message_id)
        .map(|(index, _)| index)
        .collect()
}

fn selected_message(
    screen: &SignalOpenScreenSnapshot,
    selection: &SignalReviewSelection,
) -> Result<SharedMarkedMessage, SignalMarkedDeletionError> {
    if selection.place_id != screen.place_id {
        return Err(SignalMarkedDeletionError::PlaceNotOpen {
            place_id: selection.place_id.clone(),
        });
    }

    let matching = screen
        .messages
        .iter()
        .filter(|message| message.message_id == selection.message_id)
        .collect::<Vec<_>>();
    let row = match matching.as_slice() {
        [row] => *row,
        [] => {
            return Err(SignalMarkedDeletionError::MessageNotFound {
                message_id: selection.message_id.clone(),
            })
        }
        _ => {
            return Err(SignalMarkedDeletionError::DuplicateMessage {
                message_id: selection.message_id.clone(),
            })
        }
    };

    Ok(SharedMarkedMessage::new(
        SIGNAL_SERVICE_ID,
        row.message_id.clone(),
        screen.place_id.clone(),
        Some(row.sender_id.clone()),
        selection.reviewed,
        selection.decision,
    ))
}

/// Direct one-row command. It preserves the shared `not_yours` refusal code.
pub fn delete_marked_signal_message(
    screen: &mut SignalOpenScreenSnapshot,
    signed_in_sender_id: &str,
    selection: &SignalReviewSelection,
) -> Result<SignalMarkedDeletionReport, SignalMarkedDeletionError> {
    let message = selected_message(screen, selection)?;
    let mut remover = SignalMarkedMessageRemover {
        screen,
        removals: Vec::new(),
    };
    let shared = delete_marked_message(
        &mut remover,
        SIGNAL_SERVICE_ID,
        Some(signed_in_sender_id.to_owned()),
        message,
    )?;
    Ok(SignalMarkedDeletionReport {
        shared,
        removals: remover.removals,
    })
}

/// Normal Scrub run: resolve reviewed rows, execute the shared safety boundary,
/// and file each result through gate 3010's deletion outcome record.
pub fn delete_marked_signal_messages_and_record(
    screen: &mut SignalOpenScreenSnapshot,
    signed_in_sender_id: &str,
    selections: impl IntoIterator<Item = SignalReviewSelection>,
) -> Result<SignalRecordedDeletionReport, SignalMarkedDeletionError> {
    let selections = selections.into_iter().collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    for selection in &selections {
        if !seen.insert((selection.place_id.clone(), selection.message_id.clone())) {
            return Err(SignalMarkedDeletionError::DuplicateSelection {
                message_id: selection.message_id.clone(),
            });
        }
    }
    let messages = selections
        .iter()
        .map(|selection| selected_message(screen, selection))
        .collect::<Result<Vec<_>, _>>()?;
    let request = SharedMarkedDeletionRequest::new(
        SIGNAL_SERVICE_ID,
        Some(signed_in_sender_id.to_owned()),
        messages,
    );
    let mut remover = SignalMarkedMessageRemover {
        screen,
        removals: Vec::new(),
    };
    let outcomes = run_shared_marked_deletion_and_record(&mut remover, request);
    Ok(SignalRecordedDeletionReport {
        outcomes,
        removals: remover.removals,
    })
}
