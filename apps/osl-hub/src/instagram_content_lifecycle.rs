//! Commands for timers, view-once, and sender-side burns on Instagram content.
//!
//! Instagram does not provide an authority-bearing content id to a caller. The
//! browser snapshot is therefore the only source of a target and of the
//! provider-observed `yours` bit.  A command takes a reviewed locator, resolves
//! it against that snapshot, and refuses ambiguous, absent, or non-owned rows.
//! In particular, no request field can claim authorship for another person's
//! comment or message.

use core::fmt;

use crate::services::{
    InstagramBrowserMachine, InstagramBrowserMessage, InstagramBrowserPlaceKind,
};

/// Instagram's supported maximum timer, aligned with its chat timer policy.
pub const INSTAGRAM_MAX_TIMER_SECONDS: u32 = 24 * 60 * 60;
/// A view-once item is available for one minute unless its caller requests a
/// shorter supported display window.
pub const INSTAGRAM_DEFAULT_VIEW_ONCE_SECONDS: u32 = 60;

/// A reviewed content locator. It deliberately contains no ownership field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramContentSelection {
    pub place_id: String,
    pub message_id: String,
}

impl InstagramContentSelection {
    pub fn new(place_id: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
        }
    }
}

/// The target derived from the signed-in browser snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramOwnedContentTarget {
    pub place_id: String,
    pub message_id: String,
    pub kind: InstagramBrowserPlaceKind,
}

/// The expiry returned after arming a timer or view-once command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramContentExpiry {
    pub target: InstagramOwnedContentTarget,
    pub expires_at: i64,
    pub view_once: bool,
}

/// Which copy a sender-side burn command asks to remove.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstagramBurnSide {
    YourSide,
    TheirSide,
    BothSides,
}

/// The observable result of a sender-side burn command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramBurnResult {
    pub target: InstagramOwnedContentTarget,
    pub side: InstagramBurnSide,
    pub local_deleted: bool,
    pub remote_burn_requested: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstagramContentCommandError {
    PlaceNotFound { place_id: String },
    DuplicatePlace { place_id: String },
    TargetNotFound { message_id: String },
    DuplicateTarget { message_id: String },
    NotYours { message_id: String },
    InvalidLifetime { seconds: u32 },
}

impl InstagramContentCommandError {
    /// Stable command error code for UI/API consumers.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PlaceNotFound { .. } => "place_not_found",
            Self::DuplicatePlace { .. } => "duplicate_place",
            Self::TargetNotFound { .. } => "target_not_found",
            Self::DuplicateTarget { .. } => "duplicate_target",
            Self::NotYours { .. } => "not_yours",
            Self::InvalidLifetime { .. } => "invalid_lifetime",
        }
    }
}

impl fmt::Display for InstagramContentCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PlaceNotFound { place_id } => {
                write!(f, "Instagram place {place_id} was not found")
            }
            Self::DuplicatePlace { place_id } => {
                write!(f, "Instagram place {place_id} is duplicated")
            }
            Self::TargetNotFound { message_id } => {
                write!(f, "Instagram item {message_id} was not found")
            }
            Self::DuplicateTarget { message_id } => {
                write!(f, "Instagram item {message_id} is duplicated")
            }
            Self::NotYours { message_id } => write!(
                f,
                "Instagram item {message_id} is not owned by the signed-in account"
            ),
            Self::InvalidLifetime { seconds } => {
                write!(f, "Instagram lifetime {seconds} seconds is unsupported")
            }
        }
    }
}

impl std::error::Error for InstagramContentCommandError {}

/// Arm an absolute expiry timer for one owned Instagram item.
pub fn cmd_instagram_set_timer(
    browser: &InstagramBrowserMachine,
    selection: &InstagramContentSelection,
    now: i64,
    timer_seconds: u32,
) -> Result<InstagramContentExpiry, InstagramContentCommandError> {
    if timer_seconds == 0 || timer_seconds > INSTAGRAM_MAX_TIMER_SECONDS {
        return Err(InstagramContentCommandError::InvalidLifetime {
            seconds: timer_seconds,
        });
    }
    let target = resolve_owned_target(browser, selection)?;
    Ok(InstagramContentExpiry {
        target,
        expires_at: now.saturating_add(i64::from(timer_seconds)),
        view_once: false,
    })
}

