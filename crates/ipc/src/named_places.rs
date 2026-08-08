//! Task 4203: the explicit named-place ledger.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const EXPECTED_ROW_COUNT: usize = 13;
pub const EXPECTED_APPROVED_COUNT: usize = 11;
pub const EXPECTED_LOOK_ONLY_REFUSED_COUNT: usize = 2;

pub const EXPECTED_SOURCE_TASKS: [&str; EXPECTED_ROW_COUNT] = [
    "1027", "1028", "1029a", "1055", "1089a", "1089b", "1089c", "1089d", "1125", "1127", "1154",
    "1155", "1190",
];

pub const EXPECTED_PLACE_ROWS: [(&str, &str, Option<&str>, Option<&str>, PlaceDisposition);
    EXPECTED_ROW_COUNT] = [
    (
        "1027",
        "Telegram supergroup",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1028",
        "Telegram saved messages",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1029a",
        "Signal story",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1055",
        "WhatsApp community",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1089a",
        "WhatsApp community group",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1089b",
        "WhatsApp broadcast list",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1089c",
        "X group direct message",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    ("1089d", "X reply", None, None, PlaceDisposition::Approved),
    (
        "1125",
        "Instagram comment",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1127",
        "Instagram story",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1154",
        "Messenger community",
        None,
        None,
        PlaceDisposition::Approved,
    ),
    (
        "1155",
        "Telegram story",
        Some("1029a"),
        Some("4218"),
        PlaceDisposition::LookOnlyRefused,
    ),
    (
        "1190",
        "WhatsApp Status",
        Some("1089d"),
        Some("4220"),
        PlaceDisposition::LookOnlyRefused,
    ),
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
    pub research_task: Option<String>,
    pub build_task: Option<String>,
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
        .map(
            |(source_task, place, research_task, build_task, disposition)| NamedPlace {
                source_task: source_task.to_string(),
                place: place.to_string(),
                research_task: research_task.map(str::to_string),
                build_task: build_task.map(str::to_string),
                disposition,
            },
        )
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoTaskStatus {
    pub task_id: String,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookOnlyWayToYesRow {
    pub place: String,
    pub research_task: Option<String>,
    pub build_task: Option<String>,
    pub research_task_exists: bool,
    pub build_task_exists: bool,
    pub research_task_done: bool,
    pub build_task_done: bool,
}

impl LookOnlyWayToYesRow {
    pub fn has_named_way_to_yes(&self) -> bool {
        task_name_is_filled(&self.research_task) && task_name_is_filled(&self.build_task)
    }

    pub fn has_existing_research_and_build_tasks(&self) -> bool {
        self.has_named_way_to_yes() && self.research_task_exists && self.build_task_exists
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookOnlyWayToYesReport {
    pub look_only_rows: Vec<LookOnlyWayToYesRow>,
    pub refused_rows_with_no_named_way_to_yes: usize,
    pub refused_rows_with_done_research_task: usize,
    pub refused_rows_with_done_build_task: usize,
    pub missing_task_numbers: Vec<String>,
}

impl LookOnlyWayToYesReport {
    pub fn look_only_row_count(&self) -> usize {
        self.look_only_rows.len()
    }

    pub fn rows_with_existing_research_and_build_tasks(&self) -> usize {
        self.look_only_rows
            .iter()
            .filter(|row| row.has_existing_research_and_build_tasks())
            .count()
    }

    pub fn is_valid(&self) -> bool {
        self.refused_rows_with_no_named_way_to_yes == 0
            && self.refused_rows_with_done_research_task == 0
            && self.refused_rows_with_done_build_task == 0
            && self.missing_task_numbers.is_empty()
            && self.look_only_rows.iter().all(|row| {
                row.has_existing_research_and_build_tasks()
                    && !row.research_task_done
                    && !row.build_task_done
            })
    }
}

pub fn validate_look_only_way_to_yes(
    rows: &[NamedPlace],
    todo_dir: impl AsRef<Path>,
) -> Result<LookOnlyWayToYesReport, String> {
    let task_statuses = read_todo_task_statuses(todo_dir.as_ref())?;
    Ok(validate_look_only_way_to_yes_with_tasks(
        rows,
        &task_statuses,
    ))
}

pub fn validate_look_only_way_to_yes_with_tasks(
    rows: &[NamedPlace],
    task_statuses: &BTreeMap<String, TodoTaskStatus>,
) -> LookOnlyWayToYesReport {
    let look_only_rows = rows
        .iter()
        .filter(|row| row.disposition == PlaceDisposition::LookOnlyRefused)
        .map(|row| {
            let research_task = normalized_task_name(&row.research_task);
            let build_task = normalized_task_name(&row.build_task);
            let research_status = research_task
                .as_ref()
                .and_then(|task| task_statuses.get(task.as_str()));
            let build_status = build_task
                .as_ref()
                .and_then(|task| task_statuses.get(task.as_str()));
            LookOnlyWayToYesRow {
                place: row.place.clone(),
                research_task,
                build_task,
                research_task_exists: research_status.is_some(),
                build_task_exists: build_status.is_some(),
                research_task_done: research_status.map(|status| status.done).unwrap_or(false),
                build_task_done: build_status.map(|status| status.done).unwrap_or(false),
            }
        })
        .collect::<Vec<_>>();

    let mut missing_task_numbers = BTreeSet::new();
    for row in &look_only_rows {
        if let Some(task) = &row.research_task {
            if !row.research_task_exists {
                missing_task_numbers.insert(task.clone());
            }
        }
        if let Some(task) = &row.build_task {
            if !row.build_task_exists {
                missing_task_numbers.insert(task.clone());
            }
        }
    }

    LookOnlyWayToYesReport {
        refused_rows_with_no_named_way_to_yes: look_only_rows
            .iter()
            .filter(|row| !row.has_named_way_to_yes())
            .count(),
        refused_rows_with_done_research_task: look_only_rows
            .iter()
            .filter(|row| row.research_task_done)
            .count(),
        refused_rows_with_done_build_task: look_only_rows
            .iter()
            .filter(|row| row.build_task_done)
            .count(),
        look_only_rows,
        missing_task_numbers: missing_task_numbers.into_iter().collect(),
    }
}

pub fn read_todo_task_statuses(
    todo_dir: &Path,
) -> Result<BTreeMap<String, TodoTaskStatus>, String> {
    let mut files = Vec::new();
    collect_todo_files(todo_dir, &mut files)?;
    files.sort();

    let mut statuses = BTreeMap::<String, TodoTaskStatus>::new();
    for path in files {
        let body = std::fs::read_to_string(&path)
            .map_err(|error| format!("OSL todo read refused {}: {error}", path.display()))?;
        let mut current_task: Option<String> = None;
        for line in body.lines() {
            if let Some((task_id, done)) = parse_task_heading(line) {
                statuses
                    .entry(task_id.clone())
                    .and_modify(|status| status.done |= done)
                    .or_insert(TodoTaskStatus {
                        task_id: task_id.clone(),
                        done,
                    });
                current_task = Some(task_id);
                continue;
            }

            if line.starts_with("done:") {
                if let Some(task_id) = &current_task {
                    if let Some(status) = statuses.get_mut(task_id) {
                        status.done = true;
                    }
                }
            }
        }
    }
    Ok(statuses)
}

fn collect_todo_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("OSL todo dir read refused {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("OSL todo entry refused: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_todo_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("txt") {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_task_heading(line: &str) -> Option<(String, bool)> {
    let rest = line.strip_prefix("TASK ")?;
    let mut parts = rest.split_whitespace();
    let task_id = parts.next()?.to_owned();
    let done = matches!(parts.next(), Some("[x]"));
    Some((task_id, done))
}

fn normalized_task_name(task: &Option<String>) -> Option<String> {
    let task = task.as_ref()?.trim();
    (!task.is_empty()).then(|| task.to_owned())
}

fn task_name_is_filled(task: &Option<String>) -> bool {
    task.as_deref()
        .map(|task| !task.trim().is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookOnlyShutAttempt {
    pub place_words: String,
    pub register_exit: i32,
    pub typing_boxes_opened: usize,
    pub research_task: String,
}

impl LookOnlyShutAttempt {
    pub fn is_refusal(&self) -> bool {
        self.register_exit == 1
            && self.typing_boxes_opened == 0
            && !self.place_words.trim().is_empty()
            && !self.research_task.trim().is_empty()
    }
}

pub fn try_register_look_only_and_open_typing_box(
    rows: &[NamedPlace],
    place: &str,
) -> Result<LookOnlyShutAttempt, String> {
    let row = rows
        .iter()
        .find(|row| row.place.eq_ignore_ascii_case(place))
        .ok_or_else(|| format!("look-only place not found: {}", place.to_ascii_lowercase()))?;

    let place_words = row.place.to_ascii_lowercase();
    if row.disposition != PlaceDisposition::LookOnlyRefused {
        return Err(format!("{place_words} is not look-only refused"));
    }

    let research_task = normalized_task_name(&row.research_task)
        .ok_or_else(|| format!("{place_words} refusal is missing a research task"))?;

    Ok(LookOnlyShutAttempt {
        place_words,
        register_exit: 1,
        typing_boxes_opened: 0,
        research_task,
    })
}
