use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use crypto::x25519;
use ipc::wire_v2::{
    decrypt_v3_for_sender, encrypt_v3, enforce_v3_recipient_ceiling,
    max_v3_recipients_for_plaintext_len, pubkey_hash_prefix, RecipientV3, MSG_TYPE_CONTENT,
    SLOT_V3_BYTES, V3_FIXED_WIRE_BYTES, V3_GLOBAL_HEADER_BYTES, V3_WIRE_BLOB_MAX_BYTES,
};
use keystore::{generate_identity, Identity};

fn recipient(identity: &Identity) -> RecipientV3 {
    RecipientV3 {
        x25519_pub: identity.x25519_public,
        mlkem_pub: identity.mlkem_encapsulation_key(),
    }
}

fn raw_wire(wire: &str) -> Vec<u8> {
    STANDARD
        .decode(wire.strip_prefix("DPC0::").expect("wire prefix"))
        .expect("wire body decodes")
}

fn recipient_slot_count(wire: &str) -> usize {
    raw_wire(wire)[V3_GLOBAL_HEADER_BYTES - 1] as usize
}

fn open_for(identity: &Identity, sender: &Identity, wire: &str) -> Vec<u8> {
    decrypt_v3_for_sender(
        wire,
        &identity.x25519_secret,
        &identity.mlkem_decapsulation_key(),
        &sender.x25519_public,
    )
    .expect("recipient opens v3 wire from expected sender")
    .plaintext
}

#[test]
fn task_4804_v3_carries_many_devices_and_refuses_over_ceiling() {
    let payload = b"LANTERN-4804";
    let six_devices: Vec<_> = (0..6)
        .map(|index| generate_identity(format!("task4804-device-{index}")))
        .collect();
    let sender = &six_devices[0];
    let recipients: Vec<_> = six_devices.iter().map(recipient).collect();
    let wire = encrypt_v3(
        &sender.x25519_secret,
        &sender.x25519_public,
        &recipients,
        MSG_TYPE_CONTENT,
        payload,
    )
    .expect("six-device LANTERN-4804 wire encrypts");
    assert_eq!(recipient_slot_count(&wire), six_devices.len());
    for device in &six_devices {
        assert_eq!(open_for(device, sender, &wire), payload);
    }
    println!(
        "TASK4804_SIX_DEVICES marker={} accepted_recipients={} read_back={}",
        std::str::from_utf8(payload).unwrap(),
        recipient_slot_count(&wire),
        six_devices.len()
    );

    let outsider = generate_identity("task4804-outsider".to_owned());
    let outsider_result = decrypt_v3_for_sender(
        &wire,
        &outsider.x25519_secret,
        &outsider.mlkem_decapsulation_key(),
        &sender.x25519_public,
    );
    assert!(outsider_result.is_err());
    let outsider_hash = pubkey_hash_prefix(&x25519::derive_public(&outsider.x25519_secret));
    assert!(!raw_wire(&wire)
        [V3_GLOBAL_HEADER_BYTES..V3_GLOBAL_HEADER_BYTES + six_devices.len() * SLOT_V3_BYTES]
        .chunks_exact(SLOT_V3_BYTES)
        .any(|slot| slot[..8] == outsider_hash[..]));
    println!("TASK4804_OUTSIDER_OPEN_FAILED error=not a recipient of this message");

    let ceiling = max_v3_recipients_for_plaintext_len(payload.len());
    println!(
        "TASK4804_CEILING computation=({V3_WIRE_BLOB_MAX_BYTES} - {V3_FIXED_WIRE_BYTES} - {}) / {SLOT_V3_BYTES} = {ceiling}",
        payload.len()
    );

    let sixty_devices: Vec<_> = (0..60)
        .map(|index| generate_identity(format!("task4804-sixty-{index}")))
        .collect();
    let sixty_recipients: Vec<_> = sixty_devices.iter().map(recipient).collect();
    let mut bytes_left = false;
    let refused = encrypt_v3(
        &sender.x25519_secret,
        &sender.x25519_public,
        &sixty_recipients,
        MSG_TYPE_CONTENT,
        payload,
    )
    .map(|wire| {
        bytes_left = true;
        wire
    })
    .expect_err("60 devices are refused before wire bytes are returned")
    .to_string();
    assert!(!bytes_left);
    assert!(refused.contains("too many devices for one message"));
    assert!(refused.contains("60"));
    assert!(refused.contains(&ceiling.to_string()));
    println!("TASK4804_REFUSED_60 bytes_left={bytes_left} refusal={refused}",);

    enforce_v3_recipient_ceiling(ceiling, payload.len()).expect("exact ceiling is allowed");
    let exact_devices: Vec<_> = (0..ceiling)
        .map(|index| generate_identity(format!("task4804-ceiling-{index}")))
        .collect();
    let exact_sender = &exact_devices[0];
    let exact_recipients: Vec<_> = exact_devices.iter().map(recipient).collect();
    let exact_wire = encrypt_v3(
        &exact_sender.x25519_secret,
        &exact_sender.x25519_public,
        &exact_recipients,
        MSG_TYPE_CONTENT,
        payload,
    )
    .expect("exact ceiling recipient count encrypts");
    let exact_raw_len = raw_wire(&exact_wire).len();
    assert!(exact_raw_len <= V3_WIRE_BLOB_MAX_BYTES);
    assert_eq!(recipient_slot_count(&exact_wire), ceiling);
    for device in &exact_devices {
        assert_eq!(open_for(device, exact_sender, &exact_wire), payload);
    }
    println!(
        "TASK4804_EXACT_CEILING accepted_recipients={ceiling} read_back={} raw_bytes={exact_raw_len} limit={V3_WIRE_BLOB_MAX_BYTES}",
        exact_devices.len()
    );
}
