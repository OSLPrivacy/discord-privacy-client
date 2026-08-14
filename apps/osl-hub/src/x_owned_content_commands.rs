//! X commands for expiring or burning content authored by the signed-in user.
//!
//! X itself does not provide OSL's timer, view-once, or bilateral-burn
//! semantics.  These commands therefore create OSL lifecycle work only after
//! the already-reviewed row authorship says `yours`.  In particular, a caller
//! cannot turn a visible peer row into an OSL deletion target by merely
//! supplying its id.

use core::fmt;
use std::collections::BTreeSet;

use crate::row_who_wrote_it::SharedRowWhoWroteIt;

const MAX_TIMER_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_VIEW_ONCE_SECONDS: u64 = 60;
const MAX_BURN_CONTROL_SECONDS: u64 = 90 * 24 * 60 * 60;

/// The five exact lifecycle commands exposed for a reviewed X row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XOwnedContentCommand {
    Timer,
    ViewOnce,
    BurnYourSide,
    BurnTheirSide,
    BurnBothSides,
}

impl XOwnedContentCommand {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Timer => "timer",
            Self::ViewOnce => "view-once",
            Self::BurnYourSide => "burn-your-side",
            Self::BurnTheirSide => "burn-their-side",
            Self::BurnBothSides => "burn-both-sides",
        }
    }
}

/// A target returned by the reviewed X row reader.  `who_wrote_it` is local
/// evidence, never a string accepted from a renderer command argument.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XOwnedContentTarget {
    pub content_id: String,
    pub who_wrote_it: SharedRowWhoWroteIt,
}

impl XOwnedContentTarget {
    pub fn yours(content_id: impl Into<String>) -> Self {
        Self {
            content_id: content_id.into(),
            who_wrote_it: SharedRowWhoWroteIt::Yours,
        }
    }
}

/// The time-bounded command handed to the lifecycle dispatcher.
///
/// `expires_at_ms` is always an absolute expiry, including view-once and burn
/// controls, so an offline retry has a hard stop and cannot become indefinite.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XOwnedContentCommandReceipt {
    pub command: XOwnedContentCommand,
    pub target_ids: Vec<String>,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XOwnedContentCommandError {
    EmptyTargetSet,
    EmptyTargetId,
    DuplicateTargetId(String),
    TargetIsNotOwned(String),
    InvalidExpirySeconds {
        command: XOwnedContentCommand,
        requested_seconds: u64,
        maximum_seconds: u64,
    },
}

impl XOwnedContentCommandError {
    /// Stable name of the changed request value that caused the refusal.
    ///
    /// Exact whole-day values use the plan-facing `N-days` spelling; all other
    /// lifetimes use `N-seconds`.  Ownership refusals name the unowned target,
    /// so callers never have to infer which visible row was rejected.
    pub fn refusal_name(&self) -> String {
        match self {
            Self::EmptyTargetSet => "empty-target-set".to_owned(),
            Self::EmptyTargetId => "empty-target-id".to_owned(),
            Self::DuplicateTargetId(id) | Self::TargetIsNotOwned(id) => id.clone(),
            Self::InvalidExpirySeconds {
                requested_seconds, ..
            } => lifetime_name(*requested_seconds),
        }
    }
}

impl fmt::Display for XOwnedContentCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTargetSet => f.write_str("X command needs at least one owned target"),
            Self::EmptyTargetId => f.write_str("X command target id must not be empty"),
            Self::DuplicateTargetId(id) => write!(f, "X command target {id:?} was repeated"),
            Self::TargetIsNotOwned(id) => {
                write!(
                    f,
                    "X command target {id:?} is not authored by the signed-in account"
                )
            }
            Self::InvalidExpirySeconds {
                command,
                requested_seconds,
                maximum_seconds,
            } => {
                write!(
                    f,
                    "X {} request {} is refused: expiry must be between 1-second and {}",
                    command.name(),
                    lifetime_name(*requested_seconds),
                    lifetime_name(*maximum_seconds),
                )
            }
        }
    }
}

impl std::error::Error for XOwnedContentCommandError {}

/// Create a timed-deletion command for one or more X rows authored by this
/// account.  The returned expiry is an absolute, retry-safe deadline.
pub fn cmd_x_timer_for_owned_content(
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    lifetime_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    command_for_owned_content(
        XOwnedContentCommand::Timer,
        targets,
        now_ms,
        lifetime_seconds,
    )
}

