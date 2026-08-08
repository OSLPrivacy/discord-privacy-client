use osl_privacy_hub::security::task0863_run_per_screen_resets;

fn fixture_groups() -> Vec<String> {
    let fixture = match std::env::var("TASK0863_FIXTURE") {
        Ok(path) => std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("TASK0863 could not read fixture {path}: {error}")),
        Err(_) => include_str!("fixtures/task_0863_settings_reset.json").to_owned(),
    };
    let value: serde_json::Value =
        serde_json::from_str(&fixture).expect("TASK0863 fixture must be valid JSON");
    value
        .get("reset_groups")
        .and_then(serde_json::Value::as_array)
        .expect("TASK0863 fixture must contain reset_groups")
        .iter()
        .map(|group| {
            group
                .as_str()
                .expect("TASK0863 reset group must be a string")
                .to_owned()
        })
        .collect()
}

#[test]
fn task0863_per_screen_reset_is_narrow() {
    let observations = task0863_run_per_screen_resets(fixture_groups()).unwrap_or_else(|error| {
        panic!("{error}");
    });
    for observation in &observations {
        println!(
            "TASK0863_RESET group={} settings_at_default={} other_changed={}",
            observation.group, observation.settings_at_default, observation.other_changed
        );
        assert_eq!(observation.settings_at_default, 1);
        assert_eq!(observation.other_changed, 6);
    }
    println!("TASK0863_GROUPS_CHECKED={}", observations.len());
    assert_eq!(observations.len(), 7);
}
