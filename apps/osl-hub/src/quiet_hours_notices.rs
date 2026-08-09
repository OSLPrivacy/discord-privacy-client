//! Connect quiet hours to notices: while the clock is inside an enabled
//! quiet hours window, sound and pop-up notices are held back; a clock tick
//! outside the window releases every held notice exactly once.
//!
//! Delivery itself stays behind [`NoticeSink`] so the gate decides *whether*
//! a notice sounds and pops up without owning *how* — the same seam the
//! window-and-sounds choices connect to.

use crate::quiet_hours::{time_minutes, QuietHoursSettings};

/// One notice: an identity plus the words a pop-up would show.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub detail: String,
}

/// What the gate did with a raised notice.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NoticeDelivery {
    /// The notice made one sound and one pop-up.
    Delivered,
    /// The notice was held back for the end of the quiet hours window.
    HeldForQuietHours,
}

/// Where delivered notices go: one sound and one pop-up per delivery.
pub trait NoticeSink {
    fn play_sound(&mut self, notice: &Notice);
    fn show_popup(&mut self, notice: &Notice);
}

/// The gate between raised notices and the sink, driven by the saved quiet
/// hours settings and the caller's clock.
#[derive(Debug, Default)]
pub struct QuietHoursNoticeGate {
    held: Vec<Notice>,
}

impl QuietHoursNoticeGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many notices are currently held for the end of the window.
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// Raise a notice at the clock time `clock` ("HH:MM"). Inside an enabled
    /// quiet hours window the notice is held and nothing reaches the sink;
    /// otherwise it makes exactly one sound and one pop-up.
    pub fn raise(
        &mut self,
        settings: &QuietHoursSettings,
        clock: &str,
        notice: Notice,
        sink: &mut dyn NoticeSink,
    ) -> Result<NoticeDelivery, String> {
        if is_inside_quiet_hours(settings, clock)? {
            self.held.push(notice);
            Ok(NoticeDelivery::HeldForQuietHours)
        } else {
            deliver(&notice, sink);
            Ok(NoticeDelivery::Delivered)
        }
    }

    /// Move the clock to `clock` ("HH:MM"). Outside the quiet hours window
    /// every held notice is delivered exactly once, in the order it was
    /// raised; inside it nothing happens. Returns how many were released.
    pub fn clock_tick(
        &mut self,
        settings: &QuietHoursSettings,
        clock: &str,
        sink: &mut dyn NoticeSink,
    ) -> Result<usize, String> {
        if is_inside_quiet_hours(settings, clock)? {
            return Ok(0);
        }
        let released = std::mem::take(&mut self.held);
        for notice in &released {
            deliver(notice, sink);
        }
        Ok(released.len())
    }
}

fn deliver(notice: &Notice, sink: &mut dyn NoticeSink) {
    sink.play_sound(notice);
    sink.show_popup(notice);
}

/// Whether `clock` ("HH:MM") falls inside the enabled quiet hours window
/// `[start, end)`. A window whose start is after its end crosses midnight
/// (22:00 -> 07:00 covers 23:15 and 03:00 but not 07:00). Switched-off
/// quiet hours are never "inside".
pub fn is_inside_quiet_hours(settings: &QuietHoursSettings, clock: &str) -> Result<bool, String> {
    if !settings.enabled {
        return Ok(false);
    }
    let now = time_minutes(clock, "clock")?;
    let start = time_minutes(&settings.start, "start")?;
    let end = time_minutes(&settings.end, "end")?;
    Ok(if start < end {
        start <= now && now < end
    } else {
        now >= start || now < end
    })
}
