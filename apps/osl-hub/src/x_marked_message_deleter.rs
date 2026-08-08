//! X's fill-in of the shared marked-message deleter (TASK 3031).
//!
//! Gate 3029's browser snapshot supplies both the selected row and the
//! provider-observed authorship bit. The command accepts stable row ids, not a
//! caller-provided author, so another account's post cannot be relabeled as
//! belonging to the signed-in account.

use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::services::{XBrowserMachine, XBrowserMessage, XBrowserPlaceKind};
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedDeletionError, SharedMarkedDeletionReport,
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

pub const X_SERVICE_ID: &str = "x";

// Internal identities derived only from X's provider-observed authorship bit.
const SIGNED_IN_X_ACCOUNT: &str = "x:provider:signed-in-account";
const OTHER_X_ACCOUNT: &str = "x:provider:other-account";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XReviewSelection {
    pub place_id: String,
    pub message_id: String,
    pub reviewed: bool,
    pub decision: SharedReviewDecision,
}

impl XReviewSelection {
    pub fn marked(place_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
            reviewed: true,
            decision: SharedReviewDecision::MarkedForDeletion,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum XMarkedDeletionError {
    PlaceNotFound { place_id: String },
    DuplicatePlace { place_id: String },
    MessageNotFound { message_id: String },
    DuplicateMessage { message_id: String },
    Shared(SharedMarkedDeletionError),
}

impl XMarkedDeletionError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlaceNotFound { .. } => "place_not_found",
            Self::DuplicatePlace { .. } => "duplicate_place",
            Self::MessageNotFound { .. } => "message_not_found",
            Self::DuplicateMessage { .. } => "duplicate_message",
            Self::Shared(error) => error.code(),
        }
    }
}

impl std::fmt::Display for XMarkedDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceNotFound { place_id } => write!(f, "X place {place_id} was not found"),
            Self::DuplicatePlace { place_id } => write!(f, "X place {place_id} is duplicated"),
            Self::MessageNotFound { message_id } => {
                write!(f, "X item {message_id} was not found")
            }
            Self::DuplicateMessage { message_id } => {
                write!(f, "X item {message_id} is duplicated")
            }
            Self::Shared(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for XMarkedDeletionError {}

impl From<SharedMarkedDeletionError> for XMarkedDeletionError {
    fn from(error: SharedMarkedDeletionError) -> Self {
        Self::Shared(error)
    }
}

struct XMarkedMessageRemover<'a> {
    browser: &'a mut XBrowserMachine,
}

impl SharedMarkedMessageRemover for XMarkedMessageRemover<'_> {
    fn service_id(&self) -> &str {
        X_SERVICE_ID
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        let matching = self
            .browser
            .messages
            .iter()
            .enumerate()
            .filter(|(_, held)| {
                held.place_id == message.place && held.message_id == message.message_id
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match matching.as_slice() {
            [index] => {
                self.browser.messages.remove(*index);
                Ok(())
            }
            [] => Err(format!(
                "X item {} is not in place {}",
                message.message_id, message.place
            )),
            _ => Err(format!(
                "X item {} is duplicated in place {}",
                message.message_id, message.place
            )),
        }
    }
}

fn selected_message(
    browser: &XBrowserMachine,
    selection: &XReviewSelection,
) -> Result<SharedMarkedMessage, XMarkedDeletionError> {
    let places = browser
        .places
        .iter()
        .filter(|place| place.place_id == selection.place_id)
        .collect::<Vec<_>>();
    let place = match places.as_slice() {
        [place] => *place,
        [] => {
            return Err(XMarkedDeletionError::PlaceNotFound {
                place_id: selection.place_id.clone(),
            })
        }
        _ => {
            return Err(XMarkedDeletionError::DuplicatePlace {
                place_id: selection.place_id.clone(),
            })
        }
    };

    // Exhaustive so a future X surface must make an explicit scope decision.
    match place.kind {
        XBrowserPlaceKind::DirectMessage
        | XBrowserPlaceKind::GroupDirectMessage
        | XBrowserPlaceKind::OwnPostOrReply => {}
    }

    let rows = browser
        .messages
        .iter()
        .filter(|message| {
            message.place_id == selection.place_id && message.message_id == selection.message_id
        })
        .collect::<Vec<&XBrowserMessage>>();
    let row = match rows.as_slice() {
        [row] => *row,
        [] => {
            return Err(XMarkedDeletionError::MessageNotFound {
                message_id: selection.message_id.clone(),
            })
        }
        _ => {
            return Err(XMarkedDeletionError::DuplicateMessage {
                message_id: selection.message_id.clone(),
            })
        }
    };
    let provider_sender = if row.yours {
        SIGNED_IN_X_ACCOUNT
    } else {
        OTHER_X_ACCOUNT
    };

    Ok(SharedMarkedMessage::new(
        X_SERVICE_ID,
        row.message_id.clone(),
        row.place_id.clone(),
        Some(provider_sender.to_owned()),
        selection.reviewed,
        selection.decision,
    ))
}

/// Direct one-row X command, preserving stable shared refusal codes.
pub fn delete_marked_x_message(
    browser: &mut XBrowserMachine,
    selection: &XReviewSelection,
) -> Result<SharedMarkedDeletionReport, XMarkedDeletionError> {
    let message = selected_message(browser, selection)?;
    let mut remover = XMarkedMessageRemover { browser };
    Ok(delete_marked_message(
        &mut remover,
        X_SERVICE_ID,
        Some(SIGNED_IN_X_ACCOUNT.to_owned()),
        message,
    )?)
}

/// Normal Scrub run: resolve X rows, run the shared deleter, and record each
/// deletion outcome through gate 3010's shared record path.
pub fn delete_marked_x_messages_and_record(
    browser: &mut XBrowserMachine,
    selections: impl IntoIterator<Item = XReviewSelection>,
) -> Result<DeletionOutcomeReport, XMarkedDeletionError> {
    let messages = selections
        .into_iter()
        .map(|selection| selected_message(browser, &selection))
        .collect::<Result<Vec<_>, _>>()?;
    let request = SharedMarkedDeletionRequest::new(
        X_SERVICE_ID,
        Some(SIGNED_IN_X_ACCOUNT.to_owned()),
        messages,
    );
    let mut remover = XMarkedMessageRemover { browser };
    Ok(run_shared_marked_deletion_and_record(&mut remover, request))
}
