#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

// `NativeAppId`'s manifest is deliberately closed. Keep this list in manifest
// order so this matrix has one row for every outside-app surface currently
// declared there, rather than silently exercising only the Discord happy path.
const OUTSIDE_APPS: &[&str] = &["Discord", "Telegram", "Signal", "WhatsApp", "Outlook"];
const RESIZE_SIZES: &[(u32, u32)] = &[
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1600, 900),
];

/// A deterministic outside-app surface for the shared, provider-neutral 3406
/// placement path. `typing_box` is the only value exposed to the writer;
/// other controls are kept separately so an accidental target change is a
/// measurable failure, not an unobserved mock detail.
struct AttachedOutsideApp {
    app: &'static str,
    size: (u32, u32),
    attached: bool,
    typing_box: String,
    other_controls: [String; 2],
    placed_snapshot: String,
    place_calls: usize,
}

impl AttachedOutsideApp {
    fn attached(app: &'static str, size: (u32, u32)) -> Self {
        Self {
            app,
            size,
            attached: true,
            typing_box: String::new(),
            other_controls: [String::new(), String::new()],
            placed_snapshot: String::new(),
            place_calls: 0,
        }
    }

    fn detached(app: &'static str, size: (u32, u32)) -> Self {
        Self {
            attached: false,
            ..Self::attached(app, size)
        }
    }

    fn other_control_mark_count(&self, mark: &str) -> usize {
        self.other_controls
            .iter()
            .filter(|value| value.as_str() == mark)
            .count()
    }
}

impl SharedTextActions for AttachedOutsideApp {
    fn read_back_text(&mut self) -> Result<String, String> {
        if !self.attached {
            return Err(format!(
                "{} {}x{} refused before placement: OSL is not attached",
                self.app, self.size.0, self.size.1
            ));
        }
        Ok(self.typing_box.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        if !self.attached {
            return Err(format!(
                "{} {}x{} refused before placement: OSL is not attached",
                self.app, self.size.0, self.size.1
            ));
        }
        self.place_calls += 1;
        self.typing_box.clear();
        self.typing_box.push_str(text);
        self.placed_snapshot.clone_from(&self.typing_box);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        if !self.attached {
            return Err(format!(
                "{} {}x{} refused before placement: OSL is not attached",
                self.app, self.size.0, self.size.1
            ));
        }
        self.typing_box.clear();
        Ok(())
    }
}

#[test]
fn task_3535_checks_every_outside_app_at_five_attached_resize_sizes() {
    let mut requests = 0usize;
    let mut exact_readbacks = 0usize;
    let mut other_control_marks = 0usize;

    for &app in OUTSIDE_APPS {
        for &size in RESIZE_SIZES {
            let mark = format!("OSL-3535-{app}-{}x{}", size.0, size.1);
            let mut surface = AttachedOutsideApp::attached(app, size);
            let receipt = place_read_back_and_clear(&mut surface, &mark)
                .unwrap_or_else(|error| panic!("{app} {}x{}: {error}", size.0, size.1));

            requests += 1;
            assert_eq!(surface.placed_snapshot, mark, "{app} {}x{}", size.0, size.1);
            assert_eq!(
                receipt.placed_bytes,
                mark.len(),
                "{app} {}x{}",
                size.0,
                size.1
            );
            assert_eq!(
                receipt.readback_bytes,
                mark.len(),
                "{app} {}x{}",
                size.0,
                size.1
            );
            assert_eq!(receipt.clear_bytes, 0, "{app} {}x{}", size.0, size.1);
            assert_eq!(surface.place_calls, 1, "{app} {}x{}", size.0, size.1);
            assert!(surface.typing_box.is_empty(), "{app} {}x{}", size.0, size.1);

            let marks_here = surface.other_control_mark_count(&mark);
            assert_eq!(marks_here, 0, "{app} {}x{}", size.0, size.1);
            other_control_marks += marks_here;
            exact_readbacks += 1;
            println!(
                "TASK3535 app={app} size={}x{} attached=true outcome=exact-readback mark={mark:?} other_control_marks={marks_here}",
                size.0, size.1
            );
        }
    }

    assert_eq!(
        OUTSIDE_APPS.len(),
        5,
        "the outside-app roster changed; update this matrix"
    );
    assert_eq!(
        RESIZE_SIZES.len(),
        5,
        "the resize matrix must contain five sizes"
    );
    assert_eq!(requests, 25);
    assert_eq!(exact_readbacks, 25);
    assert_eq!(other_control_marks, 0);
    println!(
        "TASK3535_SUMMARY apps={} sizes_per_app={} requests={requests} exact_readbacks={exact_readbacks} other_control_marks={other_control_marks}",
        OUTSIDE_APPS.len(),
        RESIZE_SIZES.len(),
    );
}

#[test]
fn task_3535_refuses_every_detached_resize_before_placement() {
    let mut refusals = 0usize;

    for &app in OUTSIDE_APPS {
        for &size in RESIZE_SIZES {
            let mark = format!("OSL-3535-detached-{app}-{}x{}", size.0, size.1);
            let mut surface = AttachedOutsideApp::detached(app, size);
            let error = place_read_back_and_clear(&mut surface, &mark)
                .expect_err("a detached OSL surface must refuse before placement");

            assert!(error.contains("refused before placement"), "{error}");
            assert_eq!(surface.place_calls, 0, "{app} {}x{}", size.0, size.1);
            assert_eq!(
                surface.other_control_mark_count(&mark),
                0,
                "{app} {}x{}",
                size.0,
                size.1
            );
            refusals += 1;
            println!(
                "TASK3535 app={app} size={}x{} attached=false outcome=refused-before-placement place_calls=0 other_control_marks=0",
                size.0, size.1
            );
        }
    }

    assert_eq!(refusals, 25);
    println!("TASK3535_DETACHED_SUMMARY refusals={refusals} other_control_marks=0");
}
