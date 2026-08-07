use std::collections::BTreeMap;

const WEBSITE_DRIVER_SOURCE: &str = include_str!("../../apps/osl-hub/src/website_driver.rs");

#[derive(Clone, Debug, Eq, PartialEq)]
struct WebsitePage {
    url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WebsiteTextPlacement {
    page: WebsitePage,
    editable_box_name: String,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WebsiteEditableBoxRead {
    editable_box_name: String,
    text: String,
}

trait WebsiteDriver {
    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), String>;

    fn read_editable_box(
        &mut self,
        page: &WebsitePage,
        editable_box_name: &str,
    ) -> Result<WebsiteEditableBoxRead, String>;
}

struct DraftFixtureDriver {
    drafts: BTreeMap<String, String>,
}

impl DraftFixtureDriver {
    fn new() -> Self {
        let mut drafts = BTreeMap::new();
        drafts.insert("Subject".to_owned(), "keep this subject".to_owned());
        drafts.insert("Body".to_owned(), "old draft".to_owned());
        Self { drafts }
    }
}

impl WebsiteDriver for DraftFixtureDriver {
    fn place_text(&mut self, placement: WebsiteTextPlacement) -> Result<(), String> {
        let draft = self
            .drafts
            .get_mut(&placement.editable_box_name)
            .ok_or_else(|| format!("missing editable box {}", placement.editable_box_name))?;
        *draft = placement.text;
        Ok(())
    }

    fn read_editable_box(
        &mut self,
        _page: &WebsitePage,
        editable_box_name: &str,
    ) -> Result<WebsiteEditableBoxRead, String> {
        let text = self
            .drafts
            .get(editable_box_name)
            .ok_or_else(|| format!("missing editable box {editable_box_name}"))?
            .clone();
        Ok(WebsiteEditableBoxRead {
            editable_box_name: editable_box_name.to_owned(),
            text,
        })
    }
}

fn assert_backend_command_exists() {
    for needle in [
        "ReadEditableBox",
        "\"read_editable_box\"",
        "pub struct WebsiteEditableBoxRead",
        "fn read_editable_box(",
        "fn read_named_editable_box(",
    ] {
        assert!(
            WEBSITE_DRIVER_SOURCE.contains(needle),
            "backend website driver source is missing {needle}"
        );
    }
}

fn main() {
    assert_backend_command_exists();

    let mut driver = DraftFixtureDriver::new();
    let page = WebsitePage {
        url: "https://fixture.invalid/draft".to_owned(),
    };
    let fixture_draft = "TASK1210 first fixture line\nTASK1210 second fixture line";

    driver
        .place_text(WebsiteTextPlacement {
            page: page.clone(),
            editable_box_name: "Body".to_owned(),
            text: fixture_draft.to_owned(),
        })
        .expect("place fixture draft");
    let read = driver
        .read_editable_box(&page, "Body")
        .expect("read named editable box");

    assert_eq!(read.editable_box_name, "Body");
    assert_eq!(read.text, fixture_draft);
    assert_eq!(read.text.lines().count(), 2);
    println!("{}", read.text);
}
