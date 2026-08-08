//! Shared preference gate for actions that cannot be undone.
//!
//! The destructive operation is supplied as a closure so the gate runs before
//! the operation obtains a store, service, or reset lock.  A caller receives a
//! typed `needs-confirming` answer instead of an error when the saved choice is
//! on and the trusted UI has not confirmed the exact attempt.

use serde::{Deserialize, Serialize};

use crate::app_preferences::AskBeforeIrreversibleActionsChoice;
use crate::AppState;

pub const NEEDS_CONFIRMING_ANSWER: &str = "needs-confirming";
pub const COMPLETED_ANSWER: &str = "completed";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "kebab-case")]
pub enum IrreversibleActionAnswer<T> {
    NeedsConfirming,
    Completed { result: T },
}

impl<T> IrreversibleActionAnswer<T> {
    pub const fn answer(&self) -> &'static str {
        match self {
            Self::NeedsConfirming => NEEDS_CONFIRMING_ANSWER,
            Self::Completed { .. } => COMPLETED_ANSWER,
        }
    }

    pub const fn result(&self) -> Option<&T> {
        match self {
            Self::NeedsConfirming => None,
            Self::Completed { result } => Some(result),
        }
    }
}

/// Run `operation` unless the saved choice requires an as-yet-missing
/// confirmation.  With the choice off, `confirmed` is deliberately ignored.
pub fn run_irreversible_action<T, E>(
    state: &AppState,
    confirmed: bool,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<IrreversibleActionAnswer<T>, E> {
    let choice = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .ask_before_irreversible_actions;
    if choice == AskBeforeIrreversibleActionsChoice::On && !confirmed {
        return Ok(IrreversibleActionAnswer::NeedsConfirming);
    }
    operation().map(|result| IrreversibleActionAnswer::Completed { result })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_3155_all_three_actions_refuse_until_confirmed_and_off_runs_immediately() {
        let state = AppState::new();
        for action in ["chat-burn", "scrub-delete", "full-settings-reset"] {
            state
                .app_preferences
                .lock()
                .unwrap()
                .ask_before_irreversible_actions = AskBeforeIrreversibleActionsChoice::On;
            let mut executions = 0;

            let refused = run_irreversible_action(&state, false, || {
                executions += 1;
                Ok::<_, String>(action)
            })
            .unwrap();
            assert_eq!(refused.answer(), NEEDS_CONFIRMING_ANSWER);
            assert_eq!(
                serde_json::to_value(&refused).unwrap()["answer"],
                NEEDS_CONFIRMING_ANSWER
            );
            assert_eq!(executions, 0);

            let confirmed = run_irreversible_action(&state, true, || {
                executions += 1;
                Ok::<_, String>(action)
            })
            .unwrap();
            assert_eq!(confirmed.answer(), COMPLETED_ANSWER);
            assert_eq!(executions, 1);

            state
                .app_preferences
                .lock()
                .unwrap()
                .ask_before_irreversible_actions = AskBeforeIrreversibleActionsChoice::Off;
            let off = run_irreversible_action(&state, false, || {
                executions += 1;
                Ok::<_, String>(action)
            })
            .unwrap();
            assert_eq!(off.answer(), COMPLETED_ANSWER);
            assert_eq!(executions, 2);

            println!(
                "TASK3155 action={action} on_unconfirmed={} executions_after_refusal=0 on_confirmed={} off_unconfirmed={} total_executions={executions}",
                refused.answer(),
                confirmed.answer(),
                off.answer(),
            );
        }

        let commands = include_str!("commands.rs");
        assert!(commands.contains("cmd_osl_chat_burn_sender_message_records_choice_confirmed"));
        assert!(commands.contains("run_irreversible_action(state, confirmed"));
        let hub = include_str!("../../../apps/osl-hub/src/irreversible_actions.rs");
        for connected in [
            "delete_scrub_marked_messages",
            "delete_scrub_marked_mail_message",
            "pub fn reset_every_setting",
        ] {
            assert!(
                hub.contains(connected),
                "missing ask-step adapter {connected}"
            );
        }
        assert_eq!(hub.matches("run_irreversible_action").count(), 4);
    }
}
