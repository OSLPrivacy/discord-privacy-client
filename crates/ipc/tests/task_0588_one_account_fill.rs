use ipc::usage_counters::{UsageCeiling, UsageCeilings, UsageCounterStore};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Barrier,
    },
    thread,
};

const FIXTURE_ENV: &str = "OSL_TASK_0588_FIXTURE";
const DAY_ZERO: i64 = 20_588 * 86_400;
const DAY_SECONDS: i64 = 86_400;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FillFixture {
    account: String,
    stored_bytes_ceiling: u64,
    daily_upload_bytes_ceiling: u64,
    upload_bytes: u64,
    parallel_uploads: usize,
}

fn fixture_path() -> PathBuf {
    std::env::var_os(FIXTURE_ENV).map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/task_0588_paid_account.json"),
        PathBuf::from,
    )
}

fn load_fixture(path: &Path) -> Result<FillFixture, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read fixture {}: {error}", path.display()))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid fixture {}: {error}", path.display()))?;
    let missing = ["stored_bytes_ceiling", "daily_upload_bytes_ceiling"]
        .into_iter()
        .filter(|field| value.get(*field).and_then(Value::as_u64).is_none())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "fixture missing required ceilings: {}",
            missing.join(",")
        ));
    }
    serde_json::from_value(value)
        .map_err(|error| format!("invalid fixture {}: {error}", path.display()))
}

fn ceilings(fixture: &FillFixture) -> UsageCeilings {
    UsageCeilings {
        stored_bytes: fixture.stored_bytes_ceiling,
        bytes_sent_today: fixture.daily_upload_bytes_ceiling,
        bytes_fetched_today: u64::MAX,
        messages_sent_today: u64::MAX,
    }
}

