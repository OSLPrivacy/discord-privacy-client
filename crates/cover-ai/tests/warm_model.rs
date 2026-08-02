use std::time::{Duration, Instant};
use cover_ai::warm_model::{Resident, ResidentModel, WarmModel};

#[derive(Debug)] struct Model(u64);
impl ResidentModel for Model { fn working_set_bytes(&self) -> u64 { self.0 } }

#[test]
fn t13_tl2_loads_once_and_never_cold_loads_a_generation() {
    let now = Instant::now();
    let mut slot = WarmModel::new(100, Duration::from_secs(60));
    assert!(matches!(slot.for_generation(now), Resident::Unavailable));
    assert!(slot.begin_load());
    assert!(matches!(slot.for_generation(now), Resident::Loading));
    assert!(!slot.begin_load(), "a second send cannot start a second weight load");
    slot.finish_load(Model(100), now).unwrap();
    assert!(matches!(slot.for_generation(now), Resident::Ready(_)));
}

#[test]
fn t13_tl2_releases_idle_weights_and_enforces_working_set_ceiling() {
    let now = Instant::now();
    let mut slot = WarmModel::new(100, Duration::from_secs(5));
    slot.begin_load();
    assert!(slot.finish_load(Model(101), now).is_err());
    slot.begin_load(); slot.finish_load(Model(100), now).unwrap();
    assert!(slot.release_if_idle(now + Duration::from_secs(5)));
    assert!(!slot.is_resident());
}
