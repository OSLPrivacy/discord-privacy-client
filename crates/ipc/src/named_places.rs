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
pub const TASK_4204_REAL_RECORDS_JSON: &str =
    include_str!("../../../apps/osl-hub/data/named_places_4203.json");

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

pub const TASK_4204_REQUIRED_PARTS: [&str; 5] =
    ["app", "placeName", "state", "sourceTask", "provingCheck"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task4204Record {
    pub app: String,
    pub place_name: String,
    pub state: PlaceDisposition,
    pub source_task: String,
    pub proving_check: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task4204Fault {
    MissingPart { row: usize, part: &'static str },
    BadState { row: usize, state: String },
    UnknownSourceTask { row: usize, source_task: String },
    InventedPlace { place_name: String },
    DuplicateAppPlace { app: String, place_name: String },
}

impl Task4204Fault {
    pub fn report_line(&self) -> String {
        match self {
            Self::MissingPart { row, part } => {
                format!("TASK4204_FAULT=missing_part row={row} part={part}")
            }
            Self::BadState { row, state } => {
                format!("TASK4204_FAULT=bad_state row={row} state={state}")
            }
            Self::UnknownSourceTask { row, source_task } => {
                format!("TASK4204_FAULT=unknown_source_task row={row} source_task={source_task}")
            }
            Self::InventedPlace { place_name } => {
                format!("TASK4204_FAULT=invented_place place={place_name}")
            }
            Self::DuplicateAppPlace { app, place_name } => {
                format!("TASK4204_FAULT=duplicate_app_place app={app} place={place_name}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task4204Report {
    pub row_count: usize,
    pub records: Vec<Task4204Record>,
    pub faults: Vec<Task4204Fault>,
}

impl Task4204Report {
    pub fn fault_count(&self) -> usize {
        self.faults.len()
    }

    pub fn is_green(&self) -> bool {
        self.row_count == EXPECTED_ROW_COUNT && self.faults.is_empty()
    }
}

pub fn check_task_4204_real_records() -> Result<Task4204Report, String> {
    check_task_4204_records_json(TASK_4204_REAL_RECORDS_JSON)
}

pub fn check_task_4204_records_json(input: &str) -> Result<Task4204Report, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|error| format!("parse task 4204 records: {error}"))?;
    let rows = value
        .as_array()
        .ok_or_else(|| "task 4204 records must be a JSON array".to_owned())?;

    let source_tasks = EXPECTED_SOURCE_TASKS.into_iter().collect::<BTreeSet<_>>();
    let places_from_source_tasks = EXPECTED_PLACE_ROWS
        .into_iter()
        .map(|(_, place, _)| place)
        .collect::<BTreeSet<_>>();
    let mut seen_app_places = BTreeSet::new();
    let mut records = Vec::new();
    let mut faults = Vec::new();

    for (index, row) in rows.iter().enumerate() {
        let row_number = index + 1;
        let Some(object) = row.as_object() else {
            for part in TASK_4204_REQUIRED_PARTS {
                faults.push(Task4204Fault::MissingPart {
                    row: row_number,
                    part,
                });
            }
            continue;
        };

        let app = required_task_4204_string(object, row_number, "app", &mut faults);
        let place_name = required_task_4204_string(object, row_number, "placeName", &mut faults);
        let state = required_task_4204_string(object, row_number, "state", &mut faults);
        let source_task = required_task_4204_string(object, row_number, "sourceTask", &mut faults);
        let proving_check =
            required_task_4204_string(object, row_number, "provingCheck", &mut faults);

        let (Some(app), Some(place_name), Some(state), Some(source_task), Some(proving_check)) =
            (app, place_name, state, source_task, proving_check)
        else {
            continue;
        };

        let state = match state.as_str() {
            "approved" => PlaceDisposition::Approved,
            "look-only-refused" | "look-only refused" => PlaceDisposition::LookOnlyRefused,
            _ => {
                faults.push(Task4204Fault::BadState {
                    row: row_number,
                    state,
                });
                continue;
            }
        };

        if !source_tasks.contains(source_task.as_str()) {
            faults.push(Task4204Fault::UnknownSourceTask {
                row: row_number,
                source_task: source_task.clone(),
            });
        }
        if !places_from_source_tasks.contains(place_name.as_str()) {
            faults.push(Task4204Fault::InventedPlace {
                place_name: place_name.clone(),
            });
        }
        if !seen_app_places.insert((app.clone(), place_name.clone())) {
            faults.push(Task4204Fault::DuplicateAppPlace {
                app: app.clone(),
                place_name: place_name.clone(),
            });
        }

        records.push(Task4204Record {
            app,
            place_name,
            state,
            source_task,
            proving_check,
        });
    }

    Ok(Task4204Report {
        row_count: rows.len(),
        records,
        faults,
    })
}

fn required_task_4204_string(
    object: &serde_json::Map<String, serde_json::Value>,
    row: usize,
    part: &'static str,
    faults: &mut Vec<Task4204Fault>,
) -> Option<String> {
    let value = object
        .get(part)
        .and_then(|value| value.as_str())
        .map(str::trim);
    match value {
        Some(value) if !value.is_empty() => Some(value.to_owned()),
        _ => {
            faults.push(Task4204Fault::MissingPart { row, part });
            None
        }
    }
}
