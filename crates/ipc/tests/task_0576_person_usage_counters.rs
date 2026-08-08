use ipc::usage_counters::{PersonUsageCounters, UsageCounterStore};

const TEN_MB: u64 = 10 * 1024 * 1024;
const DAY: i64 = 20_000;

#[test]
fn task_0576_new_person_is_zero_and_a_ten_mb_file_changes_only_stored_bytes() {
    let dir = tempfile::tempdir().expect("temporary counter directory");
    let mut counters = UsageCounterStore::open(dir.path()).expect("open usage counters");

    let before = counters
        .read_at("task-0576-new-person", DAY * 86_400)
        .expect("read new person");
    assert_eq!(before, PersonUsageCounters::default());
    println!("TASK0576 new_person stored_bytes={} bytes_sent_today={} bytes_fetched_today={} messages_sent_today={}", before.stored_bytes, before.bytes_sent_today, before.bytes_fetched_today, before.messages_sent_today);

    counters
        .store_file("task-0576-new-person", "task-0576-ten-mb-file", TEN_MB)
        .expect("store ten MB file");
    drop(counters);
    let counters = UsageCounterStore::open(dir.path()).expect("reopen persisted usage counters");
    let after = counters
        .read_at("task-0576-new-person", DAY * 86_400)
        .expect("read stored file");
    assert_eq!(after.stored_bytes, TEN_MB);
    assert_eq!(after.bytes_sent_today, 0);
    assert_eq!(after.bytes_fetched_today, 0);
    assert_eq!(after.messages_sent_today, 0);
    println!("TASK0576 after_file stored_bytes={} bytes_sent_today={} bytes_fetched_today={} messages_sent_today={}", after.stored_bytes, after.bytes_sent_today, after.bytes_fetched_today, after.messages_sent_today);
}
