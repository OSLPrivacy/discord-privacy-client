//! Receiver-backed protected/normal display state for marked X rows.
//!
//! X's visible feed is evidence only.  The protected eye may display the
//! private text only when that exact text was recorded by the X receiving job;
//! it never derives protected text from a fresh (or substitute) feed read.

use std::collections::BTreeSet;

/// The two X surfaces that can contain a marked row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XRowKind {
    DirectMessage,
    PublicPost,
}

/// The text currently selected by the row's eye command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XEyeState {
    Protected,
    Normal,
}

/// A single row as recorded by the receiving job.
///
/// `protected_text` remains opaque here: writing it into eye state is a pure
/// exact copy, so Unicode, whitespace, and punctuation are not normalized.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XReceivingJobRow {
    pub kind: XRowKind,
    pub marker: String,
    pub normal_text: String,
    pub protected_text: String,
}

/// The immutable output of one X receiving-job run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XReceivingJobOutput {
    pub run_id: String,
    pub rows: Vec<XReceivingJobRow>,
}

/// What the X feed says it contains for a completed receiver run.
///
/// The run id binds this observation to the receiving job that recorded the
/// protected output.  A feed from any other run is a stand-in, even when its
/// visible text happens to look identical.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XReceivedFeed {
    pub receiving_job_run_id: String,
    pub rows: Vec<XReceivingJobRow>,
}

impl XReceivedFeed {
    pub fn recorded_by(output: &XReceivingJobOutput) -> Self {
        Self {
            receiving_job_run_id: output.run_id.clone(),
            rows: output.rows.clone(),
        }
    }
}

/// A row ready for the X overlay. `protected_text` is copied directly from the
/// receiving job record; `normal_text` is the normal X feed representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XEyeRowState {
    pub kind: XRowKind,
    pub marker: String,
    pub protected_text: String,
    pub normal_text: String,
    pub eye_state: XEyeState,
}

impl XEyeRowState {
    pub fn displayed_text(&self) -> &str {
        match self.eye_state {
            XEyeState::Protected => &self.protected_text,
            XEyeState::Normal => &self.normal_text,
        }
    }
}

/// All eye rows written after a successful receiver/feed binding check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XEyeStateStore {
    rows: Vec<XEyeRowState>,
    /// An entry is present only while the corresponding protected row remains
    /// authorized to enter protected display state.
    protected_keys: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XEyeStateError {
    EmptyReceivingJobRun,
    StandInFeed,
    EmptyMarker,
    DuplicateMarker,
    MarkedRowNotFound,
    ProtectedKeyRemoved(String),
}

impl XEyeStateError {
    /// Stable name surfaced to command callers for state-change refusals.
    pub fn refusal_name(&self) -> Option<&'static str> {
        match self {
            Self::ProtectedKeyRemoved(_) => Some("removed"),
            _ => None,
        }
    }
}

impl core::fmt::Display for XEyeStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyReceivingJobRun => f.write_str("X receiving job run id is empty"),
            Self::StandInFeed => f.write_str("X feed is not the receiving job's recorded output"),
            Self::EmptyMarker => f.write_str("X marked row has an empty marker"),
            Self::DuplicateMarker => f.write_str("X receiving job recorded a duplicate marker"),
            Self::MarkedRowNotFound => f.write_str("X marked row was not found"),
            Self::ProtectedKeyRemoved(marker) => {
                write!(f, "X protected key removed for marked row {marker}")
            }
        }
    }
}

impl std::error::Error for XEyeStateError {}

impl XEyeStateStore {
    /// Write protected and normal state only from one exact receiving-job
    /// output.  The observed feed is compared before writing any rows, which
    /// makes a stand-in feed fail closed rather than populate plausible text.
    pub fn write_from_receiving_job(
        output: &XReceivingJobOutput,
        feed: &XReceivedFeed,
    ) -> Result<Self, XEyeStateError> {
        if output.run_id.trim().is_empty() {
            return Err(XEyeStateError::EmptyReceivingJobRun);
        }
        if feed.receiving_job_run_id != output.run_id || feed.rows != output.rows {
            return Err(XEyeStateError::StandInFeed);
        }

        let mut markers = BTreeSet::new();
        let mut protected_keys = BTreeSet::new();
        let mut rows = Vec::with_capacity(output.rows.len());
        for recorded in &output.rows {
            if recorded.marker.trim().is_empty() {
                return Err(XEyeStateError::EmptyMarker);
            }
            if !markers.insert(recorded.marker.as_str()) {
                return Err(XEyeStateError::DuplicateMarker);
            }
            protected_keys.insert(recorded.marker.clone());
            rows.push(XEyeRowState {
                kind: recorded.kind,
                marker: recorded.marker.clone(),
                // These are deliberately direct clones rather than any display
                // transformation: the protected eye must show the recording.
                protected_text: recorded.protected_text.clone(),
                normal_text: recorded.normal_text.clone(),
                eye_state: XEyeState::Normal,
            });
        }
        Ok(Self {
            rows,
            protected_keys,
        })
    }

    pub fn rows(&self) -> &[XEyeRowState] {
        &self.rows
    }

