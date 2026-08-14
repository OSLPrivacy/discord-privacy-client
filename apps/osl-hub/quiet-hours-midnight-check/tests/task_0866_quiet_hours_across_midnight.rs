//! TASK 0866 - check quiet hours across midnight.
//!
//! Finish line:
//! - quiet hours 23:00 -> 07:00, one notice raised at 23:30, one at 03:00, one
//!   at 08:00: the first two make zero sounds, the third makes exactly one;
//! - turning quiet hours off makes all three make one each;
//! - pointing the check at a fixture whose window does not cross midnight
//!   makes the check fail.
//!
//! The three modules below are the shipped hub sources, included by path, not
//! copies: `apps/osl-hub/src/quiet_hours.rs` (the 0864 store),
//! `apps/osl-hub/src/quiet_hours_notices.rs` (the 0865 gate), and the
//! `atomic_file` helper the store writes through. `quiet_hours.rs` reaches for
//! `crate::atomic_file` and `quiet_hours_notices.rs` for `crate::quiet_hours`,
//! and in an integration test binary `crate` is this file - so the three names
//! have to be declared here, in this order-independent flat layout, exactly as
//! `apps/osl-hub/src/lib.rs` declares them.

#[path = "../../src/atomic_file.rs"]
mod atomic_file;
#[path = "../../src/quiet_hours.rs"]
mod quiet_hours;
#[path = "../../src/quiet_hours_notices.rs"]
mod quiet_hours_notices;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use quiet_hours::{QuietHoursCommand, QuietHoursSettings, QuietHoursState};
use quiet_hours_notices::{Notice, NoticeSink, QuietHoursNoticeGate};

/// The three clock times the task names, in the order the night runs them.
const RAISED_AT: [&str; 3] = ["23:30", "03:00", "08:00"];

/// What the finish line demands with quiet hours on across midnight: the
/// 23:30 and 03:00 notices silent, the 08:00 notice sounding exactly once.
const EXPECTED_INSIDE_WINDOW: [usize; 3] = [0, 0, 1];

/// What the finish line demands with quiet hours switched off: every notice
/// sounds exactly once, whatever the clock says.
const EXPECTED_QUIET_HOURS_OFF: [usize; 3] = [1, 1, 1];

fn temporary_file(tag: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "osl-hub-quiet-hours-midnight-{tag}-{}-{nonce}",
            std::process::id()
        ))
        .join("quiet-hours.json")
}

/// Counts sounds and pop-ups per notice, so every finish-line number below is
/// a count this run measured rather than a shape the test assumed.
#[derive(Default)]
struct CountingSink {
    events: Vec<(&'static str, String)>,
}

impl CountingSink {
    fn count(&self, kind: &str, id: &str) -> usize {
        self.events
            .iter()
            .filter(|(event, notice)| *event == kind && notice == id)
            .count()
    }
}

impl NoticeSink for CountingSink {
    fn play_sound(&mut self, notice: &Notice) {
        self.events.push(("sound", notice.id.clone()));
    }

