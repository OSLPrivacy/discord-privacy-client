use std::process::Command;

#[test]
fn task_4755_pair_label_is_green_and_one_sided_label_is_red() {
    let bin = env!("CARGO_BIN_EXE_task_4755_enumeration_proof");

    let green = Command::new(bin)
        .output()
        .expect("run task 4755 green harness");
    let green_stdout = String::from_utf8(green.stdout).expect("green stdout is utf8");
    eprint!("{}", String::from_utf8_lossy(&green.stderr));
    print!("{green_stdout}");
    assert!(green.status.success(), "green harness must exit 0");
    assert!(green_stdout.contains("TASK4755 identified 0 of 10000"));
    assert!(green_stdout.contains("TASK4755 drawers_fetched="));
    assert!(green_stdout.contains("TASK4755 cards_looked="));
    assert_eq!(green_stdout.lines().last(), Some("PLUM-4755 0"));

    let red = Command::new(bin)
        .arg("--one-sided")
        .output()
        .expect("run task 4755 one-sided harness");
    let red_stdout = String::from_utf8(red.stdout).expect("red stdout is utf8");
    eprint!("{}", String::from_utf8_lossy(&red.stderr));
    print!("{red_stdout}");
    assert_eq!(red.status.code(), Some(1), "one-sided harness must exit 1");
    assert!(red_stdout.contains("TASK4755 identified 10000 of 10000"));
    assert!(red_stdout.contains("TASK4755 first5="));
    assert_eq!(red_stdout.lines().last(), Some("PLUM-4755 10000"));
}
