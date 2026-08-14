//! TASK 3009 build of the shared marked-message deleter.
//!
//! Nothing is re-implemented here: these are the hub crate's own source files,
//! compiled through `#[path]` so the deleter and the TASK 3001 owner check it
//! calls are the exact bytes `apps/osl-hub/src/lib.rs` declares.

#[path = "../../src/attachment_scan.rs"]
pub mod attachment_scan;

#[path = "../../src/privacy_scan.rs"]
pub mod privacy_scan;

#[path = "../../src/shared_marked_message_deleter.rs"]
pub mod shared_marked_message_deleter;

use shared_marked_message_deleter::{SharedMarkedMessage, SharedMarkedMessageRemover};

/// A service fill-in: one place's messages, as a service adapter would hold
/// them, with the removal the shared deleter calls once a row has cleared both
/// checks. This is the half each real service writes for itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePlaceRemover {
    service_id: String,
    place: String,
    messages: Vec<ServicePlaceMessage>,
    removal_calls: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServicePlaceMessage {
    pub message_id: String,
    pub sender: String,
    pub text: String,
}

impl ServicePlaceMessage {
    pub fn new(message_id: &str, sender: &str, text: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            sender: sender.to_owned(),
            text: text.to_owned(),
        }
    }
}

impl ServicePlaceRemover {
    pub fn new(service_id: &str, place: &str, messages: Vec<ServicePlaceMessage>) -> Self {
        Self {
            service_id: service_id.to_owned(),
            place: place.to_owned(),
            messages,
            removal_calls: Vec::new(),
        }
    }

    pub fn place(&self) -> &str {
        &self.place
    }

    pub fn message_ids(&self) -> Vec<String> {
        self.messages
            .iter()
            .map(|message| message.message_id.clone())
            .collect()
    }

    /// Every message id this fill-in was actually asked to remove. The shared
    /// deleter must leave this empty on a refusal.
    pub fn removal_calls(&self) -> &[String] {
        &self.removal_calls
    }

    pub fn holds(&self, message_id: &str) -> bool {
        self.messages
            .iter()
            .any(|message| message.message_id == message_id)
    }
}

impl SharedMarkedMessageRemover for ServicePlaceRemover {
    fn service_id(&self) -> &str {
        &self.service_id
    }

    fn remove_marked_message(&mut self, message: &SharedMarkedMessage) -> Result<(), String> {
        self.removal_calls.push(message.message_id.clone());
        let before = self.messages.len();
        self.messages
            .retain(|held| held.message_id != message.message_id);
        if self.messages.len() == before {
            return Err(format!(
                "{} does not hold message {}",
                self.service_id, message.message_id
            ));
        }
        Ok(())
    }
}
