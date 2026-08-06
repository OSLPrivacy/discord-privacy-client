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
pub const AOL_SERVICE_ID: &str = "aol";

pub const GMAIL_CONTROL_NAMES: [&str; 6] = [
    "compose",
    "body",
    "Send",
    "thread view",
    "labels",
    "reading pane or full page",
];

pub const AOL_CONTROL_NAMES: [&str; 6] = [
    "compose",
    "body",
    "Send",
    "folders",
    "thread view",
    "reading pane",
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

const AOL_CONTROL_REQUESTS: [WebsiteNamedControlRequest; 6] = [
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
        name: "folders",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
    WebsiteNamedControlRequest {
        name: "thread view",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
    WebsiteNamedControlRequest {
        name: "reading pane",
        kind: WebsiteControlKind::VisibleMessageArea,
    },
];

pub const fn gmail_control_mapping() -> &'static [WebsiteNamedControlRequest] {
    &GMAIL_CONTROL_REQUESTS
}

pub const fn aol_control_mapping() -> &'static [WebsiteNamedControlRequest] {
    &AOL_CONTROL_REQUESTS
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VisibleMailMessage {
    pub message_id: String,
    pub mailbox: String,
    pub sender_address: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MailOwnerCheckError {
    SenderAddressUnreadable,
}

impl MailOwnerCheckError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::SenderAddressUnreadable => "OSL: sender address cannot be read",
        }
    }
}

