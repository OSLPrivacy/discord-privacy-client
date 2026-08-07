use ipc::commands::cmd_osl_check_message_timer_before_send;
use ipc::AppState;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

fn accept(
    state: &AppState,
    app_name: &str,
    requested_seconds: u64,
) -> ipc::commands::MessageTimerAdmissionDto {
    cmd_osl_check_message_timer_before_send(state, app_name.to_owned(), requested_seconds)
        .unwrap_or_else(|error| panic!("{app_name} should accept {requested_seconds}s: {error}"))
}

fn refuse(state: &AppState, app_name: &str, requested_seconds: u64) -> String {
    cmd_osl_check_message_timer_before_send(state, app_name.to_owned(), requested_seconds)
        .expect_err(&format!("{app_name} should refuse {requested_seconds}s"))
}

#[test]
fn task3302_direct_command_refuses_timers_the_app_cannot_keep_by_name() {
    let state = AppState::new();

    let messenger_9 = accept(&state, "Messenger", 9 * MINUTE);
    println!("TASK3302 direct_command=cmd_osl_check_message_timer_before_send");
    println!(
        "TASK3302 messenger_9_minutes.accepted={}",
        messenger_9.accepted
    );
    println!("TASK3302 messenger_9_minutes.app={}", messenger_9.app_name);
    println!(
        "TASK3302 messenger_9_minutes.requested_seconds={}",
        messenger_9.requested_seconds
    );
    assert_eq!(messenger_9.app_name, "Messenger");
    assert_eq!(messenger_9.requested_seconds, 9 * MINUTE);
    assert_eq!(messenger_9.max_seconds, 10 * MINUTE);
    assert!(messenger_9.accepted);

    let messenger_11 = refuse(&state, "Messenger", 11 * MINUTE);
    println!("TASK3302 messenger_11_minutes.refused=true");
    println!("TASK3302 messenger_11_minutes.error={messenger_11}");
    assert_eq!(
        messenger_11,
        "OSL: Messenger cannot keep a timer for 11 minutes; longest supported timer is 10 minutes"
    );

    let signal_23 = accept(&state, "Signal", 23 * HOUR);
    println!("TASK3302 signal_23_hours.accepted={}", signal_23.accepted);
    println!("TASK3302 signal_23_hours.app={}", signal_23.app_name);
    println!(
        "TASK3302 signal_23_hours.requested_seconds={}",
        signal_23.requested_seconds
    );
    assert_eq!(signal_23.app_name, "Signal");
    assert_eq!(signal_23.requested_seconds, 23 * HOUR);
    assert_eq!(signal_23.max_seconds, 24 * HOUR);
    assert!(signal_23.accepted);

    let signal_25 = refuse(&state, "Signal", 25 * HOUR);
    println!("TASK3302 signal_25_hours.refused=true");
    println!("TASK3302 signal_25_hours.error={signal_25}");
    assert_eq!(
        signal_25,
        "OSL: Signal cannot keep a timer for 25 hours; longest supported timer is 24 hours"
    );

    let whatsapp_59 = accept(&state, "WhatsApp", 59 * HOUR);
    println!(
        "TASK3302 whatsapp_59_hours.accepted={}",
        whatsapp_59.accepted
    );
    println!("TASK3302 whatsapp_59_hours.app={}", whatsapp_59.app_name);
    println!(
        "TASK3302 whatsapp_59_hours.requested_seconds={}",
        whatsapp_59.requested_seconds
    );
    assert_eq!(whatsapp_59.app_name, "WhatsApp");
    assert_eq!(whatsapp_59.requested_seconds, 59 * HOUR);
    assert_eq!(whatsapp_59.max_seconds, 60 * HOUR);
    assert!(whatsapp_59.accepted);

    let whatsapp_61 = refuse(&state, "WhatsApp", 61 * HOUR);
    println!("TASK3302 whatsapp_61_hours.refused=true");
    println!("TASK3302 whatsapp_61_hours.error={whatsapp_61}");
    assert_eq!(
        whatsapp_61,
        "OSL: WhatsApp cannot keep a timer for 61 hours; longest supported timer is 60 hours"
    );

    let discord_30 = accept(&state, "Discord", 30 * DAY);
    println!("TASK3302 discord_30_days.accepted={}", discord_30.accepted);
    println!("TASK3302 discord_30_days.app={}", discord_30.app_name);
    println!(
        "TASK3302 discord_30_days.requested_seconds={}",
        discord_30.requested_seconds
    );
    assert_eq!(discord_30.app_name, "Discord");
    assert_eq!(discord_30.requested_seconds, 30 * DAY);
    assert_eq!(discord_30.max_seconds, 30 * DAY);
    assert!(discord_30.accepted);
}
