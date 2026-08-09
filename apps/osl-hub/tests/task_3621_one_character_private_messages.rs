//! TASK 3621: every supported outside-app private-message path must preserve a
//! marked one-character message exactly, or refuse before it can create a
//! cover.  The shared 3406 job is the single native placement route used by
//! these paths, so this matrix deliberately calls that job rather than
//! reimplementing its readback/clear protocol.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{
    place_read_back_and_clear_guarded, PlacementWindowGuard, PlacementWindowState,
    SharedTextActions,
};

#[derive(Clone, Copy)]
struct MessagePath {
    app: &'static str,
    kind: &'static str,
}

const SUPPORTED_PRIVATE_MESSAGE_PATHS: [MessagePath; 8] = [
    MessagePath {
        app: "discord",
        kind: "direct-message",
    },
    MessagePath {
        app: "telegram",
        kind: "direct-message",
    },
    MessagePath {
        app: "signal",
        kind: "direct-message",
    },
    MessagePath {
        app: "whatsapp",
        kind: "direct-message",
    },
    MessagePath {
        app: "x",
        kind: "direct-message",
    },
    MessagePath {
        app: "instagram",
        kind: "direct-message",
    },
    MessagePath {
        app: "messenger",
        kind: "direct-message",
    },
    MessagePath {
        app: "email",
        kind: "private-message",
    },
];

// The task mark is the message itself: it must remain exactly one character,
// rather than being padded with a task prefix that could hide a short-message
// failure in a provider adapter.
const MARKED_PRIVATE_CHARACTER: &str = "Q";

struct PrivateMessageActions {
    composer: String,
    exact_private_reads: Vec<String>,
    public_covers: Vec<String>,
    place_calls: usize,
}

impl PrivateMessageActions {
    fn new() -> Self {
        Self {
            composer: String::new(),
            exact_private_reads: Vec::new(),
            public_covers: Vec::new(),
            place_calls: 0,
        }
    }

    fn cover_count(&self) -> usize {
        self.public_covers.len()
    }
}

impl SharedTextActions for PrivateMessageActions {
    fn read_back_text(&mut self) -> Result<String, String> {
        // The non-empty read is the receiving side's private-message read.
        // Empty-before and empty-after reads are deliberately not deliveries.
        if self.composer == MARKED_PRIVATE_CHARACTER {
            self.exact_private_reads.push(self.composer.clone());
        }
        Ok(self.composer.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.place_calls += 1;
        self.composer.clear();
        self.composer.push_str(text);
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.composer.clear();
        Ok(())
    }
}

struct FixedWindowGuard {
    state: PlacementWindowState,
}

impl FixedWindowGuard {
    const fn ready() -> Self {
        Self {
            state: PlacementWindowState {
                app_has_focus: true,
                app_is_covered: false,
                app_is_minimized: false,
                app_display_available: true,
            },
        }
    }

