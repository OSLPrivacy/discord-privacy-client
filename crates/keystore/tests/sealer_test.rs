use keystore::{
    select_best_sealer, verify_sealer_round_trip, MemorySealer, NoOpSealer, Sealer, SealerError,
    Zeroizing, METHOD_EPHEMERAL, METHOD_MEMORY, METHOD_NOOP,
};

// `KeyringSealer::new()` writes a key to one machine-global Windows
// Credential Manager entry and then re-reads it through a fresh `Entry` to
// prove the backend actually persists. Two tests doing that at the same
// time overwrite each other's key, so the probe reads the other test's
// bytes and reports "keyring backend not persistent" -- which is how CI
// failed. It never reproduced on the Linux dev host because there the
// keyring resolves to a backend this probe rejects anyway, so nothing
// contends. Serialize every test that reaches the real credential store.
//
// 2026-07-31: the create/probe race itself is fixed inside
// `KeyringSealer::new_namespaced` (see `KEYRING_ENTRY_LOCK` in
// `src/sealer.rs`), and the Linux claim above is now stale -- `linux-native`
// keyutils is a real persistent backend, so this box does contend. This lock
// stays because the tests below share the production entry's VALUE, which is
// a separate concern from the atomicity of creating it:
// `concurrent_keyring_construction_converges_on_one_key` is the test that
// covers the race, and it uses its own namespace instead of this lock.
static CREDENTIAL_STORE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct SealFailure;

impl Sealer for SealFailure {
    fn method_label(&self) -> &'static str {
        "test-seal-failure"
    }
    fn is_tpm_backed(&self) -> bool {
        false
    }
    fn requires_insecure_banner(&self) -> bool {
        false
    }
    fn seal(&self, _plaintext: &[u8]) -> keystore::sealer::Result<Vec<u8>> {
        Err(SealerError::Tpm("fixed test failure".into()))
    }
    fn unseal(&self, _ciphertext: &[u8]) -> keystore::sealer::Result<Zeroizing<Vec<u8>>> {
        unreachable!("unseal must not run after seal fails")
    }
}

struct WrongRoundTrip;

impl Sealer for WrongRoundTrip {
    fn method_label(&self) -> &'static str {
        "test-wrong-round-trip"
    }
    fn is_tpm_backed(&self) -> bool {
        false
    }
    fn requires_insecure_banner(&self) -> bool {
        false
    }
    fn seal(&self, plaintext: &[u8]) -> keystore::sealer::Result<Vec<u8>> {
        Ok(plaintext.to_vec())
    }
    fn unseal(&self, _ciphertext: &[u8]) -> keystore::sealer::Result<Zeroizing<Vec<u8>>> {
        Ok(Zeroizing::new(b"different public test bytes".to_vec()))
    }
}

#[test]
fn noop_round_trip() {
    let s = NoOpSealer::new();
    let pt = b"hello world";
    let ct = s.seal(pt).unwrap();
    assert_eq!(ct, pt, "NoOp must be a passthrough");
    let recovered = s.unseal(&ct).unwrap();
    assert_eq!(&recovered[..], pt);
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
    assert_eq!(&recovered[..], pt);
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
    assert_eq!(&s.unseal(&ct_a).unwrap()[..], pt);
    assert_eq!(&s.unseal(&ct_b).unwrap()[..], pt);
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
    assert_eq!(&pt[..], b"");
}

#[test]
fn readiness_probe_accepts_complete_round_trip_only() {
    assert!(verify_sealer_round_trip(&MemorySealer::new()).is_ok());
    assert!(verify_sealer_round_trip(&SealFailure).is_err());
    assert!(verify_sealer_round_trip(&WrongRoundTrip).is_err());
}

#[test]
fn select_best_sealer_returns_some_implementation() {
    let _credential_store = CREDENTIAL_STORE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // On WSL: TPM unavailable, keyring may or may not work depending
    // on DBus. The fallback must remain encrypted in process memory;
    // it must never silently downgrade to NoOp/plaintext.
    let s = select_best_sealer();
    let label = s.method_label();
    assert!(
        [METHOD_EPHEMERAL, "tpm-pcp", "keyring"].contains(&label),
        "unexpected method label: {label}"
    );
    assert_ne!(label, METHOD_NOOP, "factory must not select plaintext NoOp");
    // Round-trip must work whichever sealer was picked.
    let ct = s.seal(b"factory-test").unwrap();
    let pt = s.unseal(&ct).unwrap();
    assert_eq!(&pt[..], b"factory-test");
}

