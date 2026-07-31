use serde_json::Value;

const J14_PLAN: &str = include_str!("../../../docs/plans/osl-parallel-build-plan-2026-07-29.md");

#[derive(Debug, Eq, PartialEq)]
struct RoutingDecision {
    decision: &'static str,
    reason: &'static str,
}

fn extract_routing_fixture(markdown: &str) -> &str {
    let mut after_heading = false;
    let mut json_start = None;
    let mut offset = 0usize;

    for line_with_ending in markdown.split_inclusive('\n') {
        let line = line_with_ending.trim_end_matches(&['\r', '\n'][..]);
        let trimmed = line.trim();
        if !after_heading {
            after_heading = trimmed == "### Routing exercise fixture";
            offset += line_with_ending.len();
            continue;
        }

        if json_start.is_none() {
            if trimmed == "```json" {
                json_start = Some(offset + line_with_ending.len());
            }
            offset += line_with_ending.len();
            continue;
        }

        if trimmed == "```" {
            return &markdown[json_start.expect("json start recorded")..offset];
        }

        offset += line_with_ending.len();
    }

    panic!("j14 plan must include one routing exercise JSON fixture");
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_object()
        .and_then(|object| object.get(name))
        .unwrap_or_else(|| panic!("missing JSON field `{name}`"))
}

fn string_field<'a>(value: &'a Value, name: &str) -> &'a str {
    field(value, name)
        .as_str()
        .unwrap_or_else(|| panic!("JSON field `{name}` must be a string"))
}

fn bool_field(value: &Value, name: &str) -> bool {
    field(value, name)
        .as_bool()
        .unwrap_or_else(|| panic!("JSON field `{name}` must be a boolean"))
}

fn array_field<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    field(value, name)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("JSON field `{name}` must be an array"))
}

fn unsigned_field(value: &Value, name: &str) -> u64 {
    field(value, name)
        .as_u64()
        .unwrap_or_else(|| panic!("JSON field `{name}` must be an unsigned integer"))
}

fn parse_fixture() -> Value {
    serde_json::from_str(extract_routing_fixture(J14_PLAN))
        .expect("j14 routing exercise fixture must be valid JSON")
}

fn observed_seconds(record: &Value, name: &str) -> i64 {
    let value = string_field(record, name);
    let (_, time) = value
        .strip_suffix('Z')
        .and_then(|stamp| stamp.split_once('T'))
        .unwrap_or_else(|| panic!("JSON field `{name}` must be an RFC3339 UTC timestamp"));
    let mut parts = time.split(':');
    let hour = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .expect("timestamp hour must be numeric");
    let minute = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .expect("timestamp minute must be numeric");
    let second = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .expect("timestamp second must be numeric");
    hour * 3600 + minute * 60 + second
}

fn capacity_record_is_fresh(record: &Value) -> bool {
    let observed_at = observed_seconds(record, "observedAt");
    let decision_at = observed_seconds(record, "freshForDecisionAt");
    decision_at >= observed_at && decision_at - observed_at <= 60
}

fn route_case(case: &Value) -> RoutingDecision {
    let Some(record) = field(case, "currentCapacityRecord").as_object() else {
        return RoutingDecision {
            decision: "standby",
            reason: "current capacity signal absent",
        };
    };
    let record = Value::Object(record.clone());
    let forbidden_substitute = field(case, "forbiddenSubstitute").as_str();
    let active_session_count = unsigned_field(&record, "activeSessionCount");
    let blocked_or_sleeping_sessions = array_field(&record, "blockedOrSleepingSessions");
    let account_quota_status = string_field(&record, "accountQuotaStatus");
    let machine_headroom = string_field(&record, "machineHeadroom");
    let owned_file_bound = bool_field(&record, "ownedFileBound");
    let contradictions = array_field(&record, "contradictions");

    if !capacity_record_is_fresh(&record)
        || !contradictions.is_empty()
        || active_session_count == 0
        || blocked_or_sleeping_sessions
            .iter()
            .any(|entry| !entry.is_string())
        || account_quota_status != "verified_enough_for_expected_turn"
    {
        return RoutingDecision {
            decision: "refuse",
            reason: if forbidden_substitute == Some("borrow_other_account") {
                "failed live capacity check cannot be satisfied by borrowing authority"
            } else {
                "current capacity signal absent"
            },
        };
    }

    if machine_headroom != "verified_enough_for_focused_verification" {
        return RoutingDecision {
            decision: "refuse",
            reason: if forbidden_substitute == Some("change_CODEX_HOME") {
                "failed machine headroom check cannot be satisfied by changing CODEX_HOME"
            } else {
                "current capacity signal absent"
            },
        };
    }

    if !owned_file_bound {
        return RoutingDecision {
            decision: "standby",
            reason: if forbidden_substitute == Some("speculative_background_child") {
                "unbounded ownership cannot be satisfied by speculative background work"
            } else {
                "current capacity signal absent"
            },
        };
    }

    RoutingDecision {
        decision: "dispatch",
        reason: "fresh verified capacity and owned-file bound",
    }
}

#[test]
fn update_routing_decisions_from_real_codex_capacity_instead_of_stale_pools() {
    let fixture = parse_fixture();
    assert_eq!(
        string_field(&fixture, "name"),
        "Update routing decisions from real Codex capacity instead of stale pools."
    );

    let cases = array_field(&fixture, "cases");
    assert_eq!(
        cases.len(),
        5,
        "j14 must cover absent, allowed, and failed live checks"
    );
    for case in cases {
        assert_eq!(
            string_field(case, "historicalPoolLabel"),
            "available",
            "fixture must prove stale available labels are not routing authority"
        );
        let actual = route_case(case);
        assert_eq!(actual.decision, string_field(case, "expectedDecision"));
        assert_eq!(actual.reason, string_field(case, "expectedReason"));
    }
}
