#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSendMode {
    Manual,
    DoubleEnter,
    ExperimentalSingleEnter,
    Instant,
    MatchTyping,
}

impl EmailSendMode {
    pub const ALL: [Self; 5] = [
        Self::Manual,
        Self::DoubleEnter,
        Self::ExperimentalSingleEnter,
        Self::Instant,
        Self::MatchTyping,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::DoubleEnter => "double_enter",
            Self::ExperimentalSingleEnter => "experimental_single_enter",
            Self::Instant => "instant",
            Self::MatchTyping => "match_typing",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::DoubleEnter => "Double Enter",
            Self::ExperimentalSingleEnter => "Experimental Single Enter",
            Self::Instant => "Instant",
            Self::MatchTyping => "Match typing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailComposerInput {
    Enter,
    NamedSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailComposerInputEffect {
    InsertLine,
    OpenSendReview,
}

impl EmailComposerInputEffect {
    pub const fn name(self) -> &'static str {
        match self {
            Self::InsertLine => "insert_line",
            Self::OpenSendReview => "open_send_review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmailSendReviewRule {
    pub mode: EmailSendMode,
}

impl EmailSendReviewRule {
    pub const fn id(self) -> &'static str {
        self.mode.id()
    }

    pub const fn name(self) -> &'static str {
        self.mode.name()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailComposerInputCheck {
    pub body: String,
    pub effect: EmailComposerInputEffect,
    pub send_review_rule: Option<EmailSendReviewRule>,
}

pub fn apply_email_composer_input(
    mode: EmailSendMode,
    body: &str,
    input: EmailComposerInput,
) -> EmailComposerInputCheck {
    match input {
        EmailComposerInput::Enter => EmailComposerInputCheck {
            body: format!("{body}\n"),
            effect: EmailComposerInputEffect::InsertLine,
            send_review_rule: None,
        },
        EmailComposerInput::NamedSend => EmailComposerInputCheck {
            body: body.to_owned(),
            effect: EmailComposerInputEffect::OpenSendReview,
            send_review_rule: Some(EmailSendReviewRule { mode }),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailDraftReadbackState {
    body: String,
    matching_readback_count: usize,
    send_count: usize,
}

impl EmailDraftReadbackState {
    pub fn place(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            matching_readback_count: 0,
            send_count: 0,
        }
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub const fn matching_readback_count(&self) -> usize {
        self.matching_readback_count
    }

    pub const fn send_count(&self) -> usize {
        self.send_count
    }

    pub fn readback(&mut self, returned_readback: &str) -> Result<EmailDraftReadbackProof, String> {
        if returned_readback != self.body {
            return Err("readback mismatch".to_owned());
        }
        self.matching_readback_count = self.matching_readback_count.saturating_add(1);
        Ok(EmailDraftReadbackProof {
            body: self.body.clone(),
            returned_readback: returned_readback.to_owned(),
            matching_readback_count: self.matching_readback_count,
        })
    }

    pub fn send_after_readback(&mut self, returned_readback: &str) -> Result<(), String> {
        self.readback(returned_readback)?;
        self.send_count = self.send_count.saturating_add(1);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailDraftReadbackProof {
    pub body: String,
    pub returned_readback: String,
    pub matching_readback_count: usize,
}
