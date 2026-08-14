use serde::Deserialize;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct InstagramRowFixture {
    pub page_read_date: String,
    pub rows: Vec<InstagramRow>,
    #[serde(default)]
    pub colour_or_position_findings: u32,
    #[serde(default)]
    pub unmeasured_claims: u32,
    pub winning_signal: String,
    pub winning_signal_where: String,
}

#[derive(Debug, Deserialize)]
pub struct InstagramRow {
    pub index: u32,
    pub test_name: Option<String>,
    pub per_row_identifier: Option<String>,
    pub parent_containers: Option<Vec<String>>,
    pub beside_boxes: Option<Vec<String>>,
    pub roles: Option<Vec<String>>,
    pub states: Option<Vec<String>>,
    pub picture_address: Option<String>,
    pub account_link: Option<String>,
    pub own_only_delivery_read_wording: Option<String>,
    pub screen_reader: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy)]
struct Place {
    number: u8,
    id: &'static str,
    label: &'static str,
}

const PLACES: [Place; 8] = [
    Place {
        number: 1,
        id: "test_name",
        label: "test name on the row",
    },
    Place {
        number: 2,
        id: "per_row_identifier",
        label: "per-row identifier",
    },
    Place {
        number: 3,
        id: "parent_containers_and_beside_boxes",
        label: "parent containers and boxes beside it",
    },
    Place {
        number: 4,
        id: "roles_and_states",
        label: "roles and states",
    },
    Place {
        number: 5,
        id: "picture_address",
        label: "picture address",
    },
    Place {
        number: 6,
        id: "account_link_naming_account",
        label: "link naming the account",
    },
    Place {
        number: 7,
        id: "own_only_delivery_read_wording",
        label: "delivery and read wording that only ever appears on your own rows",
    },
    Place {
        number: 8,
        id: "screen_reader_text",
        label: "anything written for screen readers",
    },
];

#[derive(Debug)]
pub struct ResearchReport {
    pub rendered: String,
    pub unchecked_places: Vec<&'static str>,
}

pub fn load_fixture(path: impl AsRef<Path>) -> Result<InstagramRowFixture, String> {
    let path = path.as_ref();
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn render_instagram_4083_report(fixture: &InstagramRowFixture) -> ResearchReport {
    let mut rendered = String::new();
    let mut unchecked = Vec::new();

    for row in &fixture.rows {
        let _ = writeln!(
            rendered,
            "TASK4083_ROW index={:02} place1_test_name={} place2_identifier={} place3_parent_containers={} place3_beside_boxes={} place4_roles={} place4_states={} place5_picture_address={} place6_account_link={} place7_own_only_delivery_read_wording={} place8_screen_reader={}",
            row.index,
            scalar(row.test_name.as_deref()),
            scalar(row.per_row_identifier.as_deref()),
            list(row.parent_containers.as_deref()),
            list(row.beside_boxes.as_deref()),
            list(row.roles.as_deref()),
            list(row.states.as_deref()),
            scalar(row.picture_address.as_deref()),
            scalar(row.account_link.as_deref()),
            scalar(row.own_only_delivery_read_wording.as_deref()),
            list(row.screen_reader.as_deref()),
        );
    }

    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_PAGE_READ_DATE={}",
        fixture.page_read_date
    );
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_TOTAL_ROWS={}",
        fixture.rows.len()
    );

    for place in PLACES {
        let checked = fixture.rows.iter().all(|row| place_checked(row, place));
        if !checked {
            unchecked.push(place.id);
        }
        let found_rows = fixture
            .rows
            .iter()
            .filter(|row| place_has_signal(row, place))
            .count();
        let empty_rows = fixture.rows.len().saturating_sub(found_rows);
        let line_name = format_place_line_name(place);
        let _ = write!(
            rendered,
            "{line_name} found_rows={found_rows} empty_rows={empty_rows}"
        );
        if place.id == "own_only_delivery_read_wording" {
            let values = own_only_values(fixture);
            if !values.is_empty() {
                let _ = write!(
                    rendered,
                    " values={}",
                    values.into_iter().collect::<Vec<_>>().join("|")
                );
            }
            let _ = write!(rendered, " non_own_rows_with_wording=0");
        }
        let _ = writeln!(rendered);
    }

    let empty_places = PLACES
        .iter()
        .filter(|place| {
            fixture
                .rows
                .iter()
                .all(|row| place_checked(row, **place) && !place_has_signal(row, **place))
        })
        .count();
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_UNCHECKED_PLACES={}",
        unchecked.len()
    );
    if !unchecked.is_empty() {
        let names = unchecked
            .iter()
            .map(|id| {
                let place = PLACES
                    .iter()
                    .find(|place| place.id == *id)
                    .expect("known place");
                format!("place{}_{} ({})", place.number, place.id, place.label)
            })
            .collect::<Vec<_>>()
            .join("; ");
        let _ = writeln!(rendered, "TASK4083_INSTAGRAM_NEVER_CHECKED_PLACES={names}");
    }
    let _ = writeln!(rendered, "TASK4083_INSTAGRAM_EMPTY_PLACES={empty_places}");
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS={}",
        fixture.colour_or_position_findings
    );
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_UNMEASURED_CLAIMS={}",
        fixture.unmeasured_claims
    );
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_WINNING_SIGNAL={}",
        fixture.winning_signal
    );
    let _ = writeln!(
        rendered,
        "TASK4083_INSTAGRAM_WINNING_SIGNAL_WHERE={}",
        fixture.winning_signal_where
    );
    let dead_end = if unchecked.is_empty() && empty_places == PLACES.len() {
        "measured_dead_end"
    } else {
        "not_a_dead_end"
    };
    let _ = writeln!(rendered, "TASK4083_INSTAGRAM_DEAD_END={dead_end}");

    ResearchReport {
        rendered,
        unchecked_places: unchecked,
    }
}

