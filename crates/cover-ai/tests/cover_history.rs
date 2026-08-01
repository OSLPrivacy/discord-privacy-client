//! Regression tests for the cover-only, AEAD-backed context store.

#[path = "../src/cover_history.rs"]
mod cover_history;

use cover_history::{
    CoverHistory, CoverHistoryAead, CoverScope, CoverText, SealedHistory, MAX_SCOPES, MAX_TURNS,
};

#[derive(Default)]
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

fn scope(id: u8) -> CoverScope {
    CoverScope::from_hash([id; 32])
}

fn cover(text: &str) -> CoverText {
    CoverText::from_verified_render(text.to_owned()).unwrap()
}

#[test]
fn only_verified_cover_values_can_be_recorded_and_they_are_aead_sealed() {
    let scope = scope(7);
    let mut history = CoverHistory::new(TestAead);
    history
        .record(scope, cover("the train was late again"), 60, 100)
        .unwrap();

    // `record` takes CoverText, not String or &str: plaintext has no route to
    // this boundary. The private CoverText constructor is only exposed to the
    // carrier-verification adapter in this crate.
    assert_eq!(
        history
            .history(scope, 101)
            .unwrap()
            .iter()
            .map(CoverText::as_str)
            .collect::<Vec<_>>(),
        ["the train was late again"]
    );
    assert!(
        !history
            .sealed_for_test(scope)
            .unwrap()
            .ciphertext
            .windows(24)
            .any(|window| window == b"the train was late again"),
        "stored context must be ciphertext, not readable cover text"
    );
}

#[test]
fn transcript_is_bounded_expiring_and_scope_keyed() {
    let mut history = CoverHistory::new(TestAead);
    let selected = scope(3);
    for turn in 0..(MAX_TURNS + 5) {
        history
            .record(selected, cover(&format!("neutral cover {turn}")), 10, 100)
            .unwrap();
    }
    assert_eq!(history.history(selected, 109).unwrap().len(), MAX_TURNS);
    assert!(history.history(selected, 110).unwrap().is_empty());

    for id in 0..(MAX_SCOPES + 4) {
        history
            .record(
                scope(id as u8),
                cover("a visible cover"),
                60,
                200 + id as u64,
            )
            .unwrap();
    }
    assert!(history.history(scope(0), 201).unwrap().is_empty());
    assert_eq!(
        history
            .history(scope((MAX_SCOPES + 3) as u8), 201)
            .unwrap()
            .len(),
        1
    );
}
