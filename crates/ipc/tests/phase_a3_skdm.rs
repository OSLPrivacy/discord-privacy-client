//! Phase 9-A3 Task 3: SenderKeyDistribution control-message tests.

use ipc::control_messages::{
    deserialize_sender_key_distribution, serialize_sender_key_distribution, SenderKeyDistribution,
};

#[test]
fn skdm_payload_serde_roundtrip() {
    let m = SenderKeyDistribution {
        scope_storage_key: "gc:1234".to_string(),
        chain_id: 7,
        rotation_root: [0x42; 32],
        sent_at: 1_700_000_000,
    };
    let bytes = serialize_sender_key_distribution(&m).unwrap();
    let back = deserialize_sender_key_distribution(&bytes).unwrap();
    assert_eq!(back, m);
}

#[test]
fn skdm_with_malformed_payload_rejected() {
    // CBOR can't be all 0xFFs.
    let err = deserialize_sender_key_distribution(&[0xFFu8; 32]).unwrap_err();
    assert!(format!("{err}").contains("CBOR"));
}

#[test]
fn skdm_roundtrip_preserves_long_scope_keys() {
    let m = SenderKeyDistribution {
        scope_storage_key: "server_channel:1111111111111111111:2222222222222222222".to_string(),
        chain_id: 0xABCDEF,
        rotation_root: [0x99; 32],
        sent_at: 1_800_000_000,
    };
    let bytes = serialize_sender_key_distribution(&m).unwrap();
    let back = deserialize_sender_key_distribution(&bytes).unwrap();
    assert_eq!(back, m);
}

#[test]
fn skdm_roundtrip_with_zero_chain_id_and_root() {
    let m = SenderKeyDistribution {
        scope_storage_key: "gc:0".to_string(),
        chain_id: 0,
        rotation_root: [0u8; 32],
        sent_at: 0,
    };
    let bytes = serialize_sender_key_distribution(&m).unwrap();
    let back = deserialize_sender_key_distribution(&bytes).unwrap();
    assert_eq!(back, m);
}

#[test]
fn skdm_truncated_bytes_rejected() {
    let m = SenderKeyDistribution {
        scope_storage_key: "gc:trunc".to_string(),
        chain_id: 1,
        rotation_root: [1u8; 32],
        sent_at: 1,
    };
    let bytes = serialize_sender_key_distribution(&m).unwrap();
    // Lop off the last 10 bytes.
    let trunc = &bytes[..bytes.len().saturating_sub(10)];
    assert!(deserialize_sender_key_distribution(trunc).is_err());
}
