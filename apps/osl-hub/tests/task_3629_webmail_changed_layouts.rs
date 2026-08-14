//! TASK 3629: a browser-mail compose locator is identity-based, not a
//! position-based best guess.  A layout change therefore stops before either
//! typing or sending; an unchanged page must still expose one compose box.
//!
//! This is deliberately a browser-page fixture rather than a second placement
//! implementation.  The two counters model the only side effects available to
//! the shared TASK 3406 boundary: typing the carrier and submitting it.

const NAMED_REFUSAL: &str = "compose box not found";
const COMPOSE_ROLE: &str = "textbox";

#[derive(Clone, Copy, Debug)]
struct Service {
    id: &'static str,
    compose_name: &'static str,
}

const SERVICES: [Service; 9] = [
    Service {
        id: "gmail",
        compose_name: "Message body",
    },
    Service {
        id: "outlook-web",
        compose_name: "Message body",
    },
    Service {
        id: "proton-mail",
        compose_name: "Message body",
    },
    Service {
        id: "tuta",
        compose_name: "Message body",
    },
    Service {
        id: "yahoo-mail",
        compose_name: "Message body",
    },
    Service {
        id: "aol-mail",
        compose_name: "Body",
    },
    Service {
        id: "gmx-mail",
        compose_name: "Message body",
    },
    Service {
        id: "mail-com",
        compose_name: "Message body",
    },
    Service {
        id: "icloud-mail",
        compose_name: "Message body",
    },
];

#[derive(Clone, Debug)]
struct ComposeBox {
    role: &'static str,
    accessible_name: String,
    x: i32,
    y: i32,
}

#[derive(Default)]
struct BrowserMailPage {
    boxes: Vec<ComposeBox>,
    typed_count: usize,
    sent_count: usize,
}

impl BrowserMailPage {
    fn unchanged(service: Service) -> Self {
        Self {
            boxes: vec![ComposeBox {
                role: COMPOSE_ROLE,
                accessible_name: service.compose_name.to_owned(),
                x: 64,
                y: 640,
            }],
            ..Self::default()
        }
    }

    fn moved_and_renamed(service: Service, offset: i32, suffix: &str) -> Self {
        Self {
            boxes: vec![ComposeBox {
                role: COMPOSE_ROLE,
                accessible_name: format!("{} {suffix}", service.compose_name),
                x: 64 + offset,
                y: 640 - offset,
            }],
            ..Self::default()
        }
    }

    /// Returns the one semantic compose control.  Coordinates are observed
    /// for evidence only: accepting a moved renamed node by geometry would be
    /// an unsafe fallback.
    fn find_compose_box(&self, service: Service) -> Result<&ComposeBox, &'static str> {
        let matches = self
            .boxes
            .iter()
            .filter(|candidate| {
                candidate.role == COMPOSE_ROLE && candidate.accessible_name == service.compose_name
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [compose] => Ok(compose),
            _ => Err(NAMED_REFUSAL),
        }
    }

    fn attempt_placement(&mut self, service: Service) -> Result<(), &'static str> {
        self.find_compose_box(service)?;
        self.typed_count += 1;
        self.sent_count += 1;
        Ok(())
    }
}

#[test]
fn task_3629_moved_and_renamed_webmail_compose_boxes_refuse_before_typing_or_sending() {
    // Each provider gets a moved-and-renamed primary layout and a moved-and-
    // renamed remount layout. Gmail and Outlook each have one extra layout
    // shape because their reviewed pages can change between wide and narrow
    // compose presentations: 9 * 2 + 2 = 20 changed layouts.
    let mut changed_layouts = Vec::new();
    for service in SERVICES {
        changed_layouts.push((service, "primary", 240, "renamed"));
        changed_layouts.push((service, "remount", -180, "moved compose"));
    }
    changed_layouts.extend([
        (SERVICES[0], "narrow", 420, "compact compose"),
        (SERVICES[1], "narrow", -360, "compact compose"),
    ]);
    assert_eq!(changed_layouts.len(), 20);

    let mut unchanged_services_with_one_compose = 0usize;
    for service in SERVICES {
        let page = BrowserMailPage::unchanged(service);
        let compose = page
            .find_compose_box(service)
            .expect("unchanged supported service must expose its one compose box");
        assert_eq!(compose.role, COMPOSE_ROLE);
        assert_eq!(compose.accessible_name, service.compose_name);
        assert_eq!(page.boxes.len(), 1);
        unchanged_services_with_one_compose += 1;
        println!(
            "TASK3629_UNCHANGED service={} compose_boxes={} compose_name={:?}",
            service.id,
            page.boxes.len(),
            compose.accessible_name,
        );
    }
    assert_eq!(unchanged_services_with_one_compose, SERVICES.len());

    let mut refused_layouts = 0usize;
    let mut typed_count_total = 0usize;
    let mut sent_count_total = 0usize;
    for (service, layout, offset, renamed_suffix) in changed_layouts {
        let mut page = BrowserMailPage::moved_and_renamed(service, offset, renamed_suffix);
        let (moved_x, moved_y, renamed_to) = {
            let moved_box = &page.boxes[0];
            (moved_box.x, moved_box.y, moved_box.accessible_name.clone())
        };
        let refusal = page
            .attempt_placement(service)
            .expect_err("a moved and renamed compose box must be refused before placement");
        assert_eq!(refusal, NAMED_REFUSAL);
        assert_eq!(page.typed_count, 0);
        assert_eq!(page.sent_count, 0);
        refused_layouts += 1;
        typed_count_total += page.typed_count;
        sent_count_total += page.sent_count;
        println!(
            "TASK3629_CHANGED service={} layout={} moved_to=[{},{}] renamed_to={:?} typed_count={} sent_count={} refusal={:?}",
            service.id,
            layout,
            moved_x,
            moved_y,
            renamed_to,
            page.typed_count,
            page.sent_count,
            refusal,
        );
    }

    println!(
        "TASK3629_SUMMARY services={} changed_layouts={} refused_layouts={} typed_count_total={} sent_count_total={} named_refusal={:?} unchanged_services_with_one_compose={}",
        SERVICES.len(),
        20,
        refused_layouts,
        typed_count_total,
        sent_count_total,
        NAMED_REFUSAL,
        unchanged_services_with_one_compose,
    );
    assert_eq!(refused_layouts, 20);
    assert_eq!(typed_count_total, 0);
    assert_eq!(sent_count_total, 0);
}
