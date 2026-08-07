const AOL_SOURCE: &str = include_str!("../src/service_connections.rs");
const MARK: &str = "MAPLE-1258";

#[test]
fn task_1258a_aol_body_rename_refuses_second_placement_without_overwrite() {
    let mut page = FakeAolPage::from_production_mapping();
    let before_names = page.control_names.clone();

    println!("TASK1258A service=aol");
    println!("TASK1258A initial_control_names={}", before_names.join(","));
    println!(
        "TASK1258A before_placement_count={}",
        page.placement_count()
    );
    println!("TASK1258A before_body={}", page.body());

    assert_eq!(page.placement_count(), 0);
    assert_eq!(page.body(), "");

    page.place_through_body(MARK)
        .expect("MAPLE-1258 places through the production AOL body target");

    println!("TASK1258A placed_text={MARK}");
    println!("TASK1258A after_placement_count={}", page.placement_count());
    println!("TASK1258A after_body={}", page.body());

    assert_eq!(page.placement_count(), 1);
    assert_eq!(page.body(), MARK);

    page.rename_body_control_to_missing_body();
    let after_names = page.control_names.clone();
    let changed_names = before_names
        .iter()
        .zip(after_names.iter())
        .filter(|(before, after)| before != after)
        .count();

    println!("TASK1258A changed_control_name=body->Missing Body");
    println!("TASK1258A changed_control_count={changed_names}");
    println!("TASK1258A renamed_control_names={}", after_names.join(","));

    assert_eq!(changed_names, 1);
    assert_eq!(
        after_names,
        vec![
            "compose".to_owned(),
            "Missing Body".to_owned(),
            "Send".to_owned(),
            "folders".to_owned(),
            "thread view".to_owned(),
            "reading pane".to_owned(),
        ]
    );

    let refusal = page
        .place_through_body("SHOULD-NOT-REPLACE")
        .expect_err("renamed AOL body must be refused");

    println!("TASK1258A missing_body_refusal={refusal}");
    println!(
        "TASK1258A after_missing_body_count={}",
        page.placement_count()
    );
    println!("TASK1258A after_missing_body_body={}", page.body());

    assert_eq!(refusal, "AOL body missing");
    assert_eq!(page.body(), MARK);
    assert_eq!(page.placement_count(), 1);
}

#[derive(Debug)]
struct FakeAolPage {
    control_names: Vec<String>,
    body_text: String,
    placements: Vec<String>,
}

impl FakeAolPage {
    fn from_production_mapping() -> Self {
        let control_names = production_aol_control_names();
        assert_eq!(
            control_names,
            vec![
                "compose".to_owned(),
                "body".to_owned(),
                "Send".to_owned(),
                "folders".to_owned(),
                "thread view".to_owned(),
                "reading pane".to_owned(),
            ],
            "the production AOL control mapping must expose the expected body target"
        );
        Self {
            control_names,
            body_text: String::new(),
            placements: Vec::new(),
        }
    }

    fn placement_count(&self) -> usize {
        self.placements.len()
    }

    fn body(&self) -> &str {
        &self.body_text
    }

    fn place_through_body(&mut self, text: &str) -> Result<(), &'static str> {
        if !self.has_exact_body_control() {
            return Err("AOL body missing");
        }
        self.body_text = text.to_owned();
        self.placements.push(text.to_owned());
        Ok(())
    }

    fn has_exact_body_control(&self) -> bool {
        self.control_names.iter().any(|name| name == "body")
    }

    fn rename_body_control_to_missing_body(&mut self) {
        let body = self
            .control_names
            .iter_mut()
            .find(|name| name.as_str() == "body")
            .expect("fixture starts with production AOL body control");
        *body = "Missing Body".to_owned();
    }
}

fn production_aol_control_names() -> Vec<String> {
    let block = source_between(
        AOL_SOURCE,
        "const AOL_CONTROL_REQUESTS: [WebsiteNamedControlRequest; 6] = [",
        "pub const fn gmail_control_mapping()",
    );
    let mut names = Vec::new();
    let mut body_is_editable = false;
    for line in block.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix("name: \"")
            .and_then(|rest| rest.strip_suffix("\","))
        {
            names.push(name.to_owned());
        }
        if trimmed == "kind: WebsiteControlKind::EditableBox,"
            && names.last().map(String::as_str) == Some("body")
        {
            body_is_editable = true;
        }
    }
    assert!(
        body_is_editable,
        "the production AOL body target must remain an editable box"
    );
    names
}

fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let after_start = source
        .split_once(start)
        .map(|(_, rest)| rest)
        .expect("production AOL control request block exists");
    after_start
        .split_once(end)
        .map(|(block, _)| block)
        .expect("production AOL control request block has a following item")
}
