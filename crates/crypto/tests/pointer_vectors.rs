use crypto::pointer::{
    derive_ack_capability, derive_capabilities, derive_fetch_authority, derive_manage_capability,
    Pointer, CAPABILITY_BYTES, POINTER_BYTES,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Vector {
    p: String,
    k_msg: String,
    k_send: String,
    k_conv_n: String,
    blob_id: String,
    fetch_cap: String,
    ack_cap: String,
    manage_cap: String,
    tag_n: String,
}

#[test]
fn pointer_derivation_matches_shared_typescript_vectors() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../cipher-store-cf/test/fixtures/pointer-vectors.json"
    ))
    .expect("shared pointer vectors must be valid JSON");

    assert!(
        !vectors.is_empty(),
        "shared pointer vectors must not be empty"
    );
    for vector in vectors {
        let pointer = Pointer::from_bytes(decode_array::<POINTER_BYTES>(&vector.p));
        let actual = derive_capabilities(
            &pointer,
            &hex::decode(&vector.k_msg).expect("k_msg hex"),
            &hex::decode(&vector.k_send).expect("k_send hex"),
            &hex::decode(&vector.k_conv_n).expect("k_conv_n hex"),
        )
        .expect("pointer capability derivation");

        assert_eq!(
            actual.blob_id,
            decode_array::<CAPABILITY_BYTES>(&vector.blob_id),
            "blob_id"
        );
        assert_eq!(
            actual.fetch_cap,
            decode_array::<CAPABILITY_BYTES>(&vector.fetch_cap),
            "fetch_cap"
        );
        assert_eq!(
            actual.ack_cap,
            decode_array::<CAPABILITY_BYTES>(&vector.ack_cap),
            "ack_cap"
        );
        assert_eq!(
            actual.manage_cap,
            decode_array::<CAPABILITY_BYTES>(&vector.manage_cap),
            "manage_cap"
        );
        assert_eq!(
            actual.delivery_tag,
            decode_array::<CAPABILITY_BYTES>(&vector.tag_n),
            "delivery_tag"
        );
    }
}

/// The sender derives every capability at once; the receiver derives fetch
/// authority from `P` alone and the burner derives manage authority from a
/// recorded id alone. All three must agree against the shared vectors, or a
/// message uploaded by one side is unreachable by the other.
#[test]
fn split_derivations_agree_with_the_shared_vectors() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../cipher-store-cf/test/fixtures/pointer-vectors.json"
    ))
    .expect("shared pointer vectors must be valid JSON");

    for vector in vectors {
        let pointer = Pointer::from_bytes(decode_array::<POINTER_BYTES>(&vector.p));
        let authority = derive_fetch_authority(&pointer).expect("pointer fetch authority");
        assert_eq!(
            authority.blob_id,
            decode_array::<CAPABILITY_BYTES>(&vector.blob_id),
            "blob_id from P alone"
        );
        assert_eq!(
            authority.fetch_cap,
            decode_array::<CAPABILITY_BYTES>(&vector.fetch_cap),
            "fetch_cap from P alone"
        );
        assert_eq!(
            derive_ack_capability(
                &hex::decode(&vector.k_msg).expect("k_msg hex"),
                &authority.blob_id,
            )
            .expect("ack capability"),
            decode_array::<CAPABILITY_BYTES>(&vector.ack_cap),
            "ack_cap from the recorded id"
        );
        assert_eq!(
            derive_manage_capability(
                &hex::decode(&vector.k_send).expect("k_send hex"),
                &authority.blob_id,
            )
            .expect("manage capability"),
            decode_array::<CAPABILITY_BYTES>(&vector.manage_cap),
            "manage_cap from the recorded id"
        );
    }
}

/// The pointer authorizes reading and nothing more. A recipient holds `P`, so
/// if either write authority were recoverable from it the recipient could burn
/// or acknowledge the sender's object.
#[test]
fn fetch_authority_does_not_reveal_the_write_capabilities() {
    let pointer = Pointer::from_bytes([0x5c; POINTER_BYTES]);
    let authority = derive_fetch_authority(&pointer).expect("pointer fetch authority");
    let full = derive_capabilities(&pointer, &[0x11; 32], &[0x22; 32], &[0x33; 32])
        .expect("pointer capability derivation");

    assert_eq!(authority.blob_id, full.blob_id);
    assert_eq!(authority.fetch_cap, full.fetch_cap);
    for derived_from_pointer in [authority.blob_id, authority.fetch_cap] {
        assert_ne!(derived_from_pointer, full.ack_cap);
        assert_ne!(derived_from_pointer, full.manage_cap);
    }
    // A different send key over the same id must not reproduce the sender's
    // burn authority.
    assert_ne!(
        derive_manage_capability(&[0x23; 32], &full.blob_id).expect("manage capability"),
        full.manage_cap
    );
}

#[test]
fn pointer_rejects_non_160_bit_wire_values() {
    assert!(Pointer::try_from(&[0u8; POINTER_BYTES - 1][..]).is_err());
    assert!(Pointer::try_from(&[0u8; POINTER_BYTES + 1][..]).is_err());
}

fn decode_array<const N: usize>(value: &str) -> [u8; N] {
    let bytes = hex::decode(value).expect("fixture hex");
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("fixture must contain {N} bytes"))
}
