//! TASK 0865 - connect quiet hours to notices.
//!
//! Finish line:
//! - with quiet hours on, a notice raised inside the window makes zero sounds
//!   and zero pop-ups,
//! - the same notice raised outside the window makes exactly one of each, and
//! - the held notice appears once when the window ends.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::quiet_hours::{QuietHoursCommand, QuietHoursState};
use osl_privacy_hub::quiet_hours_notices::{
    Notice, NoticeDelivery, NoticeSink, QuietHoursNoticeGate,
};

fn temporary_file(tag: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir()
        .join(format!(
            "osl-hub-quiet-hours-notices-{tag}-{}-{nonce}",
            std::process::id()
        ))
        .join("quiet-hours.json")
}

/// Counts every sound and pop-up the gate asks for, per notice ID, so each
/// finish-line number is a measured count rather than an assumption.
#[derive(Default)]
struct CountingSink {
    events: Vec<(&'static str, String)>,
}

impl CountingSink {
    fn sounds(&self, id: &str) -> usize {
        self.count("sound", id)
    }

    fn popups(&self, id: &str) -> usize {
        self.count("popup", id)
    }

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

fn notice() -> Notice {
    Notice {
        id: "NOTICE-0865".to_owned(),
        title: "New message".to_owned(),
        detail: "A friend wrote while you were away".to_owned(),
    }
}

#[test]
fn quiet_hours_hold_a_notice_and_release_it_once_when_the_window_ends() {
    // The settings come from the real 0864 store, not a hand-built struct:
    // this is the connection the task asks for.
    let state = QuietHoursState::load(temporary_file("hold"));
    let settings = state
        .save_command(QuietHoursCommand {
            start: "22:00".to_owned(),
            end: "07:00".to_owned(),
            enabled: true,
        })
        .expect("an overnight quiet hours window saves");

    let mut gate = QuietHoursNoticeGate::new();
    let mut sink = CountingSink::default();

    // With quiet hours on, a notice raised inside the window makes zero
    // sounds and zero pop-ups.
    let raised = gate
        .raise(&settings, "23:15", notice(), &mut sink)
        .expect("a notice raised at a valid time is judged");
    assert_eq!(raised, NoticeDelivery::HeldForQuietHours);
    assert_eq!(sink.sounds("NOTICE-0865"), 0, "inside the window: sounds");
    assert_eq!(sink.popups("NOTICE-0865"), 0, "inside the window: pop-ups");
    println!(
        "inside window 23:15 sounds={} popups={}",
        sink.sounds("NOTICE-0865"),
        sink.popups("NOTICE-0865")
    );

    // A clock tick still inside the window releases nothing.
    let released = gate
        .clock_tick(&settings, "06:59", &mut sink)
        .expect("a clock tick at a valid time is judged");
    assert_eq!(released, 0, "06:59 is still inside the window");
    assert_eq!(sink.sounds("NOTICE-0865"), 0);
    assert_eq!(sink.popups("NOTICE-0865"), 0);

    // The held notice appears once when the window ends.
    let released = gate
        .clock_tick(&settings, "07:00", &mut sink)
        .expect("a clock tick at the end time is judged");
    assert_eq!(released, 1, "the window ends at 07:00");
    assert_eq!(
        sink.popups("NOTICE-0865"),
        1,
        "the held notice appears exactly once at release"
    );
    assert_eq!(
        sink.sounds("NOTICE-0865"),
        1,
        "the held notice sounds exactly once at release"
    );
    println!(
        "window end 07:00 released={released} sounds={} popups={}",
        sink.sounds("NOTICE-0865"),
        sink.popups("NOTICE-0865")
    );

    // A later tick must not repeat it.
    let released = gate
        .clock_tick(&settings, "07:05", &mut sink)
        .expect("a clock tick after release is judged");
    assert_eq!(released, 0, "nothing is left to release");
    assert_eq!(sink.popups("NOTICE-0865"), 1, "no duplicate pop-up");
    assert_eq!(sink.sounds("NOTICE-0865"), 1, "no duplicate sound");
}

#[test]
fn the_same_notice_raised_outside_the_window_makes_exactly_one_of_each() {
    let state = QuietHoursState::load(temporary_file("outside"));
    let settings = state
        .save_command(QuietHoursCommand {
            start: "22:00".to_owned(),
            end: "07:00".to_owned(),
            enabled: true,
        })
        .expect("an overnight quiet hours window saves");

    let mut gate = QuietHoursNoticeGate::new();
    let mut sink = CountingSink::default();

    let raised = gate
        .raise(&settings, "12:00", notice(), &mut sink)
        .expect("a notice raised at a valid time is judged");
    assert_eq!(raised, NoticeDelivery::Delivered);
    assert_eq!(
        sink.sounds("NOTICE-0865"),
        1,
        "outside the window: exactly one sound"
    );
    assert_eq!(
        sink.popups("NOTICE-0865"),
        1,
        "outside the window: exactly one pop-up"
    );
    println!(
        "outside window 12:00 sounds={} popups={}",
        sink.sounds("NOTICE-0865"),
        sink.popups("NOTICE-0865")
    );
}

#[test]
fn quiet_hours_switched_off_hold_nothing_even_inside_the_saved_window() {
    let state = QuietHoursState::load(temporary_file("off"));
    let settings = state
        .save_command(QuietHoursCommand {
            start: "22:00".to_owned(),
            end: "07:00".to_owned(),
            enabled: false,
        })
        .expect("a switched-off quiet hours window saves");

    let mut gate = QuietHoursNoticeGate::new();
    let mut sink = CountingSink::default();

    let raised = gate
        .raise(&settings, "23:15", notice(), &mut sink)
        .expect("a notice raised at a valid time is judged");
    assert_eq!(raised, NoticeDelivery::Delivered);
    assert_eq!(sink.sounds("NOTICE-0865"), 1);
    assert_eq!(sink.popups("NOTICE-0865"), 1);
    println!(
        "quiet hours off 23:15 sounds={} popups={}",
        sink.sounds("NOTICE-0865"),
        sink.popups("NOTICE-0865")
    );
}
