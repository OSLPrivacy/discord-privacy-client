use serde::{Deserialize, Serialize};

/// Shared row-authorship answer for Scrub-facing app rows.
///
/// `NotPublishedByApp` is a real evidenced answer. It is separate from missing
/// evidence, which still refuses the whole batch before any row is accepted.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedRowWhoWroteIt {
    Yours,
    Theirs,
    NotPublishedByApp,
}

impl SharedRowWhoWroteIt {
    pub const STATES: [Self; 3] = [Self::Yours, Self::Theirs, Self::NotPublishedByApp];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Theirs => "theirs",
            Self::NotPublishedByApp => "not_published_by_app",
        }
    }

    pub fn parse_name(value: &str) -> Result<Self, SharedRowWhoWroteItError> {
        match value {
            "yours" => Ok(Self::Yours),
            "theirs" => Ok(Self::Theirs),
            "not_published_by_app" => Ok(Self::NotPublishedByApp),
            other => Err(SharedRowWhoWroteItError::UnknownState(other.to_owned())),
        }
    }

    pub const fn is_published_by_app(self) -> bool {
        matches!(self, Self::Yours | Self::Theirs)
    }
}

impl core::fmt::Display for SharedRowWhoWroteIt {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SharedRowWhoWroteItError {
    UnknownState(String),
}

impl SharedRowWhoWroteItError {
    pub fn reason(&self) -> String {
        match self {
            Self::UnknownState(state) => {
                format!("OSL: unknown who-wrote-it answer {state}")
            }
        }
    }
}

impl core::fmt::Display for SharedRowWhoWroteItError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.reason())
    }
}

impl std::error::Error for SharedRowWhoWroteItError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedRowWhoWroteItEvidence {
    pub row_name: String,
    pub who_wrote_it: Option<SharedRowWhoWroteIt>,
}

impl SharedRowWhoWroteItEvidence {
    pub fn new(row_name: impl Into<String>, who_wrote_it: Option<SharedRowWhoWroteIt>) -> Self {
        Self {
            row_name: row_name.into(),
            who_wrote_it,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedRowWhoWroteItBatch {
    pub accepted_rows: usize,
    pub refusal: Option<String>,
}

pub fn accept_published_row_batch(
    rows: &[SharedRowWhoWroteItEvidence],
) -> SharedRowWhoWroteItBatch {
    if let Some(row) = rows.iter().find(|row| row.who_wrote_it.is_none()) {
        return SharedRowWhoWroteItBatch {
            accepted_rows: 0,
            refusal: Some(format!(
                "OSL: row {} has no who-wrote-it evidence",
                row.row_name
            )),
        };
    }

    if let Some(row) = rows
        .iter()
        .find(|row| row.who_wrote_it == Some(SharedRowWhoWroteIt::NotPublishedByApp))
    {
        return SharedRowWhoWroteItBatch {
            accepted_rows: 0,
            refusal: Some(format!(
                "OSL: row {} was not published by the app",
                row.row_name
            )),
        };
    }

    SharedRowWhoWroteItBatch {
        accepted_rows: rows
            .iter()
            .filter(|row| {
                row.who_wrote_it
                    .is_some_and(SharedRowWhoWroteIt::is_published_by_app)
            })
            .count(),
        refusal: None,
    }
}
