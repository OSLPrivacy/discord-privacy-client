use std::collections::BTreeSet;

use serde::Serialize;

pub const TASK_1243_PROTON_MARKER: &str = "OSL-PROTON-1243";
pub const TASK_1244_PROTON_WORDS: &str = "OSL-PROTON-1244 words";

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtonFakePageControlKind {
    Compose,
    Place,
    Readback,
    Send,
}

impl ProtonFakePageControlKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Compose => "Compose",
            Self::Place => "Place",
            Self::Readback => "Readback",
            Self::Send => "Send",
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Compose => "proton-compose",
            Self::Place => "proton-place",
            Self::Readback => "proton-readback",
            Self::Send => "proton-send",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonFakePageControl {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ProtonFakePageControlKind,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonFakePageMessage {
    pub marker: String,
    pub words: String,
    pub sent: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtonFakePageSendReceipt {
    pub words: String,
    pub sent_count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProtonFakePageConnection {
    controls: BTreeSet<ProtonFakePageControlKind>,
    placed_messages: Vec<ProtonFakePageMessage>,
    sent_count: usize,
}

impl Default for ProtonFakePageConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl ProtonFakePageConnection {
    pub fn new() -> Self {
        Self {
            controls: BTreeSet::from([
                ProtonFakePageControlKind::Compose,
                ProtonFakePageControlKind::Place,
                ProtonFakePageControlKind::Readback,
                ProtonFakePageControlKind::Send,
            ]),
            placed_messages: Vec::new(),
            sent_count: 0,
        }
    }

    pub fn controls(&self) -> Vec<ProtonFakePageControl> {
        self.controls
            .iter()
            .copied()
            .map(|kind| ProtonFakePageControl {
                id: kind.id(),
                name: kind.name(),
                kind,
            })
            .collect()
    }

    pub fn placed_message_count(&self) -> usize {
        self.placed_messages.len()
    }

    pub fn sent_count(&self) -> usize {
        self.sent_count
    }

    pub fn compose_marked_cover_words(&self) -> Result<String, String> {
        self.require_control(ProtonFakePageControlKind::Compose)?;
        Ok(TASK_1244_PROTON_WORDS.to_owned())
    }

    pub fn place_marked_cover_message(
        &mut self,
        words: &str,
    ) -> Result<ProtonFakePageMessage, String> {
        self.require_control(ProtonFakePageControlKind::Place)?;
        if words != TASK_1244_PROTON_WORDS {
            return Err("Proton cover message is not the exact OSL-PROTON-1244 words".to_owned());
        }
        let message = ProtonFakePageMessage {
            marker: TASK_1244_PROTON_WORDS.to_owned(),
            words: words.to_owned(),
            sent: false,
        };
        self.placed_messages.push(message.clone());
        Ok(message)
    }

    pub fn read_marked_words(&self, marker: &str) -> Result<String, String> {
        self.require_control(ProtonFakePageControlKind::Readback)?;
        let matches = self
            .placed_messages
            .iter()
            .filter(|message| message.marker == marker)
            .collect::<Vec<_>>();
        let [message] = matches.as_slice() else {
            return Err(
                "Proton marked message read requires exactly one placed message".to_owned(),
            );
        };
        Ok(message.words.clone())
    }

    pub fn readback_marked_words(&self) -> Result<String, String> {
        self.read_marked_words(TASK_1244_PROTON_WORDS)
    }

    pub fn send_placed_message(&mut self) -> Result<usize, String> {
        self.send_placed_message_with_words()
            .map(|receipt| receipt.sent_count)
    }

    pub fn send_placed_message_with_words(&mut self) -> Result<ProtonFakePageSendReceipt, String> {
        self.require_control(ProtonFakePageControlKind::Send)?;
        let Some(message) = self
            .placed_messages
            .iter_mut()
            .find(|message| !message.sent)
        else {
            return Err("Proton send requires one unsent placed message".to_owned());
        };
        message.sent = true;
        self.sent_count += 1;
        Ok(ProtonFakePageSendReceipt {
            words: message.words.clone(),
            sent_count: self.sent_count,
        })
    }

    pub fn remove_control(&mut self, kind: ProtonFakePageControlKind) -> Result<(), String> {
        if kind == ProtonFakePageControlKind::Send {
            return Err(
                "Proton Send control cannot be removed while mapped sending is required".to_owned(),
            );
        }
        self.controls.remove(&kind);
        Ok(())
    }

    fn require_control(&self, kind: ProtonFakePageControlKind) -> Result<(), String> {
        if self.controls.contains(&kind) {
            Ok(())
        } else {
            Err(format!("Proton {} control is not mapped", kind.name()))
        }
    }
}
