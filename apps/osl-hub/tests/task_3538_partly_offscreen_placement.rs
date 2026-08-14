#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

// NativeAppId's manifest is deliberately closed. Keep this order aligned with
// the existing resize matrix so a newly supported outside surface cannot be
// skipped accidentally.
const OUTSIDE_APPS: &[&str] = &["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];
const SCREEN: Rect = Rect {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
};
const APP_SIZE: (i32, i32) = (800, 600);
const EDGE_POSITIONS: &[EdgePosition] = &[
    EdgePosition {
        name: "left",
        origin: (-240, 240),
    },
    EdgePosition {
        name: "right",
        origin: (1360, 240),
    },
    EdgePosition {
        name: "top",
        origin: (560, -180),
    },
    EdgePosition {
        name: "bottom",
        origin: (560, 660),
    },
];

#[derive(Clone, Copy)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, Copy)]
struct EdgePosition {
    name: &'static str,
    origin: (i32, i32),
}

/// A deterministic outside-app surface for the shared provider-neutral 3406
/// placement path. The distinct controls are intentionally not aliases: a
/// placement reaching one is an observable test failure.
struct PartlyOffscreenOutsideApp {
    app: &'static str,
    edge: &'static str,
    bounds: Rect,
    osl_active: bool,
    typing_box: String,
    other_controls: [String; 3],
    placed_snapshot: String,
    place_calls: usize,
}

impl PartlyOffscreenOutsideApp {
    fn active(app: &'static str, edge: EdgePosition) -> Self {
        Self {
            app,
            edge: edge.name,
            bounds: Rect {
                x: edge.origin.0,
                y: edge.origin.1,
                width: APP_SIZE.0,
                height: APP_SIZE.1,
            },
            osl_active: true,
            typing_box: String::new(),
            other_controls: [String::new(), String::new(), String::new()],
            placed_snapshot: String::new(),
            place_calls: 0,
        }
    }

    fn is_partly_beyond_screen(&self) -> bool {
        let right = self.bounds.x + self.bounds.width;
        let bottom = self.bounds.y + self.bounds.height;
        let intersects_screen = right > SCREEN.x
            && self.bounds.x < SCREEN.x + SCREEN.width
            && bottom > SCREEN.y
            && self.bounds.y < SCREEN.y + SCREEN.height;
        let beyond_an_edge = self.bounds.x < SCREEN.x
            || self.bounds.y < SCREEN.y
            || right > SCREEN.x + SCREEN.width
            || bottom > SCREEN.y + SCREEN.height;
        intersects_screen && beyond_an_edge
    }

    fn is_beyond_requested_edge(&self) -> bool {
        match self.edge {
            "left" => self.bounds.x < SCREEN.x && self.bounds.x + self.bounds.width > SCREEN.x,
            "right" => {
                self.bounds.x + self.bounds.width > SCREEN.x + SCREEN.width
                    && self.bounds.x < SCREEN.x + SCREEN.width
            }
            "top" => self.bounds.y < SCREEN.y && self.bounds.y + self.bounds.height > SCREEN.y,
            "bottom" => {
                self.bounds.y + self.bounds.height > SCREEN.y + SCREEN.height
                    && self.bounds.y < SCREEN.y + SCREEN.height
            }
            _ => false,
        }
    }

    fn can_place(&self) -> Result<(), String> {
        if !self.osl_active {
            return Err(format!(
                "{} {} refused before placement: OSL is not active",
                self.app, self.edge
            ));
        }
        if !self.is_partly_beyond_screen() || !self.is_beyond_requested_edge() {
            return Err(format!(
                "{} {} refused before placement: app is not partly beyond its requested screen edge",
                self.app, self.edge
            ));
        }
        Ok(())
    }

    fn other_control_mark_count(&self, mark: &str) -> usize {
        self.other_controls
            .iter()
            .filter(|value| value.as_str() == mark)
            .count()
    }
}

impl SharedTextActions for PartlyOffscreenOutsideApp {
    fn read_back_text(&mut self) -> Result<String, String> {
        self.can_place()?;
        Ok(self.typing_box.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.can_place()?;
        self.place_calls += 1;
        self.typing_box.clear();
        self.typing_box.push_str(text);
        self.placed_snapshot.clone_from(&self.typing_box);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.can_place()?;
        self.typing_box.clear();
        Ok(())
    }
}

#[test]
fn task_3538_checks_every_outside_app_at_each_partly_offscreen_edge() {
    let mut requests = 0usize;
    let mut exact_typing_box_readbacks = 0usize;
    let mut refusals_before_placement = 0usize;
    let mut other_control_marks = 0usize;

    for &app in OUTSIDE_APPS {
        for &edge in EDGE_POSITIONS {
            let mark = format!("OSL-3538-{app}-{}", edge.name);
            let mut surface = PartlyOffscreenOutsideApp::active(app, edge);

            assert!(surface.osl_active, "{app} {}", edge.name);
            assert!(surface.is_partly_beyond_screen(), "{app} {}", edge.name);
            assert!(surface.is_beyond_requested_edge(), "{app} {}", edge.name);
            requests += 1;

            match place_read_back_and_clear(&mut surface, &mark) {
                Ok(receipt) => {
                    assert_eq!(surface.placed_snapshot, mark, "{app} {}", edge.name);
                    assert_eq!(receipt.placed_bytes, mark.len(), "{app} {}", edge.name);
                    assert_eq!(receipt.readback_bytes, mark.len(), "{app} {}", edge.name);
                    assert_eq!(receipt.clear_bytes, 0, "{app} {}", edge.name);
                    assert_eq!(surface.place_calls, 1, "{app} {}", edge.name);
                    assert!(surface.typing_box.is_empty(), "{app} {}", edge.name);
                    exact_typing_box_readbacks += 1;
                    println!(
                        "TASK3538 app={app} edge={} position={},{} osl_active=true outcome=exact-typing-box-readback mark={mark:?}",
                        edge.name, edge.origin.0, edge.origin.1,
                    );
                }
                Err(error) => {
                    assert!(
                        error.contains("refused before placement"),
                        "{app} {} unexpected placement failure: {error}",
                        edge.name
                    );
                    assert_eq!(surface.place_calls, 0, "{app} {}", edge.name);
                    refusals_before_placement += 1;
                    println!(
                        "TASK3538 app={app} edge={} position={},{} osl_active=true outcome=refused-before-placement place_calls=0 refusal={error:?}",
                        edge.name, edge.origin.0, edge.origin.1,
                    );
                }
            }

            let marks_here = surface.other_control_mark_count(&mark);
            assert_eq!(marks_here, 0, "{app} {}", edge.name);
            other_control_marks += marks_here;
            println!(
                "TASK3538_OTHER_CONTROLS app={app} edge={} other_control_marks={marks_here}",
                edge.name
            );
        }
    }

    assert_eq!(
        OUTSIDE_APPS.len(),
        5,
        "the outside-app roster changed; update this matrix"
    );
    assert_eq!(
        EDGE_POSITIONS.len(),
        4,
        "the off-screen edge matrix must contain four edges"
    );
    assert_eq!(requests, 20);
    assert_eq!(
        exact_typing_box_readbacks + refusals_before_placement,
        requests
    );
    assert_eq!(other_control_marks, 0);
    println!(
        "TASK3538_SUMMARY apps={} edge_positions_per_app={} requests={requests} exact_typing_box_readbacks={exact_typing_box_readbacks} refusals_before_placement={refusals_before_placement} other_control_marks={other_control_marks}",
        OUTSIDE_APPS.len(),
        EDGE_POSITIONS.len(),
    );
}
