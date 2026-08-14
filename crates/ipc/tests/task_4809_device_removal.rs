use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use crypto::{ml_kem_768, x25519};
use ipc::wire_v2::{
    decrypt_v3, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT, RECIPIENT_HASH_PREFIX_LEN,
    SLOT_V3_BYTES, WIRE_VERSION_V3,
};
use sha2::{Digest, Sha256};

struct Device {
    id: &'static str,
    x_sk: x25519::SecretKey,
    x_pk: x25519::PublicKey,
    mlkem_sk: ml_kem_768::DecapsulationKey,
    mlkem_pk: ml_kem_768::EncapsulationKey,
}

fn device(id: &'static str) -> Device {
    let (x_sk, x_pk) = x25519::generate_keypair();
    let (mlkem_sk, mlkem_pk) = ml_kem_768::generate_keypair();
    Device {
        id,
        x_sk,
        x_pk,
        mlkem_sk,
        mlkem_pk,
    }
}

fn recipient(device: &Device) -> RecipientV3 {
    RecipientV3 {
        x25519_pub: device.x_pk,
        mlkem_pub: device.mlkem_pk.clone(),
    }
}

fn recipient_hash(device: &Device) -> [u8; RECIPIENT_HASH_PREFIX_LEN] {
    let digest = Sha256::digest(device.x_pk.as_bytes());
    let mut out = [0u8; RECIPIENT_HASH_PREFIX_LEN];
    out.copy_from_slice(&digest[..RECIPIENT_HASH_PREFIX_LEN]);
    out
}

fn inspect_slots(wire: &str, device: &Device) -> (usize, usize) {
    let raw = STANDARD
        .decode(wire.strip_prefix("DPC0::").expect("wire has DPC0 prefix"))
        .expect("wire base64 decodes");
    assert_eq!(raw[0], WIRE_VERSION_V3);
    let slot_count = raw[34] as usize;
    let removed_hash = recipient_hash(device);
    let mut matching_removed_slots = 0usize;
    for slot in 0..slot_count {
        let base = 35 + slot * SLOT_V3_BYTES;
        if raw[base..base + RECIPIENT_HASH_PREFIX_LEN] == removed_hash {
            matching_removed_slots += 1;
        }
    }
    (slot_count, matching_removed_slots)
}

#[test]
fn task_4809_removed_device_is_cut_off_but_old_bytes_still_open() {
    let (sender_sk, sender_pk) = x25519::generate_keypair();
    let device_1 = device("device-1");
    let removed = device("device-2");
    let device_3 = device("device-3");
    let before_removal = vec![
        recipient(&device_1),
        recipient(&removed),
        recipient(&device_3),
    ];
    let after_removal = if std::env::var("OSL_TASK4809_KEEP_REMOVED_DEVICE")
        .ok()
        .as_deref()
        == Some("1")
    {
        before_removal.clone()
    } else {
        vec![recipient(&device_1), recipient(&device_3)]
    };

    let old_wires: Vec<String> = (1..=5)
        .map(|index| {
            encrypt_v3(
                &sender_sk,
                &sender_pk,
                &before_removal,
                MSG_TYPE_CONTENT,
                format!("HERON-4809 held before removal {index}").as_bytes(),
            )
            .expect("old message encrypts to all three paired devices")
        })
        .collect();

    let mut old_messages_opened_by_removed = 0usize;
    for (index, wire) in old_wires.iter().enumerate() {
        let opened = decrypt_v3(wire, &removed.x_sk, &removed.mlkem_sk)
            .expect("removed device still opens bytes it already holds");
        assert_eq!(
            opened.plaintext,
            format!("HERON-4809 held before removal {}", index + 1).into_bytes()
        );
        old_messages_opened_by_removed += 1;
    }

    let mut post_removal_slot_counts = Vec::new();
    let mut removed_slot_counts = Vec::new();
    let mut removed_new_opens = 0usize;
    for index in 1..=10 {
        let plaintext = format!("HERON-4809 after removal {index}");
        let wire = encrypt_v3(
            &sender_sk,
            &sender_pk,
            &after_removal,
            MSG_TYPE_CONTENT,
            plaintext.as_bytes(),
        )
        .expect("post-removal message encrypts to published devices");
        let (slots, removed_slots) = inspect_slots(&wire, &removed);
        post_removal_slot_counts.push(slots);
        removed_slot_counts.push(removed_slots);
        if decrypt_v3(&wire, &removed.x_sk, &removed.mlkem_sk).is_ok() {
            removed_new_opens += 1;
        }
        println!(
            "TASK4809_CRYPTO message=HERON-4809-{index:02} recipient_slots={slots} removed_device={} removed_device_slots={removed_slots}",
            removed.id
        );
        assert_eq!(
            slots, 2,
            "TASK4809_BREAK message=HERON-4809-{index:02} slots={slots}"
        );
        assert_eq!(removed_slots, 0);
    }

    let screen_state = format!(
        "This device can still open {old_messages_opened_by_removed} messages it already holds."
    );
    println!(
        "TASK4809_CRYPTO old_messages_opened_by_removed={old_messages_opened_by_removed} removed_device_new_opens={removed_new_opens} screen_state=\"{screen_state}\" post_removal_slot_counts={} removed_slot_counts={}",
        post_removal_slot_counts
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(","),
        removed_slot_counts
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );

    assert_eq!(old_messages_opened_by_removed, 5);
    assert_eq!(removed_new_opens, 0);
    assert_eq!(
        screen_state,
        "This device can still open 5 messages it already holds."
    );
}
