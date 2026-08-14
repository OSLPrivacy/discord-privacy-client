use osl_privacy_hub::row_who_wrote_it::SharedRowWhoWroteIt;
use osl_privacy_hub::x_owned_content_commands::{
    cmd_x_burn_both_sides_for_owned_content, cmd_x_burn_their_side_for_owned_content,
    cmd_x_burn_your_side_for_owned_content, cmd_x_timer_for_owned_content,
    cmd_x_view_once_for_owned_content, XOwnedContentCommand, XOwnedContentCommandError,
    XOwnedContentTarget,
};

const NOW_MS: u64 = 1_900_001_116_000;
const OWNED_ID: &str = "x-dm-task-1116-owned";
const PEER_ID: &str = "x-dm-task-1116-peer";

fn owned_target() -> XOwnedContentTarget {
    XOwnedContentTarget::yours(OWNED_ID)
}

fn peer_target() -> XOwnedContentTarget {
    XOwnedContentTarget {
        content_id: PEER_ID.to_owned(),
        who_wrote_it: SharedRowWhoWroteIt::Theirs,
    }
}

#[test]
fn task_1116_x_lifecycle_commands_return_expiry_for_owned_content_only() {
    let timer = cmd_x_timer_for_owned_content(&[owned_target()], NOW_MS, 300)
        .expect("the owned X timer target is accepted");
    let view_once = cmd_x_view_once_for_owned_content(&[owned_target()], NOW_MS, 10)
        .expect("the owned X view-once target is accepted");
    let your_side = cmd_x_burn_your_side_for_owned_content(&[owned_target()], NOW_MS, 60)
        .expect("the owned X your-side burn target is accepted");
    let their_side = cmd_x_burn_their_side_for_owned_content(&[owned_target()], NOW_MS, 60)
        .expect("the owned X their-side burn target is accepted");
    let both_sides = cmd_x_burn_both_sides_for_owned_content(&[owned_target()], NOW_MS, 60)
        .expect("the owned X both-sides burn target is accepted");

    let commands = [
        (&timer, XOwnedContentCommand::Timer, NOW_MS + 300_000),
        (&view_once, XOwnedContentCommand::ViewOnce, NOW_MS + 10_000),
        (
            &your_side,
            XOwnedContentCommand::BurnYourSide,
            NOW_MS + 60_000,
        ),
        (
            &their_side,
            XOwnedContentCommand::BurnTheirSide,
            NOW_MS + 60_000,
        ),
        (
            &both_sides,
            XOwnedContentCommand::BurnBothSides,
            NOW_MS + 60_000,
        ),
    ];
    for (receipt, command, expiry) in commands {
        assert_eq!(receipt.command, command);
        assert_eq!(receipt.target_ids, [OWNED_ID]);
        assert_eq!(receipt.expires_at_ms, expiry);
        println!(
            "TASK1116 command={} target_count={} targets={} expires_at_ms={}",
            command.name(),
            receipt.target_ids.len(),
            receipt.target_ids.join(","),
            receipt.expires_at_ms
        );
    }

    let foreign_refusals = [
        cmd_x_timer_for_owned_content(&[peer_target()], NOW_MS, 300),
        cmd_x_view_once_for_owned_content(&[peer_target()], NOW_MS, 10),
        cmd_x_burn_your_side_for_owned_content(&[peer_target()], NOW_MS, 60),
        cmd_x_burn_their_side_for_owned_content(&[peer_target()], NOW_MS, 60),
        cmd_x_burn_both_sides_for_owned_content(&[peer_target()], NOW_MS, 60),
    ];
    for refusal in &foreign_refusals {
        assert_eq!(
            refusal,
            &Err(XOwnedContentCommandError::TargetIsNotOwned(
                PEER_ID.to_owned()
            ))
        );
    }
    println!(
        "TASK1116 foreign_target_refusal_count={}",
        foreign_refusals.len()
    );
    println!("TASK1116 foreign_target={PEER_ID}");
}
