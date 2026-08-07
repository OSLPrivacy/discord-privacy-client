//! AOL Mail fake page target map for the task 1257 email flow.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AolFakePageControl {
    pub name: &'static str,
}

pub const AOL_1257_MARKED_WORDS: &str = "OSL-AOL-1257 words";
pub const AOL_1257_CONTROL_NAMES: [&str; 5] = ["Compose", "Place", "Body", "Readback", "Send"];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AolFakePageError {
    MissingControl(&'static str),
    NoComposedMessage,
    NoPlacedMessage,
    ProtectedControlRemovalRefused(&'static str),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolFakePageConnection {
    controls: Vec<AolFakePageControl>,
    composed_message: Option<String>,
    placed_messages: Vec<String>,
    sent_emails: Vec<String>,
}

impl Default for AolFakePageConnection {
    fn default() -> Self {
        Self::new_email_flow()
    }
}

impl AolFakePageConnection {
    pub fn new_email_flow() -> Self {
        Self {
            controls: AOL_1257_CONTROL_NAMES
                .into_iter()
                .map(|name| AolFakePageControl { name })
                .collect(),
            composed_message: None,
            placed_messages: Vec::new(),
            sent_emails: Vec::new(),
        }
    }

    pub fn control_names(&self) -> Vec<&'static str> {
        self.controls.iter().map(|control| control.name).collect()
    }

    pub fn placed_message_count(&self) -> usize {
        self.placed_messages.len()
    }

    pub fn sent_email_count(&self) -> usize {
        self.sent_emails.len()
    }

    pub fn compose_marked_email(&mut self) -> Result<&str, AolFakePageError> {
        self.require_control("Compose")?;
        self.composed_message = Some(AOL_1257_MARKED_WORDS.to_owned());
        Ok(self
            .composed_message
            .as_deref()
            .expect("composed message was just assigned"))
    }

    pub fn body_marked_words(&self) -> Result<&str, AolFakePageError> {
        self.require_control("Body")?;
        self.composed_message
            .as_deref()
            .ok_or(AolFakePageError::NoComposedMessage)
    }

    pub fn place_composed_message(&mut self) -> Result<&str, AolFakePageError> {
        self.require_control("Place")?;
        self.require_control("Body")?;
        let message = self
            .composed_message
            .clone()
            .ok_or(AolFakePageError::NoComposedMessage)?;
        self.placed_messages.push(message);
        self.readback_placed_message()
    }

    pub fn readback_marked_words(&self) -> Result<&str, AolFakePageError> {
        self.require_control("Readback")?;
        self.readback_placed_message()
    }

    pub fn send_readback_message(&mut self) -> Result<&str, AolFakePageError> {
        self.require_control("Send")?;
        let message = self
            .placed_messages
            .last()
            .cloned()
            .ok_or(AolFakePageError::NoPlacedMessage)?;
        self.sent_emails.push(message);
        Ok(self
            .sent_emails
            .last()
            .map(String::as_str)
            .expect("sent email was just appended"))
    }

    pub fn remove_control(&mut self, name: &'static str) -> Result<(), AolFakePageError> {
        if name == "Body" && self.placed_messages.is_empty() {
            return Err(AolFakePageError::ProtectedControlRemovalRefused("Body"));
        }
        self.controls.retain(|control| control.name != name);
        Ok(())
    }

    fn require_control(&self, name: &'static str) -> Result<(), AolFakePageError> {
        self.controls
            .iter()
            .any(|control| control.name == name)
            .then_some(())
            .ok_or(AolFakePageError::MissingControl(name))
    }

    fn readback_placed_message(&self) -> Result<&str, AolFakePageError> {
        self.placed_messages
            .last()
            .map(String::as_str)
            .ok_or(AolFakePageError::NoPlacedMessage)
    }
}
