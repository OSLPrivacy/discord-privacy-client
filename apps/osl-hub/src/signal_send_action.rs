//! Signal's direct screen-route send action.
//!
//! Placement is delegated to the provider-neutral shared place-text job. This
//! module owns only the reviewed trigger vocabulary and the final send gate.

use adapter_profile::Capability;

use crate::adapters::CapabilitySet;
use crate::shared_place_text::{
    place_text_through_shared_job, SharedPlaceTextActions, SharedPlaceTextReceipt,
};

/// The complete reviewed trigger vocabulary for Signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalSendTrigger {
    Enter,
    EnterX2,
    Clipboard,
}

impl SignalSendTrigger {
    pub const ALL: [Self; 3] = [Self::Enter, Self::EnterX2, Self::Clipboard];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::EnterX2 => "Enter x2",
            Self::Clipboard => "Clipboard",
        }
    }

    pub fn parse(name: &str) -> Result<Self, SignalSendActionError> {
        match name {
            "Enter" => Ok(Self::Enter),
            "Enter x2" => Ok(Self::EnterX2),
            "Clipboard" => Ok(Self::Clipboard),
            "Manual" | "Instant" | "Match typing" | "Insert on send" | "Type naturally" => {
                Err(SignalSendActionError::RefusedTriggerName(name.to_owned()))
            }
            _ => Err(SignalSendActionError::UnknownTriggerName(name.to_owned())),
        }
    }
}

/// How the prepared cover is inserted, independently of what triggered send.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalCoverInsertion {
    InsertOnSend,
    TypeNaturally,
}

impl SignalCoverInsertion {
    pub const ALL: [Self; 2] = [Self::InsertOnSend, Self::TypeNaturally];

    pub const fn name(self) -> &'static str {
        match self {
            Self::InsertOnSend => "Insert on send",
            Self::TypeNaturally => "Type naturally",
        }
    }
}

/// One already-selected Signal screen route.
///
/// The route performs the shared editor actions, reports its reviewed adapter
/// capabilities, and owns the actual commit operation. A route without
/// `SendProtectedPayload` is never allowed to place or send.
pub trait SelectedSignalSendRoute: SharedPlaceTextActions {
    fn route_name(&self) -> &'static str;
    fn capabilities(&self) -> CapabilitySet;
    fn open_conversation_sent_count(&self) -> Result<usize, String>;
    fn send_open_conversation(&mut self) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalSendActionReceipt {
    pub trigger: SignalSendTrigger,
    pub cover_insertion: SignalCoverInsertion,
    pub selected_route: &'static str,
    pub placement: SharedPlaceTextReceipt,
    pub sent_count_before: usize,
    pub sent_count_after: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignalSendActionError {
    RefusedTriggerName(String),
    UnknownTriggerName(String),
    NoSelectedRoute,
    SelectedRouteCannotSend(String),
    SharedPlacement(String),
    SendFailed(String),
    SentCountUnavailable(String),
    SentCountDidNotAdvance { before: usize, after: usize },
}

impl std::fmt::Display for SignalSendActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RefusedTriggerName(name) => write!(
                formatter,
                "Signal send trigger {name:?} is refused; choose Enter, Enter x2, or Clipboard"
            ),
            Self::UnknownTriggerName(name) => {
                write!(formatter, "unknown Signal send trigger {name:?}")
            }
            Self::NoSelectedRoute => formatter.write_str("Signal route not selected"),
            Self::SelectedRouteCannotSend(route) => write!(
                formatter,
                "selected Signal route {route:?} has no real send capability"
            ),
            Self::SharedPlacement(message) => {
                write!(formatter, "Signal shared placement failed: {message}")
            }
            Self::SendFailed(message) => write!(formatter, "Signal send failed: {message}"),
            Self::SentCountUnavailable(message) => {
                write!(formatter, "Signal sent count unavailable: {message}")
            }
            Self::SentCountDidNotAdvance { before, after } => write!(
                formatter,
                "Signal send did not advance the open conversation sent count exactly once ({before} -> {after})"
            ),
        }
    }
}

impl std::error::Error for SignalSendActionError {}

/// Place one cover through the shared job and commit it through the selected
/// real Signal route. Trigger parsing and route capability checks happen before
/// placement, so refused requests cannot alter the open conversation.
pub fn send_signal_cover_direct_command<R: SelectedSignalSendRoute>(
    selected_route: Option<&mut R>,
    trigger_name: &str,
    cover_insertion: SignalCoverInsertion,
    cover_text: &str,
) -> Result<SignalSendActionReceipt, SignalSendActionError> {
    let trigger = SignalSendTrigger::parse(trigger_name)?;
    let route = selected_route.ok_or(SignalSendActionError::NoSelectedRoute)?;
    let route_name = route.route_name();
    if !route
        .capabilities()
        .contains(&Capability::SendProtectedPayload)
    {
        return Err(SignalSendActionError::SelectedRouteCannotSend(
            route_name.to_owned(),
        ));
    }

    let sent_count_before = route
        .open_conversation_sent_count()
        .map_err(SignalSendActionError::SentCountUnavailable)?;
    let placement = place_text_through_shared_job(route, cover_text)
        .map_err(SignalSendActionError::SharedPlacement)?;
    route
        .send_open_conversation()
        .map_err(SignalSendActionError::SendFailed)?;
    let sent_count_after = route
        .open_conversation_sent_count()
        .map_err(SignalSendActionError::SentCountUnavailable)?;
    if sent_count_after != sent_count_before.saturating_add(1) {
        return Err(SignalSendActionError::SentCountDidNotAdvance {
            before: sent_count_before,
            after: sent_count_after,
        });
    }

    Ok(SignalSendActionReceipt {
        trigger,
        cover_insertion,
        selected_route: route_name,
        placement,
        sent_count_before,
        sent_count_after,
    })
}
