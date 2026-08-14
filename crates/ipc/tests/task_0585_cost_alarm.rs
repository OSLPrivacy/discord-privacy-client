use ipc::{
    cost_alarm::{check_cost_alarm, CostWarning, MoneyCeiling},
    usage_counters::UsageCounterStore,
};

const DAY: i64 = 20_585;
const WHEN: i64 = DAY * 86_400;
// One micro-currency unit per byte makes the fixture percentages transparent;
// production supplies its real 0575 ceiling and rates through MoneyCeiling.
const CEILING: MoneyCeiling = MoneyCeiling {
    ceiling_micros: 100,
    stored_byte_micros: 1,
    transferred_byte_micros: 1,
};

fn fixture(stored: u64, sent: u64, fetched: u64) -> (tempfile::TempDir, UsageCounterStore) {
    let dir = tempfile::tempdir().expect("temporary counter directory");
    let path = dir.path().to_path_buf();
    let mut store = UsageCounterStore::open(&path).expect("open usage counters");
    if stored != 0 {
        store
            .store_file("task-0585-alice", "task-0585-file", stored)
            .expect("store fixture bytes");
    }
    store
        .record_bytes_sent("task-0585-bob", sent, WHEN)
        .expect("record fixture sent bytes");
    store
        .record_bytes_fetched("task-0585-carol", fetched, WHEN)
        .expect("record fixture fetched bytes");
    (dir, store)
}

#[test]
fn task_0585_whole_service_cost_alarm_names_the_half_and_three_quarter_warnings() {
    let (_dir_49, at_49) = fixture(20, 19, 10);
    let report_49 = check_cost_alarm(&at_49, CEILING, WHEN).expect("49 percent report");
    assert_eq!(report_49.projected_cost_micros, 49);
    assert_eq!(report_49.warning, None);
    println!("TASK0585 percent=49 warning=none");

    let (_dir_51, at_51) = fixture(20, 20, 11);
    let report_51 = check_cost_alarm(&at_51, CEILING, WHEN).expect("51 percent report");
    assert_eq!(report_51.projected_cost_micros, 51);
    assert_eq!(report_51.warning, Some(CostWarning::HalfCostCeiling));
    println!(
        "TASK0585 percent=51 warning_name={}",
        report_51.warning.expect("half warning").name()
    );

    let (_dir_76, at_76) = fixture(20, 30, 26);
    let report_76 = check_cost_alarm(&at_76, CEILING, WHEN).expect("76 percent report");
    assert_eq!(report_76.projected_cost_micros, 76);
    assert_eq!(
        report_76.warning,
        Some(CostWarning::ThreeQuarterCostCeiling)
    );
    println!(
        "TASK0585 percent=76 warning_name={}",
        report_76.warning.expect("three-quarter warning").name()
    );
}
