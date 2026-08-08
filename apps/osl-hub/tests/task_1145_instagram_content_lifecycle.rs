use osl_privacy_hub::instagram_content_lifecycle::{
    cmd_instagram_burn_both_sides, cmd_instagram_burn_their_side, cmd_instagram_burn_your_side,
    cmd_instagram_set_timer, cmd_instagram_set_view_once, InstagramBurnSide,
    InstagramContentSelection, INSTAGRAM_DEFAULT_VIEW_ONCE_SECONDS,
};
use osl_privacy_hub::services::{
    InstagramBrowserMachine, InstagramBrowserMessage, InstagramBrowserPlace,
    InstagramBrowserPlaceKind,
};

const NOW: i64 = 1_901_145_000;
const DM: &str = "instagram-dm-1145";
const POST: &str = "instagram-post-1145";
const COMMENT: &str = "instagram-comment-1145";

fn browser() -> InstagramBrowserMachine {
    InstagramBrowserMachine::new([
        InstagramBrowserPlace::new(
            DM,
            "Task 1145 direct message",
            InstagramBrowserPlaceKind::DirectMessage,
        ),
        InstagramBrowserPlace::new(POST, "Task 1145 post", InstagramBrowserPlaceKind::OwnPost),
        InstagramBrowserPlace::new(
            COMMENT,
            "Task 1145 comment",
            InstagramBrowserPlaceKind::OwnComment,
        ),
    ])
    .with_messages([
        InstagramBrowserMessage::new(DM, "timer-mine", "TASK1145 timer", NOW, true),
        InstagramBrowserMessage::new(DM, "view-once-mine", "TASK1145 view once", NOW, true),
        InstagramBrowserMessage::new(DM, "burn-yours-mine", "TASK1145 burn yours", NOW, true),
        InstagramBrowserMessage::new(POST, "burn-theirs-mine", "TASK1145 burn theirs", NOW, true),
        InstagramBrowserMessage::new(COMMENT, "burn-both-mine", "TASK1145 burn both", NOW, true),
        InstagramBrowserMessage::new(POST, "other-person", "TASK1145 must survive", NOW, false),
    ])
}

fn selection(place_id: &str, message_id: &str) -> InstagramContentSelection {
    InstagramContentSelection::new(place_id, message_id)
}

#[test]
fn task_1145_instagram_commands_return_expiry_and_target_only_owned_content() {
    let mut browser = browser();

    let timer = cmd_instagram_set_timer(&browser, &selection(DM, "timer-mine"), NOW, 3_600)
        .expect("timer accepts owned Instagram content");
    assert_eq!(timer.expires_at, NOW + 3_600);
    assert!(!timer.view_once);
    assert_eq!(timer.target.message_id, "timer-mine");

    let view_once = cmd_instagram_set_view_once(
        &browser,
        &selection(DM, "view-once-mine"),
        NOW,
        INSTAGRAM_DEFAULT_VIEW_ONCE_SECONDS,
    )
    .expect("view once accepts owned Instagram content");
    assert_eq!(
        view_once.expires_at,
        NOW + i64::from(INSTAGRAM_DEFAULT_VIEW_ONCE_SECONDS)
    );
    assert!(view_once.view_once);
    assert_eq!(view_once.target.message_id, "view-once-mine");

    let yours = cmd_instagram_burn_your_side(&mut browser, &selection(DM, "burn-yours-mine"))
        .expect("your-side burn accepts owned Instagram content");
    assert_eq!(yours.side, InstagramBurnSide::YourSide);
    assert!(yours.local_deleted);
    assert!(!yours.remote_burn_requested);

    let theirs = cmd_instagram_burn_their_side(&mut browser, &selection(POST, "burn-theirs-mine"))
        .expect("their-side burn accepts owned Instagram content");
    assert_eq!(theirs.side, InstagramBurnSide::TheirSide);
    assert!(!theirs.local_deleted);
    assert!(theirs.remote_burn_requested);

    let both = cmd_instagram_burn_both_sides(&mut browser, &selection(COMMENT, "burn-both-mine"))
        .expect("both-sides burn accepts owned Instagram content");
    assert_eq!(both.side, InstagramBurnSide::BothSides);
    assert!(both.local_deleted);
    assert!(both.remote_burn_requested);

    let unowned = selection(POST, "other-person");
    let refusals = [
        cmd_instagram_set_timer(&browser, &unowned, NOW, 60)
            .expect_err("timer refuses another person's target")
            .code(),
        cmd_instagram_set_view_once(&browser, &unowned, NOW, 60)
            .expect_err("view once refuses another person's target")
            .code(),
        cmd_instagram_burn_your_side(&mut browser, &unowned)
            .expect_err("your-side burn refuses another person's target")
            .code(),
        cmd_instagram_burn_their_side(&mut browser, &unowned)
            .expect_err("their-side burn refuses another person's target")
            .code(),
        cmd_instagram_burn_both_sides(&mut browser, &unowned)
            .expect_err("both-sides burn refuses another person's target")
            .code(),
    ];
    assert_eq!(refusals, ["not_yours"; 5]);
    assert!(browser
        .messages
        .iter()
        .any(|row| row.message_id == "other-person"));
    assert!(browser
        .messages
        .iter()
        .any(|row| row.message_id == "burn-theirs-mine"));
    assert!(!browser
        .messages
        .iter()
        .any(|row| row.message_id == "burn-yours-mine"));
    assert!(!browser
        .messages
        .iter()
        .any(|row| row.message_id == "burn-both-mine"));

    println!("TASK1145_TIMER_EXPIRES_AT={}", timer.expires_at);
    println!("TASK1145_VIEW_ONCE_EXPIRES_AT={}", view_once.expires_at);
    println!(
        "TASK1145_BURN_YOUR_SIDE local_deleted={} remote_requested={}",
        yours.local_deleted, yours.remote_burn_requested
    );
    println!(
        "TASK1145_BURN_THEIR_SIDE local_deleted={} remote_requested={}",
        theirs.local_deleted, theirs.remote_burn_requested
    );
    println!(
        "TASK1145_BURN_BOTH_SIDES local_deleted={} remote_requested={}",
        both.local_deleted, both.remote_burn_requested
    );
    println!(
        "TASK1145_OWNERSHIP_REFUSALS={} code={}",
        refusals.len(),
        refusals[0]
    );
}
