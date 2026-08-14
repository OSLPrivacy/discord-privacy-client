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
enum OutlookAlternateLayout {
    AlternateTheme,
    CompactMode,
    BetaLayout,
}

impl OutlookAlternateLayout {
    fn name(self) -> &'static str {
        match self {
            Self::AlternateTheme => "alternate-theme",
            Self::CompactMode => "compact-mode",
            Self::BetaLayout => "beta-layout",
        }
    }

    fn refusal(self) -> Option<&'static str> {
        match self {
            Self::AlternateTheme => None,
            Self::CompactMode => Some("OUTLOOK_DESKTOP_COMPOSE_REFUSED: COMPACT_MODE"),
            Self::BetaLayout => Some("OUTLOOK_DESKTOP_COMPOSE_REFUSED: BETA_LAYOUT"),
        }
    }
}

/// Deterministic model of the reviewed Outlook desktop compose binding.
///
/// An alternate colour theme leaves the accessibility target unchanged. Compact
/// mode changes its recorded geometry and the beta layout changes its
/// accessibility name, so neither may be treated as a reviewed compose box.
struct OutlookDesktopComposeSurface {
    layout: OutlookAlternateLayout,
    controls: Vec<OutlookDesktopControl>,
    compose_text: String,
    typed_characters: usize,
    sent_messages: usize,
    refusal: Option<&'static str>,
}

impl OutlookDesktopComposeSurface {
    fn for_layout(layout: OutlookAlternateLayout) -> Self {
        let compose = match layout {
            OutlookAlternateLayout::AlternateTheme => OutlookDesktopControl {
                name: REVIEWED_COMPOSE_NAME.to_owned(),
                rect: REVIEWED_COMPOSE_RECT,
                editable: true,
            },
            OutlookAlternateLayout::CompactMode => OutlookDesktopControl {
                name: REVIEWED_COMPOSE_NAME.to_owned(),
                rect: Rect {
                    y: REVIEWED_COMPOSE_RECT.y - 104,
                    height: 184,
                    ..REVIEWED_COMPOSE_RECT
                },
                editable: true,
            },
            OutlookAlternateLayout::BetaLayout => OutlookDesktopControl {
                name: "Compose message".to_owned(),
                rect: REVIEWED_COMPOSE_RECT,
                editable: true,
            },
        };

        Self {
            layout,
            controls: vec![compose],
            compose_text: String::new(),
            typed_characters: 0,
            sent_messages: 0,
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

        let refusal = self
            .layout
            .refusal()
            .expect("only changed layouts can fail reviewed compose discovery");
        self.refusal = Some(refusal);
        Err(refusal.to_owned())
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
        self.typed_characters += text.chars().count();
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.require_reviewed_compose()?;
        self.compose_text.clear();
        Ok(())
    }
}

#[test]
fn task_3639_outlook_desktop_alternate_layouts_find_or_refuse_before_typing() {
    let mark = "OSL-3639-alternate-layout";
    let mut found_compose_boxes = 0usize;
    let mut refused_layouts = 0usize;

    for layout in [
        OutlookAlternateLayout::AlternateTheme,
        OutlookAlternateLayout::CompactMode,
        OutlookAlternateLayout::BetaLayout,
    ] {
        let mut surface = OutlookDesktopComposeSurface::for_layout(layout);
        let typed_before = surface.typed_characters;
        let sent_before = surface.sent_messages;

        match layout.refusal() {
            None => {
                assert_eq!(
                    surface.matching_compose_count(),
                    1,
                    "{} compose boxes",
                    layout.name()
                );
                let receipt = place_read_back_and_clear(&mut surface, mark)
                    .expect("the alternate theme must retain its reviewed compose box");
                assert_eq!(receipt.placed_bytes, mark.len());
                assert_eq!(receipt.readback_bytes, mark.len());
                assert_eq!(receipt.clear_bytes, 0);
                assert_eq!(surface.sent_messages, 0);
                assert!(surface.compose_text.is_empty());
                found_compose_boxes += surface.matching_compose_count();
                println!(
                    "TASK3639 layout={} compose_box_count={} typed_characters={} sent_messages={}",
                    layout.name(),
                    surface.matching_compose_count(),
                    surface.typed_characters,
                    surface.sent_messages
                );
            }
            Some(expected_refusal) => {
                let error = place_read_back_and_clear(&mut surface, mark)
                    .expect_err("a changed Outlook layout must refuse before placement");
                assert_eq!(error, expected_refusal, "{} refusal", layout.name());
                assert_eq!(
                    surface.refusal,
                    Some(expected_refusal),
                    "{} refusal state",
                    layout.name()
                );
                assert_eq!(
                    surface.typed_characters,
                    typed_before,
                    "{} typed characters before/after",
                    layout.name()
                );
                assert_eq!(
                    surface.sent_messages,
                    sent_before,
                    "{} sent messages before/after",
                    layout.name()
                );
                assert_eq!(
                    typed_before,
                    0,
                    "{} typed characters before refusal",
                    layout.name()
                );
                assert_eq!(
                    sent_before,
                    0,
                    "{} sent messages before refusal",
                    layout.name()
                );
                assert!(
                    surface.compose_text.is_empty(),
                    "{} compose text after refusal",
                    layout.name()
                );
                refused_layouts += 1;
                println!(
                    "TASK3639 layout={} refusal={} typed_characters_before={} typed_characters_after={} sent_messages_before={} sent_messages_after={}",
                    layout.name(),
                    error,
                    typed_before,
                    surface.typed_characters,
                    sent_before,
                    surface.sent_messages
                );
            }
        }
    }

    assert_eq!(
        found_compose_boxes, 1,
        "exactly one reviewed compose box across found layouts"
    );
    assert_eq!(
        refused_layouts, 2,
        "exactly two changed layouts must refuse"
    );
    println!(
        "TASK3639_SUMMARY layouts=3 compose_boxes_found={found_compose_boxes} refusals={refused_layouts} refusal_typed_characters=0 refusal_sent_messages=0"
    );
}
