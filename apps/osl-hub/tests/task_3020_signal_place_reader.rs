#![cfg(feature = "core")]

use std::collections::BTreeSet;
use std::process::Command;

const SIGNAL_PLACE_READER: &str = env!("CARGO_BIN_EXE_signal-place-reader");

fn run(mode: &str) -> String {
    let output = Command::new(SIGNAL_PLACE_READER)
        .arg(mode)
        .output()
        .expect("run Signal place reader direct command");
    assert!(
        output.status.success(),
        "direct command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("direct command emits UTF-8")
}

fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(key))
        .unwrap_or_else(|| panic!("missing {key:?} in {output:?}"))
}

#[test]
fn task_3020_signal_shared_place_reader_returns_each_seeded_kind_only_when_ticked() {
    let ticked = run("signal-ticked");
    let unticked = run("signal-unticked");

    let kinds = ticked
        .lines()
        .filter_map(|line| line.split(" kind=").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect::<BTreeSet<_>>();

    println!("{}", ticked.trim_end());
    println!("{}", unticked.trim_end());

    assert_eq!(
        field(&ticked, "TASK3020_DIRECT_READER="),
        "read_signal_conversation_places"
    );
    assert_eq!(field(&ticked, "TASK3020_ACCOUNT_TICKED="), "true");
    assert_eq!(field(&ticked, "TASK3020_PLACE_COUNT="), "3");
    assert!(ticked.contains("TASK3020_PLACE name=SCRUB-S kind=direct_message"));
    assert_eq!(
        kinds,
        BTreeSet::from(["direct_message", "group", "note_to_self"])
    );
    assert_eq!(field(&unticked, "TASK3020_ACCOUNT_TICKED="), "false");
    assert_eq!(field(&unticked, "TASK3020_PLACE_COUNT="), "0");
    assert!(!unticked.contains("TASK3020_PLACE name="));
}
