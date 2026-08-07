use std::process::Command;

fn fixture(name: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    root.join("tests")
        .join("fixtures")
        .join("task_4082")
        .join(name)
        .display()
        .to_string()
}

#[test]
fn task_4083_instagram_row_research_checks_all_eight_places() {
    let output = Command::new(env!("CARGO_BIN_EXE_task-4082-row-probe"))
        .args([
            "--surface",
            "instagram-4083",
            "--fixture",
            &fixture("instagram-open-conversation.json"),
        ])
        .output()
        .expect("task 4083 probe runs");

    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    print!("{stdout}");
    eprint!("{stderr}");

    assert!(output.status.success());
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("TASK4083_ROW "))
            .count(),
        10
    );
    for expected in [
        "TASK4083_INSTAGRAM_PAGE_READ_DATE=2026-08-07",
        "TASK4083_INSTAGRAM_TOTAL_ROWS=10",
        "TASK4083_PLACE_1_TEST_NAME found_rows=10 empty_rows=0",
        "TASK4083_PLACE_2_PER_ROW_IDENTIFIER found_rows=10 empty_rows=0",
        "TASK4083_PLACE_3_PARENT_CONTAINERS_AND_BESIDE_BOXES found_rows=10 empty_rows=0",
        "TASK4083_PLACE_4_ROLES_AND_STATES found_rows=10 empty_rows=0",
        "TASK4083_PLACE_5_PICTURE_ADDRESS found_rows=5 empty_rows=5",
        "TASK4083_PLACE_6_ACCOUNT_LINK_NAMING_ACCOUNT found_rows=5 empty_rows=5",
        "TASK4083_PLACE_7_OWN_ONLY_DELIVERY_READ_WORDING found_rows=5 empty_rows=5 values=Delivered|Seen|Sent non_own_rows_with_wording=0",
        "TASK4083_PLACE_8_SCREEN_READER_TEXT found_rows=10 empty_rows=0",
        "TASK4083_INSTAGRAM_UNCHECKED_PLACES=0",
        "TASK4083_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS=0",
        "TASK4083_INSTAGRAM_UNMEASURED_CLAIMS=0",
        "TASK4083_INSTAGRAM_WINNING_SIGNAL=account_link_naming_account_for_their_rows_and_own_only_delivery_read_wording_for_own_rows",
        "TASK4083_INSTAGRAM_WINNING_SIGNAL_WHERE=place6_account_link_on_rows_00_02_04_06_08;place7_own_only_mark_on_rows_01_03_05_07_09",
        "TASK4083_INSTAGRAM_DEAD_END=not_a_dead_end",
    ] {
        assert!(stdout.contains(expected), "missing output: {expected}");
    }

    assert!(stdout.contains("place6_account_link=deckard->https://www.instagram.com/deckard/"));
    assert!(stdout.contains("place7_own_only_delivery_read_wording=Seen"));
    assert!(!stdout.contains("TASK4083_INSTAGRAM_DEAD_END=measured dead end"));
}
