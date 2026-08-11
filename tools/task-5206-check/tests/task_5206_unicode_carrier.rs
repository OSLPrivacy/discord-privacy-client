#[path = "../../../apps/osl-hub/src/carrier_placement.rs"]
mod carrier_placement;

use carrier_placement::{CarrierPlacement, CarrierPlacementTiming};

#[test]
fn task_5206_real_unicode_carrier_path_stays_byte_exact() {
    let carrier = "naïve — 東京 🛡️\nمرحبا";
    let atomic = CarrierPlacement::new(carrier, CarrierPlacementTiming::Atomic);
    let compatibility = CarrierPlacement::new(carrier, CarrierPlacementTiming::Compatibility);

    assert_eq!(atomic.payload(), carrier);
    assert_eq!(compatibility.payload(), carrier);
    assert_eq!(
        atomic.payload().as_bytes(),
        compatibility.payload().as_bytes()
    );
    assert!(carrier.chars().any(|character| !character.is_ascii()));

    println!(
        "TASK5206_UNICODE real_carrier_module=apps/osl-hub/src/carrier_placement.rs unicode_codepoints={} utf8_bytes={} atomic_exact=1 compatibility_exact=1",
        carrier.chars().count(),
        carrier.len(),
    );
}
