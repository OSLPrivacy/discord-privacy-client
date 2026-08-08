use std::time::Duration;

use task_3422_page_connection_placer::{
    run_shared_job_without_debugging_switch, APP,
};

const MARK: &str = "MAPLE-3423";

#[test]
fn task_3423_missing_debugging_switch_refuses_the_named_page_connection_and_falls_back() {
    let run = run_shared_job_without_debugging_switch(APP, MARK)
        .expect("the shared job falls back after the page connection refuses");

    assert_eq!(run.page_refusal.app, APP);
    assert_eq!(
        run.page_refusal.message,
        "Messenger: page connection refused: debugging switch is not enabled"
    );
    assert!(run.page_refusal.elapsed < Duration::from_secs(5));
    assert_eq!(run.page_refusal.placed_count, 0);
    assert_eq!(run.page_placed_count, 0);
    assert_eq!(run.route, "front-window");
    assert_eq!(run.front_window_placed_count, 1);
    assert_eq!(run.front_window_mark, MARK);

    println!("TASK3423_PAGE_REFUSAL_APP={}", run.page_refusal.app);
    println!("TASK3423_PAGE_REFUSAL={}", run.page_refusal.message);
    println!("TASK3423_PAGE_REFUSAL_MS={}", run.page_refusal.elapsed.as_millis());
    println!("TASK3423_PAGE_PLACED_COUNT={}", run.page_placed_count);
    println!("TASK3423_SHARED_JOB_ROUTE={}", run.route);
    println!("TASK3423_FRONT_WINDOW_PLACED_COUNT={}", run.front_window_placed_count);
    println!("TASK3423_FRONT_WINDOW_MARK={}", run.front_window_mark);
}
