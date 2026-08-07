use std::time::Duration;

/// The Discord receive wake-up beat is the realtime protocol beat: four seconds.
pub const DISCORD_RECEIVE_WAKEUP_BEAT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscordWakeupNotice {
    Idle,
    Waiting,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscordReceiveWakeupDecision {
    IgnoredIdle,
    RanReceiveJob,
    SuppressedByBeat,
}

#[derive(Debug, Default)]
pub struct DiscordReceiveWakeupSubscription {
    last_job_started_at: Option<Duration>,
}

impl DiscordReceiveWakeupSubscription {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notice_received<R, E>(
        &mut self,
        noticed_at: Duration,
        notice: DiscordWakeupNotice,
        run_receive_job: R,
    ) -> Result<DiscordReceiveWakeupDecision, E>
    where
        R: FnOnce() -> Result<(), E>,
    {
        if notice != DiscordWakeupNotice::Waiting {
            return Ok(DiscordReceiveWakeupDecision::IgnoredIdle);
        }

        if self
            .last_job_started_at
            .is_some_and(|last| noticed_at.saturating_sub(last) < DISCORD_RECEIVE_WAKEUP_BEAT)
        {
            return Ok(DiscordReceiveWakeupDecision::SuppressedByBeat);
        }

        self.last_job_started_at = Some(noticed_at);
        run_receive_job()?;
        Ok(DiscordReceiveWakeupDecision::RanReceiveJob)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn fake_waiting_notice_runs_receive_job_once() {
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
        assert_eq!(DISCORD_RECEIVE_WAKEUP_BEAT, Duration::from_secs(4));
        println!("TASK3921_FAKE_WAKEUP_RECEIVE_JOBS={}", receive_jobs.get());
        println!(
            "TASK3921_WAKEUP_BEAT_SECONDS={}",
            DISCORD_RECEIVE_WAKEUP_BEAT.as_secs()
        );
    }

    #[test]
    fn ten_waiting_notices_inside_one_second_run_at_most_once() {
        let mut subscription = DiscordReceiveWakeupSubscription::new();
        let receive_jobs = Cell::new(0usize);
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
            if decision == DiscordReceiveWakeupDecision::SuppressedByBeat {
                suppressed += 1;
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
        assert_eq!(suppressed, 9);
        println!("TASK3921_BURST_NOTICES=10");
        println!("TASK3921_BURST_WINDOW_MS=900");
        println!("TASK3921_BURST_RECEIVE_JOBS={runs}");
        println!("TASK3921_BURST_SUPPRESSED={suppressed}");
    }
}
