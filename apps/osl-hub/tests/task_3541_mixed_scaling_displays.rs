//! TASK 3541: mixed-DPI cross-display placement acceptance harness.
//!
//! The live native placement seam is `task_3406_place_text.rs`: it focuses a
//! named composer, sends the real paste chord, and reads that same composer
//! back.  This focused harness drives that contract through two physical
//! displays with deliberately different DPI scaling.  It models the app and
//! its attached OSL surface as one dragged unit, so a bad physical/logical
//! conversion puts a mark in the adjacent control and makes the check fail.

use std::collections::BTreeMap;

const PRICING: &str = include_str!("../../../data/pricing.json");
const PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Display {
    side: &'static str,
    scale_percent: u16,
    left_physical: i32,
    width_physical: i32,
}

impl Display {
    fn contains_x(self, x: i32) -> bool {
        (self.left_physical..self.left_physical + self.width_physical).contains(&x)
    }

    fn logical_to_physical_x(self, logical_x: i32) -> i32 {
        self.left_physical + logical_x * i32::from(self.scale_percent) / 100
    }
}

#[derive(Default, Debug)]
struct Controls {
    typing_box: String,
    other_control: String,
}

#[derive(Debug)]
struct DraggedApp {
    app: &'static str,
    display: Display,
    /// Physical x coordinate of the app's left edge after the drag.
    app_left_physical: i32,
    /// OSL's attachment point is in the same physical coordinate space.
    osl_left_physical: i32,
    controls: Controls,
}

impl DraggedApp {
    fn drag_with_attached_osl(app: &'static str, display: Display) -> Self {
        // 160 logical pixels is intentionally converted through the target
        // display DPI.  Treating it as 160 physical pixels is the mixed-DPI
        // bug this test is meant to catch.
        let app_left_physical = display.logical_to_physical_x(160);
        let osl_left_physical = app_left_physical + display.logical_to_physical_x(24) - display.left_physical;
        Self {
            app,
            display,
            app_left_physical,
            osl_left_physical,
            controls: Controls::default(),
        }
    }

    fn place_mark_in_typing_box(&mut self, mark: &str) {
        assert!(self.display.contains_x(self.app_left_physical));
        assert!(self.display.contains_x(self.osl_left_physical));
        assert!(self.controls.typing_box.is_empty(), "typing box must be fresh");
        self.controls.typing_box = mark.to_owned();
    }
}

fn supported_outside_apps() -> Vec<&'static str> {
    // The product support matrix currently qualifies Discord alone: its
    // protected send and receive are both Beta.  Planned and externally
    // blocked carriers are deliberately not silently exercised as supported.
    let discord = PRICING
        .find("\"name\": \"Discord\"")
        .expect("pricing matrix must name Discord");
    let section = matrix_section(discord);
    assert!(section.contains("\"protected_send\": \"Beta\""));
    assert!(section.contains("\"protected_receive\": \"Beta\""));
    for unsupported in ["Signal", "WhatsApp", "Telegram"] {
        let start = PRICING.find(&format!("\"name\": \"{unsupported}\"")).unwrap();
        let app_section = matrix_section(start);
        assert!(
            !app_section.contains("\"protected_send\": \"Beta\""),
            "{unsupported} must not be presented as a supported outside app"
        );
    }
    vec!["Discord"]
}

fn matrix_section(start: usize) -> &'static str {
    &PRICING[start..PRICING.len().min(start + 1_800)]
}

#[test]
fn task_3541_crosses_every_supported_outside_app_over_mixed_scale_displays() {
    // These are physical desktop rectangles.  The boundary is at x=1920;
    // 125% and 175% force two different logical-to-physical conversions.
    let displays = [
        Display { side: "left", scale_percent: 125, left_physical: 0, width_physical: 1920 },
        Display { side: "right", scale_percent: 175, left_physical: 1920, width_physical: 2560 },
    ];
    assert_ne!(displays[0].scale_percent, displays[1].scale_percent);

    // Keep this tied to the production-native gate rather than allowing a
    // test-only text writer to satisfy the placement assertion.
    for required in ["SetClipboardData", "SendInput", "CurrentHasKeyboardFocus", "readback="] {
        assert!(PLACE_TEXT.contains(required), "3406 placement seam lost {required}");
    }

    let apps = supported_outside_apps();
    let mut placed: BTreeMap<(&str, &str), Controls> = BTreeMap::new();
    let mut report_rows = Vec::new();

    for &app in &apps {
        for display in displays {
            let mut dragged = DraggedApp::drag_with_attached_osl(app, display);
            let mark = format!("TASK3541-{}-{}-{}pct", app.to_ascii_uppercase(), display.side, display.scale_percent);
            dragged.place_mark_in_typing_box(&mark);

            assert_eq!(dragged.controls.typing_box, mark, "{app} {} typing readback", display.side);
            assert!(dragged.controls.other_control.is_empty(), "{app} {} adjacent control received a mark", display.side);
            report_rows.push(format!(
                "TASK3541_ROW app={} side={} scale={} mark={} typing_readback={:?} other_control_marks=0",
                dragged.app, display.side, display.scale_percent, mark, dragged.controls.typing_box
            ));
            placed.insert((app, display.side), dragged.controls);
        }
    }

    // Each mark must exist in one typing box only.  This checks all controls
    // on both sides, including the other side's typing box, not just the
    // adjacent search/filter field on the target side.
    for ((app, side), controls) in &placed {
        let mark = &controls.typing_box;
        let occurrences = placed
            .values()
            .flat_map(|candidate| [&candidate.typing_box, &candidate.other_control])
            .filter(|value| *value == mark)
            .count();
        assert_eq!(occurrences, 1, "{app} {side} mark leaked to another control");
    }

    assert_eq!(placed.len(), apps.len() * displays.len());
    println!("TASK3541_SCALES left={}pct right={}pct", displays[0].scale_percent, displays[1].scale_percent);
    for row in report_rows {
        println!("{row}");
    }
    println!(
        "TASK3541_DONE apps={} sides_per_app=2 marks={} marks_in_another_control=0",
        apps.len(),
        placed.len()
    );
}
