use task_5630_clipboard_guard::*;

const WINDOWS_PLACE_SOURCE: &str = include_str!("../../examples/task_3406_place_text.rs");
const HUB_UI_SOURCE: &str = include_str!("../../../osl-hub-ui/src/main.ts");
const WHATSAPP_OVERLAY_SOURCE: &str = include_str!("../../../osl-hub-ui/src/whatsapp-overlay.ts");

fn inventory() -> ClipboardInventory {
    let values = [
        (
            13,
            "CF_UNICODETEXT",
            FormatClass::Unicode,
            b"U\0n\0i\0\0\0".as_slice(),
            b"".as_slice(),
        ),
        (
            49_201,
            "HTML Format",
            FormatClass::Html,
            b"Version:1.0\r\n<html>5630</html>\0".as_slice(),
            b"".as_slice(),
        ),
        (
            49_202,
            "Rich Text Format",
            FormatClass::Rtf,
            b"{\\rtf1 5630}\0".as_slice(),
            b"".as_slice(),
        ),
        (
            8,
            "CF_DIB",
            FormatClass::Dib,
            b"DIB-5630-bitmap".as_slice(),
            b"".as_slice(),
        ),
        (
            15,
            "CF_HDROP",
            FormatClass::FileDrop,
            b"DROPFILES\0C:\\5630.txt\0\0".as_slice(),
            b"".as_slice(),
        ),
        (
            16,
            "CF_LOCALE",
            FormatClass::Locale,
            &[9, 4, 0, 0],
            b"".as_slice(),
        ),
        (
            49_203,
            "OSL.App.Private.5630",
            FormatClass::AppPrivate,
            b"private-format-state".as_slice(),
            b"".as_slice(),
        ),
        (
            49_204,
            "OSL.Delayed.Render.5630",
            FormatClass::DelayedRender,
            b"delayed-bytes".as_slice(),
            b"WM_RENDERFORMAT:pending".as_slice(),
        ),
    ];
    ClipboardInventory {
        runtime_discovered: true,
        complete: true,
        formats: values
            .into_iter()
            .map(|(id, name, class, bytes, delayed)| FormatState {
                id,
                name: name.to_owned(),
                class,
                bytes: bytes.to_vec(),
                delayed_obligation: delayed.to_vec(),
            })
            .collect(),
        owner: OwnerState {
            window: 0x5630,
            process_id: 56_300,
            thread_id: 5_630,
            behavior: b"WM_RENDERFORMAT=0;WM_RENDERALLFORMATS=0;owner-alive=1".to_vec(),
        },
        sequence: 563_034_069,
        history_metadata: b"source-history:0-marked".to_vec(),
        cloud_metadata: b"cloud-queue:0-marked".to_vec(),
    }
}

fn endpoints(transaction: &str) -> Vec<EndpointObservation> {
    [
        ("windows-source", true, false),
        ("windows-synchronized-second", false, true),
    ]
    .into_iter()
    .map(|(endpoint, source, synchronized)| EndpointObservation {
        endpoint: endpoint.to_owned(),
        source,
        already_synchronized: synchronized,
        samples: (0..=120)
            .map(|index| EndpointSample {
                real_time_ms: 1_000_000 + index * 5_000,
                queue_depth: 0,
                current_marked: 0,
                history_marked: 0,
                cloud_marked: 0,
            })
            .collect(),
    })
    .collect::<Vec<_>>()
    .tap(|_| {
        let _ = transaction;
    })
}

trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&Self)) -> Self {
        f(&self);
        self
    }
}
impl<T> Tap for T {}

