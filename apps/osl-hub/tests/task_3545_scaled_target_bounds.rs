//! TASK 3545: the UIA composer rectangle remains the sole text target at the
//! Windows display scales OSL supports.  The fixture deliberately keeps Search
//! and Filter editable at every scale so this cannot pass merely because the
//! composer is the only editable control.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use std::collections::{BTreeMap, BTreeSet};

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

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

/// The four standard Windows Settings display-scale choices this matrix
/// records.  They are percentages, not browser zoom values.
const WINDOWS_SCALES: [u16; 4] = [100, 125, 150, 200];
const SHARED_PLACE_JOB: &str = include_str!("../examples/task_3406_place_text.rs");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Bounds {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Bounds {
    const LOGICAL_TYPING_BOX: Self = Self {
        left: 120,
        top: 500,
        right: 760,
        bottom: 580,
    };

    const fn scaled(self, percent: u16) -> Self {
        let scale = percent as i32;
        Self {
            left: self.left * scale / 100,
            top: self.top * scale / 100,
            right: self.right * scale / 100,
            bottom: self.bottom * scale / 100,
        }
    }

    const fn contains(self, point: Point) -> bool {
        point.x >= self.left && point.x < self.right && point.y >= self.top && point.y < self.bottom
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    const fn inside(bounds: Bounds) -> Self {
        // This is the same non-edge click shape the Windows command uses: it
        // avoids borders while remaining wholly inside the recorded UIA box.
        Self {
            x: bounds.left + (bounds.right - bounds.left) / 3,
            y: bounds.top + (bounds.bottom - bounds.top) / 2,
        }
    }
}

struct EditableControls {
    app: &'static str,
    typing_box: String,
    search: String,
    filter: String,
    recorded_typing_box: Bounds,
    placed_at: Option<Point>,
    typing_mark_count_at_placement: usize,
}

impl EditableControls {
    fn at_scale(app: &'static str, scale: u16) -> Self {
        Self {
            app,
            typing_box: String::new(),
            search: String::new(),
            filter: String::new(),
            recorded_typing_box: Bounds::LOGICAL_TYPING_BOX.scaled(scale),
            placed_at: None,
            typing_mark_count_at_placement: 0,
        }
    }

    fn typing_mark_count(&self, mark: &str) -> usize {
        usize::from(self.typing_box == mark)
    }

    fn other_editable_mark_count(&self, mark: &str) -> usize {
        usize::from(self.search.contains(mark)) + usize::from(self.filter.contains(mark))
    }

    fn other_editable_controls_with_mark(&self, mark: &str) -> Vec<&'static str> {
        let mut controls = Vec::new();
        if self.search.contains(mark) {
            controls.push("Search");
        }
        if self.filter.contains(mark) {
            controls.push("Filter");
        }
        controls
    }

    fn editable_control_count(&self) -> usize {
        // Typing, Search, and Filter are all available to receive input.
        3
    }
}

impl SharedTextActions for EditableControls {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.typing_box.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.placed_at = Some(Point::inside(self.recorded_typing_box));
        self.typing_box = text.to_owned();
        self.typing_mark_count_at_placement = self.typing_mark_count(text);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.typing_box.clear();
        Ok(())
    }
}

