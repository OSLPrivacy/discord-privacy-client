#[path = "../../src/website_driver.rs"]
pub mod website_driver;

use std::collections::BTreeMap;

use website_driver::{
    discover_browser_conversation, place_connected_page_text, WebsiteConversationAccessibility,
    WebsiteConversationBrowserSnapshot, WebsiteConversationDiscovery, WebsiteDriver,
    WebsiteDriverError, WebsiteLiveRunProgress, WebsiteNamedControl, WebsitePage,
    WebsitePageControls, WebsitePageRequest, WebsitePageText, WebsitePlacementProof,
    WebsiteSelectedEmail, WebsiteTextPlacement,
};

pub const APP: &str = "Messenger";
pub const COMPOSER: &str = "Message";

#[derive(Debug, Eq, PartialEq)]
pub struct PageConnectionRun {
    pub app: String,
    pub page_connection: bool,
    pub front_window_grab: bool,
    pub text: String,
    pub readback: String,
    pub front_at_start: String,
    pub front_at_end: String,
    pub app_starts: usize,
    pub proof: WebsitePlacementProof,
}

struct ConnectedPageFixture {
    page: WebsitePage,
    boxes: BTreeMap<String, String>,
}

impl ConnectedPageFixture {
    fn new() -> Self {
        let page = WebsitePage::synthetic("https://www.messenger.com/t/MAPLE-3422".to_owned());
        let mut boxes = BTreeMap::new();
        boxes.insert(COMPOSER.to_owned(), String::new());
        boxes.insert("Search".to_owned(), "keep-search-unchanged".to_owned());
        Self { page, boxes }
    }

    fn read_box(&self, name: &str) -> Result<String, WebsiteDriverError> {
        self.boxes
            .get(name)
            .cloned()
            .ok_or(WebsiteDriverError::NamedControlNotFound)
    }
}

impl WebsiteDriver for ConnectedPageFixture {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        (request.url == self.page.url)
            .then_some(self.page.clone())
            .ok_or(WebsiteDriverError::PageNotFound)
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        if page != &self.page {
            return Err(WebsiteDriverError::PageNotFound);
        }
        Ok(WebsitePageText {
            page: page.clone(),
            title: "MAPLE-3422 — Messenger".to_owned(),
            text: self
                .boxes
                .iter()
                .map(|(name, value)| format!("{name}: {value}"))
                .collect::<Vec<_>>()
                .join("\n"),
            controls: WebsitePageControls {
                editable_boxes: self.boxes.keys().cloned().collect(),
                buttons: Vec::new(),
                visible_message_areas: Vec::new(),
            },
        })
    }

    fn read_selected_email(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        Err(WebsiteDriverError::PageUnavailable)
    }

    fn read_live_run_progress(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        Err(WebsiteDriverError::PageUnavailable)
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        if placement.page != self.page {
            return Err(WebsiteDriverError::PageNotFound);
        }
        let box_value = self
            .boxes
            .get_mut(&placement.editable_box_name)
            .ok_or(WebsiteDriverError::NamedControlNotFound)?;
        *box_value = placement.text.clone();
        let readback = self.read_box(&placement.editable_box_name)?;
        if readback != placement.text {
            return Err(WebsiteDriverError::TextPlacementFailed);
        }
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: "fixture-shared-1207-placer".to_owned(),
        })
    }

    fn press_named_control(
        &mut self,
        _control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        Err(WebsiteDriverError::NamedControlNotFound)
    }
}

pub fn run_direct_command(
    front_window_grab: bool,
    app: &str,
    text: &str,
) -> Result<PageConnectionRun, WebsiteDriverError> {
    if app != APP || text.is_empty() {
        return Err(WebsiteDriverError::PageUnavailable);
    }

    let front_at_start = "OSL Privacy".to_owned();
    let mut driver = ConnectedPageFixture::new();
    let page = driver.find_page(WebsitePageRequest {
        url: driver.page.url.clone(),
    })?;
    let discovery = discover_browser_conversation(&WebsiteConversationBrowserSnapshot {
        url: page.url.clone(),
        title: "MAPLE-3422 — Messenger".to_owned(),
        accessibility: WebsiteConversationAccessibility {
            service: "messenger".to_owned(),
            active_conversation: true,
            composer: Some(website_driver::WebsiteComposerAccessibility {
                role: "textbox".to_owned(),
                name: COMPOSER.to_owned(),
                accessible: true,
            }),
        },
    })?;
    place_discovered_page(
        &mut driver,
        page,
        &discovery,
        text,
        front_window_grab,
        front_at_start,
    )
}

fn place_discovered_page(
    driver: &mut ConnectedPageFixture,
    page: WebsitePage,
    discovery: &WebsiteConversationDiscovery,
    text: &str,
    front_window_grab: bool,
    front_at_start: String,
) -> Result<PageConnectionRun, WebsiteDriverError> {
    let proof =
        place_connected_page_text(driver, page, discovery, text.to_owned(), front_window_grab)?;
    let readback = driver.read_box(&discovery.composer)?;
    if readback != text {
        return Err(WebsiteDriverError::TextPlacementFailed);
    }
    Ok(PageConnectionRun {
        app: APP.to_owned(),
        page_connection: true,
        front_window_grab,
        text: text.to_owned(),
        readback,
        front_at_end: front_at_start.clone(),
        front_at_start,
        app_starts: 0,
        proof,
    })
}
