use ipc::commands::cmd_osl_list_signal_whitelist_kinds;
use std::process::{Command, Output};

fn run_fixture_place(kind: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_signal-fixture-place"))
        .arg(kind)
        .output()
        .expect("signal fixture-place helper must run")
}

#[test]
fn task_0152_signal_fixture_places_resolve_and_channel_exits_1() {
    let kinds = cmd_osl_list_signal_whitelist_kinds().expect("Signal kinds command");
    assert_eq!(kinds.len(), 2);

    let mut resolved_count = 0usize;
    for kind in kinds {
        let output = run_fixture_place(kind.id);
        let stdout = String::from_utf8(output.stdout).expect("fixture stdout is utf-8");
        let stderr = String::from_utf8(output.stderr).expect("fixture stderr is utf-8");
        assert!(
            output.status.success(),
            "Signal fixture place for {} failed: {stderr}",
            kind.id
        );
        assert!(
            stdout.contains("TASK 0152 fixture place resolved:"),
            "fixture place did not print a resolution line: {stdout}"
        );
        assert!(stdout.contains(&format!("kind={}", kind.id)));
        assert!(stdout.contains(&format!("auto_rule={}", kind.auto_rule_app_kind)));
        assert!(stdout.contains(&format!("allowed_place_kind={}", kind.allowed_place_kind)));
        print!("{stdout}");
        resolved_count += 1;
    }

    let rejected = run_fixture_place("channel");
    let rejected_code = rejected.status.code();
    let rejected_stderr =
        String::from_utf8(rejected.stderr).expect("fixture rejection stderr is utf-8");
    assert_eq!(rejected_code, Some(1), "{rejected_stderr}");
    assert!(
        rejected_stderr.contains("OSL: unknown Signal whitelist kind 'channel'"),
        "channel rejection did not name the refused kind: {rejected_stderr}"
    );
    println!(
        "TASK 0152 rejected extra kind: kind=channel exit_code={}",
        rejected_code.unwrap()
    );
    println!("TASK 0152 resolved Signal fixture places: {resolved_count}");
    assert_eq!(resolved_count, 2);
}