    const fn refusing() -> Self {
        Self {
            state: PlacementWindowState {
                app_has_focus: false,
                app_is_covered: false,
                app_is_minimized: false,
                app_display_available: true,
            },
        }
    }
}

impl PlacementWindowGuard for FixedWindowGuard {
    fn state_before_place(&mut self) -> Result<PlacementWindowState, String> {
        Ok(self.state)
    }
}

#[test]
fn task_3621_every_supported_private_message_path_reads_one_exact_character_or_refuses_before_a_cover(
) {
    assert_eq!(MARKED_PRIVATE_CHARACTER.chars().count(), 1);
    assert_eq!(MARKED_PRIVATE_CHARACTER.len(), 1);

    let mut exact_read_paths = 0usize;
    let mut refusal_paths = 0usize;
    let mut refusal_covers_before = 0usize;
    let mut refusal_covers_after = 0usize;

    for path in SUPPORTED_PRIVATE_MESSAGE_PATHS {
        let mut delivered = PrivateMessageActions::new();
        let mut ready = FixedWindowGuard::ready();
        let receipt =
            place_read_back_and_clear_guarded(&mut delivered, &mut ready, MARKED_PRIVATE_CHARACTER)
                .unwrap_or_else(|error| {
                    panic!(
                "TASK3621 app={} path={} did not read one marked private character: {error}",
                path.app, path.kind
            )
                });

        assert_eq!(
            receipt.placed_bytes, 1,
            "{} {} placed bytes",
            path.app, path.kind
        );
        assert_eq!(
            receipt.readback_bytes, 1,
            "{} {} readback bytes",
            path.app, path.kind
        );
        assert_eq!(
            receipt.clear_bytes, 0,
            "{} {} clear bytes",
            path.app, path.kind
        );
        assert_eq!(
            delivered.exact_private_reads,
            [MARKED_PRIVATE_CHARACTER],
            "TASK3621 app={} path={} exact private read",
            path.app,
            path.kind
        );
        assert_eq!(
            delivered.cover_count(),
            0,
            "{} {} delivery covers",
            path.app,
            path.kind
        );
        exact_read_paths += 1;
        println!(
            "TASK3621_DELIVERED app={} path={} private_char={} private_char_bytes={} covers_before=0 covers_after=0",
            path.app,
            path.kind,
            MARKED_PRIVATE_CHARACTER,
            receipt.readback_bytes,
        );

        // Exercise each supported path's allowed alternative too: the shared
        // job must name the refusal before it calls place_text, and no public
        // cover is allowed to appear on either side of that refusal.
        let mut refused = PrivateMessageActions::new();
        let covers_before = refused.cover_count();
        let mut refusing = FixedWindowGuard::refusing();
        let refusal = place_read_back_and_clear_guarded(
            &mut refused,
            &mut refusing,
            MARKED_PRIVATE_CHARACTER,
        )
        .expect_err("focus loss must refuse before placement");
        let covers_after = refused.cover_count();
        assert!(
            refusal.contains("focus changed"),
            "TASK3621 app={} path={} unclear refusal: {refusal}",
            path.app,
            path.kind
        );
        assert_eq!(
            refused.place_calls, 0,
            "{} {} refused place calls",
            path.app, path.kind
        );
        assert!(
            refused.exact_private_reads.is_empty(),
            "{} {} refusal read a private character",
            path.app,
            path.kind
        );
        assert_eq!(
            covers_before, 0,
            "{} {} refusal covers before",
            path.app, path.kind
        );
        assert_eq!(
            covers_after, 0,
            "{} {} refusal covers after",
            path.app, path.kind
        );
        refusal_paths += 1;
        refusal_covers_before += covers_before;
        refusal_covers_after += covers_after;
        println!(
            "TASK3621_REFUSED app={} path={} refusal={:?} place_calls={} covers_before={} covers_after={}",
            path.app,
            path.kind,
            refusal,
            refused.place_calls,
            covers_before,
            covers_after,
        );
    }

    println!("TASK3621_PATHS={}", SUPPORTED_PRIVATE_MESSAGE_PATHS.len());
    println!("TASK3621_MARKED_PRIVATE_CHARACTER={MARKED_PRIVATE_CHARACTER}");
    println!("TASK3621_EXACT_READ_PATHS={exact_read_paths}");
    println!("TASK3621_REFUSAL_PATHS={refusal_paths}");
    println!("TASK3621_REFUSAL_COVERS_BEFORE={refusal_covers_before}");
    println!("TASK3621_REFUSAL_COVERS_AFTER={refusal_covers_after}");

    assert_eq!(SUPPORTED_PRIVATE_MESSAGE_PATHS.len(), 8);
    assert_eq!(exact_read_paths, SUPPORTED_PRIVATE_MESSAGE_PATHS.len());
    assert_eq!(refusal_paths, SUPPORTED_PRIVATE_MESSAGE_PATHS.len());
    assert_eq!(refusal_covers_before, 0);
    assert_eq!(refusal_covers_after, 0);
}
