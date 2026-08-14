use osl_privacy_hub::{
    web_surface_adapter::outlook::outlook_web_mapped_controls,
    website_driver::{
        WebsiteDriver, WebsiteDriverError, WebsiteNamedControl, WebsitePage, WebsitePageControls,
        WebsitePageRequest, WebsitePageText, WebsitePlacementProof, WebsiteSelectedEmail,
        WebsiteTextPlacement,
    },
};

const PAGE_URL: &str = "https://outlook.live.com/mail/0/inbox/task-1237";
const MARKED_COVER_MESSAGE: &str = "OSL-OUTLOOK-WEB-1237 cover message";

#[test]
fn task_1237_outlook_web_fake_page_place_read_send_controls_drive_state() {
    let mut driver = FakeOutlookWebPage::new();
    let page = driver
        .find_page(WebsitePageRequest {
            url: PAGE_URL.to_owned(),
        })
        .expect("Outlook web fake page opens");
    let read_page = driver.read_page(&page).expect("Outlook web controls read");
    let control_names = mapped_control_names();

    println!("TASK1237 sent_count_start={}", driver.sent_count());
    println!("TASK1237 mapped_control_count={}", control_names.len());
    for name in &control_names {
        println!("TASK1237 mapped_control={name}");
    }

    assert_eq!(driver.sent_count(), 0);
    assert_eq!(control_names, vec!["Place", "Read", "Send"]);
    assert_eq!(read_page.controls.buttons, control_names);

    press_mapped(&mut driver, &page, "Place").expect("Place control places cover");
    println!("TASK1237 placed_message_count={}", driver.placed_count());
    println!("TASK1237 placed_message={}", driver.last_placed_message());
    assert_eq!(driver.placed_count(), 1);
    assert_eq!(driver.last_placed_message(), MARKED_COVER_MESSAGE);

    let read = read_mapped(&mut driver, &page, "Read").expect("Read control returns message");
    println!("TASK1237 read_words={}", read.body);
    assert_eq!(read.body, MARKED_COVER_MESSAGE);

    press_mapped(&mut driver, &page, "Send").expect("Send control sends placed cover");
    println!("TASK1237 sent_count_after_send={}", driver.sent_count());
    assert_eq!(driver.sent_count(), 1);

    let placed_before_refusal = driver.placed_count();
    let sent_before_refusal = driver.sent_count();
    driver.remove_control("Send");
    let refusal =
        press_mapped(&mut driver, &page, "Send").expect_err("removed Send control must be refused");
    println!("TASK1237 removed_send_refusal={refusal}");
    println!(
        "TASK1237 placed_count_after_removed_send={}",
        driver.placed_count()
    );
    println!(
        "TASK1237 sent_count_after_removed_send={}",
        driver.sent_count()
    );

    assert_eq!(refusal, WebsiteDriverError::NamedControlNotFound);
    assert_eq!(driver.placed_count(), placed_before_refusal);
    assert_eq!(driver.sent_count(), sent_before_refusal);
}

fn mapped_control_names() -> Vec<String> {
    outlook_web_mapped_controls()
        .iter()
        .map(|control| control.name.to_owned())
        .collect()
}

fn press_mapped(
    driver: &mut FakeOutlookWebPage,
    page: &WebsitePage,
    name: &str,
) -> Result<(), WebsiteDriverError> {
    let control = outlook_web_mapped_controls()
        .iter()
        .find(|control| control.name == name)
        .ok_or(WebsiteDriverError::NamedControlNotFound)?;
    driver.press_named_control(WebsiteNamedControl {
        page: page.clone(),
        name: control.name.to_owned(),
    })
}

fn read_mapped(
    driver: &mut FakeOutlookWebPage,
    page: &WebsitePage,
    name: &str,
) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
    let control = outlook_web_mapped_controls()
        .iter()
        .find(|control| control.name == name && control.action == "read-open-cover-message")
        .ok_or(WebsiteDriverError::NamedControlNotFound)?;
    if control.name != "Read" {
        return Err(WebsiteDriverError::NamedControlNotFound);
    }
    driver.read_selected_email(page)
}

#[derive(Debug)]
struct FakeOutlookWebPage {
    controls: Vec<String>,
    placed_messages: Vec<String>,
    sent_messages: Vec<String>,
}

impl FakeOutlookWebPage {
    fn new() -> Self {
        Self {
            controls: mapped_control_names(),
            placed_messages: Vec::new(),
            sent_messages: Vec::new(),
        }
    }

    fn sent_count(&self) -> usize {
        self.sent_messages.len()
    }

    fn placed_count(&self) -> usize {
        self.placed_messages.len()
    }

    fn last_placed_message(&self) -> &str {
        self.placed_messages
            .last()
            .map(String::as_str)
            .unwrap_or("")
    }

    fn remove_control(&mut self, name: &str) {
        self.controls.retain(|control| control != name);
    }

    fn has_control(&self, name: &str) -> bool {
        self.controls.iter().any(|control| control == name)
    }
}

impl WebsiteDriver for FakeOutlookWebPage {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage::synthetic(request.url))
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        Ok(WebsitePageText {
            page: page.clone(),
            title: "Task 1237 Outlook web fixture".to_owned(),
            text: self.placed_messages.join("\n"),
            controls: WebsitePageControls {
                editable_boxes: Vec::new(),
                buttons: self.controls.clone(),
                visible_message_areas: vec!["Read".to_owned()],
            },
        })
    }

    fn read_selected_email(
        &mut self,
        page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        if !self.has_control("Read") {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        let body = self
            .placed_messages
            .last()
            .cloned()
            .ok_or(WebsiteDriverError::ReadFailed)?;
        Ok(WebsiteSelectedEmail {
            page: page.clone(),
            message_id: "outlook-web-1237-open-message".to_owned(),
            body,
            conversation_identity: "outlook-web-1237-thread".to_owned(),
        })
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        if !self.has_control("Place") {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        self.placed_messages.push(placement.text.clone());
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: "outlook-web-1237-placement".to_owned(),
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        if !self.has_control(&control.name) {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        match control.name.as_str() {
            "Place" => {
                self.place_text(WebsiteTextPlacement {
                    page: control.page,
                    editable_box_name: "Place".to_owned(),
                    text: MARKED_COVER_MESSAGE.to_owned(),
                })?;
                Ok(())
            }
            "Read" => Ok(()),
            "Send" => {
                let message = self
                    .placed_messages
                    .last()
                    .cloned()
                    .ok_or(WebsiteDriverError::ReadFailed)?;
                self.sent_messages.push(message);
                Ok(())
            }
            _ => Err(WebsiteDriverError::NamedControlNotFound),
        }
    }
}
