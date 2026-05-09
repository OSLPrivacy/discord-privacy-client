use crypto::ml_kem_768;
use keystore::{
    generate_identity, load_identity, save_identity, Error, MemorySealer, NoOpSealer,
    Sealer,
};
use tempfile::TempDir;

// Test fixture: a NoOpSealer constructed once per test. Storage tests
// using NoOp confirm the on-disk layout (banner present, plaintext
// inner, etc.). Other tests use MemorySealer to confirm the sealed
// path round-trips.

#[test]
fn round_trip_to_disk_with_noop_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();

    let original = generate_identity("alice".to_string());
    save_identity(&path, &original, &sealer).unwrap();
    assert!(path.exists(), "file must be created");

    let loaded = load_identity(&path, &sealer).unwrap();
    assert_eq!(loaded.user_id, original.user_id);
    assert_eq!(
        loaded.x25519_secret.as_bytes(),
        original.x25519_secret.as_bytes(),
    );
    assert_eq!(
        loaded.x25519_public.as_bytes(),
        original.x25519_public.as_bytes(),
    );
    assert_eq!(loaded.mlkem_secret_bytes(), original.mlkem_secret_bytes());
    assert_eq!(loaded.mlkem_public_bytes, original.mlkem_public_bytes);

    // Functional check: ML-KEM keypair survives serialization.
    let ek = loaded.mlkem_encapsulation_key();
    let dk = loaded.mlkem_decapsulation_key();
    let (ct, ss_a) = ml_kem_768::encapsulate(&ek).unwrap();
    let ss_b = ml_kem_768::decapsulate(&dk, &ct).unwrap();
    assert_eq!(ss_a.as_bytes(), ss_b.as_bytes());
}

#[test]
fn round_trip_to_disk_with_memory_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = MemorySealer::new();

    let original = generate_identity("alice".to_string());
    save_identity(&path, &original, &sealer).unwrap();
    let loaded = load_identity(&path, &sealer).unwrap();
    assert_eq!(loaded.user_id, original.user_id);
    assert_eq!(
        loaded.x25519_secret.as_bytes(),
        original.x25519_secret.as_bytes(),
    );
    assert_eq!(loaded.mlkem_secret_bytes(), original.mlkem_secret_bytes());
}

#[test]
fn save_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a/b/c/identity.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("liam".to_string());
    save_identity(&nested, &id, &sealer).unwrap();
    assert!(nested.exists());
}

#[test]
fn save_overwrites_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();
    let a = generate_identity("a".to_string());
    let b = generate_identity("b".to_string());
    save_identity(&path, &a, &sealer).unwrap();
    save_identity(&path, &b, &sealer).unwrap();
    let loaded = load_identity(&path, &sealer).unwrap();
    assert_eq!(loaded.user_id, "b");
}

#[test]
fn load_rejects_version_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("c".to_string());
    save_identity(&path, &id, &sealer).unwrap();
    // Hand-edit the file to bump its version field.
    let raw = std::fs::read_to_string(&path).unwrap();
    let bumped = raw.replace("\"version\": 3", "\"version\": 999");
    std::fs::write(&path, bumped).unwrap();
    let res = load_identity(&path, &sealer);
    assert!(matches!(
        res,
        Err(Error::BlobVersionMismatch { got: 999, expected: 3 })
    ));
}

#[test]
fn load_rejects_method_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let writer = NoOpSealer::new();
    let reader = MemorySealer::new();
    let id = generate_identity("c".to_string());
    save_identity(&path, &id, &writer).unwrap();
    let res = load_identity(&path, &reader);
    assert!(
        matches!(res, Err(Error::BlobMethodMismatch { .. })),
        "method-mismatch must be a distinct error variant"
    );
}

