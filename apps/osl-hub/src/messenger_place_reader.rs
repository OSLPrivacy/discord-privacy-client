//! Messenger's fill-in for the shared conversation-place reader.
//!
//! A browser surface supplies only its currently visible conversation rows.
//! This module turns those observations into the common scrub-place shape; it
//! never opens a conversation or changes the Messenger surface.  The caller
//! must pass the per-account acknowledgement bit.  An unchecked account is a
//! hard privacy boundary: no browser rows are inspected and no places escape.

use std::collections::BTreeSet;

pub const MESSENGER_SERVICE_ID: &str = "messenger";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessengerPlaceKind {
    DirectChat,
    GroupChat,
    Community,
}

impl MessengerPlaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectChat => "direct_chat",
            Self::GroupChat => "group_chat",
            Self::Community => "community",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerBrowserConversation {
    pub conversation_id: String,
    pub label: String,
    pub kind: MessengerPlaceKind,
}

impl MessengerBrowserConversation {
    pub fn new(
        conversation_id: impl Into<String>,
        label: impl Into<String>,
        kind: MessengerPlaceKind,
    ) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            label: label.into(),
            kind,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedMessengerPlace {
    pub service_id: &'static str,
    pub account_id: String,
    pub place_id: String,
    pub label: String,
    pub place_kind: MessengerPlaceKind,
}

/// Read the visible Messenger places for one account.
///
/// `account_is_ticked` is deliberately an explicit input at this seam: the
/// account-selection gate owns persistence, whereas this reader must be safe
/// to exercise on a browser machine without opening that persistence store.
pub fn read_messenger_places_for_scrub(
    account_is_ticked: bool,
    account_id: &str,
    browser_rows: impl IntoIterator<Item = MessengerBrowserConversation>,
) -> Result<Vec<SharedMessengerPlace>, String> {
    validate_text(account_id, "Messenger account id")?;
    if !account_is_ticked {
        return Ok(Vec::new());
    }

    let mut seen = BTreeSet::new();
    browser_rows
        .into_iter()
        .map(|row| {
            validate_text(&row.conversation_id, "Messenger conversation id")?;
            validate_text(&row.label, "Messenger conversation label")?;
            if !seen.insert(row.conversation_id.clone()) {
                return Err("Messenger browser returned a duplicate conversation id".to_owned());
            }
            Ok(SharedMessengerPlace {
                service_id: MESSENGER_SERVICE_ID,
                account_id: account_id.to_owned(),
                place_id: row.conversation_id,
                label: row.label,
                place_kind: row.kind,
            })
        })
        .collect()
}

fn validate_text(value: &str, field: &str) -> Result<(), String> {
    if value.trim() == value
        && !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(format!("{field} is invalid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scrub_rows() -> Vec<MessengerBrowserConversation> {
        vec![
            MessengerBrowserConversation::new(
                "dm-scrub-m",
                "SCRUB-M",
                MessengerPlaceKind::DirectChat,
            ),
            MessengerBrowserConversation::new(
                "group-scrub-m",
                "SCRUB-M group",
                MessengerPlaceKind::GroupChat,
            ),
            MessengerBrowserConversation::new(
                "community-scrub-m",
                "SCRUB-M community",
                MessengerPlaceKind::Community,
            ),
        ]
    }

    #[test]
    fn task_3036_reads_each_supported_messenger_place_when_ticked() {
        let places = read_messenger_places_for_scrub(true, "messenger-scrub", scrub_rows())
            .expect("read ticked Messenger account");
        assert_eq!(places.len(), 3);
        assert!(places
            .iter()
            .any(|place| place.label == "SCRUB-M"
                && place.place_kind == MessengerPlaceKind::DirectChat));
        assert_eq!(
            places
                .iter()
                .map(|place| place.place_kind.as_str())
                .collect::<Vec<_>>(),
            ["direct_chat", "group_chat", "community"]
        );
    }

    #[test]
    fn task_3036_returns_no_messenger_places_when_account_is_unticked() {
        assert!(
            read_messenger_places_for_scrub(false, "messenger-scrub", scrub_rows())
                .expect("unchecked account is a normal empty result")
                .is_empty()
        );
    }
}
