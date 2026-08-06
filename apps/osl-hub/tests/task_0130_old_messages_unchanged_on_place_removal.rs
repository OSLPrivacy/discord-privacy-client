#![cfg(feature = "core")]

#[path = "sealed_relay_e2e.rs"]
mod sealed_relay_e2e;

#[test]
fn task_0130_old_messages_do_not_change_when_their_place_is_removed() {
    sealed_relay_e2e::task_0130_old_protected_message_record_survives_place_removal();
}
