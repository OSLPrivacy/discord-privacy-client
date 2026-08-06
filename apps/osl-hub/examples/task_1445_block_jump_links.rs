use serde_json::Value;

use osl_privacy_hub::privacy_scan::{scan_local_messages, LocalMessageCandidate};

#[derive(Default)]
struct ForbiddenCounts {
    url_fields: usize,
    deep_link_fields: usize,
    open_action_fields: usize,
    url_values: usize,
    deep_link_values: usize,
    open_action_values: usize,
}

fn usage() -> ! {
    eprintln!("usage: task_1445_block_jump_links result-json");
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("result-json") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() != 1 {
        usage();
    }

    let result = scan_local_messages(vec![LocalMessageCandidate {
        service_id: "discord".to_owned(),
        account_id: "account-1445".to_owned(),
        conversation_id: "conversation-1445".to_owned(),
        message_locator:
            "https://discord.com/channels/@me/1445/456?deepLink=osl://open&openAction=open_service_host"
                .to_owned(),
        authored_by_self: true,
        created_at_unix_ms: Some(1_775_000_000_000),
        text: "password: task-1445 https://discord.com/channels/@me/1445/456?openAction=open_service_host"
            .to_owned(),
        attachments: Vec::new(),
    }]);
    let value = serde_json::to_value(&result).unwrap_or_else(|error| {
        eprintln!("TASK1445_ERROR=serialize value: {error}");
        std::process::exit(1);
    });
    let json = serde_json::to_string(&value).unwrap_or_else(|error| {
        eprintln!("TASK1445_ERROR=serialize json: {error}");
        std::process::exit(1);
    });
    let mut counts = ForbiddenCounts::default();
    count_forbidden(&value, &mut counts);
    let message_locator = result
        .findings
        .first()
        .map(|finding| finding.message_locator.as_str())
        .unwrap_or("none");

    println!("TASK1445_DIRECT_COMMAND=result-json");
    println!("TASK1445_FINDING_COUNT={}", result.findings.len());
    println!("TASK1445_MESSAGE_LOCATOR={message_locator}");
    println!("TASK1445_RESULT_JSON={json}");
    println!("TASK1445_URL_FIELD_COUNT={}", counts.url_fields);
    println!("TASK1445_DEEP_LINK_FIELD_COUNT={}", counts.deep_link_fields);
    println!(
        "TASK1445_OPEN_ACTION_FIELD_COUNT={}",
        counts.open_action_fields
    );
    println!("TASK1445_URL_VALUE_COUNT={}", counts.url_values);
    println!("TASK1445_DEEP_LINK_VALUE_COUNT={}", counts.deep_link_values);
    println!(
        "TASK1445_OPEN_ACTION_VALUE_COUNT={}",
        counts.open_action_values
    );

    if result.findings.len() != 1
        || message_locator != "blocked-local-reference"
        || counts.url_fields != 0
        || counts.deep_link_fields != 0
        || counts.open_action_fields != 0
        || counts.url_values != 0
        || counts.deep_link_values != 0
        || counts.open_action_values != 0
    {
        eprintln!("TASK1445_ERROR=finish line mismatch");
        std::process::exit(1);
    }
}

fn count_forbidden(value: &Value, counts: &mut ForbiddenCounts) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                count_key(key, counts);
                count_forbidden(child, counts);
            }
        }
        Value::Array(items) => {
            for item in items {
                count_forbidden(item, counts);
            }
        }
        Value::String(text) => count_value(text, counts),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn count_key(key: &str, counts: &mut ForbiddenCounts) {
    let lower = key.to_ascii_lowercase();
    if lower == "url" || lower.ends_with("url") || lower.contains("_url") {
        counts.url_fields += 1;
    }
    if lower.contains("deeplink") || lower.contains("deep_link") || lower.contains("deep-link") {
        counts.deep_link_fields += 1;
    }
    if lower.contains("openaction")
        || lower.contains("open_action")
        || lower.contains("open-action")
    {
        counts.open_action_fields += 1;
    }
}

fn count_value(text: &str, counts: &mut ForbiddenCounts) {
    let lower = text.to_ascii_lowercase();
    if lower.contains("://")
        || lower.starts_with("www.")
        || lower.contains(".com/")
        || lower.contains(".net/")
        || lower.contains(".org/")
    {
        counts.url_values += 1;
    }
    if lower.contains("deeplink") || lower.contains("deep_link") || lower.contains("deep-link") {
        counts.deep_link_values += 1;
    }
    if lower.contains("openaction")
        || lower.contains("open_action")
        || lower.contains("open-action")
        || lower.contains("open_service")
        || lower.contains("open-service")
        || lower.contains("openservice")
        || lower.contains("open action")
    {
        counts.open_action_values += 1;
    }
}
