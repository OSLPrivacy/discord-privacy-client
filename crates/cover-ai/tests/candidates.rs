use std::collections::HashSet;

use cover_ai::{
    candidates::{sample_candidates, CAPABILITY_BYTES},
    scorer::RenderedCover,
};

#[test]
fn t13_tc2_candidates_are_independent_csprng_capabilities_and_canonical_renders() {
    let candidates = sample_candidates(64, |capability| {
        RenderedCover::from_canonical_text(hex(capability.as_bytes()))
    });
    assert_eq!(candidates.len(), 64);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.capability.as_bytes().len() == CAPABILITY_BYTES));
    let unique: HashSet<_> = candidates
        .iter()
        .map(|candidate| candidate.capability.as_bytes())
        .collect();
    assert_eq!(
        unique.len(),
        candidates.len(),
        "a candidate must never be derived/reused"
    );
    assert!(candidates
        .iter()
        .all(|candidate| !candidate.cover.as_str().is_empty()));
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
