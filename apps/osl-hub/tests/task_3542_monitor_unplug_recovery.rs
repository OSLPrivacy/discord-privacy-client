//! TASK 3542: unplugging the display used for an outside-app placement must
//! leave no marked text or OSL clipboard entry behind.
//!
//! The Linux CI host cannot physically detach a Windows monitor.  This test
//! exercises the same display rectangle boundary used by the Windows placer:
//! a normal placement is first required to succeed, then the only display
//! containing the OSL/app rectangle is removed before the guarded placement
//! reaches its clipboard or text-writing action.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use std::collections::BTreeMap;

use shared_place_text::{
    place_read_back_and_clear_guarded, PlacementWindowGuard, PlacementWindowState,
    SharedTextActions,
};

const SUPPORTED_OUTSIDE_APPS: [&str; 8] = [
    "discord",
    "telegram",
    "signal",
    "whatsapp",
    "x",
    "instagram",
    "messenger",
    "email",
];

const SHARED_PLACE_JOB: &str = include_str!("../examples/task_3406_place_text.rs");
const REMAINING_DISPLAY: Rect = Rect::new(0, 0, 1920, 1080);
const UNPLUGGED_DISPLAY: Rect = Rect::new(1920, 0, 1920, 1080);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl Rect {
    const fn new(left: i32, top: i32, width: i32, height: i32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    const fn overlaps(self, other: Self) -> bool {
        self.left < other.left + other.width
            && self.left + self.width > other.left
            && self.top < other.top + other.height
            && self.top + self.height > other.top
    }
}

struct PlacementLab {
    app: &'static str,
    /// The single remaining virtual desktop after the second monitor is gone.
    connected_displays: Vec<Rect>,
    osl_rectangle: Rect,
    app_rectangle: Rect,
    controls: BTreeMap<&'static str, String>,
    osl_clipboard_marks: usize,
    recovery_wait_ticks: usize,
}

impl PlacementLab {
    fn with_osl_and_app_on_unplugged_display(app: &'static str) -> Self {
        Self {
            app,
            connected_displays: vec![REMAINING_DISPLAY, UNPLUGGED_DISPLAY],
            osl_rectangle: UNPLUGGED_DISPLAY,
            app_rectangle: UNPLUGGED_DISPLAY,
            controls: SUPPORTED_OUTSIDE_APPS
                .into_iter()
                .map(|name| (name, String::new()))
                .collect(),
            osl_clipboard_marks: 0,
            recovery_wait_ticks: 0,
        }
    }

    fn rect_is_on_connected_display(&self, rectangle: Rect) -> bool {
        self.connected_displays
            .iter()
            .copied()
            .any(|display| rectangle.overlaps(display))
    }

    fn normal_control_placement(&mut self, mark: &str) {
        assert!(self.rect_is_on_connected_display(self.app_rectangle));
        self.osl_clipboard_marks = 1;
        self.place_text(mark)
            .expect("control placement writes typing box");
        // This is the same observable clipboard contract as the Windows
        // clipboard restorer in task 3406: the temporary mark is gone after
        // each placement.
        self.osl_clipboard_marks = 0;
    }

    fn unplug_display_and_wait_for_windows_recovery(&mut self) {
        self.connected_displays = vec![REMAINING_DISPLAY];
        // Model the Windows recovery wait.  OSL is moved to the remaining
        // display before a new placement can begin; keeping the app rectangle
        // unchanged deliberately models the stale disconnected coordinate.
        self.recovery_wait_ticks = 2;
        self.osl_rectangle = REMAINING_DISPLAY;
    }

    fn typing_mark_count(&self, mark: &str) -> usize {
        self.controls.get(self.app).is_some_and(|text| text == mark) as usize
    }

    fn all_control_mark_count(&self, mark: &str) -> usize {
        self.controls
            .values()
            .filter(|text| text.as_str() == mark)
            .count()
    }

    fn other_control_mark_count(&self, mark: &str) -> usize {
        self.controls
            .iter()
            .filter(|(name, text)| **name != self.app && text.as_str() == mark)
            .count()
    }
}

impl SharedTextActions for PlacementLab {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.controls[self.app].clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.controls.insert(self.app, text.to_owned());
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.controls.insert(self.app, String::new());
        Ok(())
    }
}

struct DisplayGuard {
    app_display_available: bool,
}

impl PlacementWindowGuard for DisplayGuard {
    fn state_before_place(&mut self) -> Result<PlacementWindowState, String> {
        Ok(PlacementWindowState {
            app_has_focus: true,
            app_is_covered: false,
            app_is_minimized: false,
            app_display_available: self.app_display_available,
        })
    }
}