    fn show_popup(&mut self, notice: &Notice) {
        self.events.push(("popup", notice.id.clone()));
    }
}

fn notice_id(index: usize) -> String {
    format!("NOTICE-0866-{}", index + 1)
}

/// The fixture a run of the check is pointed at: quiet hours settings saved
/// through the real 0864 store, read back out of it, never hand-built.
fn fixture(tag: &str, start: &str, end: &str, enabled: bool) -> QuietHoursSettings {
    let state = QuietHoursState::load(temporary_file(tag));
    state
        .save_command(QuietHoursCommand {
            start: start.to_owned(),
            end: end.to_owned(),
            enabled,
        })
        .unwrap_or_else(|refusal| panic!("fixture {tag} ({start} -> {end}) must save: {refusal}"));
    state
        .settings()
        .unwrap_or_else(|refusal| panic!("fixture {tag} must read back: {refusal}"))
}

/// What one night of the check measured.
#[derive(Debug)]
struct MidnightReport {
    sounds: [usize; 3],
    popups: [usize; 3],
}

/// Raise the three notices, on one gate, in clock order, and count what each
/// one made. One gate for all three is the point: 23:30 and 03:00 sit either
/// side of midnight inside the same window, and 08:00 is past its end.
fn raise_the_three_notices(settings: &QuietHoursSettings) -> Result<MidnightReport, String> {
    let mut gate = QuietHoursNoticeGate::new();
    let mut sink = CountingSink::default();

    for (index, clock) in RAISED_AT.iter().enumerate() {
        gate.raise(
            settings,
            clock,
            Notice {
                id: notice_id(index),
                title: "New message".to_owned(),
                detail: format!("A friend wrote at {clock}"),
            },
            &mut sink,
        )?;
    }

    let mut sounds = [0usize; 3];
    let mut popups = [0usize; 3];
    for index in 0..RAISED_AT.len() {
        sounds[index] = sink.count("sound", &notice_id(index));
        popups[index] = sink.count("popup", &notice_id(index));
    }
    Ok(MidnightReport { sounds, popups })
}

/// The check itself, pointed at one fixture. Refuses by name when the counts
/// are not the ones the finish line demands, so a fixture whose window does
/// not cross midnight fails here rather than passing quietly.
fn check_quiet_hours_across_midnight(
    settings: &QuietHoursSettings,
    expected: [usize; 3],
) -> Result<MidnightReport, String> {
    let report = raise_the_three_notices(settings)?;
    println!(
        "quiet hours {} -> {} enabled={} | {} sounds={} popups={} | {} sounds={} popups={} | {} sounds={} popups={}",
        settings.start,
        settings.end,
        settings.enabled,
        RAISED_AT[0], report.sounds[0], report.popups[0],
        RAISED_AT[1], report.sounds[1], report.popups[1],
        RAISED_AT[2], report.sounds[2], report.popups[2],
    );
    if report.sounds != expected {
        return Err(format!(
            "Quiet hours across midnight failed: the window {} -> {} made {:?} sounds \
             at {:?}, and the check demands {:?}",
            settings.start, settings.end, report.sounds, RAISED_AT, expected
        ));
    }
    Ok(report)
}

#[test]
fn quiet_hours_from_2300_to_0700_silence_2330_and_0300_and_let_0800_sound_once() {
    let settings = fixture("across-midnight", "23:00", "07:00", true);
    assert_eq!(settings.start, "23:00");
    assert_eq!(settings.end, "07:00");
    assert!(settings.enabled);

    let report = check_quiet_hours_across_midnight(&settings, EXPECTED_INSIDE_WINDOW)
        .expect("23:00 -> 07:00 must pass the midnight check");

    assert_eq!(report.sounds[0], 0, "23:30 is inside the window: sounds");
    assert_eq!(report.sounds[1], 0, "03:00 is past midnight, still inside");
    assert_eq!(report.sounds[2], 1, "08:00 is outside: exactly one sound");
    assert_eq!(report.popups, [0, 0, 1], "pop-ups follow the same window");
}

#[test]
fn turning_quiet_hours_off_makes_all_three_notices_sound_once_each() {
    let settings = fixture("switched-off", "23:00", "07:00", false);
    assert!(!settings.enabled);

    let report = check_quiet_hours_across_midnight(&settings, EXPECTED_QUIET_HOURS_OFF)
        .expect("switched-off quiet hours must let all three through");

    assert_eq!(report.sounds, [1, 1, 1], "every notice sounds exactly once");
    assert_eq!(report.popups, [1, 1, 1], "every notice pops up exactly once");
}

#[test]
fn a_fixture_whose_window_does_not_cross_midnight_fails_the_check() {
    // Same three notices, same enabled flag - only the window changes, to one
    // that runs inside a single day (07:00 < 23:00, so it never crosses
    // midnight). The check must refuse it rather than report a pass.
    let settings = fixture("same-day-window", "07:00", "23:00", true);
    assert!(
        settings.start < settings.end,
        "the negative fixture's window must not cross midnight"
    );

    let refusal = check_quiet_hours_across_midnight(&settings, EXPECTED_INSIDE_WINDOW)
        .expect_err("a window that does not cross midnight must fail the check");
    println!("same-day fixture refusal: {refusal}");

    assert!(
        refusal.starts_with("Quiet hours across midnight failed: the window 07:00 -> 23:00 made"),
        "the refusal must name the fixture it rejected, got: {refusal}"
    );
    // And it fails for the right reason: 23:30 and 03:00 fall outside a
    // same-day window, so they sound; 08:00 falls inside it, so it is held.
    let report = raise_the_three_notices(&settings).expect("the three notices are still judged");
    assert_eq!(report.sounds, [1, 1, 0], "the same-day window inverts every count");
}