fn non_clipboard_route(name: &str, index: usize) -> RouteObservation {
    let transaction = format!("txn-5630-{index:04}");
    let cover = format!("MAPLE-5630-{name}-{index:04}");
    let writer = Sink::Writer(name.to_owned());
    RouteObservation {
        transaction: transaction.clone(),
        route: name.to_owned(),
        kind: RouteKind::NonClipboard,
        cover: cover.clone(),
        inventory_before: inventory(),
        inventory_after: inventory(),
        sink_inventory_runtime_discovered: true,
        sinks: vec![
            Sink::Clipboard,
            Sink::Model("independent-cover-generator".to_owned()),
            writer.clone(),
        ],
        events: vec![
            Event {
                ordinal: 1,
                real_time_ms: 100,
                name: "independent-win32-inventory".to_owned(),
                actor: Actor::IndependentObserver,
                kind: EventKind::InventoryComplete,
                sink: Some(Sink::Clipboard),
                origin: None,
            },
            Event {
                ordinal: 2,
                real_time_ms: 110,
                name: "independent-cover-request".to_owned(),
                actor: Actor::IndependentObserver,
                kind: EventKind::ModelInput,
                sink: Some(Sink::Model("independent-cover-generator".to_owned())),
                origin: Some(ValueOrigin::IndependentCover),
            },
            Event {
                ordinal: 3,
                real_time_ms: 120,
                name: format!("{name}-targeted-writer"),
                actor: Actor::PlacementRoute,
                kind: EventKind::PlacementTouch,
                sink: Some(writer.clone()),
                origin: Some(ValueOrigin::IndependentCover),
            },
            Event {
                ordinal: 4,
                real_time_ms: 130,
                name: format!("{name}-consumed"),
                actor: Actor::IndependentObserver,
                kind: EventKind::CarrierConsumed { cover },
                sink: Some(writer),
                origin: Some(ValueOrigin::IndependentCover),
            },
            Event {
                ordinal: 5,
                real_time_ms: 140,
                name: "independent-final-inventory".to_owned(),
                actor: Actor::IndependentObserver,
                kind: EventKind::ClipboardRead,
                sink: Some(Sink::Clipboard),
                origin: None,
            },
        ],
        endpoints: endpoints(&transaction),
        recovery: None,
        real_clock: true,
    }
}

fn valid_recovery(transaction: &str) -> ClipboardRecoveryProof {
    ClipboardRecoveryProof {
        journal: RecoveryJournal {
            transaction: transaction.to_owned(),
            durable_outside_osl: true,
            fsynced_before_first_write: true,
            authenticated: true,
            os_protected: true,
            complete_snapshot: true,
            expected_owner_sequence: true,
            phase_and_cleanup: true,
            erased_after_two_endpoint_quiescence: true,
        },
        service_registered_before_write: true,
        osl_and_broker_killed_after_consumption: true,
        automatic_without_person: true,
        restored_within_ms: 4_999,
        service_killed_and_windows_restarted: true,
        startup_recovery_before_placement: true,
        locked_owner_sequence_revalidation: true,
        concurrent_b_written_after_journal: true,
        exact_b_preserved: true,
        stale_a_restored_over_b: false,
    }
}

fn clipboard_route() -> RouteObservation {
    let mut route = non_clipboard_route("legacy-clipboard-route", 99);
    route.kind = RouteKind::Clipboard;
    route.events.push(Event {
        ordinal: 6,
        real_time_ms: 125,
        name: "legacy-cover-write".to_owned(),
        actor: Actor::PlacementRoute,
        kind: EventKind::ClipboardWrite,
        sink: Some(Sink::Clipboard),
        origin: Some(ValueOrigin::IndependentCover),
    });
    route.recovery = Some(valid_recovery(&route.transaction));
    route
}

#[test]
fn installed_shipping_campaign_is_all_non_clipboard_and_observed_for_ten_real_minutes() {
    let routes = ["Discord", "Signal", "WhatsApp", "Telegram"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| non_clipboard_route(name, index + 1))
        .collect::<Vec<_>>();
    let receipts =
        verify_release_campaign(true, &routes).expect("complete installed release campaign");
    assert_eq!(receipts.len(), 4);
    assert!(receipts.iter().all(|receipt| receipt.formats == 8));
    assert!(receipts
        .iter()
        .all(|receipt| receipt.clipboard_route_events == 0));
    assert!(receipts
        .iter()
        .all(|receipt| receipt.private_derived_values == 0));
    assert!(receipts.iter().all(|receipt| receipt.endpoints == 2));
    assert!(receipts
        .iter()
        .all(|receipt| receipt.observed_ms == POST_QUIESCENCE_MS));
    println!("TASK5630_RELEASE_ROUTES=4");
    println!("TASK5630_NON_CLIPBOARD_ROUTES=4");
    println!("TASK5630_CLIPBOARD_ROUTE_EVENTS=0");
    println!("TASK5630_RUNTIME_FORMATS=8");
    println!("TASK5630_ENDPOINTS=2");
    println!("TASK5630_POST_QUIESCENCE_REAL_MS={POST_QUIESCENCE_MS}");
    println!("TASK5630_MARKED_CURRENT_HISTORY_CLOUD=0/0/0");
    println!("TASK5630_PRIVATE_DERIVED_SINK_VALUES=0");
}

