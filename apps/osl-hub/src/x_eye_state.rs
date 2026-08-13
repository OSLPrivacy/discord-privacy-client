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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XEyeStateError {
    EmptyReceivingJobRun,
    StandInFeed,
    EmptyMarker,
    DuplicateMarker,
    MarkedRowNotFound,
}

impl core::fmt::Display for XEyeStateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::EmptyReceivingJobRun => "X receiving job run id is empty",
            Self::StandInFeed => "X feed is not the receiving job's recorded output",
            Self::EmptyMarker => "X marked row has an empty marker",
            Self::DuplicateMarker => "X receiving job recorded a duplicate marker",
            Self::MarkedRowNotFound => "X marked row was not found",
        })
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
        let mut rows = Vec::with_capacity(output.rows.len());
        for recorded in &output.rows {
            if recorded.marker.trim().is_empty() {
                return Err(XEyeStateError::EmptyMarker);
            }
            if !markers.insert(recorded.marker.as_str()) {
                return Err(XEyeStateError::DuplicateMarker);
            }
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
        Ok(Self { rows })
    }

    pub fn rows(&self) -> &[XEyeRowState] {
        &self.rows
    }

    /// Add one row only after the receiver record and the X reader's observed
    /// row are bound to the same receiving-job run.  This is the incremental
    /// path used by the shipping receive job: an empty eye therefore remains
    /// empty until a real arrived row has been accepted.
    pub fn append_from_receiving_job(
        &mut self,
        output: &XReceivingJobOutput,
        feed: &XReceivedFeed,
    ) -> Result<(), XEyeStateError> {
        let incoming = Self::write_from_receiving_job(output, feed)?;
        for row in incoming.rows {
            if self
                .rows
                .iter()
                .any(|existing| existing.marker == row.marker)
            {
                return Err(XEyeStateError::DuplicateMarker);
            }
            self.rows.push(row);
        }
        Ok(())
    }

    /// The command behind the marked row's eye. It changes presentation only;
    /// the original protected and normal strings remain stored unchanged.
    pub fn switch_marked_row(
        &mut self,
        marker: &str,
        eye_state: XEyeState,
    ) -> Result<&XEyeRowState, XEyeStateError> {
        let row = self
            .rows
            .iter_mut()
            .find(|row| row.marker == marker)
            .ok_or(XEyeStateError::MarkedRowNotFound)?;
        row.eye_state = eye_state;
        Ok(row)
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
}
