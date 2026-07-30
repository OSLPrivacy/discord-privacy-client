use std::collections::BTreeMap;

use ipc::wire_rn::RN_WIRE_IN_ENABLED;

const RATCHET_REPORT: &str = include_str!("../../../docs/reports/ratchet-lane-2026-07-26.md");

fn unquote_code_cell(value: &str) -> &str {
    value
        .trim()
        .strip_prefix('`')
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or_else(|| value.trim())
}

fn table_after_heading(markdown: &str, heading: &str) -> BTreeMap<String, String> {
    let mut lines = markdown.lines().skip_while(|line| line.trim() != heading);
    assert_eq!(
        lines.next().map(str::trim),
        Some(heading),
        "heading not found"
    );
    let mut rows = BTreeMap::new();
    for line in lines {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            if rows.is_empty() {
                continue;
            }
            break;
        }
        let cells: Vec<_> = trimmed
            .trim_matches('|')
            .split('|')
            .map(unquote_code_cell)
            .collect();
        if cells.len() != 2 || cells[0].starts_with("---") || cells[0] == "B3 evidence field" {
            continue;
        }
        rows.insert(cells[0].to_owned(), cells[1].to_owned());
    }
    rows
}

#[test]
fn b3_ipc_integration_proof_status() {
    assert!(
        !RN_WIRE_IN_ENABLED,
        "B3 proof documentation must not enable the RN production fuse"
    );

    let status = table_after_heading(
        RATCHET_REPORT,
        "## B3 IPC-integration proof status against checklist evidence row",
    );
    assert_eq!(status.get("checklist_row").map(String::as_str), Some("B3"));
    assert_eq!(
        status
            .get("historical_archive_ipc_gate")
            .map(String::as_str),
        Some("blocked_missing_later_ipc_keystore_apis")
    );
    assert_eq!(
        status.get("dependency_closure").map(String::as_str),
        Some("1d8bfa8")
    );
    assert_eq!(
        status
            .get("dependency_closure_passed_cases")
            .and_then(|value| value.parse::<u32>().ok()),
        Some(35)
    );
    assert_eq!(
        status.get("evidence_tier").map(String::as_str),
        Some("test-proven-only")
    );
    assert_eq!(
        status.get("path_status").map(String::as_str),
        Some("implemented-unwired")
    );
    assert_eq!(
        status.get("runtime_claim").map(String::as_str),
        Some("none")
    );
    assert_eq!(
        status.get("rn_wire_in_enabled").map(String::as_str),
        Some("false")
    );
}
