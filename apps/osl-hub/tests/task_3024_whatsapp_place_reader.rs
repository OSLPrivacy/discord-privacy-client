#![cfg(feature = "core")]

use std::collections::BTreeSet;
use std::process::Command;

const WHATSAPP_PLACE_READER: &str = env!("CARGO_BIN_EXE_whatsapp-place-reader");

fn run(mode: &str) -> String {
    let output = Command::new(WHATSAPP_PLACE_READER)
        .arg(mode)
        .output()
        .expect("run WhatsApp place reader direct command");
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
fn task_3024_whatsapp_shared_place_reader_returns_each_seeded_kind_only_when_ticked() {
    let ticked = run("whatsapp-ticked");
    let unticked = run("whatsapp-unticked");

    let kinds = ticked
        .lines()
        .filter_map(|line| line.split(" kind=").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect::<BTreeSet<_>>();

    println!("{}", ticked.trim_end());
    println!("{}", unticked.trim_end());

    assert_eq!(
        field(&ticked, "TASK3024_DIRECT_READER="),
        "read_whatsapp_conversation_places"
    );
    assert_eq!(field(&ticked, "TASK3024_ACCOUNT_TICKED="), "true");
    assert_eq!(field(&ticked, "TASK3024_PLACE_COUNT="), "4");
    assert!(ticked.contains("TASK3024_PLACE name=SCRUB-W kind=direct_message"));
    assert_eq!(
        kinds,
        BTreeSet::from(["broadcast_list", "community", "direct_message", "group"])
    );
    assert_eq!(field(&unticked, "TASK3024_ACCOUNT_TICKED="), "false");
    assert_eq!(field(&unticked, "TASK3024_PLACE_COUNT="), "0");
    assert!(!unticked.contains("TASK3024_PLACE name="));
}
