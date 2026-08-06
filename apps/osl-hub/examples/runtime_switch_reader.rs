use osl_privacy_hub::runtime_switches::{
    assert_runtime_switch_status_safe, old_test_only_build_choice_reports,
    read_startup_test_only_runtime_switches,
    read_startup_test_only_runtime_switches_from_assignments, runtime_switch_status_lines,
    ResolvedTestOnlyRunTimeSwitches, TEST_ONLY_RUNTIME_SWITCH_LIST,
};
use std::io::Write;

enum DirectCommand {
    Status,
    Unknown(String),
}

fn main() {
    let mut command = None;
    let mut hold_after_print_until_killed = false;
    let mut assignments = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "status" if command.is_none() => command = Some(DirectCommand::Status),
            "--hold-after-print-until-killed" => hold_after_print_until_killed = true,
            _ if command.is_none() && !arg.contains('=') => {
                command = Some(DirectCommand::Unknown(arg));
            }
            _ => assignments.push(arg),
        }
    }

    let switches = if assignments.is_empty() {
        read_startup_test_only_runtime_switches()
    } else {
        read_startup_test_only_runtime_switches_from_assignments(assignments.iter())
    };
    let switches = match switches {
        Ok(switches) => switches,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    match command {
        Some(DirectCommand::Status) => {
            print_safe_status_or_exit(switches);
            hold_for_cleanup_if_requested(hold_after_print_until_killed);
            return;
        }
        Some(DirectCommand::Unknown(name)) => {
            print_safe_status_or_exit(switches);
            eprintln!("unknown app command: {name}");
            std::process::exit(1);
        }
        None => {}
    }

    println!("RUN-TIME SWITCH LIST: {TEST_ONLY_RUNTIME_SWITCH_LIST}");
    println!(
        "password_screen_access={}",
        switches.password_screen_access.as_str()
    );
    println!("safe_sending={}", switches.safe_sending.as_str());
    for report in old_test_only_build_choice_reports(switches) {
        println!(
            "OLD CHOICE {}={} source={}",
            report.name, report.value, report.source
        );
    }
    hold_for_cleanup_if_requested(hold_after_print_until_killed);
}

fn print_safe_status_or_exit(switches: ResolvedTestOnlyRunTimeSwitches) {
    let lines = runtime_switch_status_lines(switches);
    println!("RUN-TIME SWITCH STATUS: count={}", lines.len());
    for line in &lines {
        println!("{}", line.render());
    }
    if let Err(error) = assert_runtime_switch_status_safe(&lines) {
        eprintln!("{error}");
        std::process::exit(2);
    }
    println!("RUN-TIME SWITCH STATUS: all-defaults-safe");
}

fn hold_for_cleanup_if_requested(enabled: bool) {
    if !enabled {
        return;
    }
    println!("TASK0062_DIRECT_COMMAND_HELD_FOR_CLEANUP=true");
    let _ = std::io::stdout().flush();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
