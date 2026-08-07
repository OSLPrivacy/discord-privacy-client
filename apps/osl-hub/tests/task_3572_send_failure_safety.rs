#![cfg(feature = "core")]

#[path = "sealed_relay_e2e.rs"]
mod sealed_relay_e2e;

#[test]
fn task_3572_every_send_failure_is_safe_and_retries_once() {
    sealed_relay_e2e::task_3572_every_send_failure_is_safe_and_retries_once();
}
