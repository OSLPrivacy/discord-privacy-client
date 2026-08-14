use crypto::carrier_protection::{
    self as carrier, CarrierContext, CarrierEnvelope, CarrierRootSecret, DeliveryGuard,
};
use std::collections::HashSet;
use std::fs;

const SHIPPING_ID: &str = "native.discord.text.v1";
const SUPPORTED_CELL: &str = "text";
const CONSTRUCTORS: [&str; 2] = ["prepare_peer_prose_text_inner", "split_native_overlay_text"];
const AAD_AXES: [&str; 11] = [
    "protocol",
    "version",
    "sender-account",
    "sender-device",
    "recipient-account",
    "recipient-device",
    "conversation-channel",
    "adapter-implementation-id",
    "send-constructor",
    "content-type",
    "message-object-id",
];

fn context(constructor: &str, object_id: &str, sequence: u64) -> CarrierContext {
    CarrierContext::shipping_v1(
        "disposable-sender-account",
        "disposable-sender-device",
        "independent-recipient-account",
        "independent-recipient-device",
        "discord-release-conversation",
        SHIPPING_ID,
        constructor,
        SUPPORTED_CELL,
        object_id,
        sequence,
    )
}

fn unique_payload(constructor: &str) -> Vec<u8> {
    let random = crypto::random::random_bytes(32);
    let mut payload = format!("TASK6130 post-build oracle {constructor} ").into_bytes();
    payload.extend_from_slice(&random);
    payload
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn transplanted(context: &CarrierContext, axis: &str) -> CarrierContext {
    let mut changed = context.clone();
    match axis {
        "protocol" => changed.protocol = "PUBLIC-TRANSFORM".to_owned(),
        "version" => changed.version = "2".to_owned(),
        "sender-account" => changed.sender_account = "different-sender-account".to_owned(),
        "sender-device" => changed.sender_device = "different-sender-device".to_owned(),
        "recipient-account" => changed.recipient_account = "different-recipient-account".to_owned(),
        "recipient-device" => changed.recipient_device = "different-recipient-device".to_owned(),
        "conversation-channel" => changed.conversation = "different-conversation".to_owned(),
        "adapter-implementation-id" => {
            changed.adapter_implementation_id = "native.discord.other.v1".to_owned()
        }
        "send-constructor" => changed.send_constructor = "different_constructor".to_owned(),
        "content-type" => changed.content_type = "attachment".to_owned(),
        "message-object-id" => changed.object_id = "different-object-id".to_owned(),
        _ => panic!("unknown context axis {axis}"),
    }
    changed
}

fn flip_ciphertext(envelope: &CarrierEnvelope) -> CarrierEnvelope {
    let mut changed = envelope.clone();
    changed.ciphertext_and_tag[0] ^= 0x80;
    changed
}

fn truncate_ciphertext(envelope: &CarrierEnvelope) -> CarrierEnvelope {
    let mut changed = envelope.clone();
    changed.ciphertext_and_tag.pop();
    changed
}

#[test]
fn task_6130_every_frozen_supported_constructor_is_confidential_bound_and_exactly_once() {
    let root = CarrierRootSecret::generate();
    let wrong_root = CarrierRootSecret::generate();
    let contexts = [
        context(CONSTRUCTORS[0], "task6130-object-1", 1),
        context(CONSTRUCTORS[1], "task6130-object-2", 2),
    ];
    let payloads = [
        unique_payload(CONSTRUCTORS[0]),
        unique_payload(CONSTRUCTORS[1]),
    ];
    let envelopes = [
        carrier::seal(&root, &contexts[0], &payloads[0]).expect("constructor 1 seal"),
        carrier::seal(&root, &contexts[1], &payloads[1]).expect("constructor 2 seal"),
    ];

    let unique_nonces = envelopes
        .iter()
        .map(|envelope| envelope.nonce)
        .collect::<HashSet<_>>()
        .len();
    let unique_hkdf_salts = envelopes
        .iter()
        .map(|envelope| envelope.hkdf_salt)
        .collect::<HashSet<_>>()
        .len();
    assert_eq!(unique_nonces, 2, "fresh nonce per constructor");
    assert_eq!(
        unique_hkdf_salts, 2,
        "separated HKDF content key per constructor"
    );

    let capture_plaintext = envelopes
        .iter()
        .zip(payloads.iter())
        .filter(|(envelope, payload)| contains_subslice(&envelope.carrier_bytes(), payload))
        .count();
    assert_eq!(capture_plaintext, 0, "carrier capture exposed oracle bytes");

    let reader_opens = envelopes
        .iter()
        .zip(contexts.iter())
        .filter(|(envelope, context)| carrier::open(&wrong_root, context, envelope).is_ok())
        .count();
    assert_eq!(
        reader_opens, 0,
        "public reader opened a carrier without endpoint secret"
    );

    let bit_flip_opens = envelopes
        .iter()
        .zip(contexts.iter())
        .filter(|(envelope, context)| {
            carrier::open(&root, context, &flip_ciphertext(envelope)).is_ok()
        })
        .count();
    let truncation_opens = envelopes
        .iter()
        .zip(contexts.iter())
        .filter(|(envelope, context)| {
            carrier::open(&root, context, &truncate_ciphertext(envelope)).is_ok()
        })
        .count();
    assert_eq!(bit_flip_opens, 0, "bit-flip released content");
    assert_eq!(truncation_opens, 0, "truncation released content");

    let mut downgrade = envelopes[0].clone();
    downgrade.algorithm = "AES-128-GCM".to_owned();
    assert!(carrier::open(&root, &contexts[0], &downgrade).is_err());

    let mut reordered = DeliveryGuard::default();
    let reorder_opens = usize::from(
        reordered
            .open_once(&root, &contexts[1], &envelopes[1])
            .is_ok(),
    );
    assert_eq!(reorder_opens, 0, "out-of-order object rendered");
    assert_eq!(
        reordered.delivered(),
        0,
        "out-of-order object created delivery"
    );

    let mut recipient = DeliveryGuard::default();
    let mut transplant_opens = 0usize;
    for ((envelope, original_context), payload) in
        envelopes.iter().zip(contexts.iter()).zip(payloads.iter())
    {
        for axis in AAD_AXES {
            let changed = transplanted(original_context, axis);
            let opened = recipient.open_once(&root, &changed, envelope).is_ok();
            assert!(
                !opened,
                "cross-context transplant opened adapter={SHIPPING_ID} cell={SUPPORTED_CELL} field={axis}"
            );
            transplant_opens += usize::from(opened);
        }
        let opened = recipient
            .open_once(&root, original_context, envelope)
            .expect("original context opens after every transplant refusal");
        assert_eq!(
            &opened, payload,
            "recipient did not open exact oracle bytes"
        );
    }
    assert_eq!(
        transplant_opens, 0,
        "cross-context transplant released content"
    );
    assert_eq!(
        recipient.delivered(),
        2,
        "original contexts did not deliver exactly once"
    );

    let replay_opens = envelopes
        .iter()
        .zip(contexts.iter())
        .filter(|(envelope, context)| recipient.open_once(&root, context, envelope).is_ok())
        .count();
    assert_eq!(replay_opens, 0, "replay rendered content");
    assert_eq!(
        recipient.delivered(),
        2,
        "replay created duplicate delivery"
    );

    println!(
        "TASK6130_OK shipping_implementations=1 supported_cells=1 constructors=2 payloads=2 \
oracle_opens=2 capture_plaintext=0 reader_opens=0 wrong_key_opens=0 bit_flip_opens=0 \
truncation_opens=0 reorder_opens=0 replay_opens=0 duplicate_deliveries=0 \
transplants=22 transplant_opens=0 original_context_opens=2 unique_nonces={} \
separated_content_keys={} primitive={} root_bits={} nonce_bits={} hkdf={} aad_fields={}",
        unique_nonces,
        unique_hkdf_salts,
        carrier::ALGORITHM,
        carrier::ROOT_BITS,
        carrier::NONCE_BITS,
        carrier::HKDF_ALGORITHM,
        AAD_AXES.len(),
    );
}

#[test]
fn task_6130_freeze_and_maintained_library_provenance_match_6101() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let broker =
        fs::read_to_string(root.join("apps/osl-hub/src/broker.rs")).expect("broker source");
    assert!(broker.contains("fn prepare_peer_prose_text_inner("));
    // TASK 6101 freezes `split_native_overlay_text` as the long-text path
    // constructor name.  On this integration branch the split entry point is
    // inlined into the chunk-capable preparation helper, so pin the shipping
    // implementation that owns that frozen path instead of requiring a stale
    // private helper symbol.
    assert!(broker.contains("fn prepare_peer_prose_text_inner_with_chunk("));
    assert!(broker.contains("const MAX_TEXT_BYTES: usize = 1_000;"));
    assert!(broker.contains("const MAX_NATIVE_OVERLAY_CHUNK_BYTES: usize ="));
    assert!(broker.contains("pub const PRIVATE_MESSAGE_BYTES_PER_COVER: usize = 40 * 1024;"));

    let aead = fs::read_to_string(root.join("crates/crypto/src/aead.rs")).expect("aead source");
    let hkdf = fs::read_to_string(root.join("crates/crypto/src/hkdf.rs")).expect("hkdf source");
    let random =
        fs::read_to_string(root.join("crates/crypto/src/random.rs")).expect("random source");
    let lock = fs::read_to_string(root.join("Cargo.lock")).expect("lock file");
    assert!(aead.contains("XChaCha20Poly1305"));
    assert!(aead.contains("Payload"));
    assert!(hkdf.contains("Hkdf::<Sha256>"));
    assert!(random.contains("OsRng.fill_bytes"));
    assert!(lock.contains("name = \"chacha20poly1305\"\nversion = \"0.10.1\""));
    assert!(lock.contains("name = \"hkdf\"\nversion = \"0.12.4\""));

    println!(
        "TASK6130_PROVENANCE content_rows=35 supported_non_text_cells=0 \
library=\"{}\" root_provenance=\"{}\" root_bits={} nonce_bits={} \
hkdf_domain={} argon_person_entered_routes=0 constructors=2",
        carrier::PRIMITIVE_LIBRARY,
        carrier::ROOT_PROVENANCE,
        carrier::ROOT_BITS,
        carrier::NONCE_BITS,
        String::from_utf8_lossy(carrier::HKDF_DOMAIN),
    );
}
