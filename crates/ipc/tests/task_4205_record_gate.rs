use ipc::allowed_places::record_gated_place_kind_for_old_model;
use ipc::auto_whitelist_rules::normalize_auto_whitelist_app_kind;

#[test]
fn task_4205_record_list_gates_the_old_place_conversion() {
    let approved = record_gated_place_kind_for_old_model("Telegram", "Saved Messages")
        .expect("approved record must reach the old place conversion");
    let old_conversion = normalize_auto_whitelist_app_kind("telegram")
        .expect("old four-place conversion allows telegram");
    let approved_reached_old_conversion = approved == old_conversion;
    println!("TASK4205_APPROVED_APP=Telegram");
    println!("TASK4205_APPROVED_PLACE=Saved Messages");
    println!("TASK4205_APPROVED_OLD_MODEL_RESULT={approved}");
    println!(
        "TASK4205_APPROVED_REACHED_OLD_FOUR_PLACE_CONVERSION={approved_reached_old_conversion}"
    );
    assert_eq!(approved, "telegram");
    assert!(approved_reached_old_conversion);

    let telegram_story = record_gated_place_kind_for_old_model("Telegram", "story")
        .expect_err("look-only Telegram story must be refused before old conversion")
        .to_string();
    let whatsapp_status = record_gated_place_kind_for_old_model("WhatsApp", "status")
        .expect_err("look-only WhatsApp status must be refused before old conversion")
        .to_string();
    println!("TASK4205_LOOK_ONLY_TELEGRAM_STORY_REFUSAL={telegram_story}");
    println!("TASK4205_LOOK_ONLY_WHATSAPP_STATUS_REFUSAL={whatsapp_status}");
    assert!(telegram_story.contains("telegram story"));
    assert!(whatsapp_status.contains("whatsapp status"));

    let unknown = record_gated_place_kind_for_old_model("Signal", "Note to self")
        .expect_err("unknown place must be refused with its app and place")
        .to_string();
    println!("TASK4205_UNKNOWN_REFUSAL={unknown}");
    assert!(unknown.contains("Signal"));
    assert!(unknown.contains("Note to self"));

    let refusals = [&telegram_story, &whatsapp_status, &unknown];
    let empty_or_unsupported_only = refusals
        .iter()
        .filter(|message| refusal_message_is_empty_or_unsupported_only(message))
        .count();
    println!("TASK4205_EMPTY_OR_UNSUPPORTED_ONLY_REFUSALS={empty_or_unsupported_only}");
    assert_eq!(empty_or_unsupported_only, 0);
}

fn refusal_message_is_empty_or_unsupported_only(message: &str) -> bool {
    let normalized = message
        .trim()
        .trim_matches('.')
        .to_ascii_lowercase()
        .replace('-', " ");
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "unsupported" | "unsupported place" | "place unsupported" | "the place is unsupported"
        )
}
