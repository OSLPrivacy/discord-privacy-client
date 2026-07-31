#![cfg(all(feature = "core", feature = "discord-qa-shell"))]

use std::fs;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
struct FindingRow {
    finding_id: String,
    severity: String,
    status: String,
    owner_unit: String,
    source: String,
    required_remediation: String,
    rn_enabled: String,
}

fn report_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("docs/reports/reviewer-findings-b15.md")
}

fn findings_table(report: &str) -> Vec<FindingRow> {
    let lines: Vec<&str> = report.lines().collect();
    let heading = lines
        .iter()
        .position(|line| line.trim() == "## Findings")
        .expect("missing Findings section");
    let table_lines: Vec<&str> = lines
        .iter()
        .skip(heading + 1)
        .copied()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| line.trim().starts_with('|'))
        .collect();

    assert!(
        table_lines.len() >= 5,
        "expected header, separator and at least three findings"
    );
    let header: Vec<&str> = table_lines[0]
        .trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    assert_eq!(
        header,
        [
            "finding_id",
            "severity",
            "status",
            "owner_unit",
            "source",
            "required_remediation",
            "rn_enabled"
        ]
    );
    assert!(
        table_lines[1]
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .all(|cell| cell.chars().all(|ch| ch == '-')),
        "malformed markdown table separator"
    );

    table_lines
        .into_iter()
        .skip(2)
        .map(|line| {
            let cells: Vec<String> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .map(ToOwned::to_owned)
                .collect();
            assert_eq!(cells.len(), 7, "unexpected finding row width: {line}");
            FindingRow {
                finding_id: cells[0].clone(),
                severity: cells[1].clone(),
                status: cells[2].clone(),
                owner_unit: cells[3].clone(),
                source: cells[4].clone(),
                required_remediation: cells[5].clone(),
                rn_enabled: cells[6].clone(),
            }
        })
        .collect()
}

#[test]
fn reviewer_produces_findings_report() {
    let report = fs::read_to_string(report_path()).expect("read b15 reviewer report");
    let rows = findings_table(&report);

    let session_reset = rows
        .iter()
        .find(|row| row.finding_id == "b20_session_reset_symptom_deadlock")
        .expect("missing b20 SESSION_RESET finding");
    assert_eq!(session_reset.severity, "high");
    assert_eq!(session_reset.status, "open");
    assert_eq!(session_reset.owner_unit, "b20");
    assert_eq!(session_reset.source, "ratchet-lane-review");
    assert!(
        session_reset
            .required_remediation
            .contains("Honor authenticated fresh SESSION_RESET"),
        "SESSION_RESET remediation must be concrete: {session_reset:?}"
    );

    let observability = rows
        .iter()
        .find(|row| row.finding_id == "b20_recovery_result_observability")
        .expect("missing recovery observability finding");
    assert_eq!(observability.status, "open");
    assert_eq!(observability.owner_unit, "b20");

    let signoff = rows
        .iter()
        .find(|row| row.finding_id == "b36_requires_re_review_signoff")
        .expect("missing b36 re-review finding");
    assert_eq!(signoff.status, "pending_re_review");
    assert_eq!(signoff.owner_unit, "b36");

    assert!(
        rows.iter().all(|row| row.rn_enabled == "no"),
        "findings report must not authorize RN enablement: {rows:?}"
    );
    assert!(
        report.contains("Absence of a valid binding remains refusal."),
        "report must preserve fail-closed authority language"
    );
}
