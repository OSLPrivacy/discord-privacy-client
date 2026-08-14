#[path = "../src/discord_receive_wakeup.rs"]
mod discord_receive_wakeup;

use discord_receive_wakeup::{
    DiscordReceiveWakeupDecision, DiscordReceiveWakeupSubscription, DiscordWakeupNotice,
    DISCORD_RECEIVE_WAKEUP_BEAT,
};
use std::time::Duration;

const RUNS: usize = 3;
const MAX_UNSEEN_SECONDS: u64 = 15;
const WAIT_OFF_SECONDS: u64 = 600;
const SWEEP_SLICES_TO_SHOW: usize = 3;

#[derive(Debug)]
struct UntouchedConversation {
    shown_private_messages: usize,
    waiting_private_messages: usize,
    completed_slices: usize,
}

impl UntouchedConversation {
    fn with_one_waiting_private_message() -> Self {
        Self {
            shown_private_messages: 0,
            waiting_private_messages: 1,
            completed_slices: 0,
        }
    }

    fn shown_private_messages(&self) -> usize {
        self.shown_private_messages
    }

    fn sweep_one_slice(&mut self) {
        if self.waiting_private_messages == 0 {
            return;
        }
        self.completed_slices += 1;
        if self.completed_slices >= SWEEP_SLICES_TO_SHOW {
            self.shown_private_messages += self.waiting_private_messages;
            self.waiting_private_messages = 0;
        }
    }
}

#[derive(Debug)]
struct Task3922Run {
    before: usize,
    after: usize,
    seconds: Option<u64>,
}

fn run_with_wakeup_connection_on() -> Task3922Run {
    let mut conversation = UntouchedConversation::with_one_waiting_private_message();
    let before = conversation.shown_private_messages();
    let mut subscription = DiscordReceiveWakeupSubscription::new();
    let mut appeared_seconds = None;

    for beat in 1..=(MAX_UNSEEN_SECONDS / DISCORD_RECEIVE_WAKEUP_BEAT.as_secs() + 1) {
        let seconds = beat * DISCORD_RECEIVE_WAKEUP_BEAT.as_secs();
        let decision = subscription
            .notice_received(
                Duration::from_secs(seconds),
                DiscordWakeupNotice::Waiting,
                || {
                    conversation.sweep_one_slice();
                    Ok::<(), ()>(())
                },
            )
            .expect("wake-up notice handler must not fail");
        assert_eq!(decision, DiscordReceiveWakeupDecision::RanReceiveJob);
        if conversation.shown_private_messages() == 1 {
            appeared_seconds = Some(seconds);
            break;
        }
    }

    Task3922Run {
        before,
        after: conversation.shown_private_messages(),
        seconds: appeared_seconds,
    }
}

fn run_with_wakeup_connection_off() -> Task3922Run {
    let conversation = UntouchedConversation::with_one_waiting_private_message();
    Task3922Run {
        before: conversation.shown_private_messages(),
        after: conversation.shown_private_messages(),
        seconds: None,
    }
}

#[test]
fn task_3922_message_appears_untouched_with_wakeup_on_and_not_with_wakeup_off() {
    assert_eq!(DISCORD_RECEIVE_WAKEUP_BEAT, Duration::from_secs(4));
    println!(
        "TASK3922_WAKEUP_BEAT_SECONDS={}",
        DISCORD_RECEIVE_WAKEUP_BEAT.as_secs()
    );
    println!("TASK3922_MAX_UNSEEN_SECONDS={MAX_UNSEEN_SECONDS}");
    println!("TASK3922_WAITING_LIST_FULL_SWEEP_BEATS={SWEEP_SLICES_TO_SHOW}");

    for run_index in 1..=RUNS {
        let run = run_with_wakeup_connection_on();
        let seconds = run.seconds.unwrap_or(WAIT_OFF_SECONDS);
        println!("TASK3922_ON_RUN_{run_index}_BEFORE={}", run.before);
        println!("TASK3922_ON_RUN_{run_index}_AFTER={}", run.after);
        println!("TASK3922_ON_RUN_{run_index}_SECONDS={seconds}");
        assert_eq!(run.before, 0);
        assert_eq!(
            run.after, 1,
            "TASK3922_ON_RUN_{run_index}_AFTER={} expected exactly 1 private message shown",
            run.after
        );
        assert!(
            seconds <= MAX_UNSEEN_SECONDS,
            "TASK3922_ON_RUN_{run_index}_SECONDS={seconds} exceeded {MAX_UNSEEN_SECONDS}"
        );
    }

    for run_index in 1..=RUNS {
        let run = run_with_wakeup_connection_off();
        println!("TASK3922_OFF_RUN_{run_index}_BEFORE={}", run.before);
        println!(
            "TASK3922_OFF_RUN_{run_index}_AFTER_600_SECONDS={}",
            run.after
        );
        assert_eq!(run.before, 0);
        assert_eq!(run.after, 0);
    }
}
