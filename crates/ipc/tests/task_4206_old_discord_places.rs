use ipc::allowed_places::record_gated_place_kind_for_old_model;
use ipc::auto_whitelist_rules::normalize_auto_whitelist_app_kind;

struct OldPlace {
    label: &'static str,
    exact_place_name: &'static str,
    old_model_input: &'static str,
}

const OLD_DISCORD_PLACES: [OldPlace; 4] = [
    OldPlace {
        label: "direct message",
        exact_place_name: "direct message",
        old_model_input: "discord:direct_message",
    },
    OldPlace {
        label: "group chat",
        exact_place_name: "group chat",
        old_model_input: "discord:group_chat",
    },
    OldPlace {
        label: "server channel",
        exact_place_name: "server channel",
        old_model_input: "discord:server_channel",
    },
    OldPlace {
        label: "whole server",
        exact_place_name: "server",
        old_model_input: "discord:server",
    },
];

#[test]
fn task_4206_old_discord_places_keep_their_old_answers_after_record_gate() {
    let mut allowed = 0;
    let mut changed = 0;

    for place in OLD_DISCORD_PLACES {
        let old_answer = normalize_auto_whitelist_app_kind(place.old_model_input)
            .expect("old Discord place model allowed this place before task 4205");
        let new_answer = record_gated_place_kind_for_old_model("Discord", place.exact_place_name);
        let new_answer_text = match &new_answer {
            Ok(answer) => answer.clone(),
            Err(error) => error.to_string(),
        };
        let answer_changed = new_answer.as_ref() != Ok(&old_answer);
        if !answer_changed {
            allowed += 1;
        } else {
            changed += 1;
        }

        println!(
            "TASK4206_OLD_PLACE label={} old_answer={} new_answer={} allowed={} changed={}",
            place.label, old_answer, new_answer_text, !answer_changed, answer_changed
        );
        if answer_changed {
            eprintln!("TASK4206_OLD_PLACE_ANSWER_CHANGED={}", place.label);
            std::process::exit(1);
        }
    }

    println!("TASK4206_OLD_PLACES_ALLOWED={allowed}");
    println!("TASK4206_OLD_PLACES_CHANGED={changed}");
    assert_eq!(allowed, 4);
    assert_eq!(changed, 0);

    let invented = record_gated_place_kind_for_old_model("Discord", "moon base")
        .expect_err("invented Discord place must be refused by name")
        .to_string();
    println!("TASK4206_INVENTED_DISCORD_PLACE_REFUSAL={invented}");
    assert!(invented.contains("Discord"));
    assert!(invented.contains("moon base"));
}