/// Arm a view-once expiry for one owned Instagram item.
pub fn cmd_instagram_set_view_once(
    browser: &InstagramBrowserMachine,
    selection: &InstagramContentSelection,
    now: i64,
    display_seconds: u32,
) -> Result<InstagramContentExpiry, InstagramContentCommandError> {
    if display_seconds == 0 || display_seconds > INSTAGRAM_MAX_TIMER_SECONDS {
        return Err(InstagramContentCommandError::InvalidLifetime {
            seconds: display_seconds,
        });
    }
    let target = resolve_owned_target(browser, selection)?;
    Ok(InstagramContentExpiry {
        target,
        expires_at: now.saturating_add(i64::from(display_seconds)),
        view_once: true,
    })
}

/// Remove only this device's copy of one owned Instagram item.
pub fn cmd_instagram_burn_your_side(
    browser: &mut InstagramBrowserMachine,
    selection: &InstagramContentSelection,
) -> Result<InstagramBurnResult, InstagramContentCommandError> {
    burn_owned(browser, selection, InstagramBurnSide::YourSide)
}

/// Request deletion of the recipient copy of one owned Instagram item.
///
/// This leaves the local browser snapshot unchanged; the returned result is
/// the authority-limited remote request a provider adapter must perform.
pub fn cmd_instagram_burn_their_side(
    browser: &mut InstagramBrowserMachine,
    selection: &InstagramContentSelection,
) -> Result<InstagramBurnResult, InstagramContentCommandError> {
    burn_owned(browser, selection, InstagramBurnSide::TheirSide)
}

/// Remove this device's copy and request deletion of the recipient copy of an
/// owned Instagram item.
pub fn cmd_instagram_burn_both_sides(
    browser: &mut InstagramBrowserMachine,
    selection: &InstagramContentSelection,
) -> Result<InstagramBurnResult, InstagramContentCommandError> {
    burn_owned(browser, selection, InstagramBurnSide::BothSides)
}

fn resolve_owned_target(
    browser: &InstagramBrowserMachine,
    selection: &InstagramContentSelection,
) -> Result<InstagramOwnedContentTarget, InstagramContentCommandError> {
    let places = browser
        .places
        .iter()
        .filter(|place| place.place_id == selection.place_id)
        .collect::<Vec<_>>();
    let place = match places.as_slice() {
        [place] => *place,
        [] => {
            return Err(InstagramContentCommandError::PlaceNotFound {
                place_id: selection.place_id.clone(),
            })
        }
        _ => {
            return Err(InstagramContentCommandError::DuplicatePlace {
                place_id: selection.place_id.clone(),
            })
        }
    };

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
            return Err(InstagramContentCommandError::TargetNotFound {
                message_id: selection.message_id.clone(),
            })
        }
        _ => {
            return Err(InstagramContentCommandError::DuplicateTarget {
                message_id: selection.message_id.clone(),
            })
        }
    };
    if !row.yours {
        return Err(InstagramContentCommandError::NotYours {
            message_id: row.message_id.clone(),
        });
    }

    Ok(InstagramOwnedContentTarget {
        place_id: row.place_id.clone(),
        message_id: row.message_id.clone(),
        kind: place.kind,
    })
}

fn burn_owned(
    browser: &mut InstagramBrowserMachine,
    selection: &InstagramContentSelection,
    side: InstagramBurnSide,
) -> Result<InstagramBurnResult, InstagramContentCommandError> {
    let target = resolve_owned_target(browser, selection)?;
    let remove_local = matches!(
        side,
        InstagramBurnSide::YourSide | InstagramBurnSide::BothSides
    );
    if remove_local {
        let index = browser
            .messages
            .iter()
            .position(|row| row.place_id == target.place_id && row.message_id == target.message_id)
            .expect("owned target was resolved from this browser snapshot");
        browser.messages.remove(index);
    }
    Ok(InstagramBurnResult {
        target,
        side,
        local_deleted: remove_local,
        remote_burn_requested: matches!(
            side,
            InstagramBurnSide::TheirSide | InstagramBurnSide::BothSides
        ),
    })
}