pub fn mail_message_is_owned_by_signed_in_address(
    signed_in_address: &str,
    message: &VisibleMailMessage,
) -> Result<bool, MailOwnerCheckError> {
    let sender = message
        .sender_address
        .as_deref()
        .map(str::trim)
        .filter(|sender| !sender.is_empty())
        .ok_or(MailOwnerCheckError::SenderAddressUnreadable)?;
    Ok(sender.eq_ignore_ascii_case(signed_in_address.trim()))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServiceControlMappingError {
    MissingNamedTarget(&'static str),
}

pub fn validate_aol_control_mapping(
    mapping: &[WebsiteNamedControlRequest],
) -> Result<(), ServiceControlMappingError> {
    validate_required_control_names(mapping, &AOL_CONTROL_NAMES)
}

fn validate_required_control_names(
    mapping: &[WebsiteNamedControlRequest],
    required_names: &'static [&'static str],
) -> Result<(), ServiceControlMappingError> {
    for required_name in required_names {
        if !mapping.iter().any(|request| request.name == *required_name) {
            return Err(ServiceControlMappingError::MissingNamedTarget(
                *required_name,
            ));
        }
    }
    Ok(())
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

    #[test]
    fn task_1255_aol_mapping_contains_all_six_named_targets_and_refuses_each_omission() {
        let mapping = aol_control_mapping();
        let names: Vec<&str> = mapping.iter().map(|request| request.name).collect();
        let expected_names = [
            "compose",
            "body",
            "Send",
            "folders",
            "thread view",
            "reading pane",
        ];

        println!("TASK1255 service_connection={AOL_SERVICE_ID}");
        println!("TASK1255 named_target_count={}", names.len());
        for name in &names {
            println!("TASK1255 named_target={name}");
        }

        assert_eq!(names, expected_names);
        assert_eq!(AOL_CONTROL_NAMES, expected_names);
        assert_eq!(mapping[0].kind, WebsiteControlKind::Button);
        assert_eq!(mapping[1].kind, WebsiteControlKind::EditableBox);
        assert_eq!(mapping[2].kind, WebsiteControlKind::Button);
        assert_eq!(mapping[3].kind, WebsiteControlKind::VisibleMessageArea);
        assert_eq!(mapping[4].kind, WebsiteControlKind::VisibleMessageArea);
        assert_eq!(mapping[5].kind, WebsiteControlKind::VisibleMessageArea);
        validate_aol_control_mapping(mapping).expect("complete AOL mapping is accepted");

        let mut refused_missing_names = Vec::new();
        for omitted_name in expected_names {
            let candidate: Vec<WebsiteNamedControlRequest> = mapping
                .iter()
                .copied()
                .filter(|request| request.name != omitted_name)
                .collect();
            let error = validate_aol_control_mapping(&candidate)
                .expect_err("AOL mapping missing a named target is refused");
            let ServiceControlMappingError::MissingNamedTarget(missing_name) = error;
            println!(
                "TASK1255 omitted_named_target={omitted_name} refused_missing_named_target={missing_name}"
            );
            assert_eq!(missing_name, omitted_name);
            refused_missing_names.push(missing_name);
        }
        println!(
            "TASK1255 refused_missing_target_count={}",
            refused_missing_names.len()
        );

        assert_eq!(refused_missing_names, expected_names);
    }

    #[test]
    fn task_3044_shared_mail_owner_check_uses_sender_address_only_and_refuses_unreadable_sender() {
        let signed_in_address = "signed-in@example.test";
        let messages = vec![
            VisibleMailMessage {
                message_id: "sent-3044-1".to_owned(),
                mailbox: "Sent".to_owned(),
                sender_address: Some("signed-in@example.test".to_owned()),
            },
            VisibleMailMessage {
                message_id: "sent-3044-2".to_owned(),
                mailbox: "Sent".to_owned(),
                sender_address: Some("Signed-In@Example.Test".to_owned()),
            },
            VisibleMailMessage {
                message_id: "sent-3044-3".to_owned(),
                mailbox: "Sent".to_owned(),
                sender_address: Some(" signed-in@example.test ".to_owned()),
            },
            VisibleMailMessage {
                message_id: "inbox-3044-1".to_owned(),
                mailbox: "Inbox".to_owned(),
                sender_address: Some("friend-one@example.test".to_owned()),
            },
            VisibleMailMessage {
                message_id: "inbox-3044-2".to_owned(),
                mailbox: "Inbox".to_owned(),
                sender_address: Some("alerts@example.test".to_owned()),
            },
            VisibleMailMessage {
                message_id: "inbox-3044-3".to_owned(),
                mailbox: "Inbox".to_owned(),
                sender_address: Some("team@example.test".to_owned()),
            },
        ];

        let mut sent_yes = 0;
        let mut inbox_no = 0;
        for message in &messages {
            let owned = mail_message_is_owned_by_signed_in_address(signed_in_address, message)
                .expect("seeded message sender is readable");
            println!(
                "TASK3044 message={} mailbox={} owned={owned}",
                message.message_id, message.mailbox
            );
            match message.mailbox.as_str() {
                "Sent" if owned => sent_yes += 1,
                "Inbox" if !owned => inbox_no += 1,
                _ => panic!("unexpected ownership verdict for {message:?}"),
            }
        }

        let unreadable = VisibleMailMessage {
            message_id: "unreadable-3044".to_owned(),
            mailbox: "Sent".to_owned(),
            sender_address: None,
        };
        let unreadable_error =
            mail_message_is_owned_by_signed_in_address(signed_in_address, &unreadable)
                .expect_err("unreadable sender address is refused");

        println!("TASK3044 check=mail_message_is_owned_by_signed_in_address");
        println!("TASK3044 signed_in_address={signed_in_address}");
        println!("TASK3044 sent_seeded_yes_count={sent_yes}");
        println!("TASK3044 inbox_seeded_no_count={inbox_no}");
        println!("TASK3044 unreadable_sender_refused=true");
        println!(
            "TASK3044 unreadable_sender_error={}",
            unreadable_error.reason()
        );

        assert_eq!(sent_yes, 3);
        assert_eq!(inbox_no, 3);
        assert_eq!(
            unreadable_error.reason(),
            "OSL: sender address cannot be read"
        );
        assert!(
            mail_message_is_owned_by_signed_in_address(
                signed_in_address,
                &VisibleMailMessage {
                    message_id: "blank-sender-3044".to_owned(),
                    mailbox: "Sent".to_owned(),
                    sender_address: Some(" ".to_owned()),
                },
            )
            .is_err(),
            "blank sender addresses must fail closed too"
        );
    }
}
