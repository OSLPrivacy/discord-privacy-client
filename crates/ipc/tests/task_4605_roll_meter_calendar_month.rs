use ipc::metered_bytes::{MeteredByteClass, MeteredSendPath, MonthlyAllowanceMeter};
use std::ffi::OsString;

const ACCOUNT_RESET: i64 = 1_785_585_600; // 2026-08-01 12:00:00 UTC
const OLD_EVENT_AT: i64 = 1_788_177_540; // 2026-08-31 11:59:00 UTC
const ONE_MINUTE_BEFORE_RESET: i64 = 1_788_263_940; // 2026-09-01 11:59:00 UTC
const ONE_MINUTE_AFTER_RESET: i64 = 1_788_264_060; // 2026-09-01 12:01:00 UTC
const REQUESTED_TOTAL: u64 = 3_151_353;

fn seed_old_month(meter: &mut MonthlyAllowanceMeter) {
    meter
        .record_send_at(
            OLD_EVENT_AT,
            MeteredSendPath::BackgroundCoverTick,
            4_096,
            "old-background",
        )
        .expect("record background bytes");
    meter
        .record_message_at(OLD_EVENT_AT, 37, "old-message")
        .expect("record message bytes and count together");
    meter
        .record_send_at(
            OLD_EVENT_AT,
            MeteredSendPath::Attachments,
            1_048_576,
            "old-attachment",
        )
        .expect("record attachment bytes");
    meter
        .record_send_at(
            OLD_EVENT_AT,
            MeteredSendPath::StoryAndPostMedia,
            2_097_152,
            "old-story",
        )
        .expect("record story bytes");
    meter
        .record_send_at(
            OLD_EVENT_AT,
            MeteredSendPath::MultiDeviceSyncTraffic,
            1_492,
            "old-sync",
        )
        .expect("record sync bytes");
}

fn restore_timezone(original: Option<OsString>) {
    if let Some(value) = original {
        std::env::set_var("TZ", value);
    } else {
        std::env::remove_var("TZ");
    }
}

