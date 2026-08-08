//! Instagram's fill-in of the shared marked-message deleter (TASK 3035).
//!
//! Gate 3033's browser snapshot is the source of both the row and its
//! provider-observed `yours` signal. Callers name a reviewed row but never
//! provide an author, so they cannot turn another account's comment into one
//! the shared owner check accepts. Gate 3010's recorded batch path is the
//! normal run; the direct form exposes the shared refusal code to commands.

use crate::pro_marked_deletion_outcomes::DeletionOutcomeReport;
use crate::services::{
    InstagramBrowserMachine, InstagramBrowserMessage, InstagramBrowserPlaceKind,
};
use crate::shared_marked_deletion_record::run_shared_marked_deletion_and_record;
use crate::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedDeletionError, SharedMarkedDeletionReport,
    SharedMarkedDeletionRequest, SharedMarkedMessage, SharedMarkedMessageRemover,
    SharedReviewDecision,
};

pub const INSTAGRAM_SERVICE_ID: &str = "instagram";

// These are internal owner-check identities derived from the provider's
// authorship bit. They are never accepted from a command or review payload.
const SIGNED_IN_INSTAGRAM_ACCOUNT: &str = "instagram:provider:signed-in-account";
const OTHER_INSTAGRAM_ACCOUNT: &str = "instagram:provider:other-account";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramReviewSelection {
    pub place_id: String,
    pub message_id: String,
    pub reviewed: bool,
    pub decision: SharedReviewDecision,
}

impl InstagramReviewSelection {
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
pub enum InstagramMarkedDeletionError {
    PlaceNotFound { place_id: String },
    DuplicatePlace { place_id: String },
    MessageNotFound { message_id: String },
    DuplicateMessage { message_id: String },
    Shared(SharedMarkedDeletionError),
}

impl InstagramMarkedDeletionError {
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

impl std::fmt::Display for InstagramMarkedDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlaceNotFound { place_id } => {
                write!(f, "Instagram place {place_id} was not found")
            }
            Self::DuplicatePlace { place_id } => {
                write!(f, "Instagram place {place_id} is duplicated")
            }
            Self::MessageNotFound { message_id } => {
                write!(f, "Instagram item {message_id} was not found")
            }
            Self::DuplicateMessage { message_id } => {
                write!(f, "Instagram item {message_id} is duplicated")
            }
            Self::Shared(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for InstagramMarkedDeletionError {}

impl From<SharedMarkedDeletionError> for InstagramMarkedDeletionError {
    fn from(error: SharedMarkedDeletionError) -> Self {
        Self::Shared(error)
    }
}

struct InstagramMarkedMessageRemover<'a> {
    browser: &'a mut InstagramBrowserMachine,
}

impl SharedMarkedMessageRemover for InstagramMarkedMessageRemover<'_> {
    fn service_id(&self) -> &str {
        INSTAGRAM_SERVICE_ID
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
                "Instagram item {} is not in place {}",
                message.message_id, message.place
            )),
            _ => Err(format!(
                "Instagram item {} is duplicated in place {}",
                message.message_id, message.place
            )),
        }
    }
}

fn selected_message(
    browser: &InstagramBrowserMachine,
    selection: &InstagramReviewSelection,
) -> Result<SharedMarkedMessage, InstagramMarkedDeletionError> {
    let places = browser
        .places
        .iter()
        .filter(|place| place.place_id == selection.place_id)
        .collect::<Vec<_>>();
    let place = match places.as_slice() {
        [place] => *place,
        [] => {
            return Err(InstagramMarkedDeletionError::PlaceNotFound {
                place_id: selection.place_id.clone(),
            })
        }
        _ => {
            return Err(InstagramMarkedDeletionError::DuplicatePlace {
                place_id: selection.place_id.clone(),
            })
        }
    };

    // Exhaustive on purpose: a newly added Instagram surface must make an
    // explicit deletion-scope decision before this module will compile.
    match place.kind {
        InstagramBrowserPlaceKind::DirectMessage
        | InstagramBrowserPlaceKind::GroupDirectMessage
        | InstagramBrowserPlaceKind::OwnPost
        | InstagramBrowserPlaceKind::OwnComment => {}
    }

    let rows = browser
        .messages
        .iter()
        .filter(|message| {
            message.place_id == selection.place_id && message.message_id == selection.message_id
        })
        .collect::<Vec<&InstagramBrowserMessage>>();
    let row = match rows.as_slice() {
        [row] => *row,
        [] => {
            return Err(InstagramMarkedDeletionError::MessageNotFound {
                message_id: selection.message_id.clone(),
            })
        }
        _ => {
            return Err(InstagramMarkedDeletionError::DuplicateMessage {
                message_id: selection.message_id.clone(),
            })
        }
    };
    let provider_sender = if row.yours {
        SIGNED_IN_INSTAGRAM_ACCOUNT
    } else {
        OTHER_INSTAGRAM_ACCOUNT
    };

    Ok(SharedMarkedMessage::new(
        INSTAGRAM_SERVICE_ID,
        row.message_id.clone(),
        row.place_id.clone(),
        Some(provider_sender.to_owned()),
        selection.reviewed,
        selection.decision,
    ))
}

/// Direct one-row command. The exact shared refusal is preserved for UI and
/// command callers that need the stable `not_yours` code.
pub fn delete_marked_instagram_message(
    browser: &mut InstagramBrowserMachine,
    selection: &InstagramReviewSelection,
) -> Result<SharedMarkedDeletionReport, InstagramMarkedDeletionError> {
    let message = selected_message(browser, selection)?;
    let mut remover = InstagramMarkedMessageRemover { browser };
    Ok(delete_marked_message(
        &mut remover,
        INSTAGRAM_SERVICE_ID,
        Some(SIGNED_IN_INSTAGRAM_ACCOUNT.to_owned()),
        message,
    )?)
}

/// Normal Scrub run: resolve every selection against gate 3033's provider
/// snapshot, run gate 3009's shared deleter, and file gate 3010's outcomes.
pub fn delete_marked_instagram_messages_and_record(
    browser: &mut InstagramBrowserMachine,
    selections: impl IntoIterator<Item = InstagramReviewSelection>,
) -> Result<DeletionOutcomeReport, InstagramMarkedDeletionError> {
    let messages = selections
        .into_iter()
        .map(|selection| selected_message(browser, &selection))
        .collect::<Result<Vec<_>, _>>()?;
    let request = SharedMarkedDeletionRequest::new(
        INSTAGRAM_SERVICE_ID,
        Some(SIGNED_IN_INSTAGRAM_ACCOUNT.to_owned()),
        messages,
    );
    let mut remover = InstagramMarkedMessageRemover { browser };
    Ok(run_shared_marked_deletion_and_record(&mut remover, request))
}