    /// Remove the protected-display key for an exact marked row. The row's
    /// current display is intentionally unchanged; a later show request must
    /// fail before it can mutate any row state.
    pub fn remove_protected_key(&mut self, marker: &str) -> Result<(), XEyeStateError> {
        if !self.rows.iter().any(|row| row.marker == marker) {
            return Err(XEyeStateError::MarkedRowNotFound);
        }
        if !self.protected_keys.remove(marker) {
            return Err(XEyeStateError::ProtectedKeyRemoved(marker.to_owned()));
        }
        Ok(())
    }

    pub fn has_protected_key(&self, marker: &str) -> bool {
        self.protected_keys.contains(marker)
    }

    /// The command behind the marked row's eye. It changes presentation only;
    /// the original protected and normal strings remain stored unchanged.
    pub fn switch_marked_row(
        &mut self,
        marker: &str,
        eye_state: XEyeState,
    ) -> Result<&XEyeRowState, XEyeStateError> {
        if eye_state == XEyeState::Protected && !self.protected_keys.contains(marker) {
            if self.rows.iter().any(|row| row.marker == marker) {
                return Err(XEyeStateError::ProtectedKeyRemoved(marker.to_owned()));
            }
            return Err(XEyeStateError::MarkedRowNotFound);
        }

        let row = self
            .rows
            .iter_mut()
            .find(|row| row.marker == marker)
            .ok_or(XEyeStateError::MarkedRowNotFound)?;
        row.eye_state = eye_state;
        Ok(row)
    }

    /// The closed-eye control always restores the ordinary X carrier text.
    pub fn close_marked_row_eye(&mut self, marker: &str) -> Result<&XEyeRowState, XEyeStateError> {
        self.switch_marked_row(marker, XEyeState::Normal)
    }

    /// The open-eye control shows only the protected text recorded by the
    /// receiving job for this exact marked row while its key remains present.
    pub fn open_marked_row_eye(&mut self, marker: &str) -> Result<&XEyeRowState, XEyeStateError> {
        self.switch_marked_row(marker, XEyeState::Protected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_must_be_the_exact_receiving_job_record() {
        let output = XReceivingJobOutput {
            run_id: "x-receiver-1".into(),
            rows: vec![XReceivingJobRow {
                kind: XRowKind::DirectMessage,
                marker: "m".into(),
                normal_text: "cover".into(),
                protected_text: "private".into(),
            }],
        };
        let mut stand_in = XReceivedFeed::recorded_by(&output);
        stand_in.receiving_job_run_id = "another-receiver".into();
        assert_eq!(
            XEyeStateStore::write_from_receiving_job(&output, &stand_in),
            Err(XEyeStateError::StandInFeed)
        );
    }

    #[test]
    fn closed_and_open_eye_controls_switch_one_marked_fixture_row() {
        const MARKER: &str = "TASK1114-X-MARKED";
        const ORDINARY_X_CONTENT: &str = "ordinary X content TASK1114-X-MARKED";
        const PROTECTED_TEXT: &str = "protected X text: marigold\nkeeps exact whitespace";

        let output = XReceivingJobOutput {
            run_id: "task-1114-x-receiving-job-1".into(),
            rows: vec![XReceivingJobRow {
                kind: XRowKind::DirectMessage,
                marker: MARKER.into(),
                normal_text: ORDINARY_X_CONTENT.into(),
                protected_text: PROTECTED_TEXT.into(),
            }],
        };
        let feed = XReceivedFeed::recorded_by(&output);
        let mut state = XEyeStateStore::write_from_receiving_job(&output, &feed).unwrap();
        fn count(rows: &[XEyeRowState], text: &str) -> usize {
            rows.iter()
                .filter(|row| row.displayed_text() == text)
                .count()
        }

        state.close_marked_row_eye(MARKER).unwrap();
        assert_eq!(count(state.rows(), ORDINARY_X_CONTENT), 1);
        assert_eq!(count(state.rows(), PROTECTED_TEXT), 0);
        println!(
            "TASK1114 closed_eye marked_fixture_rows={}",
            state.rows().len()
        );
        println!(
            "TASK1114 closed_eye ordinary_x_content_rows={}",
            count(state.rows(), ORDINARY_X_CONTENT)
        );
        println!(
            "TASK1114 closed_eye protected_text_rows={}",
            count(state.rows(), PROTECTED_TEXT)
        );

        state.open_marked_row_eye(MARKER).unwrap();
        assert_eq!(count(state.rows(), ORDINARY_X_CONTENT), 0);
        assert_eq!(count(state.rows(), PROTECTED_TEXT), 1);
        println!(
            "TASK1114 open_eye marked_fixture_rows={}",
            state.rows().len()
        );
        println!(
            "TASK1114 open_eye ordinary_x_content_rows={}",
            count(state.rows(), ORDINARY_X_CONTENT)
        );
        println!(
            "TASK1114 open_eye protected_text_rows={}",
            count(state.rows(), PROTECTED_TEXT)
        );

        state.close_marked_row_eye(MARKER).unwrap();
        assert_eq!(count(state.rows(), ORDINARY_X_CONTENT), 1);
        assert_eq!(count(state.rows(), PROTECTED_TEXT), 0);
        println!(
            "TASK1114 closed_again ordinary_x_content_rows={}",
            count(state.rows(), ORDINARY_X_CONTENT)
        );
        println!(
            "TASK1114 closed_again protected_text_rows={}",
            count(state.rows(), PROTECTED_TEXT)
        );
    }
}
