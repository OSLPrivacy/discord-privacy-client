use serde::Deserialize;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum NamedPlaceState {
    Approved,
    LookOnlyRefused,
}

impl NamedPlaceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::LookOnlyRefused => "look-only refused",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NamedPlaceRecord {
    pub app: String,
    pub place_name: String,
    pub state: NamedPlaceState,
    pub source_task: String,
    pub research_task: Option<String>,
    pub build_task: Option<String>,
    pub proving_check: String,
    pub person_can_do: String,
}

pub const TASK_4203_EXPECTED_ROW_COUNT: usize = 13;
pub const TASK_4203_EXPECTED_APPROVED_COUNT: usize = 11;
pub const TASK_4203_EXPECTED_LOOK_ONLY_REFUSED_COUNT: usize = 2;

pub const TASK_4203_SOURCE_TASKS: &[&str] = &[
    "1027", "1028", "1029a", "1055", "1089a", "1089b", "1089c", "1089d", "1125", "1127", "1154",
    "1155", "1190",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedPlaceSummary {
    pub row_count: usize,
    pub approved_count: usize,
    pub look_only_refused_count: usize,
    pub source_task_count: usize,
    pub missing_required_place_names: Vec<&'static str>,
    pub invalid_source_tasks: Vec<String>,
}

pub const TASK_4203_REQUIRED_PLACE_NAMES: &[&str] = &[
    "Telegram supergroup",
    "Telegram saved messages",
    "Signal story",
    "WhatsApp community",
    "WhatsApp community group",
    "WhatsApp broadcast list",
    "X group direct message",
    "X reply",
    "Instagram comment",
    "Instagram story",
    "Messenger community",
    "Telegram story",
    "WhatsApp Status",
];

pub fn summarize_named_places(records: &[NamedPlaceRecord]) -> NamedPlaceSummary {
    let approved_count = records
        .iter()
        .filter(|record| record.state == NamedPlaceState::Approved)
        .count();
    let look_only_refused_count = records
        .iter()
        .filter(|record| record.state == NamedPlaceState::LookOnlyRefused)
        .count();
    let source_task_count = records
        .iter()
        .filter(|record| TASK_4203_SOURCE_TASKS.contains(&record.source_task.as_str()))
        .count();
    let missing_required_place_names = TASK_4203_REQUIRED_PLACE_NAMES
        .iter()
        .copied()
        .filter(|place_name| {
            !records
                .iter()
                .any(|record| record.place_name.as_str() == *place_name)
        })
        .collect();
    let invalid_source_tasks = records
        .iter()
        .filter(|record| !TASK_4203_SOURCE_TASKS.contains(&record.source_task.as_str()))
        .map(|record| record.source_task.clone())
        .collect();

    NamedPlaceSummary {
        row_count: records.len(),
        approved_count,
        look_only_refused_count,
        source_task_count,
        missing_required_place_names,
        invalid_source_tasks,
    }
}

pub fn task_4203_summary_is_green(summary: &NamedPlaceSummary) -> bool {
    summary.row_count == TASK_4203_EXPECTED_ROW_COUNT
        && summary.approved_count == TASK_4203_EXPECTED_APPROVED_COUNT
        && summary.look_only_refused_count == TASK_4203_EXPECTED_LOOK_ONLY_REFUSED_COUNT
        && summary.source_task_count == summary.row_count
        && summary.missing_required_place_names.is_empty()
        && summary.invalid_source_tasks.is_empty()
}
