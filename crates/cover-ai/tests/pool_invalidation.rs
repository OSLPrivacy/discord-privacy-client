use cover_ai::pool::{CarrierPool, CoverContextVersion, PoolEntry};

#[test]
fn t13_tl5_carrier_from_old_cover_context_is_discarded_without_stalling_send() {
    let mut pool = CarrierPool::new();
    pool.push(PoolEntry::new(vec![1], "old cover".into(), CoverContextVersion([1; 32]), 1).unwrap()).unwrap();
    // A send receives `None` immediately and can use its word-bank floor.
    assert!(pool.take_ready(CoverContextVersion([2; 32]), 2, 60).is_none());
}
