//! Per-app timer limit admission for chat surfaces OSL does not control.
//!
//! The policy is pure: callers provide the app and requested timer duration,
//! and get back either an exact admission or the product refusal text.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChatAppTimerPolicy {
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub limit_hours: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcceptedChatAppTimer {
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub requested_seconds: u32,
    pub limit_hours: u32,
}

pub const CHAT_APP_TIMER_POLICIES: [ChatAppTimerPolicy; 6] = [
    ChatAppTimerPolicy {
        app_id: "snapchat",
        display_name: "Snapchat",
        limit_hours: 24,
    },
    ChatAppTimerPolicy {
        app_id: "x",
        display_name: "X",
        limit_hours: 24,
    },
    ChatAppTimerPolicy {
        app_id: "messenger",
        display_name: "Messenger",
        limit_hours: 24,
    },
    ChatAppTimerPolicy {
        app_id: "slack",
        display_name: "Slack",
        limit_hours: 24,
    },
    ChatAppTimerPolicy {
        app_id: "linkedin",
        display_name: "LinkedIn",
        limit_hours: 24,
    },
    ChatAppTimerPolicy {
        app_id: "teams",
        display_name: "Teams",
        limit_hours: 24,
    },
];

pub fn chat_app_timer_policy(app_id: &str) -> Option<ChatAppTimerPolicy> {
    CHAT_APP_TIMER_POLICIES
        .iter()
        .copied()
        .find(|policy| policy.app_id == app_id)
}

pub fn evaluate_chat_app_timer_request(
    app_id: &str,
    requested_seconds: u32,
) -> Result<AcceptedChatAppTimer, String> {
    let policy = chat_app_timer_policy(app_id)
        .ok_or_else(|| format!("OSL does not know a timer limit for {app_id}"))?;
    let limit_seconds = policy.limit_hours.saturating_mul(60 * 60);
    if requested_seconds == 0 {
        return Err(format!(
            "{} timer request is refused: requested timer must be positive and no more than {} hours.",
            policy.display_name, policy.limit_hours
        ));
    }
    if requested_seconds > limit_seconds {
        return Err(format!(
            "{} timer request is refused: {} supports timers up to {} hours.",
            policy.display_name, policy.display_name, policy.limit_hours
        ));
    }
    Ok(AcceptedChatAppTimer {
        app_id: policy.app_id,
        display_name: policy.display_name,
        requested_seconds,
        limit_hours: policy.limit_hours,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct SurfaceRuling {
        cut_surfaces: Vec<String>,
    }

    fn ruled_chat_apps() -> Vec<String> {
        let ruling: SurfaceRuling =
            serde_json::from_str(include_str!("../../../data/surface-ruling-2026-08-05.json"))
                .expect("surface ruling parses");
        ruling.cut_surfaces
    }

    #[test]
    fn task_3303_timer_inside_accepted_outside_refused_for_every_chat_app() {
        let ruled = ruled_chat_apps();
        let declared: Vec<&str> = CHAT_APP_TIMER_POLICIES
            .iter()
            .map(|policy| policy.app_id)
            .collect();
        assert_eq!(
            ruled, declared,
            "timer policy must cover every ruled app exactly once"
        );

        let mut accepted = 0usize;
        let mut refused = 0usize;
        let mut results = 0usize;

        for policy in CHAT_APP_TIMER_POLICIES {
            let limit_seconds = policy.limit_hours * 60 * 60;
            let inside_seconds = limit_seconds - 60;
            let outside_seconds = limit_seconds + 60;

            let inside = evaluate_chat_app_timer_request(policy.app_id, inside_seconds)
                .expect("one minute inside the app limit must be accepted");
            assert_eq!(inside.display_name, policy.display_name);
            assert_eq!(inside.limit_hours, policy.limit_hours);
            println!(
                "TASK3303_RESULT app={} name={} request=inside requested_seconds={} limit_hours={} accepted=true",
                inside.app_id, inside.display_name, inside.requested_seconds, inside.limit_hours
            );
            accepted += 1;
            results += 1;

            let refusal = evaluate_chat_app_timer_request(policy.app_id, outside_seconds)
                .expect_err("one minute outside the app limit must be refused");
            assert!(
                refusal.contains(policy.display_name),
                "refusal must name {}: {refusal}",
                policy.display_name
            );
            assert!(
                refusal.contains(&format!("{} hours", policy.limit_hours)),
                "refusal must state {}'s own hour limit: {refusal}",
                policy.display_name
            );
            println!(
                "TASK3303_RESULT app={} name={} request=outside requested_seconds={} limit_hours={} accepted=false refusal={}",
                policy.app_id, policy.display_name, outside_seconds, policy.limit_hours, refusal
            );
            refused += 1;
            results += 1;
        }

        println!("TASK3303_ACCEPTED_INSIDE={accepted}");
        println!("TASK3303_REFUSED_OUTSIDE={refused}");
        println!("TASK3303_TOTAL_RESULTS={results}");
        assert_eq!(accepted, 7);
        assert_eq!(refused, 7);
        assert_eq!(results, 14);
    }
}
