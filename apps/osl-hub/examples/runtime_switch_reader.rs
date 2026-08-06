use osl_privacy_hub::runtime_switches::{
    assert_runtime_switch_status_safe, old_test_only_build_choice_reports,
    read_startup_test_only_runtime_switches,
    read_startup_test_only_runtime_switches_from_assignments, runtime_switch_status_lines,
    TEST_ONLY_RUNTIME_SWITCH_LIST,
};

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let status = args.first().map(String::as_str) == Some("status");
    if status {
        args.remove(0);
    }

    let switches = if args.is_empty() {
        read_startup_test_only_runtime_switches()
    } else {
        read_startup_test_only_runtime_switches_from_assignments(args.iter())
    };
    let switches = match switches {
        Ok(switches) => switches,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

    if status {
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
        return;
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
}
