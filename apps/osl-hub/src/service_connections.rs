use crate::website_driver::{
    WebsiteControlKind, WebsiteDriver, WebsiteDriverError, WebsiteNamedControl,
    WebsiteNamedControlRequest, WebsitePage,
};

const EMAIL_COMPOSE_CONTROLS: [WebsiteNamedControlRequest; 2] = [
    WebsiteNamedControlRequest {
        name: "compose box",
        kind: WebsiteControlKind::EditableBox,
    },
    WebsiteNamedControlRequest {
        name: "Send button",
        kind: WebsiteControlKind::Button,
    },
];

pub const GMAIL_SERVICE_ID: &str = "gmail";

pub const GMAIL_CONTROL_NAMES: [&str; 6] = [
    "compose",
    "body",
    "Send",
    "thread view",
    "labels",
    "reading pane or full page",
];

const GMAIL_CONTROL_REQUESTS: [WebsiteNamedControlRequest; 6] = [
    WebsiteNamedControlRequest {
        name: "compose",
        kind: WebsiteControlKind::Button,
    },
    WebsiteNamedControlRequest {
        name: "body",
        kind: WebsiteControlKind::EditableBox,
    },
    WebsiteNamedControlRequest {
        name: "Send",
        kind: WebsiteControlKind::Button,
    },
    WebsiteNamedControlRequest {
        name: "thread view",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
    WebsiteNamedControlRequest {
        name: "labels",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
    WebsiteNamedControlRequest {
        name: "reading pane or full page",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
];

pub const fn gmail_control_mapping() -> &'static [WebsiteNamedControlRequest] {
    &GMAIL_CONTROL_REQUESTS
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EmailComposeControls {
    pub compose_box: WebsiteNamedControl,
    pub send_button: WebsiteNamedControl,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ServiceConnectionError {
    UnsupportedService,
    Driver(WebsiteDriverError),
    MissingComposeBox,
    MissingSendButton,
}

impl From<WebsiteDriverError> for ServiceConnectionError {
    fn from(error: WebsiteDriverError) -> Self {
        Self::Driver(error)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EmailServiceConnection {
    service_id: String,
    account_id: String,
}

impl EmailServiceConnection {
    pub fn new(service_id: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            service_id: service_id.into(),
            account_id: account_id.into(),
        }
    }

    pub fn request_compose_controls(
        &self,
        driver: &impl WebsiteDriver,
        page: &WebsitePage,
    ) -> Result<EmailComposeControls, ServiceConnectionError> {
        if self.service_id != "email" {
            return Err(ServiceConnectionError::UnsupportedService);
        }
        let controls = driver.read_named_controls(page, &EMAIL_COMPOSE_CONTROLS)?;
        let compose_box = controls
            .iter()
            .find(|control| {
                control.name == "compose box" && control.kind == WebsiteControlKind::EditableBox
            })
            .cloned()
            .ok_or(ServiceConnectionError::MissingComposeBox)?;
        let send_button = controls
            .iter()
            .find(|control| {
                control.name == "Send button" && control.kind == WebsiteControlKind::Button
            })
            .cloned()
            .ok_or(ServiceConnectionError::MissingSendButton)?;
        Ok(EmailComposeControls {
            compose_box,
            send_button,
        })
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::website_driver::{WebsiteDriverKind, WebsitePageSnapshot};
    use std::sync::Mutex;
    use url::Url;

    #[derive(Default)]
    struct FixtureDriver {
        requested: Mutex<Vec<WebsiteNamedControlRequest>>,
    }

    impl FixtureDriver {
        fn requested_names(&self) -> Vec<String> {
            self.requested
                .lock()
                .expect("fixture request log is readable")
                .iter()
                .map(|request| request.name.to_owned())
                .collect()
        }
    }

    impl WebsiteDriver for FixtureDriver {
        fn kind(&self) -> WebsiteDriverKind {
            WebsiteDriverKind::FakeTestBrowser
        }

        fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError> {
            Ok(WebsitePage {
                target_id: "task-1205-fixture-page".to_owned(),
                url: url.to_string(),
            })
        }

        fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError> {
            Ok(WebsitePageSnapshot {
                title: "Task 1205 fixture".to_owned(),
                url: page.url.clone(),
            })
        }

        fn read_named_controls(
            &self,
            page: &WebsitePage,
            required: &[WebsiteNamedControlRequest],
        ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
            if page.target_id != "task-1205-fixture-page" {
                return Err(WebsiteDriverError::PageUnavailable);
            }
            self.requested
                .lock()
                .expect("fixture request log is writable")
                .extend_from_slice(required);
            let has_compose = required.iter().any(|request| {
                request.name == "compose box" && request.kind == WebsiteControlKind::EditableBox
            });
            let has_send = required.iter().any(|request| {
                request.name == "Send button" && request.kind == WebsiteControlKind::Button
            });
            if !has_compose {
                return Err(WebsiteDriverError::MissingNamedControl(
                    "compose box".to_owned(),
                ));
            }
            if !has_send {
                return Err(WebsiteDriverError::MissingNamedControl(
                    "Send button".to_owned(),
                ));
            }
            Ok(vec![
                WebsiteNamedControl {
                    name: "compose box".to_owned(),
                    kind: WebsiteControlKind::EditableBox,
                },
                WebsiteNamedControl {
                    name: "Send button".to_owned(),
                    kind: WebsiteControlKind::Button,
                },
            ])
        }
    }

    #[test]
    fn task_1205_sample_email_connection_receives_compose_box_and_send_button_from_fixture() {
        let mut driver = FixtureDriver::default();
        let url = Url::parse("https://mail.google.com/task-1205-fixture")
            .expect("task 1205 fixture URL parses");
        let page = driver
            .find_page(&url)
            .expect("task 1205 fixture page opens");
        let connection = EmailServiceConnection::new("email", "sample-email-account");

        let controls = connection
            .request_compose_controls(&driver, &page)
            .expect("sample email service connection receives fixture controls");
        let requested_names = driver.requested_names();

        println!("TASK1205 service_connection=sample-email");
        println!("TASK1205 account_id={}", connection.account_id());
        println!(
            "TASK1205 driver_requested_control_count={}",
            requested_names.len()
        );
        for name in &requested_names {
            println!("TASK1205 driver_requested_control={name}");
        }
        println!(
            "TASK1205 received_compose_box={}",
            controls.compose_box.name
        );
        println!(
            "TASK1205 received_send_button={}",
            controls.send_button.name
        );

        assert_eq!(requested_names, vec!["compose box", "Send button"]);
        assert_eq!(controls.compose_box.name, "compose box");
        assert_eq!(controls.compose_box.kind, WebsiteControlKind::EditableBox);
        assert_eq!(controls.send_button.name, "Send button");
        assert_eq!(controls.send_button.kind, WebsiteControlKind::Button);
    }

    #[test]
    fn task_1230_gmail_service_connection_mapping_contains_all_six_named_targets() {
        let mapping = gmail_control_mapping();
        let names: Vec<&str> = mapping.iter().map(|request| request.name).collect();
        let expected_names = [
            "compose",
            "body",
            "Send",
            "thread view",
            "labels",
            "reading pane or full page",
        ];

        println!("TASK1230 service_connection={GMAIL_SERVICE_ID}");
        println!("TASK1230 named_target_count={}", names.len());
        for name in &names {
            println!("TASK1230 named_target={name}");
        }

        assert_eq!(names, expected_names);
        assert_eq!(GMAIL_CONTROL_NAMES, expected_names);
        assert_eq!(mapping[0].kind, WebsiteControlKind::Button);
        assert_eq!(mapping[1].kind, WebsiteControlKind::EditableBox);
        assert_eq!(mapping[2].kind, WebsiteControlKind::Button);
        assert_eq!(mapping[3].kind, WebsiteControlKind::VisibleMessageArea);
        assert_eq!(mapping[4].kind, WebsiteControlKind::VisibleMessageArea);
        assert_eq!(mapping[5].kind, WebsiteControlKind::VisibleMessageArea);
    }
}
