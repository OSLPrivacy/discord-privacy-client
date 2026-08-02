//! R2: destructive lifecycle labels are a function of the observed facts, not
//! the order in which those facts reach a device.

#![cfg(feature = "core")]

use message_lifecycle::DestructReason;

fn display_reason(events: impl IntoIterator<Item = DestructReason>) -> &'static str {
    let reason = events
        .into_iter()
        .reduce(DestructReason::higher_precedence)
        .expect("a destroyed message has at least one destructive event");

    match reason {
        DestructReason::Burn => "Burned",
        DestructReason::ViewOnceConsumed => "Viewed once",
        DestructReason::Expired => "Expired",
        DestructReason::Evicted => "Removed from this device",
        DestructReason::UnknownDestructive => "Removed",
    }
}

#[test]
fn tf_76_opposite_arrival_orders_display_the_same_reason() {
    let first_device = display_reason([DestructReason::Expired, DestructReason::Burn]);
    let second_device = display_reason([DestructReason::Burn, DestructReason::Expired]);

    assert_eq!(first_device, "Burned");
    assert_eq!(second_device, "Burned");
    assert_eq!(first_device, second_device);
}
