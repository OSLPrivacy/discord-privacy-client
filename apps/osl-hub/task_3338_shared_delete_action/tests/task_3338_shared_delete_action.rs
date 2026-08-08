//! TASK 3338 finish line.
//!
//! Every test calls the module directly *and* runs the built command, so a
//! result that only holds inside the library cannot pass on its own.

use std::path::PathBuf;
use std::process::{Command, Output};

use task_3338_shared_delete_action::privacy_scan::{
    did_signed_in_account_send_message, MessageOwnerCheckInput,
};
use task_3338_shared_delete_action::shared_delete_action::{
    approve_delete, DeleteApproval, DeleteTarget, PerAppDeleteActions, ScrubMark,
    REFUSAL_DUPLICATE_DELETE_ACTION, REFUSAL_NOT_APPROVED, REFUSAL_NOT_YOURS,
};
use task_3338_shared_delete_action::{
    accepted_timed_delete_record, case_request, run_case, PlaceDeleteAction, World, MARKED_LOCATOR,
    PLAIN_LOCATOR, THEIRS_LOCATOR, TIMER_LOCATOR,
};

fn command_path() -> PathBuf {
    let mut path = std::env::current_exe().expect("test binary path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("task-3338-shared-delete-action")
}

fn run_command(args: &[&str]) -> (Output, String) {
    let output = Command::new(command_path())
        .args(args)
        .output()
        .expect("the TASK 3338 command runs");
    let text = String::from_utf8(output.stdout.clone()).expect("command prints utf-8");
    (output, text)
}

#[test]
fn task_3338_a_scrub_marked_owned_message_deletes_through_the_app_delete_action() {
    let mut world = World::new();
    let report = run_case(&mut world, "scrub-marked-mine").expect("known case");

    assert_eq!(report.approval.as_deref(), Some("scrub_mark"));
    assert_eq!(
        report.action_name.as_deref(),
        Some("discord.delete-own-message")
    );
    assert_eq!(report.calls, vec![MARKED_LOCATOR.to_owned()]);
    assert!(!report.after.contains(&MARKED_LOCATOR.to_owned()));
    assert!(!report.still_present);

    let (output, text) = run_command(&["--case", "scrub-marked-mine"]);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(
        text.contains("result=DELETED approved_by=scrub_mark action=discord.delete-own-message"),
        "{text}"
    );
}

#[test]
fn task_3338_an_unmarked_due_timer_owned_message_deletes_through_the_same_action() {
    let mut world = World::new();
    let report = run_case(&mut world, "timer-due-mine").expect("known case");

    // No mark at all: the accepted timed-delete record is the whole approval.
    let (request, mark) = case_request("timer-due-mine").expect("known case");
    assert_eq!(mark, "none");
    assert!(request.scrub_mark.is_none());
    assert_eq!(
        request.timed_delete_records,
        vec![accepted_timed_delete_record()]
    );

    assert_eq!(report.approval.as_deref(), Some("timed_delete_due"));
    assert_eq!(
        report.action_name.as_deref(),
        Some("discord.delete-own-message")
    );
    assert_eq!(report.calls, vec![TIMER_LOCATOR.to_owned()]);
    assert!(!report.still_present);

    let (output, text) = run_command(&["--case", "timer-due-mine"]);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(
        text.contains(
            "result=DELETED approved_by=timed_delete_due action=discord.delete-own-message"
        ),
        "{text}"
    );
}

#[test]
fn task_3338_both_approvals_reach_one_and_the_same_named_action() {
    let marked = run_case(&mut World::new(), "scrub-marked-mine").expect("known case");
    let timed = run_case(&mut World::new(), "timer-due-mine").expect("known case");

    assert_ne!(marked.approval, timed.approval);
    assert_eq!(marked.action_name, timed.action_name);
    assert_eq!(
        marked.action_name.as_deref(),
        Some("discord.delete-own-message")
    );

    // Same live action instance, twice, from the two different approvals.
    let mut world = World::new();
    let first = run_case(&mut world, "scrub-marked-mine").expect("known case");
    let second = run_case(&mut world, "timer-due-mine").expect("known case");
    assert_eq!(first.action_name, second.action_name);
    assert_eq!(
        second.calls,
        vec![MARKED_LOCATOR.to_owned(), TIMER_LOCATOR.to_owned()]
    );
    assert_eq!(
        second.after,
        vec![PLAIN_LOCATOR.to_owned(), THEIRS_LOCATOR.to_owned()]
    );
}

#[test]
fn task_3338_an_unmarked_message_with_no_due_record_is_refused() {
    let mut world = World::new();
    let report = run_case(&mut world, "unmarked-no-record-mine").expect("known case");

    assert_eq!(report.refusal_code.as_deref(), Some(REFUSAL_NOT_APPROVED));
    assert!(report.calls.is_empty(), "the app was never asked");
    assert_eq!(report.after, report.before);
    assert!(report.still_present);

    let (output, text) = run_command(&["--case", "unmarked-no-record-mine"]);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("result=ERR code=not_approved"), "{text}");
    assert!(
        text.contains("action_calls=[] still_present=true"),
        "{text}"
    );
}

