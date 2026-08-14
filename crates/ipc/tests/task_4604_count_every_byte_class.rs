use ipc::metered_bytes::{
    unhooked_metered_send_path_count, MeteredByteClass, MeteredByteHooks, MeteredByteMeter,
    MeteredSendPath,
};

const REQUESTED_TOTAL: u64 = 3_151_353;
const COMPUTED_CLASS_TOTAL: u64 = 3_158_053;

const FIXTURE_SENDS: [(MeteredSendPath, u64, &str); 6] = [
    (MeteredSendPath::HeldMessageBytes, 16, "held-message-16"),
    (MeteredSendPath::Attachments, 1_048_576, "attachment-1mib"),
    (
        MeteredSendPath::StoryAndPostMedia,
        2_097_152,
        "story-media-2mib",
    ),
    (
        MeteredSendPath::MultiDeviceSyncTraffic,
        8_192,
        "device-sync-8kib",
    ),
    (
        MeteredSendPath::BackgroundCoverTick,
        4_096,
        "cover-roundtrip-4kib",
    ),
    (MeteredSendPath::PlainText, 21, "plain-text-21"),
];

fn fixture(hooks: MeteredByteHooks) -> MeteredByteMeter {
    let mut meter = MeteredByteMeter::with_hooks(hooks);
    for (path, bytes, source_id) in FIXTURE_SENDS {
        meter
            .record_send("2026-08", path, bytes, source_id)
            .unwrap_or_else(|error| panic!("TASK4604 missing hook: {error}"));
    }
    meter
}

fn fixture_without_path(disabled_path: MeteredSendPath) -> MeteredByteMeter {
    let mut meter = MeteredByteMeter::new();
    for (path, bytes, source_id) in FIXTURE_SENDS {
        if path != disabled_path {
            meter
                .record_send("2026-08", path, bytes, source_id)
                .unwrap_or_else(|error| panic!("TASK4604 missing hook: {error}"));
        }
    }
    meter
}

fn total_for(meter: &MeteredByteMeter, wanted: MeteredByteClass) -> Option<u64> {
    meter
        .class_totals()
        .into_iter()
        .find_map(|(byte_class, bytes)| (byte_class == wanted).then_some(bytes))
}

#[test]
fn task_4604_counts_every_a7_path_and_every_byte_class() {
    let meter = fixture(MeteredByteHooks::all());
    let expected = [
        (MeteredByteClass::BackgroundConnection, 4_096),
        (MeteredByteClass::Messages, 37),
        (MeteredByteClass::Attachments, 1_048_576),
        (MeteredByteClass::StoriesAndPosts, 2_097_152),
        (MeteredByteClass::Voice, 0),
        (MeteredByteClass::MultiDeviceSync, 8_192),
    ];
    let mut breaches = Vec::new();

    for (path, bytes, _) in FIXTURE_SENDS {
        println!("TASK4604_PATH path={} bytes={bytes}", path.name());
    }
    for (byte_class, expected_bytes) in expected {
        match total_for(&meter, byte_class) {
            Some(actual) => {
                println!("TASK4604_CLASS {}={actual}", byte_class.name());
                if actual != expected_bytes {
                    breaches.push(format!(
                        "{} expected {expected_bytes}, got {actual}",
                        byte_class.name()
                    ));
                }
            }
            None => {
                println!("TASK4604_CLASS {}=MISSING", byte_class.name());
                breaches.push(format!(
                    "{} class is missing rather than zero",
                    byte_class.name()
                ));
            }
        }
    }

    let total = meter.total_before_top_ups();
    println!("TASK4604_TOTAL before_top_ups={total}");
    println!(
        "TASK4604_REQUESTED_TOTAL requested={REQUESTED_TOTAL} computed={total} gap={}",
        total as i64 - REQUESTED_TOTAL as i64
    );
    if total != COMPUTED_CLASS_TOTAL {
        breaches.push(format!(
            "computed total expected {COMPUTED_CLASS_TOTAL}, got {total}"
        ));
    }

    let voice = total_for(&meter, MeteredByteClass::Voice);
    println!(
        "TASK4604_VOICE release=absent class_present={} bytes={}",
        voice.is_some(),
        voice.map_or_else(|| "MISSING".to_owned(), |bytes| bytes.to_string())
    );
    if voice != Some(0) {
        breaches.push("voice class is missing rather than zero".to_owned());
    }

    for byte_class in MeteredByteClass::ALL {
        let without = fixture(MeteredByteHooks::all().without(byte_class));
        let observed = without.total_before_top_ups();
        let difference = total - observed;
        let expected_difference =
            total_for(&meter, byte_class).expect("class catalogue is complete");
        println!(
            "TASK4604_HOOK_OFF class={} total={observed} difference={difference} expected_difference={expected_difference}",
            byte_class.name()
        );
        if difference != expected_difference {
            breaches.push(format!(
                "disabling {} changed total by {difference}, expected {expected_difference}",
                byte_class.name()
            ));
        }
    }

    for (path, expected_difference, _) in FIXTURE_SENDS {
        let without = fixture_without_path(path);
        let observed = without.total_before_top_ups();
        let difference = total - observed;
        println!(
            "TASK4604_PATH_HOOK_OFF path={} total={observed} difference={difference} expected_difference={expected_difference}",
            path.name()
        );
        if difference != expected_difference {
            breaches.push(format!(
                "disabling {} changed total by {difference}, expected {expected_difference}",
                path.name()
            ));
        }
    }

    let unhooked = unhooked_metered_send_path_count();
    println!("TASK4604_UNHOOKED_SEND_PATHS={unhooked}");
    if unhooked != 0 {
        breaches.push(format!(
            "metered send paths with no byte-class hook: {unhooked}"
        ));
    }
    if meter.records().len() != FIXTURE_SENDS.len() {
        breaches.push(format!(
            "fixture recorded {} of {} sends",
            meter.records().len(),
            FIXTURE_SENDS.len()
        ));
    }

    assert!(
        breaches.is_empty(),
        "TASK4604 contract breaches: {}",
        breaches.join("; ")
    );
}
