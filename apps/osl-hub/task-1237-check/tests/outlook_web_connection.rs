#[path = "../../src/web_surface_adapter/outlook.rs"]
mod outlook;

use outlook::outlook_web_mapped_controls;

const PAGE_URL: &str = "https://outlook.live.com/mail/0/inbox/task-1237";
const MARKED_COVER_MESSAGE: &str = "OSL-OUTLOOK-WEB-1237 cover message";

#[test]
fn task_1237_outlook_web_fake_page_place_read_send_controls_drive_state() {
    let mut fixture = FakeOutlookWebPage::new(PAGE_URL);
    let controls = fixture.named_controls();

    println!("TASK1237 sent_count_start={}", fixture.sent_count());
    println!("TASK1237 mapped_control_count={}", controls.len());
    for name in &controls {
        println!("TASK1237 mapped_control={name}");
    }

    assert_eq!(fixture.sent_count(), 0);
    assert_eq!(controls, ["Place", "Read", "Send"]);

    fixture.press("Place").expect("Place control places cover");
    println!("TASK1237 placed_message_count={}", fixture.placed_count());
    println!("TASK1237 placed_message={}", fixture.last_placed_message());
    assert_eq!(fixture.placed_count(), 1);
    assert_eq!(fixture.last_placed_message(), MARKED_COVER_MESSAGE);

    let read = fixture.read("Read").expect("Read control returns message");
    println!("TASK1237 read_words={read}");
    assert_eq!(read, MARKED_COVER_MESSAGE);

    fixture
        .press("Send")
        .expect("Send control sends placed cover");
    println!("TASK1237 sent_count_after_send={}", fixture.sent_count());
    assert_eq!(fixture.sent_count(), 1);

    let placed_before_refusal = fixture.placed_count();
    let sent_before_refusal = fixture.sent_count();
    fixture.remove_control("Send");
    let refusal = fixture
        .press("Send")
        .expect_err("removed Send control must be refused");
    println!("TASK1237 removed_send_refusal={refusal}");
    println!(
        "TASK1237 placed_count_after_removed_send={}",
        fixture.placed_count()
    );
    println!(
        "TASK1237 sent_count_after_removed_send={}",
        fixture.sent_count()
    );

    assert_eq!(refusal, "named control Send is missing");
    assert_eq!(fixture.placed_count(), placed_before_refusal);
    assert_eq!(fixture.sent_count(), sent_before_refusal);
}

#[derive(Debug)]
struct FakeOutlookWebPage {
    page_url: String,
    controls: Vec<String>,
    placed_messages: Vec<String>,
    sent_messages: Vec<String>,
}

impl FakeOutlookWebPage {
    fn new(page_url: &str) -> Self {
        Self {
            page_url: page_url.to_owned(),
            controls: outlook_web_mapped_controls()
                .iter()
                .map(|control| control.name.to_owned())
                .collect(),
            placed_messages: Vec::new(),
            sent_messages: Vec::new(),
        }
    }

    fn named_controls(&self) -> Vec<&str> {
        self.controls.iter().map(String::as_str).collect()
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

    fn press(&mut self, name: &str) -> Result<(), String> {
        if !self.has_control(name) {
            return Err(format!("named control {name} is missing"));
        }
        match mapped_action(name)? {
            "place-cover-message" => {
                self.placed_messages.push(MARKED_COVER_MESSAGE.to_owned());
                Ok(())
            }
            "send-placed-cover-message" => {
                let message = self
                    .placed_messages
                    .last()
                    .cloned()
                    .ok_or_else(|| "no placed Outlook web message to send".to_owned())?;
                self.sent_messages.push(message);
                Ok(())
            }
            "read-open-cover-message" => Ok(()),
            other => Err(format!("unsupported Outlook web action {other}")),
        }
    }

    fn read(&self, name: &str) -> Result<&str, String> {
        if !self.has_control(name) {
            return Err(format!("named control {name} is missing"));
        }
        if mapped_action(name)? != "read-open-cover-message" {
            return Err(format!("named control {name} is not a read control"));
        }
        self.placed_messages
            .last()
            .map(String::as_str)
            .ok_or_else(|| format!("no Outlook web message is open on {}", self.page_url))
    }

    fn has_control(&self, name: &str) -> bool {
        self.controls.iter().any(|control| control == name)
    }
}

fn mapped_action(name: &str) -> Result<&'static str, String> {
    outlook_web_mapped_controls()
        .iter()
        .find(|control| control.name == name)
        .map(|control| control.action)
        .ok_or_else(|| format!("named control {name} is not mapped"))
}
