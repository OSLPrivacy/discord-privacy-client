//! Runtime owner for the realtime wake-up pipe.
//!
//! The protocol state machine and WebSocket pipe remain in `realtime_client`
//! and `realtime_pipe`. This module owns only the account lifecycle boundary:
//! start one background pipe after unlock, report whether it connected, and
//! stop it when the account locks.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use serde::Serialize;

use crate::realtime_client::{RealtimeClient, FRAME_BYTES};
use crate::realtime_pipe::{
    drain_scheduled_fetches, open_realtime_connection, OsRealtimePipeEntropy, RealtimeEndpoint,
    RealtimePipeClock, RealtimePipeEntropy, RealtimePipeError, SleepingRealtimePipeClock,
};
use crate::realtime_resume::ReconnectSchedule;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeWakeupStatus {
    pub status: &'static str,
    pub generation: u64,
    pub starts: u64,
    pub stops: u64,
    pub opened_connections: usize,
    pub frames_sent: usize,
    pub authorized_fetches: usize,
    pub pretend_fetches: usize,
    pub endpoint: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Default)]
struct Inner {
    generation: u64,
    starts: u64,
    stops: u64,
    opened_connections: usize,
    frames_sent: usize,
    authorized_fetches: usize,
    pretend_fetches: usize,
    endpoint: Option<String>,
    last_error: Option<String>,
    connected: bool,
    running: Option<Arc<AtomicBool>>,
}

#[derive(Default)]
pub struct RealtimeWakeupRuntime {
    inner: Arc<Mutex<Inner>>,
}

impl RealtimeWakeupRuntime {
    pub fn connect_after_unlock(&self, endpoint: RealtimeEndpoint) {
        self.start_with_clock_and_entropy(
            endpoint,
            SleepingRealtimePipeClock::default(),
            OsRealtimePipeEntropy,
        );
    }

    pub fn stop_after_lock(&self) {
        let stop = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            inner.generation = inner.generation.wrapping_add(1);
            inner.stops = inner.stops.saturating_add(1);
            inner.connected = false;
            inner.running.take()
        };
        if let Some(stop) = stop {
            stop.store(true, Ordering::Release);
        }
    }

    pub fn status(&self) -> RealtimeWakeupStatus {
        let Ok(inner) = self.inner.lock() else {
            return RealtimeWakeupStatus {
                status: "stopped",
                generation: 0,
                starts: 0,
                stops: 0,
                opened_connections: 0,
                frames_sent: 0,
                authorized_fetches: 0,
                pretend_fetches: 0,
                endpoint: None,
                last_error: Some("wakeup state unavailable".to_owned()),
            };
        };
        RealtimeWakeupStatus {
            status: if inner.connected {
                "connected"
            } else {
                "stopped"
            },
            generation: inner.generation,
            starts: inner.starts,
            stops: inner.stops,
            opened_connections: inner.opened_connections,
            frames_sent: inner.frames_sent,
            authorized_fetches: inner.authorized_fetches,
            pretend_fetches: inner.pretend_fetches,
            endpoint: inner.endpoint.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    pub fn record_start_error(&self, error: impl Into<String>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.connected = false;
            inner.last_error = Some(error.into());
        }
    }

    pub fn start_with_clock_and_entropy<C, R>(
        &self,
        endpoint: RealtimeEndpoint,
        clock: C,
        entropy: R,
    ) where
        C: RealtimePipeClock + Send + 'static,
        R: RealtimePipeEntropy + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            if let Some(previous) = inner.running.take() {
                previous.store(true, Ordering::Release);
            }
            inner.generation = inner.generation.wrapping_add(1);
            inner.starts = inner.starts.saturating_add(1);
            inner.connected = false;
            inner.endpoint = Some(endpoint.as_url());
            inner.last_error = None;
            inner.running = Some(Arc::clone(&stop));
        }

        let inner = Arc::clone(&self.inner);
        let _ = std::thread::Builder::new()
            .name("osl-realtime-wakeup".to_owned())
            .spawn(move || run_loop(endpoint, inner, stop, clock, entropy));
    }
}

