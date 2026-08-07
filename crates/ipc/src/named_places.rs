//! Task 4203: the explicit named-place ledger.

use serde::Deserialize;
use std::collections::BTreeSet;

pub const EXPECTED_ROW_COUNT: usize = 13;
pub const EXPECTED_APPROVED_COUNT: usize = 11;
pub const EXPECTED_LOOK_ONLY_REFUSED_COUNT: usize = 2;

pub const EXPECTED_SOURCE_TASKS: [&str; EXPECTED_ROW_COUNT] = [
    "1027", "1028", "1029a", "1055", "1089a", "1089b", "1089c", "1089d", "1125", "1127", "1154",
    "1155", "1190",
];

pub const EXPECTED_PLACE_ROWS: [(&str, &str, PlaceDisposition); EXPECTED_ROW_COUNT] = [
    ("1027", "Telegram supergroup", PlaceDisposition::Approved),
    (
        "1028",
        "Telegram saved messages",
        PlaceDisposition::Approved,
    ),
    ("1029a", "Signal story", PlaceDisposition::Approved),
    ("1055", "WhatsApp community", PlaceDisposition::Approved),
    (
        "1089a",
        "WhatsApp community group",
        PlaceDisposition::Approved,
    ),
    (
        "1089b",
        "WhatsApp broadcast list",
        PlaceDisposition::Approved,
    ),
    (
        "1089c",
        "X group direct message",
        PlaceDisposition::Approved,
    ),
    ("1089d", "X reply", PlaceDisposition::Approved),
    ("1125", "Instagram comment", PlaceDisposition::Approved),
    ("1127", "Instagram story", PlaceDisposition::Approved),
    ("1154", "Messenger community", PlaceDisposition::Approved),
    ("1155", "Telegram story", PlaceDisposition::LookOnlyRefused),
    ("1190", "WhatsApp Status", PlaceDisposition::LookOnlyRefused),
];

const CANONICAL_JSON: &str = include_str!("../../../data/allowed-places-4203.json");

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlaceDisposition {
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "look-only refused")]
    LookOnlyRefused,
}

impl PlaceDisposition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::LookOnlyRefused => "look-only refused",
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NamedPlace {
    pub source_task: String,
    pub place: String,
    pub disposition: PlaceDisposition,
}

#[derive(Debug, Deserialize)]
struct NamedPlacesDocument {
    schema: String,
    places: Vec<NamedPlace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedPlacesReport {
    pub row_count: usize,
    pub approved_count: usize,
    pub look_only_refused_count: usize,
    pub invalid_source_tasks: Vec<String>,
    pub missing_rows: Vec<NamedPlace>,
    pub unexpected_rows: Vec<NamedPlace>,
}

impl NamedPlacesReport {
    pub fn is_valid(&self) -> bool {
        self.row_count == EXPECTED_ROW_COUNT
            && self.approved_count == EXPECTED_APPROVED_COUNT
            && self.look_only_refused_count == EXPECTED_LOOK_ONLY_REFUSED_COUNT
            && self.invalid_source_tasks.is_empty()
            && self.missing_rows.is_empty()
            && self.unexpected_rows.is_empty()
    }
}

pub fn canonical_named_places() -> Result<Vec<NamedPlace>, String> {
    named_places_from_json(CANONICAL_JSON)
}

pub fn named_places_from_json(input: &str) -> Result<Vec<NamedPlace>, String> {
    let document: NamedPlacesDocument =
        serde_json::from_str(input).map_err(|error| format!("OSL places JSON refused: {error}"))?;
    if document.schema != "osl-allowed-places-v1" {
        return Err(format!(
            "OSL places JSON refused: unknown schema '{}'",
            document.schema
        ));
    }
    Ok(document.places)
}

pub fn validate_named_places(rows: &[NamedPlace]) -> NamedPlacesReport {
    let source_tasks: BTreeSet<&str> = EXPECTED_SOURCE_TASKS.into_iter().collect();
    let expected: BTreeSet<NamedPlace> = EXPECTED_PLACE_ROWS
        .into_iter()
        .map(|(source_task, place, disposition)| NamedPlace {
            source_task: source_task.to_string(),
            place: place.to_string(),
            disposition,
        })
        .collect();
    let actual: BTreeSet<NamedPlace> = rows.iter().cloned().collect();

    NamedPlacesReport {
        row_count: rows.len(),
        approved_count: rows
            .iter()
            .filter(|row| row.disposition == PlaceDisposition::Approved)
            .count(),
        look_only_refused_count: rows
            .iter()
            .filter(|row| row.disposition == PlaceDisposition::LookOnlyRefused)
            .count(),
        invalid_source_tasks: rows
            .iter()
            .filter(|row| !source_tasks.contains(row.source_task.as_str()))
            .map(|row| format!("{}:{}", row.source_task, row.place))
            .collect(),
        missing_rows: expected.difference(&actual).cloned().collect(),
        unexpected_rows: actual.difference(&expected).cloned().collect(),
    }
}