#[test]
fn task_3338_another_persons_message_is_refused() {
    let mut world = World::new();
    let report = run_case(&mut world, "not-mine").expect("known case");

    // It is approved twice over — a confirmed mark and a due record — so the
    // only rule it breaks is ownership.
    let (request, _) = case_request("not-mine").expect("known case");
    assert_eq!(
        approve_delete(&request).expect("approved"),
        DeleteApproval::ConfirmedScrubMark
    );

    assert_eq!(report.refusal_code.as_deref(), Some(REFUSAL_NOT_YOURS));
    assert!(report.calls.is_empty(), "the app was never asked");
    assert!(report.still_present);

    let (output, text) = run_command(&["--case", "not-mine"]);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("result=ERR code=not_yours"), "{text}");
    assert!(
        text.contains("action_calls=[] still_present=true"),
        "{text}"
    );
}

#[test]
fn task_3338_each_app_has_exactly_one_delete_action() {
    let mut world = World::new();
    assert_eq!(world.actions.apps(), vec!["discord", "whatsapp"]);
    for app in ["discord", "whatsapp"] {
        assert_eq!(world.actions.action_count_for_app(app), 1, "{app}");
        assert_eq!(
            world.actions.action_names_for_app(app),
            vec![format!("{app}.delete-own-message")],
            "{app}"
        );
    }
    assert_eq!(world.actions.total_action_count(), 2);

    let (second, _handle) = PlaceDeleteAction::new("discord", Vec::new());
    let refusal = world
        .actions
        .register(Box::new(second))
        .expect_err("a second discord delete action is refused");
    assert_eq!(refusal.code, REFUSAL_DUPLICATE_DELETE_ACTION);
    assert_eq!(world.actions.action_count_for_app("discord"), 1);

    let (output, text) = run_command(&["--action-count"]);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(
        text.contains("app=discord delete_action_count=1 actions=[\"discord.delete-own-message\"]"),
        "{text}"
    );
    assert!(
        text.contains(
            "app=whatsapp delete_action_count=1 actions=[\"whatsapp.delete-own-message\"]"
        ),
        "{text}"
    );
    assert!(
        text.contains("second_action_for_discord=ERR code=duplicate_delete_action"),
        "{text}"
    );
    assert!(
        text.contains("after_duplicate app=discord delete_action_count=1"),
        "{text}"
    );
}

#[test]
fn task_3338_the_owner_check_is_the_shared_one_from_gate_3001() {
    // An unknown sender is refused with gate 3001's own wording, not guessed.
    let target = DeleteTarget::new("discord", "dm:task-3338", MARKED_LOCATOR);
    let (mut request, _) = case_request("scrub-marked-mine").expect("known case");
    request.message_sender = None;
    request.scrub_mark = Some(ScrubMark::confirmed(target));

    let mut actions = PerAppDeleteActions::new();
    let (action, handle) = PlaceDeleteAction::new(
        "discord",
        vec![DeleteTarget::new("discord", "dm:task-3338", MARKED_LOCATOR)],
    );
    actions.register(Box::new(action)).expect("one action");

    let refusal = task_3338_shared_delete_action::shared_delete_action::delete_one_owned_target(
        &mut actions,
        &request,
    )
    .expect_err("refused");
    assert_eq!(refusal.code, "owner_unknown");
    assert!(handle.call_locators().is_empty());

    // The refusal text is gate 3001's own, produced by calling gate 3001's
    // check directly with the same input.
    let gate_3001 = did_signed_in_account_send_message(MessageOwnerCheckInput {
        signed_in_account_sender: request.signed_in_account_sender.clone(),
        message_sender: request.message_sender.clone(),
    })
    .expect_err("gate 3001 refuses an unknown sender");
    assert_eq!(refusal.message, gate_3001.to_string());
    assert_eq!(refusal.message, "message sender is unknown");
}
