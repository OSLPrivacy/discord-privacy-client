//! TASK 0864 - write quiet hours.
//!
//! Finish line:
//! - a direct read returns the three saved values (start time, end time, and
//!   the on or off choice), and
//! - a start time equal to the end time is refused by name.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::quiet_hours::{QuietHoursCommand, QuietHoursState};

fn temporary_file(tag: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "osl-hub-quiet-hours-{tag}-{}-{nonce}",
            std::process::id()
        ))
        .join("quiet-hours.json")
}

#[test]
fn direct_read_returns_the_three_saved_quiet_hours_values() {
    let path = temporary_file("save");
    let state = QuietHoursState::load(path.clone());

    let saved = state
        .save_command(QuietHoursCommand {
            start: "22:30".to_owned(),
            end: "06:45".to_owned(),
            enabled: true,
        })
        .expect("an overnight quiet hours window saves");

    assert_eq!(saved.start, "22:30");
    assert_eq!(saved.end, "06:45");
    assert!(saved.enabled);

    // The direct read: a fresh state loaded from the same file, not the
    // in-memory copy that just did the save.
    let read = QuietHoursState::load(path.clone())
        .settings()
        .expect("saved quiet hours read back");
    assert_eq!(read.start, "22:30");
    assert_eq!(read.end, "06:45");
    assert!(read.enabled);
    println!(
        "quiet hours direct read start={} end={} enabled={}",
        read.start, read.end, read.enabled
    );

    // The off choice round-trips too - "off" must be a saved value, not the
    // absence of one.
    state
        .save_command(QuietHoursCommand {
            start: "08:00".to_owned(),
            end: "17:00".to_owned(),
            enabled: false,
        })
        .expect("a daytime quiet hours window saves switched off");
    let read = QuietHoursState::load(path)
        .settings()
        .expect("re-saved quiet hours read back");
    assert_eq!(read.start, "08:00");
    assert_eq!(read.end, "17:00");
    assert!(!read.enabled);
    println!(
        "quiet hours direct read start={} end={} enabled={}",
        read.start, read.end, read.enabled
    );
}

#[test]
fn a_start_time_equal_to_the_end_time_is_refused_by_name() {
    let path = temporary_file("equal");
    let state = QuietHoursState::load(path.clone());

    let refused = state
        .save_command(QuietHoursCommand {
            start: "09:15".to_owned(),
            end: "09:15".to_owned(),
            enabled: true,
        })
        .expect_err("a zero-length quiet hours window must be refused");

    // Refused BY NAME: the message names quiet hours and the equal times, so
    // the refusal cannot be mistaken for a generic storage failure.
    assert!(
        refused.contains("Quiet hours"),
        "refusal must name quiet hours, got: {refused}"
    );
    assert!(
        refused.contains("start time 09:15 equals the end time 09:15"),
        "refusal must name the equal start and end times, got: {refused}"
    );
    println!("quiet hours refusal: {refused}");

    // The refused save must not have written anything.
    assert!(
        !path.exists(),
        "a refused quiet hours save must not create the settings file"
    );
}
