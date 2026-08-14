use task_4082_row_probe::{load_fixture, render_instagram_4083_report};

#[test]
fn task_4083_instagram_row_research_checks_all_eight_places() {
    let fixture = load_fixture("tests/fixtures/task_4082/instagram-open-conversation.json")
        .expect("fixture parses");
    let report = render_instagram_4083_report(&fixture);
    print!("{}", report.rendered);

    assert!(
        report.unchecked_places.is_empty(),
        "unchecked places: {:?}",
        report.unchecked_places
    );
    assert!(report.rendered.contains("TASK4083_INSTAGRAM_TOTAL_ROWS=10"));
    assert!(report
        .rendered
        .contains("TASK4083_INSTAGRAM_UNCHECKED_PLACES=0"));
    assert!(report
        .rendered
        .contains("TASK4083_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS=0"));
    assert!(report
        .rendered
        .contains("TASK4083_INSTAGRAM_UNMEASURED_CLAIMS=0"));
    assert!(report.rendered.contains(
        "TASK4083_INSTAGRAM_WINNING_SIGNAL=account_link_naming_account_for_their_rows_and_own_only_delivery_read_wording_for_own_rows"
    ));
}
