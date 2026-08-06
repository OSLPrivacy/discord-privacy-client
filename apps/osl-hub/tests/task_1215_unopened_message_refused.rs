use osl_privacy_hub::{
    hub_command_surface::{
        read_protected_email_open_message_with_driver_and_state,
        ProtectedEmailOpenMessageReadRequest, ProtectedEmailReaderState,
    },
    website_driver::{
        WebsiteDriver, WebsiteDriverError, WebsitePage, WebsitePageRequest, WebsiteSelectedEmail,
    },
};

const PAGE_URL: &str = "https://mail.example.test/inbox";
const MAPLE_MESSAGE_ID: &str = "maple-mail-1";
const MAPLE_BODY: &str = "Selected protected email fixture containing MAPLE-4172.";
const MAPLE_THREAD_ID: &str = "maple-thread-1";

#[test]
fn task_1215_check_unopened_message_is_refused() {
    let mut driver = FixtureEmailDriver::new(Some(FixtureMessage {
        message_id: MAPLE_MESSAGE_ID.to_owned(),
        body: MAPLE_BODY.to_owned(),
        conversation_identity: MAPLE_THREAD_ID.to_owned(),
    }));
    let mut state = ProtectedEmailReaderState::default();

    println!(
        "TASK1215 successful_read_count_before={}",
        state.successful_read_count()
    );

    let read = read_protected_email_open_message_with_driver_and_state(
        &mut driver,
        &mut state,
        ProtectedEmailOpenMessageReadRequest {
            page_url: PAGE_URL.to_owned(),
        },
    )
    .expect("selected maple message should read once");

    assert_eq!(read.message_id, MAPLE_MESSAGE_ID);
    assert!(read.cover_message.contains("MAPLE-4172"));
    assert_eq!(state.successful_read_count(), 1);
    println!("TASK1215 reader_returned_message_id={}", read.message_id);
    println!("TASK1215 reader_returned_body_token=MAPLE-4172");
    println!(
        "TASK1215 successful_read_count_after={}",
        state.successful_read_count()
    );

    driver.set_selected_message(None);
    println!("TASK1215 selected_message_after_change=none");

    let refusal = read_protected_email_open_message_with_driver_and_state(
        &mut driver,
        &mut state,
        ProtectedEmailOpenMessageReadRequest {
            page_url: PAGE_URL.to_owned(),
        },
    )
    .expect_err("none must be refused as no selected message");

    assert_eq!(refusal, "no selected message");
    assert!(!refusal.contains(MAPLE_MESSAGE_ID));
    assert!(!refusal.contains("MAPLE-4172"));
    println!("TASK1215 none_refusal={refusal}");

    let saved = state
        .last_successful_read()
        .expect("failed reads must not clear the saved successful result");
    assert_eq!(saved.message_id, MAPLE_MESSAGE_ID);
    assert!(saved.cover_message.contains("MAPLE-4172"));
    assert_eq!(state.successful_read_count(), 1);
    println!("TASK1215 saved_result_message_id={}", saved.message_id);
    println!("TASK1215 saved_result_body_token=MAPLE-4172");
    println!(
        "TASK1215 successful_read_count_after_none={}",
        state.successful_read_count()
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FixtureMessage {
    message_id: String,
    body: String,
    conversation_identity: String,
}

#[derive(Debug)]
struct FixtureEmailDriver {
    selected_message: Option<FixtureMessage>,
}

impl FixtureEmailDriver {
    fn new(selected_message: Option<FixtureMessage>) -> Self {
        Self { selected_message }
    }

    fn set_selected_message(&mut self, selected_message: Option<FixtureMessage>) {
        self.selected_message = selected_message;
    }
}

impl WebsiteDriver for FixtureEmailDriver {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage { url: request.url })
    }

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        let selected = self
            .selected_message
            .clone()
            .ok_or(WebsiteDriverError::NoSelectedMessage)?;
        Ok(WebsiteSelectedEmail {
            page: page.clone(),
            message_id: selected.message_id,
            body: selected.body,
            conversation_identity: selected.conversation_identity,
        })
    }
}
