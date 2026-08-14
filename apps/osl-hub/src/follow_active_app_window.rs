//! Policy seam for keeping the OSL window beside the foreground application.
//!
//! Platform code supplies only the foreground window identity and its measured
//! rectangle.  The choice and the move decision stay here so the off path can
//! be tested without a desktop session.

use ipc::app_preferences::FollowActiveAppChoice;

/// Space left between the foreground application's edge and OSL.
pub const FOLLOW_ACTIVE_APP_GAP_PX: i32 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

/// Remembers which foreground window has already been handled.
///
/// Clearing the remembered window while the choice is off is intentional: if
/// the owner turns following back on, the application currently in front gets
/// one fresh placement rather than being treated as a stale observation.
#[derive(Debug, Default)]
pub struct FollowActiveAppWindowMover {
    last_front_window: Option<isize>,
}

impl FollowActiveAppWindowMover {
    pub fn disable(&mut self) {
        self.last_front_window = None;
    }

    /// Return a new OSL position only for a newly foregrounded application
    /// while following is explicitly on.
    pub fn observe(
        &mut self,
        choice: FollowActiveAppChoice,
        front_window: isize,
        front_bounds: WindowRect,
        current_osl_position: WindowPosition,
    ) -> Option<WindowPosition> {
        if choice == FollowActiveAppChoice::Off {
            self.disable();
            return None;
        }

        if self.last_front_window == Some(front_window) {
            return None;
        }
        self.last_front_window = Some(front_window);

        let target = WindowPosition {
            x: front_bounds.right.saturating_add(FOLLOW_ACTIVE_APP_GAP_PX),
            y: front_bounds.top,
        };
        (target != current_osl_position).then_some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::{FollowActiveAppWindowMover, WindowPosition, WindowRect, FOLLOW_ACTIVE_APP_GAP_PX};
    use ipc::app_preferences::FollowActiveAppChoice;

    #[test]
    fn task_3149_on_moves_osl_beside_each_new_front_app() {
        let mut mover = FollowActiveAppWindowMover::default();
        let first = mover.observe(
            FollowActiveAppChoice::On,
            101,
            WindowRect::new(100, 200, 700, 900),
            WindowPosition { x: 20, y: 20 },
        );
        let second = mover.observe(
            FollowActiveAppChoice::On,
            202,
            WindowRect::new(900, 320, 1300, 800),
            first.expect("first front app moves OSL"),
        );

        println!(
            "TASK3149 choice=on first_position={:?} second_position={:?}",
            first, second
        );
        assert_eq!(
            first,
            Some(WindowPosition {
                x: 700 + FOLLOW_ACTIVE_APP_GAP_PX,
                y: 200,
            })
        );
        assert_eq!(
            second,
            Some(WindowPosition {
                x: 1300 + FOLLOW_ACTIVE_APP_GAP_PX,
                y: 320,
            })
        );
    }

    #[test]
    fn task_3149_off_never_changes_the_osl_position() {
        let mut mover = FollowActiveAppWindowMover::default();
        let initial = WindowPosition { x: 77, y: 88 };
        let first = mover.observe(
            FollowActiveAppChoice::Off,
            101,
            WindowRect::new(100, 200, 700, 900),
            initial,
        );
        let second = mover.observe(
            FollowActiveAppChoice::Off,
            202,
            WindowRect::new(900, 320, 1300, 800),
            initial,
        );

        println!(
            "TASK3149 choice=off initial_position={initial:?} move_count={}",
            usize::from(first.is_some()) + usize::from(second.is_some())
        );
        assert_eq!(first, None);
        assert_eq!(second, None);
    }
}
