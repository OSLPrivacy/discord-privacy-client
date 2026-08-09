//! Read the kind of the Signal conversation found by `signal_surface_finder`.
//!
//! A note-to-self conversation is its own place kind.  In particular, an
//! allowance for a direct message must not make note to self allowed, even if a
//! reader reports the same stable conversation identifier for both surfaces.

use crate::signal_surface_finder::SignalSurfaceMatch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalPlaceKind {
    DirectMessage,
    Group,
    NoteToSelf,
}

impl SignalPlaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::Group => "group",
            Self::NoteToSelf => "note_to_self",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalPlace {
    pub kind: SignalPlaceKind,
    pub stable_conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalDirectMessageAllowance {
    pub stable_conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalPlaceInspection {
    pub kind: SignalPlaceKind,
    pub direct_message_allowed: bool,
}

/// Narrow seam around the native Signal place reader.
pub trait SignalPlaceReader {
    fn read_place(&mut self, surface: &SignalSurfaceMatch) -> Option<SignalPlace>;
}

impl<F> SignalPlaceReader for F
where
    F: FnMut(&SignalSurfaceMatch) -> Option<SignalPlace>,
{
    fn read_place(&mut self, surface: &SignalSurfaceMatch) -> Option<SignalPlace> {
        self(surface)
    }
}

/// Inspect the current Signal place and apply only an exact direct-message
/// allowance to a place that the reader itself classified as a direct message.
/// A missing reader result fails closed.
pub fn inspect_signal_place(
    surface: &SignalSurfaceMatch,
    reader: &mut impl SignalPlaceReader,
    direct_message_allowances: &[SignalDirectMessageAllowance],
) -> Option<SignalPlaceInspection> {
    let place = reader.read_place(surface)?;
    let direct_message_allowed = place.kind == SignalPlaceKind::DirectMessage
        && direct_message_allowances
            .iter()
            .any(|allowance| allowance.stable_conversation_id == place.stable_conversation_id);

    Some(SignalPlaceInspection {
        kind: place.kind,
        direct_message_allowed,
    })
}
