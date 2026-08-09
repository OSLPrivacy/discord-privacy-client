//! Exact inspection contract for an approved Signal one-to-one conversation.
//!
//! This is intentionally an observation boundary.  A group can expose a
//! transcript and a composer too, but it is never returned as a direct message.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalFixture {
    Direct,
    Group,
    NoteToSelf,
    UnapprovedDirect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalDirectMessageKind {
    DirectMessage,
}

impl SignalDirectMessageKind {
    pub const fn as_str(self) -> &'static str {
        "direct-message"
    }
}

/// The controls OSL may observe on a selected Signal direct-message surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalDirectMessageControls {
    pub conversation: &'static str,
    pub composer: &'static str,
}

impl SignalDirectMessageControls {
    pub const fn names(self) -> [&'static str; 2] {
        [self.conversation, self.composer]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InspectedSignalDirectMessage {
    pub kind: SignalDirectMessageKind,
    pub controls: SignalDirectMessageControls,
}

const DIRECT_MESSAGE_CONTROLS: SignalDirectMessageControls = SignalDirectMessageControls {
    conversation: "conversation",
    composer: "composer",
};

/// Inspect an already allowed Signal fixture.  The kind check is exact: group,
/// note-to-self, and an otherwise-direct but unapproved fixture all refuse.
pub fn inspect_allowed_signal_direct_message(
    fixture: SignalFixture,
) -> Result<InspectedSignalDirectMessage, &'static str> {
    match fixture {
        SignalFixture::Direct => Ok(InspectedSignalDirectMessage {
            kind: SignalDirectMessageKind::DirectMessage,
            controls: DIRECT_MESSAGE_CONTROLS,
        }),
        SignalFixture::Group => Err("Signal fixture is a group, not a direct-message"),
        SignalFixture::NoteToSelf => Err("Signal fixture is note-to-self, not a direct-message"),
        SignalFixture::UnapprovedDirect => Err("Signal direct-message fixture is not allowed"),
    }
}

pub fn render_signal_direct_message_check(value: &str) -> Result<String, String> {
    let fixture = match value {
        "signal-direct" => SignalFixture::Direct,
        "signal-group" => SignalFixture::Group,
        "signal-note-to-self" => SignalFixture::NoteToSelf,
        "signal-unapproved-direct" => SignalFixture::UnapprovedDirect,
        _ => {
            return Err("usage: signal-direct-message-check <signal-direct|signal-group|signal-note-to-self|signal-unapproved-direct>".to_owned())
        }
    };
    let inspected = inspect_allowed_signal_direct_message(fixture).map_err(str::to_owned)?;
    let controls = inspected.controls.names();
    Ok(format!(
        "TASK1052_KIND={}\nTASK1052_CONTROLS={},{}\nTASK1052_CONTROL_COUNT={}\n",
        inspected.kind.as_str(),
        controls[0],
        controls[1],
        controls.len(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_allowed_direct_fixture_returns_the_direct_message_kind() {
        assert_eq!(
            inspect_allowed_signal_direct_message(SignalFixture::Direct)
                .expect("allowed direct fixture")
                .kind
                .as_str(),
            "direct-message"
        );
        for fixture in [
            SignalFixture::Group,
            SignalFixture::NoteToSelf,
            SignalFixture::UnapprovedDirect,
        ] {
            assert!(inspect_allowed_signal_direct_message(fixture).is_err());
        }
    }
}