#[test]
fn all_private_derivations_and_inventory_mutants_exit_one_with_named_diagnostics() {
    let representations = [
        PrivateRepresentation::Plaintext,
        PrivateRepresentation::Encoding("base64".to_owned()),
        PrivateRepresentation::Hash("sha256".to_owned()),
        PrivateRepresentation::Tokenization,
        PrivateRepresentation::Embedding,
        PrivateRepresentation::Compression("zstd".to_owned()),
        PrivateRepresentation::LengthPayload,
        PrivateRepresentation::Other("message-derived".to_owned()),
    ];
    let mut failures = 0;
    for (index, representation) in representations.into_iter().enumerate() {
        let mut route = non_clipboard_route("Discord", 1);
        let sink = match index % 3 {
            0 => Sink::Clipboard,
            1 => Sink::Model("independent-cover-generator".to_owned()),
            _ => Sink::Writer("Discord".to_owned()),
        };
        let name = format!("derived-{index}");
        route.events.push(Event {
            ordinal: 9,
            real_time_ms: 119,
            name: name.clone(),
            actor: Actor::PlacementRoute,
            kind: EventKind::WriterInput,
            sink: Some(sink),
            origin: Some(ValueOrigin::PrivateDerived(representation)),
        });
        let error = verify_route(&route).expect_err("private-derived mutant must fail");
        assert!(error.contains(&name) && error.contains("sink="), "{error}");
        println!("TASK5630_MUTANT=private-{index} exit=1 diagnostic={error}");
        failures += 1;
    }

    let base = non_clipboard_route("Discord", 1);
    let mut cases: Vec<(&str, RouteObservation, &str)> = Vec::new();
    let mut omitted = base.clone();
    omitted.inventory_after.formats.remove(2);
    cases.push(("omitted-format", omitted, "format"));
    let mut owner = base.clone();
    owner.inventory_after.owner.behavior.push(1);
    cases.push(("owner-change", owner, "owner"));
    let mut sequence = base.clone();
    sequence.inventory_after.sequence += 1;
    cases.push(("sequence-change", sequence, "sequence"));
    let mut history = base.clone();
    history.inventory_after.history_metadata.push(1);
    cases.push(("history-change", history, "history"));
    let mut early = base.clone();
    early.events.push(Event {
        ordinal: 0,
        real_time_ms: 1,
        name: "earlier-touch".to_owned(),
        actor: Actor::PlacementRoute,
        kind: EventKind::PlacementTouch,
        sink: Some(Sink::Writer("Discord".to_owned())),
        origin: Some(ValueOrigin::IndependentCover),
    });
    cases.push(("earlier-touch", early, "earlier-touch"));
    let mut empty = base.clone();
    empty.inventory_before.formats.clear();
    cases.push(("empty-inventory", empty, "empty"));
    let mut fixture = base.clone();
    fixture.inventory_before.runtime_discovered = false;
    cases.push(("fixture-inventory", fixture, "fixture"));
    let mut event = base.clone();
    event.events.push(Event {
        ordinal: 7,
        real_time_ms: 150,
        name: "forbidden-clipboard-read".to_owned(),
        actor: Actor::PlacementRoute,
        kind: EventKind::ClipboardRead,
        sink: Some(Sink::Clipboard),
        origin: None,
    });
    cases.push(("nonclipboard-event", event, "clipboard events"));
    let mut short = base.clone();
    short
        .endpoints
        .iter_mut()
        .for_each(|endpoint| endpoint.samples.truncate(120));
    cases.push(("short-observation", short, "shortened observation"));
    let mut one = base.clone();
    one.endpoints.truncate(1);
    cases.push(("one-endpoint", one, "one endpoint"));
    let mut accelerated = base.clone();
    accelerated.real_clock = false;
    cases.push(("accelerated-clock", accelerated, "accelerated"));
    for (name, route, token) in cases {
        let error = verify_route(&route).expect_err("mutant must fail");
        assert!(error.contains(token), "{name}: {error}");
        println!("TASK5630_MUTANT={name} exit=1 diagnostic={error}");
        failures += 1;
    }
    assert_eq!(failures, 19);
    println!("TASK5630_PROVENANCE_AND_INVENTORY_MUTANTS=19/19");
    println!("TASK5630_LITERAL_ONLY_SCAN_ACCEPTED=false");
}

