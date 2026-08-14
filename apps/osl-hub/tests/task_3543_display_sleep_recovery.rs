#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

// Keep the recovery run bound to the closed outside-app roster used by the
// shared 3406 placement gates.  A new supported surface must add a row here
// before this sleep/recovery check can claim coverage for it.
const OUTSIDE_APPS: &[&str] = &["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisplayState {
    Awake,
    Sleeping,
}

/// Deterministic stand-in for one OSL-attached outside-app typing surface.
///
/// The Linux build lane cannot put a real Windows display to sleep.  This
/// surface therefore executes the same provider-neutral 3406 placement
/// transaction while making display wake-up and OSL process state observable:
/// placement is refused until the display has resumed, the only writable
/// control is `typing_box`, and a stopped OSL process is never accepted.
struct DisplaySleepSurface {
    app: &'static str,
    display: DisplayState,
    osl_running: bool,
    attached: bool,
    typing_box: String,
    other_controls: [String; 2],
    place_calls: usize,
    resumed: bool,
}

impl DisplaySleepSurface {
    fn open(app: &'static str) -> Self {
        Self {
            app,
            display: DisplayState::Awake,
            osl_running: true,
            attached: true,
            typing_box: String::new(),
            other_controls: [String::new(), String::new()],
            place_calls: 0,
            resumed: false,
        }
    }

    fn sleep_display(&mut self) {
        self.display = DisplayState::Sleeping;
    }

    fn resume_display(&mut self) {
        assert_eq!(
            self.display,
            DisplayState::Sleeping,
            "{} was not asleep",
            self.app
        );
        self.display = DisplayState::Awake;
        self.resumed = true;
    }

    fn all_mark_count(&self, mark: &str) -> usize {
        usize::from(self.typing_box == mark)
            + self
                .other_controls
                .iter()
                .filter(|value| value.as_str() == mark)
                .count()
    }

    fn other_control_mark_count(&self, mark: &str) -> usize {
        self.other_controls
            .iter()
            .filter(|value| value.as_str() == mark)
            .count()
    }

    fn ready_for_placement(&self) -> Result<(), String> {
        if !self.osl_running {
            return Err(format!(
                "{} refused before placement: OSL process stopped",
                self.app
            ));
        }
        if !self.attached {
            return Err(format!(
                "{} refused before placement: OSL is not attached",
                self.app
            ));
        }
        if self.display != DisplayState::Awake || !self.resumed {
            return Err(format!(
                "{} refused before placement: display has not resumed",
                self.app
            ));
        }
        Ok(())
    }
}

