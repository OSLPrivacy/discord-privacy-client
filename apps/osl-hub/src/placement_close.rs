//! Final fail-closed decision for a marked native placement.
//!
//! The outside application can disappear after OSL has opened its overlay but
//! before the carrier writer commits its marked text.  A closed overlay is the
//! locally observable consequence of that disappearance; it must veto the
//! commit rather than leave a mark in a dead application's clipboard path.

/// Fixed refusal used when an outside application closes during a marked
/// placement.  It intentionally names the user-visible state, not an HWND or
/// process identifier which may already have been recycled by Windows.
pub const OUTSIDE_APP_CLOSED_DURING_PLACEMENT: &str =
    "The native Discord overlay closed before marked placement";

/// Allow the final marked-placement commit only while the overlay session that
/// opened it is still ready.  Call this immediately before the carrier writer:
/// no clipboard mark or placed mark may be created after it returns `Err`.
pub fn allow_marked_placement(overlay_still_ready: bool) -> Result<(), String> {
    if overlay_still_ready {
        Ok(())
    } else {
        Err(OUTSIDE_APP_CLOSED_DURING_PLACEMENT.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{allow_marked_placement, OUTSIDE_APP_CLOSED_DURING_PLACEMENT};

    #[test]
    fn task_3537_close_discord_after_overlay_open_refuses_before_any_mark() {
        // `supportedNativeAppIds` admits Discord alone.  The placement is
        // active when Discord closes, so the final decision is exactly the
        // production decision made immediately before writing the carrier.
        const APP: &str = "Discord";
        let supported_outside_apps = include_str!("../../osl-hub-ui/src/main.ts");
        assert!(supported_outside_apps
            .contains("const supportedNativeAppIds = new Set<NativeAppId>([\"discord\"]);"));
        let mut active_placements = 0usize;
        let mut closes = 0usize;
        let mut placed_marks = Vec::<String>::new();
        let mut clipboard_marks = Vec::<String>::new();

        active_placements += 1;
        println!(
            "TASK3537_ACTIVE app={APP} active_placements={active_placements} overlay_open=true"
        );

        closes += 1;
        let overlay_still_ready = false;
        println!("TASK3537_CLOSE app={APP} closes={closes} overlay_open=false");

        let outcome = match allow_marked_placement(overlay_still_ready) {
            Ok(()) => {
                clipboard_marks.push("TASK3537-MARK".to_owned());
                placed_marks.push("TASK3537-MARK".to_owned());
                "placed"
            }
            Err(error) => {
                assert_eq!(error, OUTSIDE_APP_CLOSED_DURING_PLACEMENT);
                "refused"
            }
        };

        println!(
            "TASK3537_SUMMARY apps=1 supported_outside_apps=Discord active_placements={active_placements} closes={closes} outcome={outcome} placed_mark_count={} clipboard_marks={}",
            placed_marks.len(),
            clipboard_marks.len(),
        );
        assert_eq!(outcome, "refused");
        assert_eq!(placed_marks.len(), 0);
        assert_eq!(clipboard_marks.len(), 0);

        // Keep the executable decision connected to the native command: a
        // source-only copy after `place_carrier` would be too late, because the
        // carrier writer can already have created a mark by then.
        let source = include_str!("main.rs");
        let command_start = source
            .find("fn send_native_discord_overlay_carrier(")
            .expect("native carrier command must exist");
        let command_end = source[command_start..]
            .find("#[derive(Serialize)]")
            .map(|offset| command_start + offset)
            .expect("native carrier command must be bounded");
        let command = &source[command_start..command_end];
        let final_gate = command
            .find("carrier_placement.allow_marked_placement(epoch)?;")
            .expect("closed overlay must be refused at the final commit gate");
        let writer = command
            .find("let receipt = composer.place_carrier(")
            .expect("native carrier writer must exist");
        assert!(
            final_gate < writer,
            "TASK3537 final close gate must run before a marked carrier can be placed"
        );
        assert!(include_str!("native_discord_overlay.rs").contains(
            "osl_privacy_hub::placement_close::allow_marked_placement(self.state.is_ready(epoch))"
        ));
    }
}
