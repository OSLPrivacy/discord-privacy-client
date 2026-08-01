use std::collections::BTreeMap;

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
fn v5_sender_keys_enabled_default_false_rationale_is_documented() {
    let status = reconciliation_table();
    assert_eq!(
        status
            .get("sender_keys_enabled_default")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        status.get("default_false_reason").map(String::as_str),
        Some("account_scoped_sender_key_state_can_desync_across_devices")
    );
    assert_eq!(
        status
            .get("required_sender_key_remediation")
            .map(String::as_str),
        Some("bind_chains_to_explicit_physical_device_identity")
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
