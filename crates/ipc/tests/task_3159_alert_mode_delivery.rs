//! TASK 3159 — exercise alert effects with a separately marked arrival.

use ipc::commands::{cmd_osl_alert_for_received_message, cmd_osl_save_alert_mode_choice};
use ipc::state::AppState;

#[test]
fn task_3159_marked_messages_arrive_with_the_exact_alert_effects_for_every_mode() {
    let state = AppState::new();
    let mut observed = Vec::new();

    for (mode, expected_notices, expected_sounds) in
        [("silent", 0, 0), ("quiet", 1, 0), ("normal", 1, 1)]
    {
        let marked_message = format!("TASK3159-marked-message-{mode}");
        cmd_osl_save_alert_mode_choice(&state, mode.to_owned(), None).expect("save alert mode");

        // Receiving the marked id is deliberately asserted independently of the
        // effects: silent has zero effects, but must still deliver the message.
        let received = cmd_osl_alert_for_received_message(&state, marked_message.clone())
            .expect("receive marked message and dispatch local effects");
        let arrived = received.message_id == marked_message;

        println!(
            "TASK3159 mode={} marked_message={} arrived={} notices={} sounds={}",
            received.mode,
            received.message_id,
            arrived,
            received.notices.len(),
            received.sounds.len()
        );

        assert!(arrived, "marked message must arrive in {mode} mode");
        assert_eq!(received.mode, mode);
        assert_eq!(received.notices.len(), expected_notices);
        assert_eq!(received.sounds.len(), expected_sounds);
        assert!(received.notices.iter().all(|id| id == &marked_message));
        assert!(received.sounds.iter().all(|id| id == &marked_message));
        observed.push((arrived, received.notices.len(), received.sounds.len()));
    }

    assert_eq!(observed, [(true, 0, 0), (true, 1, 0), (true, 1, 1)]);
}
