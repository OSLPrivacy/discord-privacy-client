//! A8 / audit-medium "Keystore load/save leaves long-term secrets in
//! ordinary heap allocations".
//!
//! These tests pin the *observable* half of that finding. Zeroization is
//! inherently hard to observe from safe Rust — a `Vec`'s buffer is freed
//! before we could legally read it — so the suite splits into two kinds:
//!
//! 1. **Runtime drop tests.** The value lives in a `ManuallyDrop` on the
//!    test's own stack frame, so its storage is still valid (merely
//!    logically dead) after `ManuallyDrop::drop`. Reading it back as
//!    `[u8; N]` is sound: every bit pattern is a valid `u8`, and the
//!    allocation has not been released. This is the only way to prove a
//!    `Drop` impl actually ran the wipe.
//! 2. **Type-level tests.** For buffers whose storage *is* released on
//!    drop (heap `Vec`/`String`), reading after free would be undefined
//!    behaviour, so instead we assert the static type is a zeroizing
//!    wrapper. These fail by refusing to compile when a secret regresses
//!    to a plain `Vec<u8>`/`String`, which is the regression we care
//!    about.

use keystore::identity::identity_from_entropy;
use keystore::sealer::{MemorySealer, Sealer};
use std::mem::ManuallyDrop;
use zeroize::Zeroizing;

/// The 16 bytes of recovery entropy rederive the entire identity, so
/// leaving them in allocator-reusable memory is lasting key compromise
/// rather than a scratch buffer. Baseline: `Identity` has no `Drop`, so
/// the bytes survive verbatim and this assertion fails.
#[test]
fn recovery_entropy_is_zeroized_when_identity_drops() {
    let entropy = [0xA7u8; 16];
    let mut identity = ManuallyDrop::new(identity_from_entropy(entropy, "u".to_owned()));

    // Address of the entropy *inside* the identity struct. The struct is
    // stack-resident in this frame, so this address stays valid after the
    // logical drop below.
    let addr = identity
        .recovery_entropy
        .as_ref()
        .expect("identity_from_entropy always records its entropy")
        .as_ptr();

    // Sanity: the bytes really are there before the drop. Without this the
    // test could pass against an identity that never stored them.
    let before = unsafe { std::ptr::read_volatile(addr as *const [u8; 16]) };
    assert_eq!(before, entropy, "precondition: entropy is held in the clear");

    unsafe { ManuallyDrop::drop(&mut identity) };

    let after = unsafe { std::ptr::read_volatile(addr as *const [u8; 16]) };
    assert_ne!(
        after, entropy,
        "recovery entropy survived Identity::drop — the 16 bytes that \
         rederive the whole identity are still in reusable memory"
    );
    assert_eq!(
        after, [0u8; 16],
        "recovery entropy was disturbed but not wiped to zero"
    );
}

/// Every secret-bearing field must be wiped, not just the one the finding
/// names. The X25519/Ed25519/ML-KEM halves already use zeroizing wrappers;
/// this pins that the recovery entropy joins them and that a *clone*
/// (which `Identity` derives, so the hub can snapshot it out from under a
/// mutex) wipes its own copy too.
#[test]
fn cloned_identity_also_zeroizes_its_recovery_entropy() {
    let entropy = [0x5Cu8; 16];
    let original = identity_from_entropy(entropy, "u".to_owned());

    let mut snapshot = ManuallyDrop::new(original.clone());
    let addr = snapshot
        .recovery_entropy
        .as_ref()
        .expect("clone preserves the entropy")
        .as_ptr();
    assert_eq!(
        unsafe { std::ptr::read_volatile(addr as *const [u8; 16]) },
        entropy
    );

    unsafe { ManuallyDrop::drop(&mut snapshot) };

    assert_eq!(
        unsafe { std::ptr::read_volatile(addr as *const [u8; 16]) },
        [0u8; 16],
        "a dropped Identity clone left the recovery entropy behind"
    );
    // The original is untouched by the clone's drop.
    assert_eq!(original.recovery_entropy, Some(entropy));
}

/// Type-level: the common unsealer hands its caller a sensitive buffer, so
/// the plaintext it just produced is wiped when the caller drops it. The
/// baseline returns a plain `Vec<u8>` and this does not compile.
#[test]
fn unseal_returns_a_zeroizing_buffer() {
    let sealer = MemorySealer::new();
    let sealed = sealer.seal(b"long-term secret material").unwrap();

    let plaintext: Zeroizing<Vec<u8>> = sealer.unseal(&sealed).unwrap();
    assert_eq!(&*plaintext, b"long-term secret material");
}

/// The same guarantee through the trait object the shipping call sites
/// actually hold (`&dyn Sealer`), so the wrapper cannot be an artefact of
/// one concrete impl.
#[test]
fn unseal_through_trait_object_is_also_zeroizing() {
    let sealer = MemorySealer::new();
    let dynamic: &dyn Sealer = &sealer;
    let sealed = dynamic.seal(b"identity blob").unwrap();

    let plaintext: Zeroizing<Vec<u8>> = dynamic.unseal(&sealed).unwrap();
    assert_eq!(&*plaintext, b"identity blob");
}
