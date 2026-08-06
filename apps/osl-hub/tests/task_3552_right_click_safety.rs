use osl_privacy_hub::right_click_safety::{
    audit_fixed_test_screen_right_clicks, fixed_test_screen_targets, RightClickTargetKind,
};

#[test]
fn task_3552_right_click_every_fixed_screen_area_and_control_is_safe() {
    let targets = fixed_test_screen_targets();
    let report = audit_fixed_test_screen_right_clicks();
    let protected_targets = targets
        .iter()
        .filter(|target| target.kind == RightClickTargetKind::ProtectedText)
        .count();

    println!("TASK3552_RIGHT_CLICK_TARGET_COUNT={}", targets.len());
    println!("TASK3552_PROTECTED_TEXT_TARGET_COUNT={protected_targets}");
    for record in &report.records {
        match record.menu_id {
            Some(menu_id) => {
                let results = record
                    .action_results
                    .iter()
                    .map(|run| {
                        format!(
                            "{}=>{} did_the_thing={} private_marks={} unsafe_run={}",
                            run.action_id,
                            run.result_name,
                            run.did_the_thing,
                            run.exposed_private_marks,
                            run.unsafe_run
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                println!(
                    "TASK3552_RIGHT_CLICK target={} menu={} results=[{}]",
                    record.target_id, menu_id, results
                );
            }
            None => println!(
                "TASK3552_RIGHT_CLICK target={} menu=none results=[]",
                record.target_id
            ),
        }
    }
    println!("TASK3552_AREAS_SHOWING_MENU={}", report.areas_showing_menu);
    println!("TASK3552_SAFE_ACTIONS_RUN={}", report.safe_actions_run);
    println!(
        "TASK3552_NAMED_SAFE_ACTIONS_RUN={}",
        report.named_safe_actions_run
    );
    println!(
        "TASK3552_PRIVATE_MARKS_EXPOSED={}",
        report.private_marks_exposed
    );
    println!("TASK3552_UNSAFE_ACTIONS_RUN={}", report.unsafe_actions_run);
    println!(
        "TASK3552_UNNAMED_OR_UNSAFE_MENU_ACTIONS={}",
        report.unnamed_or_unsafe_menu_actions
    );
    println!(
        "TASK3552_FAILED_SAFE_RESULTS={}",
        report.failed_safe_results
    );

    assert!(
        report.areas_showing_menu > 0,
        "at least one right-click area must show a menu"
    );
    assert!(
        protected_targets > 0,
        "the fixed screen must include protected text"
    );
    assert!(
        report.safe_actions_run > 0,
        "at least one safe action must run"
    );
    assert_eq!(
        report.safe_actions_run, report.named_safe_actions_run,
        "each run safe action needs a named safe result"
    );
    assert_eq!(report.private_marks_exposed, 0);
    assert_eq!(report.unsafe_actions_run, 0);
    assert_eq!(
        report.unnamed_or_unsafe_menu_actions, 0,
        "every shown menu action must have a named safe result"
    );
    assert_eq!(
        report.failed_safe_results, 0,
        "safe actions must do the thing named by their result"
    );
}
