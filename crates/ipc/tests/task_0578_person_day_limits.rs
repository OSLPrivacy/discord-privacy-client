use ipc::usage_counters::{
    UploadRefusal, UploadStartError, UsageCeiling, UsageCeilings, UsageCounterStore,
};
use std::cell::Cell;

const PERSON: &str = "task-0578-one-person";
const SECONDS_PER_DAY: i64 = 86_400;
const FIRST_DAY: i64 = 20_100;
const MIB: u64 = 1024 * 1024;

// Approved Free limits from task 0575. These are deliberately independent of
// UsageCeilings::FREE so this test goes red if production moves a ceiling.
const STATED_STORED_BYTES: u64 = 1024 * MIB;
const STATED_DAILY_BYTES: u64 = 250 * MIB;
const STATED_DAILY_MESSAGES: u64 = 200;

fn at_day(offset: i64) -> i64 {
    (FIRST_DAY + offset) * SECONDS_PER_DAY
}

fn expect_one_byte_refusal(
    counters: &mut UsageCounterStore,
    file_id: &str,
    now: i64,
    expected_ceiling: UsageCeiling,
    stated_number: u64,
) -> UploadRefusal {
    let accepted_bytes = Cell::new(0_u64);
    let result =
        counters.with_upload_admission(PERSON, file_id, 1, UsageCeilings::FREE, now, || {
            accepted_bytes.set(accepted_bytes.get() + 1);
            Ok::<_, &'static str>("accepted unexpectedly")
        });

    let refusal = match result {
        Err(UploadStartError::Refused(refusal)) if refusal.ceiling == expected_ceiling => refusal,
        Err(UploadStartError::Refused(refusal)) => panic!(
            "{expected_ceiling} did not stop the upload at exact stated number \
             {stated_number}; {other} stopped it instead (current {current}, limit {limit})",
            other = refusal.ceiling,
            current = refusal.current,
            limit = refusal.limit,
        ),
        Err(other) => panic!(
            "{expected_ceiling} did not stop the upload at exact stated number \
             {stated_number}; unexpected error: {other}"
        ),
        Ok(_) => panic!(
            "{expected_ceiling} did not stop the upload at exact stated number \
             {stated_number}; the one-byte upload was accepted"
        ),
    };

    assert_eq!(
        accepted_bytes.get(),
        0,
        "refusal must precede byte acceptance"
    );
    assert_eq!(refusal.person_id, PERSON);
    assert_eq!(refusal.current, stated_number);
    assert_eq!(refusal.limit, stated_number);
    println!(
        "TASK0578 refusal person={} ceiling={} stated_number={} current={} limit={} accepted_bytes={}",
        refusal.person_id,
        refusal.ceiling,
        stated_number,
        refusal.current,
        refusal.limit,
        accepted_bytes.get(),
    );
    refusal
}

