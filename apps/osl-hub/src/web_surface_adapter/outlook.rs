//! Outlook on the web fixed-origin target map.
//!
//! This module is data-only.  It declares the reviewed control targets a web
//! accessibility driver must bind before any Outlook web action can become
//! eligible; it does not perform browser discovery, DOM execution, or native
//! input.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookWebTarget {
    pub name: &'static str,
    pub scope: &'static str,
    pub selector_strategy: &'static str,
    pub selectors: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookWebControlDriver {
    pub driver_id: &'static str,
    pub surface: &'static str,
    pub origin: &'static str,
    pub targets: &'static [OutlookWebTarget],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookWebMappedControl {
    pub name: &'static str,
    pub action: &'static str,
}

pub const OUTLOOK_WEB_ORIGIN: &str = "https://outlook.live.com";

pub const OUTLOOK_WEB_MAPPED_CONTROLS: &[OutlookWebMappedControl] = &[
    OutlookWebMappedControl {
        name: "Place",
        action: "place-cover-message",
    },
    OutlookWebMappedControl {
        name: "Read",
        action: "read-open-cover-message",
    },
    OutlookWebMappedControl {
        name: "Send",
        action: "send-placed-cover-message",
    },
];

pub const OUTLOOK_WEB_TARGETS: &[OutlookWebTarget] = &[
    OutlookWebTarget {
        name: "compose pane",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=dialog name=/new mail|message/i",
            "css=[aria-label*='New mail' i]",
        ],
    },
    OutlookWebTarget {
        name: "body",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=textbox name=/message body|body/i",
            "css=[contenteditable='true'][aria-label*='body' i]",
        ],
    },
    OutlookWebTarget {
        name: "Send",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=button name=/^send$/i",
            "css=button[aria-label^='Send' i]",
        ],
    },
    OutlookWebTarget {
        name: "folders",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=tree name=/folders|folder pane/i",
            "css=[aria-label*='Folders' i]",
        ],
    },
    OutlookWebTarget {
        name: "conversation view",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=list name=/message list|conversation/i",
            "css=[aria-label*='Message list' i]",
        ],
    },
    OutlookWebTarget {
        name: "reading pane",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=region name=/reading pane/i",
            "css=[aria-label*='Reading pane' i]",
        ],
    },
];

pub fn outlook_web_targets() -> &'static [OutlookWebTarget] {
    OUTLOOK_WEB_TARGETS
}

pub fn outlook_web_control_driver() -> OutlookWebControlDriver {
    OutlookWebControlDriver {
        driver_id: "outlook-web-fixed-origin",
        surface: "fixed-official-web-origin",
        origin: OUTLOOK_WEB_ORIGIN,
        targets: outlook_web_targets(),
    }
}

pub fn outlook_web_mapped_controls() -> &'static [OutlookWebMappedControl] {
    OUTLOOK_WEB_MAPPED_CONTROLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlook_web_mapping_contains_all_six_named_targets() {
        let driver = outlook_web_control_driver();
        let names = driver
            .targets
            .iter()
            .map(|target| target.name)
            .collect::<Vec<_>>();

        assert_eq!(driver.driver_id, "outlook-web-fixed-origin");
        assert_eq!(driver.surface, "fixed-official-web-origin");
        assert_eq!(driver.origin, "https://outlook.live.com");
        assert_eq!(
            names,
            vec![
                "compose pane",
                "body",
                "Send",
                "folders",
                "conversation view",
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

        println!("outlook web target count={}", names.len());
        println!("outlook web targets={}", names.join(","));
    }

    #[test]
    fn outlook_web_mapped_controls_name_place_read_and_send() {
        let names = outlook_web_mapped_controls()
            .iter()
            .map(|control| control.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["Place", "Read", "Send"]);
    }
}