fn run_loop<C, R>(
    endpoint: RealtimeEndpoint,
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicBool>,
    mut clock: C,
    mut entropy: R,
) where
    C: RealtimePipeClock,
    R: RealtimePipeEntropy,
{
    let mut client = RealtimeClient::new(Duration::ZERO);
    let mut reconnect = ReconnectSchedule::new();

    while !stop.load(Ordering::Acquire) {
        let mut socket = match open_realtime_connection(&endpoint) {
            Ok(socket) => {
                if let Ok(mut inner) = inner.lock() {
                    inner.connected = true;
                    inner.opened_connections = inner.opened_connections.saturating_add(1);
                    inner.last_error = None;
                }
                socket
            }
            Err(error) if reconnectable(&error) => {
                record_error(&inner, &error);
                wait_reconnect(&mut reconnect, &mut clock, &mut entropy);
                continue;
            }
            Err(error) => {
                record_error(&inner, &error);
                break;
            }
        };

        loop {
            if stop.load(Ordering::Acquire) {
                mark_stopped(&inner);
                return;
            }

            let tick = client.next_outbound_tick();
            clock.wait_until(tick.scheduled_at);
            if let Err(error) = socket.send_text(&tick.frame) {
                if reconnectable(&error) {
                    record_error(&inner, &error);
                    mark_stopped(&inner);
                    wait_reconnect(&mut reconnect, &mut clock, &mut entropy);
                    break;
                }
                record_error(&inner, &error);
                mark_stopped(&inner);
                return;
            }
            if let Ok(mut inner) = inner.lock() {
                inner.frames_sent = inner.frames_sent.saturating_add(1);
            }

            match socket.read_text() {
                Ok(reply) if reply.len() == FRAME_BYTES => {
                    if let Err(error) = client.receive_frame(&reply) {
                        record_error(
                            &inner,
                            &RealtimePipeError::Protocol(format!(
                                "wakeup frame rejected: {error:?}"
                            )),
                        );
                        continue;
                    }
                    reconnect.connected();
                    let drained = drain_scheduled_fetches(
                        &mut client,
                        |_| Ok::<_, ()>(()),
                        |_| Ok::<_, ()>(()),
                    )
                    .expect("count-only wakeup drain cannot fail");
                    if let Ok(mut inner) = inner.lock() {
                        inner.authorized_fetches = inner
                            .authorized_fetches
                            .saturating_add(drained.authorized_fetches);
                        inner.pretend_fetches = inner
                            .pretend_fetches
                            .saturating_add(drained.pretend_fetches);
                    }
                }
                Ok(reply) => {
                    record_error(
                        &inner,
                        &RealtimePipeError::Protocol(format!(
                            "expected {FRAME_BYTES}-byte wakeup frame, got {}",
                            reply.len()
                        )),
                    );
                }
                Err(error) if reconnectable(&error) => {
                    record_error(&inner, &error);
                    mark_stopped(&inner);
                    wait_reconnect(&mut reconnect, &mut clock, &mut entropy);
                    break;
                }
                Err(error) => {
                    record_error(&inner, &error);
                    mark_stopped(&inner);
                    return;
                }
            }
        }
    }

    mark_stopped(&inner);
}

fn reconnectable(error: &RealtimePipeError) -> bool {
    matches!(
        error,
        RealtimePipeError::Io(_) | RealtimePipeError::UnexpectedClose
    )
}

fn wait_reconnect<C, R>(reconnect: &mut ReconnectSchedule, clock: &mut C, entropy: &mut R)
where
    C: RealtimePipeClock,
    R: RealtimePipeEntropy,
{
    clock.wait_for_reconnect(reconnect.next_delay(entropy.next_u64()));
}

fn mark_stopped(inner: &Arc<Mutex<Inner>>) {
    if let Ok(mut inner) = inner.lock() {
        inner.connected = false;
    }
}

fn record_error(inner: &Arc<Mutex<Inner>>, error: &RealtimePipeError) {
    if let Ok(mut inner) = inner.lock() {
        inner.connected = false;
        inner.last_error = Some(format!("{error:?}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_3920_runtime_reports_stopped_without_start_and_stops_on_lock() {
        let runtime = RealtimeWakeupRuntime::default();
        assert_eq!(runtime.status().status, "stopped");

        runtime.stop_after_lock();
        let status = runtime.status();
        assert_eq!(status.status, "stopped");
        assert_eq!(status.stops, 1);
        assert_eq!(status.pretend_fetches, 0);
    }

    #[test]
    fn task_3920_endpoint_status_uses_the_realtime_route() {
        let endpoint = RealtimeEndpoint::parse("ws://127.0.0.1:9/v1/realtime").unwrap();
        assert_eq!(endpoint.as_url(), "ws://127.0.0.1:9/v1/realtime");
    }
}
