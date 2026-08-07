use ipc::commands::{
    cmd_osl_begin_both_sides_burn_progress, cmd_osl_get_both_sides_burn_progress,
    cmd_osl_save_both_sides_burn_removal_progress,
};
use tempfile::TempDir;

fn step_names(report: &ipc::both_sides_burn_progress::BothSidesBurnProgressDto) -> String {
    report
        .steps
        .iter()
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

fn finished_names(report: &ipc::both_sides_burn_progress::BothSidesBurnProgressDto) -> String {
    report
        .steps
        .iter()
        .filter(|step| step.finished)
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn task3606_interrupted_both_sides_burn_saves_three_removal_steps() {
    let temp = TempDir::new().expect("temp progress dir");
    let path = temp.path().join("both-sides-burn-progress.json");
    let burn_id = "task3606-burn-1".to_owned();
    let selected = vec![
        "task3606-message-1".to_owned(),
        "task3606-message-2".to_owned(),
    ];

    let started =
        cmd_osl_begin_both_sides_burn_progress(path.clone(), burn_id.clone(), selected.clone())
            .expect("begin progress");
    assert_eq!(started.steps.len(), 3);
    assert_eq!(
        step_names(&started),
        "local removal|service removal|other-side removal"
    );
    assert!(!started.completed);

    let after_local = cmd_osl_save_both_sides_burn_removal_progress(
        path.clone(),
        burn_id.clone(),
        "local removal".to_owned(),
    )
    .expect("save local removal");
    assert_eq!(finished_names(&after_local), "local removal");
    assert!(!after_local.completed);

    drop(after_local);
    let reloaded_after_interrupt =
        cmd_osl_get_both_sides_burn_progress(path.clone(), burn_id.clone())
            .expect("reload interrupted progress");
    assert_eq!(finished_names(&reloaded_after_interrupt), "local removal");
    assert!(!reloaded_after_interrupt.completed);

    let after_service = cmd_osl_save_both_sides_burn_removal_progress(
        path.clone(),
        burn_id.clone(),
        "service removal".to_owned(),
    )
    .expect("save service removal");
    assert_eq!(
        finished_names(&after_service),
        "local removal|service removal"
    );
    assert!(!after_service.completed);

    let reloaded_before_other_side =
        cmd_osl_get_both_sides_burn_progress(path.clone(), burn_id.clone())
            .expect("reload before other-side progress");
    assert_eq!(
        finished_names(&reloaded_before_other_side),
        "local removal|service removal"
    );
    assert!(!reloaded_before_other_side.completed);

    let completed = cmd_osl_save_both_sides_burn_removal_progress(
        path,
        burn_id.clone(),
        "other-side removal".to_owned(),
    )
    .expect("save other-side removal");
    assert_eq!(
        finished_names(&completed),
        "local removal|service removal|other-side removal"
    );
    assert!(completed.completed);

    println!(
        "TASK3606 burn_id={} step_count={} step_names={} interrupted_completed={} restart_finished_steps={} completed_before_all_three={} completed_after_all_three={}",
        started.burn_id,
        started.steps.len(),
        step_names(&started),
        reloaded_after_interrupt.completed,
        finished_names(&reloaded_before_other_side),
        after_service.completed,
        completed.completed
    );
}