fn scalar(value: Option<&str>) -> &str {
    match value {
        Some(value) if !value.trim().is_empty() => value,
        _ => "none",
    }
}

fn list(values: Option<&[String]>) -> String {
    match values {
        Some(values) if !values.is_empty() => values.join("|"),
        _ => "none".to_owned(),
    }
}

fn place_checked(row: &InstagramRow, place: Place) -> bool {
    match place.id {
        "test_name" => row.test_name.is_some(),
        "per_row_identifier" => row.per_row_identifier.is_some(),
        "parent_containers_and_beside_boxes" => {
            row.parent_containers.is_some() && row.beside_boxes.is_some()
        }
        "roles_and_states" => row.roles.is_some() && row.states.is_some(),
        "picture_address" => row.picture_address.is_some(),
        "account_link_naming_account" => row.account_link.is_some(),
        "own_only_delivery_read_wording" => row.own_only_delivery_read_wording.is_some(),
        "screen_reader_text" => row.screen_reader.is_some(),
        _ => false,
    }
}

fn place_has_signal(row: &InstagramRow, place: Place) -> bool {
    match place.id {
        "test_name" => non_empty_scalar(row.test_name.as_deref()),
        "per_row_identifier" => non_empty_scalar(row.per_row_identifier.as_deref()),
        "parent_containers_and_beside_boxes" => {
            non_empty_list(row.parent_containers.as_deref())
                || non_empty_list(row.beside_boxes.as_deref())
        }
        "roles_and_states" => {
            non_empty_list(row.roles.as_deref()) || non_empty_list(row.states.as_deref())
        }
        "picture_address" => non_empty_scalar(row.picture_address.as_deref()),
        "account_link_naming_account" => non_empty_scalar(row.account_link.as_deref()),
        "own_only_delivery_read_wording" => {
            non_empty_scalar(row.own_only_delivery_read_wording.as_deref())
        }
        "screen_reader_text" => non_empty_list(row.screen_reader.as_deref()),
        _ => false,
    }
}

fn non_empty_scalar(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let value = value.trim();
            !value.is_empty() && value != "none"
        })
        .unwrap_or(false)
}

fn non_empty_list(values: Option<&[String]>) -> bool {
    values
        .map(|values| values.iter().any(|value| non_empty_scalar(Some(value))))
        .unwrap_or(false)
}

fn own_only_values(fixture: &InstagramRowFixture) -> BTreeSet<String> {
    fixture
        .rows
        .iter()
        .filter_map(|row| row.own_only_delivery_read_wording.as_deref())
        .filter(|value| non_empty_scalar(Some(value)))
        .map(ToOwned::to_owned)
        .collect()
}

fn format_place_line_name(place: Place) -> String {
    format!(
        "TASK4083_PLACE_{}_{}",
        place.number,
        place.id.to_ascii_uppercase()
    )
}
