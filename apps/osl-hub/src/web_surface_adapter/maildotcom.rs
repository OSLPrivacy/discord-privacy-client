//! Mail.com fixed-origin target map.
//!
//! This module is data-only. It declares the reviewed control targets a web
//! accessibility driver must bind before any Mail.com web action can become
//! eligible; it does not perform browser discovery, DOM execution, or native
//! input.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MailDotComWebTarget {
    pub name: &'static str,
    pub scope: &'static str,
    pub selector_strategy: &'static str,
    pub selectors: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MailDotComWebControlDriver {
    pub driver_id: &'static str,
    pub surface: &'static str,
    pub origin: &'static str,
    pub targets: &'static [MailDotComWebTarget],
}

pub const MAILDOTCOM_WEB_ORIGIN: &str = "https://www.mail.com";

pub const MAILDOTCOM_WEB_TARGETS: &[MailDotComWebTarget] = &[
    MailDotComWebTarget {
        name: "compose",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=button name=/compose|write e-mail|new email/i",
            "css=[aria-label*='Compose' i]",
        ],
    },
    MailDotComWebTarget {
        name: "body",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=textbox name=/message body|body|mail text/i",
            "css=[contenteditable='true'][aria-label*='body' i]",
        ],
    },
    MailDotComWebTarget {
        name: "Send",
        scope: "compose",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=button name=/^send$/i",
            "css=button[aria-label^='Send' i]",
        ],
    },
    MailDotComWebTarget {
        name: "folders",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=tree name=/folders|folder/i",
            "css=[aria-label*='Folders' i]",
        ],
    },
    MailDotComWebTarget {
        name: "thread view",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=list name=/message list|thread|conversation/i",
            "css=[aria-label*='Message list' i]",
        ],
    },
    MailDotComWebTarget {
        name: "reading pane",
        scope: "mail",
        selector_strategy: "web-accessibility",
        selectors: &[
            "role=region name=/reading pane|mail preview|message/i",
            "css=[aria-label*='Reading pane' i]",
        ],
    },
];

pub fn maildotcom_web_targets() -> &'static [MailDotComWebTarget] {
    MAILDOTCOM_WEB_TARGETS
}

pub fn maildotcom_web_control_driver() -> MailDotComWebControlDriver {
    MailDotComWebControlDriver {
        driver_id: "maildotcom-web-fixed-origin",
        surface: "fixed-official-web-origin",
        origin: MAILDOTCOM_WEB_ORIGIN,
        targets: maildotcom_web_targets(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt;

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct RuntimeControl {
        name: String,
        value: String,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct ComposeRuntime {
        controls: Vec<RuntimeControl>,
        placement_count: usize,
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    enum ComposeRefusal {
        BodyMissing,
        SendMissing,
    }

    impl fmt::Display for ComposeRefusal {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(match self {
                ComposeRefusal::BodyMissing => "Mail.com body missing",
                ComposeRefusal::SendMissing => "Mail.com Send missing",
            })
        }
    }

    impl ComposeRuntime {
        fn from_targets(targets: &[MailDotComWebTarget]) -> Self {
            Self {
                controls: targets
                    .iter()
                    .filter(|target| target.scope == "compose")
                    .map(|target| RuntimeControl {
                        name: target.name.to_owned(),
                        value: String::new(),
                    })
                    .collect(),
                placement_count: 0,
            }
        }

        fn placement_count(&self) -> usize {
            self.placement_count
        }

        fn control_names(&self) -> Vec<&str> {
            self.controls
                .iter()
                .map(|control| control.name.as_str())
                .collect()
        }

        fn read_control(&self, name: &str) -> Option<&str> {
            self.controls
                .iter()
                .find(|control| control.name == name)
                .map(|control| control.value.as_str())
        }

        fn rename_control(&mut self, from: &str, to: &str) -> bool {
            let Some(control) = self
                .controls
                .iter_mut()
                .find(|control| control.name == from)
            else {
                return false;
            };
            control.name = to.to_owned();
            true
        }

        fn place_body(&mut self, carrier: &str) -> Result<usize, ComposeRefusal> {
            if !self.controls.iter().any(|control| control.name == "Send") {
                return Err(ComposeRefusal::SendMissing);
            }
            let Some(body) = self
                .controls
                .iter_mut()
                .find(|control| control.name == "body")
            else {
                return Err(ComposeRefusal::BodyMissing);
            };
            body.value.clear();
            body.value.push_str(carrier);
            self.placement_count += 1;
            Ok(self.placement_count)
        }
    }

    #[test]
    fn maildotcom_mapping_contains_all_six_named_targets() {
        let driver = maildotcom_web_control_driver();
        let names = driver
            .targets
            .iter()
            .map(|target| target.name)
            .collect::<Vec<_>>();

        assert_eq!(driver.driver_id, "maildotcom-web-fixed-origin");
        assert_eq!(driver.surface, "fixed-official-web-origin");
        assert_eq!(driver.origin, "https://www.mail.com");
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

        println!("maildotcom target count={}", names.len());
        println!("maildotcom targets={}", names.join(","));
    }

    #[test]
    fn task_1270a_body_name_break_refuses_without_replacing_body() {
        let mut runtime = ComposeRuntime::from_targets(maildotcom_web_targets());
        let before_count = runtime.placement_count();

        let after_count = runtime
            .place_body("MAPLE-1270")
            .expect("Mail.com body control accepts MAPLE-1270");
        let body_after = runtime
            .read_control("body")
            .expect("Mail.com body remains readable after placement")
            .to_owned();

        let names_before = runtime
            .control_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert!(runtime.rename_control("body", "Missing Body"));
        let names_after = runtime
            .control_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let changed_names = names_before
            .iter()
            .zip(names_after.iter())
            .filter(|(before, after)| before != after)
            .collect::<Vec<_>>();
        assert_eq!(
            changed_names,
            vec![(&"body".to_owned(), &"Missing Body".to_owned())]
        );

        let refusal = runtime
            .place_body("MAPLE-1270-SECOND")
            .expect_err("renamed Mail.com body control must refuse placement");
        let refusal_text = refusal.to_string();
        let body_after_refusal = runtime
            .read_control("Missing Body")
            .expect("renamed Mail.com body still holds the first placement")
            .to_owned();
        let final_count = runtime.placement_count();

        assert_eq!(before_count, 0);
        assert_eq!(after_count, 1);
        assert_eq!(body_after, "MAPLE-1270");
        assert_eq!(refusal, ComposeRefusal::BodyMissing);
        assert_eq!(refusal_text, "Mail.com body missing");
        assert_eq!(body_after_refusal, "MAPLE-1270");
        assert_eq!(final_count, 1);

        println!(
            "TASK1270A before_count={before_count} after_count={after_count} body=\"{body_after}\""
        );
        println!("TASK1270A changed_control_name=body->Missing Body");
        println!("TASK1270A refusal=\"{refusal_text}\"");
        println!("TASK1270A body_after_refusal=\"{body_after_refusal}\" final_count={final_count}");
    }
}
