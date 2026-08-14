//! TASK 3338 direct command.
//!
//! `--case <name>` runs one delete request against a fresh fixture world and
//! prints what happened; `--action-count` prints how many delete actions each
//! app has and shows a second one being refused. Exit status is 0 for an
//! accepted delete and 1 for a refusal, so a caller does not have to read the
//! text to tell them apart.

use task_3338_shared_delete_action::shared_delete_action::AppDeleteAction;
use task_3338_shared_delete_action::{run_case, CaseReport, PlaceDeleteAction, World, CASES};

fn usage() -> String {
    format!(
        "usage: task-3338-shared-delete-action --case <{}> | --action-count",
        CASES.join("|")
    )
}

fn print_case(report: &CaseReport) {
    let target = &report.target;
    println!("### case {}", report.case);
    println!(
        "TASK3338_BEFORE command=delete_one_owned_target case={} app={} conversation={} place_messages={:?}",
        report.case, target.app, target.conversation, report.before
    );
    println!(
        "TASK3338_APPROVAL command=delete_one_owned_target case={} message={} sender=\"{}\" signed_in_sender=\"{}\" scrub_mark={} timed_delete_records={}",
        report.case,
        target.locator,
        report.sender,
        task_3338_shared_delete_action::SIGNED_IN_ACCOUNT,
        report.mark,
        report.records
    );
    match (&report.approval, &report.action_name) {
        (Some(approval), Some(action)) => println!(
            "TASK3338_DELETE command=delete_one_owned_target case={} message={} result=DELETED approved_by={} action={}",
            report.case, target.locator, approval, action
        ),
        _ => println!(
            "TASK3338_REFUSAL command=delete_one_owned_target case={} message={} result=ERR code={} error=\"{}\"",
            report.case,
            target.locator,
            report.refusal_code.as_deref().unwrap_or("?"),
            report.refusal_message.as_deref().unwrap_or("?")
        ),
    }
    println!(
        "TASK3338_AFTER command=delete_one_owned_target case={} place_messages={:?} action_calls={:?} still_present={}",
        report.case, report.after, report.calls, report.still_present
    );
}

fn print_action_count() {
    let mut world = World::new();
    println!("### action-count");
    for app in world.actions.apps() {
        println!(
            "TASK3338_ACTIONS app={} delete_action_count={} actions={:?}",
            app,
            world.actions.action_count_for_app(app),
            world.actions.action_names_for_app(app)
        );
    }
    println!(
        "TASK3338_ACTIONS total_apps={} total_delete_actions={}",
        world.actions.apps().len(),
        world.actions.total_action_count()
    );

    let (second, _handle) = PlaceDeleteAction::new("discord", Vec::new());
    let name = second.action_name().to_owned();
    match world.actions.register(Box::new(second)) {
        Ok(()) => println!("TASK3338_ACTIONS second_action_for_discord=ACCEPTED name={name}"),
        Err(refusal) => println!(
            "TASK3338_ACTIONS second_action_for_discord=ERR code={} error=\"{}\"",
            refusal.code, refusal.message
        ),
    }
    println!(
        "TASK3338_ACTIONS after_duplicate app=discord delete_action_count={}",
        world.actions.action_count_for_app("discord")
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.split_first() {
        Some((flag, rest)) if flag == "--action-count" && rest.is_empty() => {
            print_action_count();
            std::process::exit(0);
        }
        Some((flag, rest)) if flag == "--case" && rest.len() == 1 => {
            let mut world = World::new();
            let Some(report) = run_case(&mut world, &rest[0]) else {
                eprintln!("{}", usage());
                std::process::exit(2);
            };
            print_case(&report);
            std::process::exit(i32::from(report.approval.is_none()));
        }
        _ => {
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
}
