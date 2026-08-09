use osl_privacy_hub::instagram_content_lifecycle::{
    cmd_instagram_burn_both_sides, cmd_instagram_set_timer, cmd_instagram_set_view_once,
    InstagramContentExpiry, InstagramContentSelection,
};
use osl_privacy_hub::services::{
    InstagramBrowserMachine, InstagramBrowserMessage, InstagramBrowserPlace,
    InstagramBrowserPlaceKind,
};

const NOW: i64 = 1_901_147_000;
const OWN_POST: &str = "instagram-own-1147";
const OTHER_PERSON: &str = "instagram-other-person-1147";
const THIRTY_ONE_DAYS_SECONDS: u32 = 31 * 24 * 60 * 60;
const SIXTY_ONE_SECONDS: u32 = 61;

fn browser() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([InstagramBrowserPlace::new(
        OWN_POST,
        "Task 1147 owned post",
        InstagramBrowserPlaceKind::OwnPost,
    )])
    .with_messages([
        InstagramBrowserMessage::new(OWN_POST, OWN_POST, "TASK1147 owned timed post", NOW, true),
        InstagramBrowserMessage::new(
            OWN_POST,
            OTHER_PERSON,
            "TASK1147 another person's item must survive",
            NOW,
            false,
        ),
    ])
}

fn selection(message_id: &str) -> InstagramContentSelection {
    InstagramContentSelection::new(OWN_POST, message_id)
}

fn matching_timed_posts<'a>(
    timed_posts: &'a [InstagramContentExpiry],
    message_id: &str,
) -> Vec<&'a InstagramContentExpiry> {
    timed_posts
        .iter()
        .filter(|post| post.target.message_id == message_id)
        .collect()
}

#[test]
fn task_1147_changed_time_and_delete_requests_are_refused_without_mutation() {
    let mut browser = browser();
    let browser_before = browser.clone();
    let mut timed_posts = Vec::new();

    let good = cmd_instagram_set_timer(&browser, &selection(OWN_POST), NOW, 3_600)
        .expect("good owned Instagram post accepts a one-hour timer");
    assert_eq!(good.target.kind, InstagramBrowserPlaceKind::OwnPost);
    assert_eq!(good.target.message_id, OWN_POST);
    assert_eq!(good.expires_at, NOW + 3_600);
    assert!(!good.view_once);
    timed_posts.push(good);

    let good_matches = matching_timed_posts(&timed_posts, OWN_POST);
    assert_eq!(good_matches.len(), 1);
    let timed_posts_before_attacks = timed_posts.clone();

    let thirty_one_days =
        cmd_instagram_set_timer(&browser, &selection(OWN_POST), NOW, THIRTY_ONE_DAYS_SECONDS)
            .expect_err("31-days must exceed the Instagram timer limit");
    assert_eq!(thirty_one_days.code(), "invalid_lifetime");

    let sixty_one_seconds =
        cmd_instagram_set_view_once(&browser, &selection(OWN_POST), NOW, SIXTY_ONE_SECONDS)
            .expect_err("61-seconds must exceed the Instagram view-once limit");
    assert_eq!(sixty_one_seconds.code(), "invalid_lifetime");

    let another_person_delete =
        cmd_instagram_burn_both_sides(&mut browser, &selection(OTHER_PERSON))
            .expect_err("another-person-delete must not delete unowned Instagram content");
    assert_eq!(another_person_delete.code(), "not_yours");

    assert_eq!(
        browser, browser_before,
        "refused deletion must leave the browser snapshot byte-for-byte equivalent"
    );
    assert_eq!(
        timed_posts, timed_posts_before_attacks,
        "refused changed requests must leave the timed-post set unchanged"
    );
    let final_matches = matching_timed_posts(&timed_posts, OWN_POST);
    assert_eq!(final_matches.len(), 1);
    assert_eq!(final_matches[0], good_matches[0]);

    println!("TASK1147_GOOD_TIMED_POST_COUNT={}", good_matches.len());
    println!("TASK1147_GOOD_TIMED_POST={OWN_POST}");
    println!(
        "TASK1147_REFUSAL name=31-days code={}",
        thirty_one_days.code()
    );
    println!(
        "TASK1147_REFUSAL name=61-seconds code={}",
        sixty_one_seconds.code()
    );
    println!(
        "TASK1147_REFUSAL name=another-person-delete code={}",
        another_person_delete.code()
    );
    println!("TASK1147_FINAL_TIMED_POST_COUNT={}", final_matches.len());
    println!("TASK1147_FINAL_TIMED_POST={OWN_POST}");
    println!("TASK1147_TIMED_POST_UNCHANGED=true");
}
