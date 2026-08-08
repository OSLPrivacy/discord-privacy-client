use ipc::usage_counters::{UploadStartError, UsageCeiling, UsageCeilings, UsageCounterStore};
use std::cell::Cell;

const DAY: i64 = 20_001;
const NOW: i64 = DAY * 86_400;
const STORED_CEILING: u64 = 1_024;

fn test_ceilings() -> UsageCeilings {
    UsageCeilings {
        stored_bytes: STORED_CEILING,
        bytes_sent_today: STORED_CEILING * 4,
        bytes_fetched_today: STORED_CEILING * 4,
        messages_sent_today: 10,
    }
}

#[test]
fn task_0577_checks_before_accepting_any_upload_bytes() {
    let dir = tempfile::tempdir().expect("temporary counter directory");
    let mut counters = UsageCounterStore::open(dir.path()).expect("open usage counters");

    let under_person = "task-0577-one-byte-under";
    counters
        .store_file(under_person, "existing", STORED_CEILING - 1)
        .expect("seed one byte under stored ceiling");
    let accepted_bytes = Cell::new(0_u64);
    let receipt = counters
        .with_upload_admission(under_person, "last-byte", 1, test_ceilings(), NOW, || {
            accepted_bytes.set(accepted_bytes.get() + 1);
            Ok::<_, &'static str>("accepted")
        })
        .expect("one final byte is accepted");
    let accepted_stored = counters
        .read_at(under_person, NOW)
        .expect("read accepted person's stored total")
        .stored_bytes;
    assert_eq!(receipt, "accepted");
    assert_eq!(accepted_bytes.get(), 1);
    assert_eq!(accepted_stored, STORED_CEILING);
    println!(
        "TASK0577 accepted person={under_person} starting_stored={} upload_bytes={} accepted_bytes={} ending_stored={accepted_stored}",
        STORED_CEILING - 1,
        1,
        accepted_bytes.get(),
    );

    let refused_person = "task-0577-at-ceiling";
    counters
        .store_file(refused_person, "existing", STORED_CEILING)
        .expect("seed exact stored ceiling");
    let refused_stored_before = counters
        .read_at(refused_person, NOW)
        .expect("read refused person before")
        .stored_bytes;
    let refused_bytes = Cell::new(0_u64);
    let error = counters
        .with_upload_admission(
            refused_person,
            "must-not-start",
            1,
            test_ceilings(),
            NOW,
            || {
                refused_bytes.set(refused_bytes.get() + 1);
                Ok::<_, &'static str>("must not run")
            },
        )
        .expect_err("person at stored ceiling is refused");
    let refusal = match error {
        UploadStartError::Refused(refusal) => refusal,
        other => panic!("expected named ceiling refusal, got {other}"),
    };
    let refused_stored_after = counters
        .read_at(refused_person, NOW)
        .expect("read refused person after")
        .stored_bytes;
    assert_eq!(refusal.person_id, refused_person);
    assert_eq!(refusal.ceiling, UsageCeiling::StoredBytes);
    assert_eq!(refused_bytes.get(), 0);
    assert_eq!(refused_stored_before, STORED_CEILING);
    assert_eq!(refused_stored_after, refused_stored_before);
    println!(
        "TASK0577 refused person={} ceiling={} accepted_bytes={} stored_before={} stored_after={} message=\"{}\"",
        refusal.person_id,
        refusal.ceiling,
        refused_bytes.get(),
        refused_stored_before,
        refused_stored_after,
        refusal,
    );
}
