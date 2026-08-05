#![cfg(feature = "core")]

//! D-223's gate. Its own binary, like every other sealed-relay case, because
//! the fixture drives process-global account-directory state.

#[path = "sealed_relay_e2e.rs"]
mod sealed_relay_e2e;

#[test]
fn osl_chat_queues_a_relay_notice_the_key_server_never_accepted() {
    sealed_relay_e2e::osl_chat_queues_a_relay_notice_the_key_server_never_accepted();
}
