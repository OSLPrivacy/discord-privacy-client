//! Proves `cover_ai::cover_history` is reachable as ordinary crate API.
//!
//! T13-C5 added `src/cover_history.rs` without a `mod` declaration in
//! `lib.rs`, so the module was never compiled into the crate at all: the only
//! thing that built it was a `#[path]` recompile in this file, which produced a
//! private second copy and made the module look both "tested" and "dead" at the
//! same time. Linking the real crate here means a `lib.rs` that drops the module
//! fails to compile this test instead of silently returning it to that state.
//!
//! Assertions that need `CoverText` live in the module's own unit tests: its
//! constructor is crate-private on purpose, which is the whole point of the
//! type, so an integration test cannot mint one and must not be given a way to.

use cover_ai::cover_history::{CoverHistory, CoverHistoryAead, CoverScope, SealedHistory};

struct TestAead;

impl CoverHistoryAead for TestAead {
    type Error = ();

    fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<SealedHistory, Self::Error> {
        // Deliberately reversible test double: production supplies AES-GCM.
        let mask = aad.iter().fold(0u8, |mask, byte| mask ^ byte);
        Ok(SealedHistory {
            nonce: vec![mask],
            ciphertext: plaintext.iter().map(|byte| byte ^ mask).collect(),
        })
    }

    fn open(&self, aad: &[u8], sealed: &SealedHistory) -> Result<Vec<u8>, Self::Error> {
        let mask = aad.iter().fold(0u8, |mask, byte| mask ^ byte);
        if sealed.nonce.as_slice() != [mask] {
            return Err(());
        }
        Ok(sealed.ciphertext.iter().map(|byte| byte ^ mask).collect())
    }
}

#[test]
fn the_store_is_public_crate_api_and_holds_nothing_until_a_render_is_recorded() {
    let mut history = CoverHistory::new(TestAead);
    let scope = CoverScope::from_hash([9u8; 32]);

    assert!(
        history.history(scope, 100).unwrap().is_empty(),
        "an unseen scope must read back empty, not fabricate context"
    );
    history.burn_scope(scope);
    history.clear();
    assert!(history.history(scope, 100).unwrap().is_empty());
}
