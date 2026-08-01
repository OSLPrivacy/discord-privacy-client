use std::collections::BTreeMap;

use ipc::state::AppState;
use std::sync::atomic::Ordering;

const THREAT_MODEL: &str = include_str!("../../../docs/THREAT_MODEL.md");

fn unquote_code_cell(value: &str) -> &str {
    value
        .trim()
        .strip_prefix('`')
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or_else(|| value.trim())
}

fn reconciliation_table() -> BTreeMap<String, String> {
    let mut lines = THREAT_MODEL
        .lines()
        .skip_while(|line| line.trim() != "### v4/v5 reconciliation and remediation");
    assert_eq!(
        lines.next().map(str::trim),
        Some("### v4/v5 reconciliation and remediation"),
        "reconciliation heading not found"
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
        if cells.len() != 2 || cells[0].starts_with("---") || cells[0] == "Reconciliation field" {
            continue;
        }
        rows.insert(cells[0].to_owned(), cells[1].to_owned());
    }
    rows
}

#[test]
fn v5_sender_key_ipc_default_matches_the_reconciliation_record() {
    let (_, json) = THREAT_MODEL
        .split_once("```json threat-model-reconciliation-v1\n")
        .expect("reconciliation JSON block starts");
    let (json, _) = json
        .split_once("\n```")
        .expect("reconciliation JSON block ends");
    let record: serde_json::Value = serde_json::from_str(json).expect("valid reconciliation JSON");

    assert_eq!(
        record["v5_sender_keys"]["ipc_owner_switch_default"],
        serde_json::Value::Bool(true),
        "THREAT_MODEL must record the IPC sender-key default"
    );
    assert_eq!(
        AppState::new().sender_keys_enabled.load(Ordering::Acquire),
        record["v5_sender_keys"]["ipc_owner_switch_default"]
            .as_bool()
            .expect("IPC sender-key default is a boolean"),
        "THREAT_MODEL IPC default and AppState initializer disagree"
    );
}

#[test]
fn threat_model_reconciles_v4_retirement_and_v5_ratchet_limits() {
    let status = reconciliation_table();
    assert_eq!(
        status.get("v4_shipping_status").map(String::as_str),
        Some("retired_due_to_ratchet_desync")
    );
    assert_eq!(
        status.get("v5_shipping_status").map(String::as_str),
        Some("disabled_default_false_uses_stateless_v3")
    );
    assert_eq!(
        status.get("v5_pairwise_dependency").map(String::as_str),
        Some("no_current_proven_pairwise_distribution_channel")
    );
    assert_eq!(
        status.get("ratchet_limit_status").map(String::as_str),
        Some("planned_until_atomic_state_reset_authority_and_cross_device_tests")
    );
    assert_eq!(
        status
            .get("unsupported_group_blast_radius_claims")
            .map(String::as_str),
        Some("one_hour_500_messages_suspicious_event_current_rotation_only")
    );
}
