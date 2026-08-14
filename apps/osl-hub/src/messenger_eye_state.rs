//! Protected/normal eye state for one received Messenger row.
//!
//! The protected text is read through [`MessengerRowFeed`].  The production
//! feed is intentionally only a view over the live receiving job's recorded
//! output; it does not decode the carrier again or accept caller-supplied text.

use crate::messenger_delivery::{MessengerReceivingJobOutput, MessengerTestAccount};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessengerRowEyeState {
    Normal,
    Protected,
}

impl MessengerRowEyeState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Protected => "protected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessengerVisibleRow {
    pub marked_cover: String,
    pub state: MessengerRowEyeState,
    pub text: String,
}

/// Source boundary used by the direct command.
///
/// Implementations return an owned record so the writer cannot accidentally
/// retain a borrow into a changing browser/receiver feed.
pub trait MessengerRowFeed {
    fn recorded_output(
        &self,
        receiver: &MessengerTestAccount,
        marked_cover: &str,
    ) -> Result<MessengerReceivingJobOutput, String>;
}

/// Production feed: the receiving job's accepted output, byte for byte.
pub struct ReceivingJobMessengerRowFeed;

impl MessengerRowFeed for ReceivingJobMessengerRowFeed {
    fn recorded_output(
        &self,
        receiver: &MessengerTestAccount,
        marked_cover: &str,
    ) -> Result<MessengerReceivingJobOutput, String> {
        receiver
            .receiving_job_output_for_marked_cover(marked_cover)
            .cloned()
    }
}

#[derive(Default)]
pub struct MessengerRowStateWriter {
    rows: Vec<MessengerVisibleRow>,
}

impl MessengerRowStateWriter {
    pub fn rows(&self) -> &[MessengerVisibleRow] {
        &self.rows
    }

    pub fn row(&self, marked_cover: &str) -> Option<&MessengerVisibleRow> {
        self.rows
            .iter()
            .find(|row| row.marked_cover.as_bytes() == marked_cover.as_bytes())
    }

    /// Control behind Messenger's closed-eye button. The row stays marked,
    /// but its ordinary carrier text replaces the protected text on screen.
    pub fn close_marked_row_eye(
        &mut self,
        receiver: &MessengerTestAccount,
        feed: &dyn MessengerRowFeed,
        marked_cover: &str,
    ) -> Result<MessengerVisibleRow, String> {
        self.write_marked_row_state(receiver, feed, marked_cover, MessengerRowEyeState::Normal)
    }

    /// Control behind Messenger's open-eye button. Only the protected text
    /// recorded for this exact marked row is selected for display.
    pub fn open_marked_row_eye(
        &mut self,
        receiver: &MessengerTestAccount,
        feed: &dyn MessengerRowFeed,
        marked_cover: &str,
    ) -> Result<MessengerVisibleRow, String> {
        self.write_marked_row_state(
            receiver,
            feed,
            marked_cover,
            MessengerRowEyeState::Protected,
        )
    }

    /// Direct command body that writes one marked Messenger row's eye state.
    ///
    /// Normal shows the public carrier. Protected shows only the exact text in
    /// the receiving-job output returned by the feed. Repeated commands update
    /// the same marked row rather than adding duplicate UI rows.
    pub fn write_marked_row_state(
        &mut self,
        receiver: &MessengerTestAccount,
        feed: &dyn MessengerRowFeed,
        marked_cover: &str,
        state: MessengerRowEyeState,
    ) -> Result<MessengerVisibleRow, String> {
        let output = feed.recorded_output(receiver, marked_cover)?;
        if output.marked_cover.as_bytes() != marked_cover.as_bytes() {
            return Err("Messenger row feed returned a different marked row".to_owned());
        }

        let text = match state {
            MessengerRowEyeState::Normal => output.marked_cover.clone(),
            MessengerRowEyeState::Protected => output.protected_text,
        };
        let written = MessengerVisibleRow {
            marked_cover: output.marked_cover,
            state,
            text,
        };

        match self
            .rows
            .iter_mut()
            .find(|row| row.marked_cover.as_bytes() == marked_cover.as_bytes())
        {
            Some(row) => *row = written.clone(),
            None => self.rows.push(written.clone()),
        }
        Ok(written)
    }
}