/// Create a view-once command for X content authored by this account.
///
/// The display lifecycle can use this deadline after the first authenticated
/// open; the receipt is still bounded before an open so a queued control is
/// not retained forever.
pub fn cmd_x_view_once_for_owned_content(
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    display_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    command_for_owned_content(
        XOwnedContentCommand::ViewOnce,
        targets,
        now_ms,
        display_seconds,
    )
}

/// Create the local-side burn command for authored X content.
pub fn cmd_x_burn_your_side_for_owned_content(
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    control_lifetime_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    command_for_owned_content(
        XOwnedContentCommand::BurnYourSide,
        targets,
        now_ms,
        control_lifetime_seconds,
    )
}

/// Create the peer-side burn request for authored X content.
pub fn cmd_x_burn_their_side_for_owned_content(
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    control_lifetime_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    command_for_owned_content(
        XOwnedContentCommand::BurnTheirSide,
        targets,
        now_ms,
        control_lifetime_seconds,
    )
}

/// Create the both-sides burn request for authored X content.
pub fn cmd_x_burn_both_sides_for_owned_content(
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    control_lifetime_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    command_for_owned_content(
        XOwnedContentCommand::BurnBothSides,
        targets,
        now_ms,
        control_lifetime_seconds,
    )
}

fn command_for_owned_content(
    command: XOwnedContentCommand,
    targets: &[XOwnedContentTarget],
    now_ms: u64,
    lifetime_seconds: u64,
) -> Result<XOwnedContentCommandReceipt, XOwnedContentCommandError> {
    let maximum_seconds = match command {
        XOwnedContentCommand::Timer => MAX_TIMER_SECONDS,
        XOwnedContentCommand::ViewOnce => MAX_VIEW_ONCE_SECONDS,
        XOwnedContentCommand::BurnYourSide
        | XOwnedContentCommand::BurnTheirSide
        | XOwnedContentCommand::BurnBothSides => MAX_BURN_CONTROL_SECONDS,
    };
    if !(1..=maximum_seconds).contains(&lifetime_seconds) {
        return Err(XOwnedContentCommandError::InvalidExpirySeconds {
            command,
            requested_seconds: lifetime_seconds,
            maximum_seconds,
        });
    }
    if targets.is_empty() {
        return Err(XOwnedContentCommandError::EmptyTargetSet);
    }

    let mut seen = BTreeSet::new();
    let mut target_ids = Vec::with_capacity(targets.len());
    for target in targets {
        if target.content_id.trim().is_empty() {
            return Err(XOwnedContentCommandError::EmptyTargetId);
        }
        if !seen.insert(target.content_id.as_str()) {
            return Err(XOwnedContentCommandError::DuplicateTargetId(
                target.content_id.clone(),
            ));
        }
        if target.who_wrote_it != SharedRowWhoWroteIt::Yours {
            return Err(XOwnedContentCommandError::TargetIsNotOwned(
                target.content_id.clone(),
            ));
        }
        target_ids.push(target.content_id.clone());
    }

    Ok(XOwnedContentCommandReceipt {
        command,
        target_ids,
        expires_at_ms: now_ms.saturating_add(lifetime_seconds.saturating_mul(1_000)),
    })
}

fn lifetime_name(seconds: u64) -> String {
    const DAY_SECONDS: u64 = 24 * 60 * 60;
    if seconds > 0 && seconds % DAY_SECONDS == 0 {
        format!("{}-days", seconds / DAY_SECONDS)
    } else {
        format!("{seconds}-seconds")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rows_authored_by_the_current_x_account_receive_commands() {
        let own = XOwnedContentTarget::yours("x-row-own");
        let peer = XOwnedContentTarget {
            content_id: "x-row-peer".to_owned(),
            who_wrote_it: SharedRowWhoWroteIt::Theirs,
        };

        let receipt = cmd_x_timer_for_owned_content(&[own], 100, 1).unwrap();
        assert_eq!(receipt.expires_at_ms, 1_100);
        assert_eq!(
            cmd_x_timer_for_owned_content(&[peer], 100, 1),
            Err(XOwnedContentCommandError::TargetIsNotOwned(
                "x-row-peer".to_owned()
            ))
        );
    }
}
