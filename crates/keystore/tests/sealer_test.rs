use keystore::{select_best_sealer, MemorySealer, NoOpSealer, Sealer, METHOD_MEMORY, METHOD_NOOP};

#[test]
fn noop_round_trip() {
    let s = NoOpSealer::new();
    let pt = b"hello world";
    let ct = s.seal(pt).unwrap();
    assert_eq!(ct, pt, "NoOp must be a passthrough");
    let recovered = s.unseal(&ct).unwrap();
    assert_eq!(recovered, pt);
}

#[test]
fn noop_method_label_and_banner() {
    let s = NoOpSealer::new();
    assert_eq!(s.method_label(), METHOD_NOOP);
    assert!(!s.is_tpm_backed());
    assert!(s.requires_insecure_banner());
}

#[test]
fn memory_round_trip() {
    let s = MemorySealer::new();
    let pt = b"hello memory sealer";
    let ct = s.seal(pt).unwrap();
    assert_ne!(ct, pt, "memory sealer must not store plaintext");
    let recovered = s.unseal(&ct).unwrap();
    assert_eq!(recovered, pt);
}

#[test]
fn memory_seal_is_unique_per_call_via_random_nonce() {
    let s = MemorySealer::new();
    let pt = b"deterministic input";
    let ct_a = s.seal(pt).unwrap();
    let ct_b = s.seal(pt).unwrap();
    assert_ne!(
        ct_a, ct_b,
        "fresh nonce per seal — same plaintext must yield distinct ciphertexts"
    );
    assert_eq!(s.unseal(&ct_a).unwrap(), pt);
    assert_eq!(s.unseal(&ct_b).unwrap(), pt);
}

#[test]
fn memory_unseal_rejects_truncated_blob() {
    let s = MemorySealer::new();
    let ct = s.seal(b"x").unwrap();
    // Truncate so the nonce prefix is incomplete.
    assert!(s.unseal(&ct[..5]).is_err());
}

#[test]
fn memory_unseal_rejects_tampered_ciphertext() {
    let s = MemorySealer::new();
    let mut ct = s.seal(b"sensitive").unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 0x01;
    assert!(s.unseal(&ct).is_err());
}

#[test]
fn memory_method_label_and_no_banner() {
    let s = MemorySealer::new();
    assert_eq!(s.method_label(), METHOD_MEMORY);
    assert!(!s.requires_insecure_banner());
}

#[test]
fn cross_sealer_unseal_fails() {
    // Two distinct MemorySealer instances have independent random
    // keys; sealing under one and unsealing under the other must fail.
    let writer = MemorySealer::new();
    let reader = MemorySealer::new();
    let ct = writer.seal(b"x").unwrap();
    assert!(reader.unseal(&ct).is_err());
}

#[test]
fn empty_plaintext_round_trips() {
    let s = MemorySealer::new();
    let ct = s.seal(b"").unwrap();
    let pt = s.unseal(&ct).unwrap();
    assert_eq!(pt, b"");
}

#[test]
fn select_best_sealer_returns_some_implementation() {
    // On WSL: TPM unavailable, keyring may or may not work depending
    // on DBus. Either way, NoOpSealer is the unconditional fallback,
    // so the call must never fail.
    let s = select_best_sealer();
    let label = s.method_label();
    assert!(
        [METHOD_NOOP, "tpm-pcp", "keyring"].contains(&label),
        "unexpected method label: {label}"
    );
    // Round-trip must work whichever sealer was picked.
    let ct = s.seal(b"factory-test").unwrap();
    let pt = s.unseal(&ct).unwrap();
    assert_eq!(pt, b"factory-test");
}
