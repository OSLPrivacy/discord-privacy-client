//! TASK 3338 check crate: the hub's own sources, compiled through `#[path]`.
//!
//! Nothing here re-implements the module under test. The three `#[path]`
//! declarations below name the exact files `apps/osl-hub/src/lib.rs` declares,
//! so a change to any of them changes what this crate builds:
//!
//! * `shared_delete_action.rs` — TASK 3338, the module under test;
//! * `privacy_scan.rs` — gate 3001's shared owner check, which it calls;
//! * `attachment_scan.rs` — the only module `privacy_scan` needs.
//!
//! What this crate adds is a *service fill-in*: [`PlaceDeleteAction`] is a real
//! per-app delete action that holds a place's messages and really removes from
//! them, so a refusal that never reaches the app is visible as an empty call
//! log on its [`PlaceHandle`].

#[path = "../../src/attachment_scan.rs"]
pub mod attachment_scan;
#[path = "../../src/privacy_scan.rs"]
pub mod privacy_scan;
#[path = "../../src/shared_delete_action.rs"]
pub mod shared_delete_action;

use std::cell::RefCell;
use std::rc::Rc;

use shared_delete_action::{AppDeleteAction, DeleteTarget};

#[derive(Default)]
struct PlaceState {
    messages: Vec<DeleteTarget>,
    calls: Vec<DeleteTarget>,
}

/// A reader onto a place the registry now owns the action for.
///
/// The registry takes the action by value, so this is how a check sees what the
/// app really did: which messages are still there, and which removals the
/// action was actually asked to perform.
#[derive(Clone)]
pub struct PlaceHandle {
    state: Rc<RefCell<PlaceState>>,
}

impl PlaceHandle {
    /// The locators still present in the place, in held order.
    pub fn remaining_locators(&self) -> Vec<String> {
        self.state
            .borrow()
            .messages
            .iter()
            .map(|message| message.locator.clone())
            .collect()
    }

    /// Every target the app's delete action was actually asked to remove.
    pub fn call_locators(&self) -> Vec<String> {
        self.state
            .borrow()
            .calls
            .iter()
            .map(|target| target.locator.clone())
            .collect()
    }

    pub fn holds(&self, target: &DeleteTarget) -> bool {
        self.state.borrow().messages.contains(target)
    }
}

/// One app's single delete action, over a place that really holds messages.
///
/// This is everything a service supplies: the removal, and nothing else. Every
/// rule about whether a removal may happen lives in `shared_delete_action`.
pub struct PlaceDeleteAction {
    app: String,
    action_name: String,
    state: Rc<RefCell<PlaceState>>,
}

impl PlaceDeleteAction {
    pub fn new(app: &str, messages: Vec<DeleteTarget>) -> (Self, PlaceHandle) {
        let state = Rc::new(RefCell::new(PlaceState {
            messages,
            calls: Vec::new(),
        }));
        let action = Self {
            app: app.to_owned(),
            action_name: format!("{app}.delete-own-message"),
            state: Rc::clone(&state),
        };
        (action, PlaceHandle { state })
    }
}

impl AppDeleteAction for PlaceDeleteAction {
    fn app(&self) -> &str {
        &self.app
    }

    fn action_name(&self) -> &str {
        &self.action_name
    }

