use task_3422_page_connection_placer::{run_direct_command, APP, COMPOSER};

#[test]
fn task_3422_direct_page_command_uses_the_shared_named_placer_without_grabbing_front() {
    let run = run_direct_command(false, APP, "MAPLE-3422")
        .expect("the connected named page accepts the marked text");

    assert!(run.page_connection);
    assert!(!run.front_window_grab);
    assert_eq!(run.app_starts, 0);
    assert_eq!(run.app, APP);
    assert_eq!(run.proof.editable_box_name, COMPOSER);
    assert_eq!(run.readback, "MAPLE-3422");
    assert_eq!(run.front_at_end, run.front_at_start);

    println!("TASK3422_TEST_APP={}", run.app);
    println!("TASK3422_TEST_TEXT={:?}", run.text);
    println!("TASK3422_TEST_READBACK={:?}", run.readback);
    println!(
        "TASK3422_TEST_FRONT_UNCHANGED={}",
        run.front_at_end == run.front_at_start
    );
}

#[test]
fn task_3422_front_window_grab_on_is_refused_before_placing() {
    let refusal = run_direct_command(true, APP, "MAPLE-3422")
        .expect_err("the page route must refuse an activation request");
    assert_eq!(refusal.to_string(), "website page is unavailable");
    println!("TASK3422_GRAB_ON_REFUSAL={refusal}");
}
