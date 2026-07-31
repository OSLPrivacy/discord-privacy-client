use serde_json::Value;

const ACCELERATION_PLAN: &str =
    include_str!("../../../docs/plans/osl-acceleration-plan-2026-07-27.md");

#[derive(Debug, Eq, PartialEq)]
enum ControlError {
    Missing(String),
    Refused(String),
}

fn extract_acceptance_contract(markdown: &str) -> Value {
    let marker = "Machine-checkable acceptance contract:";
    let after_marker = markdown
        .split_once(marker)
        .expect("acceleration plan must name the machine-checkable acceptance contract")
        .1;
    let json_block = after_marker
        .split_once("```json\n")
        .expect("acceptance contract must be a JSON fenced block")
        .1
        .split_once("\n```")
        .expect("acceptance contract JSON block must be closed")
        .0;

    serde_json::from_str(json_block).expect("acceleration plan acceptance contract is valid JSON")
}

fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("contract field `{field}` must be a string"))
}

fn array_field<'a>(value: &'a Value, field: &str) -> &'a [Value] {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("contract field `{field}` must be an array"))
}

fn string_array_field(value: &Value, field: &str) -> Vec<String> {
    array_field(value, field)
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .unwrap_or_else(|| panic!("contract field `{field}` entries must be strings"))
                .to_owned()
        })
        .collect()
}

fn control_by_name<'a>(contract: &'a Value, name: &str) -> &'a Value {
    array_field(contract, "controls")
        .iter()
        .find(|control| control.get("name").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("missing acceleration plan control `{name}`"))
}

fn evaluate_control(control: &Value, observed_facts: &[&str]) -> Vec<ControlError> {
    let observed = observed_facts
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let required = string_array_field(control, "requiredFacts");
    let refused = string_array_field(control, "refusedFacts");
    let mut errors = Vec::new();

    for fact in required {
        if !observed.contains(fact.as_str()) {
            errors.push(ControlError::Missing(fact));
        }
    }

    for fact in refused {
        if observed.contains(fact.as_str()) {
            errors.push(ControlError::Refused(fact));
        }
    }

    errors
}

#[test]
fn keep_codex_account_and_heavy_resource_routing_explicit() {
    let contract = extract_acceptance_contract(ACCELERATION_PLAN);
    assert_eq!(
        contract.get("schemaVersion").and_then(Value::as_u64),
        Some(1)
    );
    let control = control_by_name(
        &contract,
        "Keep Codex account and heavy-resource routing explicit.",
    );
    assert_eq!(
        string_field(control, "name"),
        "Keep Codex account and heavy-resource routing explicit."
    );

    let permitted = [
        "preserves_inherited_codex_home",
        "cargo_uses_osl_cargo",
        "broad_verification_uses_osl_heavy",
        "delegation_uses_osl_fast_delegate",
        "child_prompts_forbid_account_changes",
    ];
    assert!(evaluate_control(control, &permitted).is_empty());

    let switched_account = [
        "cargo_uses_osl_cargo",
        "broad_verification_uses_osl_heavy",
        "delegation_uses_osl_fast_delegate",
        "child_prompts_forbid_account_changes",
        "delegate_may_switch_account",
    ];
    assert!(
        evaluate_control(control, &switched_account).contains(&ControlError::Missing(
            "preserves_inherited_codex_home".to_owned()
        ))
    );
    assert!(
        evaluate_control(control, &switched_account).contains(&ControlError::Refused(
            "delegate_may_switch_account".to_owned()
        ))
    );

    let heavy_bypass = [
        "preserves_inherited_codex_home",
        "cargo_uses_osl_cargo",
        "delegation_uses_osl_fast_delegate",
        "child_prompts_forbid_account_changes",
        "broad_verification_bypasses_osl_heavy",
    ];
    assert!(
        evaluate_control(control, &heavy_bypass).contains(&ControlError::Missing(
            "broad_verification_uses_osl_heavy".to_owned()
        ))
    );
    assert!(
        evaluate_control(control, &heavy_bypass).contains(&ControlError::Refused(
            "broad_verification_bypasses_osl_heavy".to_owned()
        ))
    );
}

#[test]
fn pin_mirror_sessions_by_identifier_before_prompting_or_resuming_them() {
    let contract = extract_acceptance_contract(ACCELERATION_PLAN);
    assert_eq!(
        contract.get("schemaVersion").and_then(Value::as_u64),
        Some(1)
    );
    let control = control_by_name(
        &contract,
        "Pin mirror sessions by identifier before prompting or resuming them.",
    );
    assert_eq!(
        string_field(control, "name"),
        "Pin mirror sessions by identifier before prompting or resuming them."
    );

    let permitted = [
        "session_inspected_immediately_before_action",
        "session_selected_by_concrete_id",
        "pin_required_before_prompt",
        "pin_required_before_resume",
    ];
    assert!(evaluate_control(control, &permitted).is_empty());

    let routed_by_title = [
        "session_inspected_immediately_before_action",
        "pin_required_before_prompt",
        "pin_required_before_resume",
        "route_by_title",
    ];
    assert!(
        evaluate_control(control, &routed_by_title).contains(&ControlError::Missing(
            "session_selected_by_concrete_id".to_owned()
        ))
    );
    assert!(evaluate_control(control, &routed_by_title)
        .contains(&ControlError::Refused("route_by_title".to_owned())));

    let stale_resume = [
        "session_inspected_immediately_before_action",
        "session_selected_by_concrete_id",
        "pin_required_before_prompt",
        "route_by_unverified_background_terminal_report",
    ];
    assert!(
        evaluate_control(control, &stale_resume).contains(&ControlError::Missing(
            "pin_required_before_resume".to_owned()
        ))
    );
    assert!(
        evaluate_control(control, &stale_resume).contains(&ControlError::Refused(
            "route_by_unverified_background_terminal_report".to_owned()
        ))
    );
}

#[test]
fn rotate_telegram_credentials_without_changing_operator_allowlists() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo_root.join("keyserver-cf/scripts/test-configure-telegram-reporting-bot.sh");
    let output = std::process::Command::new("bash")
        .arg(&script)
        .current_dir(&repo_root)
        .output()
        .expect("run Telegram reporting bot rotation behavior harness");

    assert!(
        output.status.success(),
        "Telegram reporting bot rotation harness failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PASS Rotate Telegram credentials without changing operator allowlists."),
        "rotation harness must report the exact acceptance title after exercising fake Telegram and fake Wrangler"
    );
}
