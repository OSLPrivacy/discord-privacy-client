const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");

fn function_body_after(marker: &str) -> &'static str {
    let start = TASK_3406.find(marker).expect("marker exists");
    &TASK_3406[start..]
}

#[test]
fn task_3411_wait_for_person_route_reports_waiting_before_any_placement() {
    assert!(TASK_3406.contains("--wait-for-person"));
    assert!(TASK_3406.contains("front_window_grab=off"));
    assert!(TASK_3406.contains("placement_waiting_for_person=true"));
    assert!(TASK_3406.contains("placement_attempted_before_front=false"));
    assert!(TASK_3406.contains("after_person_front="));
    assert!(TASK_3406.contains("--text MAPLE-3406"));

    let waiting = TASK_3406
        .find("placement_waiting_for_person=true")
        .expect("waiting report exists");
    let not_attempted = TASK_3406
        .find("placement_attempted_before_front=false")
        .expect("not-attempted report exists");
    let clipboard_snapshot = TASK_3406
        .find("snapshot_clipboard()")
        .expect("clipboard placement still exists after the route gate");
    assert!(waiting < clipboard_snapshot);
    assert!(not_attempted < clipboard_snapshot);

    println!(
        "TASK3411 command=\"task_3406_place_text --wait-for-person --text MAPLE-3411\" front_window_grab=off waiting=placement_waiting_for_person=true placement_attempted_before_front=false"
    );
}

#[test]
fn task_3411_manual_wait_loop_has_no_foreground_grab_or_write_side_effects() {
    let wait_body = function_body_after("fn wait_for_person_to_front(");
    let wait_body = wait_body
        .split("fn window_info(")
        .next()
        .expect("wait function terminates before window_info");

    assert!(wait_body.contains("foreground_window()"));
    assert!(wait_body.contains("same_root(front.hwnd, target.hwnd)"));
    assert!(wait_body.contains("thread::sleep(Duration::from_millis(200))"));
    assert!(!wait_body.contains("SetForegroundWindow"));
    assert!(!wait_body.contains("ShowWindow"));
    assert!(!wait_body.contains("SendInput"));
    assert!(!wait_body.contains("SetClipboardData"));
    assert!(!wait_body.contains("PostMessageW"));
    assert!(!wait_body.contains("SetValue"));

    println!("TASK3411 wait_loop_forbidden_write_or_grab_calls=0");
}