    fn delete_message(&mut self, target: &DeleteTarget) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        state.calls.push(target.clone());
        let before = state.messages.len();
        state.messages.retain(|held| held != target);
        if state.messages.len() == before {
            return Err(format!("{} is not in this place", target.label()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The fixture world every case runs against
// ---------------------------------------------------------------------------

use shared_delete_action::{
    delete_one_owned_target, DeleteRequest, PerAppDeleteActions, ScrubMark, TimedDeleteProtection,
    TimedDeleteRecordInput,
};

/// The signed-in account, and the other person. They differ only in identity.
pub const SIGNED_IN_ACCOUNT: &str = "task-3338-signed-in-account";
pub const OTHER_ACCOUNT: &str = "task-3338-other-account";

pub const DISCORD_PLACE: &str = "dm:task-3338";
pub const WHATSAPP_PLACE: &str = "chat:task-3338";

/// The four discord messages the fixture place holds.
pub const MARKED_LOCATOR: &str = "task-3338-scrub-marked-mine";
pub const TIMER_LOCATOR: &str = "task-3338-timer-due-mine";
pub const PLAIN_LOCATOR: &str = "task-3338-unmarked-mine";
pub const THEIRS_LOCATOR: &str = "task-3338-marked-theirs";

/// Fixture clock, and the deadline the accepted timer promised.
pub const NOW_UNIX_SECS: i64 = 1_900_000_000;
pub const TIMER_SENT_AT: i64 = 1_899_000_000;
pub const TIMER_DELETE_AT: i64 = 1_899_900_000;

pub fn discord_target(locator: &str) -> DeleteTarget {
    DeleteTarget::new("discord", DISCORD_PLACE, locator)
}

pub fn whatsapp_target(locator: &str) -> DeleteTarget {
    DeleteTarget::new("whatsapp", WHATSAPP_PLACE, locator)
}

/// The one timed-delete record OSL already accepted, in TASK 3306's shape.
pub fn accepted_timed_delete_record() -> TimedDeleteRecordInput {
    TimedDeleteRecordInput {
        app: "discord".to_owned(),
        conversation: DISCORD_PLACE.to_owned(),
        locator: TIMER_LOCATOR.to_owned(),
        sent_at: TIMER_SENT_AT,
        delete_at: TIMER_DELETE_AT,
        protection: TimedDeleteProtection::Protected,
    }
}

/// The product's delete actions plus readers onto both places.
pub struct World {
    pub actions: PerAppDeleteActions,
    pub discord: PlaceHandle,
    pub whatsapp: PlaceHandle,
}

impl World {
    pub fn new() -> Self {
        let (discord_action, discord) = PlaceDeleteAction::new(
            "discord",
            vec![
                discord_target(MARKED_LOCATOR),
                discord_target(TIMER_LOCATOR),
                discord_target(PLAIN_LOCATOR),
                discord_target(THEIRS_LOCATOR),
            ],
        );
        let (whatsapp_action, whatsapp) =
            PlaceDeleteAction::new("whatsapp", vec![whatsapp_target(MARKED_LOCATOR)]);

        let mut actions = PerAppDeleteActions::new();
        actions
            .register(Box::new(discord_action))
            .expect("discord takes its one delete action");
        actions
            .register(Box::new(whatsapp_action))
            .expect("whatsapp takes its one delete action");

        Self {
            actions,
            discord,
            whatsapp,
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

/// What one case did, without any printing.
pub struct CaseReport {
    pub case: String,
    pub target: DeleteTarget,
    pub sender: String,
    pub mark: String,
    pub records: usize,
    pub approval: Option<String>,
    pub action_name: Option<String>,
    pub refusal_code: Option<String>,
    pub refusal_message: Option<String>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    pub calls: Vec<String>,
    pub still_present: bool,
}

/// Build the request one named case asks with. `None` for an unknown case.
pub fn case_request(case: &str) -> Option<(DeleteRequest, &'static str)> {
    let base = |target: DeleteTarget, sender: &str| DeleteRequest {
        target,
        signed_in_account_sender: Some(SIGNED_IN_ACCOUNT.to_owned()),
        message_sender: Some(sender.to_owned()),
        scrub_mark: None,
        timed_delete_records: Vec::new(),
        now_unix_secs: NOW_UNIX_SECS,
    };

    match case {
        // A person confirmed the Scrub review's mark on their own message.
        "scrub-marked-mine" => {
            let target = discord_target(MARKED_LOCATOR);
            let mut request = base(target.clone(), SIGNED_IN_ACCOUNT);
            request.scrub_mark = Some(ScrubMark::confirmed(target));
            Some((request, "confirmed"))
        }
        // No mark at all; only a timer OSL already accepted, now due.
        "timer-due-mine" => {
            let target = discord_target(TIMER_LOCATOR);
            let mut request = base(target, SIGNED_IN_ACCOUNT);
            request.timed_delete_records = vec![accepted_timed_delete_record()];
            Some((request, "none"))
        }
        // The account's own message, but nothing approves removing it: the
        // review's mark is missing and the accepted record names another
        // message.
        "unmarked-no-record-mine" => {
            let target = discord_target(PLAIN_LOCATOR);
            let mut request = base(target, SIGNED_IN_ACCOUNT);
            request.timed_delete_records = vec![accepted_timed_delete_record()];
            Some((request, "none"))
        }
        // Confirmed exactly as the first case, and due exactly as the second.
        // The only thing that differs is who sent it.
        "not-mine" => {
            let target = discord_target(THEIRS_LOCATOR);
            let mut request = base(target.clone(), OTHER_ACCOUNT);
            request.scrub_mark = Some(ScrubMark::confirmed(target.clone()));
            request.timed_delete_records = vec![TimedDeleteRecordInput {
                locator: THEIRS_LOCATOR.to_owned(),
                ..accepted_timed_delete_record()
            }];
            Some((request, "confirmed"))
        }
        _ => None,
    }
}

/// Run one named case against a fresh world.
pub fn run_case(world: &mut World, case: &str) -> Option<CaseReport> {
    let (request, mark) = case_request(case)?;
    let before = world.discord.remaining_locators();
    let result = delete_one_owned_target(&mut world.actions, &request);
    let after = world.discord.remaining_locators();

    let (approval, action_name, refusal_code, refusal_message) = match &result {
        Ok(outcome) => (
            Some(outcome.approval.as_str().to_owned()),
            Some(outcome.action_name.clone()),
            None,
            None,
        ),
        Err(refusal) => (
            None,
            None,
            Some(refusal.code.to_owned()),
            Some(refusal.message.clone()),
        ),
    };

    Some(CaseReport {
        case: case.to_owned(),
        still_present: world.discord.holds(&request.target),
        target: request.target,
        sender: request.message_sender.unwrap_or_default(),
        mark: mark.to_owned(),
        records: request.timed_delete_records.len(),
        approval,
        action_name,
        refusal_code,
        refusal_message,
        before,
        after,
        calls: world.discord.call_locators(),
    })
}

/// Every case name the command knows, in the order it runs them.
pub const CASES: [&str; 4] = [
    "scrub-marked-mine",
    "timer-due-mine",
    "unmarked-no-record-mine",
    "not-mine",
];
