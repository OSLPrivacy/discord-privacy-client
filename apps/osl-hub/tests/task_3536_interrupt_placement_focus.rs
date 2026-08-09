//! TASK 3536: a changed native-window relationship must refuse before the
//! shared placer puts its marked text into any outside application's control.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use std::collections::BTreeMap;

use shared_place_text::{
    place_read_back_and_clear_guarded, PlacementWindowGuard, PlacementWindowState,
    SharedTextActions,
};

// This is the same outside-app support inventory guarded by task 0004's
// allowed-place contract.  It intentionally includes web-backed providers and
// email, not just the Windows-native adapter enum.
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

const INTERRUPTIONS: [(&str, PlacementWindowState); 3] = [
    (
        "focus changed",
        PlacementWindowState {
            app_has_focus: false,
            app_is_covered: false,
            app_is_minimized: false,
            app_display_available: true,
        },
    ),
    (
        "app covered",
        PlacementWindowState {
            app_has_focus: true,
            app_is_covered: true,
            app_is_minimized: false,
            app_display_available: true,
        },
    ),
    (
        "app minimized",
        PlacementWindowState {
            app_has_focus: true,
            app_is_covered: false,
            app_is_minimized: true,
            app_display_available: true,
        },
    ),
];

struct Controls {
    text: BTreeMap<&'static str, String>,
    place_calls: usize,
}

impl Controls {
    fn new() -> Self {
        Self {
            text: SUPPORTED_OUTSIDE_APPS
                .into_iter()
                .map(|app| (app, String::new()))
                .collect(),
            place_calls: 0,
        }
    }

    fn mark_count(&self) -> usize {
        self.text
            .values()
            .filter(|text| text.contains("TASK3536-"))
            .count()
    }

    fn other_controls_mark_count(&self, active_app: &str) -> usize {
        self.text
            .iter()
            .filter(|(app, text)| **app != active_app && text.contains("TASK3536-"))
            .count()
    }
}

struct AppComposer<'a> {
    app: &'static str,
    controls: &'a mut Controls,
}

impl SharedTextActions for AppComposer<'_> {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.controls.text[self.app].clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.controls.place_calls += 1;
        self.controls.text.insert(self.app, text.to_owned());
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.controls.text.insert(self.app, String::new());
        Ok(())
    }
}

struct InterruptedWindow {
    state: PlacementWindowState,
    samples: usize,
}

impl PlacementWindowGuard for InterruptedWindow {
    fn state_before_place(&mut self) -> Result<PlacementWindowState, String> {
        self.samples += 1;
        Ok(self.state)
    }
}

#[test]
fn task_3536_every_supported_outside_app_refuses_three_pre_paste_interruptions() {
    let mut interrupted_runs = 0usize;
    let mut interrupted_mark_count = 0usize;
    let mut other_control_mark_count = 0usize;

    for app in SUPPORTED_OUTSIDE_APPS {
        for (interruption, state) in INTERRUPTIONS {
            let mark = format!("TASK3536-{app}-{interruption}");
            let mut controls = Controls::new();
            let mut guard = InterruptedWindow { state, samples: 0 };
            let result = {
                let mut composer = AppComposer {
                    app,
                    controls: &mut controls,
                };
                place_read_back_and_clear_guarded(&mut composer, &mut guard, &mark)
            };

            let refusal = result.expect_err("the changed window relationship must refuse");
            assert!(
                refusal.contains(interruption),
                "refusal did not name {interruption:?}: {refusal}"
            );
            assert_eq!(
                guard.samples, 1,
                "{app} {interruption} must sample before paste"
            );
            assert_eq!(
                controls.place_calls, 0,
                "{app} {interruption} placed text despite the interruption"
            );
            assert_eq!(
                controls.mark_count(),
                0,
                "{app} {interruption} left a mark in a control"
            );
            assert_eq!(
                controls.other_controls_mark_count(app),
                0,
                "{app} {interruption} marked another app's control"
            );

            interrupted_runs += 1;
            interrupted_mark_count += controls.mark_count();
            other_control_mark_count += controls.other_controls_mark_count(app);
            println!(
                "TASK3536_INTERRUPT app={app} interruption={interruption:?} refusal={refusal:?} place_calls={} interrupted_marks={} other_control_marks={}",
                controls.place_calls,
                controls.mark_count(),
                controls.other_controls_mark_count(app),
            );
        }
    }

    println!(
        "TASK3536_SUPPORTED_OUTSIDE_APP_COUNT={}",
        SUPPORTED_OUTSIDE_APPS.len()
    );
    println!("TASK3536_INTERRUPTION_RUNS={interrupted_runs}");
    println!("TASK3536_INTERRUPTION_RUNS_PER_APP=3");
    println!("TASK3536_INTERRUPTED_MARK_COUNT={interrupted_mark_count}");
    println!("TASK3536_OTHER_CONTROL_MARK_COUNT={other_control_mark_count}");

    assert_eq!(SUPPORTED_OUTSIDE_APPS.len(), 8);
    assert_eq!(interrupted_runs, SUPPORTED_OUTSIDE_APPS.len() * 3);
    assert_eq!(interrupted_mark_count, 0);
    assert_eq!(other_control_mark_count, 0);
}

#[test]
fn task_3536_windows_job_samples_focus_cover_and_minimise_at_the_paste_boundary() {
    // Keep the live Windows command wired to the guarded path; otherwise the
    // simulated interruption matrix above could pass while the actual paste
    // route retained its old unguarded call.
    assert!(SHARED_PLACE_JOB.contains("WindowsPlacementWindowGuard"));
    assert!(SHARED_PLACE_JOB.contains("super::place_read_back_and_clear_guarded"));
    assert!(SHARED_PLACE_JOB.contains("GetForegroundWindow"));
    assert!(SHARED_PLACE_JOB.contains("WindowFromPoint"));
    assert!(SHARED_PLACE_JOB.contains("IsIconic"));
    assert!(SHARED_PLACE_JOB.contains("placement refused: {} before text was put down"));
    println!("TASK3536_WINDOWS_PASTE_BOUNDARY_GUARD=focus,covered,minimized");
}
