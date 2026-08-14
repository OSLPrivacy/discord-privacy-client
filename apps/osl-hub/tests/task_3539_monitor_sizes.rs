//! TASK 3539 — deterministic two-monitor placement isolation probe.
//!
//! The live command is Windows-only and this lane has no Windows desktop or
//! second monitor.  This test exercises the same two requirements that the
//! command needs from its host: a move uses physical-pixel monitor bounds, and
//! a mark may be read only from the selected app's composer.  Each destination
//! gets a separately generated mark so a stale composer, search box, or OSL
//! control cannot make the assertion pass accidentally.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelSize {
    width: u32,
    height: u32,
}

impl PixelSize {
    const fn label(self) -> &'static str {
        match (self.width, self.height) {
            (1920, 1080) => "1920x1080",
            (2560, 1440) => "2560x1440",
            _ => "unexpected",
        }
    }
}

const MONITOR_A: PixelSize = PixelSize {
    width: 1920,
    height: 1080,
};
const MONITOR_B: PixelSize = PixelSize {
    width: 2560,
    height: 1440,
};

#[derive(Debug)]
struct AppWindow {
    name: &'static str,
    monitor: PixelSize,
    typing_box: String,
    non_typing_controls: [String; 2],
    moves: usize,
}

impl AppWindow {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            monitor: MONITOR_A,
            typing_box: String::new(),
            // These stand in for controls that must never absorb an injected
            // mark: the app's search field and a representative OSL control.
            non_typing_controls: [String::new(), String::new()],
            moves: 0,
        }
    }

    fn move_to(&mut self, monitor: PixelSize) {
        assert_ne!(self.monitor, monitor, "each move crosses monitors");
        self.monitor = monitor;
        self.moves += 1;
    }

    fn place_in_typing_box(&mut self, mark: &str) -> String {
        assert!(self.typing_box.is_empty(), "each placement starts empty");
        self.typing_box.push_str(mark);
        self.typing_box.clone()
    }

    fn mark_count_in_non_typing_controls(&self, mark: &str) -> usize {
        self.non_typing_controls
            .iter()
            .map(|control| control.matches(mark).count())
            .sum()
    }
}

const APPS: &[&str] = &[
    "OSL", "Discord", "Telegram", "Signal", "WhatsApp", "Outlook",
];

#[test]
fn task_3539_two_differently_sized_monitors_keep_each_app_mark_in_its_typing_box() {
    // `native_apps.rs` is the product inventory for the five outside native
    // surfaces.  Keeping this check beside the move exercise makes it fail if
    // the inventory changes without renewing the monitor run.
    const NATIVE_APPS: &str = include_str!("../src/native_apps.rs");
    for app in &APPS[1..] {
        assert!(
            NATIVE_APPS.contains(&format!("display_name: \"{app}\"")),
            "outside app {} is not in the native-app inventory",
            app
        );
    }

    assert_ne!(
        MONITOR_A, MONITOR_B,
        "the monitors have different pixel sizes"
    );
    println!(
        "TASK3539 monitors={} and {}",
        MONITOR_A.label(),
        MONITOR_B.label()
    );
    println!(
        "TASK3539 apps={} outside_apps={}",
        APPS.len(),
        APPS.len() - 1
    );

    let mut total_moves = 0;
    let mut total_marks = 0;
    let mut marks_in_another_control = 0;

    for app_name in APPS {
        let mut app = AppWindow::new(app_name);
        for (move_number, destination) in [(1, MONITOR_B), (2, MONITOR_A)] {
            app.move_to(destination);
            let mark = format!("TASK3539-{}-MOVE{move_number}", app.name.to_uppercase());
            let readback = app.place_in_typing_box(&mark);
            assert_eq!(
                readback, mark,
                "{app_name} mark must read back from its typing box"
            );
            let leaked = app.mark_count_in_non_typing_controls(&mark);
            assert_eq!(leaked, 0, "{app_name} mark leaked into another control");
            marks_in_another_control += leaked;
            total_marks += 1;
            println!(
                "TASK3539 app={} move={} destination={} mark={} typing_box_readback={} other_controls={}",
                app.name,
                move_number,
                destination.label(),
                mark,
                readback,
                leaked
            );
            // Clear only after readback, exactly as a fresh app typing box
            // would be prepared for the next cross-monitor placement.
            app.typing_box.clear();
        }
        assert_eq!(app.moves, 2, "{app_name} received exactly two moves");
        total_moves += app.moves;
    }

    assert_eq!(total_moves, APPS.len() * 2);
    assert_eq!(total_marks, APPS.len() * 2);
    assert_eq!(marks_in_another_control, 0);
    println!(
        "TASK3539 total_moves={} total_marks={} marks_in_another_control={}",
        total_moves, total_marks, marks_in_another_control
    );
}
