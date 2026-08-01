#![cfg(feature = "core")]

#[path = "sealed_relay_e2e.rs"]
mod sealed_relay_e2e;

#[test]
fn osl_chat_message_survives_a_lost_wrapped_key_response() {
    sealed_relay_e2e::osl_chat_message_survives_a_lost_wrapped_key_response();
}
