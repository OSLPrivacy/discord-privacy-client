//! Read the kind of the Signal conversation found by `signal_surface_finder`.
//!
//! A note-to-self conversation is its own place kind.  In particular, an
//! allowance for a direct message must not make note to self allowed, even if a
//! reader reports the same stable conversation identifier for both surfaces.

use std::collections::HashSet;

use crate::services::{
    read_shared_conversation_places, ConversationPlaceCandidate, ConversationPlaceKind,
    SharedConversationPlace,
};
use crate::signal_surface_finder::SignalSurfaceMatch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalConversationPlaceKind {
    DirectChat,
    Group,
    NoteToSelf,
}

impl SignalConversationPlaceKind {
    pub const fn shared_kind(self) -> ConversationPlaceKind {
        match self {
            Self::DirectChat => ConversationPlaceKind::DirectMessage,
            Self::Group => ConversationPlaceKind::Group,
            Self::NoteToSelf => ConversationPlaceKind::NoteToSelf,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalConversationPlaceCandidate {
    pub place_id: String,
    pub label: String,
    pub kind: SignalConversationPlaceKind,
}

impl SignalConversationPlaceCandidate {
    pub fn new(
        place_id: impl Into<String>,
        label: impl Into<String>,
        kind: SignalConversationPlaceKind,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            label: label.into(),
            kind,
        }
    }

    fn shared_candidate(&self) -> ConversationPlaceCandidate {
        ConversationPlaceCandidate {
            place_id: self.place_id.clone(),
            label: self.label.clone(),
            place_kind: self.kind.shared_kind(),
            server: None,
            channel: None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignalConversationPlaceSnapshot {
    pub signed_in: bool,
    pub places: Vec<SignalConversationPlaceCandidate>,
}

impl SignalConversationPlaceSnapshot {
    pub fn signed_in(places: impl IntoIterator<Item = SignalConversationPlaceCandidate>) -> Self {
        Self {
            signed_in: true,
            places: places.into_iter().collect(),
        }
    }

    pub fn signed_out() -> Self {
        Self::default()
    }
}

pub fn read_signal_conversation_places(
    owner_osl_user_id: &str,
    account_id: &str,
    snapshot: &SignalConversationPlaceSnapshot,
) -> Result<Vec<SharedConversationPlace>, String> {
    let candidates = if snapshot.signed_in {
        ensure_unique_place_ids(&snapshot.places)?;
        snapshot
            .places
            .iter()
            .map(SignalConversationPlaceCandidate::shared_candidate)
            .collect()
    } else {
        Vec::new()
    };

    read_shared_conversation_places(owner_osl_user_id, "signal", account_id, candidates)
}

fn ensure_unique_place_ids(places: &[SignalConversationPlaceCandidate]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(places.len());
    for place in places {
        if !seen.insert(place.place_id.as_str()) {
            return Err("Signal conversation place id is duplicated".to_owned());
        }
    }
    Ok(())
}

pub fn seeded_signal_conversation_places() -> SignalConversationPlaceSnapshot {
    SignalConversationPlaceSnapshot::signed_in([
        SignalConversationPlaceCandidate::new(
            "signal-scrub-s-direct",
            "SCRUB-S",
            SignalConversationPlaceKind::DirectChat,
        ),
        SignalConversationPlaceCandidate::new(
            "signal-scrub-s-group",
            "SCRUB-S Group",
            SignalConversationPlaceKind::Group,
        ),
        SignalConversationPlaceCandidate::new(
            "signal-scrub-s-note-to-self",
            "Note to Self",
            SignalConversationPlaceKind::NoteToSelf,
        ),
    ])
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_signal_ids_are_refused_before_the_shared_reader() {
        let snapshot = SignalConversationPlaceSnapshot::signed_in([
            SignalConversationPlaceCandidate::new(
                "same",
                "One",
                SignalConversationPlaceKind::DirectChat,
            ),
            SignalConversationPlaceCandidate::new(
                "same",
                "Two",
                SignalConversationPlaceKind::Group,
            ),
        ]);
        assert_eq!(
            read_signal_conversation_places("owner", "signal-account", &snapshot),
            Err("Signal conversation place id is duplicated".to_owned())
        );
    }
}
