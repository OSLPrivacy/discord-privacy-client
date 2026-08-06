use ipc::commands::{
    cmd_osl_save_message_defaults, cmd_osl_start_direct_new_message_plan, MessageDefaultsDto,
};
use ipc::AppState;

#[test]
fn task_0753_direct_new_message_plan_contains_all_four_saved_values() {
    let state = AppState::new();

    let saved = cmd_osl_save_message_defaults(
        &state,
        MessageDefaultsDto {
            burn_scope: "app".to_owned(),
            timer_seconds: 86_400,
            view_once_length_seconds: 45,
            cover_writing: "ai_covertext".to_owned(),
        },
        None,
    )
    .unwrap();

    let plan = cmd_osl_start_direct_new_message_plan(&state).unwrap();

    println!(
        "TASK0753 direct_new_message_plan.kind={}",
        plan.conversation_kind
    );
    println!(
        "TASK0753 direct_new_message_plan.burn_scope={}",
        plan.burn_scope
    );
    println!(
        "TASK0753 direct_new_message_plan.timer_seconds={}",
        plan.timer_seconds
    );
    println!(
        "TASK0753 direct_new_message_plan.view_once_length_seconds={}",
        plan.view_once_length_seconds
    );
    println!(
        "TASK0753 direct_new_message_plan.cover_writing={}",
        plan.cover_writing
    );

    assert_eq!(plan.conversation_kind, "direct");
    assert_eq!(plan.burn_scope, saved.burn_scope);
    assert_eq!(plan.timer_seconds, saved.timer_seconds);
    assert_eq!(
        plan.view_once_length_seconds,
        saved.view_once_length_seconds
    );
    assert_eq!(plan.cover_writing, saved.cover_writing);
}