#[test]
fn task_0578_all_four_exact_ceilings_refuse_and_daily_limits_roll_over() {
    let dir = tempfile::tempdir().expect("temporary counter directory");
    let mut counters = UsageCounterStore::open(dir.path()).expect("open usage counters");

    // Five real admissions reach 1 GiB without crossing the 250 MiB daily send
    // ceiling: 250 MiB on four days, then the remaining 24 MiB on day five.
    let stored_uploads = [250 * MIB, 250 * MIB, 250 * MIB, 250 * MIB, 24 * MIB];
    let accepted_stored = Cell::new(0_u64);
    for (day, byte_len) in stored_uploads.into_iter().enumerate() {
        counters
            .with_upload_admission(
                PERSON,
                &format!("stored-part-{day}"),
                byte_len,
                UsageCeilings::FREE,
                at_day(day as i64),
                || {
                    accepted_stored.set(accepted_stored.get() + byte_len);
                    Ok::<_, &'static str>(())
                },
            )
            .expect("upload used to reach the stored ceiling must be accepted");
    }
    let at_stored_ceiling = counters
        .read_at(PERSON, at_day(4))
        .expect("read exact stored ceiling");
    assert_eq!(accepted_stored.get(), STATED_STORED_BYTES);
    assert_eq!(at_stored_ceiling.stored_bytes, STATED_STORED_BYTES);
    assert_eq!(at_stored_ceiling.bytes_sent_today, 24 * MIB);
    println!(
        "TASK0578 reached person={PERSON} ceiling=stored bytes ceiling exact={} accepted_uploads=5",
        at_stored_ceiling.stored_bytes,
    );
    expect_one_byte_refusal(
        &mut counters,
        "stored-over-limit",
        at_day(4),
        UsageCeiling::StoredBytes,
        STATED_STORED_BYTES,
    );

    // Free the held storage, then use one accepted upload to fill the remaining
    // 226 MiB of this day's send budget. Removing that file leaves only the
    // daily counter as the reason the next upload can be refused.
    for day in 0..stored_uploads.len() {
        counters
            .remove_file(PERSON, &format!("stored-part-{day}"))
            .expect("remove stored-ceiling fixture file");
    }
    let sent_fill = STATED_DAILY_BYTES - 24 * MIB;
    counters
        .with_upload_admission(
            PERSON,
            "sent-fill",
            sent_fill,
            UsageCeilings::FREE,
            at_day(4),
            || Ok::<_, &'static str>(()),
        )
        .expect("upload that reaches the daily send ceiling must be accepted");
    counters
        .remove_file(PERSON, "sent-fill")
        .expect("remove sent-ceiling fixture file");
    let at_sent_ceiling = counters
        .read_at(PERSON, at_day(4))
        .expect("read exact sent ceiling");
    assert_eq!(at_sent_ceiling.stored_bytes, 0);
    assert_eq!(at_sent_ceiling.bytes_sent_today, STATED_DAILY_BYTES);
    println!(
        "TASK0578 reached person={PERSON} ceiling=bytes sent today ceiling exact={}",
        at_sent_ceiling.bytes_sent_today,
    );
    expect_one_byte_refusal(
        &mut counters,
        "sent-over-limit",
        at_day(4),
        UsageCeiling::BytesSentToday,
        STATED_DAILY_BYTES,
    );

    // The next UTC day clears the send counter. Bring fetched bytes to its
    // approved number, then prove upload admission names that exact blocker.
    counters
        .record_bytes_fetched(PERSON, STATED_DAILY_BYTES, at_day(5))
        .expect("reach fetched-byte ceiling");
    let at_fetched_ceiling = counters
        .read_at(PERSON, at_day(5))
        .expect("read exact fetched ceiling");
    assert_eq!(at_fetched_ceiling.bytes_sent_today, 0);
    assert_eq!(at_fetched_ceiling.bytes_fetched_today, STATED_DAILY_BYTES);
    println!(
        "TASK0578 reached person={PERSON} ceiling=bytes fetched today ceiling exact={}",
        at_fetched_ceiling.bytes_fetched_today,
    );
    expect_one_byte_refusal(
        &mut counters,
        "fetched-over-limit",
        at_day(5),
        UsageCeiling::BytesFetchedToday,
        STATED_DAILY_BYTES,
    );

    // On another UTC day, send exactly 200 messages for this same person.
    for _ in 0..STATED_DAILY_MESSAGES {
        counters
            .record_message_sent(PERSON, at_day(6))
            .expect("record message toward daily ceiling");
    }
    let at_message_ceiling = counters
        .read_at(PERSON, at_day(6))
        .expect("read exact message ceiling");
    assert_eq!(at_message_ceiling.bytes_fetched_today, 0);
    assert_eq!(
        at_message_ceiling.messages_sent_today,
        STATED_DAILY_MESSAGES
    );
    println!(
        "TASK0578 reached person={PERSON} ceiling=messages sent today ceiling exact={}",
        at_message_ceiling.messages_sent_today,
    );
    expect_one_byte_refusal(
        &mut counters,
        "message-over-limit",
        at_day(6),
        UsageCeiling::MessagesSentToday,
        STATED_DAILY_MESSAGES,
    );

    // The very next upload after midnight belongs to the same person and must
    // be admitted because all three daily counters read as zero on the new day.
    let rollover_accepted_bytes = Cell::new(0_u64);
    let rollover_receipt = counters
        .with_upload_admission(
            PERSON,
            "after-rollover",
            1,
            UsageCeilings::FREE,
            at_day(7),
            || {
                rollover_accepted_bytes.set(rollover_accepted_bytes.get() + 1);
                Ok::<_, &'static str>("accepted after rollover")
            },
        )
        .expect("same person's next-day upload must be accepted");
    let after_rollover = counters
        .read_at(PERSON, at_day(7))
        .expect("read counters after rollover upload");
    assert_eq!(rollover_receipt, "accepted after rollover");
    assert_eq!(rollover_accepted_bytes.get(), 1);
    assert_eq!(after_rollover.stored_bytes, 1);
    assert_eq!(after_rollover.bytes_sent_today, 1);
    assert_eq!(after_rollover.bytes_fetched_today, 0);
    assert_eq!(after_rollover.messages_sent_today, 0);
    println!(
        "TASK0578 rollover person={PERSON} from_day={} to_day={} accepted_bytes={} stored_bytes={} bytes_sent_today={} bytes_fetched_today={} messages_sent_today={}",
        FIRST_DAY + 6,
        FIRST_DAY + 7,
        rollover_accepted_bytes.get(),
        after_rollover.stored_bytes,
        after_rollover.bytes_sent_today,
        after_rollover.bytes_fetched_today,
        after_rollover.messages_sent_today,
    );
}
