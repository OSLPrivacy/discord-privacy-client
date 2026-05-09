use crypto::ml_kem_768;
use keystore::{generate_identity, load_identity, save_identity, Error};
use tempfile::TempDir;

#[test]
fn round_trip_to_disk() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");

    let original = generate_identity("alice".to_string());
    save_identity(&path, &original).unwrap();
    assert!(path.exists(), "file must be created");

    let loaded = load_identity(&path).unwrap();
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
fn save_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("a/b/c/identity.json");
    let id = generate_identity("liam".to_string());
    save_identity(&nested, &id).unwrap();
    assert!(nested.exists());
}

#[test]
fn save_overwrites_existing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let a = generate_identity("a".to_string());
    let b = generate_identity("b".to_string());
    save_identity(&path, &a).unwrap();
    save_identity(&path, &b).unwrap();
    let loaded = load_identity(&path).unwrap();
    assert_eq!(loaded.user_id, "b");
}

#[test]
fn load_rejects_version_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let id = generate_identity("c".to_string());
    save_identity(&path, &id).unwrap();
    // Hand-edit the file to bump its version field.
    let raw = std::fs::read_to_string(&path).unwrap();
    let bumped = raw.replace("\"version\": 1", "\"version\": 999");
    std::fs::write(&path, bumped).unwrap();
    let res = load_identity(&path);
    assert!(matches!(
        res,
        Err(Error::BlobVersionMismatch { got: 999, expected: 1 })
    ));
}

#[test]
fn load_rejects_short_field() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let id = generate_identity("d".to_string());
    save_identity(&path, &id).unwrap();
    // Replace the x25519_secret_b64 with a short byte string.
    let raw = std::fs::read_to_string(&path).unwrap();
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let short = STANDARD.encode(&[0u8; 16]);
    let mutated = mutate_b64_field(&raw, "x25519_secret_b64", &short);
    std::fs::write(&path, mutated).unwrap();
    let res = load_identity(&path);
    assert!(matches!(
        res,
        Err(Error::BlobFieldLength { field: "x25519_secret", got: 16, expected: 32 })
    ));
}

#[test]
fn insecure_banner_present_on_disk() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let id = generate_identity("e".to_string());
    save_identity(&path, &id).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains("INSECURE prototype storage"),
        "the on-disk file must carry an INSECURE banner field — it's the only \
         in-band signal a future caller has that this format is not stable"
    );
}

fn mutate_b64_field(json: &str, field: &str, new_b64: &str) -> String {
    // Naive but sufficient for tests: find the field's value via a
    // simple substring search and replace it with new_b64.
    let needle = format!("\"{field}\": \"");
    let start = json
        .find(&needle)
        .expect("field present in the test JSON")
        + needle.len();
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
