//! TASK 0854 — read one direct status record for every Home label.
//!
//! This is intentionally a data test. It does not construct a webview, render
//! markup, or navigate to a route. The fixture supplies the same capability
//! facts Home uses plus the label carried by the tile's status data. Home's
//! label is recomputed by the production `generated_tile_label` function and
//! compared byte-for-byte with the direct status label.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use osl_privacy_hub::services::{generated_tile_label, ServiceCapabilityFacts};
use serde::Deserialize;

const ALL_LABELS_FIXTURE: &str = "tests/fixtures/task_0854_all_label_types.json";
const EXPECTED_HOME_LABELS: [&str; 5] = [
    "Ready",
    "Placing only",
    "Reading only",
    "Opens the app",
    "Not started",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatusFixture {
    tiles: Vec<StatusTile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatusTile {
    tile_id: String,
    capability_facts: ServiceCapabilityFacts,
    status_data: DirectStatusData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DirectStatusData {
    generated_label: String,
}

fn fixture_path() -> PathBuf {
    let requested = std::env::var_os("OSL_TASK_0854_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(ALL_LABELS_FIXTURE));
    if requested.is_absolute() {
        requested
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(requested)
    }
}

fn read_fixture() -> (PathBuf, StatusFixture) {
    let path = fixture_path();
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read TASK 0854 fixture {}: {error}", path.display()));
    let fixture = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("parse TASK 0854 fixture {}: {error}", path.display()));
    (path, fixture)
}

#[test]
fn task_0854_direct_status_data_matches_one_home_tile_of_every_label_type() {
    let (path, fixture) = read_fixture();
    let expected = EXPECTED_HOME_LABELS.into_iter().collect::<BTreeSet<_>>();
    let mut seen_tile_ids = BTreeSet::new();
    let mut label_counts = BTreeMap::<&str, usize>::new();

    println!("TASK0854_FIXTURE={}", path.display());
    for tile in &fixture.tiles {
        assert!(
            seen_tile_ids.insert(tile.tile_id.as_str()),
            "fixture repeats tile id {}",
            tile.tile_id
        );
        let home_label = generated_tile_label(tile.capability_facts);
        let status_label = tile.status_data.generated_label.as_str();
        let agrees = status_label == home_label;
        println!(
            "TASK0854_STATUS tile={} home_label=\"{}\" status_label=\"{}\" exact_match={}",
            tile.tile_id, home_label, status_label, agrees
        );
        assert_eq!(
            status_label, home_label,
            "{} direct status label disagrees with its generated Home label",
            tile.tile_id
        );
        *label_counts.entry(home_label).or_default() += 1;
    }

    let actual = label_counts.keys().copied().collect::<BTreeSet<_>>();
    let missing = expected.difference(&actual).copied().collect::<Vec<_>>();
    let unexpected = actual.difference(&expected).copied().collect::<Vec<_>>();
    let repeated = label_counts
        .iter()
        .filter_map(|(label, count)| (*count != 1).then_some((*label, *count)))
        .collect::<Vec<_>>();
    println!("TASK0854_RESULT_COUNT={}", fixture.tiles.len());
    println!("TASK0854_LABEL_TYPE_COUNT={}", actual.len());
    println!("TASK0854_MISSING_LABEL_TYPES={}", missing.join("|"));
    println!("TASK0854_UNEXPECTED_LABEL_TYPES={}", unexpected.join("|"));
    println!(
        "TASK0854_LABEL_COUNTS={}",
        EXPECTED_HOME_LABELS
            .iter()
            .map(|label| format!(
                "{label}:{}",
                label_counts.get(label).copied().unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join("|")
    );

    assert_eq!(
        fixture.tiles.len(),
        EXPECTED_HOME_LABELS.len(),
        "fixture must contain exactly one tile for each Home label type; missing={missing:?} unexpected={unexpected:?} repeated={repeated:?}"
    );
    assert!(
        missing.is_empty() && unexpected.is_empty() && repeated.is_empty(),
        "fixture must contain exactly one tile for each Home label type; missing={missing:?} unexpected={unexpected:?} repeated={repeated:?}"
    );
}
