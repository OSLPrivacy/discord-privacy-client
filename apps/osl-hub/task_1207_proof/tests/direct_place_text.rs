#[allow(dead_code)]
#[path = "../../src/website_driver.rs"]
mod website_driver;

use std::collections::BTreeMap;
use website_driver::{
    WebsiteControlKind, WebsiteDriver, WebsiteDriverError, WebsiteNamedControl, WebsitePage,
    WebsitePageControls, WebsitePageRequest, WebsitePageText, WebsitePlacementProof,
    WebsiteTextPlacement,
};

#[derive(Debug)]
struct DraftFixtureDriver {
    page: WebsitePage,
    drafts: BTreeMap<String, String>,
    sent_message_count: usize,
}

impl DraftFixtureDriver {
    fn new() -> Self {
        let page = WebsitePage::synthetic("https://fixture.invalid/task-1207".to_owned());
        let mut drafts = BTreeMap::new();
        drafts.insert("Subject".to_owned(), "keep this subject".to_owned());
        drafts.insert("Body".to_owned(), "old fixture draft".to_owned());
        Self {
            page,
            drafts,
            sent_message_count: 0,
        }
    }

    fn draft(&self, editable_box_name: &str) -> &str {
        self.drafts
            .get(editable_box_name)
            .map(String::as_str)
            .expect("fixture editable box exists")
    }
}

impl WebsiteDriver for DraftFixtureDriver {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        if request.url == self.page.url {
            Ok(self.page.clone())
        } else {
            Err(WebsiteDriverError::PageNotFound)
        }
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        if page != &self.page {
            return Err(WebsiteDriverError::PageNotFound);
        }
        Ok(WebsitePageText {
            page: page.clone(),
            title: "TASK1207 fixture draft".to_owned(),
            text: self
                .drafts
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join("\n"),
            controls: WebsitePageControls {
                editable_boxes: self.drafts.keys().cloned().collect(),
                buttons: vec!["Send".to_owned()],
                visible_message_areas: Vec::new(),
            },
        })
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        if placement.page != self.page {
            return Err(WebsiteDriverError::PageNotFound);
        }
        let draft = self
            .drafts
            .get_mut(&placement.editable_box_name)
            .ok_or(WebsiteDriverError::NamedControlNotFound)?;
        *draft = placement.text;
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: draft.encode_utf16().count(),
            placed_sha256: "fixture-direct-place-text".to_owned(),
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        if control.page != self.page || control.kind != WebsiteControlKind::Button {
            return Err(WebsiteDriverError::NamedControlNotFound);
        }
        if control.name == "Send" {
            self.sent_message_count += 1;
            Ok(())
        } else {
            Err(WebsiteDriverError::NamedControlNotFound)
        }
    }
}

#[test]
fn task_1207_direct_place_text_changes_named_draft_without_sending() {
    let mut driver = DraftFixtureDriver::new();
    let page = driver
        .find_page(WebsitePageRequest {
            url: "https://fixture.invalid/task-1207".to_owned(),
        })
        .expect("fixture page exists");
    let before_sent = driver.sent_message_count;

    let proof = driver
        .place_text(WebsiteTextPlacement {
            page,
            editable_box_name: "Body".to_owned(),
            text: "TASK1207 placed draft".to_owned(),
        })
        .expect("direct place_text command changes the named editable box");

    let after_sent = driver.sent_message_count;
    assert_eq!(proof.editable_box_name, "Body");
    assert_eq!(driver.draft("Body"), "TASK1207 placed draft");
    assert_eq!(driver.draft("Subject"), "keep this subject");
    assert_eq!(before_sent, 0);
    assert_eq!(after_sent, 0);

    println!("TASK1207 direct_command=place_text");
    println!("TASK1207 changed_editable_box={}", proof.editable_box_name);
    println!("TASK1207 draft_after={:?}", driver.draft("Body"));
    println!("TASK1207 untouched_editable_box=Subject");
    println!("TASK1207 sent_message_count_before={before_sent}");
    println!("TASK1207 sent_message_count_after={after_sent}");
}
