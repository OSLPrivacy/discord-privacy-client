//! TASK 4878: numeric per-role limits are enforced as relay decisions.

#[allow(dead_code)]
#[path = "../../src/osl_enclave_roles.rs"]
mod osl_enclave_roles;

use std::collections::BTreeMap;

use osl_enclave_roles::{
    resolve_enclave_relay_limits, EnclavePermissionDecision, EnclaveRelayActionBudget,
    EnclaveRelayActionMetadata, EnclaveRelayLimitInput, EnclaveRelayLimitKind,
    EnclaveRoleRelayLimits,
};

const ROLE_NAME: &str = "WATCH_CAPTAIN";
const START: u64 = 1_800_000_000;

fn limits() -> BTreeMap<String, EnclaveRoleRelayLimits> {
    BTreeMap::from([(
        ROLE_NAME.to_owned(),
        EnclaveRoleRelayLimits {
            slow_mode_seconds: 10,
            longest_mute_seconds: 2 * 60 * 60,
            actions_per_hour_budget: 5,
        },
    )])
}

#[test]
fn task_4878_relay_refuses_slow_mode_mute_length_and_action_budget() {
    let role_limits = limits();

    let first_message = resolve_enclave_relay_limits(EnclaveRelayLimitInput {
        role_name: ROLE_NAME,
        role_limits: &role_limits,
        action: EnclaveRelayActionMetadata::SendMessage,
        now_unix_seconds: START,
        last_message_unix_seconds: None,
    });
    assert!(first_message.allowed());

    let slow_refusals = [START + 1, START + 9]
        .into_iter()
        .map(|now| {
            resolve_enclave_relay_limits(EnclaveRelayLimitInput {
                role_name: ROLE_NAME,
                role_limits: &role_limits,
                action: EnclaveRelayActionMetadata::SendMessage,
                now_unix_seconds: now,
                last_message_unix_seconds: Some(START),
            })
        })
        .collect::<Vec<_>>();
    let after_slow_mode = resolve_enclave_relay_limits(EnclaveRelayLimitInput {
        role_name: ROLE_NAME,
        role_limits: &role_limits,
        action: EnclaveRelayActionMetadata::SendMessage,
        now_unix_seconds: START + 10,
        last_message_unix_seconds: Some(START),
    });

    println!(
        "TASK4878_SLOW_MODE role={ROLE_NAME} slow_mode_seconds=10 sent_messages=1 refused_before_wait={} allowed_after_seconds=10 denied_by={}",
        slow_refusals.len(),
        slow_refusals[0].denied_by().unwrap_or("NONE")
    );
    assert!(after_slow_mode.allowed());
    assert_eq!(slow_refusals.len(), 2);
    for refusal in &slow_refusals {
        assert_eq!(refusal.decision, EnclavePermissionDecision::Denied);
        assert_eq!(refusal.limit, Some(EnclaveRelayLimitKind::SlowMode));
        assert_eq!(refusal.denied_by(), Some("RELAY"));
    }

    let mute_refusal = resolve_enclave_relay_limits(EnclaveRelayLimitInput {
        role_name: ROLE_NAME,
        role_limits: &role_limits,
        action: EnclaveRelayActionMetadata::CreateMute {
            duration_seconds: 3 * 60 * 60,
        },
        now_unix_seconds: START,
        last_message_unix_seconds: None,
    });
    println!(
        "TASK4878_MUTE role={ROLE_NAME} cap_seconds=7200 requested_seconds=10800 decision=refused denied_by={}",
        mute_refusal.denied_by().unwrap_or("NONE")
    );
    assert_eq!(mute_refusal.decision, EnclavePermissionDecision::Denied);
    assert_eq!(mute_refusal.limit, Some(EnclaveRelayLimitKind::LongestMute));
    assert_eq!(mute_refusal.denied_by(), Some("RELAY"));

    let mut budget = EnclaveRelayActionBudget::default();
    let mut allowed_budget_actions = 0;
    for action_number in 1..=5 {
        let result = budget.check_and_record(ROLE_NAME, &role_limits, START + action_number);
        assert!(
            result.allowed(),
            "action {action_number} must fit the hourly budget"
        );
        allowed_budget_actions += 1;
    }
    let action_six = budget.check_and_record(ROLE_NAME, &role_limits, START + 6);
    println!(
        "TASK4878_ACTION_BUDGET role={ROLE_NAME} budget_per_hour=5 allowed_actions={allowed_budget_actions} refused_action=6 denied_by={}",
        action_six.denied_by().unwrap_or("NONE")
    );
    assert_eq!(allowed_budget_actions, 5);
    assert_eq!(action_six.decision, EnclavePermissionDecision::Denied);
    assert_eq!(action_six.limit, Some(EnclaveRelayLimitKind::ActionBudget));
    assert_eq!(action_six.denied_by(), Some("RELAY"));

    let relay_refusal_tags = [
        slow_refusals[0].denied_by(),
        mute_refusal.denied_by(),
        action_six.denied_by(),
    ];
    println!(
        "TASK4878_RELAY_REFUSALS count=3 tags={},{},{}",
        relay_refusal_tags[0].unwrap_or("NONE"),
        relay_refusal_tags[1].unwrap_or("NONE"),
        relay_refusal_tags[2].unwrap_or("NONE")
    );
    assert_eq!(
        relay_refusal_tags,
        [Some("RELAY"), Some("RELAY"), Some("RELAY")]
    );
}