impl SharedTextActions for DisplaySleepSurface {
    fn read_back_text(&mut self) -> Result<String, String> {
        self.ready_for_placement()?;
        Ok(self.typing_box.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.ready_for_placement()?;
        self.place_calls += 1;
        self.typing_box.clear();
        self.typing_box.push_str(text);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.ready_for_placement()?;
        self.typing_box.clear();
        Ok(())
    }
}

#[test]
fn task_3543_display_sleep_recovery_places_one_mark_only_after_resume_for_every_app() {
    let mut control_exact_typing_box_marks = 0usize;
    let mut pre_resume_marks = 0usize;
    let mut post_resume_exact_typing_box_marks = 0usize;
    let mut post_resume_refusals = 0usize;
    let mut other_control_marks = 0usize;
    let mut stopped_osl_processes = 0usize;

    for &app in OUTSIDE_APPS {
        // Control: one ordinary 3406 placement in an already-awake surface.
        // Marking `resumed` true models the established awake session, not a
        // display wake-up; the recovery row below independently exercises it.
        let control_mark = format!("OSL-3543-control-{app}");
        let mut control = DisplaySleepSurface::open(app);
        control.resumed = true;
        let control_receipt = place_read_back_and_clear(&mut control, &control_mark)
            .unwrap_or_else(|error| panic!("{app} control: {error}"));
        assert_eq!(
            control_receipt.placed_bytes,
            control_mark.len(),
            "{app} control"
        );
        assert_eq!(
            control_receipt.readback_bytes,
            control_mark.len(),
            "{app} control"
        );
        assert_eq!(control_receipt.clear_bytes, 0, "{app} control");
        assert_eq!(control.place_calls, 1, "{app} control");
        assert_eq!(
            control.other_control_mark_count(&control_mark),
            0,
            "{app} control"
        );
        assert!(control.osl_running, "{app} control stopped OSL");
        control_exact_typing_box_marks += 1;
        println!(
            "TASK3543 app={app} phase=control outcome=exact-typing-box-mark marks_in_typing_box=1 other_control_marks=0"
        );

        // Recovery: sleep while attached, prove no mark exists before wake-up,
        // then make exactly one marked placement after the display resumes.
        let recovery_mark = format!("OSL-3543-resume-{app}");
        let mut recovery = DisplaySleepSurface::open(app);
        recovery.sleep_display();
        let marks_before_resume = recovery.all_mark_count(&recovery_mark);
        assert_eq!(
            marks_before_resume, 0,
            "{app} has a mark before display resume"
        );
        assert_eq!(
            recovery.place_calls, 0,
            "{app} placed before display resume"
        );
        pre_resume_marks += marks_before_resume;

        recovery.resume_display();
        match place_read_back_and_clear(&mut recovery, &recovery_mark) {
            Ok(receipt) => {
                assert_eq!(receipt.placed_bytes, recovery_mark.len(), "{app} recovery");
                assert_eq!(
                    receipt.readback_bytes,
                    recovery_mark.len(),
                    "{app} recovery"
                );
                assert_eq!(receipt.clear_bytes, 0, "{app} recovery");
                assert_eq!(recovery.place_calls, 1, "{app} recovery");
                assert_eq!(
                    recovery.other_control_mark_count(&recovery_mark),
                    0,
                    "{app} recovery"
                );
                post_resume_exact_typing_box_marks += 1;
                println!(
                    "TASK3543 app={app} phase=post-resume outcome=exact-typing-box-mark marks_before_resume={marks_before_resume} other_control_marks=0"
                );
            }
            Err(error) => {
                assert!(error.contains("refused before placement"), "{app}: {error}");
                assert_eq!(
                    recovery.place_calls, 0,
                    "{app} refusal must precede placement"
                );
                assert_eq!(
                    recovery.all_mark_count(&recovery_mark),
                    0,
                    "{app} refusal placed a mark"
                );
                post_resume_refusals += 1;
                println!(
                    "TASK3543 app={app} phase=post-resume outcome=refused-before-placement marks_before_resume={marks_before_resume} other_control_marks=0"
                );
            }
        }
        other_control_marks += recovery.other_control_mark_count(&recovery_mark);
        stopped_osl_processes +=
            usize::from(!control.osl_running) + usize::from(!recovery.osl_running);
    }

    assert_eq!(
        OUTSIDE_APPS.len(),
        5,
        "the supported outside-app roster changed"
    );
    assert_eq!(control_exact_typing_box_marks, OUTSIDE_APPS.len());
    assert_eq!(pre_resume_marks, 0);
    assert_eq!(
        post_resume_exact_typing_box_marks + post_resume_refusals,
        OUTSIDE_APPS.len()
    );
    assert_eq!(other_control_marks, 0);
    assert_eq!(stopped_osl_processes, 0);
    println!(
        "TASK3543_SUMMARY apps={} control_exact_typing_box_marks={control_exact_typing_box_marks} pre_resume_marks={pre_resume_marks} post_resume_exact_typing_box_marks={post_resume_exact_typing_box_marks} post_resume_refusals={post_resume_refusals} other_control_marks={other_control_marks} stopped_osl_processes={stopped_osl_processes}",
        OUTSIDE_APPS.len()
    );
}

#[test]
fn task_3543_refuses_a_mark_before_resume_without_writing_any_control() {
    let mark = "OSL-3543-before-resume-refusal";
    let mut surface = DisplaySleepSurface::open("Discord");
    surface.sleep_display();

    let error = place_read_back_and_clear(&mut surface, mark)
        .expect_err("a sleeping display must refuse before placement");
    assert!(error.contains("refused before placement"), "{error}");
    assert_eq!(surface.place_calls, 0);
    assert_eq!(surface.all_mark_count(mark), 0);
    assert_eq!(surface.other_control_mark_count(mark), 0);
    assert!(surface.osl_running);
    println!(
        "TASK3543_PRE_RESUME_REFUSAL app=Discord outcome=refused-before-placement marks=0 other_control_marks=0 stopped_osl_processes=0"
    );
}
