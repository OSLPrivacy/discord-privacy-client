use osl_privacy_hub::row_who_wrote_it::SharedRowWhoWroteIt;
use osl_privacy_hub::x_owned_content_commands::{
    cmd_x_burn_both_sides_for_owned_content, cmd_x_timer_for_owned_content,
    cmd_x_view_once_for_owned_content, XOwnedContentCommandError, XOwnedContentTarget,
};

const NOW_MS: u64 = 1_900_001_118_000;
const OWNED_POST: &str = "x-own-1118";
const ANOTHER_PERSON_DELETE: &str = "another-person-delete";
const THIRTY_DAYS_SECONDS: u64 = 30 * 24 * 60 * 60;
const THIRTY_ONE_DAYS_SECONDS: u64 = 31 * 24 * 60 * 60;

fn owned_post() -> XOwnedContentTarget {
    XOwnedContentTarget::yours(OWNED_POST)
}

fn another_persons_post() -> XOwnedContentTarget {
    XOwnedContentTarget {
        content_id: ANOTHER_PERSON_DELETE.to_owned(),
        who_wrote_it: SharedRowWhoWroteIt::Theirs,
    }
}

#[test]
fn task_1118_changed_timer_view_once_and_delete_requests_are_named_and_atomic() {
    let accepted = cmd_x_timer_for_owned_content(&[owned_post()], NOW_MS, THIRTY_DAYS_SECONDS)
        .expect("the owned X post accepts the maximum timer");
    let timed_posts = accepted.target_ids.clone();

    assert_eq!(timed_posts, [OWNED_POST]);
    assert_eq!(timed_posts.len(), 1);
    println!(
        "TASK1118 good timed_post_count={} timed_posts={}",
        timed_posts.len(),
        timed_posts.join(",")
    );

    let thirty_one_days =
        cmd_x_timer_for_owned_content(&[owned_post()], NOW_MS, THIRTY_ONE_DAYS_SECONDS)
            .expect_err("31 days must be outside the X timer limit");
    assert_eq!(thirty_one_days.refusal_name(), "31-days");
    assert!(thirty_one_days.to_string().contains("31-days"));
    assert!(matches!(
        thirty_one_days,
        XOwnedContentCommandError::InvalidExpirySeconds { .. }
    ));
    println!(
        "TASK1118 changed_request_value=31-days refused_by_name={} timed_post_count={}",
        thirty_one_days.refusal_name(),
        timed_posts.len()
    );

    let sixty_one_seconds = cmd_x_view_once_for_owned_content(&[owned_post()], NOW_MS, 61)
        .expect_err("61 seconds must be outside the X view-once limit");
    assert_eq!(sixty_one_seconds.refusal_name(), "61-seconds");
    assert!(sixty_one_seconds.to_string().contains("61-seconds"));
    assert!(matches!(
        sixty_one_seconds,
        XOwnedContentCommandError::InvalidExpirySeconds { .. }
    ));
    println!(
        "TASK1118 changed_request_value=61-seconds refused_by_name={} timed_post_count={}",
        sixty_one_seconds.refusal_name(),
        timed_posts.len()
    );

    let another_person_delete =
        cmd_x_burn_both_sides_for_owned_content(&[another_persons_post()], NOW_MS, 60)
            .expect_err("another person's X post must not become a deletion target");
    assert_eq!(
        another_person_delete,
        XOwnedContentCommandError::TargetIsNotOwned(ANOTHER_PERSON_DELETE.to_owned())
    );
    assert_eq!(another_person_delete.refusal_name(), ANOTHER_PERSON_DELETE);
    assert!(another_person_delete
        .to_string()
        .contains(ANOTHER_PERSON_DELETE));
    println!(
        "TASK1118 changed_request_value={ANOTHER_PERSON_DELETE} refused_by_name={} timed_post_count={}",
        another_person_delete.refusal_name(),
        timed_posts.len()
    );

    assert_eq!(timed_posts, accepted.target_ids);
    assert_eq!(timed_posts, [OWNED_POST]);
    assert_eq!(timed_posts.len(), 1);
    println!(
        "TASK1118 final timed_post_count={} timed_posts={} unchanged=true",
        timed_posts.len(),
        timed_posts.join(",")
    );
}