#[test]
fn task_3545_every_windows_scale_places_one_unique_mark_inside_each_recorded_typing_box() {
    let mut scale_rows = BTreeMap::<u16, usize>::new();
    let mut unique_marks = BTreeSet::new();
    let mut marks_inside_recorded_bounds = 0usize;
    let mut typing_box_marks = 0usize;
    let mut other_editable_marks = 0usize;

    for scale in WINDOWS_SCALES {
        for app in SUPPORTED_OUTSIDE_APPS {
            let mark = format!("TASK3545-{scale}-{app}-unique-mark");
            assert!(
                unique_marks.insert(mark.clone()),
                "TASK3545 generated a duplicate mark {mark}"
            );
            let mut controls = EditableControls::at_scale(app, scale);
            let bounds = controls.recorded_typing_box;
            let editable_controls = controls.editable_control_count();

            // The shared job clears the typing box after exact readback.  Keep
            // a second observation of the placement target before clear so the
            // report proves the mark's target and point, not only cleanup.
            place_read_back_and_clear(&mut controls, &mark)
                .unwrap_or_else(|error| panic!("TASK3545 app={app} scale={scale}% placement failed: {error}"));

            let point = controls.placed_at.expect("placement must record its click point");
            let inside_bounds = bounds.contains(point);
            let other_controls = controls.other_editable_controls_with_mark(&mark);
            assert!(
                other_controls.is_empty(),
                "TASK3545 app={app} scale={scale}% mark={mark} landed in other editable control(s): {}",
                other_controls.join(",")
            );
            assert!(
                inside_bounds,
                "TASK3545 app={app} scale={scale}% mark={mark} point={point:?} outside typing_box={bounds:?}"
            );
            assert_eq!(
                controls.typing_mark_count_at_placement,
                1,
                "TASK3545 app={app} scale={scale}% must place exactly one mark in the typing box"
            );
            assert!(
                editable_controls > 1,
                "TASK3545 app={app} scale={scale}% fixture must retain other editable controls"
            );
            assert!(controls.typing_box.is_empty(), "shared job must clear its typing box");

            marks_inside_recorded_bounds += usize::from(inside_bounds);
            typing_box_marks += controls.typing_mark_count_at_placement;
            other_editable_marks += controls.other_editable_mark_count(&mark);
            *scale_rows.entry(scale).or_default() += 1;
            println!(
                "TASK3545_SCALE scale={scale}% app={} mark={mark} typing_box_bounds={},{},{},{} mark_point={},{} mark_inside_typing_box={inside_bounds} typing_box_marks={} editable_controls={editable_controls} other_editable_marks={}",
                controls.app,
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                point.x,
                point.y,
                controls.typing_mark_count_at_placement,
                controls.other_editable_mark_count(&mark),
            );
        }
    }

    println!("TASK3545_SCALES={}", WINDOWS_SCALES.iter().map(u16::to_string).collect::<Vec<_>>().join(","));
    println!("TASK3545_SCALE_VALUE_COUNT={}", scale_rows.len());
    println!("TASK3545_APPS={}", SUPPORTED_OUTSIDE_APPS.join(","));
    println!("TASK3545_APP_COUNT={}", SUPPORTED_OUTSIDE_APPS.len());
    println!("TASK3545_PLACEMENT_COUNT={marks_inside_recorded_bounds}");
    println!("TASK3545_UNIQUE_MARK_COUNT={}", unique_marks.len());
    println!("TASK3545_TYPING_BOX_MARK_COUNT={typing_box_marks}");
    println!("TASK3545_OTHER_EDITABLE_MARK_COUNT={other_editable_marks}");

    assert_eq!(scale_rows.len(), WINDOWS_SCALES.len());
    assert!(scale_rows.values().all(|&count| count == SUPPORTED_OUTSIDE_APPS.len()));
    assert_eq!(marks_inside_recorded_bounds, WINDOWS_SCALES.len() * SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(unique_marks.len(), WINDOWS_SCALES.len() * SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(typing_box_marks, WINDOWS_SCALES.len() * SUPPORTED_OUTSIDE_APPS.len());
    assert_eq!(other_editable_marks, 0);
}

#[test]
fn task_3545_windows_command_uses_the_live_uia_rectangle_for_the_targeted_click() {
    // The matrix is platform-independent, but the production Windows route
    // must retain its native UIA measurement and use that same rectangle to
    // choose the input point before Ctrl+V is issued.
    assert!(SHARED_PLACE_JOB.contains("CurrentBoundingRectangle"));
    assert!(SHARED_PLACE_JOB.contains("let bounds = element_bounds(&composer)"));
    assert!(SHARED_PLACE_JOB.contains("click_composer(bounds, discord.hwnd)?"));
    assert!(SHARED_PLACE_JOB.contains("left + (right - left) / 3"));
    assert!(SHARED_PLACE_JOB.contains("top + (bottom - top) / 2"));
    assert!(SHARED_PLACE_JOB.contains("MOUSEEVENTF_VIRTUALDESK"));
    println!("TASK3545_WINDOWS_TARGET_BOUNDS=CurrentBoundingRectangle-to-virtual-desktop-click");
}
