//! Read the kind of the WhatsApp conversation bound by the native adapter.
//!
//! A broadcast list is its own place kind. In particular, an allowance for a
//! group must not make a broadcast list allowed, even if the native reader
//! reports the same stable conversation identifier for both surfaces.

use crate::native_whatsapp_adapter::DiscoveredWhatsAppPair;

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
