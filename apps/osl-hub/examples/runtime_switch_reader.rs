use osl_privacy_hub::runtime_switches::{
    old_test_only_build_choice_reports, read_startup_test_only_runtime_switches_from_assignments,
    TEST_ONLY_RUNTIME_SWITCH_LIST,
};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let switches = match read_startup_test_only_runtime_switches_from_assignments(args.iter()) {
        Ok(switches) => switches,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };

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
