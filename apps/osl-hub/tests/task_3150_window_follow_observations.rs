//! TASK 3150 - record the OSL position across foreground-app switches.

use ipc::app_preferences::FollowActiveAppChoice;
use osl_privacy_hub::follow_active_app_window::{
    FollowActiveAppWindowMover, WindowPosition, WindowRect,
};

#[test]
fn task_3150_window_follows_three_switches_then_stops_following() {
    let mut mover = FollowActiveAppWindowMover::default();
    let mut osl_position = WindowPosition { x: 40, y: 50 };
    let mut observed_positions = Vec::with_capacity(6);

    // Three distinct foreground applications while following is enabled.
    for (front_window, bounds) in [
        (101, WindowRect::new(100, 200, 700, 900)),
        (202, WindowRect::new(900, 320, 1300, 800)),
        (303, WindowRect::new(1500, 440, 2000, 1000)),
    ] {
        let before = osl_position;
        osl_position = mover
            .observe(FollowActiveAppChoice::On, front_window, bounds, osl_position)
            .expect("choice on must move OSL for each newly frontmost app");
        assert_ne!(
            osl_position, before,
            "choice on must change OSL's position for front window {front_window}"
        );
        observed_positions.push(osl_position);
    }

    // The same kind of foreground changes must leave OSL fixed once disabled.
    for (front_window, bounds) in [
        (404, WindowRect::new(2100, 120, 2600, 700)),
        (505, WindowRect::new(2700, 360, 3200, 960)),
        (606, WindowRect::new(3300, 540, 3900, 1100)),
    ] {
        let before = osl_position;
        assert_eq!(
            mover.observe(FollowActiveAppChoice::Off, front_window, bounds, osl_position),
            None,
            "choice off must not issue a move for front window {front_window}"
        );
        assert_eq!(
            osl_position, before,
            "choice off must retain OSL's position for front window {front_window}"
        );
        observed_positions.push(osl_position);
    }

    println!("TASK3150 positions={observed_positions:?}");
    assert_eq!(observed_positions.len(), 6, "all six positions are recorded");
    assert_ne!(observed_positions[0], observed_positions[1]);
    assert_ne!(observed_positions[1], observed_positions[2]);
    assert_eq!(observed_positions[3], observed_positions[2]);
    assert_eq!(observed_positions[4], observed_positions[2]);
    assert_eq!(observed_positions[5], observed_positions[2]);
}