#[cfg(windows)]
#[test]
fn windows_credential_manager_survives_fresh_entry() {
    let _credential_store = CREDENTIAL_STORE_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let writer = keystore::KeyringSealer::new().expect("Windows Credential Manager available");
    let ciphertext = writer.seal(b"fixed public persistence probe").unwrap();
    let reader = keystore::KeyringSealer::new().expect("fresh credential entry can read key");
    // `unseal` returns Zeroizing<Vec<u8>>, so slice it before comparing to a byte
    // literal -- the same shape the non-Windows test above uses. This is
    // #[cfg(windows)] and never compiled on the Linux dev host, so the mismatch
    // sat here uncaught: the test has never actually run.
    let plaintext = reader.unseal(&ciphertext).unwrap();
    assert_eq!(&plaintext[..], b"fixed public persistence probe");
}

/// Starve test for the credential-store create race that `KeyringSealer::new`
/// used to carry (the "KNOWN RACE (deliberately not fixed here, 2026-07-31)"
/// note in `src/sealer.rs`).
///
/// `get_password` -> `set_password` -> read-back is a read-modify-write on one
/// shared credential entry. Before the fix, N threads that all found the entry
/// absent each generated and wrote their own key, and every loser was left
/// holding a key the store no longer contained: anything it had already sealed
/// failed to unseal with `AEAD operation failed`, which is how
/// `ipc::wire_rn::b42_rn_session_store_uses_select_best_sealer_for_at_rest_sessions`
/// failed under `cargo test --workspace`.
///
/// The barrier is what makes this deterministic rather than a lottery: it
/// forces every thread into the create path in the same instant, which is the
/// interleaving `--workspace` only stumbles into occasionally. Removing the
/// lock from `new_namespaced` must make this fail; that is the only evidence
/// the lock is load-bearing.
///
/// It runs against its OWN namespace, never the production entry, so it cannot
/// purge or rotate the key that a concurrently-running test -- or the
/// developer's installed client -- depends on.
#[test]
fn concurrent_keyring_construction_converges_on_one_key() {
    use keystore::KeyringSealer;
    use std::sync::{Arc, Barrier};

    const NAMESPACE: &str = "test.sealer.concurrent-construction";
    const THREADS: usize = 8;
    const ROUNDS: usize = 40;

    // This asserts nothing on a host with no real credential store (no
    // Windows Credential Manager, no keyutils, no Keychain): there the
    // backend is the keyring crate's in-memory mock, whose every `Entry`
    // is independent, so construction fails by design and there is no
    // shared entry to race over. Say so on stderr rather than passing
    // quietly, because a silent pass here would look like proof.
    if let Err(e) = KeyringSealer::new_namespaced(NAMESPACE) {
        eprintln!(
            "SKIPPED concurrent_keyring_construction_converges_on_one_key: \
             no persistent credential store on this host ({e:?})"
        );
        return;
    }

    for round in 0..ROUNDS {
        KeyringSealer::purge_keyring_entry_namespaced(NAMESPACE).expect("purge namespace");
        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|thread| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || -> std::result::Result<(), String> {
                    barrier.wait();
                    let writer = KeyringSealer::new_namespaced(NAMESPACE)
                        .map_err(|e| format!("round {round} thread {thread}: create: {e:?}"))?;
                    let sealed = writer
                        .seal(b"fixed public concurrency probe")
                        .map_err(|e| format!("round {round} thread {thread}: seal: {e:?}"))?;
                    // A fresh construction is what production does: `save` and
                    // `load` each call `select_best_sealer()` separately.
                    let reader = KeyringSealer::new_namespaced(NAMESPACE)
                        .map_err(|e| format!("round {round} thread {thread}: reopen: {e:?}"))?;
                    let opened = reader
                        .unseal(&sealed)
                        .map_err(|e| format!("round {round} thread {thread}: unseal: {e:?}"))?;
                    if &opened[..] != b"fixed public concurrency probe" {
                        return Err(format!("round {round} thread {thread}: wrong plaintext"));
                    }
                    Ok(())
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("thread panicked").expect(
                "every concurrent construction must converge on the one stored key: a \
                 create/probe interleaving left a sealer holding a key the store no longer has",
            );
        }
    }

    KeyringSealer::purge_keyring_entry_namespaced(NAMESPACE).expect("purge namespace");
}
