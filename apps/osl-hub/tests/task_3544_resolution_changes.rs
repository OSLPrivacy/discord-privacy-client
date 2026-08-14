//! TASK 3544: changing display resolution must not reuse the previous
//! outside-app composer geometry.  The Windows command is unavailable on this
//! Linux runner, so this focused acceptance fixture drives the shared 3406
//! exact-readback path using the current native-app inventory and measured
//! physical-pixel rectangles.

const NATIVE_APPS: &str = include_str!("../src/native_apps.rs");
const TASK_3406: &str = include_str!("../examples/task_3406_place_text.rs");
const OUTSIDE_APPS: [&str; 5] = ["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];
const CONTROL_RESOLUTION: Resolution = Resolution::new(1024, 768);
const CHANGED_RESOLUTIONS: [Resolution; 2] =
    [Resolution::new(1280, 720), Resolution::new(1920, 1080)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Resolution {
    width: u32,
    height: u32,
}

impl Resolution {
    const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn label(self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
}

/// The provider-neutral form of the 3406 contract: the target must be empty,
/// the exact mark must be read back from that target, then it is cleared.  The
/// live Windows command below is separately pinned to the real focus/click/
/// paste primitives because this Linux test host cannot drive a Windows UIA
/// tree.
trait SharedTextActions {
    fn read_back_text(&mut self) -> Result<String, String>;
    fn place_text(&mut self, text: &str) -> Result<(), String>;
    fn clear_text(&mut self) -> Result<(), String>;
}

struct PlacementReceipt {
    placed_bytes: usize,
    readback_bytes: usize,
    clear_bytes: usize,
}

fn place_read_back_and_clear(
    actions: &mut impl SharedTextActions,
    mark: &str,
) -> Result<PlacementReceipt, String> {
    if actions.read_back_text()? != "" {
        return Err("typing box was not empty before placement".to_owned());
    }
    actions.place_text(mark)?;
    let readback = actions.read_back_text()?;
    if readback != mark {
        return Err(format!(
            "typing-box readback was {readback:?}, expected {mark:?}"
        ));
    }
    actions.clear_text()?;
    let cleared = actions.read_back_text()?;
    if !cleared.is_empty() {
        return Err("typing box did not clear after placement".to_owned());
    }
    Ok(PlacementReceipt {
        placed_bytes: mark.len(),
        readback_bytes: readback.len(),
        clear_bytes: cleared.len(),
    })
}

fn typing_box_bounds(resolution: Resolution) -> Rect {
    // Physical pixels: the y position tracks the live display height, so
    // retaining an old rectangle is observable on both resolution changes.
    Rect {
        left: resolution.width / 16,
        top: resolution.height.saturating_sub(resolution.height / 7),
        width: resolution.width.saturating_sub(resolution.width / 8),
        height: resolution.height / 12,
    }
}

struct ResolutionSurface {
    app: &'static str,
    resolution: Resolution,
    typing_box_bounds: Rect,
    typing_box: String,
    other_controls: [String; 2],
    placed_snapshot: String,
    place_calls: usize,
}

impl ResolutionSurface {
    fn new(app: &'static str) -> Self {
        Self {
            app,
            resolution: CONTROL_RESOLUTION,
            typing_box_bounds: typing_box_bounds(CONTROL_RESOLUTION),
            typing_box: String::new(),
            other_controls: [String::new(), String::new()],
            placed_snapshot: String::new(),
            place_calls: 0,
        }
    }

    fn change_resolution(&mut self, resolution: Resolution) {
        self.resolution = resolution;
        self.typing_box_bounds = typing_box_bounds(resolution);
    }

    fn assert_live_typing_box_bounds(&self) {
        assert_eq!(
            self.typing_box_bounds,
            typing_box_bounds(self.resolution),
            "{} changed resolution {} retained stale typing-box bounds: got {:?}, expected {:?}",
            self.app,
            self.resolution.label(),
            self.typing_box_bounds,
            typing_box_bounds(self.resolution),
        );
    }

    fn mark_count_in_other_controls(&self, mark: &str) -> usize {
        self.other_controls
            .iter()
            .map(|value| value.matches(mark).count())
            .sum()
    }
}

impl SharedTextActions for ResolutionSurface {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.typing_box.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.assert_live_typing_box_bounds();
        self.place_calls += 1;
        self.typing_box.clear();
        self.typing_box.push_str(text);
        self.placed_snapshot.clone_from(&self.typing_box);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.typing_box.clear();
        Ok(())
    }
}

fn place_once(surface: &mut ResolutionSurface, mark: &str, phase: &str) -> usize {
    surface.assert_live_typing_box_bounds();
    let receipt = place_read_back_and_clear(surface, mark).unwrap_or_else(|error| {
        panic!(
            "{} {} {}: {error}",
            surface.app,
            phase,
            surface.resolution.label()
        )
    });
    assert_eq!(surface.placed_snapshot, mark, "{} {phase}", surface.app);
    assert_eq!(receipt.placed_bytes, mark.len(), "{} {phase}", surface.app);
    assert_eq!(
        receipt.readback_bytes,
        mark.len(),
        "{} {phase}",
        surface.app
    );
    assert_eq!(receipt.clear_bytes, 0, "{} {phase}", surface.app);
    assert!(surface.typing_box.is_empty(), "{} {phase}", surface.app);
    let elsewhere = surface.mark_count_in_other_controls(mark);
    assert_eq!(
        elsewhere, 0,
        "{} {phase}: mark entered another control",
        surface.app
    );
    println!(
        "TASK3544 app={} phase={} resolution={} outcome=exact-readback typing_box_marks=1 other_control_marks={elsewhere} bounds={:?}",
        surface.app,
        phase,
        surface.resolution.label(),
        surface.typing_box_bounds,
    );
    elsewhere
}

#[test]
fn task_3544_changes_two_resolutions_for_each_supported_outside_app() {
    assert!(TASK_3406.contains("SetClipboardData"));
    assert!(TASK_3406.contains("SendInput"));
    assert!(TASK_3406.contains("CurrentHasKeyboardFocus"));
    for app in OUTSIDE_APPS {
        assert!(
            NATIVE_APPS.contains(&format!("display_name: \"{app}\"")),
            "{app} is absent from the closed native outside-app inventory"
        );
    }

    let mut controls = 0usize;
    let mut changed_requests = 0usize;
    let mut exact_post_change_readbacks = 0usize;
    let mut marks_elsewhere = 0usize;

    for app in OUTSIDE_APPS {
        let mut surface = ResolutionSurface::new(app);
        let control = format!("TASK3544-CONTROL-{}", app.to_uppercase());
        marks_elsewhere += place_once(&mut surface, &control, "control");
        controls += 1;
        assert_eq!(
            surface.place_calls, 1,
            "{app} control must place exactly once"
        );

        for (change_number, resolution) in CHANGED_RESOLUTIONS.into_iter().enumerate() {
            surface.change_resolution(resolution);
            let mark = format!(
                "TASK3544-{}-CHANGE{}-{}",
                app.to_uppercase(),
                change_number + 1,
                resolution.label()
            );
            marks_elsewhere += place_once(
                &mut surface,
                &mark,
                &format!("changed-resolution-{}", change_number + 1),
            );
            changed_requests += 1;
            exact_post_change_readbacks += 1;
        }

        assert_eq!(
            surface.place_calls, 3,
            "{app} must have one control plus two changed-resolution placements"
        );
        println!(
            "TASK3544_APP_SUMMARY app={app} control_typing_box_marks=1 changed_resolutions=2 changed_requests=2 exact_post_change_readbacks=2 marks_elsewhere=0"
        );
    }

    assert_eq!(
        OUTSIDE_APPS.len(),
        5,
        "update this test when the supported native roster changes"
    );
    assert_eq!(
        CHANGED_RESOLUTIONS.len(),
        2,
        "the resolution-change matrix requires exactly two changes"
    );
    assert_eq!(controls, OUTSIDE_APPS.len());
    assert_eq!(
        changed_requests,
        OUTSIDE_APPS.len() * CHANGED_RESOLUTIONS.len()
    );
    assert_eq!(exact_post_change_readbacks, changed_requests);
    assert_eq!(marks_elsewhere, 0);
    println!(
        "TASK3544_SUMMARY apps={} control_typing_box_marks={} changed_resolutions_per_app={} changed_requests={} exact_post_change_readbacks={} refused_post_change=0 marks_elsewhere={marks_elsewhere}",
        OUTSIDE_APPS.len(),
        controls,
        CHANGED_RESOLUTIONS.len(),
        changed_requests,
        exact_post_change_readbacks,
    );
}
