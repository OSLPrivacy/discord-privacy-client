//! TASK 3628: a social-web composer is bound by both its accessible name and
//! its location.  A provider layout refresh may leave a plausible, editable
//! message box on screen, but OSL must not type into it if either observation
//! changed after discovery.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{place_read_back_and_clear, SharedTextActions};

const SITES: [&str; 3] = ["X", "Instagram", "Messenger"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MessageBox {
    accessible_name: &'static str,
    left: i32,
    top: i32,
}

#[derive(Clone, Copy)]
enum LayoutChange {
    Moved,
    Renamed,
}

impl LayoutChange {
    const fn name(self) -> &'static str {
        match self {
            Self::Moved => "message box moved",
            Self::Renamed => "message box renamed",
        }
    }
}

const CHANGED_LAYOUTS: [LayoutChange; 2] = [LayoutChange::Moved, LayoutChange::Renamed];

struct SocialSurface {
    site: &'static str,
    discovered_message_box: MessageBox,
    current_message_boxes: Vec<MessageBox>,
    typing_box_text: String,
    placement_attempts: usize,
    typed_count: usize,
    sent_count: usize,
}

impl SocialSurface {
    fn unchanged(site: &'static str) -> Self {
        // The distinct names keep this fixture honest about name matching;
        // every normal layout still has exactly one discoverable message box.
        let message_box = match site {
            "X" => MessageBox {
                accessible_name: "Message @Ada",
                left: 96,
                top: 684,
            },
            "Instagram" => MessageBox {
                accessible_name: "Message Ada",
                left: 104,
                top: 701,
            },
            "Messenger" => MessageBox {
                accessible_name: "Message",
                left: 112,
                top: 692,
            },
            _ => unreachable!("task 3628 only exercises named social sites"),
        };
        Self {
            site,
            discovered_message_box: message_box,
            current_message_boxes: vec![message_box],
            typing_box_text: String::new(),
            placement_attempts: 0,
            typed_count: 0,
            sent_count: 0,
        }
    }

    fn changed(site: &'static str, change: LayoutChange) -> Self {
        let mut surface = Self::unchanged(site);
        let current = surface
            .current_message_boxes
            .first_mut()
            .expect("normal social fixture has one message box");
        match change {
            LayoutChange::Moved => current.left += 240,
            LayoutChange::Renamed => current.accessible_name = "Write a message",
        }
        surface
    }

    fn message_box_count(&self) -> usize {
        self.current_message_boxes.len()
    }

    fn current_message_box(&self) -> Result<MessageBox, String> {
        match self.current_message_boxes.as_slice() {
            [message_box] => Ok(*message_box),
            _ => Err(format!(
                "placement refused: {} must expose exactly one message box before text was put down",
                self.site
            )),
        }
    }

    fn refusal_for_changed_message_box(&self) -> Result<(), String> {
        let current = self.current_message_box()?;
        if current.left != self.discovered_message_box.left
            || current.top != self.discovered_message_box.top
        {
            return Err(format!(
                "placement refused: {} message box moved before text was put down",
                self.site
            ));
        }
        if current.accessible_name != self.discovered_message_box.accessible_name {
            return Err(format!(
                "placement refused: {} message box renamed before text was put down",
                self.site
            ));
        }
        Ok(())
    }
}

impl SharedTextActions for SocialSurface {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.typing_box_text.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.placement_attempts += 1;
        // This is the last operation before the shared 3406 job would put
        // text down.  Do not mutate a page that no longer matches discovery.
        self.refusal_for_changed_message_box()?;
        self.typing_box_text = text.to_owned();
        self.typed_count += 1;
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.typing_box_text.clear();
        Ok(())
    }
}

#[test]
fn task_3628_changed_social_layouts_refuse_before_typing_or_sending() {
    let mut changed_layouts = 0usize;
    let mut typed_count = 0usize;
    let mut sent_count = 0usize;

    for site in SITES {
        for change in CHANGED_LAYOUTS {
            let mark = format!("TASK3628-{site}-{}", change.name());
            let mut surface = SocialSurface::changed(site, change);
            let refusal = place_read_back_and_clear(&mut surface, &mark)
                .expect_err("a changed social-web message box must refuse placement");

            assert_eq!(surface.message_box_count(), 1, "{site} changed fixture box count");
            assert_eq!(surface.placement_attempts, 1, "{site} must attempt the shared placement");
            assert_eq!(surface.typed_count, 0, "{site} {change_name} typed despite refusal", change_name = change.name());
            assert_eq!(surface.sent_count, 0, "{site} {change_name} sent despite refusal", change_name = change.name());
            assert!(
                refusal.contains(change.name()),
                "TASK3628 site={site} expected named refusal {:?}, got {refusal:?}",
                change.name()
            );

            changed_layouts += 1;
            typed_count += surface.typed_count;
            sent_count += surface.sent_count;
            println!(
                "TASK3628_CHANGED site={site} change={} message_boxes={} placement_attempts={} typed_count={} sent_count={} refusal={refusal:?}",
                change.name(),
                surface.message_box_count(),
                surface.placement_attempts,
                surface.typed_count,
                surface.sent_count,
            );
        }
    }

    println!("TASK3628_CHANGED_LAYOUTS={changed_layouts}");
    println!("TASK3628_CHANGED_TYPED_COUNT={typed_count}");
    println!("TASK3628_CHANGED_SENT_COUNT={sent_count}");
    assert_eq!(changed_layouts, 6);
    assert_eq!(typed_count, 0);
    assert_eq!(sent_count, 0);
}

#[test]
fn task_3628_unchanged_social_sites_still_find_one_message_box() {
    let mut sites_with_one_message_box = 0usize;

    for site in SITES {
        let mut surface = SocialSurface::unchanged(site);
        assert_eq!(surface.message_box_count(), 1, "TASK3628 site={site} message box count");
        let receipt = place_read_back_and_clear(&mut surface, "TASK3628-unchanged-message-box")
            .unwrap_or_else(|error| panic!("TASK3628 unchanged site={site} refused: {error}"));
        assert_eq!(surface.typed_count, 1, "TASK3628 unchanged site={site} typed once");
        assert_eq!(surface.sent_count, 0, "TASK3628 unchanged site={site} did not send");
        assert!(surface.typing_box_text.is_empty(), "TASK3628 unchanged site={site} cleared mark");
        assert_eq!(receipt.clear_bytes, 0);
        sites_with_one_message_box += 1;
        println!(
            "TASK3628_UNCHANGED site={site} message_boxes={} typed_count={} sent_count={} clear_bytes={}",
            surface.message_box_count(), surface.typed_count, surface.sent_count, receipt.clear_bytes
        );
    }

    println!("TASK3628_UNCHANGED_SITES_WITH_ONE_MESSAGE_BOX={sites_with_one_message_box}");
    assert_eq!(sites_with_one_message_box, SITES.len());
}
