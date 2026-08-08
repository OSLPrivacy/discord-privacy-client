#![allow(dead_code)]

mod row_who_wrote_it {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SharedRowWhoWroteIt {
        Yours,
        Theirs,
        NotPublishedByApp,
    }
}

mod website_driver {
    use url::Url;

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum WebsiteDriverKind {
        RealBrowser,
        FakeTestBrowser,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct WebsitePage {
        pub target_id: String,
        pub url: String,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct WebsitePageSnapshot {
        pub title: String,
        pub url: String,
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub enum WebsiteControlKind {
        EditableBox,
        Button,
        VisibleMessageArea,
    }

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct WebsiteNamedControlRequest {
        pub name: &'static str,
        pub kind: WebsiteControlKind,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub struct WebsiteNamedControl {
        pub name: String,
        pub kind: WebsiteControlKind,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    pub enum WebsiteDriverError {
        PageUnavailable,
        MissingNamedControl(String),
        InvalidUrl,
    }

    pub trait WebsiteDriver {
        fn kind(&self) -> WebsiteDriverKind;
        fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError>;
        fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError>;
        fn read_named_controls(
            &self,
            page: &WebsitePage,
            required: &[WebsiteNamedControlRequest],
        ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError>;
    }
}

#[path = "../../src/service_connections.rs"]
mod service_connections;

use service_connections::{
    icloud_control_mapping, validate_icloud_control_mapping, EmailServiceConnection,
    ServiceConnectionError, ICLOUD_CONTROL_NAMES, ICLOUD_SERVICE_ID,
};
use std::cell::RefCell;
use url::Url;
use website_driver::{
    WebsiteControlKind, WebsiteDriver, WebsiteDriverError, WebsiteDriverKind, WebsiteNamedControl,
    WebsiteNamedControlRequest, WebsitePage, WebsitePageSnapshot,
};

const TASK_1275_WORDS: &str = "OSL-ICLOUD-1275";
const TASK_1275_PAGE_ID: &str = "task-1275-icloud-fake-page";

#[derive(Debug, Default)]
struct IcloudFakePage {
    requested: RefCell<Vec<WebsiteNamedControlRequest>>,
    send_name: String,
    draft: Option<String>,
    placed_message: Option<String>,
    placed_message_count: usize,
    sent_messages: Vec<String>,
}

impl IcloudFakePage {
    fn new() -> Self {
        Self {
            send_name: "Send".to_owned(),
            ..Self::default()
        }
    }

    fn named_controls(&self) -> Vec<WebsiteNamedControl> {
        vec![
            WebsiteNamedControl {
                name: "Compose".to_owned(),
                kind: WebsiteControlKind::Button,
            },
            WebsiteNamedControl {
                name: "Place".to_owned(),
                kind: WebsiteControlKind::EditableBox,
            },
            WebsiteNamedControl {
                name: "Readback".to_owned(),
                kind: WebsiteControlKind::VisibleMessageArea,
            },
            WebsiteNamedControl {
                name: self.send_name.clone(),
                kind: WebsiteControlKind::Button,
            },
        ]
    }

    fn requested_names(&self) -> Vec<String> {
        self.requested
            .borrow()
            .iter()
            .map(|request| request.name.to_owned())
            .collect()
    }

    fn compose(&mut self, control: &WebsiteNamedControl) -> String {
        assert_eq!(control.name, "Compose");
        assert_eq!(control.kind, WebsiteControlKind::Button);
        self.draft = Some(TASK_1275_WORDS.to_owned());
        TASK_1275_WORDS.to_owned()
    }

    fn place(&mut self, control: &WebsiteNamedControl, text: &str) -> String {
        assert_eq!(control.name, "Place");
        assert_eq!(control.kind, WebsiteControlKind::EditableBox);
        assert_eq!(self.draft.as_deref(), Some(text));
        self.placed_message = Some(text.to_owned());
        self.placed_message_count += 1;
        text.to_owned()
    }

    fn readback(&self, control: &WebsiteNamedControl) -> String {
        assert_eq!(control.name, "Readback");
        assert_eq!(control.kind, WebsiteControlKind::VisibleMessageArea);
        self.placed_message
            .clone()
            .expect("placed message is readable")
    }

    fn send(&mut self, control: &WebsiteNamedControl) -> String {
        assert_eq!(control.name, "Send");
        assert_eq!(control.kind, WebsiteControlKind::Button);
        let message = self
            .placed_message
            .clone()
            .expect("placed message exists before send");
        self.sent_messages.push(message.clone());
        message
    }

    fn rename_send(&mut self, renamed: &str) {
        self.send_name = renamed.to_owned();
    }
}

impl WebsiteDriver for IcloudFakePage {
    fn kind(&self) -> WebsiteDriverKind {
        WebsiteDriverKind::FakeTestBrowser
    }

    fn find_page(&mut self, url: &Url) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage {
            target_id: TASK_1275_PAGE_ID.to_owned(),
            url: url.to_string(),
        })
    }

    fn read_page(&self, page: &WebsitePage) -> Result<WebsitePageSnapshot, WebsiteDriverError> {
        if page.target_id != TASK_1275_PAGE_ID {
            return Err(WebsiteDriverError::PageUnavailable);
        }
        Ok(WebsitePageSnapshot {
            title: "Task 1275 iCloud fake page".to_owned(),
            url: page.url.clone(),
        })
    }

    fn read_named_controls(
        &self,
        page: &WebsitePage,
        required: &[WebsiteNamedControlRequest],
    ) -> Result<Vec<WebsiteNamedControl>, WebsiteDriverError> {
        if page.target_id != TASK_1275_PAGE_ID {
            return Err(WebsiteDriverError::PageUnavailable);
        }
        self.requested.borrow_mut().extend_from_slice(required);
        let available = self.named_controls();
        required
            .iter()
            .map(|request| {
                available
                    .iter()
                    .find(|control| control.name == request.name && control.kind == request.kind)
                    .cloned()
                    .ok_or_else(|| WebsiteDriverError::MissingNamedControl(request.name.to_owned()))
            })
            .collect()
    }
}

#[test]
fn task_1275_icloud_fake_page_flow_refuses_renamed_send_without_mutating_counts() {
    let mapping = icloud_control_mapping();
    validate_icloud_control_mapping(mapping).expect("complete iCloud mapping is accepted");
    let mapping_names: Vec<&str> = mapping.iter().map(|request| request.name).collect();
    assert_eq!(mapping_names, ICLOUD_CONTROL_NAMES);

    let mut fixture = IcloudFakePage::new();
    let url =
        Url::parse("https://www.icloud.com/mail/task-1275-fixture").expect("task 1275 URL parses");
    let page = fixture.find_page(&url).expect("fake iCloud page opens");
    let connection = EmailServiceConnection::new(ICLOUD_SERVICE_ID, "icloud-task-1275");
    let initial_controls = fixture.named_controls();
    let initial_control_names: Vec<String> = initial_controls
        .iter()
        .map(|control| control.name.clone())
        .collect();

    assert_eq!(
        initial_control_names,
        ["Compose", "Place", "Readback", "Send"]
    );
    assert_eq!(fixture.sent_messages.len(), 0);
    assert_eq!(fixture.placed_message_count, 0);

    let controls = connection
        .request_icloud_flow_controls(&fixture, &page)
        .expect("iCloud named controls are present");
    let compose_word = fixture.compose(&controls.compose);
    let place_word = fixture.place(&controls.place, &compose_word);
    let readback_word = fixture.readback(&controls.readback);
    let send_word = fixture.send(&controls.send);

    assert_eq!(compose_word, TASK_1275_WORDS);
    assert_eq!(place_word, TASK_1275_WORDS);
    assert_eq!(readback_word, TASK_1275_WORDS);
    assert_eq!(send_word, TASK_1275_WORDS);
    assert_eq!(fixture.placed_message_count, 1);
    assert_eq!(fixture.sent_messages.len(), 1);

    let placed_before_rename = fixture.placed_message.clone();
    let placed_count_before_rename = fixture.placed_message_count;
    let sent_count_before_rename = fixture.sent_messages.len();
    fixture.rename_send("Renamed Send");

    let renamed_error = connection
        .request_icloud_flow_controls(&fixture, &page)
        .expect_err("renamed Send is refused before place or send");

    assert_eq!(
        renamed_error,
        ServiceConnectionError::Driver(WebsiteDriverError::MissingNamedControl("Send".to_owned()))
    );
    assert_eq!(fixture.placed_message, placed_before_rename);
    assert_eq!(fixture.placed_message_count, placed_count_before_rename);
    assert_eq!(fixture.sent_messages.len(), sent_count_before_rename);

    println!("TASK1275 service_connection={ICLOUD_SERVICE_ID}");
    println!("TASK1275 initial_sent_email_count=0");
    println!("TASK1275 initial_control_count={}", initial_controls.len());
    for control in &initial_controls {
        println!(
            "TASK1275 initial_control name={} kind={:?}",
            control.name, control.kind
        );
    }
    println!(
        "TASK1275 requested_control_count={}",
        fixture.requested_names().len()
    );
    for name in fixture.requested_names() {
        println!("TASK1275 requested_control={name}");
    }
    println!("TASK1275 compose_word={compose_word}");
    println!("TASK1275 place_word={place_word}");
    println!("TASK1275 readback_word={readback_word}");
    println!("TASK1275 send_word={send_word}");
    println!(
        "TASK1275 placed_message_count_after_send={}",
        fixture.placed_message_count
    );
    println!(
        "TASK1275 sent_email_count_after_send={}",
        fixture.sent_messages.len()
    );
    println!("TASK1275 renamed_send_refused=true");
    println!("TASK1275 renamed_send_error={renamed_error:?}");
    println!(
        "TASK1275 placed_message_after_refusal={}",
        fixture.placed_message.as_deref().unwrap_or("")
    );
    println!(
        "TASK1275 placed_message_count_after_refusal={}",
        fixture.placed_message_count
    );
    println!(
        "TASK1275 sent_email_count_after_refusal={}",
        fixture.sent_messages.len()
    );
}
