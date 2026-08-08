//! TASK 0777 - connect window, movement, tray, and sound choices.
//!
//! The four direct actions each read the persisted choice(s) they need.  This
//! test changes every saved value from its default and checks the action
//! result, rather than merely reading the preference map again.

use ipc::commands::{
    cmd_osl_apply_movement_behaviour, cmd_osl_open_window_with_behaviour,
    cmd_osl_play_sound_with_behaviour, cmd_osl_save_behaviour_choice,
    cmd_osl_show_tray_with_behaviour,
};
use ipc::AppState;

#[test]
fn task_0777_direct_actions_report_every_selected_behaviour() {
    let state = AppState::new();
    let saved = [
        ("position", "x=111,y=222,w=333,h=444"),
        ("remember place", "remember-last-window"),
        ("movement", "snap-to-current-screen"),
        ("tray picture", "contact-avatar"),
        ("sound", "soft-chime"),
        ("mute", "muted"),
        ("quiet hours", "22:15-06:45"),
    ];

    for (name, choice) in saved {
        cmd_osl_save_behaviour_choice(&state, name.to_string(), choice.to_string(), None)
            .unwrap_or_else(|error| panic!("save {name}: {error}"));
    }

    let window = cmd_osl_open_window_with_behaviour(&state).expect("window opening action");
    let movement = cmd_osl_apply_movement_behaviour(&state).expect("movement action");
    let tray = cmd_osl_show_tray_with_behaviour(&state).expect("tray action");
    let sound = cmd_osl_play_sound_with_behaviour(&state).expect("sound action");

    println!(
        "TASK0777 action={} position={} remember_place={}",
        window.action, window.position, window.remember_place
    );
    println!(
        "TASK0777 action={} movement={}",
        movement.action, movement.movement
    );
    println!(
        "TASK0777 action={} tray_picture={}",
        tray.action, tray.tray_picture
    );
    println!(
        "TASK0777 action={} sound={} mute={} quiet_hours={}",
        sound.action, sound.sound, sound.mute, sound.quiet_hours
    );

    assert_eq!(window.action, "open-window");
    assert_eq!(window.position, "x=111,y=222,w=333,h=444");
    assert_eq!(window.remember_place, "remember-last-window");
    assert_eq!(movement.action, "move-window");
    assert_eq!(movement.movement, "snap-to-current-screen");
    assert_eq!(tray.action, "show-tray");
    assert_eq!(tray.tray_picture, "contact-avatar");
    assert_eq!(sound.action, "play-sound");
    assert_eq!(sound.sound, "soft-chime");
    assert_eq!(sound.mute, "muted");
    assert_eq!(sound.quiet_hours, "22:15-06:45");
}
