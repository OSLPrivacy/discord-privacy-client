use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::wire_v2::{
    decrypt_v3_for_sender, encrypt_v3, RecipientV3, MSG_TYPE_NATIVE_OVERLAY_RELAY, WIRE_VERSION_V3,
};

const FUTURE_WIRE_VERSION: u8 = 0x07;
const EXACT_PRIVATE_MESSAGE: &str = "TASK4015 exact private message that only a newer OSL opens";

#[derive(Clone)]
struct QueuedPrivateRow {
    id: String,
    wire: String,
}

struct OpenOutcome {
    opened: Vec<String>,
    unrecognized_wire_rows: usize,
}

fn raw_wire(wire: &str) -> Vec<u8> {
    let body = wire.strip_prefix("DPC0::").expect("wire keeps DPC0 prefix");
    STANDARD.decode(body).expect("wire body is base64")
}

fn wire_from_raw(raw: &[u8]) -> String {
    format!("DPC0::{}", STANDARD.encode(raw))
}

fn marked_with_version(wire: &str, version: u8) -> String {
    let mut raw = raw_wire(wire);
    raw[0] = version;
    wire_from_raw(&raw)
}

fn open_private_queue(
    queue: &mut Vec<QueuedPrivateRow>,
    known_versions: &[u8],
    receiver: &keystore::Identity,
    sender: &keystore::Identity,
) -> OpenOutcome {
    let mut opened = Vec::new();
    let mut retained = Vec::new();
    let mut unrecognized_wire_rows = 0usize;

    for row in queue.drain(..) {
        let raw = raw_wire(&row.wire);
        let version = raw[0];
        if !known_versions.contains(&version) {
            unrecognized_wire_rows += 1;
            retained.push(row);
            continue;
        }

        // This fixture models a newer OSL that recognizes FUTURE_WIRE_VERSION
        // as the same encrypted relay layout used here. The older build never
        // reaches this normalization because it does not know that version.
        let decode_wire = if version == WIRE_VERSION_V3 {
            row.wire.clone()
        } else {
            let mut known_raw = raw;
            known_raw[0] = WIRE_VERSION_V3;
            wire_from_raw(&known_raw)
        };
        match decrypt_v3_for_sender(
            &decode_wire,
            &receiver.x25519_secret,
            &receiver.mlkem_decapsulation_key(),
            &sender.x25519_public,
        ) {
            Ok(recovered) if recovered.msg_type == MSG_TYPE_NATIVE_OVERLAY_RELAY => {
                opened
                    .push(String::from_utf8(recovered.plaintext).expect("fixture plaintext UTF-8"));
            }
            _ => retained.push(row),
        }
    }

    *queue = retained;
    OpenOutcome {
        opened,
        unrecognized_wire_rows,
    }
}

#[test]
fn task_4015_unknown_version_is_reported_retained_then_opened_by_compatible_build() {
    let sender = keystore::generate_identity("task-4015-sender".to_owned());
    let receiver = keystore::generate_identity("task-4015-receiver".to_owned());
    let v3_wire = encrypt_v3(
        &sender.x25519_secret,
        &sender.x25519_public,
        &[RecipientV3 {
            x25519_pub: receiver.x25519_public,
            mlkem_pub: receiver.mlkem_encapsulation_key(),
        }],
        MSG_TYPE_NATIVE_OVERLAY_RELAY,
        EXACT_PRIVATE_MESSAGE.as_bytes(),
    )
    .expect("seal task 4015 private fixture");
    let too_new_wire = marked_with_version(&v3_wire, FUTURE_WIRE_VERSION);
    assert_eq!(raw_wire(&too_new_wire)[0], FUTURE_WIRE_VERSION);

    let expected_row_id = "task-4015-row";
    let mut queue = vec![QueuedPrivateRow {
        id: expected_row_id.to_owned(),
        wire: too_new_wire,
    }];
    let old_build = open_private_queue(&mut queue, &[WIRE_VERSION_V3], &receiver, &sender);
    let opened_private_messages = old_build.opened.len();
    let queue_after_too_new = queue.len();

    assert_eq!(
        opened_private_messages, 0,
        "a build that does not know the wire version must open zero private messages"
    );
    assert_eq!(
        old_build.unrecognized_wire_rows, 1,
        "the too-new row must be counted as unrecognized"
    );
    assert!(
        queue.iter().any(|row| row.id == expected_row_id),
        "TASK4015_MISSING_FROM_QUEUE={expected_row_id} TASK4015_QUEUE_AFTER_TOO_NEW={queue_after_too_new}"
    );
    assert_eq!(
        queue_after_too_new, 1,
        "the too-new row must stay queued for a compatible build"
    );
    assert_eq!(queue[0].id, expected_row_id);

    let newer_build = open_private_queue(
        &mut queue,
        &[WIRE_VERSION_V3, FUTURE_WIRE_VERSION],
        &receiver,
        &sender,
    );
    let newer_exact_matches = newer_build
        .opened
        .iter()
        .filter(|opened| opened.as_str() == EXACT_PRIVATE_MESSAGE)
        .count();

    println!("TASK4015_TOO_NEW_VERSION={FUTURE_WIRE_VERSION}");
    println!("TASK4015_OLD_BUILD_OPENED_PRIVATE_MESSAGES={opened_private_messages}");
    println!(
        "TASK4015_OLD_BUILD_UNRECOGNIZED_WIRE_ROWS={}",
        old_build.unrecognized_wire_rows
    );
    println!("TASK4015_QUEUE_AFTER_TOO_NEW={queue_after_too_new}");
    println!(
        "TASK4015_NEWER_BUILD_OPENED_PRIVATE_MESSAGES={}",
        newer_build.opened.len()
    );
    println!("TASK4015_NEWER_BUILD_EXACT_MATCHES={newer_exact_matches}");

    assert_eq!(
        newer_build.opened.len(),
        1,
        "a build that knows the version must open exactly one private message"
    );
    assert_eq!(
        newer_exact_matches, 1,
        "the compatible build must open the exact marked private text"
    );
    assert!(
        queue.is_empty(),
        "the compatible build consumes the retained row after opening it"
    );
}