#[test]
fn task_0588_one_paid_account_cannot_fill_the_service() {
    let selected_fixture = fixture_path();
    let fixture = load_fixture(&selected_fixture).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        fixture.stored_bytes_ceiling,
        UsageCeilings::PRO.stored_bytes,
        "fixture must exercise the stated Pro stored ceiling"
    );
    assert_eq!(
        fixture.daily_upload_bytes_ceiling,
        UsageCeilings::PRO.bytes_sent_today,
        "fixture must exercise the stated Pro daily upload ceiling"
    );
    assert_eq!(fixture.upload_bytes, 1024 * 1024 * 1024);
    assert!(
        fixture.parallel_uploads > 5,
        "fixture must race many uploads"
    );
    assert_eq!(
        load_fixture(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/task_0588_missing_ceilings.json")
                .as_path()
        )
        .unwrap_err(),
        "fixture missing required ceilings: stored_bytes_ceiling,daily_upload_bytes_ceiling"
    );

    let dir = tempfile::tempdir().expect("temporary durable account ledger");
    UsageCounterStore::open(dir.path()).expect("initialize usage store");
    let accepted_bytes = Arc::new(AtomicU64::new(0));
    let mut refusal_events = Vec::new();
    let mut restart_count = 0_u64;
    let mut max_observed_stored = 0_u64;
    let mut days_stopped_at_daily_ceiling = 0_u64;
    let days = fixture.stored_bytes_ceiling / fixture.daily_upload_bytes_ceiling;
    assert_eq!(
        days, 30,
        "the stated Pro fixture must take thirty daily waves"
    );

    for day_index in 0..days {
        let when = DAY_ZERO + i64::try_from(day_index).unwrap() * DAY_SECONDS;
        let barrier = Arc::new(Barrier::new(fixture.parallel_uploads));
        let mut workers = Vec::with_capacity(fixture.parallel_uploads);
        for upload_index in 0..fixture.parallel_uploads {
            let root = dir.path().to_owned();
            let barrier = Arc::clone(&barrier);
            let account = fixture.account.clone();
            let accepted_bytes = Arc::clone(&accepted_bytes);
            let file_id = format!("day-{day_index:02}-parallel-{upload_index:02}.1GiB");
            let file_bytes = fixture.upload_bytes;
            let active_ceilings = ceilings(&fixture);
            workers.push(thread::spawn(move || {
                let mut store = UsageCounterStore::open(&root).expect("open concurrent store");
                barrier.wait();
                store
                    .with_upload_admission(
                        &account,
                        &file_id,
                        file_bytes,
                        active_ceilings,
                        when,
                        || {
                            accepted_bytes.fetch_add(file_bytes, Ordering::SeqCst);
                            Ok::<_, &'static str>(())
                        },
                    )
                    .map_err(|error| (file_id, error.to_string()))
            }));
        }

        for worker in workers {
            match worker.join().expect("concurrent upload worker panicked") {
                Ok(()) => {}
                Err((file_id, refusal)) => {
                    assert!(
                        refusal.contains("bytes sent today ceiling")
                            || refusal.contains("stored bytes ceiling"),
                        "all refused uploads must name the ceiling: {refusal}"
                    );
                    refusal_events.push(format!("file={file_id} refusal={refusal}"));

                    // Model an unattended service restart after every refusal,
                    // and prove the durable totals remain within both ceilings.
                    let restarted = UsageCounterStore::open(dir.path())
                        .expect("restart durable usage store after refusal");
                    let persisted = restarted
                        .read_at(&fixture.account, when)
                        .expect("read persisted account totals after restart");
                    assert!(persisted.stored_bytes <= fixture.stored_bytes_ceiling);
                    assert!(persisted.bytes_sent_today <= fixture.daily_upload_bytes_ceiling);
                    max_observed_stored = max_observed_stored.max(persisted.stored_bytes);
                    restart_count += 1;
                }
            }
        }
        let daily_store =
            UsageCounterStore::open(dir.path()).expect("restart after concurrent daily wave");
        let daily_usage = daily_store
            .read_at(&fixture.account, when)
            .expect("read usage after concurrent daily wave");
        assert_eq!(
            daily_usage.bytes_sent_today, fixture.daily_upload_bytes_ceiling,
            "every wave must stop exactly at the stated daily ceiling"
        );
        max_observed_stored = max_observed_stored.max(daily_usage.stored_bytes);
        days_stopped_at_daily_ceiling += 1;
    }

    let final_when = DAY_ZERO + (i64::try_from(days).unwrap() - 1) * DAY_SECONDS;
    let final_store = UsageCounterStore::open(dir.path()).expect("final durable restart");
    let final_usage = final_store
        .read_at(&fixture.account, final_when)
        .expect("read final usage");
    assert_eq!(final_usage.stored_bytes, fixture.stored_bytes_ceiling);
    assert_eq!(
        final_usage.bytes_sent_today,
        fixture.daily_upload_bytes_ceiling
    );
    assert_eq!(
        accepted_bytes.load(Ordering::SeqCst),
        fixture.stored_bytes_ceiling
    );
    assert!(
        max_observed_stored <= fixture.stored_bytes_ceiling + fixture.upload_bytes,
        "stored total may never exceed the ceiling by more than one file"
    );

    let mut refusal_counts = BTreeMap::<String, usize>::new();
    for event in &refusal_events {
        let name = if event.contains("stored bytes ceiling") {
            UsageCeiling::StoredBytes.to_string()
        } else {
            UsageCeiling::BytesSentToday.to_string()
        };
        *refusal_counts.entry(name).or_default() += 1;
    }
    assert!(refusal_counts.contains_key("stored bytes ceiling"));
    assert!(refusal_counts.contains_key("bytes sent today ceiling"));
    assert_eq!(refusal_counts["bytes sent today ceiling"], 319);
    assert_eq!(refusal_counts["stored bytes ceiling"], 11);
    assert_eq!(restart_count as usize, refusal_events.len());
    assert_eq!(restart_count, 330);
    assert_eq!(days_stopped_at_daily_ceiling, 30);

    let report_path = dir.path().join("task-0588-saved-run.txt");
    let report = format!(
        "TASK0588 account={} fixture={} parallel_uploads={} attempts={} accepted_files={} refused={} file_bytes={} days_at_daily_ceiling={} stored_ceiling={} daily_ceiling={} final_stored={} final_daily={} max_observed_stored={} max_overage_bytes={} restarts={} refusal_counts={:?}\n{}\n",
        fixture.account,
        selected_fixture.display(),
        fixture.parallel_uploads,
        u64::try_from(fixture.parallel_uploads).unwrap() * days,
        fixture.stored_bytes_ceiling / fixture.upload_bytes,
        refusal_events.len(),
        fixture.upload_bytes,
        days_stopped_at_daily_ceiling,
        fixture.stored_bytes_ceiling,
        fixture.daily_upload_bytes_ceiling,
        final_usage.stored_bytes,
        final_usage.bytes_sent_today,
        max_observed_stored,
        max_observed_stored.saturating_sub(fixture.stored_bytes_ceiling),
        restart_count,
        refusal_counts,
        refusal_events.join("\n"),
    );
    fs::write(&report_path, &report).expect("save complete run report");
    let saved = fs::read_to_string(&report_path).expect("read saved run report");
    assert_eq!(saved, report);
    for refusal in refusal_counts.keys() {
        assert!(
            saved.contains(refusal),
            "saved run omitted refusal {refusal}"
        );
    }
    println!("{}", report.lines().next().unwrap());
    println!("TASK0588 saved_run_named_refusals={:?}", refusal_counts);
}
