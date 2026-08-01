//! T16-T23: Free carrier placement may differ only in timing.

#[path = "../src/carrier_placement.rs"]
mod carrier_placement;

use carrier_placement::{CarrierPlacement, CarrierPlacementTiming};

#[test]
fn atomic_and_compatibility_place_byte_identical_carriers() {
    // Covers Unicode, multi-byte punctuation, and a soft line break: byte equality
    // is stricter than visual or character equality and catches truncation.
    let carrier = "coffee\nnaïve — 東京 🛡️";
    let atomic = CarrierPlacement::new(carrier, CarrierPlacementTiming::Atomic);
    let compatibility = CarrierPlacement::new(carrier, CarrierPlacementTiming::Compatibility);

    assert_eq!(
        atomic.payload().as_bytes(),
        compatibility.payload().as_bytes()
    );
    assert_ne!(atomic.timing(), compatibility.timing());
}
