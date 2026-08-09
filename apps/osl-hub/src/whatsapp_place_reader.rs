//! Read the kind of the WhatsApp conversation bound by the native adapter.
//!
//! A broadcast list is its own place kind. In particular, an allowance for a
//! group must not make a broadcast list allowed, even if the native reader
//! reports the same stable conversation identifier for both surfaces.

use std::collections::HashSet;

use crate::native_whatsapp_adapter::DiscoveredWhatsAppPair;
use crate::services::{
    read_shared_conversation_places, ConversationPlaceCandidate, ConversationPlaceKind,
    SharedConversationPlace,
};

/// WhatsApp conversation-list entries exposed as Scrub-capable places.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppConversationPlaceKind {
    DirectChat,
    Group,
    Community,
    BroadcastList,
}

impl WhatsAppConversationPlaceKind {
    pub const fn shared_kind(self) -> ConversationPlaceKind {
        match self {
            Self::DirectChat => ConversationPlaceKind::DirectMessage,
            Self::Group => ConversationPlaceKind::Group,
            Self::Community => ConversationPlaceKind::Community,
            Self::BroadcastList => ConversationPlaceKind::BroadcastList,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppConversationPlaceCandidate {
    pub place_id: String,
    pub label: String,
    pub kind: WhatsAppConversationPlaceKind,
}

impl WhatsAppConversationPlaceCandidate {
    pub fn new(
        place_id: impl Into<String>,
        label: impl Into<String>,
        kind: WhatsAppConversationPlaceKind,
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
pub struct WhatsAppConversationPlaceSnapshot {
    pub signed_in: bool,
    pub places: Vec<WhatsAppConversationPlaceCandidate>,
}

impl WhatsAppConversationPlaceSnapshot {
    pub fn signed_in(places: impl IntoIterator<Item = WhatsAppConversationPlaceCandidate>) -> Self {
        Self {
            signed_in: true,
            places: places.into_iter().collect(),
        }
    }

    pub fn signed_out() -> Self {
        Self::default()
    }
}

pub fn read_whatsapp_conversation_places(
    owner_osl_user_id: &str,
    account_id: &str,
    snapshot: &WhatsAppConversationPlaceSnapshot,
) -> Result<Vec<SharedConversationPlace>, String> {
    let candidates = if snapshot.signed_in {
        ensure_unique_place_ids(&snapshot.places)?;
        snapshot
            .places
            .iter()
            .map(WhatsAppConversationPlaceCandidate::shared_candidate)
            .collect()
    } else {
        Vec::new()
    };

    read_shared_conversation_places(owner_osl_user_id, "whatsapp", account_id, candidates)
}

fn ensure_unique_place_ids(places: &[WhatsAppConversationPlaceCandidate]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(places.len());
    for place in places {
        if !seen.insert(place.place_id.as_str()) {
            return Err("WhatsApp conversation place id is duplicated".to_owned());
        }
    }
    Ok(())
}

pub fn seeded_whatsapp_conversation_places() -> WhatsAppConversationPlaceSnapshot {
    WhatsAppConversationPlaceSnapshot::signed_in([
        WhatsAppConversationPlaceCandidate::new(
            "whatsapp-scrub-w-direct",
            "SCRUB-W",
            WhatsAppConversationPlaceKind::DirectChat,
        ),
        WhatsAppConversationPlaceCandidate::new(
            "whatsapp-scrub-w-group",
            "SCRUB-W Group",
            WhatsAppConversationPlaceKind::Group,
        ),
        WhatsAppConversationPlaceCandidate::new(
            "whatsapp-scrub-w-community",
            "SCRUB-W Community",
            WhatsAppConversationPlaceKind::Community,
        ),
        WhatsAppConversationPlaceCandidate::new(
            "whatsapp-scrub-w-broadcast",
            "SCRUB-W Broadcast",
            WhatsAppConversationPlaceKind::BroadcastList,
        ),
    ])
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WhatsAppPlaceKind {
    DirectMessage,
    GroupChat,
    Channel,
    Community,
    CommunityGroup,
    BroadcastList,
}

impl WhatsAppPlaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::GroupChat => "group_chat",
            Self::Channel => "channel",
            Self::Community => "community",
            Self::CommunityGroup => "community_group",
            Self::BroadcastList => "broadcast_list",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppPlace {
    pub kind: WhatsAppPlaceKind,
    pub stable_conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppGroupAllowance {
    pub stable_conversation_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WhatsAppPlaceInspection {
    pub kind: WhatsAppPlaceKind,
    pub group_allowed: bool,
}

/// Narrow seam around the native WhatsApp place reader.
pub trait WhatsAppPlaceReader {
    fn read_place(&mut self, surface: &DiscoveredWhatsAppPair) -> Option<WhatsAppPlace>;
}

impl<F> WhatsAppPlaceReader for F
where
    F: FnMut(&DiscoveredWhatsAppPair) -> Option<WhatsAppPlace>,
{
    fn read_place(&mut self, surface: &DiscoveredWhatsAppPair) -> Option<WhatsAppPlace> {
        self(surface)
    }
}

/// Inspect the current WhatsApp place and apply only an exact group allowance
/// to a place that the reader itself classified as a group. A missing reader
/// result fails closed.
pub fn inspect_whatsapp_place(
    surface: &DiscoveredWhatsAppPair,
    reader: &mut impl WhatsAppPlaceReader,
    group_allowances: &[WhatsAppGroupAllowance],
) -> Option<WhatsAppPlaceInspection> {
    let place = reader.read_place(surface)?;
    let group_allowed = place.kind == WhatsAppPlaceKind::GroupChat
        && group_allowances
            .iter()
            .any(|allowance| allowance.stable_conversation_id == place.stable_conversation_id);

    Some(WhatsAppPlaceInspection {
        kind: place.kind,
        group_allowed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_whatsapp_ids_are_refused_before_the_shared_reader() {
        let snapshot = WhatsAppConversationPlaceSnapshot::signed_in([
            WhatsAppConversationPlaceCandidate::new(
                "same",
                "One",
                WhatsAppConversationPlaceKind::DirectChat,
            ),
            WhatsAppConversationPlaceCandidate::new(
                "same",
                "Two",
                WhatsAppConversationPlaceKind::Group,
            ),
        ]);
        assert_eq!(
            read_whatsapp_conversation_places("owner", "whatsapp-account", &snapshot),
            Err("WhatsApp conversation place id is duplicated".to_owned())
        );
    }
}