#[test]
fn load_rejects_short_inner_field() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("d".to_string());
    save_identity(&path, &id, &sealer).unwrap();

    // With NoOpSealer the inner JSON sits in `sealed_b64` (which is
    // base64 of the inner JSON bytes). Decode it, mutate the
    // x25519_secret_b64 to a short value, re-encode, write back.
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let raw = std::fs::read_to_string(&path).unwrap();
    let on_disk: keystore::IdentityOnDisk = serde_json::from_str(&raw).unwrap();
    let inner_bytes = STANDARD.decode(&on_disk.sealed_b64).unwrap();
    let inner_str = std::str::from_utf8(&inner_bytes).unwrap().to_string();
    let mutated_inner = mutate_b64_field(
        &inner_str,
        "x25519_secret_b64",
        &STANDARD.encode(&[0u8; 16]),
    );
    let new_sealed_b64 = STANDARD.encode(mutated_inner.as_bytes());
    let mutated_outer = raw.replace(&on_disk.sealed_b64, &new_sealed_b64);
    std::fs::write(&path, mutated_outer).unwrap();

    let res = load_identity(&path, &sealer);
    assert!(matches!(
        res,
        Err(Error::BlobFieldLength { field: "x25519_secret", got: 16, expected: 32 })
    ));
}

#[test]
fn insecure_banner_present_for_noop_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("e".to_string());
    save_identity(&path, &id, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("INSECURE prototype storage"),
        "NoOpSealer must surface the INSECURE banner — it's the only \
         in-band signal that this on-disk blob is unencrypted"
    );
    assert!(
        raw.contains("\"method\": \"noop-insecure\""),
        "method tag must record the sealer used"
    );
}

#[test]
fn insecure_banner_absent_for_memory_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = MemorySealer::new();
    let id = generate_identity("e".to_string());
    save_identity(&path, &id, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        !raw.contains("INSECURE prototype storage"),
        "non-NoOp sealers must NOT emit the insecure banner"
    );
    assert!(raw.contains("\"method\": \"memory-test\""));
}

#[test]
fn sealed_blob_is_opaque_on_disk_for_memory_sealer() {
    // Confirm that with a real sealer the inner JSON is not visible
    // in the on-disk file — a leak would defeat the whole point.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = MemorySealer::new();
    let id = generate_identity("greeting-bytes-as-marker".to_string());
    save_identity(&path, &id, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        !raw.contains("greeting-bytes-as-marker"),
        "inner user_id must not appear in plaintext when sealed"
    );
}

#[test]
fn user_id_visible_on_disk_for_noop_sealer() {
    // Sanity check that the NoOp path really is plain. (And that the
    // prior "opaque" test for MemorySealer is a meaningful contrast.)
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let sealer = NoOpSealer::new();
    let id = generate_identity("noop-marker-1234".to_string());
    save_identity(&path, &id, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let on_disk: keystore::IdentityOnDisk = serde_json::from_str(&raw).unwrap();
    let inner_bytes = STANDARD.decode(&on_disk.sealed_b64).unwrap();
    let inner_str = std::str::from_utf8(&inner_bytes).unwrap();
    assert!(inner_str.contains("noop-marker-1234"));
}

fn mutate_b64_field(json: &str, field: &str, new_b64: &str) -> String {
    // Naive but sufficient for tests: find the field's value via a
    // simple substring search and replace it with new_b64.
    let needle = format!("\"{field}\":\"");
    let start = json.find(&needle).map(|s| s + needle.len()).or_else(|| {
        let alt = format!("\"{field}\": \"");
        json.find(&alt).map(|s| s + alt.len())
    });
    let start = start.expect("field present in the test JSON");
    let end = start
        + json[start..]
            .find('"')
            .expect("closing quote present");
    let mut out = String::with_capacity(json.len());
    out.push_str(&json[..start]);
    out.push_str(new_b64);
    out.push_str(&json[end..]);
    out
}

#[test]
fn _marker_traits_for_sealer() {
    fn assert_send_sync<T: Send + Sync + ?Sized>() {}
    assert_send_sync::<dyn Sealer>();
}
