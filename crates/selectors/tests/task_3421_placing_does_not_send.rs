use selectors::{
    place_marked_message_in_all_conversations, seeded_task_3421_conversations,
    task_3421_finish_line_holds, DraftOnlyPlacingJob, MarkedMessagePlacingJob, NoopPlacingJob,
};

const MARKED_TEXT: &str = "TASK3421-MARKED-UNSENT-7f3c9a";

#[test]
fn task_3421_placing_marked_messages_does_not_send() {
    let mut conversations = seeded_task_3421_conversations();
    let draft_job = DraftOnlyPlacingJob;
    let noop_job = NoopPlacingJob;
    let use_noop = std::env::var_os("OSL_TASK_3421_STUB_PLACING").is_some();
    let job: &dyn MarkedMessagePlacingJob = if use_noop { &noop_job } else { &draft_job };

    let observations =
        place_marked_message_in_all_conversations(&mut conversations, MARKED_TEXT, job);

    println!("TASK3421_MARKED_TEXT={MARKED_TEXT}");
    println!("TASK3421_STUBBED_PLACING={use_noop}");
    for observation in &observations {
        println!(
            "TASK3421 service={} sent_before={} sent_after={} pressed_controls_after={} draft_after={}",
            observation.surface.as_str(),
            observation.sent_before,
            observation.sent_after,
            observation.pressed_controls_after,
            observation.draft_after
        );
    }

    assert!(
        task_3421_finish_line_holds(&observations, MARKED_TEXT),
        "marked text must be unsent in all four boxes and sent counts must not change"
    );
}
