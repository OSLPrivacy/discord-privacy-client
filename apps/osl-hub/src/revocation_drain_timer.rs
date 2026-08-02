//! Schedules delivery attempts for the durable bilateral-burn outbox.
//!
//! Unlike an inbox poll, this scheduler has no active-conversation dependency:
//! a burn issued in an idle conversation is retried while the Hub is open.

use crate::broker;
use crate::core_bridge::HubCoreState;
use crate::security::HubSecurityState;
use std::{future::Future, time::Duration};
use tauri::Manager;

const DRAIN_EVERY: Duration = Duration::from_secs(5);

/// Start the bounded revocation outbox drain for this app process.
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(run_drain_schedule(DRAIN_EVERY, move || {
        let app = app.clone();
        async move {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                let core = app.state::<HubCoreState>();
                let security = app.state::<HubSecurityState>();
                let _ = broker::drain_due_revocations(
                    &core,
                    &security,
                    ipc::main_password::now_unix_secs_pub(),
                );
            })
            .await;
        }
    }));
}

async fn run_drain_schedule<Drain, DrainFuture>(period: Duration, mut drain: Drain)
where
    Drain: FnMut() -> DrainFuture,
    DrainFuture: Future<Output = ()>,
{
    // Attempt immediately after launch, then keep retrying even if no message
    // poll or UI interaction occurs.
    drain().await;
    let mut interval = tokio::time::interval(period);
    interval.tick().await;
    loop {
        interval.tick().await;
        drain().await;
    }
}

#[cfg(test)]
mod tests {
    use super::run_drain_schedule;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn drains_immediately_and_when_the_conversation_stays_idle() {
        let (sent, received) = mpsc::channel();
        let task =
            tauri::async_runtime::spawn(run_drain_schedule(Duration::from_millis(5), move || {
                let sent = sent.clone();
                async move {
                    sent.send(()).expect("test receiver remains open");
                }
            }));

        received
            .recv_timeout(Duration::from_millis(50))
            .expect("the outbox drain must run without a message poll");
        received
            .recv_timeout(Duration::from_millis(100))
            .expect("the outbox drain must run again while the conversation is idle");
        task.abort();
    }
}
