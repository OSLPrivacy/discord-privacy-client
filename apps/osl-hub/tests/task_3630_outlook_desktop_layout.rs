#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

const REVIEWED_COMPOSE_NAME: &str = "Message body";
const REVIEWED_COMPOSE_RECT: Rect = Rect {
    x: 320,
    y: 410,
    width: 760,
    height: 280,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Rect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[derive(Clone, Debug)]
struct OutlookDesktopControl {
    name: String,
    rect: Rect,
    editable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutlookComposeRefusal {
    Moved,
    Renamed,
}

impl OutlookComposeRefusal {
    fn named(self) -> &'static str {
        match self {
            Self::Moved => "OUTLOOK_DESKTOP_COMPOSE_REFUSED: MOVED",
            Self::Renamed => "OUTLOOK_DESKTOP_COMPOSE_REFUSED: RENAMED",
        }
    }
}

/// A small, deterministic model of the reviewed desktop compose binding.
///
/// The placement target is only available when the unique writable control has
/// both the recorded accessibility name and its reviewed desktop geometry.  A
/// layout drift therefore cannot turn into a best-effort write to a nearby
/// Outlook control.
struct OutlookDesktopComposeSurface {
    controls: Vec<OutlookDesktopControl>,
    typed_count: usize,
    sent_count: usize,
    compose_text: String,
    refusal: Option<OutlookComposeRefusal>,
}

impl OutlookDesktopComposeSurface {
    fn unchanged() -> Self {
        Self::with_compose(OutlookDesktopControl {
            name: REVIEWED_COMPOSE_NAME.to_owned(),
            rect: REVIEWED_COMPOSE_RECT,
            editable: true,
        })
    }

    fn moved() -> Self {
        Self::with_compose(OutlookDesktopControl {
            name: REVIEWED_COMPOSE_NAME.to_owned(),
            rect: Rect {
                x: REVIEWED_COMPOSE_RECT.x + 96,
                ..REVIEWED_COMPOSE_RECT
            },
            editable: true,
        })
    }

    fn renamed() -> Self {
        Self::with_compose(OutlookDesktopControl {
            name: "Write a message".to_owned(),
            rect: REVIEWED_COMPOSE_RECT,
            editable: true,
        })
    }

    fn with_compose(compose: OutlookDesktopControl) -> Self {
        Self {
            controls: vec![compose],
            typed_count: 0,
            sent_count: 0,
            compose_text: String::new(),
            refusal: None,
        }
    }

    fn matching_compose_count(&self) -> usize {
        self.controls
            .iter()
            .filter(|control| {
                control.editable
                    && control.name == REVIEWED_COMPOSE_NAME
                    && control.rect == REVIEWED_COMPOSE_RECT
            })
            .count()
    }

    fn require_reviewed_compose(&mut self) -> Result<(), String> {
        if self.matching_compose_count() == 1 {
            return Ok(());
        }

        let candidate = self
            .controls
            .iter()
            .find(|control| control.editable)
            .expect("fixture retains one editable Outlook compose candidate");
        let refusal = if candidate.name != REVIEWED_COMPOSE_NAME {
            OutlookComposeRefusal::Renamed
        } else {
            OutlookComposeRefusal::Moved
        };
        self.refusal = Some(refusal);
        Err(refusal.named().to_owned())
    }
}

impl SharedTextActions for OutlookDesktopComposeSurface {
    fn read_back_text(&mut self) -> Result<String, String> {
        self.require_reviewed_compose()?;
        Ok(self.compose_text.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.require_reviewed_compose()?;
        self.compose_text.clear();
        self.compose_text.push_str(text);
        self.typed_count += 1;
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.require_reviewed_compose()?;
        self.compose_text.clear();
        Ok(())
    }
}

#[test]
fn task_3630_outlook_desktop_changed_layouts_refuse_before_typing_or_sending() {
    let mark = "OSL-3630-layout-drift";
    let cases = [
        (
            "moved",
            OutlookDesktopComposeSurface::moved(),
            OutlookComposeRefusal::Moved,
        ),
        (
            "renamed",
            OutlookDesktopComposeSurface::renamed(),
            OutlookComposeRefusal::Renamed,
        ),
    ];
    let mut total_typed = 0usize;
    let mut total_sent = 0usize;

    for (layout, mut surface, expected_refusal) in cases {
        let error = place_read_back_and_clear(&mut surface, mark)
            .expect_err("a changed Outlook desktop compose layout must refuse");
        assert_eq!(error, expected_refusal.named(), "{layout} refusal");
        assert_eq!(
            surface.refusal,
            Some(expected_refusal),
            "{layout} refusal state"
        );
        assert_eq!(surface.typed_count, 0, "{layout} must not type");
        assert_eq!(surface.sent_count, 0, "{layout} must not send");
        assert!(
            surface.compose_text.is_empty(),
            "{layout} must not write text"
        );
        total_typed += surface.typed_count;
        total_sent += surface.sent_count;
        println!(
            "TASK3630 layout={layout} typed_count={} sent_count={} refusal={error}",
            surface.typed_count, surface.sent_count
        );
    }

    assert_eq!(total_typed, 0);
    assert_eq!(total_sent, 0);
    println!(
        "TASK3630_CHANGED_LAYOUTS_SUMMARY layouts=2 typed_count={total_typed} sent_count={total_sent}"
    );
}

#[test]
fn task_3630_outlook_desktop_unchanged_layout_finds_exactly_one_compose_box() {
    let mark = "OSL-3630-control";
    let mut surface = OutlookDesktopComposeSurface::unchanged();

    assert_eq!(surface.matching_compose_count(), 1);
    let receipt = place_read_back_and_clear(&mut surface, mark)
        .expect("the unchanged reviewed Outlook compose layout remains placeable");
    assert_eq!(receipt.placed_bytes, mark.len());
    assert_eq!(receipt.readback_bytes, mark.len());
    assert_eq!(receipt.clear_bytes, 0);
    assert_eq!(surface.typed_count, 1);
    assert_eq!(surface.sent_count, 0);
    assert!(surface.compose_text.is_empty());
    assert_eq!(surface.refusal, None);
    println!(
        "TASK3630 layout=unchanged compose_box_count={} typed_count={} sent_count={}",
        surface.matching_compose_count(),
        surface.typed_count,
        surface.sent_count
    );
}
