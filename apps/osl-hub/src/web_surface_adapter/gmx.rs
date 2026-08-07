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
