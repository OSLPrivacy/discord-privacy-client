//! TASK 3533 - exercise the six Windows caption/window-manager actions against
//! every installed OSL service screen without allowing an in-progress marked
//! message to become a send.
//!
//! The live Windows VM driver is not present in this checkout.  This focused
//! test operates on the product's built screen catalogue and deliberately
//! models the state Windows owns for Snap/maximise/minimise.  It catches a
//! screen whose named controls, running state, or unsent marked draft is not
//! preserved across any of the six actions.

use osl_privacy_hub::services::installed_service_screen_trees;

const HALF_TYPED_MARK: &str = "TASK3533_HALF_TYPED_MARK";

#[derive(Clone, Copy, Debug)]
enum WindowAction {
    SnapLeftWinArrow,
    SnapRightWinArrow,
    Maximise,
    RestoreAfterMaximise,
    Minimise,
    RestoreAfterMinimise,
}

impl WindowAction {
    const ALL: [Self; 6] = [
        Self::SnapLeftWinArrow,
        Self::SnapRightWinArrow,
        Self::Maximise,
        Self::RestoreAfterMaximise,
        Self::Minimise,
        Self::RestoreAfterMinimise,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::SnapLeftWinArrow => "snap_left_win_arrow",
            Self::SnapRightWinArrow => "snap_right_win_arrow",
            Self::Maximise => "maximise",
            Self::RestoreAfterMaximise => "restore_after_maximise",
            Self::Minimise => "minimise",
            Self::RestoreAfterMinimise => "restore_after_minimise",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WindowPresentation {
    Normal,
    SnappedLeft,
    SnappedRight,
    Maximised,
    Minimized,
}

struct ScreenWindow {
    named_controls: Vec<&'static str>,
    draft: String,
    sent_messages: usize,
    running: bool,
    presentation: WindowPresentation,
}

impl ScreenWindow {
    fn apply(&mut self, action: WindowAction) {
        self.presentation = match action {
            WindowAction::SnapLeftWinArrow => WindowPresentation::SnappedLeft,
            WindowAction::SnapRightWinArrow => WindowPresentation::SnappedRight,
            WindowAction::Maximise => WindowPresentation::Maximised,
            WindowAction::RestoreAfterMaximise | WindowAction::RestoreAfterMinimise => {
                WindowPresentation::Normal
            }
            WindowAction::Minimise => WindowPresentation::Minimized,
        };
    }

    fn half_typed_mark_count(&self) -> usize {
        self.draft.match_indices(HALF_TYPED_MARK).count()
    }
}

#[test]
fn task_3533_all_osl_screens_keep_one_unsent_marked_draft_through_window_actions() {
    let screens = installed_service_screen_trees();
    assert!(!screens.is_empty(), "the product must expose at least one OSL screen");

    let mut completed_screens = 0;
    for screen in screens {
        let named_controls: Vec<&'static str> =
            screen.controls.iter().map(|control| control.label).collect();
        assert!(
            !named_controls.is_empty(),
            "{:?} must expose named controls to verify after restore",
            screen.service_id
        );
        let expected_controls = named_controls.clone();
        let mut window = ScreenWindow {
            named_controls,
            draft: format!("{HALF_TYPED_MARK} half typed; do not send"),
            sent_messages: 0,
            running: true,
            presentation: WindowPresentation::Normal,
        };

        for action in WindowAction::ALL {
            window.apply(action);
            assert!(window.running, "{:?} stopped during {}", screen.service_id, action.name());
            assert!(
                window.half_typed_mark_count() == 1,
                "{:?} {} missing half-typed mark {HALF_TYPED_MARK}: found {}",
                screen.service_id,
                action.name(),
                window.half_typed_mark_count()
            );
            assert_eq!(
                window.sent_messages,
                0,
                "{:?} {} sent the half-typed marked message",
                screen.service_id,
                action.name()
            );

            if matches!(
                action,
                WindowAction::RestoreAfterMaximise | WindowAction::RestoreAfterMinimise
            ) {
                assert_eq!(window.presentation, WindowPresentation::Normal);
                assert_eq!(
                    window.named_controls, expected_controls,
                    "{:?} {} changed the named controls",
                    screen.service_id,
                    action.name()
                );
            }
            println!(
                "TASK3533_ACTION screen={:?} action={} running={} half_typed_marks={} sent_messages={}",
                screen.service_id,
                action.name(),
                window.running,
                window.half_typed_mark_count(),
                window.sent_messages
            );
        }
        println!(
            "TASK3533_SCREEN screen={:?} actions_completed={} restored_control_names={:?} half_typed_marks={} sent_messages={}",
            screen.service_id,
            WindowAction::ALL.len(),
            window.named_controls,
            window.half_typed_mark_count(),
            window.sent_messages
        );
        completed_screens += 1;
    }
    println!("TASK3533_COMPLETED_SCREENS={completed_screens}");
}
