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

/// The boring subject OSL Mail asks for when the authored one would travel in
/// the clear. Same string the desktop send boundary offers as the replacement
/// (`apps/osl-hub/src/osl_mail.rs`), so the review names the same fix the send
/// refusal names.
pub const BORING_PROTECTED_SUBJECT: &str = "OSL protected message";

/// The command that actually hands the draft to the carrier. The review always
/// lists it last, so a warning can never appear after the send it is about.
pub const NAMED_SEND_COMMAND: &str = "Send";

/// Below this, a normalized subject is too short to be carrying the protected
/// text: "Hello" appearing inside a body is a greeting, not a leak.
const MIN_COPIED_SUBJECT_BYTES: usize = 16;

/// The one visible-subject warning. Task 1297 made the direct send command
/// refuse with this string; task 1298 shows the same string in the review, so
/// the user reads it before the send instead of after.
pub fn visible_subject_protection_warning() -> String {
    format!("OSL Mail subject is visible. Use \"{BORING_PROTECTED_SUBJECT}\" instead.")
}

/// Whether the visible subject would carry the protected text into the clear.
///
/// This is content-linked, not length-based: a long subject that says nothing
/// the body says is not a leak, and the boring replacement never warns about
/// itself.
pub fn visible_subject_copies_protected_text(subject: &str, protected_text: &str) -> bool {
    let subject = normalized_visible_subject_text(subject);
    if subject.len() < MIN_COPIED_SUBJECT_BYTES
        || subject == normalized_visible_subject_text(BORING_PROTECTED_SUBJECT)
    {
        return false;
    }
    let protected_text = normalized_visible_subject_text(protected_text);
    !protected_text.is_empty()
        && (subject == protected_text || protected_text.contains(subject.as_str()))
}

fn normalized_visible_subject_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// What the named Send command opens: the chosen send rule, the draft it is
/// about, and any warning that has to be read first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailSendReview {
    pub rule: EmailSendReviewRule,
    pub subject: String,
    pub body: String,
    pub visible_subject_warning: Option<String>,
    pub named_send_command: &'static str,
}

impl EmailSendReview {
    /// The review rows in the order the user reads them: every warning first,
    /// then the named Send command.
    pub fn rows(&self) -> Vec<String> {
        let mut rows: Vec<String> = self.visible_subject_warning.iter().cloned().collect();
        rows.push(self.named_send_command.to_owned());
        rows
    }

    /// Where the named Send command sits. Always the last row.
    pub fn named_send_row_index(&self) -> usize {
        self.rows().len() - 1
    }

    /// Where the visible-subject warning sits, when there is one.
    pub fn visible_subject_warning_row_index(&self) -> Option<usize> {
        self.visible_subject_warning.as_ref().map(|_| 0)
    }

    pub fn warns_about_visible_subject(&self) -> bool {
        self.visible_subject_warning.is_some()
    }
}

/// Open the send review the named Send command leads to.
///
/// The review is what stands between the draft and the carrier, so the
/// visible-subject warning is computed here rather than only at the send
/// boundary.
pub fn open_email_send_review(mode: EmailSendMode, subject: &str, body: &str) -> EmailSendReview {
    EmailSendReview {
        rule: EmailSendReviewRule { mode },
        subject: subject.to_owned(),
        body: body.to_owned(),
        visible_subject_warning: visible_subject_copies_protected_text(subject, body)
            .then(visible_subject_protection_warning),
        named_send_command: NAMED_SEND_COMMAND,
    }
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
