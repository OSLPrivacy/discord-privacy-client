use cover_ai::pool::{CarrierPool, CoverContextVersion, PoolEntry};

fn entry(context: u8, age: u64) -> PoolEntry {
    PoolEntry::new(
        vec![7; 32],
        "ordinary cover text".into(),
        CoverContextVersion([context; 32]),
        age,
    )
    .unwrap()
}

#[test]
fn t13_tl3_send_path_is_a_pop_not_generation() {
    let mut pool = CarrierPool::new();
    pool.push(entry(1, 10)).unwrap();
    let entry = pool
        .take_ready(CoverContextVersion([1; 32]), 11, 30)
        .expect("ready carrier");
    assert_eq!(entry.into_capability(), vec![7; 32]);
    assert!(pool.is_empty(), "pre-drawn capabilities are single-use");
}

#[test]
fn t13_tl5_stale_context_age_and_burned_scope_are_never_served() {
    let mut pool = CarrierPool::new();
    pool.push(entry(1, 10)).unwrap();
    assert!(pool
        .take_ready(CoverContextVersion([2; 32]), 11, 30)
        .is_none());
    pool.push(entry(1, 10)).unwrap();
    assert!(pool
        .take_ready(CoverContextVersion([1; 32]), 200, 30)
        .is_none());
    pool.push(entry(1, 200)).unwrap();
    pool.invalidate_for_scope_burn();
    assert!(pool.is_empty());
}
