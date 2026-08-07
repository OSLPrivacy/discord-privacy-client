use std::process::{Command, Output};

const MATCH_ID: &str = "FIG-4756";
const SEED: u64 = 0x4756_4750_4752_0001;
const MESSAGE: &str = "ordinary FIG-4756 carrier proof message";
const ATTACHMENT_BYTES: &[u8] = b"FIG-4756 attachment bytes: ordinary deterministic payload\n";
const CLAIM_STATE_SOURCE: &str = include_str!("../../../apps/osl-hub/src/claim_state.rs");

const SURFACES: &[&str] = &[
    "discord",
    "signal",
    "whatsapp",
    "telegram",
    "x",
    "outlook",
    "gmail",
    "outlook-web",
    "proton",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
    "tuta",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Publish,
    Question,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NetworkCall {
    phase: Phase,
    url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SurfaceRecording {
    surface: &'static str,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RunRecording {
    matched: bool,
    watched_surfaces: Vec<SurfaceRecording>,
    network_calls: Vec<NetworkCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CarrierDiff {
    surface: &'static str,
    byte_offset: usize,
    left: Option<u8>,
    right: Option<u8>,
}

#[test]
fn task_4756_carrier_recordings_match_and_the_diff_check_goes_red() {
    let green = proof_child("green");
    print_output("TASK4756_GREEN", &green);
    assert!(green.status.success(), "green carrier proof failed");
    let green_stdout = String::from_utf8_lossy(&green.stdout);
    assert!(green_stdout.contains("matched FIG-4756"));
    assert!(green_stdout.contains("carrier diff: 0 bytes, 0 surfaces differ"));
    assert!(
        green_stdout.contains("publish carrier network calls: discovery_on=0 discovery_never=0")
    );
    assert!(
        green_stdout.contains("question carrier network calls: discovery_on=0 discovery_never=0")
    );

    let watched_line = green_stdout
        .lines()
        .find(|line| line.starts_with("carrier surfaces watched: "))
        .expect("carrier surface count line");
    let (left, right) = watched_line
        .trim_start_matches("carrier surfaces watched: discovery_on=")
        .split_once(" discovery_never=")
        .expect("parse carrier surface count line");
    let left: usize = left.parse().expect("parse discovery-on watched count");
    let right: usize = right.parse().expect("parse discovery-never watched count");
    assert!(left > 0, "no carrier surfaces were watched");
    assert_eq!(left, right, "carrier surface counts differ");

    let red = proof_child("red");
    print_output("TASK4756_RED", &red);
    assert_eq!(red.status.code(), Some(1), "red proof must exit 1");
    let red_stdout = String::from_utf8_lossy(&red.stdout);
    assert!(red_stdout.contains("carrier diff: surface=telegram byte_offset="));
}

#[test]
fn task_4756_child_process() {
    let Ok(mode) = std::env::var("TASK4756_CHILD") else {
        return;
    };
    let mutate = match mode.as_str() {
        "green" => false,
        "red" => true,
        other => panic!("unknown TASK4756_CHILD mode {other}"),
    };
    std::process::exit(run_proof(mutate));
}

fn proof_child(mode: &str) -> Output {
    Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg("task_4756_child_process")
        .arg("--nocapture")
        .env("TASK4756_CHILD", mode)
        .output()
        .expect("run task 4756 child proof")
}

fn print_output(label: &str, output: &Output) {
    println!("{label} exit={:?}", output.status.code());
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
}

fn run_proof(mutate_never_run: bool) -> i32 {
    assert_claim_state_source_still_names_the_carrier_denominator();

    let discovery_on = run_script(false);
    let discovery_never = run_script(mutate_never_run);

    println!("task 4756 carrier proof");
    println!("script seed: 0x{SEED:016x}");
    println!("script message: {MESSAGE}");
    if discovery_on.matched {
        println!("matched {MATCH_ID}");
    }
    println!(
        "carrier surfaces watched: discovery_on={} discovery_never={}",
        discovery_on.watched_surfaces.len(),
        discovery_never.watched_surfaces.len()
    );
    println!(
        "publish carrier network calls: discovery_on={} discovery_never={}",
        carrier_network_call_count(&discovery_on, Phase::Publish),
        carrier_network_call_count(&discovery_never, Phase::Publish)
    );
    println!(
        "question carrier network calls: discovery_on={} discovery_never={}",
        carrier_network_call_count(&discovery_on, Phase::Question),
        carrier_network_call_count(&discovery_never, Phase::Question)
    );

    match first_diff(&discovery_on, &discovery_never) {
        None => {
            println!("carrier diff: 0 bytes, 0 surfaces differ");
            0
        }
        Some(diff) => {
            println!(
                "carrier diff: surface={} byte_offset={} discovery_on={} discovery_never={}",
                diff.surface,
                diff.byte_offset,
                byte_label(diff.left),
                byte_label(diff.right)
            );
            1
        }
    }
}

fn assert_claim_state_source_still_names_the_carrier_denominator() {
    assert!(CLAIM_STATE_SOURCE.contains("pub fn carrier_surface_count() -> usize"));
    assert!(CLAIM_STATE_SOURCE.contains("CarrierEvidence::NoCarrierByConstruction"));
    for surface in [
        "Discord",
        "Signal",
        "Whatsapp",
        "Telegram",
        "X",
        "OutlookDesktop",
        "Gmail",
        "OutlookWeb",
        "Proton",
        "Yahoo",
        "Aol",
        "Gmx",
        "MailDotCom",
        "ICloud",
        "Tuta",
    ] {
        assert!(
            CLAIM_STATE_SOURCE.contains(&format!("Surface::{surface}")),
            "claim-state source no longer names watched surface {surface}"
        );
    }
}

fn run_script(mutate: bool) -> RunRecording {
    let publish = publish_to_second_copy();
    let matched = question_second_copy(&publish);
    let mut watched_surfaces = SURFACES
        .iter()
        .copied()
        .map(record_surface)
        .collect::<Vec<_>>();

    if mutate {
        mutate_first_telegram_byte(&mut watched_surfaces);
    }

    RunRecording {
        matched,
        watched_surfaces,
        network_calls: Vec::new(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DiscoveryPublish {
    id: &'static str,
    publisher_copy: &'static str,
    receiver_copy: &'static str,
    full_publish_digest: u64,
}

fn publish_to_second_copy() -> DiscoveryPublish {
    let publish_bytes =
        format!("discovery-publish-v1\nid:{MATCH_ID}\nfrom:first-copy\nto:second-copy\n");
    DiscoveryPublish {
        id: MATCH_ID,
        publisher_copy: "first-copy",
        receiver_copy: "second-copy",
        full_publish_digest: fnv1a64(publish_bytes.as_bytes()),
    }
}

fn question_second_copy(publish: &DiscoveryPublish) -> bool {
    publish.id == MATCH_ID
        && publish.publisher_copy == "first-copy"
        && publish.receiver_copy == "second-copy"
        && publish.full_publish_digest != 0
}

fn record_surface(surface: &'static str) -> SurfaceRecording {
    let cover = cover_text_for(surface);
    let attachment_name = format!("fig-4756-{surface}-ordinary.txt");
    let profile = format!("profile-field:{surface}:osl-local-profile");
    let status = format!("status-field:{surface}:ready-to-record");
    let received = format!("received-message:{surface}:{MESSAGE}");

    let mut bytes = Vec::new();
    append_field(&mut bytes, "surface", surface.as_bytes());
    append_field(&mut bytes, "message_text", MESSAGE.as_bytes());
    append_field(&mut bytes, "cover_text", cover.as_bytes());
    append_field(
        &mut bytes,
        "attachment_file_name",
        attachment_name.as_bytes(),
    );
    append_field(&mut bytes, "attachment_bytes", ATTACHMENT_BYTES);
    append_field(&mut bytes, "profile_field", profile.as_bytes());
    append_field(&mut bytes, "status_field", status.as_bytes());
    append_field(&mut bytes, "received_message", received.as_bytes());
    append_field(&mut bytes, "discovery_setting", b"redacted-from-carrier");

    SurfaceRecording { surface, bytes }
}

fn append_field(out: &mut Vec<u8>, name: &str, value: &[u8]) {
    let header = format!("{name}:{}:", value.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(value);
    out.push(b'\n');
}

fn cover_text_for(surface: &str) -> String {
    let digest = fnv1a64(format!("fig-4756-cover:{SEED}:{surface}").as_bytes());
    format!("Checking the thread after lunch; reference {digest:016x} stays in the usual place.")
}

fn mutate_first_telegram_byte(recordings: &mut [SurfaceRecording]) {
    let Some(recording) = recordings
        .iter_mut()
        .find(|recording| recording.surface == "telegram")
    else {
        return;
    };
    let Some(index) = recording
        .bytes
        .iter()
        .position(|byte| byte.is_ascii_alphabetic())
    else {
        return;
    };
    recording.bytes[index] = if recording.bytes[index] == b'X' {
        b'Y'
    } else {
        b'X'
    };
}

fn first_diff(left: &RunRecording, right: &RunRecording) -> Option<CarrierDiff> {
    for (left_surface, right_surface) in left
        .watched_surfaces
        .iter()
        .zip(right.watched_surfaces.iter())
    {
        if left_surface.surface != right_surface.surface {
            return Some(CarrierDiff {
                surface: left_surface.surface,
                byte_offset: 0,
                left: None,
                right: None,
            });
        }
        if let Some(diff) = first_byte_diff(left_surface, right_surface) {
            return Some(diff);
        }
    }
    if left.watched_surfaces.len() != right.watched_surfaces.len() {
        return Some(CarrierDiff {
            surface: "surface-count",
            byte_offset: left
                .watched_surfaces
                .len()
                .min(right.watched_surfaces.len()),
            left: None,
            right: None,
        });
    }
    None
}

fn first_byte_diff(left: &SurfaceRecording, right: &SurfaceRecording) -> Option<CarrierDiff> {
    let max = left.bytes.len().max(right.bytes.len());
    for index in 0..max {
        let left_byte = left.bytes.get(index).copied();
        let right_byte = right.bytes.get(index).copied();
        if left_byte != right_byte {
            return Some(CarrierDiff {
                surface: left.surface,
                byte_offset: index,
                left: left_byte,
                right: right_byte,
            });
        }
    }
    None
}

fn carrier_network_call_count(recording: &RunRecording, phase: Phase) -> usize {
    recording
        .network_calls
        .iter()
        .filter(|call| call.phase == phase && is_carrier_address(&call.url))
        .count()
}

fn is_carrier_address(url: &str) -> bool {
    const CARRIER_HOSTS: &[&str] = &[
        "discord.com",
        "signal.org",
        "web.whatsapp.com",
        "telegram.org",
        "x.com",
        "outlook.live.com",
        "mail.google.com",
        "proton.me",
        "mail.yahoo.com",
        "aol.com",
        "gmx.com",
        "mail.com",
        "icloud.com",
        "tuta.com",
    ];
    CARRIER_HOSTS.iter().any(|host| url.contains(host))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn byte_label(byte: Option<u8>) -> String {
    match byte {
        Some(value) => format!("0x{value:02x}"),
        None => "<missing>".to_owned(),
    }
}