#[test]
fn task_4605_rolls_month_and_daily_figures_on_timezone_independent_clocks() {
    let mut meter = MonthlyAllowanceMeter::new(ACCOUNT_RESET);
    seed_old_month(&mut meter);

    let old_month = meter
        .usage_at(ONE_MINUTE_BEFORE_RESET)
        .expect("read old allowance month");
    assert_eq!(old_month.current_month().class_totals().len(), 6);
    for (byte_class, bytes) in old_month.current_month().class_totals() {
        println!("TASK4605_OLD_CLASS {}={bytes}", byte_class.name());
    }
    println!(
        "TASK4605_OLD_MONTH total={}",
        old_month.current_month().total_bytes()
    );
    assert_eq!(old_month.current_month().total_bytes(), REQUESTED_TOTAL);

    // At 11:58 the five old events are 23h59m old. Exactly one minute later
    // they cross the single shared rolling cutoff, so both daily figures drop.
    let rolling_before = meter
        .usage_at(ONE_MINUTE_BEFORE_RESET - 60)
        .expect("read rolling figures before cutoff")
        .rolling_24_hours();
    let rolling_after = old_month.rolling_24_hours();
    println!(
        "TASK4605_ROLLING_BEFORE transfer={} messages={}",
        rolling_before.transfer_bytes(),
        rolling_before.message_count()
    );
    println!(
        "TASK4605_ROLLING_AFTER transfer={} messages={} shared_cutoff={}",
        rolling_after.transfer_bytes(),
        rolling_after.message_count(),
        rolling_after.window_start_exclusive()
    );
    assert_eq!(
        (
            rolling_before.transfer_bytes(),
            rolling_before.message_count()
        ),
        (REQUESTED_TOTAL, 1)
    );
    assert_eq!(
        (
            rolling_after.transfer_bytes(),
            rolling_after.message_count()
        ),
        (0, 0)
    );

    let rolled = meter
        .usage_at(ONE_MINUTE_AFTER_RESET)
        .expect("read one minute after monthly reset");
    println!(
        "TASK4605_MONTH_ROLLED current={} previous={}",
        rolled.current_month().total_bytes(),
        rolled.previous_month().total_bytes()
    );
    assert_eq!(rolled.current_month().total_bytes(), 0);
    assert_eq!(rolled.previous_month().total_bytes(), REQUESTED_TOTAL);

    meter
        .record_message_at(ONE_MINUTE_AFTER_RESET, 37, "new-message")
        .expect("record new-month message");
    let after_message = meter
        .usage_at(ONE_MINUTE_AFTER_RESET)
        .expect("read new-month message");
    println!(
        "TASK4605_AFTER_MESSAGE current={} previous={} transfer_24h={} messages_24h={}",
        after_message.current_month().total_bytes(),
        after_message.previous_month().total_bytes(),
        after_message.rolling_24_hours().transfer_bytes(),
        after_message.rolling_24_hours().message_count()
    );
    assert_eq!(after_message.current_month().total_bytes(), 37);
    assert_eq!(
        after_message.previous_month().total_bytes(),
        REQUESTED_TOTAL
    );
    assert_eq!(
        (
            after_message.rolling_24_hours().transfer_bytes(),
            after_message.rolling_24_hours().message_count()
        ),
        (37, 1)
    );

    // The meter uses only UTC civil arithmetic and absolute Unix seconds.
    // Deliberately move the process timezone across the date line and verify
    // both reset instants remain byte-for-byte identical.
    let original_timezone = std::env::var_os("TZ");
    std::env::set_var("TZ", "Pacific/Kiritimati");
    let kiritimati_meter = MonthlyAllowanceMeter::new(ACCOUNT_RESET);
    let kiritimati = kiritimati_meter.usage_at(ONE_MINUTE_AFTER_RESET);
    std::env::set_var("TZ", "America/Los_Angeles");
    let los_angeles_meter = MonthlyAllowanceMeter::new(ACCOUNT_RESET);
    let los_angeles = los_angeles_meter.usage_at(ONE_MINUTE_AFTER_RESET);
    restore_timezone(original_timezone);
    let kiritimati = kiritimati.expect("construct and read meter in UTC+14");
    let los_angeles = los_angeles.expect("construct and read meter in Pacific timezone");

    let month_reset_kiritimati = kiritimati.current_month().window().start_unix_seconds();
    let month_reset_los_angeles = los_angeles.current_month().window().start_unix_seconds();
    let daily_reset_kiritimati = kiritimati.rolling_24_hours().window_start_exclusive();
    let daily_reset_los_angeles = los_angeles.rolling_24_hours().window_start_exclusive();
    println!(
        "TASK4605_TIMEZONE month_reset_kiritimati={month_reset_kiritimati} month_reset_los_angeles={month_reset_los_angeles} daily_cutoff_kiritimati={daily_reset_kiritimati} daily_cutoff_los_angeles={daily_reset_los_angeles}"
    );
    assert_eq!(month_reset_kiritimati, month_reset_los_angeles);
    assert_eq!(daily_reset_kiritimati, daily_reset_los_angeles);

    assert_eq!(
        old_month.current_month().class_totals(),
        &[
            (MeteredByteClass::BackgroundConnection, 4_096),
            (MeteredByteClass::Messages, 37),
            (MeteredByteClass::Attachments, 1_048_576),
            (MeteredByteClass::StoriesAndPosts, 2_097_152),
            (MeteredByteClass::Voice, 0),
            (MeteredByteClass::MultiDeviceSync, 1_492),
        ]
    );
}

#[test]
fn task_4605_short_month_clamps_without_drifting_the_account_reset_day() {
    let meter = MonthlyAllowanceMeter::new(1_769_860_800); // 2026-01-31 12:00 UTC

    let february = meter
        .usage_at(1_772_323_200) // 2026-03-01 00:00 UTC
        .expect("read clamped February window")
        .current_month()
        .window();
    let march = meter
        .usage_at(1_775_001_600) // 2026-04-01 00:00 UTC
        .expect("read restored March window")
        .current_month()
        .window();

    println!(
        "TASK4605_SHORT_MONTH feb_start={} feb_end={} mar_start={} mar_end={}",
        february.start_unix_seconds(),
        february.end_unix_seconds(),
        march.start_unix_seconds(),
        march.end_unix_seconds()
    );
    assert_eq!(
        (february.start_unix_seconds(), february.end_unix_seconds()),
        (1_772_280_000, 1_774_958_400) // Feb 28 12:00 through Mar 31 12:00
    );
    assert_eq!(
        (march.start_unix_seconds(), march.end_unix_seconds()),
        (1_774_958_400, 1_777_550_400) // Mar 31 restored, then Apr 30 clamp
    );
}