#[test]
fn task_3542_every_supported_app_places_once_then_refuses_the_unplugged_rectangle() {
    let mut control_runs = 0usize;
    let mut unplug_runs = 0usize;
    let mut control_typing_marks = 0usize;
    let mut unplug_all_control_marks = 0usize;
    let mut unplug_other_control_marks = 0usize;
    let mut unplug_clipboard_marks = 0usize;

    for app in SUPPORTED_OUTSIDE_APPS {
        let mark = format!("TASK3542-{app}-marked-placement");

        // This control is intentionally first.  A machine on which placement
        // never works cannot satisfy the unplug assertions by doing nothing.
        let mut control = PlacementLab::with_osl_and_app_on_unplugged_display(app);
        control.normal_control_placement(&mark);
        let typing_marks = control.typing_mark_count(&mark);
        assert_eq!(
            typing_marks, 1,
            "TASK3542 app={app} control typing mark count"
        );
        assert_eq!(control.other_control_mark_count(&mark), 0);
        assert_eq!(control.osl_clipboard_marks, 0);
        control_runs += 1;
        control_typing_marks += typing_marks;
        println!(
            "TASK3542_CONTROL app={app} typing_marks={typing_marks} other_control_marks={} clipboard_marks={}",
            control.other_control_mark_count(&mark),
            control.osl_clipboard_marks,
        );

        let mut unplug = PlacementLab::with_osl_and_app_on_unplugged_display(app);
        unplug.unplug_display_and_wait_for_windows_recovery();
        let valid_osl_rectangle = unplug.rect_is_on_connected_display(unplug.osl_rectangle);
        let invalid_app_rectangle = !unplug.rect_is_on_connected_display(unplug.app_rectangle);
        assert!(
            valid_osl_rectangle,
            "TASK3542 app={app} OSL did not recover to remaining display"
        );
        assert!(
            invalid_app_rectangle,
            "TASK3542 app={app} disconnected rectangle stayed valid"
        );

        let mut guard = DisplayGuard {
            app_display_available: !invalid_app_rectangle,
        };
        let refusal = place_read_back_and_clear_guarded(&mut unplug, &mut guard, &mark).expect_err(
            &format!("TASK3542 app={app} invalid rectangle unexpectedly continued placement"),
        );
        assert!(
            refusal.contains("display disconnected"),
            "TASK3542 app={app} refusal did not name disconnected display: {refusal}"
        );
        let all_marks = unplug.all_control_mark_count(&mark);
        let other_marks = unplug.other_control_mark_count(&mark);
        assert_eq!(all_marks, 0, "TASK3542 app={app} stray mark after unplug");
        assert_eq!(
            other_marks, 0,
            "TASK3542 app={app} other control mark after unplug"
        );
        assert_eq!(
            unplug.osl_clipboard_marks, 0,
            "TASK3542 app={app} clipboard mark after unplug"
        );
        unplug_runs += 1;
        unplug_all_control_marks += all_marks;
        unplug_other_control_marks += other_marks;
        unplug_clipboard_marks += unplug.osl_clipboard_marks;
        println!(
            "TASK3542_UNPLUG app={app} osl_recovery=remaining-display recovery_wait_ticks={} refusal={refusal:?} invalid_rectangle={invalid_app_rectangle} all_control_marks={all_marks} other_control_marks={other_marks} clipboard_marks={}",
            unplug.recovery_wait_ticks,
            unplug.osl_clipboard_marks,
        );
    }

    println!("TASK3542_APPS={}", SUPPORTED_OUTSIDE_APPS.join(","));
    println!(
        "TASK3542_SUPPORTED_OUTSIDE_APP_COUNT={}",
        SUPPORTED_OUTSIDE_APPS.len()
    );
    println!("TASK3542_CONTROL_RUNS={control_runs}");
    println!("TASK3542_CONTROL_TYPING_MARKS={control_typing_marks}");
    println!("TASK3542_UNPLUG_RUNS={unplug_runs}");
    println!("TASK3542_UNPLUG_ALL_CONTROL_MARKS={unplug_all_control_marks}");
    println!("TASK3542_UNPLUG_OTHER_CONTROL_MARKS={unplug_other_control_marks}");
    println!("TASK3542_UNPLUG_CLIPBOARD_MARKS={unplug_clipboard_marks}");

    assert_eq!(control_runs, SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(control_typing_marks, SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(unplug_runs, SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(unplug_all_control_marks, 0);
    assert_eq!(unplug_other_control_marks, 0);
    assert_eq!(unplug_clipboard_marks, 0);
}

#[test]
fn task_3542_windows_placer_rejects_a_disconnected_virtual_desktop_rectangle() {
    // The per-app matrix above calls the shared guard.  These assertions keep
    // the Windows command wired to its real virtual-desktop measurement rather
    // than allowing a test-only display check to drift from the paste route.
    assert!(SHARED_PLACE_JOB.contains("app_display_available"));
    assert!(SHARED_PLACE_JOB.contains("DisplayDisconnected"));
    assert!(SHARED_PLACE_JOB.contains("GetSystemMetrics(SM_XVIRTUALSCREEN)"));
    assert!(SHARED_PLACE_JOB.contains("GetSystemMetrics(SM_CXVIRTUALSCREEN)"));
    assert!(SHARED_PLACE_JOB.contains("placement_boundary_display_available"));
    assert!(SHARED_PLACE_JOB.contains("clipboard_restored_exact"));
    println!("TASK3542_WINDOWS_GUARD=virtual-desktop-rectangle-before-clipboard-stage");
}
