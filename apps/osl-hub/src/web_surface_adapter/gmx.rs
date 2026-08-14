//! GMX Mail fixed-origin target map.
//!
//! This module is data-only. It declares the reviewed control targets a web
//! accessibility driver must bind before any GMX web action can become
//! eligible; it does not perform browser discovery, DOM execution, or native
//! input.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GmxWebTarget {
    pub name: &'static str,
    pub scope: &'static str,
    pub selector_strategy: &'static str,
    pub selectors: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct GmxWebControlDriver {
    pub driver_id: &'static str,
    pub surface: &'static str,
    pub origin: &'static str,
    pub targets: &'static [GmxWebTarget],
}

pub const GMX_WEB_ORIGIN: &str = "https://www.gmx.com";

pub const GMX_WEB_TARGETS: &[GmxWebTarget] = &[
    GmxWebTarget {
        name: "compose",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=button name=/compose|write e-mail|new email/i",
            "css=[aria-label*='Compose' i]",
        ],
    },
    GmxWebTarget {
        name: "body",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=textbox name=/message body|body|mail text/i",
            "css=[contenteditable='true'][aria-label*='body' i]",
        ],
    },
    GmxWebTarget {
        name: "Send",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=button name=/^send$/i",
            "css=button[aria-label^='Send' i]",
        ],
    },
    GmxWebTarget {
        name: "folders",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=tree name=/folders|folder/i",
            "css=[aria-label*='Folders' i]",
        ],
    },
    GmxWebTarget {
        name: "thread view",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=list name=/message list|thread|conversation/i",
            "css=[aria-label*='Message list' i]",
        ],
    },
    GmxWebTarget {
        name: "reading pane",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=region name=/reading pane|mail preview|message/i",
            "css=[aria-label*='Reading pane' i]",
        ],
    },
];

pub fn gmx_web_targets() -> &'static [GmxWebTarget] {
    GMX_WEB_TARGETS
}

pub fn gmx_web_control_driver() -> GmxWebControlDriver {
    GmxWebControlDriver {
        driver_id: "gmx-web-fixed-origin",
        surface: "fixed-official-web-origin",
        origin: GMX_WEB_ORIGIN,
        targets: gmx_web_targets(),
    }
}

pub const GMX_1261_MARKED_WORDS: &str = "OSL-GMX-1261 cover message";
pub const GMX_1261_CONTROL_NAMES: [&str; 3] = ["Place", "Read", "Send"];
pub const GMX_1262_MARKED_WORDS: &str = "OSL-GMX-1262 cover message";
pub const GMX_1262_CONTROL_NAMES: [&str; 4] = ["Compose", "Place", "Readback", "Send"];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmxFakePageControl {
    pub name: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum GmxFakePageError {
    MissingControl(&'static str),
    NoComposedMessage,
    NoPlacedMessage,
    ProtectedControlRemovalRefused(&'static str),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GmxFakePageConnection {
    controls: Vec<GmxFakePageControl>,
    composed_message: Option<String>,
    placed_messages: Vec<String>,
    sent_emails: Vec<String>,
}

impl Default for GmxFakePageConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl GmxFakePageConnection {
    pub fn new() -> Self {
        Self {
            controls: GMX_1261_CONTROL_NAMES
                .into_iter()
                .map(|name| GmxFakePageControl { name })
                .collect(),
            composed_message: None,
            placed_messages: Vec::new(),
            sent_emails: Vec::new(),
        }
    }

    pub fn new_email_flow() -> Self {
        Self {
            controls: GMX_1262_CONTROL_NAMES
                .into_iter()
                .map(|name| GmxFakePageControl { name })
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

    pub fn compose_marked_email(&mut self) -> Result<&str, GmxFakePageError> {
        self.require_control("Compose")?;
        self.composed_message = Some(GMX_1262_MARKED_WORDS.to_owned());
        Ok(self
            .composed_message
            .as_deref()
            .expect("composed message was just assigned"))
    }

    pub fn place_composed_message(&mut self) -> Result<&str, GmxFakePageError> {
        self.require_control("Place")?;
        let message = self
            .composed_message
            .clone()
            .ok_or(GmxFakePageError::NoComposedMessage)?;
        self.placed_messages.push(message);
        self.readback_placed_message()
    }

    pub fn readback_marked_words(&self) -> Result<&str, GmxFakePageError> {
        self.require_control("Readback")?;
        self.readback_placed_message()
    }

    pub fn send_readback_message(&mut self) -> Result<&str, GmxFakePageError> {
        self.require_control("Send")?;
        let message = self
            .placed_messages
            .last()
            .cloned()
            .ok_or(GmxFakePageError::NoPlacedMessage)?;
        self.sent_emails.push(message);
        Ok(self
            .sent_emails
            .last()
            .map(String::as_str)
            .expect("sent email was just appended"))
    }

    pub fn place_marked_cover_message(&mut self) -> Result<(), GmxFakePageError> {
        self.require_control("Place")?;
        self.placed_messages.push(GMX_1261_MARKED_WORDS.to_owned());
        Ok(())
    }

    pub fn read_marked_words(&self) -> Result<&str, GmxFakePageError> {
        self.require_control("Read")?;
        self.placed_messages
            .last()
            .map(String::as_str)
            .ok_or(GmxFakePageError::NoPlacedMessage)
    }

    pub fn send_placed_message(&mut self) -> Result<(), GmxFakePageError> {
        self.require_control("Send")?;
        let message = self
            .placed_messages
            .last()
            .cloned()
            .ok_or(GmxFakePageError::NoPlacedMessage)?;
        self.sent_emails.push(message);
        Ok(())
    }

    pub fn remove_control(&mut self, name: &'static str) -> Result<(), GmxFakePageError> {
        if name == "Send" {
            return Err(GmxFakePageError::ProtectedControlRemovalRefused("Send"));
        }
        self.controls.retain(|control| control.name != name);
        Ok(())
    }

    fn require_control(&self, name: &'static str) -> Result<(), GmxFakePageError> {
        self.controls
            .iter()
            .any(|control| control.name == name)
            .then_some(())
            .ok_or(GmxFakePageError::MissingControl(name))
    }

    fn readback_placed_message(&self) -> Result<&str, GmxFakePageError> {
        self.placed_messages
            .last()
            .map(String::as_str)
            .ok_or(GmxFakePageError::NoPlacedMessage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmx_mapping_contains_all_six_named_targets() {
        let driver = gmx_web_control_driver();
        let names = driver
            .targets
            .iter()
            .map(|target| target.name)
            .collect::<Vec<_>>();

        assert_eq!(driver.driver_id, "gmx-web-fixed-origin");
        assert_eq!(driver.surface, "fixed-official-web-origin");
        assert_eq!(driver.origin, "https://www.gmx.com");
        assert_eq!(
            names,
            vec![
                "compose",
                "body",
                "Send",
                "folders",
                "thread view",
                "reading pane",
            ]
        );
        assert!(driver.targets.iter().all(|target| {
            !target.scope.is_empty()
                && target.selector_strategy == "web-accessibility"
                && !target.selectors.is_empty()
                && target
                    .selectors
                    .iter()
                    .all(|selector| selector.starts_with("role=") || selector.starts_with("css="))
        }));

        println!("gmx target count={}", names.len());
        println!("gmx targets={}", names.join(","));
    }
}
