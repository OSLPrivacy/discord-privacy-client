use std::cell::Cell;
use std::time::Duration;

use osl_privacy_hub::discord_receive_wakeup::{
    DiscordReceiveWakeupDecision, DiscordReceiveWakeupSubscription, DiscordWakeupNotice,
    DISCORD_RECEIVE_WAKEUP_BEAT,
};
use osl_privacy_hub::realtime_client::TICK_INTERVAL;

#[test]
fn task_3921_fake_wakeup_notice_runs_discord_receive_job_once() {
    assert_eq!(DISCORD_RECEIVE_WAKEUP_BEAT, Duration::from_secs(4));
    assert_eq!(DISCORD_RECEIVE_WAKEUP_BEAT, TICK_INTERVAL);

    let mut subscription = DiscordReceiveWakeupSubscription::new();
    let receive_jobs = Cell::new(0usize);

    let decision = subscription
        .notice_received(Duration::ZERO, DiscordWakeupNotice::Waiting, || {
            receive_jobs.set(receive_jobs.get() + 1);
            Ok::<(), ()>(())
        })
        .expect("fake wake-up notice is accepted");

    assert_eq!(decision, DiscordReceiveWakeupDecision::RanReceiveJob);
    assert_eq!(receive_jobs.get(), 1);
    println!("TASK3921_FAKE_WAKEUP_RECEIVE_JOBS={}", receive_jobs.get());
    println!(
        "TASK3921_WAKEUP_BEAT_SECONDS={}",
        DISCORD_RECEIVE_WAKEUP_BEAT.as_secs()
    );
}

#[test]
fn task_3921_ten_wakeup_notices_inside_one_second_run_at_most_once() {
    let mut subscription = DiscordReceiveWakeupSubscription::new();
    let receive_jobs = Cell::new(0usize);
    let mut ran = 0usize;
    let mut suppressed = 0usize;

    for notice in 0..10 {
        let decision = subscription
            .notice_received(
                Duration::from_millis(notice * 100),
                DiscordWakeupNotice::Waiting,
                || {
                    receive_jobs.set(receive_jobs.get() + 1);
                    Ok::<(), ()>(())
                },
            )
            .expect("fake wake-up notice is accepted");
        match decision {
            DiscordReceiveWakeupDecision::RanReceiveJob => ran += 1,
            DiscordReceiveWakeupDecision::SuppressedByBeat => suppressed += 1,
            DiscordReceiveWakeupDecision::IgnoredIdle => panic!("waiting notice cannot be idle"),
        }
    }

    let runs = receive_jobs.get();
    if runs > 1 {
        panic!(
            "TASK3921_RECEIVE_JOBS_ABOVE_LIMIT={} TASK3921_BURST_RECEIVE_JOBS={runs} TASK3921_LIMIT=1",
            runs - 1
        );
    }
    assert_eq!(runs, 1);
    assert_eq!(ran, 1);
    assert_eq!(suppressed, 9);
    println!("TASK3921_BURST_NOTICES=10");
    println!("TASK3921_BURST_WINDOW_MS=900");
    println!("TASK3921_BURST_RECEIVE_JOBS={runs}");
    println!("TASK3921_BURST_SUPPRESSED={suppressed}");
}

#[test]
fn task_3921_idle_notice_does_not_run_discord_receive_job() {
    let mut subscription = DiscordReceiveWakeupSubscription::new();
    let receive_jobs = Cell::new(0usize);

    let decision = subscription
        .notice_received(Duration::ZERO, DiscordWakeupNotice::Idle, || {
            receive_jobs.set(receive_jobs.get() + 1);
            Ok::<(), ()>(())
        })
        .expect("idle wake-up notice is accepted");

    assert_eq!(decision, DiscordReceiveWakeupDecision::IgnoredIdle);
    assert_eq!(receive_jobs.get(), 0);
}

#[test]
fn task_3921_subscription_source_has_zero_repeating_timers() {
    let source = include_str!("../src/discord_receive_wakeup.rs");
    let forbidden = [
        "std::thread::sleep",
        "thread::sleep",
        "tokio::time::interval",
        "setInterval",
        "setTimeout",
    ];
    let found: usize = forbidden
        .iter()
        .map(|pattern| source.matches(pattern).count())
        .sum();

    assert_eq!(found, 0);
    println!("TASK3921_REPEATING_TIMERS_FOUND={found}");
}
