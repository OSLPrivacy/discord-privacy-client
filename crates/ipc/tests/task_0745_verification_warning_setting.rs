use ipc::commands::{
    cmd_osl_check_verification_warning_before_sending,
    cmd_osl_check_verification_warning_on_opening, cmd_osl_read_verification_warning_choice,
    cmd_osl_save_verification_warning_choice,
};
use ipc::state::AppState;

#[test]
fn task_0745_connects_opening_and_sending_checks_to_saved_warning_choice() {
    let state = AppState::new();

    let cases = [
        ("every time", "opening", "sending"),
        ("once", "opening", "none"),
        ("before sending", "none", "sending"),
        ("never", "none", "none"),
    ];

    for (choice, expected_opening_point, expected_sending_point) in cases {
        let saved =
            cmd_osl_save_verification_warning_choice(&state, choice.to_string(), None).unwrap();
        let read_back = cmd_osl_read_verification_warning_choice(&state).unwrap();
        assert_eq!(saved.choice, choice);
        assert_eq!(read_back.choice, choice);

        let conversation_id = format!("task-0745-{choice}");
        let opening =
            cmd_osl_check_verification_warning_on_opening(&state, conversation_id.clone(), false)
                .unwrap();
        let sending = cmd_osl_check_verification_warning_before_sending(
            &state,
            conversation_id.clone(),
            false,
        )
        .unwrap();

        println!(
            "TASK0745 choice={} opening_warning_point={} sending_warning_point={}",
            choice, opening.warning_point, sending.warning_point
        );

        assert_eq!(opening.choice, choice);
        assert_eq!(sending.choice, choice);
        assert_eq!(opening.warning_point, expected_opening_point);
        assert_eq!(sending.warning_point, expected_sending_point);
    }

    let once_send_first_conversation = "task-0745-once-send-first".to_string();
    cmd_osl_save_verification_warning_choice(&state, "once".to_string(), None).unwrap();
    let once_send_first = cmd_osl_check_verification_warning_before_sending(
        &state,
        once_send_first_conversation,
        false,
    )
    .unwrap();
    println!(
        "TASK0745 choice=once send_first_warning_point={}",
        once_send_first.warning_point
    );
    assert_eq!(once_send_first.warning_point, "sending");

    let verified = cmd_osl_check_verification_warning_on_opening(
        &state,
        "task-0745-verified".to_string(),
        true,
    )
    .unwrap();
    println!(
        "TASK0745 verified_conversation_warning_point={}",
        verified.warning_point
    );
    assert_eq!(verified.warning_point, "none");
}
