//! TASK 3158 - connect saved alert modes to received-message effects.
//!
//! A message always reaches this alert dispatcher. The selected mode decides
//! only whether the desktop host receives a notice and/or a sound request.

use ipc::commands::{cmd_osl_alert_for_received_message, cmd_osl_save_alert_mode_choice};
use ipc::state::AppState;

#[test]
fn task_3158_each_alert_mode_dispatches_the_required_notice_and_sound_counts() {
    let state = AppState::new();
    let mut counts = Vec::new();

    for (mode, expected_notices, expected_sounds) in
        [("silent", 0, 0), ("quiet", 1, 0), ("normal", 1, 1)]
    {
        cmd_osl_save_alert_mode_choice(&state, mode.to_owned(), None).expect("save alert mode");
        let effects = cmd_osl_alert_for_received_message(&state, format!("message-{mode}"))
            .expect("dispatch received-message alert");

        println!(
            "TASK3158 mode={} message={} notices={} sounds={}",
            effects.mode,
            effects.message_id,
            effects.notices.len(),
            effects.sounds.len()
        );

        assert_eq!(effects.mode, mode);
        assert_eq!(effects.notices.len(), expected_notices);
        assert_eq!(effects.sounds.len(), expected_sounds);
        assert!(effects.notices.iter().all(|id| id == &effects.message_id));
        assert!(effects.sounds.iter().all(|id| id == &effects.message_id));
        counts.push((effects.notices.len(), effects.sounds.len()));
    }

    assert_eq!(counts, [(0, 0), (1, 0), (1, 1)]);
}
