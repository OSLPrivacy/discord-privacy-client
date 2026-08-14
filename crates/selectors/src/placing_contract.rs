use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MessagingSurface {
    Discord,
    Telegram,
    Signal,
    WhatsApp,
}

impl MessagingSurface {
    pub const ALL: [Self; 4] = [Self::Discord, Self::Telegram, Self::Signal, Self::WhatsApp];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discord => "Discord",
            Self::Telegram => "Telegram",
            Self::Signal => "Signal",
            Self::WhatsApp => "WhatsApp",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationBox {
    sent_count: usize,
    draft: String,
    pressed_controls: usize,
}

impl ConversationBox {
    pub fn new(sent_count: usize) -> Self {
        Self {
            sent_count,
            draft: String::new(),
            pressed_controls: 0,
        }
    }

    pub fn sent_count(&self) -> usize {
        self.sent_count
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    pub fn pressed_controls(&self) -> usize {
        self.pressed_controls
    }

    fn place_draft(&mut self, marked_text: &str) {
        self.draft.clear();
        self.draft.push_str(marked_text);
    }
}

pub type ConversationSet = BTreeMap<MessagingSurface, ConversationBox>;

pub fn seeded_task_3421_conversations() -> ConversationSet {
    BTreeMap::from([
        (MessagingSurface::Discord, ConversationBox::new(12)),
        (MessagingSurface::Telegram, ConversationBox::new(7)),
        (MessagingSurface::Signal, ConversationBox::new(19)),
        (MessagingSurface::WhatsApp, ConversationBox::new(4)),
    ])
}

pub trait MarkedMessagePlacingJob {
    fn place_marked_message(&self, conversation: &mut ConversationBox, marked_text: &str);
}

pub struct DraftOnlyPlacingJob;

impl MarkedMessagePlacingJob for DraftOnlyPlacingJob {
    fn place_marked_message(&self, conversation: &mut ConversationBox, marked_text: &str) {
        conversation.place_draft(marked_text);
    }
}

pub struct NoopPlacingJob;

impl MarkedMessagePlacingJob for NoopPlacingJob {
    fn place_marked_message(&self, _conversation: &mut ConversationBox, _marked_text: &str) {}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementObservation {
    pub surface: MessagingSurface,
    pub sent_before: usize,
    pub sent_after: usize,
    pub draft_after: String,
    pub pressed_controls_after: usize,
}

pub fn place_marked_message_in_all_conversations(
    conversations: &mut ConversationSet,
    marked_text: &str,
    job: &dyn MarkedMessagePlacingJob,
) -> Vec<PlacementObservation> {
    MessagingSurface::ALL
        .into_iter()
        .map(|surface| {
            let conversation = conversations
                .get_mut(&surface)
                .expect("task 3421 fixture includes every messaging surface");
            let sent_before = conversation.sent_count();
            job.place_marked_message(conversation, marked_text);
            PlacementObservation {
                surface,
                sent_before,
                sent_after: conversation.sent_count(),
                draft_after: conversation.draft().to_owned(),
                pressed_controls_after: conversation.pressed_controls(),
            }
        })
        .collect()
}

pub fn task_3421_finish_line_holds(
    observations: &[PlacementObservation],
    marked_text: &str,
) -> bool {
    observations.len() == MessagingSurface::ALL.len()
        && observations.iter().all(|observation| {
            observation.sent_before == observation.sent_after
                && observation.draft_after == marked_text
                && observation.pressed_controls_after == 0
        })
}
