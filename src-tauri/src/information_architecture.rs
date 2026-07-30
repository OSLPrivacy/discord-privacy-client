//! Fixed desktop information architecture for the trusted OSL shell.
//!
//! These are product destinations, not implementation subsystems. Keep the
//! labels plain and user-facing.

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InformationArchitectureDestination {
    pub id: DestinationId,
    pub label: &'static str,
    pub user_question: &'static str,
    pub primary_action: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DestinationId {
    Home,
    Inbox,
    People,
    Privacy,
    Activity,
    Connections,
}

pub const PRIMARY_DESTINATIONS: [InformationArchitectureDestination; 6] = [
    InformationArchitectureDestination {
        id: DestinationId::Home,
        label: "Home",
        user_question: "Am I protected, and what needs attention?",
        primary_action: "Fix the most important issue",
    },
    InformationArchitectureDestination {
        id: DestinationId::Inbox,
        label: "Inbox",
        user_question: "Where are my conversations?",
        primary_action: "Start a private conversation",
    },
    InformationArchitectureDestination {
        id: DestinationId::People,
        label: "People",
        user_question: "Who do I trust and where do I know them?",
        primary_action: "Add or verify a person",
    },
    InformationArchitectureDestination {
        id: DestinationId::Privacy,
        label: "Privacy",
        user_question: "What will OSL do for me?",
        primary_action: "Review or change protection",
    },
    InformationArchitectureDestination {
        id: DestinationId::Activity,
        label: "Activity",
        user_question: "What did OSL actually do?",
        primary_action: "Review an item needing attention",
    },
    InformationArchitectureDestination {
        id: DestinationId::Connections,
        label: "Connections",
        user_question: "Which accounts and devices are connected?",
        primary_action: "Connect a service",
    },
];

pub fn primary_destinations() -> &'static [InformationArchitectureDestination; 6] {
    &PRIMARY_DESTINATIONS
}

#[cfg(test)]
fn parse_destination_id(id: &str) -> Option<DestinationId> {
    match id {
        "home" => Some(DestinationId::Home),
        "inbox" => Some(DestinationId::Inbox),
        "people" => Some(DestinationId::People),
        "privacy" => Some(DestinationId::Privacy),
        "activity" => Some(DestinationId::Activity),
        "connections" => Some(DestinationId::Connections),
        _ => None,
    }
}

#[cfg(test)]
fn destination_by_id(id: DestinationId) -> Option<&'static InformationArchitectureDestination> {
    PRIMARY_DESTINATIONS
        .iter()
        .find(|destination| destination.id == id)
}

#[tauri::command]
pub fn get_information_architecture_destinations() -> Vec<InformationArchitectureDestination> {
    primary_destinations().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_destinations_are_the_fixed_six_in_sidebar_order() {
        let labels = primary_destinations()
            .iter()
            .map(|destination| destination.label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "Home",
                "Inbox",
                "People",
                "Privacy",
                "Activity",
                "Connections"
            ]
        );
    }

    #[test]
    fn settings_is_not_a_primary_destination() {
        assert_eq!(primary_destinations().len(), 6);
        assert!(primary_destinations()
            .iter()
            .all(|destination| destination.label != "Settings"));
    }

    #[test]
    fn every_destination_has_plain_user_facing_copy() {
        for destination in primary_destinations() {
            assert!(!destination.label.is_empty());
            assert!(!destination.user_question.is_empty());
            assert!(!destination.primary_action.is_empty());

            let copy = format!(
                "{} {} {}",
                destination.label, destination.user_question, destination.primary_action
            )
            .to_ascii_lowercase();
            for hidden_concept in [
                "keyserver",
                "keyservers",
                "ratchet",
                "ratchets",
                "receipt",
                "receipts",
                "browser profile",
                "browser profiles",
                "provider adapter",
                "provider adapters",
            ] {
                assert!(!copy.contains(hidden_concept));
            }
        }
    }

    #[test]
    fn absent_destination_lookup_refuses_by_returning_none() {
        assert!(destination_by_id(parse_destination_id("home").expect("home is fixed")).is_some());
        assert!(parse_destination_id("settings").is_none());
        assert!(parse_destination_id("keyserver").is_none());
        assert!(parse_destination_id("").is_none());
    }
}
