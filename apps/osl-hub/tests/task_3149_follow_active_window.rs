//! TASK 3149 - the stored follow-active-app choice controls the window mover.

use ipc::commands::cmd_osl_set_follow_active_app_choice;
use ipc::AppState;
use osl_privacy_hub::follow_active_app_window::{
    FollowActiveAppWindowMover, WindowPosition, WindowRect, FOLLOW_ACTIVE_APP_GAP_PX,
};

fn saved_choice(state: &AppState) -> ipc::app_preferences::FollowActiveAppChoice {
    state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .follow_active_app_choice
}

#[test]
fn task_3149_on_moves_beside_a_changed_front_app_and_off_leaves_position_alone() {
    let state = AppState::new();
    let initial = WindowPosition { x: 40, y: 50 };
    let mut mover = FollowActiveAppWindowMover::default();

    cmd_osl_set_follow_active_app_choice(&state, "on", None).expect("save on");
    let first = mover
        .observe(
            saved_choice(&state),
            71,
            WindowRect::new(100, 300, 740, 900),
            initial,
        )
        .expect("front app A moves OSL");
    let second = mover
        .observe(
            saved_choice(&state),
            72,
            WindowRect::new(900, 120, 1500, 800),
            first,
        )
        .expect("front app B moves OSL again");

    cmd_osl_set_follow_active_app_choice(&state, "off", None).expect("save off");
    let off_move = mover.observe(
        saved_choice(&state),
        73,
        WindowRect::new(1600, 600, 2000, 1000),
        second,
    );

    println!(
        "TASK3149 choice_on_positions=({},{})->({},{}) choice_off_move_count={}",
        first.x,
        first.y,
        second.x,
        second.y,
        usize::from(off_move.is_some())
    );
    assert_eq!(
        first,
        WindowPosition {
            x: 740 + FOLLOW_ACTIVE_APP_GAP_PX,
            y: 300,
        }
    );
    assert_eq!(
        second,
        WindowPosition {
            x: 1500 + FOLLOW_ACTIVE_APP_GAP_PX,
            y: 120,
        }
    );
    assert_ne!(first, second, "front-app change must change OSL position");
    assert_eq!(off_move, None, "off must not issue a position change");
}
