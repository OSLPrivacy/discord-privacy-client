pub const AOL_TASK_1256_MARKED_WORDS: &str = "OSL-AOL-1256 cover message";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolFakePageControl {
    pub name: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolFakePageMessage {
    pub body: String,
    pub marked: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AolFakePageFixture {
    placed_messages: Vec<AolFakePageMessage>,
    sent_email_count: usize,
}

impl AolFakePageFixture {
    pub fn task_1256() -> Self {
        Self {
            placed_messages: Vec::new(),
            sent_email_count: 0,
        }
    }

    pub fn mapped_controls(&self) -> [AolFakePageControl; 3] {
        [
            AolFakePageControl { name: "Place" },
            AolFakePageControl { name: "Read" },
            AolFakePageControl { name: "Send" },
        ]
    }

    pub fn placed_message_count(&self) -> usize {
        self.placed_messages.len()
    }

    pub fn marked_placed_message_count(&self) -> usize {
        self.placed_messages
            .iter()
            .filter(|message| message.marked && message.body == AOL_TASK_1256_MARKED_WORDS)
            .count()
    }

    pub fn sent_email_count(&self) -> usize {
        self.sent_email_count
    }

    pub fn place(&mut self) -> Result<(), String> {
        require_aol_fake_page_control("Place")?;
        self.placed_messages.push(AolFakePageMessage {
            body: AOL_TASK_1256_MARKED_WORDS.to_owned(),
            marked: true,
        });
        Ok(())
    }

    pub fn read(&self) -> Result<String, String> {
        require_aol_fake_page_control("Read")?;
        self.placed_messages
            .iter()
            .find(|message| message.marked && message.body == AOL_TASK_1256_MARKED_WORDS)
            .map(|message| message.body.clone())
            .ok_or_else(|| "AOL fake page has no marked cover message".to_owned())
    }

    pub fn send(&mut self) -> Result<(), String> {
        require_aol_fake_page_control("Send")?;
        if self.marked_placed_message_count() == 0 {
            return Err("AOL fake page has no placed cover message to send".to_owned());
        }
        self.sent_email_count += 1;
        Ok(())
    }

    pub fn remove_control(&mut self, name: &str) -> Result<(), String> {
        if name == "Send" {
            return Err("AOL fake page Send control is required".to_owned());
        }
        require_aol_fake_page_control(name)?;
        Err("AOL fake page mapped controls are fixed".to_owned())
    }
}

fn require_aol_fake_page_control(name: &str) -> Result<(), String> {
    match name {
        "Place" | "Read" | "Send" => Ok(()),
        _ => Err("unknown AOL fake page control".to_owned()),
    }
}
