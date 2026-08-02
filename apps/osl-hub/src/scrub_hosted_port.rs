//! The fixed hosted-webview command boundary for Scrub.
//!
//! A service-host preload receives only these semantic commands.  It owns the
//! provider-specific UI recipe; callers cannot send selectors, scripts, or
//! generic browser operations across this boundary.

use core::fmt;

pub const MAX_HISTORY_SCROLLS: u8 = 10;
pub const MAX_HISTORY_ITEMS: u16 = 500;

/// Bounded, read-only history loading requested from a provider preload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedHistoryRequest {
    pub max_scrolls: u8,
    pub max_items: u16,
    pub before_unix_ms: i64,
}

/// The argument shape supplied with one tagged hosted-port command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedPortCommandArgs {
    None,
    History(BoundedHistoryRequest),
    ItemId(String),
}

/// A checked, semantic command that a hosted provider preload may receive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostedPortCommand {
    ScrollHistory(BoundedHistoryRequest),
    ListOwnItems,
    DeleteOwnItem { item_id: String },
    VerifyGone { item_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedPortCommandParseError {
    UnknownCommand,
    WrongArguments,
    EmptyItemId,
    ItemIdTooLong,
    TooManyScrolls,
    TooManyItems,
    InvalidBeforeTime,
}

impl fmt::Display for HostedPortCommandParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownCommand => "hosted Scrub command is not allowlisted",
            Self::WrongArguments => "hosted Scrub command has the wrong argument shape",
            Self::EmptyItemId => "hosted Scrub item id must not be empty",
            Self::ItemIdTooLong => "hosted Scrub item id is too long",
            Self::TooManyScrolls => "hosted Scrub history request exceeds the scroll limit",
            Self::TooManyItems => "hosted Scrub history request exceeds the item limit",
            Self::InvalidBeforeTime => "hosted Scrub history request has an invalid cutoff",
        })
    }
}

impl std::error::Error for HostedPortCommandParseError {}

/// Parses the untrusted command tag at the native/preload boundary.
///
/// This deliberately has no fallback or passthrough branch.  The host must
/// decode its transport envelope into `HostedPortCommandArgs` before calling
/// this parser; any unknown tag or mismatched payload is refused here.
pub fn parse_hosted_port_command(
    tag: &str,
    args: HostedPortCommandArgs,
) -> Result<HostedPortCommand, HostedPortCommandParseError> {
    match tag {
        "scrollHistory" => match args {
            HostedPortCommandArgs::History(request) => {
                validate_history_request(request)?;
                Ok(HostedPortCommand::ScrollHistory(request))
            }
            _ => Err(HostedPortCommandParseError::WrongArguments),
        },
        "listOwnItems" => match args {
            HostedPortCommandArgs::None => Ok(HostedPortCommand::ListOwnItems),
            _ => Err(HostedPortCommandParseError::WrongArguments),
        },
        "deleteOwnItem" => match args {
            HostedPortCommandArgs::ItemId(item_id) => {
                validate_item_id(&item_id)?;
                Ok(HostedPortCommand::DeleteOwnItem { item_id })
            }
            _ => Err(HostedPortCommandParseError::WrongArguments),
        },
        "verifyGone" => match args {
            HostedPortCommandArgs::ItemId(item_id) => {
                validate_item_id(&item_id)?;
                Ok(HostedPortCommand::VerifyGone { item_id })
            }
            _ => Err(HostedPortCommandParseError::WrongArguments),
        },
        _ => Err(HostedPortCommandParseError::UnknownCommand),
    }
}

fn validate_history_request(
    request: BoundedHistoryRequest,
) -> Result<(), HostedPortCommandParseError> {
    if request.max_scrolls > MAX_HISTORY_SCROLLS {
        return Err(HostedPortCommandParseError::TooManyScrolls);
    }
    if request.max_items > MAX_HISTORY_ITEMS {
        return Err(HostedPortCommandParseError::TooManyItems);
    }
    if request.before_unix_ms < 0 {
        return Err(HostedPortCommandParseError::InvalidBeforeTime);
    }
    Ok(())
}

fn validate_item_id(item_id: &str) -> Result<(), HostedPortCommandParseError> {
    if item_id.is_empty() {
        return Err(HostedPortCommandParseError::EmptyItemId);
    }
    if item_id.len() > 512 {
        return Err(HostedPortCommandParseError::ItemIdTooLong);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_hosted_port_command, BoundedHistoryRequest, HostedPortCommand, HostedPortCommandArgs,
        HostedPortCommandParseError,
    };

    #[test]
    fn scr_h1_only_the_four_semantic_commands_cross_the_boundary() {
        assert_eq!(
            parse_hosted_port_command(
                "scrollHistory",
                HostedPortCommandArgs::History(BoundedHistoryRequest {
                    max_scrolls: 4,
                    max_items: 500,
                    before_unix_ms: 1_700_000_000_000,
                }),
            ),
            Ok(HostedPortCommand::ScrollHistory(BoundedHistoryRequest {
                max_scrolls: 4,
                max_items: 500,
                before_unix_ms: 1_700_000_000_000,
            }))
        );
        assert_eq!(
            parse_hosted_port_command("listOwnItems", HostedPortCommandArgs::None),
            Ok(HostedPortCommand::ListOwnItems)
        );
        assert_eq!(
            parse_hosted_port_command(
                "deleteOwnItem",
                HostedPortCommandArgs::ItemId("item-7".into())
            ),
            Ok(HostedPortCommand::DeleteOwnItem {
                item_id: "item-7".into()
            })
        );
        assert_eq!(
            parse_hosted_port_command("verifyGone", HostedPortCommandArgs::ItemId("item-7".into())),
            Ok(HostedPortCommand::VerifyGone {
                item_id: "item-7".into()
            })
        );
    }

    #[test]
    fn scr_h1_rejects_generic_or_passthrough_runtime_messages() {
        for forbidden in ["eval", "invoke", "selector", "click", "passthrough"] {
            assert_eq!(
                parse_hosted_port_command(forbidden, HostedPortCommandArgs::None),
                Err(HostedPortCommandParseError::UnknownCommand),
                "{forbidden} must not cross the hosted Scrub boundary"
            );
        }
    }
}
