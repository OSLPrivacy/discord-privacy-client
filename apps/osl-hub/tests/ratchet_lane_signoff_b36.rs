#![cfg(all(feature = "core", feature = "discord-qa-shell"))]

use std::fs;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
struct SignoffRow {
    finding_id: String,
    b15_status_before: String,
    remediation_unit: String,
    acceptance_test: String,
    reviewer_status: String,
    closed: String,
    enables_rn: String,
}

fn root_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn section<'a>(source: &'a str, heading: &str) -> &'a str {
    let marker = format!("## {heading}");
    let start = source
        .find(&marker)
        .unwrap_or_else(|| panic!("missing section {heading}"));
    let after = &source[start + marker.len()..];
    let end = after.find("\n## ").unwrap_or(after.len());
    &after[..end]
}

fn parse_signoff_rows(section: &str) -> Vec<SignoffRow> {
    let table_lines: Vec<&str> = section
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|') && line.ends_with('|'))
        .collect();
    assert!(table_lines.len() >= 4, "expected two sign-off rows");

    let header: Vec<&str> = table_lines[0]
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    assert_eq!(
        header,
        [
            "finding_id",
            "b15_status_before",
            "remediation_unit",
            "acceptance_test",
            "reviewer_status",
            "closed",
            "enables_rn"
        ]
    );

    assert!(
        table_lines[1]
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .all(|cell| cell.chars().all(|ch| ch == '-')),
        "malformed table separator"
    );

    table_lines
        .into_iter()
        .skip(2)
        .map(|line| {
            let cells: Vec<String> = line
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .map(ToOwned::to_owned)
                .collect();
            assert_eq!(cells.len(), 7, "unexpected sign-off row width: {line}");
            SignoffRow {
                finding_id: cells[0].clone(),
                b15_status_before: cells[1].clone(),
                remediation_unit: cells[2].clone(),
                acceptance_test: cells[3].clone(),
                reviewer_status: cells[4].clone(),
                closed: cells[5].clone(),
                enables_rn: cells[6].clone(),
            }
        })
        .collect()
}

#[test]
fn reviewer_signoff_confirms_ratchet_remediations_closed() {
    let root = root_path();
    let signoff_report = fs::read_to_string(root.join("docs/reports/ratchet-lane-2026-07-26.md"))
        .expect("read ratchet lane report");
    let b15_report = fs::read_to_string(root.join("docs/reports/reviewer-findings-b15.md"))
        .expect("read b15 findings report");
    let commands = fs::read_to_string(root.join("crates/ipc/src/commands.rs"))
        .expect("read IPC commands source");
    let wire_rn =
        fs::read_to_string(root.join("crates/ipc/src/wire_rn.rs")).expect("read RN gate source");
    let state = fs::read_to_string(root.join("crates/ipc/src/state.rs"))
        .expect("read RN runtime gate source");
    let keystore_client = fs::read_to_string(root.join("crates/keystore/src/client.rs"))
        .expect("read RN capability source");

    assert!(
        b15_report.contains("| b20_session_reset_symptom_deadlock | high | open | b20 |"),
        "b36 must sign off a real b15 finding, not an invented row"
    );
    assert!(
        b15_report.contains("| b20_recovery_result_observability | medium | open | b20 |"),
        "b36 must sign off the b15 observability finding"
    );

    let rows = parse_signoff_rows(section(
        &signoff_report,
        "Reviewer Sign-Off: b15-b20 Findings Closed",
    ));
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .map(|row| row.finding_id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([
            "b20_recovery_result_observability",
            "b20_session_reset_symptom_deadlock",
        ])
    );
    assert!(
        rows.iter().all(|row| row.b15_status_before == "open"
            && row.remediation_unit == "b20"
            && row.acceptance_test == "remediate_independent_review_findings"
            && row.reviewer_status == "signed_off"
            && row.closed == "yes"
            && row.enables_rn == "no"),
        "every b20 finding must be signed off without enabling RN: {rows:?}"
    );

    assert!(
        commands.contains("fn remediate_independent_review_findings()"),
        "sign-off must point at the actual b20 acceptance test"
    );
    assert!(
        commands.contains("had_recent_v4_failure(PEER, now)")
            && commands.contains("OSL_RESULT_SESSION_RESET_APPLIED")
            && commands.contains("OSL_RESULT_RECOVERY_IGNORED"),
        "b20 proof must cover no-symptom apply plus replay/stale refusal"
    );
    assert!(
        wire_rn.contains("pub const RN_WIRE_IN_ENABLED: bool = true;")
            && state.contains("rn_wire_in_enabled: AtomicBool::new(false)")
            && state.contains("pub fn set_rn_wire_in_enabled")
            && keystore_client.contains(
                "CLIENT_RN_CAPABILITY_FLOOR: u32 = rn_capabilities_for_wire_in(true)"
            )
            && wire_rn.contains("pub const RN_SESSION_DIR: &str = \"rn_sessions\"")
            && wire_rn.contains(".create_new(true)"),
        "RN build capability requires live advertisement, one session directory, and writer locking; runtime activation remains explicitly closed until D41's desync proof"
    );
}
