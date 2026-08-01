//! Keeps the in-memory entitlement synchronized with the keyserver.
//!
//! Startup classification is deliberately cache-only so the first render does
//! not wait for I/O. This scheduler performs the network refresh immediately
//! afterwards and every six hours while the shipping app remains open.

use crate::core_bridge::HubCoreState;
use std::{future::Future, time::Duration};
use tauri::Manager;

const REFRESH_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Start the entitlement refresh task for the lifetime of this app process.
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(run_refresh_schedule(REFRESH_EVERY, move || {
        let app = app.clone();
        async move {
            let _ = tauri::async_runtime::spawn_blocking(move || {
                let core = app.state::<HubCoreState>();
                // license.json is device-level, matching the cache-only
                // `launch_classify` read performed during core bootstrap.
                if let Ok(base_dir) = keystore::osl_base_dir() {
                    let _ = ipc::license_lifecycle::refresh_license_state(&core.osl, &base_dir);
                }
            })
            .await;
        }
    }));
}

async fn run_refresh_schedule<Refresh, RefreshFuture>(period: Duration, mut refresh: Refresh)
where
    Refresh: FnMut() -> RefreshFuture,
    RefreshFuture: Future<Output = ()>,
{
    // Refresh now rather than making a paid user wait until the next cadence.
    refresh().await;

    // Tokio intervals tick immediately. Consume that first tick because the
    // immediate refresh above is the first run for this process.
    let mut interval = tokio::time::interval(period);
    interval.tick().await;
    loop {
        interval.tick().await;
        refresh().await;
    }
}

#[cfg(test)]
mod tests {
    use super::run_refresh_schedule;
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn refreshes_immediately_then_on_each_interval() {
        let (sent, received) = mpsc::channel();
        let task = tauri::async_runtime::spawn(run_refresh_schedule(
            Duration::from_millis(5),
            move || {
                let sent = sent.clone();
                async move {
                    sent.send(()).expect("test receiver remains open");
                }
            },
        ));

        received
            .recv_timeout(Duration::from_millis(50))
            .expect("the scheduler must refresh immediately");
        received
            .recv_timeout(Duration::from_millis(100))
            .expect("the scheduler must refresh again after its interval");
        task.abort();
    }
}
