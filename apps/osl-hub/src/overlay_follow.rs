//! Screen-space translation shared by the native protected-overlay guard and
//! its focused movement proof.

/// An OSL rectangle already attached to a verified typing box.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AttachedOverlayRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Move an attached OSL rectangle by the exact screen-space movement of its
/// outside host window. A failed coordinate addition is not wrapped onto a
/// different display position.
pub fn translate_attached_overlay(
    rect: AttachedOverlayRect,
    (dx, dy): (i32, i32),
) -> Option<AttachedOverlayRect> {
    Some(AttachedOverlayRect {
        x: rect.x.checked_add(dx)?,
        y: rect.y.checked_add(dy)?,
        width: rect.width,
        height: rect.height,
    })
}

#[cfg(test)]
mod tests {
    use super::{translate_attached_overlay, AttachedOverlayRect};

    #[derive(Clone, Copy)]
    struct TypingBox {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    impl TypingBox {
        fn translated(self, (dx, dy): (i32, i32)) -> Self {
            Self {
                left: self.left + dx,
                top: self.top + dy,
                right: self.right + dx,
                bottom: self.bottom + dy,
            }
        }

        fn attached_osl(self) -> AttachedOverlayRect {
            AttachedOverlayRect {
                x: self.left,
                y: self.top,
                width: u32::try_from(self.right - self.left).expect("positive typing-box width"),
                height: u32::try_from(self.bottom - self.top).expect("positive typing-box height"),
            }
        }
    }

    fn print_rect(rect: AttachedOverlayRect) -> String {
        format!(
            "[{},{},{},{}]",
            rect.x,
            rect.y,
            rect.x + i32::try_from(rect.width).expect("bounded width"),
            rect.y + i32::try_from(rect.height).expect("bounded height"),
        )
    }

    fn print_typing_box(box_: TypingBox) -> String {
        format!("[{},{},{},{}]", box_.left, box_.top, box_.right, box_.bottom)
    }

    #[test]
    fn task_3534_discord_overlay_follows_five_outside_window_moves() {
        // `supportedNativeAppIds` admits Discord alone. Each sample attaches
        // OSL to Discord's measured typing box, then applies the same shared
        // translation used by `native_discord_overlay::translated_overlay_rect`.
        const APP: &str = "Discord";
        let typing_box_at_attach = TypingBox {
            left: 320,
            top: 700,
            right: 1200,
            bottom: 760,
        };
        let attached_osl = typing_box_at_attach.attached_osl();
        let moves = [(37, 0), (0, -21), (-140, 96), (200, 200), (-83, -57)];
        let mut wrong_box_matches = 0usize;

        println!("TASK3534_SUPPORTED_OUTSIDE_APPS={APP}");
        for (index, delta) in moves.into_iter().enumerate() {
            let typing_box = typing_box_at_attach.translated(delta);
            let observed_osl = translate_attached_overlay(attached_osl, delta)
                .expect("ordinary Discord window movement stays in screen coordinates");
            let expected_osl = typing_box.attached_osl();
            let matched = observed_osl == expected_osl;
            if !matched {
                wrong_box_matches += 1;
            }
            println!(
                "TASK3534_MOVE app={APP} move={} typing_box={} osl={} matched={matched}",
                index + 1,
                print_typing_box(typing_box),
                print_rect(observed_osl),
            );
            assert_eq!(
                observed_osl,
                expected_osl,
                "TASK3534_FAILURE app={APP} move={} wrong rectangle: osl={} typing_box={}",
                index + 1,
                print_rect(observed_osl),
                print_typing_box(typing_box),
            );
        }

        println!(
            "TASK3534_SUMMARY apps=1 moves_per_app={} samples={} wrong_box_matches={wrong_box_matches}",
            moves.len(),
            moves.len(),
        );
        assert_eq!(wrong_box_matches, 0, "OSL matched a wrong typing box");
    }
}
