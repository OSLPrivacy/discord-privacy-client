use crypto::pointer::{derive_capabilities, Pointer, CAPABILITY_BYTES, POINTER_BYTES};
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