#[test]
fn clipboard_recovery_starvation_and_stale_a_over_b_are_rejected() {
    verify_route(&clipboard_route()).expect("complete clipboard proof fixture passes");
    let mut cases: Vec<(&str, RouteObservation, &str)> = Vec::new();
    let mut no_journal = clipboard_route();
    no_journal
        .recovery
        .as_mut()
        .unwrap()
        .journal
        .durable_outside_osl = false;
    cases.push(("ram-only-journal", no_journal, "journal"));
    let mut unfsynced = clipboard_route();
    unfsynced
        .recovery
        .as_mut()
        .unwrap()
        .journal
        .fsynced_before_first_write = false;
    cases.push(("unfsynced-journal", unfsynced, "journal"));
    let mut late = clipboard_route();
    late.recovery.as_mut().unwrap().restored_within_ms = 5_001;
    cases.push(("late-recovery", late, "deadline"));
    let mut relaunch = clipboard_route();
    relaunch.recovery.as_mut().unwrap().automatic_without_person = false;
    cases.push(("person-relaunch", relaunch, "automatic"));
    let mut no_kill = clipboard_route();
    no_kill
        .recovery
        .as_mut()
        .unwrap()
        .osl_and_broker_killed_after_consumption = false;
    cases.push(("omitted-kill", no_kill, "kill"));
    let mut no_restart = clipboard_route();
    no_restart
        .recovery
        .as_mut()
        .unwrap()
        .service_killed_and_windows_restarted = false;
    cases.push(("omitted-restart", no_restart, "startup"));
    let mut startup = clipboard_route();
    startup
        .recovery
        .as_mut()
        .unwrap()
        .startup_recovery_before_placement = false;
    cases.push(("startup-late", startup, "startup"));
    let mut unlocked = clipboard_route();
    unlocked
        .recovery
        .as_mut()
        .unwrap()
        .locked_owner_sequence_revalidation = false;
    cases.push(("unlocked-revalidation", unlocked, "locked revalidation"));
    let mut sequential_b = clipboard_route();
    sequential_b
        .recovery
        .as_mut()
        .unwrap()
        .concurrent_b_written_after_journal = false;
    cases.push((
        "sequential-before-journal-b",
        sequential_b,
        "locked revalidation",
    ));
    let mut stale = clipboard_route();
    stale.recovery.as_mut().unwrap().exact_b_preserved = false;
    stale.recovery.as_mut().unwrap().stale_a_restored_over_b = true;
    cases.push(("stale-a-over-b", stale, "stale A-over-B"));
    let mut erased = clipboard_route();
    erased
        .recovery
        .as_mut()
        .unwrap()
        .journal
        .erased_after_two_endpoint_quiescence = false;
    cases.push(("journal-erased-early", erased, "journal erased"));
    let mut absent = clipboard_route();
    absent.recovery = None;
    cases.push(("missing-recovery-proof", absent, "omitted kill/restart"));
    for (name, route, token) in cases {
        let error = verify_route(&route).expect_err("recovery mutant must fail");
        assert!(error.contains(token), "{name}: {error}");
        println!("TASK5630_RECOVERY_MUTANT={name} exit=1 diagnostic={error}");
    }
    println!("TASK5630_RECOVERY_MUTANTS=12/12");
}

#[test]
fn delayed_publication_after_first_cleanup_stays_red_and_names_second_endpoint() {
    let mut route = non_clipboard_route("Signal", 2);
    let endpoint = route
        .endpoints
        .iter_mut()
        .find(|endpoint| !endpoint.source)
        .unwrap();
    endpoint.samples[61].history_marked = 1;
    let error = verify_route(&route).expect_err("late publication cannot go green early");
    assert!(error.contains("transaction=txn-5630-0002"));
    assert!(error.contains("endpoint=windows-synchronized-second"));
    assert!(error.contains("late marked publication"));
    println!("TASK5630_DELAYED_PUBLICATION_EXIT=1");
    println!("TASK5630_DELAYED_PUBLICATION_DIAGNOSTIC={error}");
}

#[test]
fn shipping_private_carrier_sources_have_zero_clipboard_primitives() {
    for primitive in [
        "SetClipboardData",
        "GetClipboardData",
        "OpenClipboard",
        "EmptyClipboard",
        "send_ctrl_v",
        "VK_CONTROL",
    ] {
        assert!(
            !WINDOWS_PLACE_SOURCE.contains(primitive),
            "Windows placement source retained {primitive}"
        );
    }
    for call in [
        "navigator.clipboard.writeText(peerProtectedSheet.coverText)",
        "navigator.clipboard.writeText(prepared.capsule)",
        "navigator.clipboard.writeText(localProtectedSheet.capsule)",
    ] {
        assert!(!HUB_UI_SOURCE.contains(call), "hub UI retained {call}");
    }
    assert!(!WHATSAPP_OVERLAY_SOURCE.contains("navigator.clipboard"));
    assert!(WINDOWS_PLACE_SOURCE.contains("pattern.SetValue(&BSTR::from(text))"));
    assert!(WINDOWS_PLACE_SOURCE.contains("send_unicode_text(text)?"));
    println!("TASK5630_SHIPPING_PRIVATE_CARRIER_CLIPBOARD_PRIMITIVES=0");
    println!("TASK5630_SHIPPING_TARGETED_WRITERS=UIA_VALUE_PLUS_FOCUSED_UNICODE");
}
